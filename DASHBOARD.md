# Dashboard

Updated 2026-10-20. **0.4.0 interaction fixes:** geometry previews use thick model-line strokes; GPUI motion restores Push/Pull and specialized tool feedback. Move Copy/Rotate Copy are enabled for mesh fragments and preserve their sources. Ordinary fragment transforms detach shared boundaries; Stretch remains connected. Middle-button drag pans. Isolated circles insert in one batch and line repair preserves untouched topology. Selection rendering caches membership and omits redundant interior edges/per-vertex object markers. On the local software GPU, 5,183 selected faces/edges rendered in 23.83 ms versus 23.22 ms unselected at 1280×800; the local circle probe fell from 60.59 s to 248 ms, plus 50 ms for snapping refresh. These are bounded probes, not broad hardware qualification. **494 regular workspace tests pass** (228 core, 246 desktop, 15 IO, 5 CLI), plus explicit GPU and circle probes; build, strict Clippy, formatting and plan checks pass. See [captures and exact validation](docs/rewrite/handoffs/interaction-performance-validation.md). The [selection controls](docs/rewrite/handoffs/selection-controls-validation.md) and user-tested [object/hotspot workflow with 19 captures](docs/rewrite/spec/objects.md) remain in place. Wider domain/platform/performance qualification stays with the existing tasks. `docs/rewrite/tasks.json` is authoritative.

**Counts:** Todo 24 · In progress 14 · Done 22 · Superseded 1 · Total 61

## Current in-progress tasks

