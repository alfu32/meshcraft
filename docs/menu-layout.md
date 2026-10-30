# Functional workspace — 0.48.0

Menus and the searchable left command panel use the functional groups below.
Every visible command has one canonical menu/left-panel home. The ribbon has
its own projection of those sections and a Home tab of common shortcuts.

| Group | Functions |
| --- | --- |
| Files | New/open/recent/save, mesh import/export and IFC |
| Clipboard & History | Undo/redo, cut/copy/paste, duplication and deletion |
| Selection | Selection, clearing selection and selection properties |
| Construction | Linework, annotations, guides, transforms and input/settings |
| Meshing | Mesh construction, sweeps/extrusion, filling loops and advanced mesh tools |
| Modify | Mesh modification and arrays |
| Solids | Primitives, booleans, intersection curves, sections, mechanical and voxel tools |
| Civil | Architecture, HVAC and their defaults |
| Materials | Material management, painting, procedural materials and rendering |
| Objects | Prototypes/instances/groups, scripts, shader objects, baking and external prototypes |
| Hotspots | Placement, attachments, references, deletion and settings |
| Settings | Model/UI settings, cameras, profiles, MCP and help |

The left panel keeps its search input above the scrolling tool sections. Search
matches category, function name, command ID and legacy keywords, ignoring case;
all words must match. Empty sections disappear. Drag the panel's right edge to
resize it; the width is saved in UI preferences, with a 180–600 logical-pixel
range and space reserved for the viewport on narrow windows. Search and resizing
do not change the drawing or history. Buttons keep their usual availability.

## Ribbon

Home is the initial tab. It includes Select, Move, a selection material switch,
an object prototype insertion picker, Undo/Redo, Cut/Copy/Paste/Paste in Place,
Line, Polyline, Double Line, Rectangle, Circle, Polyline Mesh, 3D Mesh, Extrude,
Sweep, Move Copy, Rotate, Scale, Axis/Grid Guide, Hotspot, Cuboid and Sphere.
Each shortcut uses the existing command, icon and availability rules. Home
shortcuts intentionally also appear in their functional tabs.

Construction replaces the former 2D Construction name. Its ribbon also includes
the Selection section and transforms. Selection remains an early menu/left-panel
group; there is no separate Selection ribbon tab. Modify contains the former
Meshing Modify and Arrays sections. Other ribbon tabs retain their functional
sections. Long strips scroll horizontally without growing the ribbon vertically.

Home and Materials contain the same material picker. It displays the selected
entities' material or **Multiple materials** when their finishes differ. Picking
a material changes all editable selected entities in one Undo transaction.
With no selection it sets the current drawing/paint material. Material and object
pickers have searchable lists. Choosing a prototype starts its normal floating
insertion preview; geometry is placed only after picking an insertion point.

Paint treats a picked mesh or scripted object as its whole instance after
normalizing the hit to the current edit context. Its finish is instance data,
not a replacement mesh or a change to the shared prototype. Script reevaluation,
copying and save/reopen preserve it. Inside prototype editing, painting an authored
face/edge keeps that local editing scope. Architectural elements use their owned
surface material slots; hotspot colors remain independent of materials.
GPU-generated shader objects retain their generator settings; use Shader Object
Settings, or bake to mesh before assigning a surface finish.

Object Instance, Object Prototypes and Materials inspectors retain their lists,
properties and parameter editing. Action commands live in toolbars. Material
and prototype list double-click behavior, prototype deletion confirmation and
Files' Recent Files submenu remain available.

The command panel lists every registered command as `Category > Function Name`.
Search also includes command IDs and legacy keywords. Up/Down selects and scrolls;
Enter executes the selected entry, and clicking executes the clicked entry.
Unavailable commands stay visible with their usual context requirements.

The dropdown row starts with Files. The title displays
`Meshcraft x.y.z /path/to/filename.octd`, or `Meshcraft x.y.z Untitled`.

[functional_ui.rs](../apps/desktop/src/functional_ui.rs) defines canonical and
ribbon placements. The [tool catalog](tools/tools.csv) records canonical placement;
regeneration preserves all four user review columns by command ID. See
[workspace validation](rewrite/references/workspace-controls/README.md) and the
[README screenshot manifest](screenshots/manifest.json).
