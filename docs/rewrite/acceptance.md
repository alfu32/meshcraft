# Acceptance scenarios

These are executable-test specifications for implementation agents, not claims that tests already exist. T00 creates fixtures, a canonical comparison format and baseline records; feature tasks implement the applicable assertions. Prefer deterministic headless core tests, codec fixtures and dispatcher tests, with GUI tests only for actual focus/layout/render behavior.

Canonical comparison normalizes entity ordering and numeric tolerances but retains IDs, colors, prototype/instance references, domain parameters and hotspot attachments. For topology-changing algorithms compare boundaries, area/volume, winding and manifoldness where applicable; arbitrary triangle order is not a correctness condition. A screenshot alone cannot prove editable semantic parity.

## C01 — Coordinate/precision/unit contracts

Round-trip local→world→local for translated, rotated, nonuniformly scaled and nested instances; verify normals and winding. Test tiny geometry, large offsets, negative coordinates and unit size/name changes without implicit geometry rescaling. Record the old display/export scale conversion with numeric examples. Reject singular/nonfinite transforms safely.

## C02 — Segment/face editing topology

Draw crossing segments, endpoint-on-edge, collinear overlap/touch and near misses; ordinary Line on a face does not silently split its triangles or infer a face from a closed loop. Separately invoke explicit face-cut operations for a line wholly inside a triangle, a concave boundary and a hole. Assert no accidental cross-context cuts, expected edge union/face area, correct colors/winding and preserved geometry on failed polygon operations. Raw insertion must not run editing operations.

## C03 — Sparse current-document round trip

Decode all 60 indexed fixture files read-only, grouped into small deterministic subsets and larger opt-in runs. Compare canonical state before/after Rust save and Kotlin reopen, including `{}` vectors, missing version/kind/alpha, frame glazing default, IDs, glyph models, domains and overrides. Preserve unknown nested data. Use staged feature readiness for semantic regeneration; all DTO payloads survive from T05 onward.

## C04 — Historical and malformed documents

Retrieve representative flat/rootGroup/prototype-only documents from pinned history without changing the source checkout. Record each supported historical shape, writer/default assumptions and fixture hash. Exercise missing prototype references, duplicate IDs, singular root, truncated gzip, malformed JSON and unknown keys; failed load preserves the live document. Do not advertise arbitrary versions 1..17 without fixtures.

## C05 — Save/load/cancel/recovery

Save below/above 10 MiB, both magic-detected containers, Save As, autosave burst, overlapping save requests and closing with pending work. Inject failures before/after temporary write and replacement; last good file survives. Cancel staged decode/application and verify current document remains intact. Test default path and no accidental overwrite on New/Open.

## C06 — Undo/redo and old history

One completed operation→one undo; preview/cancel→none; redo truncates after branch; memory cap behaves predictably. Decode base64/gzip legacy entries lazily, honor index/labels, preserve a corrupt entry diagnostically without discarding a good model. Check camera/settings policy and document timing-coalescing divergence. Reopen a model with history and continue editing.

## C07 — Object definitions and instances

Group a known mesh, place two instances, edit the prototype, and verify both update. Move/scale one instance, nest it, exit/re-enter edit context, glue-place on a tilted face and explode. Verify local-space geometry, reusable definition count, expected selected result and independent transforms. Drawing through a closed group from outside cannot cut it.

Copy architectural surface picks for standalone windows/doors alongside walls and
slabs. Exercise Paste in Place and floating Paste with an insertion pick, then
Explode and Undo. Repeat after save/reopen and inside a user prototype containing a frame. Preserve
owned holes, material references, independent instance state, sibling placements
and active edit context. Copy as Script includes realized walls/slabs/frames.
Reject read-only explode without mutation. For mixed architectural surface picks,
accept Move's final point at +9 vertical units, Move Copy and Rotate Copy; compare
preview/committed world geometry and unchanged source/sibling instances. Verify
one-step Undo/Redo, numeric repeated copies and array-span queries, both in new
and reopened drawings. Inside user prototype editing, move its frame geometry while
retaining placement transforms. Compare individual picks with both native marquee
directions for new Frame entities and compatible legacy frame placements; Copy,
Move Copy and Rotate Copy must accept the same targets. New frames add no implicit
object/prototype; double-click stays in the architectural parameter workflow.
Explode emits loose mesh into the parent graph without a replacement prototype;
a second Explode on that mesh fails without mutation. Check that idle rendering
and repeated previews do not advance the persistent ID allocator. Delete unused
prototypes directly from the browser, including obsolete exploded wrappers.
For used prototypes, the same button warns and offers Cancel/Delete Prototype and
Instances. Cancel/Escape changes nothing; confirmation removes every placement,
including nested, private-layout and unplaced-parent references, without deleting
unrelated child definitions. Test after 100 later edits, one-step Undo/Redo,
Save/Open and deletion while editing an affected instance. Reject stale warnings.
Unowned stale layouts must not prevent deletion.