- **T05 — Implement sparse legacy DTO reading and model/history containers**: Finish sparse legacy DTO/model-history compatibility.
- **T09 — Implement desktop document service and safe storage**: Complete safe desktop project storage/recovery.
- **T16 — Implement reusable objects, placement and explode**: Finish reusable object editing and explode behavior.
- **T17 — Complete GPUI inspectors, settings and preferences (#26)**: Editable model/UI controls, grid normal, normal/thick model line widths, persisted settings accordion and shader AA are wired. Toolbars remain expanded. Buttons keep text beside 419 transparent SVG icons. Ely toolkit investigation (#26) is complete; see [the audit and migration assessment](docs/rewrite/ely-gpui-review.md). Broader inspector/domain acceptance remains open; see [T17](docs/rewrite/handoffs/T17.md).
- **T21 — Port every copy and array state machine**: Complete the remaining array state machines and adapters.
- **T22 — Port hole/cutout algorithms and robust replacement**: Finish robust cutout/hole replacement cases.
- **T32 — Implement hotspot definitions, IDs and attachment graph**: Porting legacy authoring/property/attachment workflow into the shared model and GPUI panels.
- **T33 — Implement dynamic hotspot instance evaluation**: Porting immutable-source evaluation, instance values, handle drag/preview and copy semantics.
- **T34 — Implement external prototype references**: Integrating external scenes as reusable subscenes, with root hotspots and refresh/error handling.
- **T39 — Complete real-time lighting, shadows and view capture (#13)**: Implemented the raylib light-camera shadow pass, directional/ambient face shading, black authored lines, three-plane guides/local point field, grid-line snapping, reversed orbit and staged Escape. The extended transparent SVG atlas generates at build time. Tight light-camera clip bounds fix detached shadows (analytic ground projection IoU 0.937). GPU checks pass for ground/entity receivers, black model line widths and all three shader AA qualities; 233 regular desktop tests pass. The full GPUI shell builds/runs with generated icons. Lighting controls are session-local; capture, legacy lighting persistence and broader T39 quality acceptance remain open. See [T39](docs/rewrite/handoffs/T39.md).
- **T51 — Restore and verify geometric feedback in the raylib viewport (#2, #21)**: Relative point/vector feedback now includes array stages, revolve and Paint; full interactive tool coverage remains open.
- **T54 — Persistent oriented cursor and context-safe guide placement (#8, #9, #14, #15)**: Carry cursor orientation and context-safe guide placement through the new viewport.
- **T59 — Prove GPUI + raylib-rs viewport integration (#21, high priority)**: The raylib 6 offscreen proof renders at 1280×800 and presents frames through a regular GPUI image component, so GPUI overlays remain above the canvas. Release runs sustain 59–60 new images/sec; worker frame work is typically 4–12 ms, with occasional 18 ms samples. This meets the requested 25 FPS floor and 30–60 FPS target for the simple proof scene. The production gate remains open for live viewport input, resize/DPI behavior, and the dense-array workflow.
- **T60 — Integrate GPUI and raylib desktop while preserving headless commands (#21, #25, high priority)**: Main now calls the actual `0.2.0` shell layout functions in `gpui_layout.rs`: original palette grouping/button sizes, labeled two-row ribbon, selection/object tables, paired model fields and all domain settings panels, matched against `docs/full_ui.png`. Supported controls bind to existing commands; unfinished controls remain disabled. The event-driven offscreen raylib image and model/session helpers are retained. Fixed zero session viewport bounds that ignored construction clicks, unified pointer/render layout dimensions, and kept menu/palette overlays out of normal layout with measured dropdown anchors. Live X11 checks confirm ribbon switching, settings scrolling, menu layering, two-point Line creation with feedback/count updates, and Undo outside the viewport. The compact searchable palette and dropdown menus are left-aligned; tool feedback text is a GPUI overlay. Retired GPU frame images are explicitly evicted (120-update baseline grew 241 MiB; patched 600-update run grew under 2 MiB). Palette scrolling consumes input without zooming the canvas, and Delete routes to selection deletion outside text editors. The latest run passes 235 regular desktop tests plus the explicit GPU regression, the application build, formatting and the plan verifier. Screenshots and exact commands are in [the T60 handoff](docs/rewrite/handoffs/T60.md). Physical high-DPI, dense-array acceptance and remaining egui/eframe extraction stay open.

## Architecture change

The accepted desktop architecture is GPUI for the shell and raylib-rs for the 3D viewport. T58/issue #20 are superseded; high-priority issue #21 tracks the replacement. T59's release proof presents raylib `RenderTexture2D` frames as normal GPUI images at 59–60 updates/sec at 1280×800, fixing menu/viewport stacking in the proof shell. T60 now starts the production app through GPUI, publishes the live document/session to the offscreen raylib renderer, and routes viewport selection, picking, snapping, tools, guides and previews through existing APIs. The worker renders on scene, camera or viewport changes; a lightweight frame watcher refreshes GPUI when a frame arrives, avoiding continuous idle redraws. All catalog commands are reachable in their menus, ribbon groups, tool palettes or the full command palette. UI preferences now update the shared session and renderer grid/workplane. The retired Bevy host and dependencies are removed. Remaining egui/eframe coupling is still under extraction; manual visual/input, resize/DPI and dense-array acceptance remain open. Keep command IDs, JSON command/settings definitions, session operations, model and IO behavior.

The local recordings `docs/Screencast From 2026-10-19 10-42-51.mp4` and `docs/Screencast From 2026-10-19 10-47-07.mp4` are intentionally Git-ignored. C41 records the dense object-array, orbit/zoom/selection and responsiveness workflow; the legacy freeze is a regression case.

## All task statuses

| ID | Task | Status |
| --- | --- | --- |
| T00 | Capture legacy behavior, fixtures and measurement baseline | Done |
| T01 | Prove the Rust rendering, menu and geometry stack | Done |
| T02 | Implement numerical geometry and coordinate contracts | Done |
| T03 | Build canonical document stores, IDs and revisioned derived data | Done |
| T04 | Implement transactions and bounded undo | Done |
| T05 | Implement sparse legacy DTO reading and model/history containers | In progress |
| T06 | Create command registry and host-effect dispatch | Done |
| T07 | Build GPUI desktop window and raylib viewport with normalized input ownership | Todo |
| T08 | Implement GPUI desktop menus, grouped ribbon, and context command menu (#16, #17) | Todo |
| T09 | Implement desktop document service and safe storage | In progress |
| T10 | Port orbit, orthographic and walkthrough cameras | Done |
| T11 | Implement snapping, permanent grid and guides | Done |
| T12 | Port mixed selection and semantic picking framework | Done |
| T13 | Implement tool protocol and line editing | Done |
| T14 | Implement basic planar shape tools | Done |
| T15 | Port transforms, stretch variants and painting | Done |
| T16 | Implement reusable objects, placement and explode | In progress |
| T17 | Complete GPUI inspectors, JSON-driven settings, preferences and command coverage (#26) | In progress |
| T18 | Implement dimensions, bitmap text and vector glyph text | Todo |
| T19 | Implement paths, offsets, outlines and mesh creation | Done |
| T20 | Implement region push/pull | Done |
| T21 | Port every copy and array state machine | In progress |
| T22 | Port hole/cutout algorithms and robust replacement | In progress |
| T23 | Port planar/spatial sweeps, ribbon and revolve | Done |
| T24 | Port plane section, plane cuts and mesh intersection | Todo |
| T25 | Implement primitive mesh generators | Done |
| T26 | Implement built-in mechanical tools | Todo |
| T27 | Implement object cutting and booleans | Todo |
| T28 | Implement voxel domain and tools | Todo |
| T29 | Implement architecture walls, slabs and holes | Todo |
| T30 | Implement stairs and free-form frames | Todo |
| T31 | Implement HVAC plumbing and ventilation | Todo |
| T32 | Implement hotspot definitions, IDs and attachment graph | In progress |
| T33 | Implement dynamic hotspot instance evaluation | In progress |
| T34 | Implement external prototype references | In progress |
| T35 | Implement random offset, surface scatter and both remesh modes | Todo |
| T36 | Implement OBJ/STL/DXF interchange and unit policy | Todo |
| T37 | Implement extended mesh formats | Todo |
| T38 | Implement IFC4 proxy export and supported geometry import | Todo |
| T39 | Complete real-time lighting, shadows and view capture; remove mesh triangulation seams (#13) | In progress |
| T40 | Port progressive CPU offline rendering | Todo |
| T41 | Implement structured automation and regression dispatcher | Todo |
| T42 | Audit phase-specific capabilities and preserve migration boundaries | Todo |
| T43 | Implement desktop tutorial recorder/player and schema adapters | Todo |
| T44 | Qualify desktop performance, complete UI coverage and package releases | Todo |
| T45 | Deliver browser runtime, component and docs integration | Todo |
| T46 | Ratify the normalized object model and migration contracts | Done |
| T47 | Prove JavaScriptCore embedding, scheduling and native packaging | Todo |
| T48 | Implement JavaScriptCore runtime lifecycle and context facade | Todo |
| T49 | Connect the versioned JavaScript plugin SDK to normalized host services | Todo |
| T50 | Deliver JavaScriptCore plugin UX, samples and qualified desktop packages | Todo |
| T51 | Restore and verify geometric feedback in the raylib viewport (#2, #21) | In progress |
| T52 | Organize Tools menu into toolbar groups and favorites (#3) | Done |
| T53 | Delete selected mesh entities and object instances (#4) | Done |
| T54 | Persistent oriented cursor and context-safe guide placement (#8, #9, #14, #15) | In progress |
| T55 | Add a persistent recent project list (#10) | Done |
| T56 | Make Cuboid a two-point primitive (#11) | Done |
| T57 | Close Bevy UI direction; retain GPU viewport (#19, superseded by #20) | Done |
| T58 | egui shell + Bevy viewport prototype (superseded by GPUI + raylib-rs; #20) | Superseded |
| T59 | Prove GPUI + raylib-rs viewport integration (#21, high priority) | In progress |
| T60 | Integrate GPUI and raylib desktop while preserving headless commands (#21, #25, high priority) | In progress |
