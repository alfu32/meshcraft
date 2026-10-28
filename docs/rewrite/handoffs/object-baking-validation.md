# Object baking — #29 / F04 / T32–T34 / 0.42.0

Implemented 2026-10-28. The Objects menu and Object Instance inspector expose
Bake to Shader and Bake to Mesh; MCP discovers the same registered commands.
[Behavior and limits](../spec/objects.md#bake-to-shader-and-bake-to-mesh).

Bake to Shader freezes evaluated mesh/script/subscene content as an immutable GPU
mesh using the existing retained OpenGL meshes and shared surface/line shaders.
Save/Open retains the source data and GPU classification. Bake to Mesh restores
editable mesh objects. Conversions preserve placement, topology/material bindings
and root IFC data; unselected instances retain their prototypes. Nested geometry
is flattened. Static GPU objects cannot enter geometry or script editing before
conversion back to mesh.

Procedural tree baking captures actual vertex shader output via OpenGL transform
feedback on the graphics worker. It honors instance parameters, placement, current
animation time and custom vertex displacement. The host job rejects failed/stale
captures without changing the drawing. Numeric/color generators are supported;
texture-dependent generators fail explicitly. Fragment colors are sampled into
face colors; leaf cards remain cards rather than baking fragment cutout contours.

## Validation

- `cargo build -p meshcraft-desktop --bin meshcraft` — passed, 0.42.0.
- `cargo test -p meshcraft-core operations::objects::bake -- --nocapture` — 4 passed:
  topology/material/placement round trip, private nested conversion with unrelated
  instances intact, and atomic rejection of missing GPU captures or unsupported
  semantic content.
- `cargo test -p meshcraft-desktop --lib bake_tests -- --nocapture` — 2 passed:
  command dispatch, editing guard, project Save/Open, Undo/Redo, GPU errors and
  stale-result rejection. Undo content comparisons allow the intentional revision
  increment used to invalidate viewport caches.
- `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/deps/meshcraft_desktop-a45f0c11be37fc57 gpu_bake_tree_geometry_and_shader_deformation --ignored --nocapture`
  — passed with a private Linux OpenGL context. Covers actual emitted triangles,
  mesh validity, animation, leaf count, translated placement and custom GLSL
  displacement. This is a live GPU execution test, not source inspection.
- `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/deps/meshcraft_desktop-a45f0c11be37fc57 procedural_material_gpu_generation_and_fallback --ignored --nocapture`
  — existing GPU generation/fallback regression passed after the compiler gained
  optional transform-feedback outputs.
- `python3 -m unittest discover -s scripts/stress -p 'test_*.py'` — 16 passed.
- `python3 scripts/export_tool_inventory.py --check --binary target/debug/meshcraft`
  — 289 entries; review columns retained, including the two new command rows.
- `python3 docs/rewrite/scripts/verify_plan.py` — passed.

## Four-tier operation measurements

```sh
python3 scripts/stress/run.py --binary target/debug/meshcraft \
  --output target/stress-object-baking-0.42.0 --samples 1 --timeout 120 \
  --only object.bake_to_shader object.bake_to_mesh
```

[Detailed Markdown](../../performance/object-baking-0.42.0/performance.md),
[CSV](../../performance/object-baking-0.42.0/performance.csv) and
[JSON](../../performance/object-baking-0.42.0/performance.json) retain all 8 cases,
raw samples, binary provenance and failure diagnostics. These are filtered local
software-GL measurements, not full-suite or released-binary CI qualification.

Both commands completed at small, medium and high tiers. Heavy Bake to Shader
failed because its Undo transaction needed 451,499,764 snapshot bytes, exceeding
the existing 268,435,456-byte budget. Heavy Bake to Mesh failed earlier while
creating its prerequisite tree (451,490,102 bytes), leaving that GPU capture
unvalidated. No successful latency is reported for either failure. The fixtures
and Undo budget were unchanged. Same-tag release CI and non-Linux live OpenGL
readback remain unverified; the existing stress workflow has both real recipes.

A final small-tier desktop rerun after the semantic-content and editing-context
guards passed both commands (GPU tree capture and static object freeze). This
smoke rerun is supplementary; the original four-tier observations above remain
unchanged, including both heavy-tier failures.
