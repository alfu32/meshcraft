# ADR-04: Cross-platform UI and rendering stack

- Status: Superseded for the native desktop application by the Bevy migration recorded below; retained as history for the first shell and browser adapter.
- Date: 2026-10-18; superseded 2026-10-19

## Current decision (2026-10-19)

Use Bevy 0.20 for the native desktop application window, ECS scene, GPU 3D renderer, input and Bevy UI. This replaces the initial eframe/egui desktop shell because rendering and hover are too slow on large scenes (#18); the staged migration is high priority under T57 and [issue #19](https://github.com/alfu32/meshcraft/issues/19). Bevy 0.20 requires Rust 1.97.1, so the workspace MSRV is raised accordingly. The normalized document and shared command/session services remain authoritative; Bevy entities and meshes are derived presentation data. Keep `meshcraft-core` and `meshcraft-io` free of Bevy dependencies. The current web adapter is separate and remains on its existing UI stack until the later browser delivery.

The first Bevy vertical slice must load real Meshcraft data, render its entities without rebuilding unchanged geometry per frame, and expose actions through Bevy UI. Pin Bevy 0.20.0 in Cargo manifests after validating its native platform feature set and build; do not claim a performance improvement until C39 records measurements against the T00 baseline.

## Decision

Use `eframe`/`egui` 0.37.0 as the initial application and UI layer. It supplies the native event loop through winit, a shared native/WASM UI, and the WGPU renderer. Pin `eframe` to `=0.37.0`; set the project MSRV to Rust 1.95 and pin the development toolchain to Rust 1.99.1. Keep the UI independent of the normalized document contracts gated by T46.

Use an in-window `egui::MenuBar` for the desktop menu profile. The shell also exposes Buttons and Hybrid profiles through the same UI state. Do not add an OS menu adapter in this slice: it would create a second platform integration surface before the command registry exists. Revisit macOS menu-bar integration during T08 using the shared command tree, if native integration provides a clear usability benefit.

## Evidence and limits

The shell compiles against the native target and `wasm32-unknown-unknown`. A text-edit probe and a viewport rectangle are included to exercise input focus, text/IME forwarding and layout offsets. Build evidence alone does not establish IME behavior, menu accessibility, focus restoration, or native macOS menu conventions; record those only after interactive platform checks under C23. Eframe documents native and web builds and exposes WGPU as a renderer; [eframe 0.37.0](https://docs.rs/crate/eframe/0.37.0) requires Rust 1.95 or newer.

## Consequences

- One UI code path can serve native and browser builds.
- The first shell uses in-window menus on every desktop platform; OS-specific menu behavior is not claimed.
- Desktop WGPU and WASM adapter availability remain runtime capabilities to verify on supported platforms.
- UI actions must migrate to the shared command registry before feature menus are enabled.
