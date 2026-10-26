# Dashboard

**0.40.10 confined desktop startup:** the installed 0.40.8 Snap exposed two
packaging gaps: snapd places the Wayland socket beside its private
`XDG_RUNTIME_DIR`, and the Snap entrypoint bypassed the native launcher that
sets the bundled XKB runtime. Startup now uses the reachable absolute Wayland
socket, while the Linux payload includes XKB configuration and the Snap routes
through the normal launcher and GPU provider wrapper. See [Snap runtime
evidence](docs/rewrite/handoffs/T44.md#confined-desktop-startup--0409).

**0.40.9 Snap Wayland startup fix:** the Snap now requests the Wayland plug and
checks the advertised compositor socket before GPUI selects a backend. If the
socket is unavailable inside confinement and X11 is available, it restarts using
X11. `cargo check --package meshcraft-desktop --bin meshcraft --locked` passes.
See [Snap startup evidence](docs/rewrite/handoffs/T44.md#snap-wayland-startup--0408).

**0.40.7 matched keyboard runtime:** native backtraces on both Linux architectures
identified the remaining crash in the old bundled XKB X11 bridge, loaded together
with the modern host core. Packaging now keeps a private matched fallback pair
and compatible locale data; coherent host pairs remain preferred. The exact CI
binary reproduced the segfault in a minimal Ubuntu 24.04 container and opened
offline after applying the new launcher and runtime layout. Four selection
regression tests pass. See [keyboard runtime evidence](docs/rewrite/handoffs/T44.md#matched-keyboard-runtime--0407).

**0.40.6 native startup diagnosis:** the 0.40.5 Linux release checks still exposed
a CI-specific crash after the display race was fixed. Publication stays blocked.
A dedicated manual workflow captures native backtraces from the existing failed
packages on both Linux architectures without rebuilding the application.

**0.40.5 stable offline release check:** private Xvfb uses `-noreset` so polling
clients cannot reset the display during startup. The full offline AppImage smoke
passed locally with a development fixture, including runtime/sandbox preflights,
painted application contents and missing-driver acknowledgement. This fixture is
not a qualified release artifact; both released Linux architectures remain gated
on actual-AppImage checks. See [display evidence](docs/rewrite/handoffs/T44.md#stable-offline-test-display--0405).

**0.40.4 Linux graphics startup:** virtual X11 displays without DRI3 select the
installed software Vulkan driver before native GPU initialization. Other failed graphics initialization can retry once
with the installed Mesa software Vulkan driver, without altering healthy hardware
startup or explicit driver choices. If no backend is available, a native X11
message explains the requirement instead of a startup panic. Offline release
smokes check the actual AppImage and preserve screenshots and native logs.
See [startup evidence](docs/rewrite/handoffs/T44.md#appimage-offline-startup--0403).

**0.40.2 Snap channel eligibility:** amd64 and arm64 Snap packages now use
`grade: stable`, allowing candidate/stable promotion as well as edge/beta releases.
The sidecar manifest records the grade, and packaging regression tests cover both
architectures. Store upload and channel selection remain explicit actions.

**0.40.1 non-gating stress reports:** completed measurements succeed regardless
of tool outcomes. CI always publishes Markdown/CSV/JSON and a diagnostic ZIP to
the same release. Interrupted, missing or rejected worker data produce explicit
incomplete diagnostic reports, preserving all available observations and logs.
Source/binary validation still governs complete measurement tables; missing
binaries and publication errors remain infrastructure failures.

**0.40.0 performance infrastructure:** operation recipes for all 287 inventory
entries at four resident model weights, plus scalable primitive Select/Move/Rotate
probes. The real desktop runner records input, phantom feedback, commit and fresh
viewport timing. The manual Stress test workflow consumes the selected tag's
verified release binary, runs four isolated workers and publishes complete
Markdown/CSV/JSON release assets, including failures. No binary means failure,
never an implicit build. See the [suite and local results](docs/performance/README.md)
and [T69 qualification](docs/rewrite/handoffs/T69.md). Release-binary CI publication
remains a manual qualification step after Release at the same tag. Phase 3 still
awaits menu assignments; no menu restructuring is included.

**0.39.0 presentation:** product README with eight actual screenshots at stable,
descriptive paths; a generated application icon embedded in the native title bar
and wired into the existing platform packaging; a live-registry inventory of
287 tools/commands with icons and preserved blank menu-review columns.
See the [CSV and review guide](docs/tools/README.md) and
[T68 qualification](docs/rewrite/handoffs/T68.md). Four-tier performance tests
and CSV-driven menu restructuring are the next planned phases, not part of this
presentation release.

**0.38.0 architecture:** Stairs accept contour C and walking path P from objects
or selected lines, with editable height, steps/support and left/right guardrail
guides. Height drags use phantom wireframes. Architectural material pickers now
commit conventional and procedural material IDs through the validated command API.
See the [guide and example](docs/architecture.md) and
[T67 qualification](docs/rewrite/handoffs/T67.md).

**0.37.0 shader editor:** a full-window editor with equal code/preview panes,
three highlighted JSON/GLSL tabs, live GPU preview and independent orbit/pan/zoom.
Draft errors stay local; Save/Cancel and program Undo/Redo are qualified.
See the [guide](docs/procedural-materials.md#custom-programs) and
[T66 evidence](docs/rewrite/handoffs/T66.md).

**0.36.0 foliage and shader colors:** shader material/object color parameters
use the HSL/RGB/swatch picker with opacity. Foliage scatters broad curled leaves
on and beyond source surfaces using GPU instancing, with seed, wind, bounds,
LOD and candidate limits. See the [preset/color guide](docs/procedural-presets.md)
and [T65 validation](docs/rewrite/handoffs/T65.md).

**0.35.0 shader presets:** Water, Brick and Wood join the material library.
Independent procedural Tree prototypes/instances generate branches and foliage
on the GPU, with seeded shape, wind, bounded detail and editable settings.
Bounds support selection and manipulation; copy/paste, Undo/Redo and schema-17
save/reopen retain instance values. See the [preset guide and example](docs/procedural-presets.md)
and [T64 validation](docs/rewrite/handoffs/T64.md).

**0.33.6 selection activation and counts:** architecture handles appear for one
selected entity. Copy/array button availability no longer tessellates selected
solids and shares a revisioned result across buttons. Marquee filtering deduplicates
resolved owners. Marquee containment uses realized facets instead of rebuilding
owners per facet; the user's dense-model probe fell from 42.4 s to 55 ms. Selection
edge/face totals now include generated architecture and
effective nested instance geometry, with overlapping selections counted once.

**0.33.5 selection refresh:** unfocused property inputs no longer start repeating
cursor timers or queue frames on every paint. Architecture properties/material
selectors and world-space controls are cached; control symbols use the batched
viewport overlay. In the local 600-edge/796-face fixture, settled selected idle CPU
fell from about 57% to 1.5%, with no new viewport frames. Native slab/wall dragging,
Escape, Undo/Redo and numeric input focus/commit were checked.

**0.33.3 object handle feedback:** instance hotspot dragging keeps the filled drawing
fixed and evaluates only an object phantom wireframe in the background. Rapid
targets coalesce; the final value commits once on release and Escape cancels.
Unrelated scripts and scene/GPU geometry caches stay unchanged during the drag.

**0.33.2 line display:** UI Settings defaults to black model lines, with an option
to show line material colors. The preference persists independently of drawings;
faces and authored materials retain their colors. Native/browser views and the
script editor preview use the selected style.

**0.33.1 Solids/camera fixes:** Intersection Curves has explicit menu, ribbon and
left-toolbar entries. Difference respects picking order (first minus second),
including ordered MCP selections. Shift-wheel translates eye and target together
along the view axis, including walkthrough height synchronization.

**0.33.0 selection, grid and mesh tools:** auto-grid crosses are larger and grid
points outrank line attraction. Closed-object hotspots appear for one selected
instance. Selection preserves face colors, highlights lines with a blue bounding
box, and skips identical GPU redraws. Modify adds Mirror and Fill Loops; Arrays
hosts Surface Array. Solids groups working Union/Intersection/Difference, mesh
intersection curves and two-point Planar Section with wireframe feedback.

**0.32.0 prototype management:** Delete Prototype is available for used and unused
prototypes. Used definitions require a warning confirmation that removes all their
instances, including placements inside other prototypes; Cancel is non-mutating.
Undo/Redo remains available afterward. New architecture defaults match the requested
1-unit walls/slabs, 9-unit wall heights, 0.5 axis offset and 0.4/0.3 frame section.

**0.31.0 frame ownership and Explode:** windows/doors are built-in Frame entities,
with consistent click/window/crossing selection and architectural properties.
Legacy plain frame wrappers stay compatible but no longer appear as user objects.
Explode puts ordinary mesh in the parent graph in one step; it creates no
replacement prototype. Unused prototypes have a Delete Prototype action, and
stale private layouts no longer block deletion. Allocator checks cover idle
rendering and repeated transform previews.

**0.30.2 architecture transforms:** picked window/door frames use instance ownership
for transform previews and commits, including mixed wall/slab/frame selections.
Move, Move Copy and Rotate Copy accept their final point; array eligibility and
span queries use the same targets. Prototype editing retains shared geometry edits.

**0.30.1 architecture clipboard:** architectural frame picks use their independent
window/door instance for Copy, Paste and Explode. Mixed walls/slabs/frames retain
owned openings and material bindings; Copy as Script includes realized wall/slab
geometry. Explode respects editability and preserves sibling instances.

**0.30.0 drawing materials:** new drawings include 16 named ANSI colors plus
smoke-white, smoke-grey, side-grey and transparent glass. Default lines use
ansi-black and faces ansi-grey. The scrollable Materials manager deletes unused
materials with Undo; prototype, instance, script and architecture references are
protected. Older drawing colors are preserved.

**0.29.0 numeric tool entry:** buffered distances, unit suffixes and relative 2D/3D
coordinates advance normal tool workflows. Copy previews support xN repetition
and /N equal divisions for movement, rotation and scale, with one Undo step.
Backspace edits the buffer; Enter accepts it or finishes a path when empty.
See the [input guide](docs/numeric-tool-input.md).

**0.28.0 IFC starter presets:** Object Instance and Object Prototypes offer compact
Wall/Slab/Door/Window Common property subsets, also discoverable/applicable by MCP.
Only missing fields are added, initially Unset; authored values are preserved,
instance batches have one Undo step, and prototype defaults still copy at creation.
See the [preset fields and API](docs/ifc-properties.md#small-architectural-presets).

**0.27.0 object metadata and materials:** Object Instance and Object Prototypes manage
independent IFC properties using the offline IFC4X3_ADD2 catalog. Prototype creation
defaults are copied into each instance; scripts use `$IFC` and immutable `this.ifc`.
The prototype browser has left-aligned cached isometric thumbnails. Architecture
surfaces and model painting select materials; the color picker provides HSL, RGB
and Swatches with opacity. `$HOTSPOT_SHAPE_*` uses the corrected spelling only.
See the [IFC guide](docs/ifc-properties.md) and [validation](docs/rewrite/handoffs/object-properties-validation.md).

**0.26.0 scripted handles:** generated hotspot targets stay absolute across asynchronous frames,
preventing drag previews from alternating with their declared positions. `$HOTSPOT` accepts
shape constants and CSS hex color, defaulting to a cyan circle; metadata survives frame reuse,
copy and Save/Open. See the [appearance API](docs/script-parameters.md#hotspot-appearance) and
[T62 evidence](docs/rewrite/handoffs/T62.md#0260--stable-scripted-handle-drags-and-appearance).

**0.25.0 script model parameters:** `$TEXT`, `$NUMBER`, `$CHECKBOX`, `$LIST` and `$HOTSPOT`
register prototype or instance parameters according to execution context. Object Properties renders
typed controls, with selection-wide instance edits, shared prototype values, Undo and persistence.
API v2 supplies read-only values to `onAnimationFrame(params, model)`; legacy scripts retain API v1.
Declarations are bold magenta, lead completion and have hover/MCP help. See the
[parameter guide](docs/script-parameters.md) and [T62 validation](docs/rewrite/handoffs/T62.md#0250--script-model-parameters).

**0.24.3 import placement:** insertion feedback no longer clones/places/validates the document and
rebuilds its wireframe on each move. It shares prepared edges and an uploaded GPU mesh, changing
only the placement transform. The 51,200-face probe's 20 warmed CPU preview moves fell from
1.46 seconds to 0.07 ms. Full native mouse placement, Undo, cancellation and 651 tests pass.
See [placement evidence](docs/rewrite/handoffs/interaction-performance-validation.md#import-placement-feedback--0243).

**0.24.2 prototype picking and count layout:** the right panel is 420 px wide, with non-wrapping
seven-digit count columns. Native snapping, cursor alignment, picking and rendering share a cached
prototype edit view. A 51,200-face CPU probe's 20 warmed interaction cycles fell from 809 ms to
9.7 ms; 649 tests and native face-click checks pass. See [performance evidence](docs/rewrite/handoffs/interaction-performance-validation.md#prototype-edit-picking-and-wider-counts--0242).

**0.24.1 camera target:** the native viewport shows a colored 3D cross spanning 2 internal model
units on each axis, centered on the camera target and visible through solids. Native perspective,
translated-target and orthographic checks pass; see [T10 evidence](docs/rewrite/handoffs/T10.md#visible-native-camera-target--0241-2026-10-22).

Updated 2026-10-22. **0.24.0 mesh interchange:** OBJ, STL ASCII/binary, ASCII FBX, glTF/GLB, Collada, ASCII DXF, 3MF, AMF and IFC proxy import/export are implemented with explicit subset/loss reporting. Native File commands and MCP use background jobs, transactional prototype import/placement, frozen scene/selection export, unit conversion, cancellation and protected destination publication. See the [format/workflow guide](docs/interchange.md), [common IO handoff](docs/rewrite/handoffs/T36.md) and [extended/IFC qualification](docs/rewrite/handoffs/T37.md).

**0.23.3 selection properties and chrome:** material rows align left; top menu labels keep a fixed footprint when active and have wider spacing. Typed architecture edits apply to every matching selected owner in one transaction, with mixed-type inspector sections. Hotspot metadata and object names also support batch edits. Re-entering the representative value updates differing owners; invalid batches roll back and unchanged batches add no history. 615 workspace tests, strict Clippy, build and native menu/panel/batch-edit checks pass. See [validation](docs/rewrite/handoffs/T29.md#selection-wide-typed-properties--0233-2026-10-22). **0.23.2 architecture inspector:** Walls, Slabs and Frames have separate property tables with aligned value/measurement columns and shortened labels. Plain architecture inputs replace stepper controls; generic “units” suffixes are removed. Colors, Autojoin and glazing stay with their corresponding subsection. Strict Clippy, build and native validation/edit/Undo checks pass. See [layout and editing evidence](docs/rewrite/handoffs/T29.md#grouped-architecture-tables--0232-2026-10-22). **0.23.1 units and snapping:** native snap radius is measured in pixels at each candidate's depth, including orthographic views, with no grid-size minimum. Wall/slab controls snap to the real ground grid and elevated references; unsnapped motion preserves elevation parallel to the active grid. Converted measurements use internal length × unit size; cm/30/0.5 shows 15 cm per grid step. Architecture length inputs keep internal values and show converted results beside them. 611 workspace tests, strict Clippy, build and isolated native checks pass. See [snapping evidence](docs/rewrite/handoffs/T11.md#pixel-radius-and-ground-control-snapping--0231-2026-10-22) and [unit behavior](docs/rewrite/handoffs/T02.md#displayed-units-without-rescaling-geometry--0231-2026-10-22). **0.23.0 control and transform feedback:** architecture drags preview only the owner's wireframe over the unchanged model and commit once on release. Wall bases and slab points move freely in 3D, with identical Shift movement; wall ends are circles and top-height handles triangles. Squash scales along a picked axis. Rotation keeps center-relative rays, angle feedback and an arc. 605 workspace tests, strict Clippy, build and isolated native checks pass, including control dragging with 200 background cuboids. The local CPU preview probe with 300 background cuboids fell from 690 ms to 2.4 ms for 100 positions (excludes rendering). See [architecture evidence](docs/rewrite/handoffs/T29.md#owner-only-drag-wireframes-and-free-controls--0230-2026-10-22) and [transform evidence](docs/rewrite/handoffs/T15.md#squash-and-center-relative-rotational-feedback--0230-2026-10-22). **0.22.0 recent files and navigation:** File → Recent Files is a persistent, keyboard-accessible submenu with filenames/paths and an empty state. Native Save/Discard/Cancel now resolves unsaved Open/New replacement; New detaches the old save path to prevent overwriting it through autosave. Ctrl/Alt wheel rates are 0.1/0.05 of normal. Axis/grid guides keep the complete cursor alignment, including segment snaps. 600 tests, strict Clippy, build and isolated native menu/dialog/zoom/guide checks pass. See [validation](docs/rewrite/handoffs/T09.md#native-recent-files-submenu-and-safe-replacement--0220-2026-10-21). **0.21.0 roof/control corrections:** wall controls stay on the axis plane; frame input follows the displayed blue cursor vector. Slabs retain independent profile heights with Shift-drag elevation, vertical sides, thickness-preserving sloped roofs and owned openings across folds. Architecture wireframes omit internal tessellation. Ctrl/Alt wheel rates are 0.5/0.1 of normal. 597 regular tests, strict Clippy, native build and isolated GPUI drag/zoom/save/reopen checks pass. See [roof validation](docs/rewrite/handoffs/T29.md#roof-profiles-and-cursorcontrol-corrections--0210-2026-10-21). **0.20.0 architecture controls:** optional wall Autojoin, axis offset, independent base/top endpoints, hole/frame corner resizing, cursor-normal windows and direct architecture explosion are implemented. 589 workspace tests, strict Clippy, native build and isolated pointer/settings/reopen checks pass. Legacy wall axes and roof-hole plane/depth conversions are corrected; the copied six-wall user model qualifies without source changes. See [T29 evidence](docs/rewrite/handoffs/T29.md) and the [controls guide](docs/architecture.md#wall-opening-and-frame-controls). **0.18.0 architecture:** wall chains, rectangular/freeform closed-polyline slabs, owned openings, independent window/door objects, editable properties and slab profile dragging are implemented. Copy/move/clipboard/explode preserve canonical ownership. 575 workspace tests, strict Clippy and native build pass; see [T29 evidence](docs/rewrite/handoffs/T29.md) and [T30 frame scope](docs/rewrite/handoffs/T30.md). **0.6.0 materials:** shared RGBA material catalog, undoable creation/editing/assignment, GPUI editor and color rendering are implemented; texture sampling remains future work. See [T61 evidence](docs/rewrite/handoffs/T61.md). **0.7.0 scripted prototypes:** `onCreate` / `onAnimationFrame`, per-instance state, generated hotspots, primitive helpers and a single monospace GPUI source editor with Ctrl+Space API help/completion and offscreen preview are implemented. 505 regular Rust tests, strict Clippy, build and packaging checks pass locally. Native runtime/platform qualification remains open; see [T62](docs/rewrite/handoffs/T62.md). The 0.5.3 release matrix has now passed all six native targets and published its artifacts.  **0.5.3 release follow-up:** CI investigation fixes root-owned Linux packaging output and raylib static CRT selection on Windows. Validation/builds run in parallel with four workers, a lighter validation profile and dependency caches. The 0.5.2 matrix passes Linux installers and both macOS targets; a subsequent Windows ShowCursor collision comes from unused winit. Eframe/winit/wgpu are now browser-only; native tests (495), strict Clippy, build and browser compile checks pass locally. The corrected tagged matrix is pending; see [T44 evidence](docs/rewrite/handoffs/T44.md#native-dependency-reduction-and-second-windows-failure--053). **0.5.0:** The manual six-target release pipeline is prepared, selecting an existing tag without a redundant tag input. Linux builds use pinned Ubuntu 18.04 and audit glibc ≤2.27; 22 primary artifacts cover portable ZIP/tar.gz, AppImage/Snap, MSI/unsigned MSIX and DMG. 495 Rust tests, eight packaging tests, build, strict Clippy and workflow lint pass. Actual Linux archives/AppImage/Snap built, and the AppImage GUI/pan smoke check passed. The full native CI matrix, installed-platform checks and signing remain open. See [release validation](docs/rewrite/handoffs/T44.md) and [manual instructions](packaging/README.md). **0.4.1:** Connected selection uses positional adjacency at `1e-2` without welding the model; both pan gestures run at 50% of the prior multiplier. 495 regular workspace tests, build and strict Clippy pass. [Follow-up validation](docs/rewrite/handoffs/interaction-performance-validation.md#follow-up-tolerance-and-pan-tuning--041). **0.4.0 interaction fixes:** geometry previews use thick model-line strokes; GPUI motion restores Push/Pull and specialized tool feedback. Move Copy/Rotate Copy are enabled for mesh fragments and preserve their sources. Ordinary fragment transforms detach shared boundaries; Stretch remains connected. Middle-button drag pans. Isolated circles insert in one batch and line repair preserves untouched topology. Selection rendering caches membership and omits redundant interior edges/per-vertex object markers. On the local software GPU, 5,183 selected faces/edges rendered in 23.83 ms versus 23.22 ms unselected at 1280×800; the local circle probe fell from 60.59 s to 248 ms, plus 50 ms for snapping refresh. These are bounded probes, not broad hardware qualification. **494 regular workspace tests pass** (228 core, 246 desktop, 15 IO, 5 CLI), plus explicit GPU and circle probes; build, strict Clippy, formatting and plan checks pass. See [captures and exact validation](docs/rewrite/handoffs/interaction-performance-validation.md). The [selection controls](docs/rewrite/handoffs/selection-controls-validation.md) and user-tested [object/hotspot workflow with 19 captures](docs/rewrite/spec/objects.md) remain in place. Wider domain/platform/performance qualification stays with the existing tasks. `docs/rewrite/tasks.json` is authoritative.

**Counts:** Todo 19 · In progress 20 · Done 23 · Superseded 1 · Total 63

## Current in-progress tasks

- **T05 — Implement sparse legacy DTO reading and model/history containers**: Finish sparse legacy DTO/model-history compatibility.
- **T09 — Implement desktop document service and safe storage**: Complete safe desktop project storage/recovery.
- **T16 — Implement reusable objects, placement and explode**: Finish reusable object editing and explode behavior.
- **T17 — Complete GPUI inspectors, settings and preferences (#26)**: Editable model/UI controls, grid normal, normal/thick model line widths, persisted settings accordion and shader AA are wired. Toolbars remain expanded. Buttons keep text beside 419 transparent SVG icons. Ely toolkit investigation (#26) is complete; see [the audit and migration assessment](docs/rewrite/ely-gpui-review.md). Broader inspector/domain acceptance remains open; see [T17](docs/rewrite/handoffs/T17.md).
- **T21 — Port every copy and array state machine**: Complete the remaining array state machines and adapters.
- **T22 — Port hole/cutout algorithms and robust replacement**: Finish robust cutout/hole replacement cases.
- **T29 — Implement architecture walls, slabs and holes**: Wall/slab/opening tools, joins, offsets, independent end heights, settings and profile/corner controls are implemented; wider legacy edge/center-handle acceptance remains open.
- **T30 — Implement stairs and free-form frames**: Independent window/door objects and properties are implemented; stairs remain planned.
- **T32 — Implement hotspot definitions, IDs and attachment graph**: Porting legacy authoring/property/attachment workflow into the shared model and GPUI panels.
- **T33 — Implement dynamic hotspot instance evaluation**: Porting immutable-source evaluation, instance values, handle drag/preview and copy semantics.
- **T34 — Implement external prototype references**: Integrating external scenes as reusable subscenes, with root hotspots and refresh/error handling.
- **T39 — Complete real-time lighting, shadows and view capture (#13)**: Implemented the raylib light-camera shadow pass, directional/ambient face shading, black authored lines, three-plane guides/local point field, grid-line snapping, reversed orbit and staged Escape. The extended transparent SVG atlas generates at build time. Tight light-camera clip bounds fix detached shadows (analytic ground projection IoU 0.937). GPU checks pass for ground/entity receivers, black model line widths and all three shader AA qualities; 233 regular desktop tests pass. The full GPUI shell builds/runs with generated icons. Lighting controls are session-local; capture, legacy lighting persistence and broader T39 quality acceptance remain open. See [T39](docs/rewrite/handoffs/T39.md).
- **T44 — Qualify desktop performance, UI coverage and releases**: Manual six-target native release pipeline and 22 primary archives/installers are prepared. Tag/app-version/SHA matching, glibc auditing, checksums and complete-matrix verification gate publication. MSIX signing and native platform GUI qualification remain open; see [T44](docs/rewrite/handoffs/T44.md).
- **T51 — Restore and verify geometric feedback in the raylib viewport (#2, #21)**: Relative point/vector feedback now includes array stages, revolve and Paint; full interactive tool coverage remains open.
- **T54 — Persistent oriented cursor and context-safe guide placement (#8, #9, #14, #15)**: Carry cursor orientation and context-safe guide placement through the new viewport.
- **T59 — Prove GPUI + raylib-rs viewport integration (#21, high priority)**: The raylib 6 offscreen proof renders at 1280×800 and presents frames through a regular GPUI image component, so GPUI overlays remain above the canvas. Release runs sustain 59–60 new images/sec; worker frame work is typically 4–12 ms, with occasional 18 ms samples. This meets the requested 25 FPS floor and 30–60 FPS target for the simple proof scene. The production gate remains open for live viewport input, resize/DPI behavior, and the dense-array workflow.
- **T60 — Integrate GPUI and raylib desktop while preserving headless commands (#21, #25, high priority)**: Main now calls the actual `0.2.0` shell layout functions in `gpui_layout.rs`: original palette grouping/button sizes, labeled two-row ribbon, selection/object tables, paired model fields and all domain settings panels, matched against `docs/full_ui.png`. Supported controls bind to existing commands; unfinished controls remain disabled. The event-driven offscreen raylib image and model/session helpers are retained. Fixed zero session viewport bounds that ignored construction clicks, unified pointer/render layout dimensions, and kept menu/palette overlays out of normal layout with measured dropdown anchors. Live X11 checks confirm ribbon switching, settings scrolling, menu layering, two-point Line creation with feedback/count updates, and Undo outside the viewport. The compact searchable palette and dropdown menus are left-aligned; tool feedback text is a GPUI overlay. Retired GPU frame images are explicitly evicted (120-update baseline grew 241 MiB; patched 600-update run grew under 2 MiB). Palette scrolling consumes input without zooming the canvas, and Delete routes to selection deletion outside text editors. The latest run passes 235 regular desktop tests plus the explicit GPU regression, the application build, formatting and the plan verifier. Screenshots and exact commands are in [the T60 handoff](docs/rewrite/handoffs/T60.md). Physical high-DPI, dense-array acceptance and remaining egui/eframe extraction stay open.

## Architecture change

The accepted desktop architecture is GPUI for the shell and raylib-rs for the 3D viewport. T58/issue #20 are superseded; high-priority issue #21 tracks the replacement. T59's release proof presents raylib `RenderTexture2D` frames as normal GPUI images at 59–60 updates/sec at 1280×800, fixing menu/viewport stacking in the proof shell. T60 now starts the production app through GPUI, publishes the live document/session to the offscreen raylib renderer, and routes viewport selection, picking, snapping, tools, guides and previews through existing APIs. The worker renders on scene, camera or viewport changes; a lightweight frame watcher refreshes GPUI when a frame arrives, avoiding continuous idle redraws. All catalog commands are reachable in their menus, ribbon groups, tool palettes or the full command palette. UI preferences now update the shared session and renderer grid/workplane. The retired Bevy host and dependencies are removed. Remaining egui/eframe coupling is still under extraction; manual visual/input, resize/DPI and dense-array acceptance remain open. Keep command IDs, JSON command/settings definitions, session operations, model and IO behavior.

The local recordings `docs/Screencast From 2026-10-19 10-42-51.mp4` and `docs/Screencast From 2026-10-19 10-47-07.mp4` are intentionally Git-ignored. C41 records the dense object-array, orbit/zoom/selection and responsiveness workflow; the legacy freeze is a regression case.

## All task statuses

| ID  | Task                                                                                           | Status      |
|-----|------------------------------------------------------------------------------------------------|-------------|
| T00 | Capture legacy behavior, fixtures and measurement baseline                                     | Done        |
| T01 | Prove the Rust rendering, menu and geometry stack                                              | Done        |
| T02 | Implement numerical geometry and coordinate contracts                                          | Done        |
| T03 | Build canonical document stores, IDs and revisioned derived data                               | Done        |
| T04 | Implement transactions and bounded undo                                                        | Done        |
| T05 | Implement sparse legacy DTO reading and model/history containers                               | In progress |
| T06 | Create command registry and host-effect dispatch                                               | Done        |
| T07 | Build GPUI desktop window and raylib viewport with normalized input ownership                  | Todo        |
| T08 | Implement GPUI desktop menus, grouped ribbon, and context command menu (#16, #17)              | Todo        |
| T09 | Implement desktop document service and safe storage                                            | In progress |
| T10 | Port orbit, orthographic and walkthrough cameras                                               | Done        |
| T11 | Implement snapping, permanent grid and guides                                                  | Done        |
| T12 | Port mixed selection and semantic picking framework                                            | Done        |
| T13 | Implement tool protocol and line editing                                                       | Done        |
| T14 | Implement basic planar shape tools                                                             | Done        |
| T15 | Port transforms, stretch variants and painting                                                 | Done        |
| T16 | Implement reusable objects, placement and explode                                              | In progress |
| T17 | Complete GPUI inspectors, JSON-driven settings, preferences and command coverage (#26)         | In progress |
| T18 | Implement dimensions, bitmap text and vector glyph text                                        | Todo        |
| T19 | Implement paths, offsets, outlines and mesh creation                                           | Done        |
| T20 | Implement region push/pull                                                                     | Done        |
| T21 | Port every copy and array state machine                                                        | In progress |
| T22 | Port hole/cutout algorithms and robust replacement                                             | In progress |
| T23 | Port planar/spatial sweeps, ribbon and revolve                                                 | Done        |
| T24 | Port plane section, plane cuts and mesh intersection                                           | Todo        |
| T25 | Implement primitive mesh generators                                                            | Done        |
| T26 | Implement built-in mechanical tools                                                            | Todo        |
| T27 | Implement object cutting and booleans                                                          | Todo        |
| T28 | Implement voxel domain and tools                                                               | Todo        |
| T29 | Implement architecture walls, slabs and holes                                                  | In progress |
| T30 | Implement stairs and free-form frames                                                          | In progress |
| T31 | Implement HVAC plumbing and ventilation                                                        | Todo        |
| T32 | Implement hotspot definitions, IDs and attachment graph                                        | In progress |
| T33 | Implement dynamic hotspot instance evaluation                                                  | In progress |
| T34 | Implement external prototype references                                                        | In progress |
| T35 | Implement random offset, surface scatter and both remesh modes                                 | Todo        |
| T36 | Implement OBJ/STL/DXF interchange and unit policy                                              | Todo        |
| T37 | Implement extended mesh formats                                                                | Todo        |
| T38 | Implement IFC4 proxy export and supported geometry import                                      | Todo        |
| T39 | Complete real-time lighting, shadows and view capture; remove mesh triangulation seams (#13)   | In progress |
| T40 | Port progressive CPU offline rendering                                                         | Todo        |
| T41 | Implement structured automation and regression dispatcher                                      | Todo        |
| T42 | Audit phase-specific capabilities and preserve migration boundaries                            | Todo        |
| T43 | Implement desktop tutorial recorder/player and schema adapters                                 | Todo        |
| T44 | Qualify desktop performance, complete UI coverage and package releases                         | In progress |
| T45 | Deliver browser runtime, component and docs integration                                        | Todo        |
| T46 | Ratify the normalized object model and migration contracts                                     | Done        |
| T47 | Prove JavaScriptCore embedding, scheduling and native packaging                                | Todo        |
| T48 | Implement JavaScriptCore runtime lifecycle and context facade                                  | Todo        |
| T49 | Connect the versioned JavaScript plugin SDK to normalized host services                        | Todo        |
| T50 | Deliver JavaScriptCore plugin UX, samples and qualified desktop packages                       | Todo        |
| T51 | Restore and verify geometric feedback in the raylib viewport (#2, #21)                         | In progress |
| T52 | Organize Tools menu into toolbar groups and favorites (#3)                                     | Done        |
| T53 | Delete selected mesh entities and object instances (#4)                                        | Done        |
| T54 | Persistent oriented cursor and context-safe guide placement (#8, #9, #14, #15)                 | In progress |
| T55 | Add a persistent recent project list (#10)                                                     | Done        |
| T56 | Make Cuboid a two-point primitive (#11)                                                        | Done        |
| T57 | Close Bevy UI direction; retain GPU viewport (#19, superseded by #20)                          | Done        |
| T58 | egui shell + Bevy viewport prototype (superseded by GPUI + raylib-rs; #20)                     | Superseded  |
| T59 | Prove GPUI + raylib-rs viewport integration (#21, high priority)                               | In progress |
| T60 | Integrate GPUI and raylib desktop while preserving headless commands (#21, #25, high priority) | In progress |

| T61 | Implement material catalog and color rendering | Done |
| T62 | Implement scripted prototype hooks and preview IDE | In progress |
