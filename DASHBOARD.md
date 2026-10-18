# Meshcraft Progress Dashboard

Updated 2026-10-18 from [`docs/rewrite/tasks.json`](docs/rewrite/tasks.json). The task registry is authoritative; this dashboard presents `planned` as Todo and `complete` as Done.

## Progress

| Status | Tasks |
| --- | ---: |
| Todo | 36 |
| In progress | 5 |
| Done | 10 |
| **Total** | **51** |

## Currently in Progress

- **T05 — Implement sparse legacy DTO reading and model/history containers**
- **T07 — Build desktop window and basic viewport**
- **T09 — Implement desktop document service and safe storage**
- **T13 — Implement tool protocol and line editing**
- **T17 — Complete inspectors, settings, preferences and menu coverage**

## Current Activity

T12 is complete: mixed typed selection supports replace/add/remove, injected 350 ms multi-click timing, coplanar and connected expansion, instance edit entry, directional screen windows, Alt-forced windows, two-corner world volumes, and shared visibility/lock/wireframe policy. Selection and semantic-handle picking respect transformed instances and occlusion; construction in an entered instance edits its definition in local space. T13 now supports chained Line and paired Construction Line drawing directly in the active root or definition graph, with local-space insertion, same-context intersection splitting, collinear consolidation, selection-ID remapping across shared instances, numeric preview/commit, and one undo transaction per committed segment. Escape clears pending input while preserving committed segments; ordinary line edits do not cut faces. Current validation passes: 92 core, 107 desktop, 14 IO and 5 CLI tests, Clippy with warnings denied, formatting, and `verify_plan.py`. T13 remains In Progress while generic copy/operator dispatch and the hotspot-operation hook are resolved. The 200-command registry covers all 74 built-in tools, 13 visible toolbar groups, palette and menu actions, and deferred commands. User-confirmed requirements for selection-first modeling, planar-first construction, nested object definitions, attached hotspots, distinct linear/planar/spatial and rotational/helical arrays, and staged point-picking remain recorded for T14 and later tasks. T09’s dirty Open/New decisions, T05’s schema-17 compatibility probe, T06 command-surface checks, T10 camera workflows and T11 snapping/grid/guides remain covered by their completed handoffs. T07 has validated launch sizing and maximized startup. Autosave/recovery and the remaining UI, modeling, domain, rendering and browser tasks remain open. Current task counts: 36 Todo, 5 In Progress, 10 Done.

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
| T13 | Implement tool protocol and line editing | In progress |
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
