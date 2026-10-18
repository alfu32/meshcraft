# Foundation specification

Labels describe evidence status, not implementation progress. `K` refers to the legacy Kotlin package root defined in `../evidence.md`. Acceptance scenario IDs are specified in `../acceptance.md`; task ownership and dependencies are in `../tasks.json`.

### F01 Document and editing space

**Observed.** A model contains reusable prototypes plus an instance tree rooted at a root prototype/instance. Each prototype owns segment, triangle, dimension, text, hotspot, and optional domain stores. Geometry is stored in local coordinates; instances supply origins and basis axes, composed through parents. Y is up. Definition axes/origin and instance axes/origin are separate persisted values.

Rust must represent this distinction before implementing grouping, while normalizing ownership according to F10. The Kotlin class layout and its materialized trees/stores are not a required canonical Rust layout. `to_world`, `to_local`, vector and normal transforms have distinct contracts; a normal under nonuniform scaling uses inverse transpose. Selection and edits belong to an explicit active context. Raw geometry in one context does not cut or merge another context, even when snapping to its visible geometry.

The observed matrix order is `world = parent_world × instance_matrix × definition_matrix`. To assign a desired world pose, legacy code computes `instance = inverse(parent_world) × desired_world × inverse(definition_matrix)`. Preserve that order; definition axes are not simply an inverse pivot transform.

Source: `K/model/GroupScene.kt`: `ObjectPrototype`, `GroupNode`, `worldMatrix`, `toLocal`, `toWorld`. Acceptance: C01, C07.

### F02 Precision, units, and tolerances

**Observed + proposed change.** Current stores use f32/libGDX vectors; `ModelUnit` is an arbitrary positive size and label, initially `(1,"unit")`. Grid spacing is initially 1, snap radius 12 pixels, circle segments 24. The importer/exporter has additional recognized physical units. Do not assume one stored coordinate always equals one meter, or that changing the display unit rescales geometry.

Observed conversions: displayed length is raw length × `modelUnit.size`; physical export treats that size as meters per model unit. Output coordinate scale is `size / meters_per_output_unit`, or simply `size` for unitless output. Export target basename's last token can select a unit suffix (for example `_mm.obj`). 3MF import uses `model_units_per_mm = 0.001 / size`. Thus size 0.001 with `part_mm.obj` has coordinate scale 1, while unitless `part.obj` has scale 0.001. Retain this filename convention or record a deliberate UI migration to an explicit unit selector.

