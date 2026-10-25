# Meshcraft MCP

Meshcraft is a direct mesh modeler. Start its local server in the **MCP Server** settings
panel; the default URL is `http://127.0.0.1:8765/mcp`. Change the port, then Restart to
apply it. It starts stopped and closes with the application. Connect an MCP client using
Streamable HTTP at that URL. No cloud service, account or API key is required. The Settings ribbon has a shortcut to this panel.

## Discover before operating

1. Call `k3d_contract` and `k3d_status`.
2. Call `k3d_list_commands` with a query such as `line`, `object` or `material`.
   Read each command's `inputSchema`, current state and disabled reason.
3. Read `meshcraft_scene` for prototype IDs, occurrence paths, settings and materials.
   Read `meshcraft_geometry` for exact primitive IDs and transforms.
4. Use `k3d_execute_command` with `commandId` and `arguments`. It uses the same
   command dispatcher and undo transactions as buttons. Disabled/unimplemented operations
   fail explicitly. `accepted` is distinct from completed: poll `meshcraft_get_job` for a
   pending job ID. Inspect the document generation/revision before retrying a lost response.

Coordinates and numeric settings are in **internal model units**. Displayed
lengths multiply the internal length by `unit_size` and append `unit_name`; a grid
step measures `grid_size * unit_size`. For `unit_size=30`, `grid_size=0.5` and
`unit_name="cm"`, one grid step displays `15 cm`. No stored geometry or numeric
command parameter is rescaled. `snap_radius` is a screen-pixel radius.

Y is up, +Z faces the Front view. A prototype owns local
geometry, scripts, hotspots and nested instances. An occurrence is a placed instance with a
numeric `path` of layout-entry IDs. An empty path `[]` addresses scene-root geometry.
Imported scenes can have a wrapper occurrence with a nonempty path; `is_scene_root` identifies
that record. Its own mesh counts exclude its child instances.
World point = the occurrence's `transform` applied to a local point. Selection targets returned
by geometry queries can be passed unchanged to `meshcraft_select`. External geometry in
prototype edit mode stays visible/snappable but cannot be selected or changed.

Normal Paste starts a floating bundle; pick its insertion point. Paste in Place inserts
immediately. Escape cancels the active tool first. Enter finishes multipoint paths.

### First construction

Call `k3d_execute_command` with `{"commandId":"tool.builtin.line"}`, then call
`k3d_pointer_event` twice:

```json
{"action":"click","worldX":0,"worldY":0,"worldZ":0}
{"action":"click","worldX":3,"worldY":0,"worldZ":0}
```

Use `{"action":"move","worldX":4,"worldY":0,"worldZ":1}` for live tool feedback.
Use `{"action":"finish"}` to finish a polyline/path or `{"action":"cancel"}` to press
Escape. To save, discover `file.save_as` and provide an absolute `path`; this avoids a file dialog.
Use `edit.undo` to undo an edit. Open/save/import can return a pending job.

## Script model parameters

Use `api_version: 2` with `meshcraft_object_script` to declare `$TEXT`, `$NUMBER`, `$CHECKBOX`,
`$LIST` and `$HOTSPOT` model parameters. Top-level calls declare shared prototype properties;
`onCreate` calls declare independent instance properties. Read combined values through
`onAnimationFrame(params, model)`, with the instance still bound to `this`. `$HOTSPOT` replaces
the old unprefixed helper. `meshcraft_parameters` gets the current selection's typed property forms
or sets a shared prototype value / values for explicit instance paths in one Undo transaction.
See [declarations, persistence and exact examples](script-parameters.md).

## JavaScript objects

`meshcraft_script_api` and `meshcraft://scripting/api` publish the **same signatures,
descriptions and examples** as Ctrl+Space in the script editor. `meshcraft://scripting/sdk`
contains the exact injected implementation. Use `meshcraft_object_script` to create, get or
set a prototype's unified source. Read back the prototype/occurrence IDs after creation.

```javascript
function onCreate(instance, model) {
  this.height = 2;
}
function onAnimationFrame(instance, model) {
  return [this.geometry,
    cuboid(new Point(0, 0, 0), new Point(3, this.height, 1), 'body', phong('#83bdeaff'))];
}
```

