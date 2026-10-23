# Numeric input during tools

While a tool is picking points, type directly. The pending value appears in the
feedback/status text. Backspace removes the last character and Enter accepts the
input as the next point. There is no text selection or caret navigation; Delete
and arrow/Home/End keys do not edit the model while a value is buffered. Escape
cancels the tool draft. Menus, the palette and focused property/code editors keep
their own keyboard input.

| Input | Result |
| --- | --- |
| `5` | Point 5 internal units from the relevant reference, in the current pointer direction |
| `15cm` | Same construction with a physical length, converted through the model unit settings |
| `2,3` | Relative point in the cursor's red/green construction-plane axes |
| `2,3,4` | Relative world X/Y/Z offset |
| `30,60 cm` | Relative plane offset; the trailing suffix applies to both components |
| `1m,20cm,3` | Per-component units; the unsuffixed component remains internal |
| `90deg` / `1.5707963267948966rad` | Final rotation angle (bare final rotation numbers are degrees) |
| `2` at the final Scale/Squash step | Scale factor, retaining the tool's reference-axis constraints |

Bare distances remain internal values. For example, with unit name `cm`, unit size
30 and grid size 0.5, `1` means 1 internal unit (displayed as 30 cm), while `15cm`
means 0.5 internal units. The grid size does not alter the parser's conversion.
Recognized physical suffixes include nm, µm/um, mm, cm, m, km, in, ft and yd, plus
common singular/plural names. A suffix matching the configured unit name also
works for custom units. Converting between different physical names requires a
recognized physical model unit name. Unknown units and nonfinite values report
an error and leave the buffer available for correction.

The reference is the tool's accepted point, or its center for center-based
transforms. Primitive height input uses the base center. A 2D basis is captured
when typing starts, so it does not rotate while the pointer moves. A first point
entered as coordinates is relative to the world origin.

With an empty buffer, Enter retains its normal role: finish a multipoint path or
mesh chain. With a buffered value, it accepts one point and keeps the path going.

## Copy repetitions

While previewing the final destination of Move Copy or Rotate Copy, enter:

- `x12`: **12 new copies**, in addition to the source, at multiples of the picked
  increment. Rotation repeats its signed angle; scale copies compound the factor.
- `/12`: **12 new copies** evenly dividing the picked span or angle, including
  its endpoint. Scale division interpolates evenly from the original size to the
  target size.
- `2m x12`: combine an exact step distance and repetition.
- `90deg /3`: three copies at 30°, 60° and 90°.

Scale and Squash also support Copy Mode. Pick their reference points, enable
Copy Mode, then use `xN` or `/N` at the final preview. Counts must be whole numbers
from 1 to 10,000. The preview displays at most 64 copies. All copies are computed
from the source selection and committed in one Undo transaction; invalid or
singular transforms commit nothing. Existing instance parameters/IFC values follow
ordinary copy behavior. Enter repetitions before accepting the destination; this
is not a post-commit array-editing mode.

## MCP

`k3d_pointer_event` uses the same buffer and transaction paths:

```json
{"action":"move","worldX":10,"worldY":0,"worldZ":0}
{"action":"text","text":"2m x12"}
{"action":"backspace"}
{"action":"finish"}
```

`finish` is Enter, `backspace` removes a character (or undoes a point when the
buffer is empty), and `cancel` is Escape. `k3d_status.numericInput` exposes the
pending text. Inspect errors before retrying; invalid input remains pending.
