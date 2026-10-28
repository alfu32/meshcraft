param(
  [Parameter(Mandatory=$true)][ValidateSet('x64','arm64')][string]$Arch,
  [Parameter(Mandatory=$true)][string]$Version
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Identity = Get-Content (Join-Path $PSScriptRoot 'store-identity.json') -Raw | ConvertFrom-Json
$Assets = Join-Path $Root 'target\release-assets'
$Public = Join-Path $Assets "meshcraft-$Version-windows-$Arch.cer"
$Msix = Join-Path $Assets "meshcraft-$Version-windows-$Arch.msix"
$Certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($Public)
$Store = [Security.Cryptography.X509Certificates.X509Store]::new('TrustedPeople','LocalMachine')
$Installed = $null
$AddedCertificate = $false
try {
  if ($Certificate.Subject -cne $Identity.publisher) { throw 'Public certificate publisher mismatch' }
  $Store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
  if ($Store.Certificates.Find([Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,$Certificate.Thumbprint,$false).Count -eq 0) {
    $Store.Add($Certificate)
    $AddedCertificate = $true
  }
  if (Get-AppxPackage -Name $Identity.name) { throw 'Unexpected pre-existing Store package on CI runner' }
  Add-AppxPackage -Path $Msix -ErrorAction Stop
  $Installed = Get-AppxPackage -Name $Identity.name
  $Metadata = Get-Content "$Msix.manifest.json" -Raw | ConvertFrom-Json
  if (-not $Installed -or $Installed.PackageFamilyName -cne $Identity.packageFamilyName -or
      $Installed.Publisher -cne $Identity.publisher -or $Installed.Version.ToString() -ne $Metadata.packageVersion -or
      $Installed.Architecture.ToString() -ine $Arch) { throw 'Installed MSIX identity/version/architecture mismatch' }
  $Executable = Join-Path $Installed.InstallLocation 'meshcraft.exe'
  python -c 'import subprocess,sys; actual=subprocess.check_output([sys.argv[1],"--version"],text=True).strip(); assert actual=="Meshcraft "+sys.argv[2],actual; print(actual)' $Executable $Version
  if ($LASTEXITCODE -ne 0) { throw 'Installed MSIX command launch failed' }
} finally {
  try {
    if ($Installed) { Remove-AppxPackage -Package $Installed.PackageFullName -ErrorAction Stop }
  } finally {
    if ($AddedCertificate) { $Store.Remove($Certificate) }
    $Store.Close()
    $Certificate.Dispose()
  }
}
