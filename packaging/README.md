# Manual build and release

Meshcraft uses the same six-target distribution matrix as the Rust Kode reference
(`alfu32/kodecraft`, commit `f3f915c5a9c14f445a3f4e83e520ce2970831636`). There is no JDBC/Java payload.

| Native target | Portable archives | Additional installers |
| --- | --- | --- |
| Linux x86_64 / aarch64 | ZIP, tar.gz | AppImage, Snap |
| Windows x64 / ARM64 | ZIP, tar.gz | MSI, unsigned MSIX |
| macOS Intel / Apple Silicon | ZIP, tar.gz | DMG containing Meshcraft.app |

Each release contains 22 primary artifacts, per-file SHA-256 sidecars and a combined
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
A cold build still compiles the GPUI/raylib and transitional egui/wgpu dependency trees;
warm-run improvements depend on the GitHub cache being available.

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
come from the OS. The launcher prefers available desktop/graphics-provider libraries and uses the bundle for missing dependencies, so XKB remains compatible with the host Compose data. A working X11 desktop, fonts and a GPUI-compatible Vulkan driver
are still runtime requirements; the glibc ceiling alone is not full desktop support.

AppImage uses checksum-pinned appimagetool/type2 runtime from the reference release
pipeline and works without FUSE via `APPIMAGE_EXTRACT_AND_RUN=1`.
The Snap uses `core24` and Canonical's `mesa-2404` graphics content provider so the
GPUI/Vulkan and raylib/OpenGL APIs are available inside confinement. This isolated
Snap runtime is separate from the old-glibc portable/AppImage build baseline.
For local unsigned Snap installation, install the base/provider and connect the
`gpu-2404`, `x11` and `opengl` plugs as required by snapd. Store publication is separate.

The Ubuntu 18.04 dependency and diagnostic link/ELF checks passed with the pinned Rust toolchain. The
container uses Python 3.8 and pip 25.0.1 for the newer CMake tool. Its multi-architecture
base image is pinned by digest. The actual Linux AppImage also passed a GUI smoke check; the exact executed
probes and remaining platform limits are recorded in the [release handoff](../docs/rewrite/handoffs/T44.md).

## Windows and macOS

Windows builds use the native SDK, LLVM/libclang for raylib bindings, GPUI's `fxc`
shader compiler, and static CRT linking. The CMake toolchain explicitly sets the
MSVC runtime to `MultiThreaded` for raylib/GLFW, matching Rust `+crt-static`;
setting only Rust flags leaves CMake targets using the DLL runtime. WiX 5.0.2 produces MSI, and the Windows SDK
MakeAppx produces **unsigned** MSIX. No signing secret is requested or referenced.
MSIX signing is deliberately deferred until repository secrets and the publisher
identity are configured; unsigned MSIX is a packaging output, not a trusted,
ready-to-install production package.

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
