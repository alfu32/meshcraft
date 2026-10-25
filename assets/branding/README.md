# Meshcraft application icon

The blue faceted mesh, visible vertex handles and orange pulled face represent
direct mesh editing. The open shape suggests extensibility. This is an original
AI-generated bitmap, created with the built-in imagegen tool in generate mode
for this project, then mechanically resized for desktop packaging. It is
distributed under the repository's MIT OR Apache-2.0 license.

- `meshcraft.png`: transparent 1254 × 1254 RGBA master.
- `meshcraft-64.png`: embedded native title-bar image, decoded/cached by GPUI.
- `../../packaging/meshcraft.png`: 1024 × 1024 package icon; AppImage, Snap,
  portable archives, macOS ICNS generation and MSIX logo generation use this.
- `../../packaging/windows/meshcraft.ico`: 16, 24, 32, 48, 64, 128 and 256 pixel
  images; Windows executable resource and MSI shortcuts use this.

Regenerate format/size derivatives with `python3 scripts/prepare_app_icon.py`
from the repository root. This requires ImageMagick locally; ordinary builds
use the checked-in images and need no generation service or image converter.
macOS packaging creates ICNS with `sips`/`iconutil` as before. No release matrix
or signing policy changes are needed. Existing tool glyphs remain separate.

## Generation prompt

Use case: logo-design. Asset: final desktop application icon for Meshcraft, a
free, performant, feature-rich and extensible direct mesh modeler that is enjoyable
for both designers and developers. Create one original, coherent geometric symbol:
a bold isometric faceted cube being directly edited, one face pulled out and a few
clear vertex handles, with an open corner suggesting freedom and extensibility and
a subtly forward-moving silhouette suggesting speed. Friendly precision, confident
craftsmanship, minimal broad surfaces with restrained dimensional shading, strong
silhouette legible at 16–32 pixels as well as 1024. Colors should harmonize with a
dark blue CAD interface, using blue/teal facets and a small warm orange edit-handle
accent. Center the single symbol in a square with generous safe margins. Transparent
background, preserve actual alpha, no text, letters, watermarks, borders, badges,
gradients into the background, photorealistic scenery, or excessive tiny wireframe
details. Production-quality app icon, not a sheet of variants.
