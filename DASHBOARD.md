# Meshcraft Progress Dashboard

Updated 2026-10-18 from [`docs/rewrite/tasks.json`](docs/rewrite/tasks.json). The task registry is authoritative; this dashboard presents `planned` as Todo and `complete` as Done.

## Progress

| Status | Tasks |
| --- | ---: |
| Todo | 31 |
| In progress | 7 |
| Done | 13 |
| **Total** | **51** |

## Currently in Progress

- **T05 — Implement sparse legacy DTO reading and model/history containers**
- **T07 — Build desktop window and basic viewport**
- **T09 — Implement desktop document service and safe storage**
- **T16 — Implement reusable objects, placement and explode**
- **T17 — Complete inspectors, settings, preferences and menu coverage**
- **T19 — Implement paths, offsets, outlines and mesh creation**
- **T39 — Complete real-time lighting, shadows and view capture**

## Current Activity

T12 is complete: typed selection supports object/mesh targets, directional windows, occurrence editing, connected-line double-click, coplanar-face double-click, connected-component triple-click, and Escape cancellation/guide clearing. T13–T15 deliver lines, planar shapes, transforms and RGBA paint. T19 is in progress: Polyline supports line/arc paths and closed concave meshes; Double Line builds offset rails and a ribbon. Face Outline operates on selected triangles and emits only the selected region boundary. Polyline Mesh turns selected connected segments into concave faces without adding edges. General shared boundary ordering, Mesh and Line Offset remain open.

T39 is in progress: the viewport distinguishes darker, slightly bluer back faces and uses a bounded CPU directional depth map for shadows on the ground and other entity faces, including transformed instances. Lighting controls, filtering/quality settings and inclusion-aware view capture remain open. User requirements for planar-first construction, nested object definitions, attached hotspots, distinct array families and staged point-picking remain mapped to their owning tasks.

Validation: `cargo test --workspace --quiet` passes 134 core, 142 desktop library, 14 IO and 5 CLI tests. Clippy with warnings denied, formatting, `git diff --check` and `verify_plan.py` pass. Path tests cover Double Line joins/topology, Face Outline boundaries, and Polyline Mesh chain filling and face-only insertion. Rendering tests cover front/back tint, alpha preservation, ground and entity occlusion, out-of-map receivers and self-shadow bias. The registry contains 216 commands for all 74 built-in tools and 13 visible toolbar groups. Current task counts: 31 Todo, 7 In Progress, 13 Done.

## Task Statuses

| ID | Task | Status |
| --- | --- | --- |
| T00 | Capture legacy behavior, fixtures and measurement baseline | Done |
| T01 | Prove the Rust rendering, menu and geometry stack | Done |
| T02 | Implement numerical geometry and coordinate contracts | Done |
| T03 | Build canonical document stores, IDs and revisioned derived data | Done |
| T04 | Implement transactions and bounded undo | Done |
| T05 | Implement sparse legacy DTO reading and model/history containers | In progress |
| T06 | Create command registry and host-effect dispatch | Done |
| T07 | Build desktop window and basic viewport | In progress |
| T08 | Implement shared desktop menus, button and hybrid workspaces | Todo |
| T09 | Implement desktop document service and safe storage | In progress |
| T10 | Port orbit, orthographic and walkthrough cameras | Done |
| T11 | Implement snapping, permanent grid and guides | Done |
| T12 | Port mixed selection and semantic picking framework | Done |
| T13 | Implement tool protocol and line editing | Done |
| T14 | Implement basic planar shape tools | Done |
| T15 | Port transforms, stretch variants and painting | Done |
| T16 | Implement reusable objects, placement and explode | In progress |
| T17 | Complete inspectors, settings, preferences and menu coverage | In progress |
| T18 | Implement dimensions, bitmap text and vector glyph text | Todo |
| T19 | Implement paths, offsets, outlines and mesh creation | In progress |
| T20 | Implement region push/pull | Todo |
| T21 | Port every copy and array state machine | Todo |
| T22 | Port hole/cutout algorithms and robust replacement | Todo |
| T23 | Port planar/spatial sweeps, ribbon and revolve | Todo |
| T24 | Port plane section, plane cuts and mesh intersection | Todo |
| T25 | Implement primitive mesh generators | Todo |
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
| T39 | Complete real-time lighting, shadows and view capture | In progress |
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
