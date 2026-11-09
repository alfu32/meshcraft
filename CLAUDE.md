# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

`AGENTS.md` holds the project rules (versioning, tagging, release gate, issue handling, durable assets, stress and distribution constraints). Its opening statement that implementation has not started is out of date: the Rust application is implemented and released, and the rest of its rules still apply.

## Commands

The toolchain is pinned by `rust-toolchain.toml`. Native build dependencies are listed in `packaging/linux/Dockerfile` and `packaging/README.md`.

```sh
# Run (unsetting WAYLAND_DISPLAY selects the X11 path used for desktop validation)
env -u WAYLAND_DISPLAY cargo run -- --open examples/dynamic-object.octd
env -u WAYLAND_DISPLAY cargo run -- --mcp-port 8765     # start the MCP server at launch
cargo run -- --command-catalog                          # print the command registry as JSON; no display needed

# Release gate: all three are required before publishing a version
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

# Single test (filter by substring)
cargo test -p meshcraft-desktop --lib --locked native_toolbar_catalog_has_unique
cargo test -p meshcraft-core --locked <name>
cargo test -p meshcraft-io --locked <name>

# Python helpers (standard library)
python3 -m unittest discover -s scripts/release -p 'test_*.py' -v
python3 -m unittest discover -s scripts/stress -p 'test_*.py' -v
python3 -m unittest discover -s scripts -p 'test_export_tool_inventory.py' -v

# Keep the tool inventory aligned with the registry (needs a built target/debug/meshcraft)
python3 scripts/export_tool_inventory.py          # add --check to fail on drift

# Plan and spec navigation
python3 docs/rewrite/scripts/verify_plan.py
python3 docs/rewrite/scripts/handoff.py --ready   # also --list, Txx, --find SYMBOL, --tool NAME
```

- `default-members` is only `apps/desktop`, so a bare `cargo test` or `cargo clippy` skips `meshcraft-core` and `meshcraft-io`. Pass `--workspace` or `-p`.
- `[profile.dev.package."*"]` builds dependencies at `opt-level = 3`. CI overrides this with `cargo --config 'profile.dev.package."*".opt-level=0' ...` to speed up validation builds.

### Native UI verification

`scripts/verify_*.py` drive the real application in a private Xvfb display over MCP, and most write evidence to `docs/rewrite/references/<topic>/`. They require `Xvfb`, `xdotool` and the Mesa lavapipe Vulkan driver, and take the executable as `--binary`. `scripts/stress/run.py --binary <path> --tiers small --samples 1` uses the same harness (`scripts/stress/runtime.py`) for a quick benchmark. UI fixes are normally committed with a matching verify script and its captured evidence.

## Versioning

The app version is `version` in `apps/desktop/Cargo.toml`; the workspace version in the root `Cargo.toml` is not the app version. Bumping it also changes `Cargo.lock`, which `--locked` enforces. Tags have no `v` prefix (`0.48.5`), and `scripts/release/meta.py` requires the tag to equal the app version.

## Architecture

There are three workspace crates. `docs/rewrite/architecture.md` proposes further crates (`meshcraft-session`, `meshcraft-render`, `meshcraft-ui`) that do not exist; their responsibilities currently live in `apps/desktop`.

- **`crates/meshcraft-core`**: f64 geometry, `Document`, selection, transactions, the generic `CommandRegistry`, tool state machines and `operations/`. It has no UI, GPU or filesystem dependencies.
- **`crates/meshcraft-io`**: `.octd`/`.k3d` project reading and writing, legacy DTOs, and interchange codecs in `mesh/`.
- **`apps/desktop`** (`meshcraft-desktop`, binary `meshcraft`): everything else.

`experiments/stack` is a workspace member. `experiments/stack-gpui-raylib` is not, but the root `Cargo.toml` patches `xattr` from its `vendor/` directory, and `gpui-component` from the top-level `vendor/`. Neither can be removed casually.

### How the desktop app fits together

The two large files are entry points to two layers, each split into `#[path]` submodules:

- **Session layer, `app.rs` (`MeshcraftApp`)**: the headless-capable model session, with `*_workflow.rs` submodules. It owns the document and tool state, and contains no GPUI code.
- **Native shell, `gpui_app.rs`**: the GPUI window, menus, ribbon and inspectors, with `gpui_*.rs` submodules. It holds a `MeshcraftApp` as `session`. raylib rendering is also driven from the `gpui_*` modules (notably `gpui_gpu_mesh.rs`, `gpui_procedural.rs`, `gpui_lighting.rs`), and the result is presented inside the GPUI viewport.

`viewport.rs` holds camera, picking and overlay logic. `egui` remains a dependency because `app.rs` and `viewport.rs` use its types and the wasm entry (`web.rs`, `eframe`) uses it, so its presence in `app.rs` does not mean the native UI is egui.

### Commands are the central seam

Menus, ribbon, left panel, palette and MCP all go through one path: `MeshcraftApp::dispatch_command(id, payload)` → `shell_commands()` (a `CommandRegistry<ShellAction>` in `app.rs`) → `execute_command`, which matches on `ShellAction`. UI surfaces are projections of this registry; the functional layout is in `docs/menu-layout.md`.

Adding or renaming a command touches more than the registry:
- `docs/tools/tools.csv` and `docs/tools/index.html`, regenerated with `export_tool_inventory.py`, which preserves the review columns by command ID.
- The stress suite, which reads `--command-catalog` live and reports `coverage_complete` only if every command ran at every tier (`scripts/stress/run.py`). `recipe()` in `scripts/stress/cases.py` decides how each command is exercised.
- Icon assignments, covered by `assets/tool-icons/` and `scripts/test_tool_icons.py`.

### Process boundaries

- **MCP (`mcp.rs`, `mcp_protocol.rs`, `mcp_workflow.rs`)**: a bounded loopback HTTP server, off by default, whose work is queued onto the application owner thread rather than touching the model from the socket thread. See `docs/mcp.md`.
- **Scripted objects (`script_runtime.rs`, `script_engine.rs`, `script_sdk.js`, `script_api.rs`)**: JavaScript runs in a child process, the same binary re-executed with `--script-worker`. It loads system JavaScriptCore at runtime with `libloading` and has no filesystem or network bindings. `script_api.rs` backs both Ctrl+Space discovery and the MCP API listing, so keep it in step with `script_sdk.js`.

## Where to look

- `docs/rewrite/spec/*.md`: behavior requirements (`F01`-style IDs).
- `docs/rewrite/tasks.json`: authoritative task registry and dependencies. `acceptance.md` holds the scenarios (`C01`-style IDs), and `handoffs/Txx.md` records per-task validation evidence.
- `docs/decisions/`: ADRs, including the GPUI and raylib stack choice.
- `fixtures/`: legacy samples and scenarios, some embedded in tests with `include_str!`. `crates/meshcraft-io/tests/fixtures/` holds interchange samples.
