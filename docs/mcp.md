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
retain it. `this.hotspots` holds dragged overrides; use `hotspot(position, stableId)` to
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
  "source": "return [cuboid(new Point(), new Point(this.width, 2, 1), 'body', phong('#83bdeaff')), hotspot(new Point(this.width, 0, 0), 'width')];",
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
