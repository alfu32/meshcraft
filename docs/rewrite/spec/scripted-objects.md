# Scripted prototype configuration points

### P08 Scripted object geometry and editor

**New user requirement (2026-10-20).** Scripting augments every object prototype, including prototypes retaining graphically authored geometry and nested instances. It does not introduce a separate incompatible object type. Instances share source hooks but retain their own serializable properties and hotspot values. Copying an instance preserves these values.

Expose two configuration points initially:

- `onCreate(instance, model)`: create additional instance properties; `this` is bound to the same instance. Initialize once per instance for the current initializer, including an existing instance newly given a script. An explicit initializer-source change reinitializes affected instance properties; ordinary frames do not rerun it.
- `onAnimationFrame(instance, model)`: return an array of drawables, allowing nested arrays from geometry helpers. `this.geometry` exposes the object's graphically authored geometry; the default hook returns it. Model access is query-only. Stable user IDs identify points, segments, faces and hotspots across frames.

The unified source defines `function onCreate(instance, model)` and `function onAnimationFrame(instance, model)`. Older separately stored function bodies are combined automatically when opened. User pseudocode with typed arguments is a contract description; editor examples use valid JavaScript. The model facade supplies materials, object/geometry queries and frame timing without exposing mutable Rust/model pointers, filesystem, network, Node or DOM services. The existing engine direction remains WebKit JavaScriptCore; [runtime/plugin boundaries](../plugins.md) still apply. Object configuration is now desktop scope independently of the full plugin manager/API.

Provide `Point(x,y,z,id)`, `Material(color[,texturePath])`, `Color`, `Segment(p0,p1,id[,material])`, `Face(p0,p1,p2,id[,material])`, plus `line`, `face`, `hotspot`, `phong` and `gouraud`. Geometry helpers include `polyline`, `circle`, `cube`, `sphere` and `cone`, returning arrays of entities. Document exact helper signatures with runnable examples. Geometry is emitted in prototype-local coordinates; instance placement applies afterward. Script-created material values use the same color/texture/shading schema as the project catalog.

Scripted hotspots appear on closed instances and drive regenerated geometry through their per-instance values. Stable IDs retain their values when the hook emits a new frame. Prototype editing isolates authored geometry as before; external geometry remains visible for snapping. Script output is derived and is not destructively written over the prototype's authored geometry. Explode replaces an instance with its current issued geometry, transformed into the containing scene, with no script dependency. Per-frame output does not create an unbounded undo history.

The mini IDE has one monospace JavaScript editor on the left and an independent preview on the right. Both hooks live in the same script definition; developers can define shared helper functions, classes and constants outside the hooks. Ctrl+Space opens a scrollable API/help completion list at the cursor, with signatures, field types, explanations and insertion. Preview supports orbit, pan and zoom. Save, Preview and Cancel remain visible. Draft evaluation and camera navigation do not modify the source model; Save commits the prototype configuration; Cancel/Escape discards it. Exceptions, invalid output and execution limits produce visible diagnostics and leave the document usable. Run JavaScript away from the UI/render loop, with bounded output, interruption and clean disposal. Preserve native renderer performance and avoid leaked runtime processes, contexts or popup windows. The preview uses the existing offscreen raylib worker, not a second conflicting raylib global context.

Source: new user explanation. Acceptance: C43. Implementation: T62; shared JSC embedding evidence also informs T47. Further optimization is a later iteration.

## Initial SDK

The source editor accepts a complete JavaScript script defining both hooks. `onAnimationFrame` is required; `onCreate` is optional. Shared helper functions and constants can be declared outside them.
Arguments are `instance, model`; `this` and `instance` refer to the same object.
Use serializable properties such as `this.height`; functions, cyclic objects,
engine handles and closures are not persisted. `this.time`/`model.time` are elapsed
seconds and `this.deltaTime`/`model.deltaTime` are frame intervals. Source and
per-instance state are saved in the project; generated geometry is a disposable
cache. Returning `this.geometry` retains authored faces/segments, their colors and
material links. Nested object instances retain the prototype hierarchy.

```javascript
const bodyMaterial = phong("#83bdeaff");

function onCreate(instance, model) {
  this.height = 2;
  this.width = 3;
}

function onAnimationFrame(instance, model) {
  const width = hotspot(new Point(this.width, 0, 0), "width");
  return [this.geometry,
    cube(new Point(0, 0, 0),
         new Point(Math.max(0.1, width.position.x), this.height, 1),
         "body", bodyMaterial),
    width];
}
```

Supported signatures (coordinates/radii/heights/sizes are model units):

| Function | Result |
| --- | --- |
| `new Point(x=0,y=0,z=0,id="")` | Point with optional stable vertex ID |
| `new Color(red=255,green=255,blue=255,alpha=255)` | RGBA channels, 0–255 |
| `new Material(color="#ffffffff",texturePath=null)` | Color/shading/texture value, U/V sizes default to 1 |
| `phong(color)`, `gouraud(color)` | Material with the specified shading metadata |
| `new Segment(a,b,id,material)`, `line(a,b,id,material)` | Segment; unassigned line color defaults to dark |
| `new Face(a,b,c,id,material)`, `face(a,b,c,id,material)` | Triangle |
| `hotspot(position,id)` | Handle whose position reflects this instance's override |
| `polyline(points,id,material,closed=false)` | Segment array |
| `circle(center,radius,id,material,segments=32)` | Closed path on the local XZ plane |
| `cube(origin,size,id,material)` | Faces and boundary edges; size is a Point or scalar |
| `sphere(center,radius,id,material,segments=24)` | Triangle array |
| `cone(origin,radius,height,id,material,segments=32)` | Triangle array along local +Y |

