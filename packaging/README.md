# Manual build and release

The generated application icon's transparent master and provenance are in
[assets/branding](../assets/branding/README.md). The native title bar embeds its
64-pixel image; release packaging uses `packaging/meshcraft.png` for portable
archives, AppImage, Snap, macOS ICNS and MSIX logos. Windows also links the
multi-size `packaging/windows/meshcraft.ico` into the executable and MSI shortcuts.
Regenerate checked-in derivatives with `python3 scripts/prepare_app_icon.py`
(ImageMagick needed only for regeneration).

Meshcraft uses the same six-target distribution matrix as the Rust Kode reference
(`alfu32/kodecraft`, commit `f3f915c5a9c14f445a3f4e83e520ce2970831636`). There is no JDBC/Java payload.

| Native target | Portable archives | Additional installers |
| --- | --- | --- |
| Linux x86_64 / aarch64 | ZIP, tar.gz | AppImage, Snap, DEB, RPM |
| Windows x64 / ARM64 | ZIP, tar.gz | MSI, signed MSIX, public signing CER |
| macOS Intel / Apple Silicon | ZIP, tar.gz | DMG containing Meshcraft.app |

Each release contains 28 primary artifacts, per-file SHA-256 sidecars and a combined
`SHA256SUMS.txt`. Portable archives include a file manifest, source revision,
version, notices and the small dynamic-object example. UI icons/shaders stay embedded
in the executable. User scenes, configuration, build directories and test projects
are never collected by directory-wide source copying.

## Release a tag

1. Set the **literal** `version = "X.Y.Z"` in `apps/desktop/Cargo.toml` before committing;
   update `Cargo.lock`, commit, then tag that exact commit `X.Y.Z` and push both.
   Follow SemVer; the user owns major-version increments.
2. In GitHub Actions, choose **Release → Run workflow**, then select that existing
   tag reference. No tag/source parameter is required. With the CLI:
   `gh workflow run distribution.yaml --ref X.Y.Z`.
3. The workflow rejects branches, a `v` prefix, a different app version or a tag
   pointing at another commit. Every job checks out the resolved SHA. The pinned
   toolchain comes from `rust-toolchain.toml`; builds use `--locked`.
4. Validation and all six native builds/installers run in parallel after tag validation. Publication
   requires the complete artifact matrix, every checksum and matching source/file
   manifests. A failed platform prevents publication; rerunning does not silently
   replace an existing release. The workflow never creates or moves a tag.

Only the final publication job has `contents: write`. Other jobs read source and
upload build artifacts. Actions and AppImage inputs are pinned to commits/digests.

## Build time and caches

Validation and native builds start together; publication still waits for all of them.
Validation disables debug info and dependency optimization, while the shipped binaries
retain the normal optimized release profile. Four compile workers replace the former
two-worker limit. Cargo registry and compiled dependencies are cached separately by
platform/architecture, toolchain/native build configuration and lockfile; compatible
older lockfile caches can seed a build within the cache scope. GitHub scopes caches
to the selected ref (and its default-branch fallback), so a different version tag
starts cold unless a compatible default-branch cache has been seeded. Cache saves run before installer steps
and on failed compilation so a packaging failure does not throw away the build work.
A cold native build still compiles GPUI/raylib and shared egui session/test types;
the unused native eframe/winit/wgpu renderer stack has been removed. Eframe and its
renderer remain dependencies of the browser target only.
Warm-run improvements depend on the GitHub cache being available.

Linux containers run as the runner UID/GID with pre-created writable mount points.
A dedicated Linux Cargo home contains no host rustup proxy executables.
This keeps both the Cargo cache and host packaging output writable without root-owned
`target` directories or recursive permission repairs.

## Linux compatibility