## C08 — Mixed payloads and lifecycle

Group/copy/explode selected edges/faces/dimensions/text and supported domain content; verify no double transform at shared vertices. Delete a prototype in use according to defined policy. Preserve color and override data through cloning and document reload. Invalid operations leave the prior state intact.

## C09 — Camera and viewport mapping

Orbit/pan/zoom, all six orthographic views and walkthrough movement/support/jump tuning. Project/unproject/pick the same known point at multiple window sizes, DPI scales and menu/panel layouts. Orthographic picks must not perform a depth-proportional perspective search. Save/load camera and retain a coherent target.

## C10 — Snapping and event routing

Exercise endpoints/midpoints/segments/faces/grid/grid lines/guide planes/guide axes with configured pixel radius, transformed groups, hidden geometry and perspective/ortho cameras. Test ties near corners and no-hit virtual plane. While a text field/menu owns input, wheel/keys/clicks cannot also act on the camera/tool. A world pointer request and equivalent GUI hit must resolve consistently.

Verify that a grid point within the pixel radius wins over a closer segment;
outside the radius the segment remains available. Check enlarged auto-grid crosses
in perspective and orthographic views.

## C11 — Selection semantics

Click/add/remove, double-click coplanar face, triple-click connected geometry, double-click group editing; timed clicks use an injected clock. Test left→right window, right→left crossing, Alt-forced window, and two-corner world volume separately. Validate per-type visibility/lock rules and counts for mixed entities and transformed instances.

## C12 — Semantic handles and display policy

Pick/drag wall endpoints, wall/slab hole corner/edge/center handles, HVAC controls, voxels and hotspot markers with occluding geometry and wireframe settings. Verify hidden/locked entity policy is consistent across click/window/volume and snapping. Deleting a hole edits semantic parameters, not an arbitrary generated triangle.

Closed-object hotspots are visible/hittable for exactly one selected instance,
hidden for zero/multiple instances, and remain available while its hotspot is
dragged. Authored root/active prototype handles remain visible. Selection retains
face fill colors, highlights lines only, displays a blue bounding box and does not
advance the GPU frame counter during idle or identical scene publications.

## C13 — Core modeling slice

Create a rectangle on ground and on a tilted face, a three-pick parallelogram quad and a nondefault-resolution circle. Chain Line versus one Construction Line. Push/pull a prism by numeric signed distance; move/rotate/axis-scale/copy and paint RGBA. Cover Enter/Escape/Backspace/numeric popup, invalid and zero inputs, selection/result policy and undo grouping.

## C14 — Paths, meshes, annotations and glyphs

Polyline line/arc/circle transitions, backtrack, open/closed paths, double-line joins/offsets, mesh/Polyline Mesh/Face Outline/Line Offset prerequisites. Add dimension/bitmap text/vector text, edit content/size/spacing/plane/source, transform/copy/save/reload. Unknown glyph/source errors are visible. Outline omits interior triangulation edges.

## C15 — Transform/array/hotspot behavior

For every array/Rotate2/stretch variant capture ordered source states and test numeric counts/angles/height/equalized endpoints, mixed payloads and local axes. A preview is derived from original inputs, not accumulated. Build two instances of a hotspot prototype with different states, chained and missing/cyclic attachments, all eight operations and five shapes; verify reference picks, attach/select actions, copy/explode/undo/reopen and unaffected sibling instances.

## C16 — Xref bundles

Load current xref examples with companions, a moved/relative bundle, missing reference, cycle and edited linked prototype. Refresh without losing instance overrides or duplicating geometry. Saving an unresolved link never writes an empty external file. Keep source bundle hashes and explicit resolution bases.

## C17 — Everyday desktop UI

Menu and button access for open/save/import/export, every tool family, undo/delete/group, camera/grid, panels and settings. Switch profiles mid-draft, resize/reopen, reset offscreen layouts, edit text/numeric fields and inspect status. Test DPI, keyboard focus, visible enabled/checked state, errors and load/save progress. No plugin-manager/console workflow is required initially.