Colors accept `#RRGGBB`, `#RRGGBBAA`, Color objects or Material values. Helpers
prefix child entity IDs with their supplied ID. Duplicate entity/handle IDs,
conflicting coordinates for one Point ID, nonfinite geometry and oversized
results are rejected. Material color/alpha is rendered in this phase; shading and
texture values are retained as metadata.

The frozen model facade has `objects`, `materials`, `settings`, `time` and
`deltaTime`; `getObject(id)`, `findObjects(name)`, `getMaterial(id)` and
`queryGeometry(id)` query those records. Object records include stable occurrence
IDs, prototype IDs, names, local drawables and world transforms. Queries do
not mutate the source document. No Node, DOM, filesystem or networking functions
are injected into the JavaScriptCore global object.

## Runtime and platform status

Evaluation runs in a private child process, outside GPUI and the raylib worker.
JavaScriptCore interrupts an invocation after 40 ms; the parent applies a two-second
watchdog and Linux RSS budget of 512 MiB. Results are limited to 100,000 drawables,
32 nesting levels and 16 MB JSON. Failed hooks retain their last good geometry and
show diagnostics; source/model edits allow retry. Each scene sweep sends one query
snapshot to the helper through buffered IPC. The SDK source stays constant instead
of recompiling it with an embedded scene/request literal. The VM is shared within
that sweep, with fresh globals per
instance; only an immutable JSON string crosses contexts. Scene queries parse and
freeze that string on first use, so local generators do not process unrelated
geometry. Contexts are released after each invocation; the VM is replaced for the
next sweep and after script errors. The library remains loaded until helper exit.

A sweep uses one scene/time snapshot and publishes partial results in approximately
50 ms work slices, advancing to the remaining instances on the next request.
Reaching the slice budget is normal scheduling, not an error: even a slow sweep
must reach its last instance. An individual failed instance retains its last good
output without preventing healthy instances from animating. Source/model changes
discard the previous continuation. `deltaTime` measures time between sweep starts,
rather than UI polling intervals. The watchdog remains per helper operation; there
is no aggregate two-second rejection based solely on instance count.

Linux loads JavaScriptCoreGTK 4.1, 6.0 or 4.0; the old-glibc release bundle includes
its 4.0 dependency closure. macOS uses the system JavaScriptCore framework.
Windows expects a compatible JavaScriptCore DLL: native runtime redistribution
and Windows/macOS live qualification remain explicit T47/T62 follow-ups. Missing
engines produce a visible editor/runtime error; no alternate engine or network
runtime download is attempted. Browser execution remains outside this native
object-scripting increment.

## Editor API discovery

Press Ctrl+Space inside the editor, or use **API · Ctrl+Space**, to display a
scrollable list of the built-ins, hooks, `this`/`instance` fields and `model` query
methods. Type a prefix such as `model.` or `this.` to filter, use Up/Down and
Enter/Tab to insert, or click an entry. Escape dismisses the list before closing
the editor. Each entry includes its signature, return/field type and explanation.
Per-instance custom properties from the preview are included with their current
JSON type/value. The source editor uses the platform's monospace font.

`this` and the first `instance` argument are aliases. Host-provided inputs are
`id`, `geometry`, `hotspots`, `time` and `deltaTime`; these reserved fields are
refreshed for each frame. Assign your own JSON fields in `onCreate` and update
them in `onAnimationFrame`; they persist per instance. Top-level definitions are
evaluated with the current frame in this first runtime; use instance properties
for state that must survive between frames. The prototype configuration and query
facade do not expose document mutation methods.

## Construction and transform helpers (0.9.0)

The JavaScript scope includes functions drawn from the
[OpenSCAD function reference](https://openscad.org/cheatsheet/), expressed in
Meshcraft coordinates with Y up and degree angles. This is a JavaScript helper
API; OpenSCAD source files and its CSG language are not evaluated.

`rectangle(p0,p1,id?,material?)`, `cuboid(p0,p1,id?,material?)`, `polyline(points,id?,material?,closed=false)`
and `circle(center,radius,resolution=12)` create ordinary drawables. Rectangles use
two opposite corners sharing a coordinate plane; rotate the output for other
orientations. Optional IDs default to deterministic call-order IDs. The earlier
`circle(center,radius,id,material?,segments)` form remains supported.

`translate(delta)(items)`, `scale(sizes)(items)`,
`rotate(origin,up,degrees)(items)` transform a Point, drawable, or nested list
without mutating inputs. The final items argument may also be supplied directly.
Scale accepts a scalar or a Point. Rotation also accepts Euler XYZ degrees or
`rotate(degrees,axis)`. `mirror(normal)` and `multmatrix(matrix4x4)` compose in the
same way. Transforms retain entity/point IDs; `withIds(items,prefix)` creates
independent identities when originals and copies are emitted together. Negative
scales/reflections correct triangle winding. Hotspot overrides remain in final
instance coordinates.

Additional construction helpers include `square`, `polygon` (simple planar
concave outlines), indexed `polyhedron`, `cylinder` with optional top radius, and
`linear_extrude` of an XZ profile along Y. `color(tint)(items)` applies a material;
tints accept hex, basic named colors, Color, or normalized RGBA arrays. Vector
helpers include `dot`, `cross`, `norm`, `normalize`, `distance`, `add`, and
`subtract`. Degree trigonometry, common scalar math, list/string/type helpers,
linear `lookup`, and optionally seeded `rands` are available. Ctrl+Space lists the
functions and examples. Standard JavaScript Math and Array operations remain
available.
