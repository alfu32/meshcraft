# Legacy Tool Icons

These unchanged icon atlases and their coordinate map were copied from the pinned Octodraw source at commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7` for the Meshcraft rewrite. The 32, 48, and 64 pixel sheets share the coordinates in `icons.mapping.csv`.

These files retain the source repository's PolyForm Noncommercial License 1.0.0 in `LEGACY-LICENSE.md`; the workspace license does not replace it. Marketing images, old branding, and plugin binaries are not included in this runtime icon set.

`icons.svg` is the original k3d vector grid; `icons.mapping.csv` remains authoritative for tile names/coordinates. The desktop Cargo build rasterizes this grid into 32/48/64-pixel tile atlases under `OUT_DIR` using resvg. GPUI crops/caches the 64-pixel tiles once and displays them on command buttons, including disabled commands; labels, tooltips and stable command IDs remain available. The checked-in PNGs are historical evidence, not the desktop build source.
