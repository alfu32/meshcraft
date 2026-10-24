# Insertion latency and deferred scene maintenance

## Reproduction and causes (0.33.9)

The user's current `/home/alfu64/demo.octd` was copied into an isolated scratch
fixture, without writing to the original drawing. SHA-256:
`3ba00aad87ce5c670e3ece9cb1f72ff1b75ae23de926d3716096ef0e76d25392`.
The imported scene container has 20,882 vertices, 27,320 segments and 37,254
triangles, plus separately placed objects. This is a larger fixture than the one
used for the 0.33.7/0.33.8 transform and GPU checks.

The user reported wall insertion at 20 seconds, rectangles at two seconds,
extrusions at two to three seconds, and committing a sweep with approximately
1,000 edges/1,000 triangles at 22 seconds. Independent app-session probes reproduced
wall commit at 20.910 seconds and a generated curved sweep (504 vertices,
1,004 edges, 1,000 triangles) at 25.036 seconds. These are local optimized-debug
measurements, including the transaction/history path, not Solidcraft benchmarks.

Two independent insertion algorithms scaled badly:

- `put_architecture` rebuilt every wall, including distant unchanged walls. Each
  wall replacement scanned the scene's generated facets again. Insertion now
  uses the affected-owner/junction regeneration already used by transforms.
  Both old and new junctions participate when editing existing wall parameters;
  unrelated geometry and its IDs survive unchanged.
- Indexed mesh insertion searched every existing primitive corner for each new
  vertex, then every existing segment for each new edge. Transaction-local
  spatial bins now index those candidates once. Corner positions retain priority
  over raw/unreferenced vertices; nearest-distance ties retain source order.
  Imported per-corner offsets, reverse-direction edge reuse, colors, topology
  tolerance and failure atomicity are preserved. Newly inserted vertices and
  edges enter the same lookup during the operation. No persistent format or
  snapping-radius changes are involved.

The user's subsequently saved larger drawing also exposed the initial 64 MiB
Undo budget: a before/after sweep pair needed 67,227,626 bytes, exceeding
67,108,864 bytes. The default budget is now 256 MiB, still capped at 20 edits.
Configured/custom byte limits and atomic rejection beyond the limit are retained.
This permits the tested drawing to grow; it is not structural sharing or a removal
of snapshot memory costs.

The edit is still a validated transaction with ordinary Undo/Redo. No pending
geometry is hidden outside the document, and no operation is committed later
without a user's edit. The sweep still produces the same mesh; this change is
not a resolution reduction.

## Solidcraft comparison and staging decision

