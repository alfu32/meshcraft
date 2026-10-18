# ADR: Normalized document and geometry ownership

Status: draft for T46 review. The schema and migration rules below are a gate for T02/T03/T05/T16; migration and ownership cases remain incomplete.

## Ownership and lifetime

| Value | Sole authored owner | Durable? | Derived or session state kept elsewhere |
| --- | --- | --- | --- |
| Reusable mesh, name, semantic frame, authored domain parameters, hotspot definitions | `Definition` | Yes | Generated domain geometry and usage indexes are derived |
| Vertices, segments, triangles, dimensions and text in one definition | Definition-owned `GeometryBundle` | Yes | GPU buffers, adjacency, spatial indexes and selected flags are derived/session |
| Ordered child placements and their local inputs | Shared `ChildLayout` templates | Yes | Parent links, realized child trees and reverse-definition maps are indexes |
| Transform, hotspot/input changes, geometry replacement, or layout variant for one realized placement | `InstanceEdits`, keyed by `OccurrenceRef` | Only when authored | Inheritance is absent; resolved world matrices and effective content are derived |
| Unknown legacy fields and source identity aliases | Compatibility envelope at the smallest owning DTO/record | Yes | Import lookup tables are rebuilt from it |
| Active occurrence, edit target, selection, guides, tool draft and focus | `EditorSession` | No | Never stored on a shared definition or layout |
| Generated wall/HVAC/voxel mesh, evaluated hotspots and bounds | Revision-keyed `ResolvedContent` | No | Rebuild or discard without changing document bytes or undo |

`DefinitionKind` remains an editing profile, not an exclusive content union: root mesh definitions may contain sparse architecture/HVAC data. Domain parameters use typed sparse components; unknown future values stay in the compatibility envelope. `W = parent_W × I × D` remains the transform contract. Store semantic definition frame `D` and placement transform `I` once each as affine values; derive matrices and bounds.

## Geometry identity and measured alternatives

Reproduce the 60-fixture probe with `python3 docs/rewrite/scripts/geometry_storage_probe.py --output docs/rewrite/geometry-storage-probe.json`. It verifies source hashes and counts only base segment/triangle coordinate slots. Of 168,849 point slots, 28,561 exact positions give a 44.11% packed-column lower-bound saving for coordinate-deduplicated indexing. That layout infers identity from position and is unsafe. The legacy 1e-3 key yields 24,718 keys but 3,181 keys contain distinct exact positions; collapsing them would lose corner coordinates. The probe estimates 2,603,960 bytes (35.74% lower) when mapped legacy keys share stable IDs, exact differences use sparse per-corner position residuals, and the 8,427 slots whose keys are absent receive independent IDs. A no-weld indexed layout costs 66.67% more than inline coordinates. These are packed position/ID/reference estimates, not serialized-file or Rust-heap measurements; primitive IDs/colors, compatibility alias strings, opaque metadata and allocator/container overhead are excluded, so the source-alias-preserving estimate is a lower bound.

Do not weld by exact or quantized coordinate equality. New geometry shares a `VertexId` only when its creating/editing operation establishes that relationship. The core represents each segment endpoint and triangle corner as `VertexRef { id, position_offset }`; resolve the exact point as the referenced vertex position plus its per-corner offset. Import maps persisted vertex IDs to shared identity aliases, uses offsets to preserve each source coordinate exactly, and gives unaliased corners independent IDs. `GeometryBundle::vertex_position` is the canonical point query. Serde accepts the previous bare-ID corner encoding as a zero-offset reference. Geometric adjacency remains a tolerance-based predicate, not identity. Segment/triangle IDs and RGBA remain independent records, so coincident faces with different IDs/colors survive. Topology edits return explicit old-to-new identity remaps for selection and hotspot attachments.

The source `prototypeVertices` map is not a complete mesh vertex table: 1,266 base-geometry keys have no record and 5,747 serialized keys are absent from base segment/face positions. Preserve unmatched aliases and unknown payloads; do not rebuild or discard the map from geometry alone. The probe reports its measurement scope in `geometry-storage-probe.json`.

## Occurrence, replacement and migration rules

`OccurrenceRef` is a root-entry plus ordered `LayoutEntryId` path. A shared `ChildLayout` can therefore appear under multiple parents while each realized occurrence keeps independent `InstanceEdits`. A layout change creates a variant reference, not a cloned mutable subtree. Detect definition/layout cycles before resolving content.

For each editable component, absent override means inherit; a present empty replacement intentionally removes inherited content. Do not discard an equal legacy snapshot until source attribution confirms it is a generated no-op; retain the original envelope for compatible export. Preserve differing or ambiguous snapshots as opaque authored fallbacks. Store domain parameters and their provenance as authored values; generated output is revision-keyed and never becomes a second source of truth. Legacy export may materialize shared definitions/layouts when schema-17 compatibility requires it.

The `VertexRef` contract and importer path are implemented and tested for quantized aliases with distinct exact coordinates, coincident independent vertices, colored coincident faces, occurrence-level sharing, explicit empty replacement, and nonidentity frames. T46 still needs explicit topology-remap and domain-provenance migration cases plus a full hierarchy migration comparison before this draft is ratified. Downstream adapters must use `vertex_position` when they need exact primitive-corner coordinates.
