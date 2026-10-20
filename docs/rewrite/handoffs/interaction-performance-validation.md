# Interaction and performance corrections — T14/T15/T51/T60

Implemented 2026-10-20 for app version **0.4.0**. These changes retain the full GPUI layout and offscreen raylib image presentation.

## Behavior

- Preview geometry uses the renderer’s portable pixel-width ribbon strokes at `max(thick_line_width, normal_line_width)`. Faces draw before orange outlines. Settings affect preview and model strokes consistently.
- GPUI pointer motion updates Push/Pull, transform, mesh and line-offset tool pointers before requesting feedback. A pending extrusion now displays its edges and faces without accepting another point or changing the model.
- The Copy and Rotate Copy buttons dispatch dedicated `tool.move_copy` / `tool.rotate_copy` commands. They accept mesh fragments and explicitly enable duplication; activating ordinary Move/Rotate retains their ordinary semantics. Copy commits preserve all source primitives, select new identities and undo as one transaction.
- Ordinary Move/Rotate/Scale detach vertices shared by selected edges/faces and unselected primitives. Sharing within the fragment is retained. This fixes unintended deformation of neighboring extrusion faces. Explicit selected vertices remain connected node edits; Stretch variants deliberately retain connected geometry and preview the affected neighbors.
- Isolated planar outlines publish their whole mesh in one batch. Touching, overlapping and self-crossing outlines retain the topology insertion path. Individual line insertion reclusters only touched lines and preserves unrelated IDs, junctions, corner offsets and metadata.
- Selection membership and selected flags are cached when geometry, policy or selection changes. Camera frames reuse them. Object highlighting no longer draws a sphere at each vertex; only explicit vertex selections get markers. Selected patch boundaries are deduplicated; wireframe retains triangle edges. Selection alone does not invalidate the shadow map.
- Middle-button drag pans through the same camera path as Shift+right-button drag. Right-button drag still orbits.

## Automated validation

```sh
cargo test --workspace --offline
cargo build -p meshcraft-desktop --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
python3 docs/rewrite/scripts/verify_plan.py
target/debug/meshcraft --version
```

**494 regular tests passed:** 228 core, 246 desktop, 15 IO and 5 CLI. Two opt-in desktop tests are ignored by the regular suite and were run explicitly below. Build, strict Clippy, formatting, diff and plan validation passed; version output is `Meshcraft 0.4.0`. The existing upstream `proc-macro-error2` future-incompatibility notice remains.

New regressions exercise real extruded topology, detached Move versus connected Stretch, exact source preservation and undo for both copy buttons, non-mutating native extrusion feedback, and a distant circle that must preserve an existing split junction. Existing overlapping/crossing line, raw-corner, transform, nested-instance and object/hotspot tests continue to pass. The explicit GPU test additionally checks that selecting whole objects marks faces without generating vertex markers or redundant closed-mesh triangle outlines.

## Bounded measurements

Environment: Linux 6.17.0-41-generic, Intel Core i7-6700HQ (8 logical CPUs), Mesa 25.2.8 llvmpipe / LLVM 20.1.8, Xvfb `:95`, software OpenGL. Cargo test/dev uses workspace optimization level 1 with dependency optimization level 3. UI smoke checks used software Vulkan. These probes do not qualify physical GPU/DPI/platform coverage or all large-model operations.

The circle and optional selection probes read an immutable temporary snapshot of the local test scene: 1,178,226 bytes, SHA-256 `d6e11ff40fbd5034b394fc47b419d0a334c013fb00579e9d9e0e9cdcae8613b8`. The user scene and temporary snapshot are not included in the repository; supply a local project path to reproduce. Neither probe writes the input file. No timing threshold is asserted.

