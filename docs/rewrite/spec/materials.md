# Project materials

### P07 Material catalog and color rendering

**User requirement (updated 2026-10-23).** Each project exposes a Materials inspector, initially populated with 16 named ANSI colors and four architecture materials. Users can create additional materials, see their color swatches and names, select them, and open a material editor by double-click or Edit. Preserve text labels on controls.

A material has a stable ID, name, RGBA color including transparent alpha, shading type (Unlit, Flat, Gouraud or Phong initially), optional texture path, and positive finite texture U/V sizes in model units. Defaults are an opaque neutral color, Phong shading, no texture and unit U/V sizes. The viewport renders material colors/alpha with existing viewport lighting, and the texture image as specified in [Textures](#textures-t77). Distinct shading algorithms remain retained metadata, not implemented rendering promises.

Save validates and commits one undoable edit. Cancel/Escape discards the draft. Modal pointer, wheel and keyboard input cannot reach modeling tools or camera navigation. Apply to Selection binds editable selected edges/faces or selected instance content to the material, respecting prototype editing context and independent instances. Later edits to a referenced material update its displayed color. Direct color painting replaces the material reference. Existing imported colors remain intact until explicitly assigned; new model geometry uses separate line and face defaults. A material catalog and its references survive project IO and history; scene import remaps conflicting material IDs without recoloring the host catalog.

Source: new user contract; existing [object workflow](objects.md), paint and project/history APIs. Acceptance: C42. Implementation: T61; textures T77.

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

### Textures (T77)

**User requirement (2026-11-05).** A conventional material can show an image.

- **Picking.** The Material Editor's texture field is a file input. Its Browse button opens the
  system file dialog that Open and Save use, filtered to PNG, JPEG, WebP, BMP and TGA. A file in or
  below the project's folder is stored relative to the project; any other file, or any file picked
  before the project is saved, keeps its full path. Relative paths resolve against the project's
  folder. Clear removes the image. Procedural `texture` parameters use the same input.
- **Size.** Width × height of one image tile, in model units (`textureScaleU`, `textureScaleV`).
- **Colorization.** `textureTint`, 0 to 1. 0 shows the image as it is; 1 multiplies it by the
  material color. The color's alpha keeps meaning opacity: an opaque material cuts out pixels
  whose image alpha is under one half, and a translucent one blends them.
- **Mapping.** Each face projects onto its own plane in its prototype's space, so the image stays
  put on an object that moves. U runs horizontally to the right as the face is seen and V runs up
  it; a face within 0.999 of horizontal uses X for U. `textureOffsetU/V` place a tile's lower-left
  corner along those axes. The tile repeats in both directions, upright. The viewport shader and
  `texture_mapping.rs` evaluate the same formulas.
- **Loading.** Images decode off the render thread and draw at most 4096 pixels on a side, with
  mipmaps. A face whose image is missing or still loading shows its material color, and the
  Materials panel reports an unreadable file.
- **Edit Texture Position** (`material.edit_texture`). Clicking a textured face shows one tile of
  its image on that face, with a handle at each corner and one in the middle. Dragging a corner
  resizes the tile while the opposite corner stays put; dragging past it reverses the span without
  mirroring the image. Dragging the middle slides the image. A drag follows snaps that lie on the
  face's plane and otherwise the plane under the pointer. Releasing saves the size and offset to
  the material, which changes every face using it, as one undoable edit. Escape cancels a drag in
  progress, then ends the mode; choosing another tool ends it too. `material.edit` accepts an `id`
  naming the material to open.
- Interchange export still reports textures as omitted.



A conventional material may carry an optional procedural configuration: stable
program ID, typed parameter values, optional conventional/PBR base-material
reference, LOD distance, candidate budget and bounds display. Shared program
resources define generation mode, backend requirements, shader functions,
parameter schemas, conservative bounds expansion, animation and transparency.
Parameter resolution is program defaults, material values, then surface/instance
assignment overrides. The existing material assignment interface and independent
instance/prototype editing semantics apply; architectural overrides belong to
canonical owners and survive their regeneration. Assignment and program changes
are validated and undoable. Import remaps resource, material and base references.
Unknown backend/program data round-trips and cannot prevent unrelated geometry
from loading; render it with an explicit diagnostic and conventional fallback.

The first native mechanism is OpenGL 3.3 vertex instancing. A grass reference
material exposes density, blade height/width, curvature, seed, wind and color
variation. Geometry is generated on the GPU from source surface triangles, with
stable seeded distribution, cached source buffers, transformed expanded bounds,
culling, density LOD and configurable work limits. Program changes compile/cache
GPU resources on the rendering owner; parameter/time changes reuse geometry.
Opaque generated details participate in shadows/depth, and alpha uses blending.
Texture inputs and optional PBR base references are evaluated by procedural
shaders; conventional viewport texture/shading promises above remain unchanged.

Generated details do not enter the persistent mesh, CPU picking/snapping,
collision, mesh export or Explode. Interaction uses the source owner. The material
editor exposes declarations, parameter controls and read/apply/reset of selection
values. Custom shader definitions have separate monospace metadata, vertex and
fragment editors and real compiler/linker diagnostics. Apply commits while keeping
the dialog open; Cancel/Escape discards only edits not yet applied. The program
editor has its own explicit Save action. Idle animation runs only while a used,
visible program requests it. Compute/mesh shader generation is a later #29 phase;
independent procedural objects and tree/bush examples belong to #31.


### Shader preset and procedural owner extension — #29/#31, T64

Include Water, Brick and Wood material creation actions beside Grass. Water uses
GPU subdivision of the source triangles, animated wave displacement/normals,
transparent color, Fresnel sky approximation and editable wave/offset/detail
controls. Brick uses local model coordinates for running-bond courses, mortar,
seeded variation and shading relief. Wood uses local solid coordinates for growth
rings, fibers, adjustable longitudinal axis and shading relief. These patterns
must follow object transforms, stay continuous across coplanar source triangles,
and preserve source topology. Relief is shading, not geometric displacement.

New Tree in Object Prototypes creates a reusable independent procedural owner,
with no authored vertices/lines/faces. The GPU generates tapered branch cylinders
and cutout leaf cards. Height, crown/trunk dimensions, branching, foliage, seed,
wind, colors, LOD and work budget are editable at prototype or instance scope.
Existing instances inherit prototype defaults until given private settings;
copying preserves private settings. Ordinary object transforms, hierarchy,
visibility, owner/bounds selection, wireframe placement/transform feedback,
copy/paste, history and project persistence apply. Procedural leaves/branches are
not individual CPU pick/snap targets, mesh-exportable or Explode-baked geometry.
Bounds remain selectable/visible with diagnostics if the program is unavailable.
Reuse the shared program/resource/parameter machinery; mesh shaders and compute
remain later work. The tree deliverable is a slice of #31, which also asks for a
bush and broader source-domain/LOD qualification.

### Procedural object settings preview — #29, T65

The procedural object settings dialog has independently scrolling settings on the
left and a live GPU preview of the object on the right. Keep Apply, Cancel and
Save visible. Preview the effective prototype or private instance settings in an
isolated scene, without cloning the full drawing or changing its geometry,
selection, camera or history. Debounce valid draft updates; invalid input shows
the error and retains the last valid preview. Orbit, pan, zoom and Fit View affect
only the preview camera. Returning from the shader program editor restores the
parent preview and draft. Closing the dialog releases preview resources and
resumes the drawing; zero-wind previews must let rendering idle. Apply and Save
retain their existing commit scopes. MCP viewport capture can explicitly request
the active editor preview for visual verification; its revision metadata continues
to describe the drawing rather than the draft.


### Shader parameter pickers and foliage — #29/#30, T65

Every declared `color` parameter uses the existing color picker in both shader
material and procedural object editors, including custom programs. HSL, RGB and
swatches expose opacity. Convert to normalized RGBA internally; opening/saving
an untouched color must retain the original stored values. Parameter Reset and
Read Selection Values synchronize the picker and backing value. Apply/Save/Cancel
retain the existing shared-default, private-instance and assignment scopes.

Foliage is a built-in surface material, similar to Grass. GPU-only broad leaf
strips scatter on the source and within a configurable tangent-plane radius
around it, including beyond its boundary. Expose density, leaf length/width,
height, edge spread, tilt, curl, wind, seed, brightness variation and two leaf
colors. Stable seeds, cached source domains, expanded transformed bounds, culling,
LOD, work budgets, shadow cutouts and alpha apply. Color-parameter opacity must
participate in transparent rendering, including object programs. Zero wind or
zero emitted density must let rendering idle. No generated leaves enter CPU
geometry, snapping, export or Explode. Publish the program and schema via MCP.

### Shader editor layout and preview — #29/#30, T66

The shader editor occupies the same full-window area as the object script editor,
with equal-width code and GPU preview panes. Three tabs retain independent drafts
and editing state: Definition (JSON), Vertex (GLSL) and Fragment (GLSL). All use a
monospace code editor with language-specific syntax highlighting. GLSL highlighting
includes keywords/qualifiers, types, function names, comments, literals, parameter
uniforms and time.

Preview automatically after a typing pause, with an explicit Preview button. Use
the existing renderer on a small private scene: a cube for surface shaders, a
plane for subdivided/scattered surface programs and a procedural owner for trees.
Orbit, pan and zoom affect only the preview camera. Report JSON/schema and actual
GPU compiler/linker errors without changing the active drawing, selection, camera,
materials or undo history. Tab switching preserves every draft. Cancel/Escape
retire the preview and restore the drawing viewport; Save Program is an explicit
undoable program edit. Retain compatible material parameter values when returning
to the parent editor; newly declared fields use their defaults.

### Architectural material assignments — #29, T67

All architectural material selectors send material IDs through the validated
command dispatcher. Register the exterior/interior, top/bottom/side, frame/glazing
and tread/support ID fields on architecture creation, update and defaults
commands. Both conventional and procedural catalog materials must commit,
regenerate, survive Undo/Redo and reopen without reverting to prior bindings.
A failed assignment retains the inspector draft instead of silently resetting it.

### Conditional branching shader — #29/#31

New procedural trees use a shared GLSL conditional branching evaluator with
Broadleaf, Conifer, Weeping, Shrub, Coral and Roots rule families. Preserve existing
`meshcraft.tree.v1` objects and provide `meshcraft.branching.v1` as an independent
owner program. Generate frames, curved variable-radius branches and foliage on
the GPU from stable path IDs and seeds. Rust supplies only settings, conservative
bounds and bounded candidate counts. No CPU branch generation or per-frame mesh
readback. Parent-relative frames, continuous growth/survival and descendant-aware
LOD apply to every family. Expose the GLSL `branchingRule` hook, typed parameters
and backend work limits through the editor and MCP. Previews, shadows and explicit
mesh bake share the same shader. Preserve prototype/instance properties, copy,
history and persistence. See [the shader contract](../../branching-shaders.md) for
limits, controls, path encoding and the scope of continuity/topological claims.

### Material form previews and pattern orientation — #29/#30

All material settings use the same split dialog as procedural object settings:
independently scrolling fields on the left, an isolated live GPU preview on the
right, with orbit, pan, zoom and Fit. Conventional materials and surface programs use a cube; scattered/water
programs use a plane. Draft changes are debounced; invalid values retain the last
valid preview. Previewing never modifies the drawing, selection or history.
The preview stays available when switching between Conventional and shader programs.
Conventional previews use the same material color/opacity rendering as the drawing.

Brick and Wood expose `rotationU` and `rotationV` in degrees (default zero). These
rotate the local procedural pattern frame about U/X followed by V/Y; the shader
applies the inverse transform when sampling. Wood's grain-axis selection remains
available. Source geometry, object transforms and mesh topology stay unchanged.
Material defaults and per-assignment overrides use the existing parameter
resolution, save/reopen and Undo/Redo paths; the schema is published through MCP.

### Selection and instance finishes — 0.48.0

Home and Materials expose a catalog picker for the selection's current material,
or a multiple-material state. Changing it applies to all editable selected mesh
primitives, object instances and architectural owners in one Undo step. With no
selection it chooses the current construction/paint material.

A mesh/script object finish is a persistent instance material binding. It applies
to that instance's complete geometry, including nested contents, without baking
or replacing topology, modifying its prototype, or changing sibling placements.
Painting a parent replaces older child finishes only within that parent; later
child painting can specialize it again. Instance bindings survive copying,
schema-17 save/open and material-ID remapping on subscene import. Their materials
cannot be deleted while referenced. Generated script updates retain the finish.
Within prototype editing, direct face/edge painting still edits that authored
geometry. Architectural owners update their surface slots; hotspots keep colors.

### Shader objects from scratch — T82

Objects › New Shader Object (`object.new_tree`, ID unchanged) opens the shader
editor on a blank `custom` object program rather than a tree preset. A **Start
from** list offers the blank object, a blank surface and every built-in. An
unchanged built-in object program is used as is; an edited built-in is saved under
an unused `custom.<kind>.<n>` ID, so built-ins are never replaced. Saving an object
program in this mode stores the program (when new) and a new prototype as one Undo
step and starts placement (**Create Object**); a surface program is only saved.
Materials › New Shader opens the same editor starting from a blank surface.

A `custom` program defines `void customVertex(int vertexId, int instanceId, out vec3
position, out vec3 normal, out vec2 uv)`, called `vertexCount` times (a multiple of
3, at most 65,536) for each instance. `instances` is a fixed count or an integer or
unsigned parameter. `objectBounds` is required: each corner coordinate is a number or
an arithmetic expression over parameter values, evaluated on the CPU for culling,
picking, selection and Fit Model. No tree engine, branch state or tree output is
compiled into a custom program, and omitted `proceduralVertex`/`proceduralColor`
hooks get pass-through defaults. Instances drawn are limited by the object's
candidate budget and 6,000,000 vertices. Bake to Mesh and Bake to GPU Mesh capture
custom objects from the same shader; a zero count reports an error.

A read-only **Full Source** tab shows the exact vertex and fragment shaders compiled
for the viewport, including the host template and, for branching programs, a note on
the transform-feedback state pass. Its text can be selected and copied.
See [Shader objects](../../shader-objects.md).

### Standard shaders and shader program management — T83

Built-in surface programs (read-only, like the presets): `emboss.rough_texture`
(fractal noise relief), `emboss.tiling` (raised, bevelled tiles on each face's
dominant plane), `emboss.rough_tiling` (both), `metal.polished` (mirror reflection of
a procedural studio environment tinted by the material colour, with Fresnel),
`glass.clear` (transparent, Fresnel opacity and reflection) and `noise.perlin`
(fractal classic Perlin noise between two colours, optional relief). Relief is
shading only. Patterns are sampled in local coordinates.

Fragment programs may define `vec3 proceduralLighting(vec3 lit, vec4 color, vec3
position, vec3 normal)`, called after scene lighting and shadows to return the final
colour; it defaults to returning `lit`. Surface programs receive smooth corner
normals: each corner averages the area-weighted normals of faces around its position
within 40° of its own face, so curved meshes shade smoothly and sharper edges stay
crisp. Bake capture evaluates colour hooks per vertex with derivatives reading as
zero, so fragment-style code still bakes.

There is one **New Material** command. Its Program list names *Conventional · no
shader* and every surface program; choosing a built-in for an unsaved material also
applies its suggested name (while the name is untouched), colour and PBR settings.
**New Shader…** beside the list opens the shader editor on surface starters and
returns to the material with the saved program chosen. The dedicated New Grass,
Foliage, Water, Brick and Wood buttons are removed from menus and panels; their
command IDs stay registered for MCP and scripts.

The **Shader Programs** panel lists custom programs, then built-ins, with ID, kind
and users. `program.edit {id}` opens a program in the shader editor (built-ins
unchanged; saving stores a copy). `program.delete {id}` removes a custom program as
one Undo step and makes materials that used it conventional, keeping their colour,
texture and PBR settings. Built-ins and programs that shader objects use are refused.
See [Standard shaders](../../standard-shaders.md).
