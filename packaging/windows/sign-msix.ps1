param(
  [Parameter(Mandatory=$true)][string]$Msix,
  [Parameter(Mandatory=$true)][ValidateSet('x64','arm64')][string]$Arch,
  [Parameter(Mandatory=$true)][string]$Version,
  [Parameter(Mandatory=$true)][string]$SourceRevision,
  [Parameter(Mandatory=$true)][string]$PublicCertificate
)
$ErrorActionPreference = 'Stop'
$Identity = Get-Content (Join-Path $PSScriptRoot 'store-identity.json') -Raw | ConvertFrom-Json
$TemporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$Pfx = Join-Path $TemporaryRoot ("meshcraft-sign-" + [guid]::NewGuid().ToString('N') + '.pfx')
$Certificate = $null
try {
  if ([string]::IsNullOrWhiteSpace($env:MESHCRAFT_PFX_B64) -or [string]::IsNullOrWhiteSpace($env:MESHCRAFT_PFX_PASSWORD)) {
    throw 'MSIX signing secrets are missing'
  }
  [IO.File]::WriteAllBytes($Pfx, [Convert]::FromBase64String($env:MESHCRAFT_PFX_B64))
  $Certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $Pfx, $env:MESHCRAFT_PFX_PASSWORD, [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet)
  if (-not $Certificate.HasPrivateKey -or $Certificate.Subject -cne $Identity.publisher) {
    throw 'Signing certificate/private key does not match the reserved Store publisher'
  }
  if ($Certificate.NotBefore.ToUniversalTime() -gt [DateTime]::UtcNow -or $Certificate.NotAfter.ToUniversalTime() -le [DateTime]::UtcNow) {
    throw 'MSIX signing certificate is not currently valid'
  }
  $Eku = $Certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' }
  if (-not $Eku -or '1.3.6.1.5.5.7.3.3' -notin @($Eku.EnhancedKeyUsages | ForEach-Object { $_.Value })) {
    throw 'MSIX signing certificate must permit code signing'
  }
  $SignTool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $SignTool) { throw 'Windows SDK signtool.exe not found' }
  & $SignTool.FullName sign /fd SHA256 /a /f $Pfx /p $env:MESHCRAFT_PFX_PASSWORD /tr http://timestamp.digicert.com /td SHA256 $Msix
  if ($LASTEXITCODE -ne 0) { throw 'MSIX signing/timestamping failed' }

  # Verify the embedded CMS signature without changing the machine trust store.
  # Native Add-AppxPackage checks package integrity and deployment trust in CI.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  Add-Type -AssemblyName System.Security.Cryptography.Pkcs
  $Archive = [IO.Compression.ZipFile]::OpenRead($Msix)
  try {
    $Entry = $Archive.GetEntry('AppxSignature.p7x')
    if (-not $Entry) { throw 'Signed MSIX has no AppxSignature.p7x' }
    $Stream = $Entry.Open()
    $Buffer = [IO.MemoryStream]::new()
    try { $Stream.CopyTo($Buffer); $Signature = $Buffer.ToArray() }
    finally { $Stream.Dispose(); $Buffer.Dispose() }
  } finally { $Archive.Dispose() }
  if ([Text.Encoding]::ASCII.GetString($Signature,0,4) -cne 'PKCX') { throw 'Invalid MSIX signature format' }
  $Cms = [Security.Cryptography.Pkcs.SignedCms]::new()
  $Cms.Decode([byte[]]$Signature[4..($Signature.Length-1)])
  $Cms.CheckSignature($true)
  if ($Cms.SignerInfos.Count -ne 1 -or $Cms.SignerInfos[0].Certificate.Thumbprint -cne $Certificate.Thumbprint) {
    throw 'MSIX embedded signature does not match the configured signing certificate'
  }
  [IO.File]::WriteAllBytes($PublicCertificate, $Certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
  $CertificateHash = (Get-FileHash $PublicCertificate -Algorithm SHA256).Hash.ToLowerInvariant()
  $MsixVersion = "$(([int]$Version.Split('.')[0]) + $Identity.versionMajorOffset).$($Version.Split('.')[1]).$($Version.Split('.')[2]).0"
  $Metadata = @{
    schemaVersion=1; format='msix'; version=$Version; packageVersion=$MsixVersion; architecture=$Arch
    sourceRevision=$SourceRevision; artifact=(Split-Path $Msix -Leaf); signed=$true
    identityName=$Identity.name; publisher=$Identity.publisher; publisherDisplayName=$Identity.publisherDisplayName
    packageFamilyName=$Identity.packageFamilyName; storeId=$Identity.storeId
    certificateSha256=$CertificateHash; certificateThumbprint=$Certificate.Thumbprint
    certificateType=$(if ($Certificate.Subject -ceq $Certificate.Issuer) { 'self-signed' } else { 'issued' })
    sha256=(Get-FileHash $Msix -Algorithm SHA256).Hash.ToLowerInvariant(); size=(Get-Item $Msix).Length
  }
  $Metadata | ConvertTo-Json | Set-Content "$Msix.manifest.json" -Encoding utf8
} finally {
  if ($Certificate) { $Certificate.Dispose() }
  if (Test-Path $Pfx) { Remove-Item $Pfx -Force }
}