For C17/C23 native GPUI layout visibility, toggle the menu bar, each dropdown,
ribbon and left panel in UI Settings. Confirm hidden items disappear, canvas space
expands, picking remains aligned at multiple DPIs and the right-side UI Settings
and Ctrl+Shift+P can restore everything. Restart to verify persistence, including
individual dropdown choices while the menu bar is hidden. Document/undo state
must remain unchanged, and older preferences default to the full layout.

## C18 — Shared command semantics

Invoke the same operation via menu, toolbar, palette and structured automation; compare canonical document state, active tool, selection, dirty revision and undo count. Verify command discovery/preconditions/parameters, wrong-context diagnostics, pending versus complete jobs and absence of unsupported deferred commands. Tool activation alone does not generate undo.

## C19 — Generative and cutting tools

Reference solids for primitives, multi-face push/pull, all three hole/cutout IDs, plane cuts/sections, selected mesh intersections, planar/spatial sweeps, ribbon and full revolve. Validate caps/seams/color/area/volume and manifoldness on valid closed inputs; preserve nonmanifold editability elsewhere. Mechanical tool cases include required source profiles, all pick stages and explicit dimensional interpretations.

## C20 — Boolean restrictions and operands

Two overlapping sibling mesh instances: cut-all, union, intersection, A−B and B−A with distinct colors. Check selected loose result in parent and source removal only on successful commit. Add disjoint/tangent/coincident/open/inconsistent-normal cases, wrong parent count and voxel rejection. Characterize legacy outcomes first; improvements require recorded divergences.

Exercise the Solids menu with overlapping, contained, disjoint and identical closed
operands. Verify result volume, outward winding, material references, unchanged
unselected objects and one-step Undo/Redo. Section a selected solid by two picked
points, including coplanar/tangent cases; previews cannot advance the revision.
Mirror a fragment/instance about an oblique plane, then Undo. Fill unordered directed
lines with crossings, holes, gaps and open tails; reversed/disjoint rings retain
their own surface directions and open-only input rejects without mutation.

## C21 — Randomized operations/remesh

Fixed-seed offset/scatter tests assert shared-vertex movement, strength bounds, surface containment, payload count, normal alignment and fuzz ranges. Planar grid remesh and surface remesh cover distinct inputs, invalid tolerance, holes/edges and fidelity. Record statistical/geometric expectations; do not compare unseeded legacy output coordinate-for-coordinate.

## C22 — Domain generation

Voxel cell/adjacency/fill/frame/explode at negative and rotated coordinates. Walls/slabs with holes and inclination/joins; stairs with contours/walking lines/rails; free-form window/door frames/glazing. Plumbing elbows and ventilation joins/humps/controls. Edit parameters then save/reload and regenerate exactly once, preserving authored loose geometry and per-element selection/handles.

For T67, supply C/P as object instances and as connected selections. Test two-stage
picking, preselection, separate selection capture, direction reversal, invalid
branches/disconnected/nonplanar/crossing inputs and Escape without mutation. Verify
straight, L/U and sampled curved stairs, closed oriented triangles, exact total
height (default 10), guide offsets at 5 × spacing 2, independent sides and zero
levels. Check numeric edits and the single height control: phantom-only drag,
one release commit, Escape cancellation. Copy/transform/paste/explode and reopen
retain one semantic stair and its settings; Explode leaves ordinary geometry.
Architecture selectors must commit procedural IDs for every material slot and
for creation defaults, including through the same validated command API as MCP.

## C23 — Desktop menu accessibility and parity

Every shipping action has a primary menu path. Test F10/Alt or platform equivalent, arrows, Enter/Escape, shortcuts, disabled and checked states, focus restoration, high-DPI labels, accessible names and platform conventions. Open a menu during numeric draft entry: no leaked tool-cycle letter, Delete or point commit. Switching profile preserves model/session/draft; viewport picking/capture offsets remain correct. Test native menu adapter separately if selected.

## C24 — Interactive renderer/capture

Fixed-camera reference scenes: front/back face color and winding, including equal orientation/color across coplanar triangles joined by shared vertex IDs or coincident positions; RGBA/transparent overlap, selected/hover/guide overlays, transformed instances, ground/grid, and shadow-map casters on the ground and other entities. Cover shadow acne/bias/PCF/dither toggles and varying scale. Use deterministic model-only GPU readback with documented tolerance; inspect readability separately. SVG projected geometry and PNG dimensions/capture completion must match declared inclusion rules.

## C25 — Offline renderer

