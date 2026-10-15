# Compatibility and migration contract

## Compatibility levels

Track these independently: document read, document round-trip, editable feature behavior, command/API compatibility, visual equivalence, platform workflow. Passing one does not imply the others. A mesh export cannot prove a hotspot or wall remains editable.

Initial delivery targets desktop functionality excluding Android, plugin execution and developer CLI/TUI. Browser and required JavaScriptCore plugin delivery follow desktop independently. Mark phase-specific capabilities explicitly in catalogs and migration reports. Existing Groovy model-building scripts and `/scene/console` requests will not work in the first delivery; translate regression setup into typed commands rather than hiding a JVM dependency. Later JS plugins receive the versioned context in `plugins.md`, not direct legacy object bindings.

## Model payloads and defaults

`ModelPersistence.VERSION` is 17, but the 60 indexed fixtures all omit the version property. libGDX JSON uses object prototypes/default omission; the Rust reader must materialize the correct default for each DTO field. The census records 32 DTO classes, field declarations/default expressions and their source lines. Treat that as a source locator; conversions and constructors still define important behavior.

Examples of necessary interpretations:

| Serialized form | Meaning |
| --- | --- |
| `{"definitionOrigin":{}}` | Zero origin |
| `{"definitionAxisU":{"x":1}}` | `(1,0,0)` basis vector |
| `{"definitionAxisV":{"y":1}}` | `(0,1,0)` basis vector |
| `{"color":{"r":0.5}}` | Remaining channels use ColorDto defaults: g=1, b=1, a=1 |
| omitted `kind` on ObjectPrototypeDto | `MESH` |
| omitted frame `glazingEnabled` | false in persisted DTO, even though new-tool settings default true |
| omitted top-level `version` | Current constructor default when read by current Kotlin code; cannot determine historical writer version |
| missing `circleSegments` | Loader supplies 24 |
| omitted `modelUnit.size` / `name` | 1 / `unit` |

Missing basis axes at a DTO level can mean zero vectors, not identity. Current web docs warn that version-only seeds can create a singular root. For new empty documents Rust constructs a valid identity basis. For imported malformed files it diagnoses and offers a defined repair/default policy; it must not silently reinterpret a nonempty invalid transform as a different valid model.

Maintain separate types for raw decoded DTO, validated canonical document, runtime cache and selection/session state. Retain unknown keys at their owning DTO object path so future/deferred content can round-trip. Do not drop nested unknown keys when a recognized field is edited. Warnings identify lost/unsupported behavior, not just whether JSON parsed.

## Payload shapes

Read current `prototypes + rootInstance` documents, prototype-only recovery, old `rootGroup` trees and flat top-level `segments/faces`. No claim of universal versions 1..17 is made: T00 retrieves historical writer examples and records each supported shape/version/default set. Current input may already contain old optional fields.

Legacy prototype DTO contains definition origin/basis, glue flag, kind, external-reference settings, voxels, walls/slabs/stairs/frames, plumbing/ventilation, hotspots, prototype vertex identities, segments, triangles, dimensions and text. Legacy instance DTO contains prototype ID, instance origin/basis, children, hotspot position/attachment overrides and optional per-instance geometry/annotation overrides. Preserve those meanings without requiring the same duplicated layout in Rust's canonical graph. `object-model.md`/F10 governs shared geometry/layouts, typed authored edits and occurrence aliases; flattening to unrelated loose geometry is not an acceptable migration.

Generated architecture/HVAC geometry has filtering/synchronization rules in the legacy snapshot/application paths, including special root handling. Canonical parameters remain authoritative; preserve authored loose geometry and avoid duplicating regenerated output. Voxel surfaces are likewise derived. T05 retains domain DTO payloads before their editing modules ship; T28–T33 progressively take ownership.

Selection is not a ModelSnapshot field. Some selection changes trigger saves/undo listeners, but that does not make the selection itself a persisted guarantee. Application preferences such as toolbar layout/walk tuning are stored outside model JSON. Record exact new preference keys/schema separately.

## Containers, history, and storage safety

Project files are UTF-8 JSON or gzip magic `1f 8b`; extension alone is insufficient. Current compression threshold is strictly greater than 10 MiB of uncompressed JSON. History entry `data` is base64(gzip(JSON snapshot)), independent of the project compression threshold. The history snapshot excludes nested undoHistory; avoid recursive growth. Index/entry labels/timestamps/max entries are part of stored history.

Read history without forcing all entries to decode at startup. Preserve unreadable entries with a warning; do not reject an otherwise valid document solely for history corruption. Exported schema-17 history must retain legacy encoding. A different native Rust undo representation remains internal until an explicitly versioned format is agreed.

Save snapshots must capture a single immutable document revision, including xref decisions. New disk write protocol: write temporary sibling, flush as required for the platform, replace atomically when possible, retain prior recoverable file, publish successful saved revision only after completion. A canceled/failed save keeps dirty state and the prior good file. Autosave, manual save, Save As and close cannot race or allow an older async job to overwrite a newer one.

Staged load pipeline: decode container → parse sparse DTO → validate references/transforms → build document off to the side → regenerate required derived data → atomically publish model and invalidate caches. Progress and cancellation apply across all long stages. The live document remains usable if any stage fails. Source path, repair warning and original bytes/hashes are retained in the migration report when relevant.

## Identity and transformed geometry

Keep stable source IDs for segments/triangles/prototypes/instances/hotspots and deterministic remapping only when imports create actual collisions. Derived caches may use dense/generational runtime handles; persisted references use stable IDs. Hotspot references can contain IDs plus quantized vertex coordinates; preserve the legacy quantization basis during conversion.

