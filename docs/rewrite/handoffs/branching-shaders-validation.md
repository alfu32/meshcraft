# Conditional branching shaders — #29/#31 / 0.45.0

Implemented the user's recursive branching proposal as a GLSL 3.3 independent
object generator, `meshcraft.branching.v1`. New Tree and default
`object.configure_procedural` settings use it; explicit legacy IDs and persisted
`meshcraft.tree.v1` objects retain their original evaluator. Six families share
one rule engine: Broadleaf, Conifer, Weeping, Shrub, Coral and Roots.

The implementation and its finite limits, mathematical scope, controls and GLSL
ABI are specified in [the branching shader contract](../../branching-shaders.md).
It reconstructs inherited curve frames from stable slot-path IDs, uses deterministic
seeded rules, and emits curved tapered tubes/leaf cards. Growth/survival and
subpixel hierarchy LOD fade geometrically; integer arity/depth remain discrete.
The editable `branchingRule` hook can inspect parent curve/radius data and change
per-child transformation or survival. MCP publishes source, schema, signatures,
field descriptions, the canonical injected GLSL `ruleType` and limits. Roots get downward conservative bounds; custom
vertex deformation can use program bounds expansion.

An initial direct-per-vertex path was expensive on Mesa software rendering. The
viewport now uses a GPU transform-feedback state pass: each branch is evaluated
once, then tube and leaf vertices sample four RGBA32F texels per branch. Buffers
are reused and shared per compiled program, with at most 1 MiB for the 16,384
branch-candidate ceiling. No CPU branch graph, vertex uploads of generated
geometry, mapped/read-back state or geometry commits occur during rendering.
Time-dependent rules run on the GPU. Direct shader evaluation remains the mesh
bake path; GPU parity tests compare it with the state-reuse path. Shadow/depth and
preview rendering share that same rule/evaluator machinery.

## Verification

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --locked`: **769 passed, 21 ignored** GPU checks.
  Includes new owner persistence/private-copy/downward-bounds/invalid-input checks
  and bounded draw counts, while retaining the legacy tree path.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo build --locked -p meshcraft-desktop --bin meshcraft`: passed, 0.45.0.
- Real OpenGL tests were explicitly run in separate private X11 contexts using
  `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/deps/meshcraft_desktop-cfea067ee611b836 NAME --ignored --nocapture`:
  - `branching_shader_families_bounds_continuity_and_custom_rules`: passed.
    Actual generated/baked geometry for all six families, finite vertices within
    declared bounds, downward roots, smooth growth samples, trunk-only pruning
    and a custom GLSL rule controlling survival.
  - `branching_shader_path_identity_is_independent_of_draw_ordinals`: passed.
    GPU feedback preserves a budget prefix and surviving slot paths across a
    primary-arity change, with no CPU branching reference implementation.
  - `branching_gpu_state_reuse_matches_direct_shader_vertices`: passed.
    Cached/direct tube and leaf positions/normals agree within 0.0002 in all six
    families; the GPU buffer/capacity are reused.
- `python3 scripts/verify_branching_shaders.py`: passed. Opens the persisted
  six-family example and verifies six owners with **zero authored vertices and
  triangles**, actual GPU output without diagnostics, published generator limits,
  live instance preview and stable idle frames with zero wind. Saved native
  viewport and settings captures below. Mesa software OpenGL/Vulkan, private
  1280×900 desktop and isolated preferences; no hardware-driver claim.
- `python3 scripts/verify_object_preview.py --output target/branching-preview-final`:
  passed. Four new-generator open/cancel cycles render 3447 candidates in three
  draws each and return to idle. A genuine legacy fixture verifies typed edits,
  validation, camera controls, nested shader-editor return and Save/reopen with
  exactly one additional history entry (revision/history 1 → 2). Early automation
  probes assumed the old form/popup layout; they were diagnostic attempts, not
  successful measurements. The final helper explicitly creates its legacy fixture.
- `python3 -m unittest discover -s scripts/stress -p 'test_*.py'`: 17 passed.
- Inventory export/check, diff whitespace and planning validation passed: 289
  commands, no added IDs and preserved menu-review assignments.

[Six-family viewport](../references/branching-shaders/families.png).
[Object settings preview](../references/branching-shaders/settings.png).
[Native scene/idle observations](../references/branching-shaders/observations.json).
[Preview/save regression](../references/branching-shaders/preview-regression.json).
[Example drawing](../../../examples/branching-families.octd).

## Four-tier operation measurements

`python3 scripts/stress/run.py --binary target/debug/meshcraft --only object.new_tree object.edit_tree object.configure_procedural object.bake_to_mesh --samples 1 --timeout 90 --output target/branching-stress`
produced all 16 selected cases on unchanged small/medium/high/heavy fixtures:
**13 passed, 3 failed**. Heavy configure/place, edit and mesh bake hit the existing
Undo snapshot limit while preparing/creating the owner: 451490107 bytes required
against 268435456 available. These remain explicit failures. New Tree's actual
GPU editor preview completed on all four tiers. Configure/place and bake recipes
exercise actual authored insertion and shader readback rather than command
activation timing. No fixture or application limit was changed.

[Markdown report](../references/branching-shaders/performance.md),
[CSV](../references/branching-shaders/performance.csv), and
[JSON](../references/branching-shaders/performance.json) retain timing stages,
failed prerequisites, fixture counts and local candidate binary hashes. This is a
partial local development run with software rendering, not a complete suite or
an exact released-binary performance claim. Final help/documentation refinements
followed the measured candidate; generator code is the same. No 0.45.0 CI/release
execution is claimed here. The prior 0.44.1 six-platform release completed
successfully while this feature was being developed.