[Solidcraft's performance notes](https://github.com/tangency-labs/solidcraft/blob/main/docs/perf.md)
describe the same general failure mode: work over the entire design on every
small command. Their fixes retain unchanged placed bodies and display meshes,
avoid repeated parameter/name scans, and retain CPU/GPU scene blocks containing
small groups of bodies. Their published benchmark timings are author-reported;
Meshcraft has not independently benchmarked their application. The earlier
[pinned renderer source analysis](gpu-renderer-analysis.md) records the separate
GPU comparison.

The notes also describe worker preparation for Open/import, retaining the previous
scene until successful adoption, and revision/setup checks for background tool
previews. They do not establish a deferred authoritative-document merge as their
insertion model.

The user's proposed short-term/long-term scene is useful as **derived scene
maintenance**, with these constraints:

1. Accept an edit into the canonical document and Undo history immediately.
   Queries, snapping, export and Save must already see it.
2. Retain stable spatial/render blocks; publish changed/new blocks as an overlay.
   A picking overlay must also suppress removed or replaced old items. Merely
   postponing the full rebuild would otherwise leave stale snap/select targets.
3. Coalesce maintenance requests by document generation/revision and edit scope.
   Compact changed blocks in bounded worker tasks during quiet periods. Tag
   results with their input revision and discard stale results after another edit,
   Undo, Redo or document replacement.
4. Keep GPU uploads/resource disposal on the owning graphics thread. Publish
   immutable prepared CPU blocks and limit work admitted per frame.

That broader CPU scene/index update is a follow-up architecture change, not
implemented by this fix. In particular, native world-space snapping/picking
indexes still rebuild after a committed geometry revision, document snapshots
remain full copies, and display metadata still has whole-scene work. The new
insertion lookup is local to one transaction; it does not claim incremental BVHs
or idle compaction. The immediate fix removes the measured 20–25-second repeated
work before introducing scheduling complexity.

## Validation

The opt-in app probe can be rerun with:

```sh
MESHCRAFT_BENCH_PROJECT=/absolute/path/to/demo.octd \
  cargo test -p meshcraft-desktop --lib insertion_latency_probe \
  --locked --offline -- --ignored --nocapture
```

`MESHCRAFT_INSERT_CASE` optionally selects `line`, `rectangle`, `extrude`,
`circle-far`, `circle-model`, `cuboid`, `wall`, `slab`, `instance` or `sweep`.
The probe separates commit/history from the subsequent spatial refresh and
checks complete serialized authored-state restoration through Undo and Redo.
Timing excludes drawing-file decoding and excludes window/render presentation.

Regression tests compare affected wall output with a complete architecture
rebuild, including holes and autojoin toggles, check unchanged distant owner IDs,
and bound the number of tessellated owners independently of distant wall count.
Mesh insertion checks corner-offset identity priority, raw vertex reuse,
reverse/duplicate segment reuse, face color and rollback after a late invalid
face. Spatial lookup tests check nearest results against an ordered exhaustive
search across cell boundaries, negative coordinates, distance ties and scales.

### Results

The [machine-readable evidence](../references/insertion-latency/validation.json)
separates app commit/history, first spatial refresh and native frame capture.
All before/after comparisons below use the same frozen drawing.

| Operation | Before commit | After commit | After first spatial refresh |
| --- | ---: | ---: | ---: |
| New wall | 20.910 s | 0.540 s | 0.474 s |
| Curved sweep, 1,004 edges / 1,000 faces | 25.036 s | 0.766 s | 0.585 s |
| 24-sided circle, isolated | 2.031 s | 0.952 s | 0.460 s |
| Cuboid | 1.154 s | 0.670 s | 0.465 s |
| Rectangle | Not measured | 0.758 s | 0.474 s |
| Rectangle extrusion | Not measured | 1.101 s | 0.372 s |

These are single runs on a shared host, with ordinary timing variability. The
full ten-case probe passes serialized authored-state Undo/Redo checks. The
extrusion case prepares its selected rectangle before timing its actual
Push/Pull commit. The sweep timing includes generation of its test path/profile
and its normal mesh commit; fixture decoding is outside the timer.

A private native GPUI run on Xvfb `:96`, software Mesa, MCP port `18766`, at
1500×1000 replayed the actual tool workflows through world-point input. Three
successive wall commits returned in 1.06/1.44/1.59 seconds; fresh captured frames
arrived in 1.52/1.89/2.19 seconds. Rectangle capture took 2.92 seconds, extrusion
2.11 seconds. All three walls and the extrusion passed Undo/Redo, and the
rectangle added the expected four segments/two triangles. Captures require a
fresh renderer request revision; these are not stale-frame measurements.
These capture times include the UI/display refresh, software renderer, bridge
and PNG capture overhead, and are not direct comparisons with the isolated
commit timings. Rectangle/extrusion still expose remaining whole-scene costs.
The native test used its own drawing copy and settings directory. The user
continued editing the original drawing during the investigation; it was never
an output target of these tests.

Checks:

- `cargo test -p meshcraft-core -p meshcraft-io -p meshcraft-desktop --locked --offline`:
  726 passed (305 core, 374 desktop library, five CLI, 42 IO), 11 opt-in probes ignored.
- The explicit `insertion_latency_probe` above passes all ten cases and Undo/Redo.
- Strict Clippy over all targets passes; native build reports `Meshcraft 0.33.9`.
- Browser/wasm library check passes with three existing dead-code warnings.
- Formatting, diff and plan/link verification pass; eight release-helper tests pass.


A second snapshot of the user's subsequently saved drawing (2,222,123 bytes,
SHA-256 `d49ac395dc85473237d5a0a13fb34318f3a1a4343c4331f56879f3ece01785b1`)
contains 26,174 container vertices, 28,379 segments and 38,312 triangles. Its wall
commit takes 1.01 seconds. After the Undo budget fix, the sweep that had exceeded
64 MiB commits in 1.40 seconds, refreshes the spatial index in 0.57 seconds, and
passes full serialized-state Undo/Redo checks. These timings have no matching
before baseline on that newer drawing and must not be substituted into the
before/after table.
