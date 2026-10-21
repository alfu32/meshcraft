# Architecture tools

The Architecture ribbon contains Wall, Rectangular Slab, Freeform Slab, Add Hole,
Window and Door. The Architecture settings panel edits the selected element, or
sets defaults for new elements when nothing is selected.

![Architecture tools and independent frames](rewrite/references/architecture/overview.png)

- **Wall:** pick the first endpoint and subsequent chain points. Each span commits
  separately; Enter ends the chain. Thickness, independent start/end heights,
  signed axis offset, inclination and exterior/interior colors are editable.
  Autojoin is enabled by default: compatible walls sharing a baseline endpoint
  join when both owners enable it. Disabling either one regenerates their ends.
  Zero offset centers the wall on its axis; positive offset moves it toward its
  exterior normal.
- **Rectangular Slab:** pick two opposite corners. It creates a four-point closed
  polyline slab, using the picked surface/workplane normal.
- **Freeform Slab:** pick the boundary points, then press Enter or close onto the
  first point. A/C/L select the existing arc/circular/line path modes. A selected
  closed polyline can also supply the boundary directly.
- **Add Hole:** pick two opposite opening corners on a wall or slab. The first
  pick locks the owner; the hole belongs only to that element. The panel lists
  its openings and provides individual deletion buttons.
- **Window / Door:** pick two opposite frame corners, or use a selected closed
  contour. Each placement is an independent object with its own prototype.
  Dimensions, frame color and optional glazing are editable. These tools never
  create a wall/slab hole automatically; use Add Hole explicitly. The initial
  frame plane takes its normal from the cursor, including endpoint/line snaps.

Select a slab to show its blue profile controls. Drag a control on the slab plane
with live geometry and relative-distance feedback. Releasing commits one undoable
edit; Escape cancels the preview. Existing openings retain their positions while
its outline changes. Invalid/self-intersecting outlines are rejected.

![Selected slab and profile controls](rewrite/references/architecture/slab-profile-controls.png)

Wall/slab selections operate on canonical elements rather than generated facets.
Move, copy, arrays and clipboard placement retain their profiles and owned holes.
Windows/doors retain ordinary object placement/prototype editing behavior.
Explode materializes an object's current architecture geometry as ordinary mesh.
It also works directly on selected walls or slabs, replacing their canonical
parameters with their currently realized lines and triangles.

## Wall, opening and frame controls

Select one or several architectural elements to expose their controls:

- **Blue wall bases:** drag on the wall's vertical plane to adjust length and
  elevation. Shift-drag moves a base in plan. Moving a base preserves that end's
  height, so its top moves with it.
- **Magenta wall tops:** adjust each end's height independently of its base and
  the opposite end. Both bottom and top edges can slope.
- **Orange opening corners:** resize each wall/slab hole using two opposite
  corners on the owner's definition plane.
- **Purple frame corners:** resize the complete planar contour between opposite
  bounds, keeping its border width and depth.

Every drag previews privately, shows relative feedback, commits once on release,
and supports Escape, Undo and Redo. Visibility/editability and prototype context
still apply.

![Independent wall tops](rewrite/references/architecture-controls/unequal-wall-tops.png)

![Opening and frame controls](rewrite/references/architecture-controls/frame-controls.png)

Legacy import preserves inner-face wall axes through a half-thickness offset and
maps lower-face slab definitions to the canonical top profile. Its opening UVs
already use the normalized plane basis; they are retained without another flip.

[The controls example](../examples/architecture-controls.octd) contains joined
walls, unequal base/top levels, an offset, wall/slab openings and independent
frames with two different normals.

Open [the example project](../examples/architecture.octd) with:

```sh
cargo run -- --open examples/architecture.octd
```

Automation uses the same commands; see [the MCP guide](mcp.md#architecture).
