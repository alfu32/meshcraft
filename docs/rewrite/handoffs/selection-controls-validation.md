# Selection controls and point input — T17/T60

Implemented 2026-10-20, following the user-tested object workflow. This slice activates the GPUI selection table, applies the requested lighting defaults, and restores keyboard completion/modes for multipoint tools. It does not add rendering adapters for deferred voxel/architecture/HVAC/text domains.

## Controls and input contracts

- Type/T/S/V/E/W are aligned table columns; T/S remain numeric and include hidden entities. V/E/W have per-row and whole-column checkboxes. Hidden types remain read-only, and E is disabled for a hidden row. Showing a type restores its default editability. Combined rows affect each represented type. W has no additional visual effect on geometry that is already line-only.
- Visibility, selection, marquee, hotspot handles and raylib draw/shadow caches consume the same session policy. Object toggles exempt the imported scene wrapper. Locked geometry remains available for snapping. Policy changes cancel drafts and prune selected targets; command completion also prunes newly created hidden targets so they cannot be modified via a retained selection. Policy-only updates reuse the immutable model snapshot.
- Object and face wireframe remove fills and solid shadow casting; hotspot W draws outline markers. Hidden entities do not cast residual shadows. The renderer rebuilds its policy-dependent caches when these flags change, even when the document snapshot is unchanged.
- Shader defaults match [the supplied preset](../references/selection-controls/requested-shader-defaults.png): azimuth −35°, elevation 55°, directional 0.73, ambient 0.64, specular 0.15, shadow alpha 0.5, depth bias 0.00025, normal bias 0.037, shadows enabled and Medium AA. Explicit saved AA preferences remain respected.
- GPUI Enter dispatches `tool.finish`; Backspace dispatches `tool.undo_point`. Polyline and Double Line commit their accepted path, sweep/ribbon tools finish their path stage, Mesh ends its chain, and existing transform/revolve finish contracts remain authoritative. Insufficient input reports the existing command-state reason instead of committing invalid geometry.
- All five path tools consume A/C/L for through-point arc, center-based circular arc and straight segment modes. Accepted points survive mode changes. Shift+L explicitly cycles planar tools; plain L still cycles outside path input. The legacy input host now exempts every active path from plain-L cycling. Palette, menus and focused text fields keep keyboard ownership.
- Native component check/chevron/search/close SVGs are embedded in the application AssetSource; no runtime fetch or new dependency is required.

## Automated checks

```sh
cargo test --workspace --offline
cargo build -p meshcraft-desktop --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo fmt --all --check
git diff --check
python3 docs/rewrite/scripts/verify_plan.py
```

Workspace checks cover policy hierarchy, visibility/lock coupling, whole-column and grouped controls, selection pruning, hidden geometry created by commands, command compatibility aliases, Enter commit/cancel behavior, A/C/L across all five paths and the old Double Line/L collision. A preset regression checks every numeric lighting default.

The explicit renderer regression ran on an isolated X11/OpenGL display:

```sh
env DISPLAY=:94 LIBGL_ALWAYS_SOFTWARE=1 cargo test -p meshcraft-desktop --lib --offline \
  gpu_light_camera_casts_shadows_and_black_lines_survive_composition \
  -- --ignored --test-threads=1 --nocapture
```

It passed with the requested lighting defaults: analytic ground-shadow alignment IoU 0.919, 2,806 ground-shadow receiver pixels and 1,778 entity-shadow receiver pixels. Existing black-line and shader-AA checks passed. New assertions verify policy-only cache changes remove hidden faces, preserve independent visible edges, mark object faces wireframe, and remove all hidden-object geometry/handles without cloning the document.

## Live GPUI observations

The built app ran on Xvfb with software Vulkan, opening a temporary copy of `examples/dynamic-object.octd`. Tests used `xdotool` input and window captures after redraws; tracked example geometry was not modified. These are functional observations, not hardware FPS measurements.

- Native checkboxes display their ticks; totals/selected counts remain aligned.
- Locking Objects clears an existing object selection and rejects another canvas click on that object. The visible object still supplies cursor snapping.
- Object wireframe removes fills and solid shadows. Hiding Objects leaves scene-owned path lines; the imported scene wrapper is not hidden with placed instances.
- Header visibility controls affect every row; hidden rows cannot be re-enabled for editing through E alone.
- Three-point Polyline input followed by Enter commits two segments. Double Line input using A for an arc, then L for a straight segment, finishes with Enter (sixteen tessellated path points in the final observed case). The tool remains Double Line throughout; Escape after commit retains its geometry.

Final results: **490 regular workspace tests passed** (226 core, 244 desktop, 15 IO, 5 CLI); the explicit GPU check above also passed. The regular suite ignores that GPU test. Build, strict Clippy, formatting and diff checks passed. Plan validation reports 61 tasks and 216 slices with valid links/dependencies; no broad task status was changed. The existing upstream proc-macro-error2 future-incompatibility notice remains.

| Live evidence | Screenshot |
| --- | --- |
| Working table and native checkmarks | [Selection table](../references/selection-controls/table.png) |
| Object wireframe | [Wireframe](../references/selection-controls/wireframe.png) |
| Hidden objects with scene lines retained | [Object visibility](../references/selection-controls/objects-hidden.png) |
| Visible locked objects with selection cleared | [Read-only objects](../references/selection-controls/read-only.png) |
| Exact requested shader defaults and working dropdown arrow | [Shader defaults](../references/selection-controls/shader-defaults.png) |
| Arc followed by straight segment, committed with Enter | [Finished path](../references/selection-controls/enter-path.png) |