Normalization has an explicit compatibility boundary: classify equal base/override copies, genuine differing instance edits, generated domain content and copied child layouts using source attribution and fixture comparisons. Collapse only proven no-op copies; retain ambiguous/differing snapshots and unknown fields. Explicit empty authored replacement is different from no override. Schema-17 export may re-expand shared canonical content/layouts as the legacy codec requires. Cache eviction must not change authored state, undo or effective query results. Nonidentity frames and coincident independent vertices remain valid despite identity frames/repeated coordinates in the current corpus.

Transforms compose definition/instance/local-parent relationships as characterized from `GroupScene`, not merely a new `Transform::translation` guessed from names. Test rotated, reflected/nonuniformly scaled and nested objects. Affine normal conversion and output winding need explicit checks. f64 migration uses tolerance-aware canonical comparisons; preserving float formatting or triangle order is not required unless a client demonstrably relies on it.

Exact observed order: `W = parent_W × I × D`, and setting a desired world transform solves `I = inverse(parent_W) × desired_W × inverse(D)`. Ordinary point/vector transforms and inverse-transpose normal transforms are separate operations. See F02 for observed display/export scale formulas and filename unit-suffix selection.

## Xrefs

Capture a small bundle containing `examples/xrefs.octd` and all referenced companion files, plus a synthetic missing-file case. Resolve file bases explicitly; use source filenames/hashes in the fixture manifest. Do not copy all examples into every prompt. Missing/cyclic references must be observable, with unresolved metadata preserved. Never resave an unresolved external definition as empty content. Browser and later Android adapters need their own permission/storage models and are not desktop filesystem emulators.

## Format support matrix

| Format | Legacy import | Legacy export | Required parity boundary |
| --- | --- | --- | --- |
| OBJ | yes | yes, includes normals | Supported faces/indexing; output geometry/normals; not full MTL/UV scene fidelity |
| STL | ASCII/binary/auto | ASCII/binary | Triangle geometry, correct winding and binary detection |
| DXF | yes, lines + mesh subset | yes, lines + triangles | Supported entity list/units characterized in T36 |
| FBX | parser subset | ASCII writer | Exact supported versions/nodes/transform subset, no format-wide promise |
| glTF/GLB | parser subset | yes | Supported accessors/primitive/buffer/transform subset, no implicit animation/material fidelity |
| DAE | parser subset | yes | Supported geometry/transform/unit subset |
| 3MF | ZIP/XML subset with unit/component work | indexed mesh with validation | Unit scaling, component/build transforms, closed-mesh diagnostics |
| AMF | XML subset | yes | Supported mesh/unit subset |
| IFC | geometry subset | separate IFC4 proxy writer | Spatial structure, proxy mesh/line representations, units; no semantic BIM authoring guarantee |
| SVG/PNG | not editable project import | view/image capture | Current view projection or raster output; not project round-trip |

T36/T37 produce an explicit parser-support ledger from code branches and fixtures, including unsupported constructs and error behavior. Do not choose a replacement library merely because it advertises the same extension. Color/domain/instance information loss must be reported per export path. Sample external files and current exported examples are preferable to tests where a new reader only reads its own writer.

## Commands and automation

Preserve supported literal palette IDs, six orthographic IDs, generated built-in tool IDs and toolbar show/hide IDs. Initial runtime omits `PLUGIN` and deferred plugin namespaces. `tool.builtin.cut_out_3` is generated from the enum; older prose spelling `cutout` must not override code. Hotspot placement needs a separately named action because no hotspot ToolId exists.

The legacy command census is static, platform-gated and plugin-extended. Runtime discovery is authoritative. Rust adds canonical commands for direct UI callbacks (File/Edit/Render etc.) with explicit payload schemas. Menus/toolbar/palette bind to IDs and metadata, not separate callbacks. Version the schema and document supported versus legacy aliases.

Keep REST command discovery aliases and supported pointer/status routes. JSON-RPC initialize/tools/resources are characterized before claiming protocol conformance; echoing any requested protocol version in legacy code is not a good specification to repeat blindly. Return only versions implemented by the new server. Preserve `k3d_*` tool and `k3d://*` resource names for retained functionality; the contract declares console/plugin capabilities false initially. Deferred Groovy endpoints return a clear unsupported result and are not advertised as callable tools.

Legacy default timeouts are approximately command 20 s, console 30 s, pointer 5 s. New structured async jobs should return completion IDs/revisions so a client need not infer completion from dispatch success or sleeps. World-coordinate pointer requests use Y-up; screen coordinates are viewport/window-defined consistently after menu layout. Capture completion waits for actual output availability.

## Browser and future surfaces

Retain JS method/event/command names from the actual component and Main embed dispatcher. Preserve the two tutorial schemas and account for `pendingDiscovery`, manual checks and currently unimplemented scene-state expressions. Do not reclassify those as tests that already pass.

Defer Android, Groovy/JVM execution and developer console/CLI/TUI implementation. Required later plugins use JavaScriptCore and a versioned context; old scripts need a source/API port, not a JVM bridge or promised class identity compatibility. Preserve sample metadata and unknown plugin values through initial editing. Later plugin state is owned by plugin ID/schema version and consists of serializable authored values, never runtime JS heap objects. JSC context/callback handles and generated caches are not document data. Native JSC packaging is separate from browser execution; neither phase may claim capabilities it does not implement.