Both hook arguments `instance` and `this` are the same per-instance object. Custom JSON
properties persist per instance. `this.geometry` is authored local geometry; return it to
retain it. `this.hotspots` holds dragged overrides; use `$HOTSPOT(position, stableId)` to
expose instance handles. Stable, unique drawable IDs retain topology across evaluations.
Transforms accept Points, drawables or nested arrays without mutating inputs. Use `withIds`
when returning an original and a transformed copy together. SDK trigonometry/rotation uses
degrees; standard `Math` trigonometry uses radians. `circle` lies on XZ; cylinders/extrusions
run along Y. Inspect helper signatures rather than assuming complete OpenSCAD compatibility.

`model` is a read-only scene snapshot: `objects`, `materials`, `settings`, `time`, `deltaTime`,
`getObject(id)`, `findObjects(name)`, `getMaterial(id)`, `queryGeometry(id)`. Each object record
has `id`, `name`, `definitionId`, `transform` and LOCAL `geometry`. Model settings use snake_case.
Object IDs in this JS facade are path strings; native automation uses numeric path arrays.
Scripts have ordinary ECMAScript built-ins and the documented SDK. They cannot write directly
to the document or access Node, DOM, filesystem or network APIs. Execution runs in the bounded
JavaScript worker, never on the UI/render threads. Script failures preserve the previous frame.

## One-shot geometry scripts

Call `meshcraft_run_script` with a hook **body**:

```json
{
  "source": "return [cuboid(new Point(), new Point(this.width, 2, 1), 'body', phong('#83bdeaff')), $HOTSPOT(new Point(this.width, 0, 0), 'width')];",
  "properties": {"width": 3},
  "name": "Generated part",
  "position": [0, 0, 0]
}
```

The result contains `job_id`. Poll `meshcraft_get_job` until `result.completion.state` is
`completed` or `failed`; do not re-run to check completion. The worker uses the same SDK,
geometry validator and immutable `model` facade as object scripts. `this.geometry` starts
empty; `this.id` is `mcp-once`, time/deltaTime are zero and supplied JSON properties initialize
`this`. For a full module containing shared code and both hooks, set `source_type` to `module`.
The initializer and frame hook each run once. Only one one-shot may run at a time.

Default `commit:true` inserts the returned geometry and hotspots as a **static object**, in one
undo step. There is no continuing per-frame script cost. Coordinates are world model units,
translated by optional `position`; the object is owned by the active editing context. The result
includes counts, bounds, resulting instance targets and final custom properties. `commit:false`
only evaluates and reports those values. A source error, timeout, cancellation or document/edit
context change leaves the model untouched. Use `meshcraft_cancel_script` or Stop/Restart to
cancel; closing the app cancels too. Source is limited to 256 KiB, initial properties to 64 KiB;
existing JS time, memory and output limits apply. Completed one-shot job history is bounded to 128 entries; final properties larger than 64 KiB
are omitted from summaries, diagnostics and logs are length-limited.

Use `meshcraft_capture_view` to receive a PNG image and frame/revision metadata after an edit.
It captures the 3D canvas, including helpers and wireframe feedback. GPUI text labels/panels
are separate. Retry a rendering-pending error, or request `allow_stale:true` explicitly.
Query geometry for exact numeric verification; images supplement the model data.

If an Open/New command returns pending and `k3d_status.pendingDialog` is non-null, explicitly
call `meshcraft_resolve_document` with `save`, `discard` or `cancel`. Provide an absolute save
path for an unnamed document. This continues the existing replacement job; poll its ID.

### Selection and transforms

A target example is `{"path":[44,42],"kind":"segment","id":"17"}`; an object target is
`{"path":[44,42],"kind":"occurrence","id":null}`. `meshcraft_geometry` returns these targets
verbatim. Primitive IDs are decimal strings because generated IDs can exceed JavaScript's safe
integer range; keep them as strings. Occurrence paths and prototype IDs are different identities.
`transform` is an affine object with a 3×3 `linear` matrix and a `translation` Point; each world
coordinate is a row of `linear` dotted with the local point, plus that translation component.
To use SDK `multmatrix`, append translation as column four and `[0,0,0,1]` as the final row.

For instance hotspots, read the occurrence's `hotspots` list, select a target such as
`{"path":[44,42],"kind":"semantic_handle","semantic_kind":"hotspot","id":"width"}`,
then execute `hotspot.target` with `{"target":{"x":4,"y":0,"z":0}}`. This target uses the
instance's **local** coordinates; leave prototype editing before operating an instance handle.
By contrast, `hotspot.add.position` takes a world Point object. Discover command schemas to
distinguish Point objects from three-number arrays.

