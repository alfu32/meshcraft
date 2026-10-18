# Modeling specification

Together with guide, selection and object-placement requirements in `foundation.md`, these sections account for all 74 built-in ToolIds. `PLUGIN` is the remaining adapter ID and is deferred. Hotspot placement is a separate action/mode. Tool specifics must be characterized with the named source class; shared family descriptions do not authorize erasing distinct state machines. All commits use the document/command/undo contracts in `foundation.md`.

### User-confirmed modeling foundations

These migration constraints were confirmed by the user on 2026-10-18. Treat them as behavior to preserve and validate; they are not a claim that every workflow is already implemented. Modeling is selection-driven: select supported lines, faces, meshes, objects, or other entities, activate the operator, then provide its required picks. Do not make supported operations object-only.

The scene graph is hierarchical. Construction writes directly into the active graph, normally the root; entering an object instance by double-click or Edit Object switches to its shared definition, where edits update its instances. Definitions may contain nested instances. The original two-toolbar workflow separated Planar Construction from Modification; Modification included move/copy, 3D rotate and rotate-copy, scale, stretch, and vertical extrusion. Planar Construction included lines, construction polylines, meshed double lines, closed-polyline meshes, rectangles, circles, and hotspots. A common 3D workflow starts from a planar mesh and extrudes it.

Keep pick sequences explicit: two points for line/circle/rectangle/dimension/move/stretch/extrude, three for three-point tools, four points (origin plus three vectors) for 3D rotation, and unbounded picks for path tools. Linear, planar, and spatial spreads remain distinct selection-based array commands; rotational and helical arrays are separate advanced commands. Hotspots bind an operation graph to attached geometry and keep their operation set distinct from ordinary selection tools.

### D01 Line and construction line

**Observed.** `LINE`, `CONSTRUCTION_LINE`. Line chains successive picked points; construction line creates a single segment draft but emits the same ordinary segment entity type. Enter finishes/resets according to the active tool. Added lines split intersecting same-context segments and consolidate collinear overlaps/contiguities. The inspected current path does not invoke `cutBySegmentInPlane` or infer a new face from closing a line loop. Those are older documented intentions; adding them is a separately reviewed enhancement. Do not auto-cut other contexts or automatically cut during arbitrary transforms.

Line edits use local coordinates and a shared transaction. Cover endpoint-on-edge, interior segment intersection, collinear touching/overlap, zero-length rejection, cancellation and exact distance input. Test that a line drawn on a face currently adds a line without silently changing face topology. The explicit cutter's infinite-line behavior is tested through the operations that call it. A chain's undo grouping must be specified by T13.

Source: `K/tools/{LineTool,ConstructionLineTool}.kt`, line/face stores. Acceptance: C02, C13.

### D02 Planar shapes

**Observed.** `RECTANGLE`, `SURFACE_RECTANGLE`, `QUAD`, `CIRCLE`. Rectangles use their picked construction plane; surface rectangles retain a picked face plane/basis. Quads use the actual staged picking construction in `QuadTool` (do not assume four unrelated arbitrary points from old docs). Circles produce polygonal geometry with configurable segment count, initially 24. Shapes create edges/triangles through editing paths and support appropriate previews/numeric values.

Concave triangulation and hole-aware region operations belong to shared geometry support. Shape edges use the same segment-splitting path as line tools; shape creation emits its own triangles without assuming an automatic existing-face cutting pipeline. Quad specifically takes origin, second point and third point, then computes the fourth as second + (third − origin). Test rotated groups, reversed pick order, off-plane/degenerate input, and non-default circle resolution.

Source: `K/tools/{RectangleTool,SurfaceRectangleTool,QuadTool,CircleTool,PlaneBasis}.kt`. Acceptance: C02, C13.

### D03 Polyline and double line

**Observed.** `POLYLINE`, `DOUBLE_LINE`. Path workflows allow line/arc/circle segment modes, point undo, finish, and cancel. Settings include arc maximum length 0.5, double-line size 1, offset 0.5. Double line emits offset path geometry and corners according to current join rules. Typed dimensions, open/closed paths and arc subdivision must be preserved.

A reusable sampled path/plane basis representation feeds later architecture and sweep tools. Keep authored path intent during preview and materialize tessellated segments on commit. Do not add a general spline editor as implicit scope.

Source: `K/tools/PolylineTools.kt`, `PolylineSettings.kt`. Acceptance: C14.

