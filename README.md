# Meshcraft: k3d / Octodraw Rust rewrite

The rewrite began as a source-backed specification and implementation handoff, prepared from `/home/alfu64/Development/k3d` at commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7` (tag `5.7.2`). The native application now uses the full GPUI interface with an offscreen raylib viewport, shared modeling/session commands, project persistence, undo, reusable prototypes, per-instance hotspots and selection/display controls. The rewrite remains in progress; the [dashboard](DASHBOARD.md) records tested coverage and remaining work.

Start with [the rewrite guide](docs/rewrite/README.md). It links the feature specification, compatibility requirements, architecture proposal, history analysis, acceptance scenarios, and dependency-ordered agent tasks.

Desktop is the first delivery target. It includes a conventional menu interface alongside the button-oriented workspace. Browser support and JavaScriptCore plugins with a versioned host context follow desktop qualification. The object model has an early normalization gate; the current Rust model is a prototype pending that gate. Android, Groovy/JVM plugin execution, and CLI/TUI developer tools remain deferred.

## Run and release

```sh
env -u WAYLAND_DISPLAY cargo run -- --open examples/dynamic-object.octd
cargo test --workspace --locked
```

The manual [release workflow](.github/workflows/distribution.yaml) builds Linux, Windows and macOS on x86_64 and ARM64. Select an existing version tag to run it; the tag must match the literal app version in `apps/desktop/Cargo.toml`. See [packaging and signing status](packaging/README.md) for the artifact set, old-glibc build environment and platform qualification limits. MSIX signing is deferred.
