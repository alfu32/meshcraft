# Meshcraft performance — 0.45.0

Source revision / working-tree base: `3446de2520da1f897b8e8058c43e3ab10c793b38` · Binary SHA-256: `0211ae39e86babaf60be32973fffb25b05591170c4be12c4e1e1819d743d6e08`

Environment: Linux-6.17.0-41-generic-x86_64-with-glibc2.42 · Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz · Mesa software OpenGL / private Xvfb
Provenance: local supplied binary; source revision is a working-tree base, not an exact release claim · Build: local supplied binary (not assumed release optimized) · Activity: local development desktop; normal background activity
Samples per tool/tier: **1** · Started: 2026-10-28T20:10:32.007750+00:00

Coverage: **16 cases** — 3 failed, 13 passed

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
| high | Bake to Mesh (`object.bake_to_mesh`, standard) | 4402.049 | 4402.049 | — | 4220.992 | 109.376 | 648.094 |
| high | Configure Procedural Object (`object.configure_procedural`, standard) | 3818.269 | 3818.269 | 640.597 | 2828.937 | 2948.069 | 61.140 |
| heavy | New Procedural Tree (`object.new_tree`, standard) | 3766.135 | 3766.135 | 1528.493 | 219.640 | 28.550 | 1406.260 |
| high | Edit Procedural Tree (`object.edit_tree`, standard) | 2113.765 | 2113.765 | 698.992 | 543.573 | 56.871 | 462.327 |
| medium | Bake to Mesh (`object.bake_to_mesh`, standard) | 2093.262 | 2093.262 | — | 1941.638 | 25.132 | 258.741 |
| medium | New Procedural Tree (`object.new_tree`, standard) | 1909.292 | 1909.292 | 1023.967 | 230.053 | 45.576 | 91.173 |
| small | Bake to Mesh (`object.bake_to_mesh`, standard) | 1827.631 | 1827.631 | — | 1678.663 | 18.490 | 196.247 |
| high | New Procedural Tree (`object.new_tree`, standard) | 1637.471 | 1637.471 | 747.966 | 162.436 | 28.463 | 338.494 |
| small | Edit Procedural Tree (`object.edit_tree`, standard) | 1403.646 | 1403.646 | 723.326 | 223.981 | 25.600 | 20.149 |
| small | New Procedural Tree (`object.new_tree`, standard) | 1378.636 | 1378.636 | 780.863 | 161.797 | 27.535 | 19.250 |
| medium | Edit Procedural Tree (`object.edit_tree`, standard) | 1348.957 | 1348.957 | 696.963 | 193.898 | 28.871 | 55.844 |
| medium | Configure Procedural Object (`object.configure_procedural`, standard) | 1212.323 | 1212.323 | 318.566 | 578.141 | 408.337 | 34.165 |
| small | Configure Procedural Object (`object.configure_procedural`, standard) | 885.778 | 885.778 | 280.228 | 291.740 | 84.939 | 20.313 |

## Complete tool coverage

Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.

| Command | small | medium | high | heavy |
| --- | ---: | ---: | ---: | ---: |
| `object.bake_to_mesh` / standard | 1827.631 | 2093.262 | 4402.049 | failed |
| `object.configure_procedural` / standard | 885.778 | 1212.323 | 3818.269 | failed |
| `object.edit_tree` / standard | 1403.646 | 1348.957 | 2113.765 | failed |
| `object.new_tree` / standard | 1378.636 | 1909.292 | 1637.471 | 3766.135 |

## Failures

- `object.bake_to_mesh` / heavy: undo history needs 451490107 snapshot bytes, over its 268435456 byte budget
- `object.configure_procedural` / heavy: undo history needs 451490107 snapshot bytes, over its 268435456 byte budget
- `object.edit_tree` / heavy: undo history needs 451490107 snapshot bytes, over its 268435456 byte budget
