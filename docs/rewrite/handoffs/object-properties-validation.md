# Object metadata, materials and color controls — 0.27.0

Scope: T16/T17/T38/T61/T62. Observed on Linux with isolated Xvfb, a software GPU
and the native GPUI/raylib application at 1500×1000. The user's running desktop
application and model files were not changed by the probes.

## Native observations

- Object Instance / Object Prototypes labels and left-aligned prototype rows are
  visible, with an orthographic isometric preview. Reopening regenerates it.
- Prototype EI60→EI120 edits leave the existing instance at EI60. A newly placed
  instance receives EI120; an instance override EI90 is retained by its copy.
- Save/Open preserves EI60/EI90/EI90. Deleting a property after reopening and saving
  again preserves its absence on the next Open, with no prototype fallback.
- Deleting a script-defined prototype default stays deleted after reevaluation;
  a new placement does not receive the omitted field.
- A shared width change refreshes the thumbnail. A main-camera pose change leaves
  its pixels unchanged. The native IFC dialog exposes the catalog search and all
  record/options fields. An earlier native Add interaction saved a custom default
  without changing the already-created instance.
- HSL opens by default, with numeric channels, a smooth two-dimensional swatch,
  RGBA hex and opacity. Switching to RGB retains the edited opacity. Saving RGB
  255/128/0 at 25% produces RGBA 255/128/0/64. Switching to Swatches and selecting
  blue preserves alpha 64.

Evidence: [prototype browser](../references/object-properties/ifc-prototypes-final.png),
[IFC editor](../references/object-properties/ifc-editor-final.png),
[updated thumbnail](../references/object-properties/ifc-thumbnail-updated-final.png),
[HSL](../references/object-properties/color-hsl-final.png),
[RGB](../references/object-properties/color-rgb-final.png),
[Swatches](../references/object-properties/color-swatches-final.png),
[IFC results](../references/object-properties/ifc-native-validation-final.json),
[color results](../references/object-properties/color-native-validation.json).

## Automated checks

- `cargo test --workspace --quiet`: 664 regular tests passed (281 core, 336 desktop,
  five CLI, 42 IO), five manual probes ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build -p meshcraft-desktop --bin meshcraft`: passed.
- `cargo check -p meshcraft-desktop --lib --target wasm32-unknown-unknown`: passed,
  with three existing warnings about native-only walkthrough/clipboard methods.
- `cargo fmt --all -- --check` and separate vendored picker rustfmt: passed.
- `python3 scripts/ifc/catalog.py`: 3,988 entries, 760 sets, 436 types; regenerating
  the catalog leaves its SHA-256 unchanged. Unmodified source hashes match the
  [pinned attribution](../../../vendor/ifc4.3.2.0/README.md).
- `python3 -m unittest discover -s scripts/release -p 'test_*.py' -v`: eight passed.
- `python3 docs/rewrite/scripts/verify_plan.py`: checks task graph and local links.

Regression coverage includes prototype snapshots and instance deletion after
reopening, script/onCreate scopes and immutable readers, one-shot metadata baking,
selection-wide IFC edits/Undo, all property kinds, mixed table axis types and
units, Unicode IFC round trips, equal-color material identities, architecture
regeneration and imported material-ID collision handling. Prototype omissions are
retained so a script or a stale evaluation cannot recreate a user-deleted default.

## Limits

This validates the property model and native controls, not full IFC model-view
certification. IFC files retain tessellated/linework proxies; arbitrary BIM
relationship graphs and material styles remain outside the exporter. Supported
explicit units and reference restrictions are documented in the
[IFC guide](../../ifc-properties.md). Windows/macOS and physical-GPU UI checks
were not run in this local pass. Unplaced/nested scripted thumbnail isolation is
implemented through the existing preview pipeline; it was inspected in code, not
separately exercised by the native thumbnail probe.