Small deterministic ray/path scenes, progressive tile coverage, worker counts, cancellation/restart, stale revisions, resize and PNG export. Verify UI remains interactive and rendering does not mutate model/history. Compare geometric/shading invariants and bounded image error across stochastic seeds rather than exact pixels. Browser uses a cooperative smoke case later.

## C26 — Performance baseline and regression

Record hardware/OS/GPU/build and fixture hashes/sizes/counts. Measure cold/warm raw load and save, perspective/orthographic pick percentiles, simple edit latency, static/interaction frame time, selection/copy preview, regeneration and undo/history memory. Include large `docs/tutorial/000_interface.octd` (about 49.8 MB compressed) in an opt-in performance set. Verify revision/cancellation correctness alongside timing; targets are goals until measured.

T69 tool suite extension: derive coverage from the live inventory; exercise every
registered tool/command and explicit deferred entry at small/medium/high/heavy
weights, plus scaled primitive-fragment Select/Move/Rotate variants. Verify live
fixture counts and hashes, complete operation inputs/previews/commits and relevant
state/file outcomes. Capture client stage times, MCP owner-thread time and raylib
frame work separately, with precise measurement limits. Native rendering uses an
isolated real desktop, not a replacement modeling implementation.

Run locally and in a dedicated manually selected-tag stress workflow that never
builds the app. Fail preflight if the published binary is absent; verify archive,
package-file hashes, version, architecture and source before executing it. Merge
four isolated tier reports only when binary/source/coverage agree. Publish full
Markdown/CSV/JSON reports, summaries and failure diagnostics as same-release
assets. Keep failed operations and unsupported entries distinct; measurement
outcomes never fail a completed benchmark run. Always publish detailed canonical
reports and a diagnostic ZIP. Incomplete/mismatched workers produce explicitly
incomplete diagnostic reports with unvalidated raw observations, never complete
measurement tables. Missing recipes and filtered coverage cannot be presented as
complete testing. Missing binaries/setup/upload failures remain infrastructure
errors. See [method and limits](../performance/README.md).

## C39 — GPU renderer and custom model viewport (#18, #21)

Run the native desktop app with the raylib-rs GPU renderer inside the GPUI viewport. Batch scene geometry rather than issuing one draw per entity. Verify front/back face colors and winding, shadow maps from solid geometry onto the ground and other entities, and that authored lines, construction grid, selection and preview overlays do not cast shadows. Exercise transformed/nested instances, cursor/guides, staged geometry/text feedback, selected highlights, both marquee directions, geometry/grid snapping, click deselection, connected/coplanar multi-click selection and viewport resize/DPI mapping. Confirm render snapshots follow document revisions and camera/pointer updates do not rebuild unchanged scene meshes or scan the full document. Load at least 2,000 entities; record hardware, display backend, GPU, build profile, fixture hash, frame time, idle CPU, hover, pick and edit latency. Core and IO build without GPUI/raylib/GPU dependencies.

## C27 — Interchange matrix

Use independent existing files and legacy exports for each supported format. Validate geometry bounds, unit scale, winding, normals, DXF lines, nested/component transforms, binary/ASCII cases and 3MF validation errors. Import/export scene selection paths and empty/malformed files. IFC4 proxy classes/spatial structure and units are checked explicitly; semantic `IfcWall` fidelity is not claimed.

## C28 — HTTP/MCP retained subset

Discover status/commands/contract, execute supported commands and screen/world pointer events, read scene/selection/log resources, validate JSON-RPC and REST method/error shapes, timeouts and completion revisions. Console/plugin features report deferred and are not falsely advertised. Use typed setup operations for modeling tests; no live Groovy/JVM server dependency in new CI.

## C29 — Tutorial adapters

Load desktop recorder/player action schema with previews and separate browser docs schema. Menu and button actions satisfy the same expected action. Next/previous/pause/stop/manual completion and malformed/unknown action handling are tested. Keep pending/manual success checks visible; a successful schema validator is not scene correctness proof.

## C30 — Browser delivery

WASM build/serve, browser file open/import/download and local recovery; custom element ready/model/selection/camera/tool/UI methods, promise completion/events, focus and scroll, host reparent/resize as supported, tutorial embedding and explicit single-editor constraint. Unsupported native services show capability explanations. No desktop HTTP or threads requirement. Test both configured WebGPU backend and any claimed fallback, using actual supported browser versions recorded at implementation time.

## C31 — Release qualification