Hosted runners remain supported `ubuntu-24.04`/`ubuntu-24.04-arm`; **native compilation
and dependency collection happen in Ubuntu 18.04 containers**, not the runner sysroot.
The Rust toolchain is mounted read-only. CMake 3.31.6 is installed separately because
raylib 6 requires CMake >=3.25; this does not upgrade the build glibc.

`native_linux.py` audits the executable and bundled ELF dependency closure with
`readelf` and fails above **GLIBC_2.27**. Its report records the actual maximum,
architecture and external libraries. Non-driver native libraries (including
fontconfig/FreeType and XKB libraries loaded dynamically) plus a small DejaVu font set/config receive relative RPATHs
and license notices. glibc, the loader, compiler runtimes and GPU/GL/Vulkan drivers
come from the OS. The launcher prefers available desktop/graphics-provider libraries and uses the bundle for missing dependencies. XKB's core and X11 bridge share private structures, so they must be selected as a pair: coherent host pairs are preferred, while an incomplete or split host runtime selects the matched pair in `lib/xkb`. Its compatible bundled locale data is selected through XLOCALEDIR, preserving explicit/per-user Compose preferences. A working X11 desktop, fonts and a GPUI-compatible Vulkan driver
are still runtime requirements; the glibc ceiling alone is not full desktop support.

Virtual X11 displays without DRI3 presentation support select the installed Mesa
software Vulkan driver before native graphics initialization. Other failed Linux graphics initialization gets one fresh-process retry with an installed
Mesa Lavapipe driver, preserving normal hardware startup and explicit Vulkan driver
choices. If graphics remain unavailable, an X11 error window explains the driver
requirement; the application exits unsuccessfully after acknowledgement. This does
not require network access or a portal. Runtime/editor panics are not intercepted.
Release builds test the actual AppImage under Firejail `--net=none` in private Xvfb,
verify that the real application window paints, and test the missing-driver dialog.
Startup logs and screenshots are retained as separate workflow artifacts.
Version preflights distinguish the AppImage runtime, sandbox and GUI startup;
failed checks also preserve the actual packages for reproduction.
The private Xvfb server uses `-noreset`, keeping driver probes and short-lived
window pollers from resetting the display during startup.
For native startup crashes, `appimage-diagnose.yaml` consumes the failed-package
artifacts from a selected Release run and captures direct/sandboxed startup and
GDB backtraces on both Linux architectures. It does not build or publish an app,
and its diagnostic outcomes do not constitute release qualification.