### Architecture

Use `tool.builtin.arch_wall`, `tool.builtin.arch_slab` (Rectangular Slab),
`tool.arch_slab_freeform`, `tool.builtin.arch_add_hole`,
`tool.builtin.arch_window_frame` and `tool.builtin.arch_door_frame` for point input.
Enter finishes a wall chain or closes a freeform slab; A/C/L switch path modes.
The slab tools both produce a closed polyline profile. Native selection exposes
blue draggable profile controls; `architecture.move_slab_vertex` is the equivalent
world-coordinate command. Slab profile points may have unequal heights. Native
dragging moves these controls freely in 3D, identically with or without Shift;
commands also accept the full 3D position without planar projection. Wall bases
use the same movement: real geometry/grid/guide snaps first, otherwise parallel
to the active grid at the grabbed point's elevation. The dragged owner's solid
cannot capture its own controls. Native dragging previews the owner's wireframe and
commits the solid edit once on release. The profile normal is the extrusion axis; vertical sides and owned
opening reveals follow it. Sloped planar roofs retain their normal thickness;
at folds the underside preserves at least that thickness. Opening controls
follow the interpolated roof surface while their stored UVs remain fixed.

For direct creation, discover `architecture.create_wall`,
`architecture.create_rectangular_slab`, `architecture.create_slab` and
`architecture.create_frame`. Supply `start`/`end` or `points`, plus a plane `normal`
where relevant. Frame `kind` is `window` or `door`. Dimensions use model units;
colors are `{r,g,b,a}` with normalized channels. `architecture.settings` edits
persisted defaults; `architecture.update` edits the selected owner or the supplied
`id` and optional `occurrence:{path:[...]}`.

`architecture.create_stair` takes world-point arrays `contour` (closed boundary,
without a repeated final point) and `walking_path` (open, ordered uphill), plus
optional `normal`. Dimensions are `stair_height` (default 10), `stair_steps`
(1–512), `stair_support`, `stair_spacing` (positive) and `stair_levels` (0–128).
`stair_left` and `stair_right` toggle guide sides. `architecture.stair_contour`
captures the current selection as C; `architecture.stair_path` uses the current
selection as P and creates the stair. Inputs can be segment sets or occurrences.
`architecture.move_control` accepts `{"kind":"stair_height"}` with a world
`position` to edit height. All stair settings also work with `architecture.update`
and `architecture.settings`; guide spacing, dimensions and positions use model
units. `stair_tread_material` and `stair_support_material` are material IDs.

Architecture material parameters are IDs, for conventional or procedural catalog
entries: `wall_exterior_material`, `wall_interior_material`, `slab_top_material`,
`slab_bottom_material`, `slab_side_material`, `frame_material`, `glazing_material`,
and the stair slots above. They work for selected matching owners, explicit
owner IDs and new-element defaults, and survive regeneration/save/reopen.

`meshcraft_geometry.architecture` returns canonical records, semantic selection
targets and editability. Derived face/edge targets resolve to their owner; edit
parameters rather than the generated cache. `architecture.add_hole` takes an owner
`id` and two world points `start`/`end`; `architecture.remove_hole` also takes
`hole_id`. Openings belong exclusively to that wall/slab. Windows and doors are
independent object instances and never cut a wall or slab implicitly.

Wall records also expose default-on `autoJoin`, signed `offset`, independent 3D
`start`/`end` base endpoints and optional `endHeight` (falling back to `height`).
Flat command properties are `wall_auto_join`, `wall_offset`, `wall_height` and
`wall_end_height`. Compatible shared endpoints join when both owners enable it.

Each `meshcraft_geometry.architecture` item publishes a `controls` array with
local `point`, `normal`, optional drag axis/alternate plane, and a copyable
`control` descriptor. Convert its point through the occurrence transform for
world coordinates. Send the descriptor with a **world** `position` to
`architecture.move_control`, for example:

```json
{"id":"<owner id>","control":{"kind":"wall_top","end":true},"position":[5,4,0]}
```