Presentation slice (T68): the headless `--command-catalog` export and generated
CSV cover the live registry once per command ID, validate actual icon keys,
retain explicit deferred availability, and initially leave all four review
columns blank. Regeneration preserves quoted/multiline user assignments and
rejects loss. Inspect the README's actual screenshots and icon visually; verify
descriptive paths/capture provenance, embedded native branding, package PNG/ICO
inputs and Windows resource wiring. Run release-helper and planning checks.
Native installers remain qualified on their own platform runners; Linux source
inspection is not a Windows/macOS execution result.

Linux/Windows/macOS packages launch, open legacy files, save/recover, use menus/shortcuts/dialogs, export and close cleanly. Manual release dispatch uses the selected existing tag, matches the literal app version and immutable source SHA, and rejects incomplete six-target artifact/checksum matrices. Audit old-sysroot Linux ELF requirements independently of the hosted runner; unsigned MSIX is an explicit deferred-signing output, not a signed installability proof. Complete all in-scope requirement/task mappings, measured performance report and deliberate-difference ledger. Verify docs/assets/provenance/build metadata and absence of mandatory Android/JVM/Groovy/TUI dependencies. Initial desktop release does not wait for C30/browser or C33/JavaScriptCore plugins; those have separate required delivery gates. No placeholders or unimplemented commands are reported as finished features.

## C32 — Normalized object ownership and effective-content equivalence

T46 first proves the schema/identity/migration rules with data probes and fixture comparisons; it does not require the unbuilt editor. T03/T05/T16/T33/T44 extend these into runtime/memory/behavior assertions. An unchanged thousand-placement case shares source geometry/layouts without allocating a full bundle/tree per occurrence. Definition edits update appropriate shared occurrences; a changed hotspot/input affects only its occurrence, and resetting it can return to shared content. Distinguish inheritance, explicit empty replacement, generated cache and meaningful/ambiguous imported geometry differences.

Check nested shared layouts with different child transforms/inputs, definition changes plus per-occurrence edits, stable legacy instance aliases and attachment identities, cycles, and coincident independent vertices. Nonidentity definition/placement frames preserve world geometry. Root/domain alias normalization retains authored loose geometry and semantic parameters. Cache eviction/rebuild changes neither authored serialization nor undo; selection/edit mode does not leak through shared definition stores. Compare normalized and legacy-expanded effective views for picking/snapping/export/explode/plugin queries. Measure implemented memory/storage separately from census coordinate coincidence.

## C33 — JavaScriptCore plugin runtime, context and packages

T47 proves native JSC evaluation/callback/GC/promise/interruption and distributable engine artifacts; T48/T49 add lifecycle/SDK contracts; T50 runs the full native plugin gate. Exercise API-range/capability checks, handed-in plugin context, namespaced command/tool/panel/interchange registration, and a command/edit batch that produces the same state and one undo entry as native invocation. Test a two-pick preview/cancel tool, declarative panel, exporter and namespaced state with the plugin absent.

Verify authored versus effective queries, definition/entity/occurrence identity, bounded large queries and stale-revision conflicts. Force GC with retained callbacks; fail activation midway; disable/unload/reload; deliver an async reply after document close and after plugin disposal; assert no stale callbacks, leaked registrations or partial edits. Record actual interruption behavior rather than equating worker execution with cancellation. Package/load samples on Linux/Windows/macOS with pinned engine artifacts and lifecycle ownership checks. Browser support reports its own policy; no native JSC-in-WASM assumption, Groovy/JVM, Node prerequisite or CLI/TUI is required.

## C34 — Geometric feedback for enabled tools

Enumerate every enabled command that creates or changes geometry; selection is the only exception. At every pending point stage, verify a generated-geometry preview or a vector from the previous accepted point to the current snapped pointer. Vector labels show length and signed ΔX/ΔY/ΔZ in model units to at most three decimals. Cover line/construction line, planar shapes, cuboid, path/mesh tools, Move/Rotate/Scale and stretch variants, Push/Pull, object placement, Paint edge/face and selection previews, parameterized arrays, and any other enabled geometry command. Verify array previews update when copy count or spacing changes. Compare preview points, colors and measurements with the committed result for grid/geometry snaps, numeric overrides and entered object definitions. Escape and invalid input must clear or retain the preview according to tool state without partial geometry or history changes.

## C35 — Delete selected geometry and object instances

Select and delete an object instance, a nested instance, and selected vertices, segments and triangles from the active scene graph. Verify dependent topology remains valid, sibling and unrelated shared instances are unchanged, and invalid/stale or unsupported targets fail without partial mutation. Repeat through the Edit menu, toolbar button, command palette and Delete key; each route must produce the same document state, clear deleted selection, and create one undo step. Undo and redo each operation and compare the restored/deleted document and selection state. Include a real viewport click/window selection case rather than only assigning selection targets in a test.

