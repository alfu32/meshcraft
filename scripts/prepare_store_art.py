#!/usr/bin/env python3
"""Render Microsoft Store artwork from Meshcraft's existing icon and SVG layouts.

Requires CairoSVG and ImageMagick. This never generates or edits screenshots.
The generated PNGs are checked in; importing a listing needs neither tool.
"""

from pathlib import Path
import base64
import subprocess

import cairosvg

ROOT = Path(__file__).resolve().parents[1]
ART = ROOT / "packaging/microsoft-store/assets/art"
ICON = ROOT / "assets/branding/meshcraft.png"


def artwork(width, height, title=False):
    # Design in a common 1920x1080 canvas, adjusted for portrait/square formats.
    portrait = height > width
    size = min(width * 0.77, height * (0.68 if portrait else 0.9))
    if width > height * 1.5:
        size = height * 0.65
    if width > height * 2:
        size = height * 0.87
    x = (width - size) / 2
    y = height * (0.18 if portrait else 0.015)
    # Link the canonical icon instead of duplicating its base64 in every SVG.
    icon = "../../../../assets/branding/meshcraft.png"
    paths = []
    for i in range(-8, 17):
        start = i * width / 12
        paths.append(f"M {start:.2f},{height} L {width/2:.2f},{height*.40:.2f}")
    for t in [0.49, 0.56, 0.65, 0.77, 0.93]:
        paths.append(f"M 0,{height*t:.2f} H {width}")
    wordmark = ""
    if title:
        font = width * (0.068 if portrait else 0.052)
        wordmark = (
            f'<text x="{width/2}" y="{height*(.12 if portrait else .90)}" '
            f'fill="#f4f9ff" font-family="DejaVu Sans" font-size="{font}" '
            'font-weight="600" text-anchor="middle">Meshcraft-desktop</text>'
        )
    return f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
 width="{width}" height="{height}" viewBox="0 0 {width} {height}">
 <defs>
  <linearGradient id="background" x2="0.8" y2="1">
   <stop stop-color="#081725"/><stop offset=".56" stop-color="#123c59"/>
   <stop offset="1" stop-color="#071b2c"/>
  </linearGradient>
  <radialGradient id="glow"><stop stop-color="#27b9e9" stop-opacity=".23"/>
   <stop offset="1" stop-color="#27b9e9" stop-opacity="0"/></radialGradient>
 </defs>
 <rect width="{width}" height="{height}" fill="url(#background)"/>
 <ellipse cx="{width*.50}" cy="{height*.43}" rx="{width*.52}" ry="{height*.59}" fill="url(#glow)"/>
 <path d="{' '.join(paths)}" fill="none" stroke="#59cbee" stroke-opacity=".13" stroke-width="{max(1,width/1200):.2f}"/>
 <path d="M 0,{height*.70} L {width*.22},{height*.48} L {width*.38},{height} Z"
  fill="#3a97b9" fill-opacity=".06"/>
 <path d="M {width},{height*.30} L {width*.78},{height*.56} L {width},{height*.87} Z"
  fill="#fca35a" fill-opacity=".055"/>
 <image x="{x}" y="{y}" width="{size}" height="{size}" xlink:href="{icon}"/>
 {wordmark}
</svg>'''


def main():
    ART.mkdir(parents=True, exist_ok=True)
    for size in [71, 150, 300]:
        subprocess.run(
            ["convert", str(ICON), "-resize", f"{size}x{size}", "-strip",
             str(ART / f"logo-{size}x{size}.png")], check=True,
        )
    for name, width, height, title in [
        ("poster-720x1080", 720, 1080, True),
        ("square-1080x1080", 1080, 1080, True),
        ("hero-1920x1080", 1920, 1080, False),
        ("promo-358x358", 358, 358, False),
        ("promo-1000x800", 1000, 800, True),
        ("promo-414x180", 414, 180, False),
    ]:
        svg = ART / f"{name}.svg"
        layout = artwork(width, height, title)
        svg.write_text(layout, encoding="utf-8")
        # CairoSVG's safe loader intentionally ignores external images. Inline
        # this one known source for rendering, while keeping SVG sources small.
        inline_icon = "data:image/png;base64," + base64.b64encode(ICON.read_bytes()).decode()
        rendered = layout.replace("../../../../assets/branding/meshcraft.png", inline_icon)
        cairosvg.svg2png(bytestring=rendered.encode(), write_to=str(svg.with_suffix('.png')))
        subprocess.run(
            ["convert", str(svg.with_suffix('.png')), "-strip",
             str(svg.with_suffix('.png'))], check=True,
        )
    print(f"Created 9 PNG assets in {ART}")


if __name__ == "__main__":
    main()