The AppImage Hub's `references glibc 2.27` line describes the minimum host C library,
not a bundled copy or proof of a startup error. The Hub's Ubuntu 22.04 test host
uses glibc 2.35. Its reported early exit was reproduced in a Hub-style offline
Xvfb test as a Vulkan presentation failure without DRI3, independently of network
access. [AppImage's dependency guidance](https://docs.appimage.org/introduction/concepts.html)
allows relying on system C/graphics libraries.

AppImage uses checksum-pinned appimagetool/type2 runtime from the reference release
pipeline and works without FUSE via `APPIMAGE_EXTRACT_AND_RUN=1`.
The Snap uses `core24` and Canonical's `mesa-2404` graphics content provider so the
GPUI/Vulkan and raylib/OpenGL APIs are available inside confinement. This isolated
Snap runtime is separate from the old-glibc portable/AppImage build baseline.
Snap packages use `grade: stable`, allowing the uploaded revision to be released
to edge, beta, candidate or stable. The grade determines channel eligibility;
the publisher chooses the release channel separately. A `devel`-grade revision
must be rebuilt and uploaded as a new revision before candidate/stable promotion.
The Snap requests `wayland` and `x11` so it can use either desktop compositor.
Snap's private runtime directory may differ from the directory containing the
Wayland socket, so startup resolves that socket and passes its absolute path to
GPUI. If it is unavailable, Meshcraft restarts through X11. The Linux payload
bundles XKB keyboard configuration for confined X11 sessions. For local unsigned
Snap installation, install the base/provider and connect the `gpu-2404`,
`wayland`, `x11` and `opengl` plugs as required by snapd. Store publication is separate.

The Ubuntu 18.04 dependency and diagnostic link/ELF checks passed with the pinned Rust toolchain. The
container uses Python 3.8 and pip 25.0.1 for the newer CMake tool. Its multi-architecture
base image is pinned by digest. The actual Linux AppImage also passed a GUI smoke check; the exact executed
probes and remaining platform limits are recorded in the [release handoff](../docs/rewrite/handoffs/T44.md).

## Native DEB and RPM packages

Each Linux matrix job packages the same audited old-glibc portable payload into:

| Target | DEB | RPM |
| --- | --- | --- |
| x86_64 | `meshcraft_X.Y.Z-1_amd64.deb` | `meshcraft-X.Y.Z-1.x86_64.rpm` |
| ARM64 | `meshcraft_X.Y.Z-1_arm64.deb` | `meshcraft-X.Y.Z-1.aarch64.rpm` |

The package revision is `1`, separate from the app's SemVer. The application and
its audited private dependencies install under `/opt/meshcraft`; `/usr/bin/meshcraft`
starts the shared Linux launcher. Desktop, icon, AppStream and `.octd` MIME entries
are installed under `/usr/share`. User drawings and preferences are never installed
or removed. The package manager resolves the host C/compiler runtime and GL/EGL,
OpenGL and Vulkan loader libraries. The host provides a compatible graphics driver.
These packages run directly on the host without a Snap runtime or GPU content provider.

Build locally from either architecture's portable archive (Python 3, `dpkg-deb`,
`rpmbuild`, `rpm` and `rpm2cpio` are required):

```sh
python3 scripts/release/linux_packages.py \
  --archive target/release-assets/meshcraft-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz \
  --output target/release-assets
# Use the aarch64-unknown-linux-gnu archive for ARM64 packages.
# --format deb or --format rpm builds just that format; the default builds both.
```

Packaging does not recompile or execute the target binary; it can package either
architecture on the same workstation. CI uses each architecture's native runner.
Each build extracts and verifies its package's architecture, version, source revision,
root ownership, launcher, desktop entries and every payload digest/mode. Final release
verification also checks the installed manifest against the portable archive and
requires a provenance sidecar. RPM automatic library provides/requires and stripping
are disabled for this private bundle; explicit requirements describe unbundled libraries.
This prevents the bundled libraries from satisfying unrelated system package dependencies
and preserves the audited binary hashes.

CI installs, runs `meshcraft --version` and removes the DEB on Ubuntu 24.04 and the
RPM in a digest-pinned Rocky Linux 8 container on both native architectures. RPM
installation also verifies the installed file digests with `rpm -V`. These are
installation/CLI checks; they do not qualify every distro's graphics stack.

Install a matching download through the system package manager:

```sh
sudo apt install ./meshcraft_X.Y.Z-1_amd64.deb
sudo dnf install ./meshcraft-X.Y.Z-1.x86_64.rpm
```

Both formats are unsigned release downloads with SHA-256 checksums. No repository
is added automatically. APT/RPM repository hosting, signing and automated updates
are a separate next stage; see [Open Build Service](OBS.md) for the proposed workflow.

## Windows and macOS

Windows builds use the native SDK, LLVM/libclang for raylib bindings, GPUI's `fxc`
shader compiler, and static CRT linking. The CMake toolchain explicitly sets the
MSVC runtime to `MultiThreaded` for raylib/GLFW, matching Rust `+crt-static`;
setting only Rust flags leaves CMake targets using the DLL runtime. WiX 5.0.2 produces MSI, and the Windows SDK
MakeAppx produces MSIX packages using the reserved Store identity in
[`windows/store-identity.json`](windows/store-identity.json), transcribed from the
publisher's local `.store/microsoft` details. Identity name is
`K3D0.Meshcraft-desktop`, publisher is `CN=7F0CBC95-8035-4AF2-9B17-C38D58351CE5`,
and publisher display name is `K3D0`. PFN, package SID and Store ID are recorded
for validation/reference; they are not extra AppxManifest elements. The desktop
entry uses the Store name `Meshcraft-desktop`, registers `.octd` files and the
`meshcraft.exe` execution alias.

