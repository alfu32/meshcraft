# Project materials

### P07 Material catalog and color rendering

**User requirement (updated 2026-10-23).** Each project exposes a Materials inspector, initially populated with 16 named ANSI colors and four architecture materials. Users can create additional materials, see their color swatches and names, select them, and open a material editor by double-click or Edit. Preserve text labels on controls.

A material has a stable ID, name, RGBA color including transparent alpha, shading type (Unlit, Flat, Gouraud or Phong initially), optional texture path, and positive finite texture U/V sizes in model units. Defaults are an opaque neutral color, Phong shading, no texture and unit U/V sizes. The first rendering phase uses material colors/alpha with existing viewport lighting; texture mapping and distinct shading algorithms are retained metadata, not implemented rendering promises.

Save validates and commits one undoable edit. Cancel/Escape discards the draft. Modal pointer, wheel and keyboard input cannot reach modeling tools or camera navigation. Apply to Selection binds editable selected edges/faces or selected instance content to the material, respecting prototype editing context and independent instances. Later edits to a referenced material update its displayed color. Direct color painting replaces the material reference. Existing imported colors remain intact until explicitly assigned; new model geometry uses separate line and face defaults. A material catalog and its references survive project IO and history; scene import remaps conflicting material IDs without recoloring the host catalog.

Source: new user contract; existing [object workflow](objects.md), paint and project/history APIs. Acceptance: C42. Implementation: T61. Texture evaluation remains future work.

New drawings start with 16 named ANSI materials using conventional VGA RGB values.
ANSI defines [16 color slots](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences#text-formatting),
while RGB mappings are configurable; Meshcraft uses a fixed initial palette.
`ansi-grey` is #c0c0c0, `ansi-dark-grey` #808080 and `ansi-white` #ffffff. The other
slots are ansi-black/red/green/yellow/blue/magenta/cyan and their bright variants.
Default linework uses ansi-black; default faces use ansi-grey. Script Segment/line
and Face/face helpers use matching black and grey defaults. Explicit assignments
and imported authored colors retain their values, including old white faces.

Every drawing also receives smoke-white (#ededed), smoke-grey (#d6d6d6), side-grey
(#e0e0e0) and glass (#b8d6f266). Wall exterior, slab top and frame defaults use
smoke-white; wall interior/slab bottom use smoke-grey; slab sides use side-grey;
glazing uses glass. Existing catalogs migrate once; deletion persists across IO.

The Materials manager has a bounded, left-aligned scrollable list and a Delete
button, enabled only for unused materials. Delete is a single undoable document
operation, also available as `material.delete` with an `id` argument. References
in all definitions (including unplaced prototypes), instance geometry replacements,
nested instance defaults, evaluated scripts, architecture records and architecture
creation defaults count as use. Implicit line/face/architecture defaults count as
use even before geometry is created. Unused ANSI colors and custom materials can
be deleted, with no forced replacement or recoloring of geometry.
