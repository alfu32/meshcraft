# Mesh import and export

Use **File → Import Mesh / Export Mesh** (also available in the command palette).
Choose a filename extension to select the format. Imports create a named prototype and
start wireframe instance placement; click to insert it. As in the legacy importer,
prototype coordinates are rebased to the minimum bounding-box corner for insertion. Escape leaves the imported
prototype available in Objects. Importing the prototype and placing it are undoable.
Exports use the selection when nonempty, otherwise the entire model, including hidden
geometry. Clear the selection to export everything. IFC exports geometry proxies, not
semantic BIM wall/slab entities. Save `.octd` to retain editable architecture, instances,
scripts, materials and hotspots.

Parsing, scene collection and file writing run on a worker. **File → Cancel Import / Export**
cancels the current job. A document/context change during import rejects its result.
Export captures the document and selection at dispatch, stages a sibling temporary file,
and publishes it only when complete. Failure or cancellation preserves any existing output.
Warnings report unsupported/lost features; automation receives them in job outputs.

## Supported subsets

| Format | Import | Export |
| --- | --- | --- |
| OBJ | Vertices, concave polygon faces, negative indices, lines, RGB vertex colors, local MTL diffuse/alpha | Faces, geometric normals, lines, RGB vertex colors |
| STL / STLA / STLB | Auto detection, ASCII and little-endian binary triangles | Binary `.stl`/`.stlb`, ASCII `.stla` |
| FBX | ASCII polygon arrays with baked transforms; binary/transform-bearing files fail explicitly | Static ASCII FBX 7.4 mesh |
| glTF / GLB | glTF 2, embedded/base64/local buffers, indexed/strided accessors, lines, strips/fans, scene transforms, base colors | Self-contained glTF/GLB, triangles, lines, geometric normals and RGBA materials |
| Collada / DAE | Triangle/polylist/polygon meshes, lines, accessor stride, node transforms, asset units | Static mesh/line scene and unit metadata |
| DXF | ASCII LINE, 3DFACE, SOLID, TRACE, POLYLINE/LWPOLYLINE, tessellated circle/arc, true color | ASCII 3DFACE + LINE, true color, INSUNITS |
| 3MF | ZIP mesh resources, nested components and build transforms, physical units | Closed, consistently wound manifold mesh in an OPC ZIP package |
| AMF | XML/ZIP, per-object triangle meshes, physical units | Triangle mesh XML |
| IFC | STEP tessellated/polygonal face sets, faceted BREP, polylines, local placements | IFC4 project/site/building/storey hierarchy, named occurrence mesh/curve proxies |

Textures, authored UVs/normals, animation, skinning, semantic CAD/BIM ownership and
parametric history are not preserved. glTF sparse accessors, Collada controllers/instance_node,
DXF blocks/OCS transforms, AMF constellations and IFC swept solids/mapped representations
are outside these subsets. FBX support matches the legacy ASCII-only scope; prefer glTF
for hierarchical DCC scenes. Source axes are retained (Meshcraft uses Y up); no implicit
axis rotation is applied. Polygon faces are triangulated with their winding retained.

## Units

Stored model coordinates are never rescaled. `unit_size` retains the legacy physical-export
meaning of meters per internal unit, independently of its display label. The filename's
last token may select an output unit: `part_mm.obj`, `part_cm.dxf`, `part_in.stl`.
For model `unit_size=0.001`, `part_mm.obj` uses coordinate scale 1 and `part.obj` uses
0.001. Default 3MF output uses millimeters. glTF and IFC output use meters; IFC normalizes
the requested unit back to SI meters with matching metadata. An explicit `unit`
argument overrides filename metadata. An optional positive `scale` is applied once.

Import honors embedded 3MF/AMF/Collada/DXF/IFC units; glTF coordinates are meters.
Unitless OBJ/STL/ASCII FBX coordinates default to raw internal units. A recognized filename
suffix or explicit import `unit` declares their physical units. 3MF defaults to millimeters;
AMF defaults to millimeters, Collada and IFC to meters. Changing display units after import
does not modify stored coordinates. The native legacy importer previously ignored some
embedded unit metadata; honoring it here is a deliberate interoperability improvement.

## Automation

Discover `import.mesh`, `export.mesh`, `export.ifc_model`, `interchange.cancel` and
`interchange.formats` through MCP `k3d_list_commands`. Provide an absolute `path` to avoid
a dialog. `interchange.formats` returns this guide and the supported extensions.

- Import: optional `unit`, positive `scale`, `position:[x,y,z]`. Supplying position inserts
  one instance in the active editing context in the same transaction; omitting it starts
  interactive placement after the import job finishes.
- Export: optional `unit`, positive `scale`, `selection_only` (defaults to whether anything
  is selected). `selection_only:false` explicitly exports the whole scene.
- Poll the returned job ID with `meshcraft_get_job`; dispatch success is not file completion.
  Outputs include path, face/line counts and warnings; import also returns prototype ID and the original minimum corner (`source_origin`,
in internal model coordinates after physical-unit conversion).

ZIP64 archives are unsupported. XML is limited to 128 attributes per tag, 64 KiB per
tag, two million nodes and 256 levels of nesting.

Files/resources are limited to 512 MiB, decoded geometry to two million primitives,
individual polygon rings to 4096 vertices and recursive scene traversal to 64 levels.
Only local relative linked resources inside the source directory are read. Network URIs,
external XML entities and scripts are never executed or fetched. ZIP entries are decoded
in memory with a decompressed-size limit; archives are never extracted to disk.