### D04 Mesh and outlines

**Observed.** `MESH`, `POLYLINE_MESH`, `FACE_OUTLINE`, `LINE_OFFSET`. Mesh builds triangular face geometry from picks; polyline mesh converts a selected/constructed boundary using its actual selection prerequisites. Face Outline emits boundary lines for selected face regions rather than every internal triangle edge. Line Offset operates on selected line geometry in a reference plane.

Preserve source selection requirements, winding, color and boundary extraction under concave/disconnected input. Rejected input reports a reason without deleting selection. This family supplies shared loop ordering and boundary extraction for holes, extrusion, mechanical and architecture tools.

Source: `K/tools/{MeshTool,PolylineMeshTool,FaceOutlineTool,LineOffsetTool}.kt`. Acceptance: C14.

### D05 Dimensions and screen text

**Observed.** `LINEAR_DIMENSION`, `TEXT`. Dimensions persist start/end/offset and derive offset lines/measurement display. Text persists position/string/size, plane axes/normal and screen-text flag; bitmap text is distinct from vector text. Selected text can be edited in panels. Picking, selection, transform/copy/delete, units display, and save/load must work for annotations.

Source: `K/tools/{LinearDimensionTool,TextTool}.kt`, `K/model/{DraftDimensionStore,DraftTextStore}.kt`, `K/DimensionMath.kt`. Acceptance: C14, C03.

### D06 Vector text and glyph catalogs

**Observed.** `VECTOR_TEXT`. Vector text uses a glyph catalog, with text, size, tracking, line spacing and glyph source path. Default source is `embedded:alphabet`, backed by `assets/glyphs/alphabet.octd`. Defaults: size 1, tracking 0.1, line spacing 1.25. Catalog models contain reusable geometry; they are not ordinary font files. Persist selected vector text metadata and retain link/source behavior.

Test multi-line strings, unknown glyph fallback, changed spacing/size, external catalog path, glyph plane orientation, and reopen. Do not silently convert all vector text to bitmap labels.

Source: `K/tools/{VectorTextTool,VectorGlyphCatalog,EmbeddedVectorGlyphCatalog,VectorTextSettings}.kt`, text DTO/store. Acceptance: C14.

### T01 Move, rotate, scale, and paint

**Observed.** `MOVE`, `ROTATE`, `SCALE`, `PAINT`. Transforms operate on selected supported entities with picked references, previews, numeric control, and supported copy-mode toggles. Scale includes reference-axis constraints/squash/stretch; preserve perpendicular components where the current tool does. Move can glue object instances to surfaces. Rotation orientation follows picked references/local context.

Paint applies RGBA to faces/selected supported content and exposes current color/picking flows. Transparency affects both rendering and export limitations. Shared vertices within an operation must transform once. Transforming raw geometry does not implicitly run drawing-time auto-cut. Cancellation leaves all original coordinates intact.

Source: `K/tools/{MoveTool,RotateTool,ScaleTool,PaintTool}.kt`, scene transform methods. Acceptance: C13, C15.

### T02 Stretch variants

**Observed.** `STRETCH`, `STRETCH_SCALE`, `ROTATE_STRETCH`. These deform selected regions around picked references, differing from rigid whole-instance transforms. Preserve actual scale constraints and reference-plane handling, selected/unselected boundary behavior, and hotspot/voxel special cases. Use immutable source geometry for the draft; successive pointer moves must not accumulate numerical drift.

Source: corresponding tool files and helpers in `K/tools/MultipleCopyTools.kt`/`K/model/GroupScene.kt`. Acceptance: C15.

### T03 Copy and array family

**Observed.** `COPY_MULTIPLE`, `PLANAR_TRANSLATE_MULTIPLE`, `VOLUMETRIC_TRANSLATE_MULTIPLE`, `PLANAR_ROTATE_MULTIPLE`, `HELICOIDAL_ROTATE_MULTIPLE`, `ROTATE_2`, `ROTATIONAL_ARRAY`, `HELICAL_ARRAY`, `ROTATIONAL_ARRAY_EQUALIZED`, `HELICAL_ARRAY_EQUALIZED`.

These act on the supported selection and have distinct reference-pick/count/extent/angle/height workflows. Linear, planar, and spatial spread remain separate commands; fixed-step multiplication and equalized distribution are distinct, and helical variants add axial movement. `ROTATE_2` must retain its source semantics rather than becoming an alias of Rotate. Arrays copy mixed payloads supported by the source (raw geometry, instances, annotations, voxels as applicable); determine each supported payload explicitly.

