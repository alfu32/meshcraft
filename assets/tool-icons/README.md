# Tool icon assets

`icons.svg` is Meshcraft's transparent, dark-theme vector slate. It preserves the k3d semantic icon names and extends their mapping with command and shell-control IDs. Bold strokes and blue/orange accents replace the original white tiles and fine outlines. Buttons keep their text, tooltips and stable IDs.

Run `python3 scripts/design_tool_icons.py` to regenerate the SVG and `icons.mapping.csv`. The generator requires a glyph design for every name and rejects unknown names. The current sheet contains 419 named 32-pixel tiles in 16 columns. The coordinate map is authoritative; sheet dimensions grow with coverage.

The desktop Cargo build rasterizes the SVG into 32/48/64-pixel tile atlases under `OUT_DIR` using resvg. Atlas dimensions are generated from the SVG. GPUI crops and caches individual tiles rather than parsing SVG during rendering.

The checked-in PNG sheets are historical k3d evidence, not runtime build inputs. They were copied from the pinned Octodraw source at commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7` and retain its PolyForm Noncommercial License 1.0.0 in `LEGACY-LICENSE.md`. Preserve that provenance and license; the workspace license does not replace it.
