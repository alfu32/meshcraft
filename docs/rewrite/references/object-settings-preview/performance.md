# Meshcraft performance — 0.44.0

Source revision / working-tree base: `999a3f04400ca6261e39a2b2dd8549c52bac7ff8` · Binary SHA-256: `a692105dbefd18d70711b979b8fa527846b4614381f68151412788aad9976bca`

Environment: Linux-6.17.0-41-generic-x86_64-with-glibc2.42 · Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz · Mesa software OpenGL / private Xvfb
Provenance: local supplied binary; source revision is a working-tree base, not an exact release claim · Build: local supplied binary (not assumed release optimized) · Activity: local development desktop; normal background activity
Samples per tool/tier: **1** · Started: 2026-10-28T15:26:42.154505+00:00

Coverage: **8 cases** — 1 failed, 7 passed

## What is measured

Real desktop operations through MCP, including world-point input, phantom feedback, committed edits, asynchronous jobs and fresh GPU viewport readiness. Timings use a monotonic client clock and include localhost transport/main-thread scheduling. Selection uses exact entity IDs; these numbers do not measure screen hit-testing or mouse snapping. Session commands measure their intended state transition, not construction geometry. Deferred commands are explicitly unsupported, never successful zero-ms tools.

Setup, fixture loading, prerequisite selection, correctness queries, undo/reset and cleanup are excluded. `total` sums the timed operation stages and wait for a fresh viewport (untimed prerequisite/correctness calls are excluded); it includes the small PNG capture used as a readiness fence, not a GPUI window-paint acknowledgement or GPU-only timer. `commit` includes asynchronous completion where applicable. Initial/final point input is distinguished from preview. Autosave is disabled only in the private benchmark process. Fixed-size operation inputs run against increasingly large resident unique meshes; this is not a claim about transforming all background faces. Supplemental fragment probes for Select, Move and Rotate grow the selection to 24 / 192 / 1,536 / 6,144 primitives across the tiers, and operate on primitive IDs in prototype edit mode.

Hardware and software runs must be compared separately. These are observations, not portable throughput guarantees. No threshold is inferred from a different GPU or CI runner. All raw samples, steps and failure diagnostics are in performance.json; CSV contains every case including failures/unsupported entries.

## Model tiers

| Tier | Stored background vertices | Segments | Triangles | Rendered primitives |
| --- | ---: | ---: | ---: | ---: |
| small | 1,792 | 512 | 256 | 768 |
| medium | 28,672 | 8,192 | 4,096 | 12,288 |
| high | 229,376 | 65,536 | 32,768 | 98,304 |
| heavy | 917,504 | 262,144 | 131,072 | 393,216 |

Workpieces add two shared cube placements and small contour/profile/path objects. Fixture SHA-256 hashes, exact bytes and workload details are recorded in JSON.

## Slowest completed operations

| Tier | Tool / command | Total median (ms) | Total p95 (ms) | Preview median (ms) | Commit median (ms) | MCP owner median (ms) | Viewport frame median (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small | Edit Procedural Tree (`object.edit_tree`, standard) | 3914.407 | 3914.407 | 2405.552 | 528.482 | 90.140 | 103.346 |
| small | New Procedural Tree (`object.new_tree`, standard) | 3309.877 | 3309.877 | 1688.315 | 431.070 | 113.472 | 72.758 |
| heavy | New Procedural Tree (`object.new_tree`, standard) | 3225.778 | 3225.778 | 1319.906 | 236.964 | 30.288 | 1312.455 |
| high | Edit Procedural Tree (`object.edit_tree`, standard) | 2758.288 | 2758.288 | 1209.535 | 747.561 | 38.515 | 366.022 |
| medium | Edit Procedural Tree (`object.edit_tree`, standard) | 2436.688 | 2436.688 | 1413.681 | 222.409 | 51.639 | 81.106 |
| high | New Procedural Tree (`object.new_tree`, standard) | 1639.341 | 1639.341 | 644.279 | 192.436 | 35.627 | 366.976 |
| medium | New Procedural Tree (`object.new_tree`, standard) | 1193.115 | 1193.115 | 602.035 | 159.074 | 31.234 | 56.155 |

## Complete tool coverage

Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.

| Command | small | medium | high | heavy |
| --- | ---: | ---: | ---: | ---: |
| `object.edit_tree` / standard | 3914.407 | 2436.688 | 2758.288 | failed |
| `object.new_tree` / standard | 3309.877 | 1193.115 | 1639.341 | 3225.778 |

## Failures

- `object.edit_tree` / heavy: undo history needs 451490102 snapshot bytes, over its 268435456 byte budget
