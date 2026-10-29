# Command panel and workspace refinements — 0.46.1

The [layout specification](../../../menu-layout.md) describes the eleven ordered
functional groups, toolbar actions, property inspectors and window titles.
[Catalog validation](catalog-validation.json) records 290 registered commands and
188 unique visible actions. Compatibility and parameterized entries remain in the
command panel, without duplicate toolbar placements. All four user review columns
were preserved by command ID when refreshing the tool catalog.

These are unaltered captures of the actual GPUI app on a private Linux Xvfb/Mesa
desktop, using the checked-in segment-feedback model. No Store presentation
assets or README screenshots were regenerated; that work is deferred at the user's
request until the next UI step.

- [Hotspots group and priority sections](hotspots.png): the left panel has plain
  scrolling section headings, while Files, Clipboard & History and Selection lead
  all three command surfaces.
- [Category search](architecture-search.png) finds 28 entries for `Archi`, including
  interactive Architecture tools and typed architecture APIs.
- [Arrow scrolling](architecture-search-end.png) reaches the final filtered result
  after 27 Down key presses, keeping it highlighted and visible.
- [All registered commands](all-commands.png) shows 290 matches without a filter;
  [scrolling down](all-commands-end.png) keeps the selected row visible.
- [Prototype inspector](prototype-inspector.png) retains its list and properties;
  actions are in the Objects toolbar. [Delete Prototype](prototype-delete-confirmation.png)
  invokes the existing warning for three instances; the check cancels it.
  [Receipt](prototype-validation.json) records the isolated check.
- [Keyboard selection](keyboard-selection.png): Down selects Camera > Bottom;
  Enter executes that row. A subsequent mouse click executes Camera > Back.

[Native validation](native-validation.json), [capture receipts](capture-receipts.json)
and [MCP actions](mcp-actions.json) retain the results. The native check also opens
the newly registered shader editor and renders a preview, checks unchanged model
revision/history during navigation, and checks native titles after open, save-as
and new drawing. Reproduce with `python3 scripts/verify_functional_ui.py`.

Validation:

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --locked`: 778 passed, 22 ignored platform/GPU checks.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `python3 scripts/export_tool_inventory.py --binary target/debug/meshcraft --check`:
  290 entries, review columns preserved. Inventory helper tests: 4 passed.
- Stress harness helper tests: 17 passed. Release helper tests: 19 run,
  3 skipped because their native packaging tools are unavailable.
- `python3 docs/rewrite/scripts/verify_plan.py`: passed.

The new `material.new_shader` command has a real editor/preview/cancel performance
recipe. A [filtered local probe](new-shader-performance.md) exercised it on all
four model tiers, one sample each: 1.49 / 2.19 / 2.53 / 2.90 seconds total, including
software rendering and the viewport readiness fence. [CSV](new-shader-performance.csv)
and [JSON](new-shader-performance.json) retain stage timings and provenance. This
is a one-command development probe, not a complete release stress report. Existing
manual same-tag release workflow and publication guards are unchanged.
