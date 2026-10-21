# Meshcraft MCP

Meshcraft is a direct mesh modeler. Start its local server in the **MCP Server** settings
panel; the default URL is `http://127.0.0.1:8765/mcp`. Change the port, then Restart to
apply it. It starts stopped and closes with the application. Connect an MCP client using
Streamable HTTP at that URL. No cloud service, account or API key is required.

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

Coordinates are in **model units**, Y is up, +Z faces the Front view. A prototype owns local
geometry, scripts, hotspots and nested instances. An occurrence is a placed instance with a
numeric `path` of layout-entry IDs. An empty path `[]` addresses scene-root geometry.
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

## Transport and scope

POST `/mcp` (or `/`) with JSON-RPC. Stateless JSON responses support MCP 2025-03-26,
2025-06-18, 2025-11-25 initialization and 2026-07-28 per-request metadata/discovery.
The server binds IPv4 loopback only, checks Host/Origin and has bounded body size, connections,
queue and request timeouts. GET returns 405; standalone SSE subscriptions are not advertised.
Local clients without an Origin header can connect; browser origins must exactly match the
local endpoint. There is no network-wide listener or Groovy/REPL endpoint.
