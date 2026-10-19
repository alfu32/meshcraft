# GPUI + raylib-rs desktop architecture

**Direction:** GPUI for the desktop shell and raylib-rs for the 3D canvas. T59 must prove an embedded, resizable viewport before the full cutover. The integration is a technical gate, not permission to ship a CPU-copy viewport.

## Functional layout

Use GPUI, with the GPUI Component ecosystem where its version and licensing fit, for a conventional CAD workspace:

- A textual application menu above a tabbed/grouped command ribbon, following the SolidCraft organization.
- A central raylib viewport with the model, construction feedback and camera controls.
- A right-side inspector for selection, object hierarchy, model settings and tool settings. Generate forms from the existing JSON setting/command definitions.
- `Ctrl+Shift+P` opens a searchable command palette with CADCraft-style completion, shared enabled/checked state and parameter entry.
- A status area reports the active tool, prompt, current selection and command result. Empty documents show the complete shell and useful empty states.

Menus, ribbon buttons, palette entries, automation and keyboard shortcuts invoke the same command IDs. UI widgets do not implement modeling. Keep the command registry and editor-session operations callable without GPUI, raylib, a window or a graphics context; route file dialogs and other OS requests through typed host effects.

## Ownership and update flow

`meshcraft-core` remains authoritative for documents, geometry, selection, transactions, tools and commands. The desktop session owns transient command/input state and emits versioned changes. GPUI owns shell layout, widgets, keyboard focus and UI preferences. raylib owns viewport GPU resources, scene batches, camera presentation, ray picking and high-frequency pointer feedback.

The viewport boundary uses renderer-neutral snapshots/deltas and typed input/events. Camera orbit, pan, zoom and pointer previews stay local to the viewport at frame rate. Send a model delta only after a command or committed document change; never rebuild or clone the complete scene for each mouse move. GPUI decides whether pointer/keyboard input belongs to the shell or viewport using the actual laid-out viewport bounds, including DPI scale and resize.

Preserve the editor interaction contract: select mesh entities, objects or mixed selections before choosing an operator; staged points snap to both existing geometry and the construction grid; left-to-right windows select fully contained projections while right-to-left windows include intersecting projections; clicking a selected entity toggles it off; double/triple-click selection expands connected/coplanar elements; entering an object instance edits its shared definition. Escape aborts the active tool, clears selection (or guides when selection is empty), and returns to Select. Geometry creation writes directly to the active scene graph. Keep planar construction, 3D modification, distinct linear/planar/spatial and rotational/helical arrays, and hotspot-attached operations in the shared command/session model.

### Renderer-neutral viewport protocol

Version 1 input events carry `{ protocol_version: u16, sequence: u64, event }`. `event` uses semantic values independent of GPUI and raylib: `OrbitDelta { dx_pixels: f32, dy_pixels: f32 }`, `ZoomSteps { steps: f32 }`, and `ResetCamera`. The viewport validates the version and monotonically increasing sequence before applying an event. Camera motion and pointer previews stay local; semantic tool actions and committed edits go through the headless session API.

Initial scene transfer uses `ViewportSceneSnapshotV1 { model_revision: u64, active_definition_id, entities }`. Each entity has a stable entity ID, optional parent occurrence and definition IDs, a local-to-parent 4×4 f64 transform, geometry payload (f64 positions and indexed faces/lines), and render style. Follow-up `ViewportSceneDeltaV1 { base_revision: u64, revision: u64, upsert, remove }` applies stable-ID entity changes. The renderer accepts a delta only when `base_revision` equals its current revision; otherwise it requests a replacement snapshot. Full snapshots are for initial load/recovery, never camera motion or pointer movement. Selection/hover/preview state is transmitted separately from immutable geometry revisions.

## raylib integration gate

raylib-rs normally owns an OpenGL window/context. GPUI does not provide a general cross-platform API for importing arbitrary raylib/OpenGL textures into its compositor. A continuous GPU→CPU framebuffer readback followed by GPUI image upload is rejected for the production viewport because it adds bandwidth, copies and interaction latency.

T59 must demonstrate a real raylib canvas hosted in the GPUI viewport rectangle on the target Linux session, with resize/DPI alignment, clipping, focus, pointer and keyboard routing, clean shutdown, and no per-frame CPU pixel readback. Test the active Wayland or X11/XWayland path and record the exact backend. Qualify Windows and macOS before claiming support there. Investigate a native child surface first; shared-GPU-texture integration is a separate option only when it can be implemented and maintained on the supported render backends. Pin the tested GPUI, component and raylib revisions after the proof.

The upstream GPUI project is pre-1.0 and documents ongoing API changes in its [README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md). Its [external-compositor discussion](https://github.com/zed-industries/zed/discussions/60572) describes the missing general zero-copy path for externally rendered GPU viewports on non-macOS backends. Treat the spike as required evidence, not an assumption that the proposed integration is already supported.

## Viewport rendering requirements

raylib batches stable scene geometry by material/domain and updates only changed revisions. Preserve front/back face tinting (back faces slightly darker and bluer), shadow-map shadows from solid geometry onto other entities and the ground, and exclude authored lines, selection/preview overlays and the construction grid from the shadow pass. Render selection/window feedback, guide axes, oriented cursor, staged geometry previews, measurements and annotation text in dedicated overlay paths. Keep sorting/picking data separate from translucent and screen-space feedback.

The user-provided recordings (`docs/Screencast From 2026-10-19 10-42-51.mp4` and `docs/Screencast From 2026-10-19 10-47-07.mp4`, intentionally ignored by Git) show dense copies/arrays being inspected with orbit, zoom, selection and display changes; the later recording also shows the legacy app becoming unresponsive during a large array operation. Use this workflow as the stress scenario. Measure command latency, pointer-to-feedback latency, frame time, pick/window-select time, memory and idle CPU on fixed scenes; never treat a frozen or stale viewport as acceptable feedback.