Store packages need a positive major version and a zero fourth component. MSIX
versions map app `major.minor.patch` to `(major + 1).minor.patch.0`: app `0.41.1`
is MSIX `1.41.1.0`, and future app `1.0.0` is MSIX `2.0.0.0`. This keeps package
updates monotonic while the user controls the application's major version.

CI requires repository secrets `MESHCRAFT_PFX_B64` and `MESHCRAFT_PFX_PASSWORD`.
`sign-msix.ps1` checks the PFX's private key, validity, code-signing EKU and exact
publisher before SHA-256 signing and RFC3161 timestamping. It checks the embedded
CMS signature and removes the temporary PFX in `finally`. Missing or incompatible
secrets fail the build. MSI remains unsigned.

The generated self-signed code-signing certificate is backed up under ignored
`.store/microsoft-signing/`: encrypted PEM key, password-protected PFX, password,
public DER/PEM certificates and public metadata. The directory is mode 0700 and
its files are mode 0600. Generate or safely reuse the same material and upload
it through `gh` stdin (secret values are never command arguments or log output):

```sh
python3 scripts/release/msix_signing.py --upload-secrets --repo alfu32/meshcraft
```

The **public** certificate is published per architecture as
`meshcraft-X.Y.Z-windows-x64.cer` or `meshcraft-X.Y.Z-windows-arm64.cer`; private
keys and passwords are never release assets. Sideload testers must explicitly
trust that public certificate in Local Machine Trusted People, then install:

```powershell
# Elevated PowerShell, using the CER matching the MSIX download:
Import-Certificate -FilePath .\meshcraft-X.Y.Z-windows-x64.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Add-AppxPackage .\meshcraft-X.Y.Z-windows-x64.msix
```

Self-signing does not establish public CA/SmartScreen trust. Microsoft signs Store
submissions during distribution. See Microsoft's [certificate guidance](https://learn.microsoft.com/windows/msix/package/create-certificate-package-signing)
and [Store version requirements](https://learn.microsoft.com/windows/apps/publish/publish-your-app/package-version-numbering?pivots=store-installer-msix).
On both Windows runners, CI temporarily trusts the public CER, installs the MSIX,
checks the installed PFN/publisher/version/architecture, runs its version command,
then removes the package and temporary trust. Final publication also checks the
Store manifest, public certificate, source/payload digests, signing metadata and
every MSIX block-map hash.

macOS uses a deployment target of 11.0, an application bundle/icon and an ad-hoc
signature, packaged in a verified DMG. Developer ID signing/notarization are not
configured. A successful CI build does not substitute for native GUI testing,
particularly the offscreen context on macOS and ARM64 GPU/driver behavior.

## Local checks and packagers

```sh
python3 -m unittest discover -s scripts/release -p 'test_*.py' -v
python3 scripts/release/meta.py # requires GITHUB_REF=refs/tags/<app version>
docker build -t meshcraft-build -f packaging/linux/Dockerfile .
```

The workflow contains the complete old-sysroot build invocation. Native packagers
consume the verified portable archives; they never rebuild against the packager's
newer libc. `scripts/release/verify.py --help` describes final release verification.

## Object scripting runtime

Linux bundles JavaScriptCoreGTK 4.0 and its audited old-glibc dependency closure. macOS uses the system JavaScriptCore framework. Windows script execution requires a compatible `JavaScriptCore.dll`; its redistributable package and live qualification are still tracked by T47/T62. The app reports missing runtime libraries in the script editor and never downloads an engine at runtime. Ordinary graphical modeling remains available without the engine.
