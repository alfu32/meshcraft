# Live Meshcraft Store demonstrations

The current ten Store screenshots show nine complete projects modeled in a
running Meshcraft app through its local MCP server. They are native application
captures, with no composited controls, captions or marketing frames. The models
are editable design studies, not certified engineering designs.

| Store view | Project and construction | Saved project |
| --- | --- | --- |
| 1 · Direct modeling | Serviceable instrument controller: ventilated cover, folded enclosure, electronics, display, connectors and fasteners | [instrument.octd](../captures/projects/instrument.octd) |
| 2 · Dynamic instances | Fitted kitchen and island; eleven independently configured cabinet instances, appliances, sink, lighting and stools | [kitchen.octd](../captures/projects/kitchen.octd) |
| 3 · Architecture | Two-storey courtyard residence with native walls, slabs, owned openings, independent glazing frames, solar panels, landscaping and pool | [residence.octd](../captures/projects/residence.octd) |
| 4 · JavaScript editor | Configurable casework with shelves, doors, model inputs and a live cabinet preview | [casework.octd](../captures/projects/casework.octd) |
| 5 · Shader editor | Custom seeded GLSL branching rule with a live landscape canopy preview | [park.octd](../captures/projects/park.octd) |
| 6 · Materials | Furnished rooftop garden with procedural brick, wood, water, grass and foliage | [rooftop.octd](../captures/projects/rooftop.octd) |
| 7 · GPU object preview | Editable tree parameters and a branching canopy preview from the pocket park | [park.octd](../captures/projects/park.octd) |
| 8 · Stairs | Double-height furnished loft, native turning stair, mezzanine, glazing and guardrail guides | [loft.octd](../captures/projects/loft.octd) |
| 9 · Mechanical assembly | Twin-pump skid with motors, pipework, valves, tank, controls, grating and a spatially swept I-section service rail | [plantroom.octd](../captures/projects/plantroom.octd) |
| 10 · Technical workspace | Furnished home office with shelving, task chair, equipment, a dimension annotation and an orthographic axonometric view | [studio.octd](../captures/projects/studio.octd) |

All ten final images were captured in Meshcraft **0.48.5**. Linux/Xvfb/Mesa
captures do not certify the Windows runtime. The [project recipes](projects/README.md)
record their construction and physical-unit conventions.

## Reproduce the captures

From the repository root:

```sh
cargo build --locked -p meshcraft-desktop --bin meshcraft
# Open the saved projects and capture all ten views:
python3 scripts/capture_store_projects.py --capture-only
# Rebuild the projects through MCP, then capture all views:
python3 scripts/capture_store_projects.py
# Recapture one view:
python3 scripts/capture_store_projects.py --capture-only --shot 09-mechanical-assembly
```

The driver opens an isolated 1920 × 1080 desktop and an app-owned MCP server;
it does not replace a user's open drawing. It operates the actual GPUI editor,
selection, camera and preview controls before capturing the native app window.
Linux Xvfb, Mesa, xdotool and ImageMagick are required. Script resource limits
remain intact: larger assemblies are emitted as bounded components. The service
rail uses the actual spatial Sweep tool on a twelve-edge I-profile through two
right-angle turns. The loft uses the native contour/path stair tool.

`capture_store_features.py` remains a compatibility entry point: its default
recaptures these saved projects; `--rebuild-scenes` reconstructs them first.
Its `--refresh-readme` option also copies the corresponding eight native captures
to the stable README image paths. Explicit `--legacy-studies` selects the older
small demonstrations instead.

## Evidence and durable sources

[project-modeling-actions.json](../captures/project-modeling-actions.json) records
project creation; [mcp-actions.json](../captures/mcp-actions.json) records the
latest capture and finishing session. These are action journals, not complete
protocol replays or performance reports. Routine polling and response payloads
are omitted. [project-screenshot-evidence.json](../captures/project-screenshot-evidence.json)
records scene counts, main cameras and script-error checks. Editor previews use
their own camera, adjusted by native gestures. [capture-receipts.json](../captures/capture-receipts.json)
and [assets-manifest.json](../assets-manifest.json) record final image hashes,
app versions and sources.

Projects, recipes, final images and evidence stay under
`packaging/microsoft-store`, outside Cargo's `target`. Intermediate captures and
private desktop caches live in ignored `captures/work/` and `captures/.runtime/`.
`cargo clean` cannot delete the authored work.

The older small-study recipes and saved scenes remain for historical evidence
and promotional-art regeneration. Their original action journal is
[modeling-actions-0.45.1.json](../captures/modeling-actions-0.45.1.json). The three
geometry JSON files beside those recipes contain world-space geometry queried
through MCP, including face colors. They feed the promotional illustrator and
are not claimed as renderer output. Other promotional layouts combine those
illustrations with actual MCP viewport captures in
[feature-renders](../assets/feature-renders/), losslessly re-encoded with decoded
pixel equality checked. Promotional artwork is separate from the ten screenshots.