T21 must record each tool's ordered states, numeric meaning, copy count policy, endpoint inclusion, selected result and invalid-count behavior before implementation. Reuse transform/payload and preview helpers, not a generic UI that loses distinctions. Large previews need bounded cost. Test negative angles, nonzero height, one/zero counts, full revolution, and rotated local axes.

Source: `K/tools/MultipleCopyTools.kt` (2,773 lines; inspect only the named class and shared helper region). Acceptance: C15.

### T04 Push/pull

**Observed.** `PUSH_PULL`. Extrudes picked/selected face regions with shared boundary side generation and top faces, using signed displacement along appropriate normals. History includes fixes for non-coplanar selected sets; do not assume only one isolated triangle can be extruded. Preserve colors and selection/result semantics. Concave regions and holes must not cap over an intended opening.

Test a rectangle prism, multi-triangle coplanar patch without internal duplicate walls, selected non-coplanar faces, reversed normal/negative distance, zero distance, undo/cancel. Manifoldness is a test for valid closed inputs, not a promise for arbitrary open/invalid meshes.

Source: `K/tools/PushPullTool.kt`; relevant scene/face-store helpers. Acceptance: C13, C19.

### T05 Sweeps, ribbons, and revolve

**Observed.** `EXTRUDE_SWIPE`, `EXTRUDE_SWIPE_3D`, `RIBBON_3D`, `REVOLVE`. Planar path extrusion uses polyline/arc settings; spatial sweep and ribbon transport orientation in 3D using shared sweep support. Revolve supports a full 360-degree sweep and configurable tessellation. Preserve profile/payload selection requirements, frame transport, open/closed path behavior, caps and winding.

Test an elbow, a nonplanar path, tight/collinear turns, a closed path seam, and full-revolution seam closure. Do not introduce an external parametric CAD kernel to implement behavior already expressed as triangle generation unless T01/T23 evidence justifies it.

Source: `K/tools/{ExtrudeSwipeTool,ExtrudeSwipe3dTool,Ribbon3dTool,Sweep3dSupport,RevolveTool}.kt`. Acceptance: C19.

### T06 Hole and cutout variants

**Observed.** `CUT_HOLES`, `CUT_HOLES_2`, `CUT_OUT_3`. Both hole tools are registered; the second is presented as robust and uses a different implementation. They use selected faces and selected boundary/edge geometry. Cutout has its own algorithm/interaction. Preserve all IDs and characterize the supported differences rather than dropping an older tool because a newer file exists.

Projected planar cuts, ordered loops, inside removal, selection inheritance, finite-versus-extended segment behavior, nested holes and tolerance handling need explicit fixtures. Failure leaves the original geometry intact. Internal helper files `CutHoles2.kt`/`CutOut3.kt` are shared algorithms, not extra ToolIds.

Source: `K/tools/{CutHolesTool,CutHolesTool2,CutHoles2,CutOut3Tool,CutOut3}.kt`, face-store polygon cutting. Acceptance: C19.

### T07 Sections and mesh intersection

**Observed.** `PLANE_SECTION`, `MESH_INTERSECTION`, `CUT_WITH_PLANE`. Plane Section creates intersection geometry from a specified plane. Mesh Intersection computes intersections between meshes/selected faces. Cut With Plane mutates geometry according to its picked plane/side interaction. These are modeling operations; a generic nondestructive viewport section-plane system from old intent docs is a different feature.

Share triangle/triangle and triangle/plane math, fragment identity/color handling and local/world conversion. Test no intersection, touching vertex/edge, coplanar overlap, transformed objects, open surfaces and undo. Document each tool's prerequisites/output in T24.

Source: `K/tools/{PlaneSectionTool,MeshIntersectionTool,MeshIntersectionMath,CutWithPlaneTool}.kt`. Acceptance: C19, C20.

### T08 Object cutting and solid booleans

**Observed.** `OBJECT_CUT`, `SOLID_UNION`, `SOLID_INTERSECTION`, `SOLID_SUBTRACTION`. Exactly two supported mesh object instances must share a parent context; voxel objects are rejected. Tools cut at the shared intersection, then booleans classify fragments. Object Cut keeps fragments; booleans choose union/intersection/difference. Results become loose selected geometry in the parent; source instances are removed on successful operation, rather than returning a new object definition.

