# Functional workspace — 0.46.1

Menus, ribbon tabs and the left command panel share these functional groups in
order. Each visible action has one home within each surface. The left panel is a
continuous scrolling list of function sections, without group selector tabs.
Menu section headings and ribbon/left subpanels share command definitions.

| Group | Functions |
| --- | --- |
| Files | New/open/recent/save, mesh import/export, IFC and other file operations |
| Clipboard & History | Undo/redo, cut/copy/paste, duplication and deletion |
| Selection | Selection, clearing selection and selection properties |
| 2D Construction | Linework, annotations, guides and input/settings |
| Meshing | Mesh construction, sweeps/extrusion, transforms, filling loops, mesh modification and arrays |
| Solids | Solid primitives, booleans, intersection curves, planar sections, mechanical and voxel tools |
| Civil | Walls, slabs, owned holes, stairs, windows/doors, HVAC and architectural defaults |
| Materials | Material manager, painting/assignments, procedural materials and rendering/settings |
| Objects | Prototypes/instances/groups, scripts, shader objects, baking and external prototypes |
| Hotspots | Placement, attachments, reference points, deletion and settings |
| Settings | Model/UI settings, cameras, workspace profiles, MCP, tutorials and help |

Object Instance, Object Prototypes and Materials inspectors retain their lists,
property fields and parameter editing. Creation, editing, deletion, placement,
baking and material assignment commands live in the toolbars. Double-clicking a
material still opens its editor; double-clicking a prototype still places it.
Prototype deletion uses the existing confirmation when instances are affected.

The command panel lists every registered command, including compatibility and
parameterized automation commands. Entries display `Category > Function Name`;
search matches that label, command IDs and legacy keywords without case
sensitivity. `Archi` includes `Architecture > Wall` and the other architectural
commands. Up/Down selects and scrolls the results; Enter executes the selected
entry, and clicking executes the clicked entry. Unavailable commands stay visible
with their availability state; they require their usual context or arguments.

The dropdown row starts with Files. The application name appears in the window
title as `Meshcraft x.y.z /path/to/filename.octd`, or `Meshcraft x.y.z Untitled`
for an unsaved drawing. Opening, saving under a new name and starting a drawing
update both the native title and the visible title bar.

The authoritative placement is [functional_ui.rs](../apps/desktop/src/functional_ui.rs).
All three surfaces resolve its command IDs directly. The [catalog](tools/tools.csv)
records the live placements; original edited metadata is preserved in
[reviewed-tools-0.45.3.csv](tools/reviewed-tools-0.45.3.csv). Regeneration preserves
all four review fields by command ID. Compatibility commands remain registered
without duplicate toolbar buttons. Files retains its Recent Files submenu.

[Native validation](rewrite/references/command-panel/README.md) covers category
search, keyboard and mouse dispatch, scrolling, priorities, the left panel and
window titles. Store artwork and presentation screenshots await the next UI step
at the user's request; existing published images are retained.