## C36 — Oriented cursor and context-safe guide placement (#8, #9)

In perspective and orthographic views, verify the cursor follows the current snapped or free construction point and shows a readable red horizontal tangent, green remaining tangent, and blue surface normal on level and slanted faces. Grid and axis guides use the same basis. Invoke either guide command and G/T while another multi-pick tool has accepted points; the guide is placed at the cursor while the active tool, accepted points, preview, and document history remain unchanged. With no cursor point, the command must report the missing point without interrupting an in-progress tool. Verify the selector's existing one-click guide fallback when no tool is in progress.

## C37 — Recent project list (#10)

Open more than ten distinct projects and verify the persisted list retains only the ten most recently opened, newest first. Reopen a listed project through the File menu and the command palette; both use the normal dirty-document replacement prompt and project loader. Reopening moves an existing path to the front without duplication. Cancelled, invalid and failed loads do not enter the list. Save/restart and verify preference persistence; stale paths remain visible and report the usual load error.

## C38 — Two-point Cuboid primitive (#11)

Activate Cuboid from Primitives and the command palette. Pick opposite corners in either order, including corners with different X, Y and Z coordinates. Verify a snapped box preview after the first point, final bounds/dimensions and colors against the same two points, active-definition insertion, selection, one-step undo/redo, Escape and degenerate-input rejection. No third height pick is required.


## C40 — GPUI shell with raylib-rs viewport (#21)

Run the native desktop application with a raylib viewport in the real GPUI layout. The current candidate renders to a raylib offscreen target and presents each latest frame as a normal GPUI image, so menus, popovers and panels compose above it. Verify clipping, viewport resize and DPI mapping, focus, pointer/keyboard routing and clean shutdown. Measure at the actual viewport resolution: sustain at least 25 FPS (40 ms/frame), targeting 30–60 FPS; record p50/p95 frame and pointer-feedback timings plus memory. Exercise the textual menu, grouped ribbon, searchable `Ctrl+Shift+P` command palette, right-side selection/object/settings panels, text and numeric inputs, dropdowns, color controls, checkboxes, sliders, status and validation feedback. Empty projects retain the full usable shell. Every presentation dispatches the same command ID, availability and parameter schema; commands and session tests run without a window or renderer. Exercise create, snap, select/deselect, both selector-window directions, transform, object definition/nested-instance editing, arrays, Escape, settings edits and save/reopen. Confirm UI activity does not reconstruct the model and command/model updates reach the viewport without waiting for another mouse event. Test actual text/IME, keyboard focus, shortcut suppression, accessibility labels and close/reopen on the target platform. Capture full-window review images and record the exact GPUI/component/raylib versions and native display backend.

## C41 — Dense array inspection and responsiveness from the reference recordings

Replay the observed workflow: construct a cuboid, copy it, define an object from the copy, and create increasingly dense linear, planar and spatial arrays. Orbit and zoom around the result; select and deselect objects and mesh elements; change visibility/display policy; inspect cursor, guides, tool preview and status feedback while editing. Test at 2,000 entities and at a larger fixture derived from the supplied recordings. Keep input, cancellation and progress reporting responsive during array creation; never block the UI thread on large geometry generation. Check the legacy freeze as a regression, not expected behavior. Record command completion time, pointer-to-visible-feedback and frame-time p50/p95, selection/window-pick latency, peak memory and idle CPU on fixed hardware and build settings. Compare with the T00/legacy measurements when reproducible; the minimum target is a 10% reduction in p95 pointer feedback latency, with 50% as the stretch goal, while producing identical committed geometry and undo behavior. If the legacy operation cannot complete, record that baseline as a hang and report absolute new-stack timings instead of inventing a percentage.


### Current raylib interaction and rendering regression checks

For C10/C17/C24/C36/C40: invert both orbit axes while keeping the orbit target fixed. Start a tool with a selected entity and guide, press Escape three times and observe tool cancellation, selection clearing, then guide clearing. Snap along permanent and oriented guide grid lines between intersections, to guide axes and existing geometry, and verify free points outside the configured radius. Inspect the local planar point field and three-plane grid guide. Compare identical camera frames with shadows enabled/disabled: solid entities cast onto ground and other entities, with directional face shading and ambient fill. Authored lines are black and never shadow casters. Verify the build-generated SVG icons on enabled and disabled ribbon/palette buttons and menu layering over the offscreen image.

