# ADR-06: GPUI desktop shell with a raylib-rs model viewport

- Status: Accepted direction; embedded-surface feasibility is the first implementation gate in [T59](../rewrite/handoffs/T59.md) and high-priority [issue #21](https://github.com/alfu32/meshcraft/issues/21).
- Date: 2026-10-19
- Supersedes: [ADR-05](ADR-05-ui-architecture.md), the egui shell with Bevy viewport, and the earlier Bevy UI direction in ADR-04.

## Context

The current Bevy/egui application has had persistent UI quality, responsiveness and integration problems. The user selected GPUI for a richer native component ecosystem and raylib-rs for the 3D canvas. The supplied SolidCraft reference establishes the menu/ribbon/viewport/inspector arrangement; the CADCraft reference establishes the searchable `Ctrl+Shift+P` command palette. The two local screencasts show object creation/copy and dense array workflows followed by orbit, zoom, selection and display inspection. One also captures the legacy application hanging during a large array operation.

The object model, geometry operations, command IDs, parameter definitions and transactions are independent product behavior and remain in scope. Existing UI and renderer code are replaceable presentation layers. GPUI's upstream API is pre-1.0; its [external-compositor proposal](https://github.com/zed-industries/zed/discussions/60572) documents the absence of a general zero-copy path for arbitrary external GPU renderers. raylib-rs normally owns an OpenGL context/window, so embedding is an explicit feasibility requirement.

## Decision

Use GPUI for the native desktop application and GPUI Component where its dependency/version surface is suitable. Use raylib-rs for the custom model viewport. The current presentation candidate renders into raylib `RenderTexture2D` and publishes the pixels as a normal GPUI `ImageSource::Render` image. That adds a GPU-to-CPU readback and a GPUI texture upload, but unlike a native child window it participates in normal GPUI clipping and z-order. A release proof at 1280×800 sustained 59–60 new images/sec on the current Linux host, meeting the requested 25 FPS floor and 30–60 FPS target for a simple scene. Keep this path provisional until the real viewport and dense-array workflow pass the same performance gate. Do not use Bevy, egui/eframe or a CPU raster renderer in the final desktop runtime.

The GPUI shell has a textual application menu, grouped tabbed ribbon, central viewport, right-side selection/object/settings inspector and a status/command-feedback row. The command palette is searchable with `Ctrl+Shift+P`. Build these from the existing command catalog and JSON schemas so menus, buttons, shortcuts, automation and palette invoke the same headless commands.

Keep `meshcraft-core` authoritative and independent of GPUI/raylib. Extract the desktop command/session controller behind framework-neutral typed input, output and host-effect contracts. GPUI owns widget layout, focus and UI state. raylib owns camera presentation, viewport GPU resources, batched scene drawing, picking and continuous viewport feedback. Synchronize immutable revisioned snapshots/deltas after model changes; camera motion and pointer previews stay local to the viewport. Neither UI events nor pointer movement may reconstruct the whole scene.

Preserve tool-first command semantics and staged point input, selection of both mesh entities and objects, existing geometry/grid snapping for every point operator, directional window selection, connected/coplanar multi-click selection, active-definition editing, Escape/cancel behavior, planar-first construction, separate modification and array families, and attached hotspot operations. Render front/back faces with their distinct tints, cast shadow-map shadows from solid geometry onto geometry and ground, and exclude authored lines and feedback overlays from the shadow pass.

## Required feasibility and cutover gates

T59 must choose and prove a presentation path on the target Linux desktop. The offscreen-image path must render into a viewport-sized raylib target, publish only the latest frame to a normal GPUI image component, preserve menu/panel z-order, and sustain at least 25 FPS (40 ms per frame) while targeting 30–60 FPS. Verify resize/DPI mapping, clipping, focus, pointer/keyboard routing and clean shutdown on the active Wayland or X11/XWayland backend. Validate frame timing and memory with the dense-array workflow before treating CPU readback as production-qualified. A native child surface remains an option only if its occlusion and menu stacking also pass. Pin the working dependency revisions after the selected path passes these checks.

After the session boundary and viewport/shell replacement work, remove Bevy, bevy_egui, egui/eframe and their old renderer/UI modules from the desktop executable and lockfile. Keep command/model/IO functionality and its headless tests. T60 tracks the deletion and preservation check. Qualify other operating systems before claiming support there.

## Acceptance

C39 covers GPU rendering, batched geometry, two-sided colors, shadow policy, overlays, selection and the shared model boundary. C40 covers the GPUI application, raylib viewport presentation, UI component behavior, event ownership and command parity. C41 replays the recording-derived dense-array workflow and reports interaction latency, command completion, memory and frame timing without hangs. C26 retains the reproducible 2,000-entity performance report; issue #18 stays open until those measurements show the agreed improvement.
