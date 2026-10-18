# Scenario and Canonical State Format

Scenario files describe deterministic setup, input, and expected state for acceptance suites in `docs/rewrite/acceptance.md`. `schema.json` defines the event envelope. Each event is a world or screen pointer action, key action, text input, canonical command, or completion wait. World pointer coordinates are in model units; screen coordinates are viewport-relative physical pixels. Commands use stable IDs and JSON arguments.

Canonical model snapshots compare authored state only. Represent definitions with stable IDs, indexed vertices, segments, triangles, RGBA face colors, annotations, and domain payloads. Represent occurrences with stable IDs, definition IDs, parent IDs, origin/basis transforms, and authored overrides. Sort collections by stable ID, preserve vertex order within segments and faces, and compare IDs exactly. Compare numeric coordinates and transforms with named tolerances; compare colors by channel; retain unknown domain fields. Equal positions do not make distinct vertex IDs interchangeable. Exclude renderer caches, selection, and other transient session state from model comparisons.

`python3 docs/rewrite/scripts/canonical_compare.py normalize snapshot.json` emits deterministic JSON after sorting the ID-bearing `definitions`, `child_layouts`, `occurrences`, `entries`, `vertices`, `segments`, `triangles`, `dimensions`, `texts`, `hotspots`, `walls`, `slabs`, `stairs`, `frames`, and `annotations` collections. Primitive vertex-reference order and unknown domain-list order are preserved. It detects missing or duplicate entity IDs instead of silently reordering invalid data. Compare two adapter-produced canonical snapshots with:

```sh
python3 docs/rewrite/scripts/canonical_compare.py compare legacy.json rust.json \
  --tolerance '/definitions/*/geometry/vertices/*/position/*=1e-9,1e-12' \
  --tolerance '/occurrences/*/transform/*/*=1e-9,1e-12'
```

Tolerances use absolute and optional relative components, match JSON-pointer paths (`*` matches one segment), and choose the most specific matching path. IDs, vertex references, and colors always compare exactly. Gzip is detected by magic bytes. This harness compares canonical snapshots; it does not decode legacy DTOs. T05 will provide that adapter before cross-version save/reopen comparisons can run. Exercise the harness with `python3 -m unittest discover -s docs/rewrite/scripts/tests`.

Keep each scenario focused on one acceptance behavior. Include source fixture IDs and hashes in setup, then state expected counts, IDs, colors, transforms, domain data, diagnostics, or unchanged state after failure. Record whether evidence is an automated assertion, a static source observation, or a manual visual check; never describe static inspection as executed behavior.
