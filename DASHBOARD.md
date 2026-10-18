# Meshcraft Progress Dashboard

Updated 2026-10-18 from [`docs/rewrite/tasks.json`](docs/rewrite/tasks.json). The task registry is authoritative; this dashboard presents `planned` as Todo and `complete` as Done.

## Progress

| Status | Tasks |
| --- | ---: |
| Todo | 38 |
| In progress | 6 |
| Done | 7 |
| **Total** | **51** |

## Currently in Progress

- **T05 — Implement sparse legacy DTO reading and model/history containers**
- **T07 — Build desktop window and basic viewport**
- **T09 — Implement desktop document service and safe storage**
- **T10 — Port orbit, orthographic and walkthrough cameras**
- **T11 — Implement snapping, permanent grid and guides**
- **T17 — Complete inspectors, settings, preferences and menu coverage**

## Current Activity

T09 now asks for Save, Discard, or Cancel before dirty Open/New. The active document stays intact until a choice; replacement follows a successful save only when no newer edits remain, and a save failure or newer edit restores the decision prompt. Tests cover cancel, discard, save continuation, and edits during save. `cargo test --workspace` passes: 78 core, 91 desktop library, 14 IO, and 5 CLI tests. T05’s read-only schema-17 round-trip probe passes all 60 pinned fixtures, including the 1,281-placement interface tutorial; one documented zero-basis repair is reported. T06 is complete: a regression clicks the Cuboid Tools-menu item and toolbar button, invokes it through Ctrl+Shift+P and structured dispatch, then compares document, tool, selection, revision, and undo state. T10 tests perspective picking and all six orthographic construction planes across compact, panel-offset, and wide viewport rectangles; a T07 regression covers native DPI scales of 1, 1.5, and 2. T11 snaps Line, Rectangle, and guide placement to transformed geometry, faces, and construction grids with a configured physical-pixel radius. It also snaps to triangle edge midpoints, nearest edge points and grid lines; endpoint ties beat face hits, active edit scopes exclude unrelated instances, and visible faces occlude edges behind them. Hidden edges/faces and wireframe faces follow explicit snap visibility settings. Palette commands and G/T shortcuts place oriented grid and axis guides as session overlays; palette commands and UI settings toggle guide visibility, clear all guides, and remove the nearest visible guide without modifying authored geometry. Hidden guides are excluded from snapping. A focused text field keeps wheel input from zooming the viewport. The permanent grid supports X/Y/Z normals, 1–1000 step extents, and adaptive line density as zoom changes. A temporary local grid cloud appears near the permanent grid’s edge, outside its plane or extent, or when grid guides exist; it follows the snapped surface plane, reanchors within a seven-cell radius, and leaves authored geometry unchanged. C10 world-pointer/GUI-hit consistency and complete menu/click routing remain open; T12 will provide the shared per-type display/lock policy. Autosave and recovery also remain open. The 197-command registry covers all 74 built-in tools, 13 visible toolbar groups, palette and menu actions, plus disabled deferred rendering commands. Selection-first operations, hierarchical in-place definition editing, planar-first construction, attached hotspots, arrays, and staged point-picking remain recorded in the modeling spec for upcoming tool tasks. T07 has validated `--size`, `--windowed`, `--maximized`, `--help`, and `--version` launch options while preserving maximized startup and positional project paths. Task counts remain 38 Todo, 6 In Progress, 7 Done.

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
| T10 | Port orbit, orthographic and walkthrough cameras | In progress |
| T11 | Implement snapping, permanent grid and guides | In progress |
| T12 | Port mixed selection and semantic picking framework | Todo |
| T13 | Implement tool protocol and line editing | Todo |
| T14 | Implement basic planar shape tools | Todo |
| T15 | Port transforms, stretch variants and painting | Todo |
| T16 | Implement reusable objects, placement and explode | Todo |
| T17 | Complete inspectors, settings, preferences and menu coverage | In progress |
| T18 | Implement dimensions, bitmap text and vector glyph text | Todo |
| T19 | Implement paths, offsets, outlines and mesh creation | Todo |
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
| T39 | Complete real-time lighting, shadows and view capture | Todo |
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
