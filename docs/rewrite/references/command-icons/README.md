# Command icons and Store refresh — 0.46.2

All 290 registered commands now own their icon IDs. Their explicit vector
subjects and action badges replace unrelated icon sharing: Cut/Copy, Flip/Mirror,
booleans/intersection curves, architectural tools and controls, shader/mesh baking,
procedural materials, script editors, hotspots, settings, cameras and automation.
Actual aliases intentionally share their subject. No extra modeling commands or
performance recipes were introduced by this visual change.

[Visual review](icon-review.png) shows repaired icons at large and toolbar sizes;
[SVG source](icon-review.svg) is retained. The
[design manifest](../../../../assets/tool-icons/commands.json) records every
assignment. The [command gallery](../../../tools/index.html) now displays icon IDs
and design meanings and filters by category, name or ID. All four review columns
in the tool CSV are unchanged. See the
[maintenance instructions](../../../../assets/tool-icons/README.md).

The obsolete source-layout parser was replaced with live registry discovery.
New command icons append to the atlas without moving existing named tiles.
The generator and tests reject missing semantic assignments; Rust tests require
an explicit design and a valid command-owned atlas tile for every command.
Regression cases distinguish operations that previously borrowed icons, including
all six camera orientations, architectural forms, hotspots and material types.

The [native check](native/native-validation.json) confirms command search, arrow
scrolling, keyboard/mouse dispatch, editor preview and file titles in the actual
updated GPUI app. [Its screenshots and capture receipts](native/capture-receipts.json)
are from an isolated Linux Xvfb/Mesa desktop.

All eight [Store screenshots](../../../../packaging/microsoft-store/assets/screenshots)
were recaptured by reopening the saved MCP demonstrations in Meshcraft 0.46.2,
showing the new menus and icons. They are unaltered app-window captures. README
images at stable paths were refreshed from those captures. Store feature artwork
was regenerated from its saved MCP geometry/viewport sources. Both English-first
and 35-language import bundles were rebuilt; their validated minimum app version
is 0.46.2. This prepares local imports, without submitting a Partner Center update.

Validation:

- `cargo fmt --all --check`, `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
  Rust tests: 778 passed, 22 platform/GPU checks ignored.
- `python3 scripts/design_tool_icons.py --check`: all 290 explicit designs.
- `python3 -m unittest discover -s scripts -p 'test_tool_icons.py' -v`: 3 passed.
- Inventory helper tests: 4 passed; inventory regeneration/check: 290 entries.
- `python3 scripts/verify_functional_ui.py --output docs/rewrite/references/command-icons/native`: passed.
- `python3 scripts/capture_store_features.py --refresh-readme`: 8 native captures.
- `python3 scripts/prepare_store_art.py`: 6 feature illustrations and 3 logos.
- `python3 scripts/microsoft_store.py --update-export` and `--check`: 35 locales,
  8 screenshots, 9 artwork PNGs. Listing tests: 12 passed.
- `python3 docs/rewrite/scripts/verify_plan.py`: passed.
