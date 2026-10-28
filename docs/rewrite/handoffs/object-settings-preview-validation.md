# Procedural object settings preview — #29 / T65 / 0.44.0

The procedural object settings form now has a live GPU object preview on its
right. The dialog occupies 96% of the window width (up to 1500 px) and 90% of its
height. Settings scroll independently; Apply, Cancel and Save stay visible.
Right-drag orbits, middle/Shift-right drag pans, the wheel zooms, and Fit View
frames the current draft. Prototype defaults and effective instance settings use
the same draft parser as Save.

The private preview contains one procedural owner and its chosen base material;
it does not clone the source model. Valid input is debounced, and invalid input
keeps the last valid geometry with an error. Camera and animation updates do not
rebuild the document. The nested shader editor restores the parent draft/preview
on return, including its camera after Cancel. Closing settings restores the main
viewport. Apply and Save retain the existing commit scopes.

Tree owners now contribute their animation state to the renderer, including in
private preview scenes. Built-in preview program aliases retain their source
identity so zero wind pauses the built-in animation. Atlas-image retirement waits
for a full presentation opportunity before eviction: immediate or single-callback
eviction could remove a queued Blade upload and crash repeated editor transitions.
The validation below reproduces repeated transitions and checks idle rendering.

`meshcraft_capture_view` accepts optional `preview: true` to capture the active
editor viewport; the default still captures the drawing. The result reports its
preview scope, and drawing revision/generation remain drawing metadata. The two
object-editor performance recipes now wait for a real preview frame, reject its
shader diagnostics and close the dialog before fencing the main viewport.

## Verification

- `cargo build -p meshcraft-desktop --bin meshcraft`: passed, version 0.44.0.
- `cargo test -p meshcraft-desktop --lib --no-run`: passed. The resulting native
  test executable was used for the focused tests below.
- `target/debug/deps/meshcraft_desktop-a786f7260de60249 object_form_preview --nocapture`:
  1 passed. Private effective values, copied base material, one owner/no authored
  mesh, invalid-value rejection and unchanged source model.
- `target/debug/deps/meshcraft_desktop-a786f7260de60249 materials::shader_editor::tests --nocapture`:
  3 passed. Every built-in private preview, compatible values and shader metadata.
- `target/debug/deps/meshcraft_desktop-a786f7260de60249 mcp::protocol --nocapture`:
  4 passed, including preview capture argument validation and image/metadata routing.
- `xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 target/debug/deps/meshcraft_desktop-a786f7260de60249 procedural_object_form_gpu_preview_animates_and_pauses --ignored --nocapture`:
  passed. Actual rendered leaf pixels; wind enables animation, zero wind pauses
  it, and returning to an empty drawing stops preview animation.
- `python3 scripts/verify_object_preview.py`: passed in a private 1280×900 X11
  desktop with Mesa software OpenGL/Vulkan, isolated preferences and real MCP.
  Four open/cancel cycles each rendered 3460 candidates in two draws without
  diagnostics. Each returned to a stable main-view frame sequence. Draft edits,
  invalid input, Fit View, orbit/pan/zoom and a nested shader-editor round trip
  left source data/revision/history unchanged. Save persisted height 12 and wind
  zero with revision/history **0 → 1**, and the saved form reopened successfully.
  The visible invalid-input error and retained preview were inspected in the
  captured native window; the script does not perform OCR of that message.
- `python3 -m unittest discover -s scripts/stress -p 'test_*.py'`: 17 passed,
  including preview readiness/diagnostic and dialog-close guards.
- Tool inventory, Rust formatting, diff whitespace and planning checks passed;
  289 commands and user review assignments were preserved.

[Native settings/preview capture](../references/object-settings-preview/object-settings-preview.png).
[Invalid draft retains its preview](../references/object-settings-preview/invalid-draft.png).
[Parent restored after shader editor](../references/object-settings-preview/restored-settings.png).
[Live transition/save observations](../references/object-settings-preview/observations.json).

`python3 scripts/stress/run.py --binary target/debug/meshcraft --only object.new_tree object.edit_tree --samples 1 --timeout 90 --output target/object-preview-final-stress`
produced all eight cases: seven passed; heavy Edit failed during creation of its
prerequisite because Undo required 451490102 snapshot bytes against its existing
268435456-byte budget. New Tree preview passed on all four tiers.
[Markdown results](../references/object-settings-preview/performance.md),
[CSV](../references/object-settings-preview/performance.csv), and
[JSON](../references/object-settings-preview/performance.json) retain the measurements,
fixture counts, source-state qualification and binary hash. The tool shell reported
143 after the runner had written its finished timestamp, complete eight-case ledger
and summary; this is recorded separately from the per-case results. The report's
non-gating exit policy evaluates to zero. No CI/release run is claimed here. These are partial development measurements using the native
debug-profile binary and software rendering, not release performance claims.
Heavy-model setup failures remain failures; no fixture or Undo budget was changed.
Windows/macOS and hardware-driver execution were not performed for this change.
