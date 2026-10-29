# Live Meshcraft Store demonstrations

These scenes were modeled in an actual running Meshcraft app through its local
MCP server. They are not AI-generated screenshots or diagrams masquerading as UI.
The SDK one-shot scripts emit editable geometry into the application's document;
architecture and object parameters use the same MCP commands as native tools.

| Demonstration | Model construction | Saved live project |
| --- | --- | --- |
| Direct modeling | [direct.py](direct.py), [direct-modeling.js](direct-modeling.js): housing, flange, fasteners, 12-edge I-section swept through two right-angle turns, extruded concave profile | [01-direct-modeling.octd](../captures/01-direct-modeling.octd) |
| Dynamic instances | [dynamic.py](dynamic.py), [dynamic-pavilion.js](dynamic-pavilion.js): one shared prototype, three instances, independent span/height/finish values and a hotspot | [02-dynamic-instances.octd](../captures/02-dynamic-instances.octd) |
| Architecture | [architecture.py](architecture.py): canonical walls, owned openings, independent glazed frames, slabs, deck, steps, water and trees | [03-architecture.octd](../captures/03-architecture.octd) |
| JavaScript editor | Open the pavilion's real prototype source, with API v2 declarations and live preview | Same dynamic-instance project |
| GLSL editor | Open a Water material draft and its program; select the Fragment GLSL tab and capture the live preview | Same dynamic-instance project as the background; the draft is not saved |
| Materials | [materials.py](materials.py): brick mesh, wood mesh, water/grass/foliage slab domains with actual procedural assignments | [06-procedural-materials.octd](../captures/06-procedural-materials.octd) |
| GPU branching | [trees.py](trees.py): independent Broadleaf, Conifer and Weeping objects; show Broadleaf settings/preview | [07-branching-preview.octd](../captures/07-branching-preview.octd) |
| Stairs | [stairs.py](stairs.py): turning and straight contour/path stairs, materials and guardrail guides | [08-architectural-stairs.octd](../captures/08-architectural-stairs.octd) |

The initial modeling session used 0.45.1. All eight final Store screenshots were
recaptured in 0.45.2, including its corrected straight winder fronts. The saved
projects retain their original creation history; opening them regenerates the
current architectural geometry. Linux/Xvfb/Mesa captures do not certify Windows.

From the repository root, build the executable, then run:

```sh
cargo build --locked -p meshcraft-desktop --bin meshcraft
python3 scripts/capture_store_features.py
```

The script opens an isolated 1920 × 1080 desktop, connects through MCP, opens the
saved projects, sets selection/camera/editor state, and captures the actual app
window. The image API publishes the 3D canvas; native window capture adds the
real GPUI controls. No controls, marketing labels or frames are composited into
Store screenshots. The CLI requires Linux Xvfb, Mesa, xdotool and ImageMagick.
`--scene 04-javascript-editor` captures one view; `--rebuild-scenes` reconstructs
the projects through MCP before capturing. Script resource limits remain intact;
the housing is emitted as bounded components. The spatial sweep is built by the real Extrude Swipe 3D tool using [sweep-3d.py](sweep-3d.py).

Projects, screenshots, scripts and MCP action/receipt files stay under
`packaging/microsoft-store`, outside `target`. The ignored `captures/work/` folder
keeps intermediate working captures, and `captures/.runtime/` holds only private
runtime/cache data. Neither is required to rebuild the checked-in final assets.

The three JSON files beside these recipes contain world-space geometry queried
with `meshcraft_geometry`, including the model's face colors. They are inputs to
the axonometric promotional illustrator, **not** claimed to be renderer output.
The text-free hero uses those exact mesh shapes without app UI. Other promotional
layouts combine geometry illustrations with actual `meshcraft_capture_view`
images in [feature-renders](../assets/feature-renders/). Only lossless PNG encoding
was applied to those viewport inputs, with decoded-pixel equality checked.

[modeling-actions-0.45.1.json](../captures/modeling-actions-0.45.1.json) preserves
modeling calls and parameters from the original session. It is an action journal,
not a full protocol replay or a performance report; routine polls and response
payloads are omitted. [mcp-actions.json](../captures/mcp-actions.json) and
[capture-receipts.json](../captures/capture-receipts.json) describe the latest
capture run and image hashes. The executable can be rebuilt after `cargo clean`
without losing the authored models or images.
