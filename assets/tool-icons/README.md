# Command icons

Every registered command uses its **own command ID as its icon ID**. The SVG tile
is designed for that operation; unrelated tools do not borrow a generic eye,
paintbrush, wall or object icon. True aliases can share the same drawing while
retaining their own named tiles. Related actions use a recognizable subject and
an action badge: for example, a material sphere with edit/delete, an object with
placement, or a hotspot with an attachment/reference marker.

The authored vector designs and explicit assignments are in
[scripts/tool_icon_glyphs.py](../../scripts/tool_icon_glyphs.py). There is no
substring classifier for registered commands. Missing designs fail generation
and the Rust command-coverage check. `commands.json` records the generated
subject/action assignments; `icons.mapping.csv` records tile coordinates.
Legacy named tiles retain their coordinates for compatibility.

```sh
# Edit the explicit design/assignment before generating assets for a new command.
python3 scripts/design_tool_icons.py --binary target/debug/meshcraft
cargo build -p meshcraft-desktop --bin meshcraft --locked
python3 scripts/design_tool_icons.py --check
python3 scripts/export_tool_inventory.py --binary target/debug/meshcraft
python3 -m unittest discover -s scripts -p 'test_tool_icons.py' -v
```

Generation uses CairoSVG for the checked-in `commands.32.png`,
`commands.48.png` and `commands.64.png` preview sheets. Cargo uses
its existing resvg build dependency to rasterize the canonical `icons.svg` for
the application, including high-DPI controls; it does not need Python or CairoSVG
for normal builds. `--check` validates deterministic SVG/metadata and PNG sizes
without importing CairoSVG. Icons use vector geometry and transparent tile
backgrounds, with the existing dark-workspace color palette.

The [searchable command gallery](../../docs/tools/index.html) shows the actual
atlas tiles, command IDs, icon IDs and design meanings. User review fields in
`docs/tools/tools.csv` are preserved by command ID. The
[visual review](../../docs/rewrite/references/command-icons/icon-review.png)
compares common repaired icons at large and toolbar sizes.

The historical `icons.png`, `icons.48.png` and `icons.64.png` sheets remain
unchanged as pinned k3d/Octodraw evidence from commit
`acd84f4c67509e10363c33850501eb8f4fc3ffa7`. They are not runtime build inputs or
previews of the current mapping. Their PolyForm Noncommercial License 1.0.0 is
preserved in [LEGACY-LICENSE.md](LEGACY-LICENSE.md); the workspace license does
not replace it. Current command previews use the separate `commands.*.png` files.
