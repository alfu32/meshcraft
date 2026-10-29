# Tool and command review

Edit [tools.csv](tools.csv) in a spreadsheet or text editor. There is one row per
command ID in the live application registry, including tools, menu actions,
settings commands and deferred entries. Each visible action now has one functional home across menus, ribbon and the
left panel. Editing this review file does not change the running app.

The first columns identify the current **category**, **name** and **icon**.
Category uses the functional toolbar group where present, then the command's
registry category. Parameter-only API endpoints and compatibility aliases have
no current menu path; they stay available through automation and the palette.
Icon names reference the [embedded icon atlas](../../assets/tool-icons/README.md);
`(none)` means the command has no assigned icon. Open
[index.html](index.html) locally to see every actual icon beside its command.
GitHub shows HTML source, so download/clone the repository to view that gallery.

Keep `command-id` unchanged. `kind` and `capability` come directly from the registry.
`registered` means the command has a registered implementation path; it can still
require selection, a pending tool, a platform capability or other context.
`deferred` explicitly identifies an unimplemented placeholder. Neither state
constitutes a performance or functional qualification result. The current menu
and aliases are included as references, not proposed assignments.

The four review columns are yours and survive regeneration:

| Column | Your input |
| --- | --- |
| `ribbon-tab` | Destination ribbon tab |
| `sub-panel` | Group within that tab |
| `comments` | Menu placement, labels, ordering or behavior requests |
| `icon-comments` | Icon changes or other visual notes |

The original request named three review columns while asking for four;
`icon-comments` supplies the fourth. Existing notes have not been preassigned or rewritten.
CSV quoting supports commas, quotes and multiline notes.

## Regenerating

```sh
cargo build -p meshcraft-desktop --bin meshcraft --locked
python3 scripts/export_tool_inventory.py
python3 scripts/export_tool_inventory.py --check
python3 -m unittest discover -s scripts -p 'test_export_tool_inventory.py' -v
```

The binary's `--command-catalog` prints JSON without opening a window, a drawing
or an MCP server. Pass `--binary /path/to/meshcraft` to use another build.
The exporter preserves all four review columns by command ID and refuses to
discard notes for removed commands. The gallery is regenerated from the same CSV.

## Performance and functional layout

Phase 2's [performance suite](../performance/README.md) exercises every registry
entry at small, medium, high and heavy weights, with additional bulk selection,
Move and Rotate variants. It runs locally and through the manually tagged
`stress-test.yaml` workflow using the exact existing release binary. Reports are
published as Markdown/CSV/JSON assets on that release. Deferred commands and actual
failures stay visible; no command activation is passed off as completed modeling.

Phase 3 is implemented in 0.46.0 with the [eight functional groups](../menu-layout.md).
The original edited metadata is archived unchanged in
[reviewed-tools-0.45.3.csv](reviewed-tools-0.45.3.csv); the live export preserves all
four review fields by command ID. Further review notes can refine this layout.
README captures were refreshed at their existing descriptive paths, recorded by
the [screenshot manifest](../screenshots/manifest.json).