Subtraction operand ordering must be fixed by a source-characterized test. Preserve face colors/reversed subtraction surfaces. Empty/non-overlapping/coincident/open/inconsistent-normal input needs characterized outcomes; never infer industrial CAD robustness from the names. Rust may improve invalid-input diagnostics and prevent partial loss, but changed outcomes must be recorded. No OpenSCAD dependency is required by current implementation.

Source: `K/tools/{ObjectCutTool,VolumeBooleanTool,MeshIntersectionMath}.kt`. Acceptance: C20.

### T09 Primitive meshes

**Observed.** `PRIMITIVE_SPHERE`, `PRIMITIVE_CYLINDER`, `PRIMITIVE_CONE`, `PRIMITIVE_PILL`. Tools create ordinary mesh geometry through staged center/radius/height picks, with circle segment settings supplying resolution where used. Preserve orientation and numeric semantics from each class. Test negative/zero dimensions and segment limits; verify seams/caps/normals and exportable closed geometry for normal inputs.

Source: `K/tools/PrimitiveTools.kt`. Acceptance: C19.

### T10 Mechanical tools

**Observed.** `MECH_SCREW`, `MECH_CIRCULAR_HOLE`, `MECH_ROUND_WASHER`, `MECH_COG_WHEEL`. These are built-in modeling tools, distinct from the deferred external Groovy plugin system. Screw consumes selected line profiles and picked center/height; washer/hole/cog have their own contour, numeric, subdivision and output logic. Do not invent standard thread or involute-gear engineering guarantees from names.

T26 must characterize input stages, pitch/tooth/profile/count interpretation, normals, selected result and hole-cut interaction. Reuse path/revolve/cut/primitive helpers where they match source. Validate mechanical examples and export round trips. Preserve editable source geometry when rejecting invalid profiles.

Source: `K/tools/MechanicalTools.kt`; mechanical fixtures/examples. Acceptance: C19, C20.

### T11 Fuzzy offset, scatter, and remeshing

**Observed.** `RANDOM_OFFSET`, `RANDOM_SURFACE_ARRAY`, `MESH_REGULARIZE`, `SURFACE_REMESH`. Offset moves connected selected vertices along averaged neighboring normals; strength is clamped 0..100. Surface array scatters a selected payload across selected target faces with count, normal alignment, scale fuzz and rotation fuzz. Planar Regularize creates a rectangular triangular grid on a near-planar patch; Surface Remesh is a separate mode for nonplanar surfaces.

Defaults: strength 1, scatter count 12, scale/rotation fuzz 0, normal alignment true, regularize subdivisions 12, planar tolerance 0.05. Payload/target selection and source replacement differ among operations. Rust accepts an explicit seed through structured automation for reproducible tests (proposed addition); normal UI may choose a fresh seed per commit and must save it with undo metadata. Test shared vertices, bounds, sample distribution, surface fidelity and cancellation, not identical nondeterministic coordinates.

Source: `K/tools/RandomTools.kt`; commits July 5–7. Acceptance: C21.

### P01 Voxels

**Observed.** `VOXEL`, `VOXEL_VOLUME`, `VOXEL_FRAME`. Voxel prototypes store integer local cell keys and per-cell color; cells are unit-sized and aligned with local axes. An instance itself may be fractionally translated/rotated. Tools auto-create/enter an appropriate group when needed. Volume fills the designated integer range; Frame fills its edges. Selected face voxelization and explode are actions outside the three ToolIds.

Preserve shared-face culling, selected voxel deletion/paint, integer snapping on cell transforms, group transforms, volume endpoint inclusion, and explode into ordinary mesh/lines. Do not serialize generated voxel faces as unrelated authoritative edits. Test one cell, adjacent cells, hollow frame, negative coordinates and rotated instances.

Source: `K/model/VoxelStore.kt`, voxel paths in `GroupScene.kt`, `K/tools/VoxelTools.kt`. Acceptance: C15, C22.

### P02 Walls, slabs, and holes

**Observed.** `ARCH_WALL`, `ARCH_SLAB`, `ARCH_ADD_HOLE`. Architecture tools use parameter stores and can auto-create/enter architecture context. Walls store endpoints, thickness, height, inclination, interior/exterior colors, rectangular named holes. Slabs store extents/axes/normal/thickness/colors and holes. Defaults: wall/slab thickness 0.2, wall height 2.7, wall inclination 0.

