# Functional menus — 0.46.0

[The functional layout](../../../menu-layout.md) replaces duplicated static menu,
ribbon and left-panel lists with one command placement definition. The runtime
catalog contains 289 commands: 186 visible actions, each present once per surface,
and 103 compatibility, parameterized and legacy entries with no visible menu placement. All
menu aliases are empty. [Catalog validation](catalog-validation.json) compares
the eight published toolbar groups against every visible command ID.

[The Civil workspace](civil.png), [Civil menu](civil-menu.png) and
[Files menu](files-menu.png) are actual native app-window captures. The
[native check](native-validation.json) visited all eight left/ribbon groups and
clicked Line in the real left panel, confirming activation through the same
registry command. Navigation changed neither drawing revision nor history.
[Capture receipts](capture-receipts.json) and [MCP calls](mcp-actions.json) are
retained. Captures use a private Linux Xvfb/Mesa desktop, not the user's drawing.

Reproduce with:

```sh
python3 scripts/verify_functional_ui.py
python3 scripts/capture_store_features.py --refresh-readme
```

Validation:

- `cargo test --workspace --locked`: 775 passed, 22 platform/GPU checks ignored.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- Catalog invariants: eight groups, 186 unique visible IDs, 289 retained IDs,
  no duplicate placements or menu aliases. Explicit regression checks keep
  compatibility IDs callable without generating duplicate controls.
- Native/legacy UI tests exercise menu/submenu activation, file-menu capture,
  Recent Files loading, context menus, command-state gating and unchanged edits.
- `python3 scripts/export_tool_inventory.py --check`: 289 entries, review fields preserved.
- Inventory helper tests: four passed. Store listing validation: 12 passed.

The [original edited catalog](../../../tools/reviewed-tools-0.45.3.csv) preserves
user edits to generated columns as well as review notes. The refreshed live
catalog keeps all four review fields; generated placement reflects the new UI.
