# Meshcraft Progress Dashboard

Updated 2026-10-19 from [`docs/rewrite/tasks.json`](docs/rewrite/tasks.json). The task registry is authoritative; this dashboard presents `planned` as Todo and `complete` as Done.

## Progress

| Status      |  Tasks |
|-------------|-------:|
| Todo        |     26 |
| In progress |      8 |
| Done        |     20 |
| **Total**   | **54** |

## Currently in Progress

- **T05 — Implement sparse legacy DTO reading and model/history containers**
- **T07 — Build desktop window and basic viewport**
- **T09 — Implement desktop document service and safe storage**
- **T16 — Implement reusable objects, placement and explode**
- **T17 — Complete inspectors, settings, preferences and menu coverage**
- **T21 — Port every copy and array state machine**
- **T22 — Port hole/cutout algorithms and robust replacement**
- **T39 — Complete real-time lighting, shadows and view capture**

## Current Activity

T05 remains in progress. Sparse names now follow the pinned Kotlin load behavior: a blank `root` prototype retains `Root`, while blank non-root prototypes and converted legacy groups use `Object`. Current-schema and rootGroup regressions pass; all 60 indexed fixtures, including the local `octodraw.octd`, round-trip canonically. The full 32-DTO audit and Kotlin reopen comparisons remain open.

T09 remains in progress. Named documents autosave 1.5 seconds after the latest edit, while untitled documents remain unsaved until an explicit Save or Save As. Repeated edits reset the deadline, and edits during an in-flight save schedule a follow-up. Recovery backups and Kotlin reopen checks remain open.

T16 remains in progress. Selection grouping now moves selected legacy dimensions and text with mesh into a shared hybrid child definition, preserving DTO payloads and leaving unselected annotations on the source definition. Imported domain arrays receive internal semantic IDs because dimension/text DTOs have no persisted ID; IDs remain out of schema-17 export. Move and Copy now transform definition-owned dimension/text selections in world space, allocate IDs for copies, and select copied handles; malformed and unsupported semantic selections fail atomically. Desktop explode preserves/transforms dimensions and bitmap text, converts embedded-alphabet vector text to connected line geometry, and selects the generated entities. Grouping or transforming inherited annotations from a single placed instance is rejected until occurrence override semantics exist; custom glyph sources and remaining C08/C32 lifecycle coverage remain open.

