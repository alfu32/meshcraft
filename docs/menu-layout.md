# Functional workspace — 0.46.0

Menus, ribbon tabs and the left command panel share eight groups in this order.
Each visible action has one home within each surface. The left group selectors
stay above the scrolling tools; selecting a group also selects its ribbon tab.
Menu section headings and ribbon/left subpanels use the same command definitions.

| Group | Functions |
| --- | --- |
| 2D Construction | Selection, clipboard/history, linework, annotations, guides and input/settings |
| Meshing | Mesh construction, sweeps/extrusion, transforms, filling loops, mesh modification and arrays |
| Solids | Solid primitives, booleans, intersection curves, planar sections, mechanical and voxel tools |
| Civil | Walls, slabs, owned holes, stairs, windows/doors, HVAC and architectural defaults |
| Materials | Material manager, painting/assignments, procedural materials and rendering/settings |
| Objects | Prototypes/instances/groups, scripts, shader objects, baking, external prototypes and hotspots |
| Files | New/open/recent/save, mesh import/export, IFC and other file operations |
| Settings | Model/UI settings, cameras, workspace profiles, MCP, tutorials and help |

General selection and clipboard tools also work on 3D entities; the 2D group is
their single requested home. Context-specific inspector controls remain beside
their properties. Unimplemented commands stay disabled rather than claiming a
new implementation. Settings, selection, tools and document data keep their
existing command/state/transaction paths.

Aliases such as Select/Window Select, Frame/Window and Group/Make Object no longer
produce repeated toolbar items. Group and object creation share the implemented
object model, so Make Object is the single visible creation command. Explode also
covers ungrouping. Windows and doors retain distinct commands. Old command IDs,
shortcuts and typed automation endpoints remain registered and searchable in the
command palette/MCP; parameter-only APIs are not presented as unusable buttons.
Old names remain palette search terms. Files retains its nested Recent Files menu.

The authoritative placement is [functional_ui.rs](../apps/desktop/src/functional_ui.rs).
All three surfaces resolve its command IDs directly, rather than rebinding several
button names to one operation. The [catalog](tools/tools.csv) records the live
placements; your original edited metadata is preserved unchanged in
[reviewed-tools-0.45.3.csv](tools/reviewed-tools-0.45.3.csv). Regeneration preserves
all four review fields by command ID.

[Native evidence and validation](rewrite/references/functional-menus/README.md)
cover all eight groups, the Civil/Files menus and GUI tool dispatch. README and
Store screenshots were recaptured from the real app through the saved MCP
workflow. [Stable screenshot paths](screenshots/manifest.json) are retained.
