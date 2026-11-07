# Functional workspace — 0.51.0

System-wide commands and command search sit on the title bar. The dropdown menus
and the searchable left command panel use the functional groups below, and the
ribbon has its own composition of those sections. Every registered command stays in
command search and MCP. The dropdown menus are hidden by default; UI Settings > Show
menu bar turns them on, and a saved setting is kept.

## Title bar

After the logo and the Home toggle, in order, as icon buttons with tooltips:

| Group | Commands |
| --- | --- |
| Files | New Drawing, Open, Recent Files, Save, Save As, Cancel Open, Exit |
| Import & Export | Import Mesh, Export Mesh, Export IFC, Export SVG, Save Screenshot, Cancel Import / Export |
| Clipboard & History | Undo, Redo, Cut, Copy, Paste, Paste in Place, Duplicate Selection, Delete Selection |

The title follows and truncates first on narrow windows. Unavailable commands are
dimmed. Recent Files opens a list of recent projects. The buttons stay when the
menu bar, ribbon or left panel is hidden. They are disabled while a modal editor
or prompt is open, and only the Home toggle shows on the Home screen.

## Command search

A search box, search button and chevron are right-aligned on the title bar, before
the window controls. Clicking or typing in the box opens the list below it, which
filters as you type. The search button shows the matches for the box's text, and the
chevron shows or hides the list. Up/Down select, Enter runs the selected command and
Escape closes the list and clears the text. Clicking elsewhere closes the list and
keeps the text. Ctrl+Shift+P opens the list with an empty box, or closes it. The box
shrinks on narrow windows. It works on the Home screen and is disabled while a modal
editor or prompt is open.

## Menus and left panel

| Group | Functions |
| --- | --- |
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

Files and Clipboard & History remain the canonical category of their commands in
command search paths and the tool inventory, without a dropdown menu or left panel
section.

The left panel keeps its search input above the scrolling tool sections. Search
matches category, function name, command ID and legacy keywords, ignoring case;
all words must match. Empty sections disappear. Drag the panel's right edge to
resize it; the width is saved in UI preferences, with a 180–600 logical-pixel
range and space reserved for the viewport on narrow windows. Search and resizing
do not change the drawing or history. Buttons keep their usual availability.

## Ribbon

| Tab | Groups, in order |
| --- | --- |
| Home | Selection, Draw, Annotations, Transform, Guides, Selection Material and Insert Object pickers |
| Construction | Construct, Advanced, Modify, Arrays |
| Solids | Primitives, Booleans & Sections, Mechanical, Voxels |
| Civil | Architecture, Stair Inputs, HVAC, Settings |
| Materials | Catalog, Library, Shader Programs, Rendering, material picker |
| Objects & Hotspots | Prototypes, Instances & Groups; Scripts & GPU; External Prototypes; Hotspots |
| Settings | Input & Settings, Model & Interface, Camera, Workspace Profiles, Automation & Help |

Home is the initial tab. Long strips scroll horizontally
without growing the ribbon vertically. Group headings stay on one line.

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
properties and parameter editing. Contextual management actions sit beside the
items they manage: material creation/editing/deletion/application below the material
list; prototype creation, script/shader editing, insertion, deletion and external
reference management below the prototype list; editing, exploding and baking next
to instance properties. Catalog lists scroll within a bounded area so their actions
stay adjacent even in drawings with many items. These use the same command handlers, availability rules,
and deletion safeguards as their menu/toolbar shortcuts. General modeling tools
remain in toolbars. Material and prototype list double-click behavior, prototype
deletion confirmation and the Recent Files list remain available.

Hotspot Settings likewise keeps Place Hotspot, Add at Cursor, Attach Selection,
Select Attached, Pick Reference, Clear Reference and Delete Hotspot beside the
selected hotspot's properties and attachment/reference summary. The existing
prototype-editing and root-scene authoring rules still control availability.
See [hotspot panel validation](rewrite/references/hotspot-panel/README.md).

Command search lists every registered command as `Category > Function Name`. It
also matches command IDs and legacy keywords. Clicking an entry runs it. Unavailable
commands stay visible with their usual context requirements.

The dropdown row starts with Selection. The title displays
`Meshcraft x.y.z /path/to/filename.octd`, or `Meshcraft x.y.z Untitled`.

[functional_ui.rs](../apps/desktop/src/functional_ui.rs) defines canonical and
ribbon placements. The [tool catalog](tools/tools.csv) records canonical placement;
regeneration preserves all four user review columns by command ID. See
[title bar validation](rewrite/references/title-bar/native-validation.json),
[workspace validation](rewrite/references/workspace-controls/README.md),
[contextual inspector actions](rewrite/references/panel-actions/README.md) and the
[README screenshot manifest](screenshots/manifest.json).
