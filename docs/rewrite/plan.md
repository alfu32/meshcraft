# Implementation and agent handoff plan

`tasks.json` is the authoritative task registry: 59 tasks, 207 assignable slices, explicit dependencies, target modules, source read sets, requirements, acceptance suites and completion conditions. T00–T04, T06, T10–T15, T19, T20, T25, T46, T51–T53, T55–T57 are closed; T57 is an administrative scope closure, not a claim that its Bevy UI acceptance passed. T05, T07–T09, T16, T17, T21, T22, T39, T54 remain in progress; T58 is in progress at high priority. The Bevy GPU renderer and custom model viewport remain; the application shell is moving from Bevy UI to egui integrated through `bevy_egui` (#20). The workspace has a normalized document with cuboid/copy/object/array operations, schema-17 project read/write, a desktop save/open service, a perspective orbit camera with schema-17 camera-state persistence, six orthographic views, and a walkthrough camera with movement, support, jump/gravity, and runtime tuning. These are partial task slices: most modeling domains, recovery, and complete legacy DTO coverage remain open. Task slices are deliberately smaller than a whole feature family, especially arrays, formats, domains and rendering. Existing task IDs are stable; T46 is an early gate despite its appended number.

## Delivery scope

The first usable desktop slice is followed by full initial desktop modeling scope. Both include conventional desktop menus alongside the button workspace. Android, plugin execution and the developer CLI/TUI/REPL remain outside that first delivery. The final product explicitly includes JavaScriptCore plugins receiving a versioned application context, implemented in required later T47–T50. Groovy/JVM execution is not restored. Design context boundaries now without adding the engine to initial desktop.

Structured command/pointer/status automation remains in scope so tests and agents can drive the app without the deferred developer console. The regular executable's file-opening/size/version options remain useful launch behavior. Browser/custom-element parity is a separate delivery after desktop qualification; early WASM compile experiments protect architecture without gating desktop on a shipping browser UI.

T46 uses [object-model.md](object-model.md) and its measured fixture evidence to ratify canonical ownership/identity/layouts before T02/T03/T05/T16 contracts harden. It targets no-op per-instance geometry copies, cloned shared child trees, redundant coordinate storage and session/cache fields mixed with authored content. Preserve legitimate definition/placement semantics and differing overrides. The plugin API uses those stable identities and effective views, not legacy mutable stores.

## Milestones and gates

| Milestone | Tasks / ready work | Demonstration required |
| --- | --- | --- |
| M0: evidence and normalized contracts | T00/T01, then T46 before T02–T06 | Fixture/oracle and stack proof; owner/identity/layout/migration decision; f64 math, normalized document/IDs, undo, sparse reader, shared commands |
| M1: working desktop shell | T07–T12, T08 menus, T09 storage | Open/display/save a fixture, navigate, snap, select, choose profile and drive commands with correct focus |
| M2: everyday direct modeling | T13–T20, T36, T39, T41 as dependencies allow | Draw/mesh/annotate/extrude/transform/paint/group, all relevant panels, common interchange, reliable menu/button/automation parity |
| M3: advanced modeling | T21–T27 | Every array variant, cutting, sections/intersections, sweeps/revolve, primitives, mechanical and object booleans |
| M4: editable domains and dynamic objects | T28–T34 | Voxel/architecture/HVAC parameters and handles, hotspot graphs/runtime overrides, xref bundle refresh/reopen |
| M5: surface/IO/render completeness | T35, T37–T40 | Fuzzy/remesh tools, complete supported interchange subset, real-time/capture fidelity and progressive offline rendering |
| M6: initial desktop qualification | T42–T44 | Deferred-feature honesty, tutorials, every built-in menu mapping, full acceptance/performance report and three-OS packaged smoke checks |
| M7: browser delivery | T45 | Standalone WASM runtime, component API/events, browser files/storage, docs/tutorial integration and browser-specific checks |
| M8: native JavaScriptCore plugins | T47–T50 | Engine/binding/platform proofs, activation context/SDK, lifecycle/contributions/manager UI, C33 sample/package qualification |

Milestones are demonstrable capability groups, not strict numeric task order. Dependency readiness controls execution. T25 primitive generators are complete after T14; T36 common IO follows T16; T41 structured automation follows T12/T09 and should run before advanced tools. T39 lighting/capture can run alongside advanced modeling. M6 must complete all initial desktop requirements, not just the minimal slice.

M7 and M8 are independent required successors to M6; neither blocks the initial desktop gate or waits for the other. Native JSC plugins do not automatically imply browser plugin execution. T46's early gate uses schema/data probes; full C32 editor/resource assertions mature in core/object/runtime tasks and desktop qualification, so the design gate does not depend on implementing its own downstream editor.

Do not assign calendar estimates until T00 measurements and T01 stack experiments are available. Use completion gates and task throughput to forecast; large files and compatibility unknowns make guessed dates misleading.

## Suggested parallel lanes

| Lane | Main ownership | Parallelism constraints |
| --- | --- | --- |
| Core/geometry | T46 then T02–T04, T13–T16, T19–T27 | Ratify normalized ownership first; one owner freezes contracts; geometry agents use bounded source slices/modules |
| IO/storage | T05/T09, T34, T36–T38 | Codec work can proceed after canonical DTO contract; document publication/storage has one owner |
| UI/input | T07/T08/T10–T12/T17, feature panels | Shared action-tree/focus/session owner; feature agents add registrations rather than editing a giant UI file |
| Domains | T28–T33/T35 | Independent generators after object/path/selection contracts; coordinate shared instance/attachment invalidation |
| Render | T07/T39/T40 | Snapshots and revision semantics frozen with core; shading work does not mutate documents |
| Automation/docs/qualification | T00/T41–T44 | Starts scenario/fixture work early; validates every feature incrementally; no live-Groovy dependency |
| Browser (later) | T45 | Uses established host services; native code paths are capability-gated |
| Native plugins (later) | T47–T50 | JSC runtime owner and stable context/SDK over normalized host services; parallel with browser after desktop |

For a small team, combine lanes without changing dependency contracts. Parallelize files/modules with clear ownership; never ask several agents to redesign Document, CommandDescriptor or ToolEvent independently. Shared interfaces receive small coordinator-reviewed patches; downstream workers consume them. Do not create branches/worktrees or commits automatically without the user's instruction.

## Task index

| Task | Output |
| --- | --- |
| T00 | Hashed legacy fixtures, scenarios, recorded baseline and historical-shape evidence |
| T01 | Chosen/locked stack and proven menu/polygon/native/WASM contracts |
| T02 | Numerical geometry, planes/predicates/transforms and tolerances |
| T03 | Canonical stores, IDs, prototype/instance state and cache revisions |
| T04 | Transactions and bounded undo |
| T05 | All sparse DTOs, model/history reader/writer and unknown-field preservation |
| T06 | Commands, action metadata, host effects and completion model |
| T07 | Desktop window, basic GPU viewport and input ownership |
| T08 | Desktop Menus/Buttons/Hybrid presentations and menu focus behavior |
| T09 | File commands/dialogs, staged load, serialized safe saves and recovery |
| T10 | Orbit, orthographic and walkthrough cameras |
| T11 | Snapping, grid and oriented guides |
| T12 | Mixed selection, window/crossing/volume and semantic handle contracts |
| T13 | Common tool protocol, numeric/operators and line tools |
| T14 | Rectangle, surface rectangle, parallelogram quad and circle |
| T15 | Move/rotate/scale/stretch variants and paint |
| T16 | Reusable object editing, placement, glue, nesting and explode |
| T17 | Inspectors/settings/preferences/status and current menu coverage |
| T18 | Dimensions, bitmap/vector text and glyph catalogs |
| T19 | Polyline/double-line, mesh, outline and offset tools |
| T20 | Region push/pull |
| T21 | Ten distinct copy/rotation/array state machines |
| T22 | Both hole variants and Cutout |
| T23 | Planar/spatial sweep, ribbon and revolve |
| T24 | Plane section/cut and mesh intersection |
| T25 | Four primitive generators |
| T26 | Four built-in mechanical tools |
| T27 | Object cut, union, intersection and subtraction |
| T28 | Voxel domain, tools, voxelization and explode |
| T29 | Wall/slab/hole parameters, geometry and controls |
| T30 | Stairs and free-form window/door frames |
| T31 | Plumbing/ventilation paths, joins/humps and controls |
| T32 | Hotspot definitions, attachment graph, placement and panel |
| T33 | Independent per-instance hotspot runtime geometry/overrides |
| T34 | External references and safe bundle refresh/save |
| T35 | Random offset/scatter and both remesh modes |
| T36 | Common OBJ/STL/DXF IO and physical-unit policy |
| T37 | FBX/glTF/GLB/DAE/3MF/AMF supported subsets |
| T38 | IFC4 proxy export and supported geometry import |
| T39 | Interactive lighting/shadows and SVG/PNG captures |
| T40 | Progressive CPU raytrace/pathtrace |
| T41 | Retained structured HTTP/MCP and deterministic regression commands |
| T42 | Deferred-capability honesty/data-preservation audit, no deferred feature implementation |
| T43 | Desktop tutorial recorder/player and separate schema adapters |
| T44 | Full desktop/performance/menu/packaging qualification |
| T45 | Later browser runtime/component/docs integration |
| T46 | Early object-model normalization/identity/layout/migration gate before core contracts harden |
| T47 | JavaScriptCore binding, native packaging and runtime scheduling/GC/interruption proofs |
| T48 | JSC lifecycle, handed-in context facade and runtime ownership |
| T49 | Versioned query/command/transaction/tool/UI/state/interchange SDK |
| T50 | Plugin manager, sample plugins, docs and three-OS native plugin qualification |
| T51 | Geometric feedback for enabled tools (#2) |
| T52 | Organize Tools menu into toolbar groups and favorites (#3) |
| T53 | Delete selected mesh entities and object instances (#4) |
| T54 | Persistent oriented cursor and context-safe guide placement (#8, #9) |
| T55 | Persistent recent project list, File menu and palette access (#10) |
| T56 | Two-opposite-corner Cuboid primitive in Primitives (#11) |

## How a small-context agent takes over

1. Coordinator picks a dependency-ready task and one numbered slice. Run `python3 docs/rewrite/scripts/handoff.py T21 --slice 2` (example). It prints the exact requirement/scenario text, source read set, dependencies, contracts and done criteria. `--ready` uses task statuses; `--list` shows the registry.
2. Read prerequisite completion capsules, exported Rust interfaces and the task's `design_docs` read set. Use `object-model.md`/T46 for canonical ownership and `plugins.md` for later ctx/runtime work. Read `architecture.md` only for a changed cross-cutting contract. Avoid loading the full specification or inventory JSON into a prompt.
3. Locate source symbols using `python3 docs/rewrite/scripts/handoff.py --find CopyMultipleTool --path tools/`. `--tool HELICAL_ARRAY` prints its class, state-field names, keys, parsing locations and source-message hints; these are navigation aids, not a proven state sequence. Read the named handlers/helpers with bounded `sed`/`rg` excerpts. For DTO work, `--dto ObjectPrototypeDto` prints fields/defaults; `--catalog tools|commands|mcp|fixtures` prints only that surface.
4. Write a tiny state/input/output table for ambiguous tool semantics, supported payloads, coordinate space, numeric meaning and transaction boundaries. Use current source first, then a named historical diff/regression if ambiguity remains. Record the answer once in the capsule.
5. Implement only the slice and its meaningful scenario assertions. Every new operation registers menus/buttons/palette/automation metadata, enabled/checked rules, result selection, cancellation and undo behavior where applicable. Test the relevant core/codec/UI cases as needed by the change.
6. Handoff in `docs/rewrite/handoffs/Txx.md`: completed slice numbers, exported APIs, changed paths, checks with results, deliberate differences, blockers and next ready slice. Target at most 250 words; link large logs/fixtures instead of pasting them. Update `tasks.json.status` only after all slices and completion conditions pass.

Task statuses: `planned`, `in_progress`, `complete`, `blocked`. A missing dependency is “not ready,” not an implementation blocker. A blocked slice records a concrete unmet prerequisite and independent work continues. Completion capsules can have per-slice status until the whole task qualifies. Future agents must not mark a task complete merely because enums/widgets compile.

## Validation sequence

Each task runs its named Cxx suites starting with deterministic core/IO tests. Add GUI automation for actual focus/menu/viewport behavior, renderer readback for visuals and packaged OS smoke tests for platform behavior. The new test layout should group scenarios by Cxx ID so a task can run one suite economically. Once Cargo exists, task capsules must record exact commands/results, not generic “tests pass.”

At each milestone, run `python3 docs/rewrite/scripts/verify_plan.py` for handoff consistency, current feature suites, supported legacy fixture subsets, command/menu coverage and a demonstrable user workflow. M6 runs the entire initial desktop matrix including C32 and measured performance; M7 runs browser checks; M8 runs C33 plugin/context/package qualification. Static plan validation does not validate Rust behavior or prove JSC is packaged.

## Risk and resolution owners

| Risk | Earliest resolution | Consequence if ignored |
| --- | --- | --- |
| Sparse defaults/zero axes and historical shape ambiguity | T00/T05 | Loads appear successful but models collapse or metadata disappears |
| Polygon clipping/holes/JTS replacement | T01/T02 | Cutting/domain tools each need incompatible ad-hoc fixes |
| New menus change focus and viewport mapping | T01/T08 | Wrong picks, leaked shortcuts, duplicate commits and broken captures |
| Prototype/instance/local axes and override identity | T03/T16/T32 | Shared instances corrupt each other; hotspots/xrefs cannot round-trip |
| No-op overrides/copied child trees become canonical Rust state | T46 before T02/T03/T05 | Repeated geometry/hierarchy survives the rewrite and leaks into the public plugin API |
| Undo snapshots and async save ordering | T04/T09 | Memory blow-up, stale overwrite or accidental data loss |
| Interchange support inferred from extension | T36/T37 | Broad advertised support with silent geometry/unit loss |
| Semantic IFC assumed from architecture tools | T38 | Invented parity: current exporter uses building-element proxies |
| Booleans on invalid/open/coplanar inputs | T24/T27 | Misleading CAD robustness claims or partial destructive results |
| Hotspot graph/runtime behavior | T32/T33 | Cascades repeat transformations or invalidate wrong instances |
| Premature native-thread assumptions | T01/T40/T45 | Browser needs a scheduling rewrite |
| Deferred features accidentally re-enter scope | T06/T42/T44 | JVM/plugin/TUI work delays the requested desktop delivery |
| JSC binding availability mistaken for distributable platform/runtime support | T47 | Later native plugins fail to package, manage GC/async lifetime or stop blocking callbacks |
| Context mirrors mutable model internals or lacks occurrence/revision identity | Early T46 boundary, then T48/T49 | Plugins freeze redundant structures, edit the wrong instance or apply stale results |

## Future backlog, outside first-phase tasks

JavaScriptCore native plugins are required later T47–T50, not part of this optional backlog. Still separate: Android shell/touch/SAF/lifecycle/store packaging; any Groovy/JVM compatibility bridge or developer REPL/CLI/TUI; browser plugin adapter/custom plugin entity schemas; native self-updates; optional OpenSCAD; richer semantic IFC; automatic face inference/cutting during ordinary drawing; tags/layers/material textures/UV/saved scenes and a dedicated eraser if desired. These are not promises that current source fully implements them. Preserve opaque data and source evidence for later decisions.