Other descriptors are `wall_base` with `end:false|true`, `slab_vertex` with
`index`, `hole_corner` with `hole_id` and `corner:0|1`, and `frame_corner` with
`corner:0|1`. All four wall control positions lie on the axis plane, regardless of offset or
inclination. Window/door input uses the cursor’s blue up vector for the exterior
plane normal. Wall base edits preserve that end's height; top edits alter only
its height. Opening/frame controls resize on their owner's definition plane.
`object.explode_selection` also accepts canonical wall/slab/frame selections,
replacing them with their realized lines and triangles in one undoable edit.

## Transport and scope

The transport follows [MCP Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http).
POST `/mcp` (or `/`) with JSON-RPC. Stateless JSON responses support MCP 2025-03-26,
2025-06-18, 2025-11-25 initialization and 2026-07-28 per-request metadata/discovery.
The server binds IPv4 loopback only, checks Host/Origin and has bounded body size, connections,
queue and request timeouts. GET returns 405; standalone SSE subscriptions are not advertised.
Local clients without an Origin header can connect; browser origins must exactly match the
local endpoint. There is no network-wide listener or Groovy/REPL endpoint.

### Squash and rotation input

Discover `tool.squash` in the command catalog. Select geometry, activate the tool,
then pick center, reference point defining an axis, and target axis distance.
Alternatively enter a signed scale factor after the reference. Squash scales only
the component along that axis and preserves perpendicular components; zero is
rejected. It uses the regular tool pointer/numeric input, cancel and undo commands.
Rotation tool measurements use the first center, retain the chosen center-relative
rays, and report vector-pair angles plus the projected applied rotation angle.

### Applying properties to selections

`architecture.update` without `id` applies the supplied typed parameters to all
matching selected owners. `wall_*` fields affect walls, `slab_*` affect slabs,
and `frame_*`/`glazing_*` affect frames. Mixed payloads can update multiple types;
unrelated selected geometry is ignored. `name` is common to architectural owners;
a generic `record` patch uses the representative architectural type. Supplying
`id` (and `occurrence` where needed) keeps an explicitly addressed single-owner
edit. The result includes representative `id`, changed `ids`, and `updated` count.
An all-unchanged patch reports zero without adding history. A batch validates
atomically and creates one Undo entry; visibility, editability and prototype
context rules still apply. Specific control-point/opening commands remain addressed
to their owner.

`hotspot.update` applies supplied metadata fields to every selected hotspot in
one transaction, preserving each position, attachments and reference. Without a
selected hotspot it retains the inspector/default workflow. Instance operation
values still belong to individual instances and are operated through their
hotspot target/drag commands.

## Mesh interchange

`interchange.formats` returns the complete format/subset and unit guide. `import.mesh`
creates a prototype and starts placement; an optional `position:[x,y,z]` inserts an
instance immediately in the active context. `export.mesh` and `export.ifc_model` accept
`path`, optional physical `unit`, positive extra `scale`, and `selection_only`. Export
defaults to the selection when nonempty; set `selection_only:false` for the whole model.
Poll the job before treating the output as complete. `interchange.cancel` cancels work;
failed/canceled exports retain the previous destination. Results include geometry counts
and loss/unsupported-feature warnings. See [interchange guide](interchange.md).