Selection includes semantic elements, wall/slab hole contours and corner/edge/center handles. Deleting a hole contour changes parameters/regenerates geometry; it is not arbitrary deletion of generated triangles. Preserve endpoint manipulation, joins and clipping behavior, hole bounds and interaction with rotated planes. Rebuild generated caches from canonical parameters after load, once per affected revision.

Source: `K/model/ArchitectureStore.kt`, architecture parts of `GroupScene.kt`, `K/tools/ArchitectureTools.kt`, `ArchitectureSettings.kt`. Acceptance: C22.

### P03 Stairs, window frames, and door frames

**Observed.** `ARCH_STAIR`, `ARCH_WINDOW_FRAME`, `ARCH_DOOR_FRAME`. Stairs retain contour, walking path/start/end, height, step count, support thickness, left/right rail settings, tread/support colors. Frames retain corners or contour, normal, depth, frame width, window/door kind, color, glazing enabled/color. Current frames can be free-form contours; restricting them to rectangles loses later functionality.

Defaults: stair height 2.7, 14 steps, support thickness 0.2, both rails on; frame depth 0.12, width 0.06. New-frame settings enable glazing, while persisted FrameDto defaults glazing off: preserve that distinction when loading omitted values. Support panel editing, generated mesh selection semantics, transforms, explode, and reopen.

Source: architecture store/tools/settings and architecture DTOs. Acceptance: C03, C22.

### P04 HVAC

**Observed.** `HVAC_PLUMBING`, `HVAC_VENTILATION`. Plumbing stores sampled path, diameter, side count, color; defaults 0.2 diameter and 16 sides. Ventilation stores endpoints, orientation reference, width/height, auto-join flag and hump span/clearance; defaults 0.5×0.25, auto-join false, hump half-span 0.625, clearance 0.05. Controls alter paths/duct ends and regenerate geometry.

Preserve turns/joins, control handles, automatic crossing/hump behavior where implemented, semantic selection/deletion, settings and per-element parameters. HVAC can be present on prototypes without a dedicated `HVAC` PrototypeKind enum member. Avoid inventing a new on-disk kind without a migration. Explode and interchange use resolved geometry; current IFC export is proxy geometry rather than semantic HVAC BIM entities.

Source: `K/model/HvacStore.kt`, HVAC paths in `GroupScene.kt`, `K/tools/{HvacTools,HvacSettings}.kt`. Acceptance: C22.

### P05 Hotspot definitions and attachments

**Observed.** Separate hotspot placement action; five shapes: triangle-up, square, circle, diamond, triangle-down. Operations: move, scale, stretch, rotate, multiply-linear, multiply-volumetric, multiply-rotate-2D, multiply-rotate-3D. Hotspots have ID/name/position/color, optional reference point, attached hotspot IDs, stable vertex IDs and segment/triangle references with quantized-coordinate fallbacks.

Panel actions create/edit/delete, pick/clear reference, attach selection, and select attached geometry. Definition editing changes prototype bindings. Attachment graph behavior and evaluation order need characterization, including chained/shared attachments, missing references, transformed parents, no-op operations and cycles. Add cycle detection/diagnostics in Rust so malformed graphs cannot hang evaluation. Do not change reference quantization accidentally during f64 migration.

Source: `K/model/HotspotStore.kt`, attachment/vertex identity paths in `GroupScene.kt`, hotspot settings callbacks in `Main.kt`. Acceptance: C15.

### P06 Hotspot instance evaluation

**Observed.** Instances store hotspot position/attachment overrides, resolved runtime positions, and optional line/face/dimension/text overrides. Definition-mode geometry and per-instance generated geometry are distinct. Moving a runtime hotspot recomputes from prototype source state; it must not cumulatively reapply transforms to the previous preview. Other instances using the same prototype remain independent unless editing the definition.

Use deterministic dependency evaluation with revisioned immutable inputs and a bounded/cancellable generation job. Persist enough state to restore instance behavior and old override data. Refresh xrefs and prototype edits invalidate dependent runtime geometry. Test two instances with different hotspot states, copy/explode, undo, reopen, and an array hotspot attached to another hotspot.

Source: `GroupScene.GroupNode`, hotspot recompute/apply helpers; `GroupInstanceDto`. Acceptance: C15, C16.
