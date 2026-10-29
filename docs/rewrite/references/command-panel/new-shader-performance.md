# Filtered local probe: New Shader only

This development run measures **one of 290 registered commands**, with one sample
per model tier. It is not a complete application stress report or a release-binary
benchmark. The command opens the shader editor, renders its preview and cancels.

# Meshcraft performance — 0.46.1

Source revision / working-tree base: `06d46796364be7ade3e16ce483026038722fe52f` · Binary SHA-256: `b5b9a9e158b19f32460adaccc03f8419ef6952d61579c7ef1ab64b3d88f9d8f7`

Environment: Linux-6.17.0-41-generic-x86_64-with-glibc2.42 · Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz · Mesa software OpenGL / private Xvfb
Provenance: local supplied binary; source revision is a working-tree base, not an exact release claim · Build: local supplied binary (not assumed release optimized) · Activity: local development desktop; normal background activity
Samples per tool/tier: **1** · Started: 2026-10-29T13:13:01.071671+00:00

Coverage: **4 cases** — 4 passed

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
| heavy | New Shader (`material.new_shader`, standard) | 2902.350 | 2902.350 | 689.715 | 16.407 | 41.670 | 1610.439 |
| high | New Shader (`material.new_shader`, standard) | 2527.860 | 2527.860 | 887.417 | 17.639 | 35.543 | 725.874 |
| medium | New Shader (`material.new_shader`, standard) | 2192.239 | 2192.239 | 1278.747 | 17.276 | 44.598 | 97.857 |
| small | New Shader (`material.new_shader`, standard) | 1490.501 | 1490.501 | 684.843 | 17.089 | 34.336 | 33.831 |

## Complete tool coverage

Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.

| Command | small | medium | high | heavy |
| --- | ---: | ---: | ---: | ---: |
| `material.new_shader` / standard | 1490.501 | 2192.239 | 2527.860 | 2902.350 |