Scripted hotspot appearance uses `$HOTSPOT(position, id, label, shape, color)`.
`meshcraft_script_api` publishes the shape constants and full optional defaults;
`meshcraft_geometry` returns the resulting marker shape and RGBA color. See the
[appearance API](script-parameters.md#hotspot-appearance).

## Procedural materials (#30)

`meshcraft_scene.proceduralPrograms` publishes every program, its parameter
signatures/defaults/ranges/descriptions and shader functions. `meshcraft.grass.v1`
is built in and read-only. A normal material may add:

```json
{"id":"my-grass","name":"Grass","color":{"red":65,"green":130,"blue":35,"alpha":255},"procedural":{"program":"meshcraft.grass.v1","parameters":{"density":30,"height":0.8,"width":0.06,"curvature":0.3,"seed":42,"wind":0.15,"variation":0.25},"lodDistance":100,"maxInstances":50000,"showBounds":false}}
```

Pass this as `material` to `material.save`, select surfaces/instances, then call
`material.apply` with `id`. Optional `baseMaterial` references a conventional
catalog material for color/alpha, texture dimensions and PBR settings
(`pbr:{roughness:0.6,metallic:0}`). Apply different instance values with
`material.parameters`, arguments `{"id":"my-grass","parameters":{"height":2,"seed":7}}`.
This applies to the editable selection; sibling instances keep their values.
`material.apply` clears selection overrides to inherit the material defaults.
Architectural assignments retain values through regeneration and project IO.

Use `material.program` with a `program` object to save a custom reusable program.
Fields: `id`, `name`, `generation` (`surface` or `blades`), `backend`
(`vertex_instancing_gl33`), `parameters`, `vertex`, `fragment`, `boundsExpansion`,
`animated`, `transparent`. Each parameter has `id`, `kind`, `default`, optional
`label`, `description`, `min`, `max`, `step`, `unit`, and `options` for enum choices.
Kinds: `float`, `integer` (signed 32-bit), `unsigned`/`seed` (unsigned 32-bit),
`boolean`, `vec2`, `vec3`, `vec4`, `color` (four 0–1 channels), `matrix` (16 values,
column-major), `texture` (file path), `enum` (option key). Shader uniforms are
`u_<id>`; `time` is seconds. Texture parameters bind samplers (up to eight).

Vertex source defines
`void proceduralVertex(inout vec3 position,inout vec3 normal,vec2 uv)` in local
source coordinates. Fragment source defines
`vec4 proceduralColor(vec4 color,vec3 position,vec3 normal,vec2 uv)` with world-space
position/normal, before built-in lighting. Sources use GLSL 330; `eye` and
`towardLight` are also available in the fragment scope. A `surface` program emits
the source triangles with its deformation/shading; `blades` starts with instanced
strips. Declare sufficient `boundsExpansion` for custom displacement, and
`transparent:true` if the fragment function emits blended alpha. Set `animated`
only when time affects the program. Texture paths are absolute or relative to the
process working directory.

`meshcraft_capture_view` reports `procedural.diagnostics`, candidate counts and
GPU draw counts. Unsupported/missing programs and compile failures retain base
surfaces and report diagnostics. Generated details are visual-only: geometry
queries, snapping, mesh export and Explode operate on the authored source mesh.
The native path uses OpenGL 3.3 instancing; compute/mesh shader declarations fall
back explicitly. Candidate limits and LOD bound work; a domain with more triangles
than its candidate budget needs a larger budget or a simpler distribution mesh.


Additional built-ins are `meshcraft.water.v1`, `meshcraft.brick.v1` and
`meshcraft.wood.v1`. Inspect their parameter schemas in `meshcraft_scene`.
Water uses `surface_grid` generation (bounded GPU subdivision), while brick/wood
shade existing triangles with local-coordinate patterns and optional shading
relief. `localPosition`/`sourceNormal` and optional
`vec3 proceduralShadingNormal(vec3 normal,vec3 position,vec2 uv)` extend fragment
hooks without changing older programs.

`object.configure_procedural` creates or updates an independent tree owner with
`settings` shaped like Material and `procedural.program="meshcraft.tree.v1"`.
Omit its optional IDs to create a prototype and start insertion (returns
`definition_id`); supply `definition_id` for shared defaults or `instance_path`
for private occurrence settings. `meshcraft_scene` reports each owner's
`proceduralObject`, classification and conservative bounds. Trees have no CPU
mesh triangles. Select/copy/transform their occurrence targets; generated leaves
and branches cannot be individually snapped, exported or Explode-baked.
See [preset workflow and limitations](procedural-presets.md).


`meshcraft.foliage.v1` uses GPU `foliage` generation on source triangles. The
published schema includes density, size, width, height, spread, tilt, curl, wind,
variation, seed, leafColor and alternateColor. Spread is a local tangent-plane
radius that scatters leaves beyond source boundaries. Assign and override it
through the existing material commands. All shader `color` parameters now use
native color pickers in the UI; model/MCP values remain normalized RGBA arrays.

## Automated desktop launch and timing

`meshcraft --mcp-port 18766` explicitly starts the localhost server at launch.
Without this option it remains stopped until started in the settings panel.
The [performance runner](performance/README.md) uses an isolated configuration,
cache and display; it never attaches to a user's session.

`k3d_status.history` reports the native history cursor and entry count (not imported
legacy history). Successful tool results add `_timing.ownerThreadMs`, measured on
the UI owner thread before response serialization/transport. Background worker and
GPU time are separate. `meshcraft_capture_view.renderFrameMs` reports the raylib
offscreen frame's measured rendering/readback work. Neither value is an end-to-end
GUI latency guarantee or pure GPU timer. World-point input also supports hotspot
placement and reference picking while the main tool remains Select.