```sh
# After starting an isolated X11 display, e.g. Xvfb :95 -screen 0 1920x1080x24:
env DISPLAY=:95 LIBGL_ALWAYS_SOFTWARE=1 \
  MESHCRAFT_BENCH_PROJECT=/tmp/meshcraft-perf-source.octd \
  cargo test -p meshcraft-desktop --lib --offline \
  gpu_light_camera_casts_shadows_and_black_lines_survive_composition \
  -- --ignored --test-threads=1 --nocapture

MESHCRAFT_BENCH_PROJECT=/tmp/meshcraft-perf-source.octd \
  cargo test -p meshcraft-desktop --lib --offline circle_construction_latency_probe \
  -- --ignored --nocapture
```

| Probe | Before | After |
| --- | --- | --- |
| 24-segment circle commit on local scene | 60.591 s | 248.40 ms |
| First spatial update after circle commit | 46.44 ms | 50.13 ms |
| Unselected render + readback, 1280×800 | Not measured | 23.22 ms/frame |
| 5,183 selected faces/edges, same scene/camera | Not measured | 23.83 ms/frame |

Circle time includes the real session edit/history path. The intermediate localized-line change measured 348.51 ms; adding isolated-outline batching reduced the final commit further. The circle baseline is this scene’s measured probe, not the user’s reported 2–4 seconds. History/validation/index work remains and the general 50 ms action goal is not claimed complete.

Selection timings average six warmed frames each, include GPU readback, and disable ground/grid and shadows to isolate selection rendering. The measured selected overhead is about 0.61 ms (2.6%); no pre-patch frame baseline was captured. The full GPU regression passed shadow alignment (IoU 0.919), ground/entity receivers, black line widths, all shader-AA qualities and display-policy transitions.

## Live GPUI observations

Ran the rebuilt app at 1800×1000 on the isolated display, opening a temporary copy of `examples/dynamic-object.octd`. `xdotool` drove the actual GPUI mouse handlers and ImageMagick captured the window after redraw. Tracked examples and user projects were not edited.

- Middle-button down/drag/up translates both visible instances and grid without changing orientation or selection. Returning the drag restores the framing.
- Entering a prototype and selecting one face enables Copy and Rotate Copy. Clicking Copy, picking a reference and moving shows only the displaced fragment in thick orange feedback; the source remains visible. Committing increased the displayed face total from 24 to 26 (one new triangle in the shared prototype, two visible instances); undo restored 24.
- Selecting a side face, clicking Extrude and moving after the first reference displays the full pending volume and thick edges, with relative measurements in GPUI. This capture is before the second pick; no extrusion has been committed.

| Evidence | Screenshot |
| --- | --- |
| Original camera framing | [Before pan](../references/interaction-performance/before-pan.png) |
| Middle-button pan, unchanged selection | [Panned camera](../references/interaction-performance/middle-pan.png) |
| Enabled Move Copy and isolated fragment preview | [Copy preview](../references/interaction-performance/move-copy-preview.png) |
| Pending Push/Pull volume and readable edges | [Extrusion preview](../references/interaction-performance/extrude-preview.png) |

T51/T60 and broad performance/platform acceptance remain in progress. Open GitHub #27 concerns future shading/frustum work and is not resolved by these selection optimizations; postponed #26 remains unchanged.

## Follow-up tolerance and pan tuning — 0.4.1

Connected and coplanar face/line selection now uses a non-mutating spatial adjacency graph at `1e-2` model units. Separate vertex identities and sparse raw corner positions can connect by proximity. Adjacent buckets are checked by actual distance; connected expansion traverses indexed adjacency instead of repeatedly scanning the whole graph. Authoritative geometry and legacy topology insertion tolerances stay intact. A regression connects faces/lines across a 0.006-unit gap, excludes a separate fragment beyond tolerance, and verifies the document is unchanged.

Middle and Shift+right panning use half the previous multiplier (`distance × 0.00125` rather than `distance × 0.0025`). **495 workspace tests pass** (228 core, 247 desktop, 15 IO, 5 CLI); build, strict Clippy, formatting and plan validation pass. The user confirmed that the connection tolerance is in model units.
