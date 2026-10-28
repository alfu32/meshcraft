# Meshcraft performance — 0.42.0

Source revision / working-tree base: `0c7e4e97604b8a8281cf52afa71d08dca285e0c3` · Binary SHA-256: `b525e5943a925c85d036a3f0ccbb5b86d4c899ee2e4ddd63bdb44f4259a656ae`

Environment: Linux-6.17.0-41-generic-x86_64-with-glibc2.42 · Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz · Mesa software OpenGL / private Xvfb
Provenance: local supplied binary; source revision is a working-tree base, not an exact release claim · Build: local supplied binary (not assumed release optimized) · Activity: local development desktop; normal background activity
Samples per tool/tier: **1** · Started: 2026-10-28T04:04:58.720468+00:00

Coverage: **8 cases** — 2 failed, 6 passed

## What is measured

**Filtered local development qualification: only the two object-bake commands (8 cases), not the full 289-command suite or a released-binary CI baseline.** One sample per tier, development build, Mesa software OpenGL in a private desktop. The working-tree base alone does not reproduce the uncommitted implementation; the recorded SHA-256 identifies the frozen executable actually tested.

Bake to Shader freezes the fixture cube. Bake to Mesh creates and places a procedural tree as untimed setup, then measures actual GPU readback and mesh commit. The normal GPU-mesh round trip is separately covered by session unit tests.

Six operation cases completed. Heavy Bake to Shader failed its transaction at the existing Undo budget; Heavy Bake to Mesh failed while creating its prerequisite tree, so the heavy GPU capture itself remains unvalidated. Failures remain failures and have no successful latency values. Fixture weights and the 268435456-byte Undo limit were unchanged.

The existing manual stress-test.yaml workflow discovers both commands through the same registry and recipes. Same-tag released-binary CI measurements have not run for this version.

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
| high | Bake to Mesh (`object.bake_to_mesh`, standard) | 2704.183 | 2704.183 | — | 2557.295 | 100.720 | 531.452 |
| medium | Bake to Mesh (`object.bake_to_mesh`, standard) | 1708.136 | 1708.136 | — | 1563.052 | 28.261 | 328.958 |
| small | Bake to Mesh (`object.bake_to_mesh`, standard) | 1686.590 | 1686.590 | — | 1552.216 | 14.518 | 186.917 |
| high | Bake to Shader (`object.bake_to_shader`, standard) | 1575.052 | 1575.052 | — | 1414.481 | 1292.930 | 367.472 |
| medium | Bake to Shader (`object.bake_to_shader`, standard) | 423.362 | 423.362 | — | 284.180 | 177.203 | 66.899 |
| small | Bake to Shader (`object.bake_to_shader`, standard) | 289.939 | 289.939 | — | 150.144 | 43.154 | 19.706 |

## Complete tool coverage

Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.

| Command | small | medium | high | heavy |
| --- | ---: | ---: | ---: | ---: |
| `object.bake_to_mesh` / standard | 1686.590 | 1708.136 | 2704.183 | failed |
| `object.bake_to_shader` / standard | 289.939 | 423.362 | 1575.052 | failed |

## Failures

- `object.bake_to_mesh` / heavy: undo history needs 451490102 snapshot bytes, over its 268435456 byte budget
- `object.bake_to_shader` / heavy: undo history needs 451499764 snapshot bytes, over its 268435456 byte budget
