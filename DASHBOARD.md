# Dashboard

Updated 2026-10-19. `docs/rewrite/tasks.json` is authoritative.

**Counts:** Todo 30 · In progress 8 · Done 22 · Superseded 1 · Total 61

## Current in-progress tasks

- **T05 — Implement sparse legacy DTO reading and model/history containers**: Finish sparse legacy DTO/model-history compatibility.
- **T09 — Implement desktop document service and safe storage**: Complete safe desktop project storage/recovery.
- **T16 — Implement reusable objects, placement and explode**: Finish reusable object editing and explode behavior.
- **T21 — Port every copy and array state machine**: Complete the remaining array state machines and adapters.
- **T22 — Port hole/cutout algorithms and robust replacement**: Finish robust cutout/hole replacement cases.
- **T51 — Restore and verify geometric feedback in the raylib viewport (#2, #21)**: Revalidate geometric preview/text feedback for the raylib viewport.
- **T54 — Persistent oriented cursor and context-safe guide placement (#8, #9, #14, #15)**: Carry cursor orientation and context-safe guide placement through the new viewport.
- **T59 — Prove GPUI + raylib-rs viewport integration (#21, high priority)**: Native hosting and shutdown work; populated text menus and ribbon tabs now navigate while command buttons remain disabled. The CAD shell and target-centered orbit pass unit checks, including the vertical drag fix (#22). Live-review the new navigation and child bounds, then finish scene sync and choose a supported production embedding path.

## Architecture change

The accepted desktop architecture is GPUI for the shell and raylib-rs for the 3D viewport. T58/issue #20 are superseded; high-priority issue #21 tracks the replacement. T59 proves live native-surface integration without per-frame CPU framebuffer readback. T60 removes Bevy/egui code after replacement behavior is covered. Keep the command registry, JSON command/settings definitions, headless session operations, model and IO behavior.

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
| T17 | Complete GPUI inspectors, JSON-driven settings, preferences and command coverage | Todo |
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
| T32 | Implement hotspot definitions, IDs and attachment graph | Todo |
| T33 | Implement dynamic hotspot instance evaluation | Todo |
| T34 | Implement external prototype references | Todo |
| T35 | Implement random offset, surface scatter and both remesh modes | Todo |
| T36 | Implement OBJ/STL/DXF interchange and unit policy | Todo |
| T37 | Implement extended mesh formats | Todo |
| T38 | Implement IFC4 proxy export and supported geometry import | Todo |
| T39 | Complete real-time lighting, shadows and view capture; remove mesh triangulation seams (#13) | Todo |
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
| T60 | Remove retired Bevy/egui desktop code while preserving headless commands (#21, high priority) | Todo |
