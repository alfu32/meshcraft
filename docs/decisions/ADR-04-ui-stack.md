# ADR-04: Cross-platform UI and rendering stack

- Status: Superseded by [ADR-05](ADR-05-ui-architecture.md); retained as a record of the initial eframe decision and the later Bevy UI direction.
- Date: 2026-10-18; Bevy UI direction superseded 2026-10-19

## Superseded Bevy UI decision (2026-10-19)

Use Bevy 0.20 for the native desktop window, ECS scene, GPU 3D renderer, input and Bevy UI. This direction was implemented in part under T57, but the user rejected Bevy UI's visual quality and UX. Issue #19 was closed as not planned for that UI approach; its Bevy GPU renderer and custom model viewport remain the foundation for replacement task T58 and [issue #20](https://github.com/alfu32/meshcraft/issues/20). This closure records a scope change and does not claim the old Bevy UI acceptance or the #18 performance benchmark is complete. Bevy 0.20 requires Rust 1.97.1, so the workspace MSRV was raised accordingly. The normalized document and shared command/session services remain authoritative; Bevy entities and meshes are derived presentation data. Keep `meshcraft-core` and `meshcraft-io` free of Bevy dependencies. The browser adapter remains separate until the later browser delivery.

The retained Bevy viewport loads real Meshcraft data and renders entities as GPU meshes with camera, grid and lighting. C39 still requires measurements against the T00 baseline; do not claim a performance improvement until those measurements are recorded.

## Original initial decision (2026-10-18)

Use `eframe`/`egui` 0.37.0 as the initial application and UI layer. It supplies the native event loop through winit, a shared native/WASM UI, and the WGPU renderer. Pin `eframe` to `=0.37.0`; set the project MSRV to Rust 1.95 and pin the development toolchain to Rust 1.99.1. Keep the UI independent of the normalized document contracts gated by T46.

Use an in-window `egui::MenuBar` for the desktop menu profile. The shell also exposes Buttons and Hybrid profiles through the same UI state. Do not add an OS menu adapter in this slice: it would create a second platform integration surface before the command registry exists. Revisit macOS menu-bar integration during T08 using the shared command tree, if native integration provides a clear usability benefit.

## Evidence and limits

The shell compiles against the native target and `wasm32-unknown-unknown`. A text-edit probe and a viewport rectangle are included to exercise input focus, text/IME forwarding and layout offsets. Build evidence alone does not establish IME behavior, menu accessibility, focus restoration, or native macOS menu conventions; record those only after interactive platform checks under C23. Eframe documents native and web builds and exposes WGPU as a renderer; [eframe 0.37.0](https://docs.rs/crate/eframe/0.37.0) requires Rust 1.95 or newer.

## Consequences

- One UI code path can serve native and browser builds.
- The first shell uses in-window menus on every desktop platform; OS-specific menu behavior is not claimed.
- Desktop WGPU and WASM adapter availability remain runtime capabilities to verify on supported platforms.
- UI actions must migrate to the shared command registry before feature menus are enabled.