Use f64 model calculations and f32 GPU buffers. Centralize named distance, planarity, angle, quantization, and screen-pixel tolerances. Preserve relevant legacy scales initially: line-store epsilon `1e-3`, face-store epsilon `1e-4`, hotspot keys `1e-3`, and separate snapping thresholds (including Snapper's `1e-1` internal epsilon). These values serve different purposes; replacing them all with one epsilon changes behavior. T00/T02 establish scale/extent tests and unit conversion direction. Do not add unit suffix parsing merely because old intent docs mention it.

Source: `K/model/{ModelUnit,DraftLineStore,DraftFaceStore,HotspotStore}.kt`, `K/input/Snapper.kt`, `K/export/MeshIo.kt`. Acceptance: C01, C02, C11.

### F03 Geometry stores and identities

**Observed.** Segments have endpoints and stable string IDs. Faces are colored triangles with IDs, not authoritative boundary-loop polygons. Adjacency/regions can be derived using tolerance-aware coordinates. Overlapping/touching collinear edges are consolidated; segment insertion splits existing segments. Face cutters exist and are called by explicit mesh-intersection/object-cut/boolean paths. Current ordinary Line/Construction Line call `GroupNode.addSketchSegment`, which inserts/cleans lines without calling the face cutter. Automatic face creation from a line loop and automatic face cutting by ordinary drawing are documented intent, not established current wiring. Selected geometry and hotspot attachment references depend on both identities and legacy quantized endpoints.

Rust must separate raw insertion/deserialization from editing insertion with segment intersection/cleanup and explicit face-cut operations. Raw load must not run interactive topology repair or generate fresh IDs. Preserve winding, face RGBA, independent lines, and open/nonmanifold meshes. Current `ModelCleanup.run` invokes line cleanup per prototype; do not infer automatic face union from the older roadmap. Derived adjacency, spatial indexes, render chunks, and generated domain triangles are revision-keyed caches. A failed cleanup must leave original geometry intact and report failure.

Primitive semantics and IDs do not require storing a repeated Vector3 on each segment/triangle corner. F10 evaluates a per-bundle stable vertex table with indexed primitives; a corner reference may carry a sparse position offset when a legacy alias shares identity across slightly different serialized coordinates. This preserves coincident independent identities and legacy slot aliases. Base geometry, authored instance edits and generated runtime geometry must not become three competing authorities.

Source: `K/model/DraftLineStore.kt`: `addSegment`, raw insertion, cleanup; `DraftFaceStore.kt`: `addPolygon`, `appendTriangleRaw`, `cutBySegmentInPlane`; `ModelCleanup.kt`. Acceptance: C02, C03, C04.

### F04 Prototypes, instances, and overrides

**Observed.** Grouping/create-object creates reusable prototype content and an instance. `OBJECT_PLACE` reuses a definition. Editing the prototype affects its instances; runtime hotspot edits can create instance geometry overrides without changing the definition. Nested children, transformed origins/axes, names, and glue-to-surface are supported. Explode emits ordinary local geometry in the parent context. Prototype deletion must account for users of the definition.

Keep prototype-mode edits and instance-mode changes explicit. Moving an object must not silently bake its prototype. Placement and movement with glue enabled align to a picked surface. Preserve selected result behavior, naming, nesting, and any domain-to-mesh conversion on explode. Generated geometry must not become a second authoritative copy.

Current children are stored/cloned on instances and synchronized across instances of a shared prototype. The Rust proposal shares child-layout templates and records true per-occurrence differences explicitly. Unchanged instances borrow shared geometry; resolved caches are not authored overrides merely because the legacy writer serialized them. F10/C32 governs this normalization and the distinction between inheritance and an explicit empty edit.

Source: `K/model/GroupScene.kt`; `K/tools/ObjectPlaceTool.kt`; object callbacks in `K/Main.kt`. Acceptance: C07, C08, C15.

### F05 Legacy model decoding

**Observed.** Schema constant is 17. Loader paths include prototype/instance documents, prototype-only recovery, old `rootGroup`, and flat segments/faces. JSON omits values equal to Kotlin object defaults: `{}` can mean a zero vector, omitted color alpha means 1, omitted enum kind means its default, and omitted version can still be current schema. All 60 indexed model fixtures omit top-level version.

Implement explicit per-DTO defaults from `source-inventory.json` and `ModelPersistence.kt`, not one global “missing vector” default. Preserve unknown deferred data through a raw extension envelope. Validate references and transforms before publishing the loaded scene. Invalid loads leave the existing document intact. Do not infer an older version from field absence alone. Full requirements and examples are in `../compatibility.md`.

Source: `K/model/ModelPersistence.kt`: DTO classes, `ApplySnapshotSession.initialize`/`finalizeApply`. Acceptance: C03, C04.

### F06 Encoding and compression

**Observed.** Project suffixes `.octd` and `.k3d` use the same model payload family. Loader detects gzip by magic bytes, independent of filename. Current writer emits UTF-8 JSON uncompressed at or below 10 MiB, gzip above 10 MiB. Undo entries use a separate encoded snapshot representation; inspect `encodeSnapshot`/`decodeSnapshot` rather than assuming the project-file container applies.

Rust reads both project containers, sparse defaults, and captured legacy history entries. A legacy-compatible writer must keep schema-17 field semantics while optionally including explicit defaults. A new Rust-only format/version requires a separate export decision; never reuse 17 for incompatible meanings. Compression, IDs, and round-trip geometry are independent checks.

Source: `K/model/ModelPersistence.kt`: `saveSnapshotBytes`, `parseSnapshotStream`, `encodeSnapshot`, `decodeSnapshot`. Acceptance: C03, C04, C05.

### F07 Document lifecycle, autosave, and recovery

**Observed + proposed change.** Desktop defaults to `octodraw.octd` in the working directory; GUI supports open/save-as/import/export. Changes schedule autosave after 1500 ms. Selection/settings can trigger shared change listeners; audit which state is serialized. Existing load makes a `.bak` copy and performs cancellable staged decode/application with progress. Large models must not block the UI during the whole process.

Rust uses immutable save snapshots, same-directory temporary files plus atomic replacement where supported, serialized writes, and observable errors. Cancel/failure cannot replace the current document or destroy the prior good file. Name, destination, dirty revision, and saved revision are separate. UI preferences are not document geometry. Keep open/save-as available through both menus and buttons. Original files remain untouched during import/compatibility tests.

Source: `K/Main.kt`: `scheduleAutosave`, `saveModel`, `snapshotForPersistence`, `beginAsyncModelLoad`; `K/model/ModelPersistence.kt`. Acceptance: C05, C17.

### F08 Undo and redo

**Observed + proposed change.** Legacy undo stores encoded whole-model snapshots, debounces changes for 3500 ms, truncates redo on branching, and is instantiated with max 20 entries. Project files can include those entries and an active index. Snapshots include camera/lighting/settings as well as geometry; selection alone is not an explicit ModelSnapshot field.

Use explicit operation transactions: a preview does not create history, commit is one undoable action, cancel makes none, and subsequent edits discard redo. Keep a bounded memory policy. Preserve legacy history blobs losslessly; allow replay of entries whose models the reader supports. Characterize older camera/settings undo before deciding the new policy. A current Rust transaction log need not reproduce timing-based coalescing, but that divergence must be documented.

**Native transaction policy (T04).** One explicit commit produces one undo entry; the legacy 3.5-second debounce is not reproduced. History snapshots cover all authored `Document` state, including model unit/grid/snap settings. Runtime camera, lighting/shadow controls, application preferences, and selection live outside `Document` and are not restored by undo; selection is pruned against the restored document. The initial desktop history retains at most 20 operations and 64 MiB of serialized snapshot data, pruning the farthest state from the current position. If a single before/after pair exceeds that budget, the candidate edit is rejected atomically. Imported history keeps its original envelope and encoded entries unchanged; supported snapshots decode only when traversed and join the same bounded timeline. Legacy camera and lighting values are not applied to runtime controls by replay.

Source: `K/model/UndoRedoManager.kt`, `K/Main.kt:7758`. Acceptance: C06.

### F09 External references

**Observed.** Prototypes can reference external model files. Desktop persistence saves referenced prototypes separately, strips embedded geometry from the containing snapshot, and reloads linked definitions. Therefore an xref document can be incomplete without its linked files. Relative/absolute resolution, renaming, nested links, missing files, and preserved hotspot runtime data require explicit tests. Browser behavior differs and must not assume filesystem access.

Implement link metadata in the initial reader even before live xrefs ship. Resolve relative links against the documented source-file base, detect cycles, report unresolved references, and retain cached geometry if available. Shared definitions and instance overrides must survive refresh. No silent empty overwrite of an unresolved external document. Android URI handling is deferred.

Source: xref save/load/strip functions in `K/Main.kt`, DTO xref fields. Acceptance: C16.

### F10 Normalize object ownership and eliminate redundant state

**New: explicitly requested examination; proposed normal form.** Before freezing core document/geometry contracts, classify every object-model field as authored definition data, authored placement/occurrence input, derived content/index, UI session state or legacy IO metadata. Give every authored value one owner. Preserve the definition/instance distinction and transform/identity semantics while eliminating no-op geometry clones, duplicate hierarchy trees, competing store-selection flags and generated meshes mixed with authored content.

The source/fixture audit in `../object-model.md` identifies 2,899 override bundles exactly equal to their definitions, repeated coordinate slots, per-instance child copies and domain/root ownership ambiguities. Normalize to shared definitions/content/layouts, explicit typed instance edits, occurrence-aware identity/selection, and revisioned resolved views. Preserve meaningful differing overrides, explicit empty replacements, domain source attribution, unknown legacy data and nonidentity frames. Coordinate equality alone never authorizes welding.

T46 compares the proposed stable-vertex/indexed-primitive representation and shared child layouts with a minimal alternative using real fixtures and synthetic edge cases, records the schema/ownership/migration decision, and gates T02/T03/T05/T16. Use a single effective-content query service for rendering/picking/export/explode and the later JavaScript plugin context. Do not publish old mutable GroupNode/store objects as the plugin API.

Source: `K/model/GroupScene.kt`: `recomputeInstanceGeometryFromPrototype`, `syncPrototypeInstances`, `refreshPrototypeVertexIds`, `enterGroup`; `ModelPersistence.GroupInstanceDto`; `DraftFaceStore`; `../object-model-evidence.json`. Acceptance: C01, C03, C07, C15, C32.

### I01 Orbit, pan, zoom, and coordinates

**Observed.** Perspective orbit is the normal viewport mode; right drag orbits, Shift+right drag pans, wheel zooms. Camera target and perspective pose are saved. Drawing rays, overlays, and pointer automation must use the drawable viewport rectangle, not the full window once a menu/panel consumes space. Preserve cursor-sensitive navigation and object picking under transforms.

Source: `K/ShiftCameraController.kt`, `EditorCameraController.kt`, camera functions in `K/Main.kt`. Acceptance: C09, C17.

### I02 Orthographic and walkthrough cameras

**Observed.** Six orthographic views exist: top, bottom, left, right, front, back. Orthographic zoom uses a separate projection/controller; it cannot be approximated by a distant perspective camera. Walkthrough has movement, support-height handling, gravity/jump/height tuning and persisted runtime preferences. Model schema saves a perspective camera state, not every session camera parameter.

Source: `K/OrthographicCameraController.kt`, `WalkthroughCameraController.kt`, `Main.setOrthographicView`, `configureOrthoViewport`. Acceptance: C09.

### I03 Pointer and focus routing

**Observed.** Tools accept world point/normal/valid plus mouse-button events, while selection also consumes screen coordinates. Camera routing, UI scroll capture, text input, popup numeric editing, touch modifiers, and in-tool operators coexist. Menus must capture pointer/keyboard events before the viewport. Opening or closing a menu cannot accidentally draw a point, delete geometry, or reset a draft.

Rust normalizes GUI, scripted, and embedded events through one dispatcher, with explicit source, coordinate space, modifiers, and active context. A UI gesture is consumed exactly once. Invalid world hits never commit geometry. Menu focus suspends viewport shortcuts; text editing preserves text shortcuts. Touch-specific application polish is deferred with Android, but in-tool buttons remain useful on desktop/browser.

Source: `K/input/{ToolPointerProcessor,CameraInputRouter}.kt`, `K/ui/ToolController.kt`, `ToolInputProcessor.kt`. Acceptance: C10, C17, C23.

### I04 Snapping

**Observed.** Snap types/priorities: free 0; face 1; line/grid-line 2; grid/grid-guide/axis-guide 3; endpoint/midpoint 4. Priority is one part of actual candidate comparison, along with screen distance/depth/visibility. Snapping considers active and other visible transformed groups. A virtual plane is a fallback hit and is not selectable geometry.

Preserve endpoint/midpoint/segment/face/grid/guide behavior in perspective and orthographic views. Screen radius is in pixels. Hidden/filtered geometry must follow characterized snap visibility rules. Do not promise global parallel/perpendicular/from-point inference solely from original SPEC aspirations. Rebuild stale spatial indexes safely or use a correct fallback query.

Source: `K/input/{Snapper,SnapType,SnapResult}.kt`. Acceptance: C10.

### I05 Grids and guides

**Observed.** `PLANAR_GRID`, `AXIAL_GRID`. Permanent grid visibility, normal axis and extents are adjustable; half-size is clamped to 1..1000. Grid spacing also drives helpers. `G`/Planar Grid places oriented plane guides; `T`/Axial Grid places axis guides at snap points. Adaptive local grid feedback helps construction. Guide orientation and local-space axes affect snapping.

Keep these helpers separate from persistent model lines. UI exposes spacing, visibility/orientation/extents and guide actions in both workspaces. Determine guide/session persistence from code rather than assuming guides are serialized by ModelSnapshot.

Source: `K/input/GuideManager.kt`, `K/tools/GuidePlacementTool.kt`, grid functions/prefs in `K/Main.kt`. Acceptance: C10, C17.

### I06 Selection and display filters

**Observed.** `SELECT`. Click select; Shift adds, Ctrl removes. Left-to-right screen window selects inside, right-to-left crossing includes intersections; Alt forces window selection. Empty-space two-corner flow selects a world volume. Double-click instance enters editing; double-click face expands a coplanar region; triple click expands connected geometry. Current multi-click threshold includes 350 ms timing.

Selection spans edges, faces, objects, voxels, dimensions, text, hotspots, architecture/HVAC elements and control handles as supported by each tool. Visibility/lock/wireframe state affects display and picking, with separate per-type controls. Hidden/locked entities must obey characterized rules across click/window/volume. Keep volume selection distinct from a view frustum window. Expose counts and context paths consistently to UI and automation.

Source: `K/tools/SelectTool.kt`, selection/display callbacks in `K/Main.kt`. Acceptance: C11, C12, C15.

### I07 Tool lifecycle, numeric input, and operators

**Observed.** Tools expose enter/exit/cancel, pointer move/down/up, key down/up, text input, anchor, measurement and preview geometry. Tool switches clear transient numeric/anchor/copy state. Esc generally cancels and returns to Select; Enter/Backspace have tool-specific semantics. Copy mode is tool-dependent and Ctrl can toggle it. Floating operator buttons expose finish/cancel/undo-point/copy/distance and object-close actions.

Rust uses explicit tool state machines; shared input plumbing must not flatten distinct tool workflows. Validate numeric values before commit; preview and commit must agree. Preserve tool-cycle shortcuts `L`, `D`, `O`, `M` with actual ordering from the current input handler, including hotspot placement mode outside ToolId. Numeric popup and plain typed-number paths call the same tool operation. Exact suffix/unit parsing beyond observed floats is deferred intent.

Source: `K/ui/{Tool,ToolController,ToolInputProcessor}.kt`, individual tool handlers, `docs/SHORTCUTS.md`. Acceptance: C13, C17, C23.

### A01 Command catalog and dispatch

**Observed + proposed change.** Current palette has static IDs, generated `tool.builtin.<lowercase ToolId>` IDs, toolbar visibility IDs, and plugin namespaces. Available commands vary by runtime; discover them. Many file/edit/render operations are direct UI callbacks today and need canonical commands in Rust.

One registry owns ID, label/menu path, parameters, preconditions/enabled reason, checked state, shortcut, capability, and handler. `Ctrl+Shift+P` opens the complete palette of available commands. Buttons, desktop menus, palette, automation, and tutorial actions invoke the same registry. A command that activates a multi-step tool does not itself commit geometry. Separate command dispatch from GUI widget lifetime. Stable existing IDs are preserved where meaningful; new IDs and unsupported deferred IDs are explicitly cataloged. Enumeration is generated from registrations rather than copied in several surfaces.

Source: `K/plugin/CommandPalette.kt`, `K/Main.kt`: registration functions. Acceptance: C18, C23.