## C42 — Project material catalog and editor

Create a new project and verify 16 named ANSI materials plus smoke-white, smoke-grey, side-grey and transparent glass. New lines use ansi-black and faces ansi-grey. Create a named translucent material with Phong/Gouraud metadata, optional texture path and positive U/V dimensions. Save/reopen the material and project; verify every field and alpha. Invalid dimensions or empty name cannot mutate the model. Cancel/Escape and modal scrolling/typing do not affect the canvas. Apply to a selected face, then to a selected instance; sibling instances keep their material bindings. Editing the material recolors every bound element. Direct paint removes its binding. Undo/redo catalog edits and assignment. Import a scene whose material ID collides with the host and verify remapped references and unchanged host colors. Textures and distinct shading interpolation are not part of this phase.

Delete an unused ANSI color and a custom material; verify Undo/Redo and save/reopen without resurrecting deleted entries. Reject deletion without mutation when a material is referenced by an unplaced prototype, instance geometry override, nested default edit, evaluated script output, architecture owner or creation defaults. Keep default line/face/architecture materials available in an empty drawing. Import older white geometry and confirm migration adds the palette without recoloring it. The bounded scrolling list keeps manager actions visible. Creating a new drawing resets the active material choice.

For the #30/T63 extension, create and assign procedural grass to a surface; vary
seed, density, dimensions, curvature, wind and color variation. Compare seeded
output across redraw/save/load; generated blades never change authored topology
or become snap/select/export targets. Verify GPU domain reuse for parameter edits
and repeated instances, isolated assignment overrides, canonical architectural
regeneration, Undo/Redo, resource/base-material import collisions and unknown
program round-tripping. Exercise custom surface deformation, shader errors with
real diagnostics/fallback, typed texture inputs, PBR base settings, transformed
bounds/culling, work budgets, shadows and transparent passes. Idle animation must
stop with zero wind or no visible animated assignments. Review parameter forms,
program editing, Apply/Cancel/Escape and MCP program/schema discovery. Record
backend/platform limits explicitly.

For #29/#31/T64, render Water, Brick and Wood on ordinary surfaces and inspect
wave motion/transparency, continuous running bond/mortar, longitudinal wood grain
and end grain. Change parameters and transform sources without adding CPU
triangles. Create a Tree prototype, place/copy instances, change one instance's
seed/size and preserve the sibling. Verify bounds picking/window selection,
wireframe feedback, prototype inheritance, copy/paste, project save/reopen and
history; generated branches/leaves remain GPU-only. Verify zero wind/speed idle,
frustum rejection, bounded foliage/subdivision work, shadows, compile failure
fallback and API/schema discovery. Record native backend limitations explicitly.

For #29/#30/T65, edit built-in and custom shader colors through the HSL/RGB picker
and opacity controls in material and object editors. Verify Save/Cancel, Reset,
Read Selection Values and prototype/instance isolation; unchanged colors retain
stored precision. Create Foliage, change spread to cover the source boundary,
check seeded output/wind, alpha shadows, bounds, culling/LOD and candidate limits.
Source geometry and source GPU buffers must survive parameter edits unchanged.
Save/reopen foliage and color overrides without converting model/API color arrays.

For #29/#30/T66, open New Shader and Edit Program. Compare its full-window footprint
with the object script editor and check equal-width code/preview panes. Switch
Definition/Vertex/Fragment tabs, retain unsaved text/cursor state and inspect JSON/GLSL
highlighting. Orbit/pan/zoom the preview independently. Exercise live draft refresh,
all built-in generation modes, invalid JSON, shader compilation errors and recovery.
Cancel/Escape must preserve the drawing revision, selection, camera and history;
Save updates only the intended program and retains matching material values. Closing
either way restores the ordinary viewport.

## C43 — Scripted object hooks, preview and instance values

Create or script an existing prototype and retain its authored geometry. Preview a valid `onCreate`/`onAnimationFrame` pair in one monospace script, with shared top-level helpers/constants, materials and helper-generated faces/lines. Ctrl+Space shows a scrollable documented API list; typing filters, arrows navigate, Enter/Tab inserts, and Escape dismisses completion before the editor. Verify independent preview orbit/pan/zoom, editor focus, diagnostics and Cancel isolation. Save, place two instances, drag scripted hotspots, copy an adjusted instance and confirm independent values. Edit graphical geometry in the prototype and include it through `this.geometry`. Save/load, undo/redo script changes, stable IDs and query-only model data. Explode an adjusted instance and compare its current issued world geometry; it must survive without scripts. Test syntax errors, thrown errors, invalid/duplicate IDs, nonfinite coordinates, excessive output, infinite loops, runtime shutdown and all packaged native engine providers. A failed script cannot hang or mutate other model content.