T10 is complete: orbit and restored perspective camera poses clamp at ±89°. T12 is complete: real viewport press/move/release tests apply left-to-right containment and right-to-left crossing selection, confirmed by the user. T13–T15 deliver lines, planar shapes, transforms and RGBA paint. T19 is complete: staged Polyline/Double Line, Mesh, selected Polyline Mesh, Face Outline and selected-edge Line Offset are wired to command actions, previews, active-graph commits and coverage. T22 implements distinct CUT_HOLES, CUT_HOLES_2 and CUT_OUT_3 commands over selected faces and edges. Closed loops partition/remove regions, and CUT_HOLES_2 partitions along finite boundary-to-boundary polylines with interior bends; Cutout retains its legacy infinite-plane behavior. Colors, source-to-output face remaps, active-definition and instance-local commits, selection and undo are covered. Legacy-fixture comparison remains open. T23 is complete: planar and spatial Extrude Swipe, rotation-minimizing 3D frames, Ribbon 3D, and selected-profile Revolve now have palette commands, snapped point stages, geometry previews, active-graph commits, material, result selection and undo. Revolve supports typed signed partial angles and default full turns, with welded full-turn seams; closed profiles pass manifoldness and faceted-volume checks. Open sweep surfaces and partial Revolve turns remain uncapped. The localhost MCP was unavailable for runtime comparison; see the [T23 handoff](docs/rewrite/handoffs/T23.md). T20 is complete: selected faces extrude with signed snapped-point or numeric input, colored geometry preview, unit-aware distance/ΔX/ΔY/ΔZ feedback, correctly wound manifold output, active-definition insertion, selected generated results, cancellation and undo. C13/C19 coverage verifies multi-face color retention, holes, both extrusion signs, zero/invalid-input atomicity, manifold edge incidence and prism volume. The general feedback rule from [issue #1](https://github.com/alfu32/meshcraft/issues/1) is now in U05/C34. T51 is complete for [bug #2](https://github.com/alfu32/meshcraft/issues/2): every enabled geometry tool now has stage feedback and preview/commit coverage; Paint and parameterized Array previews are included, with selection the sole exception.

T39 is in progress: the viewport distinguishes darker, slightly bluer back faces and uses a bounded CPU directional depth map for shadows on the ground and other entity faces, including transformed instances. Lighting controls, filtering/quality settings and inclusion-aware view capture remain open. T25 is complete: Sphere, Cylinder, Cone and Pill now have staged point picking, oriented closed mesh generation, shared previews, active-definition commits, selected results, undo and Escape cancellation. The legacy tools expose no separate numeric-entry stage; radius, height and orientation come from snapped picks, with `circle_segments` clamped to 3–256. T52 is complete for [bug #3](https://github.com/alfu32/meshcraft/issues/3): built-in tools now live under their source-backed toolbar groups, with common tools and Copy Selection directly available at the Tools menu root. Issue [#4](https://github.com/alfu32/meshcraft/issues/4) is complete: Delete Selection handles viewport-picked faces, edges, vertices and object instances through one undoable operation, with instance isolation, keyboard routing and atomic invalid-selection handling. User requirements for planar-first construction, nested object definitions, attached hotspots, distinct array families and staged point-picking remain mapped to their owning tasks. T21 is in progress: numeric linear array accepts selected mesh entities, root and nested object occurrences, and mixed selections. Nested copies fork the selected parent’s child layout and remap affected occurrence paths; repeated arrays preserve their source and earlier results across successive forks, and parent edit contexts retain descendant selections. Copy Multiple uses snapped U/V/D picks and places fixed-vector copies only at positive steps fitting the projected span. Planar and volumetric translation arrays fill signed XZ or XYZ grids. Planar Rotate Multiple, Helicoidal Rotate Multiple, and Rotational Array use signed fixed-angle transforms, full-circle duplicate suppression, axial lift for helicoidal copies, previews, the 2,048-copy commit cap, and selection of generated results. Helical Array now uses the legacy world-Y axis and fixed angle/lift measured from its A/B points, with staged preview and committed copies. Rotate 2 is also implemented as its own C/A/B three-point rotation, with an axis derived from those vectors and optional single-copy mode. Both equalized rotational/helical arrays now derive full-turn spacing from selected world-space span while retaining the angular hint for partial sweeps; helical full turns preserve pitch. Annotation/domain adapters and broader command coverage remain open.

Validation: `cargo test --workspace --quiet` passes (212 core, 193 desktop, 15 IO and 5 CLI tests); Clippy, the wasm desktop lib check, formatting, diff whitespace, and `verify_plan.py` also pass. T16 tests cover mixed mesh/dimension/text grouping, payload preservation, shared-definition visibility, dimension/text Move and Copy, copied-handle selection, atomic rejection of inherited instance-level transforms, safe rejection of mesh-only explode, transformed annotation preservation/selection during desktop explode, embedded vector glyph conversion, and atomic rejection of unavailable/unsupported domains. T21 covers mesh-face, mixed-target, nested sibling, and mixed nested object/face array copies across parent-layout forks; projected-span Copy Multiple; signed planar/volumetric grids; fixed-step rotational/helicoidal/Helical Array transforms; distinct Rotate 2 three-point copy/transform; selection-span equalized arrays and helical pitch; snapped staged picks; per-transform preview; source preservation; generated-result selection; point undo; Escape cancellation; and invalid-input atomicity. T23 has 13 core sweep/revolve tests and six desktop workflow tests for planar/spatial profiles, Ribbon settings, previews, typed angles, active-instance coordinates, selection and undo. T25 coverage includes closed oriented manifold volume, negative axis direction, invalid dimensions, resolution clamps, four command previews/commits, active-instance local coordinates, selection, undo and Escape. T51 coverage includes staged geometry previews and measurements, Paint targets, Array parameter updates and committed-copy parity, snapping, numeric input, entered definitions and cancellation. T52 coverage checks every legacy tool's toolbar menu path, common direct aliases, disabled capabilities, submenu activation and command metadata compatibility. Push/Pull coverage verifies signed extrusion geometry, colors, holes, manifoldness/volume, active-definition insertion, selected results and undo. Camera coverage includes orbit/restored poses at ±89°. Selection coverage exercises actual viewport release in both directions; Line Offset tests cover preview, active-definition commits, winding and invalid-input atomicity. T22 also covers bent finite open paths, chord/dangling distinctions, nested/concave loops, translation-independent tolerance, prototype isolation for instance-local replacement geometry, derived index consistency, color, selection and undo. Delete Selection coverage includes actual viewport-picked face deletion through command and Delete key, menu/toolbar/palette registration, nested instance deletion, shared-instance isolation, vertex cascade, undo/redo and atomic rejection of unsupported handles. Current task counts: 26 Todo, 8 In Progress, 20 Done.

## Task Statuses

| ID  | Task                                                                     | Status      |
|-----|--------------------------------------------------------------------------|-------------|
| T00 | Capture legacy behavior, fixtures and measurement baseline               | Done        |
| T01 | Prove the Rust rendering, menu and geometry stack                        | Done        |
| T02 | Implement numerical geometry and coordinate contracts                    | Done        |
| T03 | Build canonical document stores, IDs and revisioned derived data         | Done        |
| T04 | Implement transactions and bounded undo                                  | Done        |
| T05 | Implement sparse legacy DTO reading and model/history containers         | In progress |
| T06 | Create command registry and host-effect dispatch                         | Done        |
| T07 | Build desktop window and basic viewport                                  | In progress |
| T08 | Implement shared desktop menus, button and hybrid workspaces             | Todo        |
| T09 | Implement desktop document service and safe storage                      | In progress |
| T10 | Port orbit, orthographic and walkthrough cameras                         | Done        |
| T11 | Implement snapping, permanent grid and guides                            | Done        |
| T12 | Port mixed selection and semantic picking framework                      | Done        |
| T13 | Implement tool protocol and line editing                                 | Done        |
| T14 | Implement basic planar shape tools                                       | Done        |
| T15 | Port transforms, stretch variants and painting                           | Done        |
| T16 | Implement reusable objects, placement and explode                        | In progress |
| T17 | Complete inspectors, settings, preferences and menu coverage             | In progress |
| T18 | Implement dimensions, bitmap text and vector glyph text                  | Todo        |
| T19 | Implement paths, offsets, outlines and mesh creation                     | Done        |
| T20 | Implement region push/pull                                               | Done        |
| T21 | Port every copy and array state machine                                  | In progress |
| T22 | Port hole/cutout algorithms and robust replacement                       | In progress |
| T23 | Port planar/spatial sweeps, ribbon and revolve                           | Done        |
| T24 | Port plane section, plane cuts and mesh intersection                     | Todo        |
| T25 | Implement primitive mesh generators                                      | Done        |
| T26 | Implement built-in mechanical tools                                      | Todo        |
| T27 | Implement object cutting and booleans                                    | Todo        |
| T28 | Implement voxel domain and tools                                         | Todo        |
| T29 | Implement architecture walls, slabs and holes                            | Todo        |
| T30 | Implement stairs and free-form frames                                    | Todo        |
| T31 | Implement HVAC plumbing and ventilation                                  | Todo        |
| T32 | Implement hotspot definitions, IDs and attachment graph                  | Todo        |
| T33 | Implement dynamic hotspot instance evaluation                            | Todo        |
| T34 | Implement external prototype references                                  | Todo        |
| T35 | Implement random offset, surface scatter and both remesh modes           | Todo        |
| T36 | Implement OBJ/STL/DXF interchange and unit policy                        | Todo        |
| T37 | Implement extended mesh formats                                          | Todo        |
| T38 | Implement IFC4 proxy export and supported geometry import                | Todo        |
| T39 | Complete real-time lighting, shadows and view capture                    | In progress |
| T40 | Port progressive CPU offline rendering                                   | Todo        |
| T41 | Implement structured automation and regression dispatcher                | Todo        |
| T42 | Audit phase-specific capabilities and preserve migration boundaries      | Todo        |
| T43 | Implement desktop tutorial recorder/player and schema adapters           | Todo        |
| T44 | Qualify desktop performance, complete UI coverage and package releases   | Todo        |
| T45 | Deliver browser runtime, component and docs integration                  | Todo        |
| T46 | Ratify the normalized object model and migration contracts               | Done        |
| T47 | Prove JavaScriptCore embedding, scheduling and native packaging          | Todo        |
| T48 | Implement JavaScriptCore runtime lifecycle and context facade            | Todo        |
| T49 | Connect the versioned JavaScript plugin SDK to normalized host services  | Todo        |
| T50 | Deliver JavaScriptCore plugin UX, samples and qualified desktop packages | Todo        |
| T51 | Fix geometric feedback for enabled tools (#2)                           | Done        |
| T52 | Organize Tools menu into toolbar groups and favorites (#3)               | Done        |
| T53 | Delete selected mesh entities and object instances (#4)                 | Done        |
