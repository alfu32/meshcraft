# Hotspots, annotations and objects

Status: **done, released as 0.52.0.** Baseline: 0.51.0.

## Request

- Transforming hotspots (move, rotate, copy etc.).
- Groups/objects: cannot explode groups with hotspots.
- Adding hotspots, dimensions and text to objects/groups.

## What was wrong (reproduced before fixing)

| Action | Result |
| --- | --- |
| Move, Rotate or Scale a hotspot | "transform does not support semantic selection kind Hotspot" |
| Move Copy, Rotate Copy, arrays, Copy | Disabled for a selected hotspot |
| Explode a group or object holding a hotspot | "contains domain content without an explode adapter" |
| Make Object from a hotspot, alone or with mesh | "group object instances or supported mesh, dimension and text selections separately" |
| Make Object from an object and loose text, dimension or hotspot | Same error |
| Add a hotspot, text or dimension inside an open object or group | Already worked; now covered by tests |

## Behavior

| Point | Decision | Reason |
| --- | --- | --- |
| Transforming a hotspot | Marker and reference point move; attachments stay on their geometry | Same as dragging a marker while authoring |
| Copying a hotspot alone | Keeps its attachments, with a new ID | Two handles may drive one mesh; the copy is immediately usable |
| Copying a hotspot with its geometry | The copy drives the copied geometry | Otherwise the copy would deform the original |
| Copied default names | `HOTSPOT_n` gets a new number; chosen names are kept | Distinct default names in the list |
| Transforming a closed instance's hotspot | Sets that instance's value, as a drag does; definitions unchanged | The spec's closed-instance rule |
| Copying a closed instance's hotspot | Refused, asking to open the object | A closed instance holds values, not definitions |
| Explode | Hotspots move to the parent with new IDs and rebound attachments, like text and dimensions | Keeps the authored content |
| Exploding a dragged instance | Its mesh is baked and the marker stays where it shows | The value is already in the mesh |
| Exploding script-generated markers | Not kept | They are generated output, not authored content |
| Exploding under a closed parent | Refused, asking to open the parent | Same rule as annotations |
| Make Object with hotspots | Selected hotspots move in; attachments to geometry left behind are dropped | An object's hotspots can only drive its own geometry |
| Make Object with objects and loose content | Objects become children; loose content belongs to the new object itself | Its hotspots then show and operate on the new object |
| Paste | Each paste gets hotspots with new IDs | Hotspot IDs are unique per owner |
| Escape inside an open object | Cancels an active tool and stays; a further Escape leaves the object | User decision, so adding can continue inside the object |

## Selected hotspot feedback

Request: "when editing a hotspot (hotspot is selected) render the following: its reference point,
highlight the attached geometry."

| Point | Decision | Reason |
| --- | --- | --- |
| Reference point | White disc with a cross, a line to the marker, and a "Reference" label | Distinct from the marker shapes |
| Attached geometry | Hotspot's own color with a dark casing; faces also get a translucent fill | Ties the highlight to its handle and reads over any surface |
| Depth | Drawn over the scene, like markers | Attachments behind an object stay visible |
| Linked hotspots | Ring around their markers | They are attachments too |
| No attachments | Highlights the vertices at the marker | Those are what evaluation drives |
| Closed instances | Same feedback, at the instance's current shape | "Selected" applies to both |

## Reported scenario

Draw geometry and a hotspot, select them all, and press Make Object (screenshot: 0.51.0, "group object
instances or supported mesh, dimension and text selections separately").

## Done when

- Each row of the first table works in the real application, undoes in one step, and keeps world positions.
- The release gate passes.
