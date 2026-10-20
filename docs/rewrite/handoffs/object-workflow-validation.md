# Object workflow validation — T32/T33/T34

Verified on 2026-10-20. This record covers the implemented object workflow; it does not complete every legacy fixture, domain adapter, platform or performance acceptance case in C07/C08/C15/C16.

## Implemented behavior

- The Objects browser lists shared prototypes once, with internal/external identity and occurrence counts. Double-click starts snapped insertion with a complete subscene wireframe, independently of an existing instance selection.
- Double-clicking a viewport instance enters its shared prototype. Own geometry is authored in place; outside geometry is faded, supplies snapping references, and is excluded from selection/modification. Nested structural edits update the shared child layout. Escape and Finish Editing exit one actual object level, skipping the imported scene wrapper.
- Hotspots expose names, eight operations, five shapes, RGBA colors, geometry/handle attachments and local reference points. Selecting a hotspot opens and scrolls to its inspector. Definition editing is enabled only for the authored scene/prototype.
- Closed-instance drags recompute attached geometry from the prototype and absolute instance values. Prototype-edit drags reposition the shared marker without running its operator. Drag feedback uses GPUI and includes relative deltas/distance. Release commits one history edit; Escape cancels the draft.
- Copies preserve parameter values; later values are independent. Nested authored defaults and outer occurrence overrides remain distinct, including resets to the authored anchor. Geometry changes reconcile attachment identities; deleting a hotspot clears known values and stale baked runtime geometry.
- Import/reference/refresh loads complete external subscenes on a worker. Root hotspots, nested instance defaults and unused prototypes survive project roundtrips. Failed refresh retains cached geometry; generation checks prevent publishing into a replaced document. External files are deliberately refreshed rather than recursively loaded on every frame.

The source characterization is in [the object specification](../spec/objects.md). The authored [example](../../../examples/dynamic-object.octd) is not pinned legacy fixture evidence.

## Automated checks

All commands ran from the repository root:

```sh
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo build -p meshcraft-desktop --offline
cargo fmt --all --check
git diff --check
python3 docs/rewrite/scripts/verify_plan.py
```

Results: 225 core, 239 desktop, 15 IO and 5 CLI tests passed; doc/stack targets passed with no test cases. The regular suite ignores one explicit GPU regression. Strict Clippy, the application build, formatting and diff checks pass. Plan verification reports 61 tasks, 214 slices, 63 requirements, 41 suites and 74 built-in tools with valid references/dependencies. Cargo reports the existing upstream future-incompatibility notice for `proc-macro-error2`.

Regression coverage includes all operator variants, independent targets, copies, undo, cycles, legacy DTO spelling, sparse coincident corner identities, split attachments, removal of baked runtime caches, nested shared layouts/default resets, external hierarchy refresh, project save/reopen, background import completion without pointer input, failed refresh retention, spatial cache invalidation on editing transitions and finishing an imported top-level object directly into scene mode.

## Live GPUI checks

The application was built and launched on an isolated Linux X11 display with software Vulkan:

```sh
Xvfb :93 -screen 0 1920x1080x24 -nolisten tcp
env -u WAYLAND_DISPLAY DISPLAY=:93 \
  VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  XDG_CONFIG_HOME=/tmp/meshcraft-objects-final-config \
  target/debug/meshcraft --open /tmp/meshcraft-objects-final.octd --size 1800x1000
```

The temporary project began as a copy of `examples/dynamic-object.octd`; testing did not edit the tracked example. Input was driven with `xdotool` and window captures taken with ImageMagick `import` after allowing redraws. Earlier captures used the equivalent isolated display `:92`. A later lost Xvfb display caused a Vulkan surface-loss error in that test process; restarting the test display and reopening the saved project succeeded. This does not establish resilience to losing the X server.

| Check | Observed result | Evidence |
| --- | --- | --- |
| Closed-instance Stretch drag | Preview and relative measurements update; second instance retains its original geometry; property inputs are read-only | [Runtime drag](../references/objects/20-gpui-instance-drag.png) |
| Shared prototype editing | Base geometry is restored for authoring; other instance is faded; operation/shape/color controls enable; Finish Editing is visible | [Authoring and color picker](../references/objects/21-gpui-hotspot-authoring.png) |
| Marker repositioning | Prototype-edit drag moves the marker with “Hotspot position” feedback while authored geometry stays unchanged | Covered live; automated shared-session regression |
| Prototype insertion | Double-click row with no source selected starts a wireframe preview; a click creates a third instance and increments the existing prototype's count | [Insertion](../references/objects/22-gpui-prototype-insertion.png) |
| Save/reopen and finish | Three instances reopen under one prototype; independent deformation and shared shape/color remain; Finish Editing returns to scene mode in one click | [Reopened scene after finish](../references/objects/23-gpui-save-reopen-finish.png) |
| Active tools (#28) | A selected tool has a red indicator in ribbon/palette; an enabled hovered button has visibly stronger contrast | [Active tool and hover](../references/objects/24-gpui-active-tool.png) |

These are interaction observations, not GPU timing qualification. Software Vulkan timings must not be used as hardware FPS claims.

## Remaining broader acceptance

T32/T33/T34 remain in progress because the full pinned legacy fixture matrix, large nested/array memory and latency qualification, external path/cycle edge cases and packaged platform checks are not yet complete. Existing unsupported domain adapters still refuse grouping/explode of some semantic payload combinations; this change does not claim those broader lifecycle cases. The implemented prototype/hotspot workflow above is available for user testing.
