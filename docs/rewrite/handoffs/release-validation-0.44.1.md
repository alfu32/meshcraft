# Release check repair — 0.43.0 failure / 0.44.1 patch

[Release 0.43.0](https://github.com/alfu32/meshcraft/actions/runs/40705763323)
completed all six native builds successfully, but publication was blocked by the
strict Clippy stage in `validate`. Its Rust tests had passed. The logged error was
`clippy::needless_borrows_for_generic_args` in `objects_bake.rs`: `add_definition`
accepts the owned `String`, so borrowing `format!` was unnecessary.

The complete local all-target gate found four additional warnings which the
first core-library failure had masked: two cloned single-item slices in bake
tests, a fixed-size `chunks_exact` loop in GPU bake capture, and a nested `if let`
in the resource-telemetry GPU test. Remove the borrow, use `slice::from_ref`, use
`as_chunks::<3>().0`, and collapse the conditional. These preserve behavior,
including ignoring the triangle-data remainder. No lint is suppressed and the
workflow's `-D warnings` gate is unchanged.

The repository instructions now require the full format/test/all-target Clippy
release gate before publishing a version; targeted tests and `cargo check` are
explicitly insufficient. Version 0.44.1 carries the repair on top of 0.44.0;
existing version tags are not moved.

## Validation

Local validation of the repaired workspace:

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --locked`: passed, 767 tests / 18 ignored GPU checks.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed,
  including production and test targets. Cargo still reports the existing
  third-party future-compatibility notice for `proc-macro-error2`; it is not a
  workspace Clippy error.
- `cargo build --locked -p meshcraft-desktop --bin meshcraft`: passed, CLI reports
  `Meshcraft 0.44.1`.
- `python3 -m unittest discover -s scripts/release -p 'test_*.py' -v`:
  19 tests, 16 passed / 3 skipped because local `rpm`/`dpkg` tooling is incomplete.
  CI installs those tools; unchanged six-target packaging passed in the inspected
  0.43.0 run.
- `python3 -m unittest discover -s scripts/stress -p 'test_*.py' -v`: 17 passed.
- `python3 -m unittest discover -s scripts -p 'test_export_tool_inventory.py' -v`:
  4 passed.
- `python3 docs/rewrite/scripts/verify_plan.py`: passed (71 tasks).
- `python3 scripts/export_tool_inventory.py --check --binary target/debug/meshcraft`:
  passed, 289 commands; review assignments preserved.
- `git diff --check`: passed.
