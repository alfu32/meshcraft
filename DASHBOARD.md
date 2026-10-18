# Meshcraft Progress Dashboard

Updated 2026-10-18 from [`docs/rewrite/tasks.json`](docs/rewrite/tasks.json). The task registry is authoritative; this dashboard presents `planned` as Todo and `complete` as Done.

## Progress

| Status | Tasks |
| --- | ---: |
| Todo | 39 |
| In progress | 6 |
| Done | 6 |
| **Total** | **51** |

## Currently in Progress

- **T05 — Implement sparse legacy DTO reading and model/history containers**
- **T06 — Create command registry and host-effect dispatch**
- **T07 — Build desktop window and basic viewport**
- **T09 — Implement desktop document service and safe storage**
- **T10 — Port orbit, orthographic and walkthrough cameras**
- **T17 — Complete inspectors, settings, preferences and menu coverage**

## Current Activity

T09 now asks for Save, Discard, or Cancel before dirty Open/New. The active document stays intact until a choice; replacement follows a successful save only when no newer edits remain, and a save failure or newer edit restores the decision prompt. Tests cover cancel, discard, save continuation, and edits during save. `cargo test --workspace` passes: 78 core, 76 desktop, 14 IO, and 5 CLI tests. T05’s read-only schema-17 round-trip probe passes all 60 pinned fixtures, including the 1,281-placement interface tutorial; one documented zero-basis repair is reported. T06 has a keyboard-palette regression comparing Cuboid tool state and undo behavior with registered menu/toolbar IDs and structured dispatch; actual widget-click equivalence remains open. T10 now tests perspective picking and all six orthographic construction planes across compact, panel-offset, and wide viewport rectangles. Autosave and recovery remain open. The 194-command registry covers all 74 built-in tools, 13 visible toolbar groups, palette and menu actions, plus disabled deferred rendering commands. Selection-first operations, hierarchical in-place definition editing, planar-first construction, attached hotspots, arrays, and staged point-picking remain recorded in the modeling spec for upcoming tool tasks. T07 has validated `--size`, `--windowed`, `--maximized`, `--help`, and `--version` launch options while preserving maximized startup and positional project paths. Task counts remain 39 Todo, 6 In progress, 6 Done.

## Task Statuses

| ID | Task | Status |
| --- | --- | --- |
| T00 | Capture legacy behavior, fixtures and measurement baseline | Done |
| T01 | Prove the Rust rendering, menu and geometry stack | Done |
| T02 | Implement numerical geometry and coordinate contracts | Done |
| T03 | Build canonical document stores, IDs and revisioned derived data | Done |
| T04 | Implement transactions and bounded undo | Done |
| T05 | Implement sparse legacy DTO reading and model/history containers | In progress |
| T06 | Create command registry and host-effect dispatch | In progress |
| T07 | Build desktop window and basic viewport | In progress |
| T08 | Implement shared desktop menus, button and hybrid workspaces | Todo |
| T09 | Implement desktop document service and safe storage | In progress |
| T10 | Port orbit, orthographic and walkthrough cameras | In progress |
| T11 | Implement snapping, permanent grid and guides | Todo |
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
