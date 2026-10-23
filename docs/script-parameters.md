# Script model parameters

Object scripts declare model parameters with `$TEXT`, `$NUMBER`, `$CHECKBOX`, `$LIST` and
`$HOTSPOT`. The Object Properties panel renders these declarations. Their identifiers are
bold magenta in the editor and lead Ctrl+Space suggestions; hover shows the same help published
through MCP. The former `hotspot()` helper is removed; use `$HOTSPOT()`.

New editor scripts use **API v2**. Existing scripts retain API v1's instance argument; use the
editor's API selector to switch deliberately. Through MCP, pass `api_version: 2` to
`meshcraft_object_script` (or `meshcraft_run_script` for a one-shot module).

```javascript
// Outside hooks: values shared by every instance of this prototype.
$TEXT('caption', 'Caption', 'Cabinet');
$NUMBER('depth', 'Depth', 1);
$LIST('finish', {plain: 'Plain', blue: 'Blue'}, 'Finish');

function onCreate() {
  // Inside onCreate: independent values belonging to each instance.
  $NUMBER('width', 'Width', 3);
  $CHECKBOX('enabled', 'Enabled', true);
  $HOTSPOT(new Point(2, 2, 0), 'size', 'Size');
}

function onAnimationFrame(params, model) {
  const { caption, depth, finish, width, enabled, size } = params;
  if (!enabled) return [];
  return [this.geometry,
    cube(new Point(), new Point(width + size.position.x, size.position.y, depth),
      this.id + ':body', phong(finish === 'blue' ? '#498bd6' : '#dddddd'))];
}
```

| Declaration | Default | Frame value |
| --- | --- | --- |
| `$TEXT(id, label=id, defaultValue='')` | Empty string | String |
| `$NUMBER(id, label=id, defaultValue=0)` | Zero | Finite number in internal model units |
| `$CHECKBOX(id, label=id, defaultValue=false)` | False | Boolean |
| `$LIST(id, values, label=id, defaultValue=firstKey)` | First declared choice | Selected string key |
| `$HOTSPOT(position, id, label=id, shape=$HOTSPOT_SHAPE_CIRCLE, color="#00ffffff")` | Supplied Point | Hotspot object with `id`, `kind`, `position` |

`values` maps stable keys to display labels. The default must be a valid key. Pass `undefined`
for an omitted label when supplying only an explicit default, e.g. `$NUMBER('width', undefined, 3)`.
Declarations can be placed in helper functions: their scope follows the current execution phase,
not the helper's source-code location. Defaults initialize missing values; compatible initialized
and edited values survive ordinary frames and source edits. Use stable IDs; changing a label does
not rename a parameter. Duplicate IDs across either scope and hotspot declarations are errors.
Use distinct IDs, even if a prototype and an instance parameter have similar purposes.

The first API v2 frame argument is a read-only combined parameter object; its argument name can
be `params`, `parms` or any valid JavaScript identifier. The current instance remains `this`;
`model` is the second argument and remains query-only. Store ordinary mutable animation state
on `this`, not in `params`. `onCreate(instance, model)` still receives the instance, and runs once
per initializer. Its input declarations are retained for later frames without rerunning it.

Text/number/checkbox/list declarations are forbidden inside `onAnimationFrame`. `$HOTSPOT` is
also available there as an explicit geometry helper: return its result with the generated geometry.
Only hotspots declared outside hooks or during `onCreate` are injected into `params` and emitted
automatically. A declared hotspot need not be returned explicitly. Its position incorporates the
current instance's drag override, independently of whether the declaration is prototype-scoped.
Scripted hotspot operations are used with prototype editing closed, as in the normal object workflow.

Object Properties separates **Instance parameters** and **Prototype parameters · shared**.
Instance edits apply to selected instances of the same prototype; mixed values show an empty input
placeholder. Prototype edits update all instances. Each edit is validated and undoable; an invalid
batch leaves every target unchanged. Copy retains instance values; Save/Open retains declarations,
values and script API version. Hotspot positions are modified by their viewport handles.
Script errors retain the last good geometry and can be corrected by another parameter/source edit.

MCP exposes the exact same model access through `meshcraft_parameters`:

```json
{"action":"get"}
{"action":"set","definition_id":42,"id":"depth","value":0.5}
{"action":"set","definition_id":42,"paths":[[100],[101]],"id":"width","value":4}
```

Omit `paths` for shared prototype values. Use occurrence paths from `meshcraft_scene` for instance
values. Discover declarations/types/defaults from the current selection's returned forms before
writing. Generated schema and authored values are stored separately from arbitrary script state;
rendered property controls do not introduce another source of truth.


## Hotspot appearance

```javascript
function onCreate() {
  $HOTSPOT(new Point(3, 2, 0), 'size', 'Size',
    $HOTSPOT_SHAPE_DIAMOND, '#ffccddee');
}
function onAnimationFrame(params) {
  return cube(new Point(), params.size.position, 'body');
}
```

| Injected constant | Marker |
| --- | --- |
| `$HOTSPOT_SHAPE_CIRCLE` | Circle (default) |
| `$HOTSPOT_SHAPE_RECTANGLE` | Rectangle, rendered as a screen-sized square |
| `$HOTSPOT_SHAPE_TRIANGLE` | Upward triangle |
| `$HOTSPOT_SHAPE_DIAMOND` | Diamond |
| `$HOTSPOT_SHAPE_TRIANGLE_DOWN` | Downward triangle |

Completion, highlighting and hover help include all shape constants.
Omit shape/color for a cyan circle. Color accepts `#RRGGBB` or `#RRGGBBAA` (alpha
last); the default is `#00ffffff`. Pass `undefined` for an omitted label or shape
when supplying later arguments. Unknown shapes and invalid colors report a script
error and retain the last good geometry.

Appearance applies to top-level, `onCreate` and explicit frame hotspots. Declared
appearance is retained across frames, copy and Save/Open, and is available on
`params[id].shape` / `params[id].color`. Selection temporarily uses the normal
selection color. Dragged positions remain absolute across generated frames, so a
stationary drag does not alternate between the declared position and its target.


IFC creation metadata is declared with `$IFC` and read through `this.ifc`; unlike
shared form parameters, prototype IFC defaults are copied into each new instance.
See [IFC properties](ifc-properties.md). Only the corrected `$HOTSPOT_SHAPE_*`
constant spelling is supported; the former misspelled aliases are removed.
