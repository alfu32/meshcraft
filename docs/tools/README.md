# Tool and command review

Edit [tools.csv](tools.csv) in a spreadsheet or text editor. There is one row per
command ID in the live application registry, including tools, menu actions,
settings commands and deferred entries. Repeated menu/toolbar placements share
one row. This is the phase 1 review artifact; it does not change today's menus.

The first columns identify the current **category**, **name** and **icon**.
Category uses the first current toolbar group where present, then the command's
declared presentation group/category; it does not assign a future ribbon tab.
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

The four empty review columns are yours:

| Column | Your input |
| --- | --- |
| `ribbon-tab` | Destination ribbon tab |
| `sub-panel` | Group within that tab |
| `comments` | Menu placement, labels, ordering or behavior requests |
| `icon-comments` | Icon changes or other visual notes |

The original request named three review columns while asking for four;
`icon-comments` supplies the fourth. Blank cells have not been preassigned.
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

## Next phases (not delivered in phase 1)

Phase 2 adds operation-specific performance tests for every tool in this inventory
at **small, medium, high and heavy** model sizes. Define fixture entity counts and
hardware/software context; time preview/start/commit separately where relevant,
with output correctness checks. Do not substitute command activation for actual
tool execution or present unsupported cases as successful benchmarks.

Tests must run locally and in a dedicated manual `stress-test.yaml` workflow
dispatched at an existing tag. CI must download that tag's existing release
binary and fail if it is absent; it must not build a different executable.
Publish Markdown, CSV and machine-readable results as release assets, with a
stable future README link to `releases/latest/download/performance.md` once that
asset exists. Include both successful and unsupported/failed cases in coverage.

Phase 3 consumes your edited CSV to reorganize menus, ribbon and tool groups.
Afterward, recapture the README images at their existing descriptive paths using
the [screenshot manifest](../screenshots/manifest.json), then update documentation.
