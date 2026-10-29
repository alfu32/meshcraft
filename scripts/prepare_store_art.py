#!/usr/bin/env python3
"""Compose Store feature artwork from models and viewport captures obtained via MCP.

The hero is an original vector illustration of queried model geometry, with no
application UI or text. Other promotional layouts use real MCP viewport images.
Required small logos retain the established application icon. Requires CairoSVG,
Pillow (CairoSVG dependency) and ImageMagick for logo derivatives.
"""

import base64
import json
import math
from pathlib import Path
import subprocess

import cairosvg

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "packaging/microsoft-store"
ART = SOURCE / "assets/art"
ICON = ROOT / "assets/branding/meshcraft.png"


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]


def normalize(v):
    length = math.sqrt(dot(v, v))
    return [x / length for x in v] if length else [0, 1, 0]


def model(name, x, y, width, height, single=False, wire=False):
    """Axonometric promotional illustration of actual world-space MCP geometry."""
    meshes = json.loads((SOURCE / "scenes" / (name + ".json")).read_text())["meshes"]
    if single:
        meshes = meshes[:1]
    faces = [face for mesh in meshes for face in mesh["faces"]]
    edges = [edge for mesh in meshes for edge in mesh["edges"]]
    eye = normalize([.72, .7, 1.0])
    right = normalize(cross([0, 1, 0], eye))
    up = cross(eye, right)
    project = lambda point: (dot(point, right), -dot(point, up))
    points = [project(p) for face in faces for p in face["points"]]
    minx = min(p[0] for p in points)
    maxx = max(p[0] for p in points)
    miny = min(p[1] for p in points)
    maxy = max(p[1] for p in points)
    scale = min(width / (maxx-minx), height / (maxy-miny))
    tx, ty = x+(width-(maxx-minx)*scale)/2, y+(height-(maxy-miny)*scale)/2

    def screen(point):
        a, b = project(point)
        return f"{tx+(a-minx)*scale:.2f},{ty+(b-miny)*scale:.2f}"

    light = normalize([-.3, 1, .6])
    result = [f'<ellipse cx="{x+width*.52}" cy="{y+height*.91}" rx="{width*.40}" ry="{height*.10}" fill="#020c15" opacity=".38"/>']
    for face in sorted(faces, key=lambda f: sum(dot(p, eye) for p in f["points"])/3):
        p = face["points"]
        normal = normalize(cross([p[1][i]-p[0][i] for i in range(3)],
                                 [p[2][i]-p[0][i] for i in range(3)]))
        shade = .52 + .48*abs(dot(normal, light))
        rgb = [max(0, min(255, round(c*shade))) for c in face["color"][:3]]
        tint = '#'+''.join(f'{c:02x}' for c in rgb)
        # Fine facet edges identify mesh topology; this is artwork, not a claim
        # that the screenshot renderer has been modified or has a new UI mode.
        stroke = '#85e4ed' if wire else tint
        opacity = face["color"][3]/255
        result.append(f'<polygon points="{" ".join(screen(v) for v in p)}" fill="{tint}" fill-opacity="{opacity:.2f}" stroke="{stroke}" stroke-opacity="{.20 if wire else 1}" stroke-width="{.55 if wire else .35}" stroke-linejoin="round"/>')
    # Draw explicit authored lines. Avoid hundreds of rear edges bleeding through
    # the solids; triangle facet outlines already communicate the topology.
    if wire:
        for edge in edges[::max(1, len(edges)//90)]:
            result.append(f'<polyline points="{" ".join(screen(v) for v in edge)}" fill="none" stroke="#9ce9f5" stroke-opacity=".17" stroke-width=".7"/>')
    return '\n'.join(result)


def image(name, x, y, width, height, key):
    path = '../feature-renders/'+name+'.png'
    return f'''<defs><clipPath id="{key}"><rect x="{x}" y="{y}" width="{width}" height="{height}" rx="18"/></clipPath></defs>
+<rect x="{x}" y="{y}" width="{width}" height="{height}" rx="18" fill="#e7f3fd" stroke="#315973" stroke-width="2"/>
+<image x="{x}" y="{y}" width="{width}" height="{height}" preserveAspectRatio="xMidYMid slice" clip-path="url(#{key})" href="{path}"/>'''.replace('\n+', '\n')


def wordmark(x, y, size, anchor='start'):
    return f'<text x="{x}" y="{y}" fill="#f0f7fd" font-family="DejaVu Sans" font-size="{size}" font-weight="600" text-anchor="{anchor}">Meshcraft-desktop</text>'


def background(width, height):
    lines = []
    for i in range(-6, 16):
        lines.append(f'M {i*width/10},{height} L {width*.52},{height*.45}')
    return f'''<defs><linearGradient id="bg" x2=".9" y2="1"><stop stop-color="#081827"/><stop offset=".55" stop-color="#123b54"/><stop offset="1" stop-color="#071927"/></linearGradient></defs>
+<rect width="{width}" height="{height}" fill="url(#bg)"/>
+<path d="{' '.join(lines)}" fill="none" stroke="#66c7db" stroke-opacity=".06"/>
+<circle cx="{width*.84}" cy="{height*.17}" r="{width*.26}" fill="#388caa" opacity=".05"/>'''.replace('\n+', '\n')


def layout(name, width, height):
    parts = [background(width, height)]
    if name == 'hero-1920x1080':
        # Text-free and UI-free: modeled mesh forms, not repeated logo artwork.
        parts += [model('mesh-studies', 80, 100, 970, 600, wire=True),
                  model('pavilion-instances', 1050, 45, 690, 335, single=True),
                  model('courtyard-house', 990, 355, 830, 365)]
    elif name == 'poster-720x1080':
        parts += [wordmark(360, 92, 48, 'middle'),
                  model('mesh-studies', 30, 142, 660, 350, wire=True),
                  image('architecture-render', 32, 515, 656, 290, 'architecture'),
                  image('materials-render', 32, 830, 656, 215, 'materials')]
    elif name == 'square-1080x1080':
        parts += [wordmark(54, 97, 60),
                  model('pavilion-instances', 52, 160, 976, 400, single=True),
                  image('architecture-render', 42, 595, 600, 435, 'architecture'),
                  image('branching-grove', 664, 595, 374, 435, 'branching')]
    elif name == 'promo-358x358':
        parts += [model('mesh-studies', 16, 35, 326, 270, wire=True)]
    elif name == 'promo-1000x800':
        parts += [wordmark(40, 85, 54),
                  image('pavilions-render', 35, 125, 555, 320, 'instances'),
                  image('materials-render', 610, 125, 355, 320, 'materials'),
                  model('courtyard-house', 35, 465, 620, 295),
                  image('branching-grove', 680, 465, 285, 295, 'branching')]
    elif name == 'promo-414x180':
        parts += [model('mesh-studies', 4, 15, 185, 150, wire=True),
                  model('pavilion-instances', 202, 15, 198, 150, single=True)]
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">'+'\n'.join(parts)+'</svg>\n'


def main():
    ART.mkdir(parents=True, exist_ok=True)
    for size in [71, 150, 300]:
        subprocess.run(['convert', str(ICON), '-resize', f'{size}x{size}', '-strip',
                        str(ART / f'logo-{size}x{size}.png')], check=True)
    for name, width, height in [
        ('poster-720x1080', 720, 1080), ('square-1080x1080', 1080, 1080),
        ('hero-1920x1080', 1920, 1080), ('promo-358x358', 358, 358),
        ('promo-1000x800', 1000, 800), ('promo-414x180', 414, 180),
    ]:
        svg = ART / (name+'.svg')
        content = layout(name, width, height)
        svg.write_text(content, encoding='utf-8')
        # Render known local raster references as data URIs; CairoSVG's default
        # safe loader intentionally ignores external images.
        for path in (SOURCE/'assets/feature-renders').glob('*.png'):
            content = content.replace('../feature-renders/'+path.name,
                'data:image/png;base64,'+base64.b64encode(path.read_bytes()).decode())
        cairosvg.svg2png(bytestring=content.encode(), write_to=str(svg.with_suffix('.png')))
        subprocess.run(['convert', str(svg.with_suffix('.png')), '-strip',
                        str(svg.with_suffix('.png'))], check=True)
    print('Created 6 feature illustrations and 3 required logo PNGs')


if __name__ == '__main__':
    main()
