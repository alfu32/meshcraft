# Retained GPU geometry — Solidcraft review and Meshcraft implementation scope

## Source review, 2026-10-24

Reviewed Solidcraft commit `807e367bdb03210dba188118fe000acd0a4b3725`.
This is static source inspection and the authors' reported measurements; no
independent Solidcraft runtime benchmark was performed.

Their documented large STEP examples contain approximately 4.4 million display
triangles across hundreds/thousands of bodies. This is not evidence of millions
of independently editable CAD objects or a guaranteed interactive frame rate.
Their 600,000-triangle viewport table measures CPU UI-frame time, separately
from complete GPU presentation latency.

Relevant primary sources:

- [Performance measurements](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/docs/perf.md): reuse of repeated-part display meshes, retained scene blocks, cached bounds and picking; author-reported before/after measurements.
- [GPU renderer](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/crates/gpu/src/view.rs): persistent vertex/index buffers, shared instance geometry, cached block uploads, instance-frustum filtering and display LOD selection.
- [UI integration](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/crates/ui-egui/src/gpu.rs): viewport paint callback shares egui's wgpu device and render pass.
- [Line shader](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/assets/shaders/viewport.wgsl): GPU projection, near-plane clipping and screen-width ribbon construction.
- [Picking indexes](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/crates/ui-egui/src/pick_index.rs): cached per-mesh acceleration, asynchronous construction of larger indexes, functional picking while construction runs.
- [Mesh LOD](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/crates/ui-egui/src/mesh_lod.rs): background display simplification; authoritative picking/measurement/export geometry stays intact.
- [GPU tessellation](https://github.com/tangency-labs/solidcraft/blob/807e367bdb03210dba188118fe000acd0a4b3725/crates/gpu/src/lib.rs): compute evaluation of curved-surface display plans into GPU buffers without CPU readback. Less directly applicable to existing direct meshes.

## Meshcraft findings

At 0.33.7 the native renderer caches world-space faces and edges on the CPU,
but iterates/submits every visible triangle on every rendered frame. Thick
line ribbons are projected and constructed on the CPU each frame. Occurrences
are expanded into world-space copies. The viewport is GPU rasterized; its
geometry preparation/submission is still substantial CPU work. raylib's draw
batch does not provide persistent application-owned scene meshes.

The GPUI image path additionally reads the render texture to CPU memory,
converts/flips pixels, then presents the image. This cost depends on viewport
resolution and remains separate from mesh submission and model/history costs.
The native picker already has a BVH, currently over world-space primitives.

## Authorized implementation (T59/T60)

1. Retain application-owned OpenGL buffers for model faces and lines. Keep
   unchanged geometry buffers across camera, selection, material/style and
   document changes wherever their rendered geometry is unchanged.
2. Construct screen-width model lines in a vertex shader, including near-plane
   clipping. Preserve black/material line preference, selected line widths,
   wireframes, prototype isolation tint and independent selection feedback.
3. Share identical effective local-space geometry between occurrences and draw
   it using persistent instance-transform buffers. Different hotspot/script
   results get separate geometry variants; transforms and selection flags
   must not force geometry reuploads. Reuse the same geometry in the shadow pass.
4. Preserve existing authoritative geometry, picking, transactions, shader AA,
   transparency, mirrored winding and one-sided ground behavior. Validate actual
   output and upload reuse, including selection and prototype/variant updates.

Implementation remains in the existing raylib/OpenGL 3.3 renderer. No Solidcraft
source is copied. Frustum culling, display LOD, hierarchical per-prototype picking,
shared-texture interop and history storage redesign are later work. GitHub #27's
large-model shadow-map strategy remains open and is not completed by these changes.