For C43 scaling, run at least 56 instances of one scripted prototype. Every
instance must update after a source edit, including the final occurrence; work
slices must continue without a batch-timeout diagnostic or repeated initialization.
Keep instance state independent, send the scene snapshot once per sweep, and
verify fresh globals and frozen scene queries with the shared VM. An interrupted
instance must not stop healthy instances in later work slices or sweeps.

### Solids picking order and camera travel follow-up (0.33.1)

Intersection Curves must be directly reachable in the Solids dropdown, Modify
ribbon and left Solids toolbar. For overlapping cubes A at x=0..2 and B at x=1..3,
pick A then Shift-pick B: Difference leaves x=0..1. Undo, reverse the pick order,
and verify x=2..3 instead. The later-created object may be the first operand.
Explicit automation selections use target array order; Add preserves existing
order and appends new targets, while Remove drops only the requested targets.

Shift-wheel moves the camera position and target by the same vector parallel to
their connecting line. Assert unchanged distance, view orientation, projection and
field of view for perspective and orthographic cameras, and synchronization with
the walkthrough controller. Ordinary wheel remains zoom; Ctrl/Alt fine rates remain
1/10 and 1/20. Check real native wheel input in addition to camera arithmetic.

### Model line display preference follow-up (0.33.2)

Start with old/missing UI preferences and a model containing colored segments:
lines render black. Uncheck Black model lines in UI Settings and verify material
colors/alpha appear; recheck it and verify black lines return. Face colors and
authored geometry/materials/revision/history stay unchanged. Check selected-line
highlights, wireframes, script preview, preference persistence and frame invalidation
on each toggle without continuous idle redraw. The browser renderer uses the same
preference.

### Object handle phantom feedback follow-up (0.33.3)

In a drawing with many unrelated faces/instances, drag ordinary attached-geometry
and scripted instance hotspots. The source document, filled geometry, unrelated
script state and main scene snapshot remain fixed while the affected object's
wireframe and handle move. Rapid target updates must coalesce and eventually show
the latest target. Preserve script access to the full frozen model and original
instance IDs. Include nested transforms and private/inherited child layouts.
Release commits the final target once; one Undo restores it. Escape/click/stale
context cancel without mutation; release/cancel never wait for an in-flight script.
A stalled or invalid preview must keep pointer/selection/camera interaction responsive
and must not publish late geometry after the drag ends.


### Selection refresh follow-up (0.33.5)

Select architecture with roughly 600 edges and 800 faces. After transient UI effects
settle, an unchanged selection must not drive recurring UI/GPU redraw or control
reconstruction. Record idle process CPU and viewport frame sequence against the
previous build on the same fixture/backend. Camera/pointer changes must reuse
world-space controls; document, selection, context, tool and display-policy changes
must invalidate them. Verify marker symbols/scale and real slab/wall handle dragging,
phantom preview isolation, Escape and one-step Undo/Redo. Selectors and numeric fields
must still follow edits and selection changes. Focus/type/blur an input: the caret
blinks while focused and stops on blur. Programmatically updating an unfocused field
must not start its caret timer or a permanent window repaint loop.


### Selection activation and realized mesh counts follow-up (0.33.6)

Select two or more architectural elements: no control handles are built, displayed
or hit-tested. Select just one: its controls return and remain draggable. Verify
this on a dense architectural drawing, including after marquee selection.
Instrument the tessellator in tests: repeated copy/array availability queries must
not invoke it; the actual operation must still validate and generate its geometry.
Repeated commands reuse eligibility until document, selection or context changes.
Record native selection-release-to-feedback and selected camera interaction timing
on the reported drawing, rather than treating a small entity-count fixture as proof
that a multi-second activation pause is solved. A partly contained architectural
owner must be rejected by window selection and accepted by crossing selection.
Containment follows effective instance geometry after overrides, rather than
reconstructing its original parameters.

Check T/S edge and face cells against effective mesh data for walls/slabs/frames,
plain and scripted objects, nested assemblies and duplicate instances. Include
mixed primitive/owner selection, selected parent plus child, an empty geometry
override and root graph geometry. Generated primitives must appear in totals, and
parent/child/primitive overlaps must not inflate selected counts. Changes to
geometry or selection invalidate cached native statistics; pointer/camera updates
do not trigger recounting. Logical selection membership remains unchanged.
