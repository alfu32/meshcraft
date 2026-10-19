#!/usr/bin/env python3
"""Maintain the transparent, bold Meshcraft SVG icon grid and named coordinate map.

The drawing library is native vector geometry; Cargo rasterizes icons.svg at build time.
Run from the repository root after adding commands: python3 scripts/design_tool_icons.py.
"""
from pathlib import Path
import csv
import html
import math
import re

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'assets/tool-icons'
WHITE, BLUE, ORANGE, GREEN, PURPLE = '#e2eaf6', '#85caff', '#ffb66c', '#85dbb5', '#d3b0ff'


def path(d, color=WHITE, fill='none'):
    return f'<path d="{d}" stroke="{color}" fill="{fill}"/>'


def rect(x, y, w, h, color=WHITE, fill='none', radius=1.5):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" stroke="{color}" fill="{fill}"/>'


def circle(x, y, r, color=WHITE, fill='none'):
    return f'<circle cx="{x}" cy="{y}" r="{r}" stroke="{color}" fill="{fill}"/>'


def arrow(d='M6 16H26M21 11L26 16L21 21', color=BLUE):
    return path(d, color)


def badge(symbol, color=ORANGE):
    # Overlay glyphs remain geometric and readable at toolbar size.
    body = {'plus':'M22 25H28M25 22V28', 'minus':'M22 25H28',
            'cross':'M22 22L28 28M28 22L22 28', 'check':'M21 25L24 28L29 22',
            'arrow':'M21 25H29M25 21L29 25L25 29'}[symbol]
    return circle(25,25,6,'#26313e','#26313e') + path(body,color)


BOX = path('M5 10L16 4L27 10L16 16ZM5 10V23L16 29L27 23V10M16 16V29',BLUE)
GEAR = path('M12 5V3H20V5L23 7L26 6L29 12L26 14V18L29 20L26 26L23 25L20 27V29H12V27L9 25L6 26L3 20L6 18V14L3 12L6 6L9 7Z',WHITE)+circle(16,16,5,BLUE)
GRID = ''.join(path(f'M{x} 5V27M5 {x}H27', BLUE) for x in (6,16,26))
FILE = path('M8 3H20L26 9V29H8ZM20 3V10H26',WHITE)
TRIANGLE = path('M5 26L16 5L28 26Z',BLUE, '#253f52')


def glyph(name):
    """Choose deliberately drawn geometry by the operation represented by each name."""
    n = name.lower().replace('_','-').replace('.','-')
    modifier = ''
    if any(w in n for w in ('delete','eraser','remove','clear')):
        return path('M7 9H25M12 9V5H20V9M9 9L10 28H22L23 9M14 13V24M18 13V24',ORANGE)
    if 'undo' in n or 'redo' in n:
        d='M8 10H20C28 10 29 25 20 26H13M8 10L14 4M8 10L14 16'
        return f'<g transform="translate(32 0) scale(-1 1)">{path(d,BLUE)}</g>' if 'redo' in n else path(d,BLUE)
    if 'confirmation' in n:
        return FILE+badge('check',GREEN)
    if 'snapshot' in n:
        return rect(3,8,26,20,BLUE)+circle(16,18,6,WHITE)+path('M8 8L11 3H21L24 8',BLUE)
    if 'save' in n:
        return path('M5 4H23L28 9V28H5ZM10 4V13H22V4M10 28V19H23V28',BLUE)+rect(18,6,2,4,WHITE,WHITE,0)
    if any(w in n for w in ('open','load','browse','recent','import')):
        return path('M4 26V7H12L15 10H28V26ZM4 26L8 15H29L25 26Z',BLUE)+ (badge('plus',GREEN) if 'import' in n else '')
    if any(w in n for w in ('export','capture','screenshot')):
        return FILE+arrow('M15 20H29M24 15L29 20L24 25',GREEN)
    if n in ('file','new','file-new') or n.endswith('-new'):
        return FILE+badge('plus',GREEN)
    if any(w in n for w in ('exit','quit','escape','cancel')):
        return path('M14 5H5V27H14',WHITE)+arrow('M11 16H28M22 10L28 16L22 22',ORANGE)
    if 'select' in n and any(w in n for w in ('window','region','rectangle')):
        return rect(4,5,24,21,BLUE)+path('M7 23L14 7L20 23L14 20Z',WHITE,'#26313e')
    if any(w in n for w in ('coplanar','connected')):
        return rect(4,5,13,13,BLUE)+rect(17,5,11,13,GREEN)+path('M4 18L14 28L28 18M17 5L14 28',ORANGE)
    if 'select' in n or 'pointer' in n:
        return path('M6 3L27 17L18 19L14 29Z',ORANGE,'#684828')
    if any(w in n for w in ('copy','paste','duplicate')) and not any(w in n for w in ('multiple','array','spread')):
        return rect(4,4,17,19,BLUE)+rect(12,12,16,17,WHITE,'#26313e')
    if any(w in n for w in ('helic','rotational','multiple-copy-planar-rotate')):
        return path('M6 24C26 30 30 18 15 18C0 18 4 5 23 8M23 8L19 3M23 8L17 12',ORANGE)+circle(16,18,3,BLUE)
    if 'array' in n or 'multiple' in n or 'spread' in n:
        count = 3 if 'linear' in n or 'translate' in n and 'planar' not in n and 'volum' not in n else 6
        result=''.join(rect(4+(i%3)*9,5+(i//3)*11,5,6,BLUE,'#294a61',0.5) for i in range(count))
        if any(w in n for w in ('spatial','volum')): result+=path('M5 25L12 29L28 24',ORANGE)
        return result+arrow('M5 29H28M24 26L28 29',ORANGE)
    if 'move' in n or 'translate' in n:
        return arrow('M3 16H29M16 3V29M3 16L8 11M3 16L8 21M29 16L24 11M29 16L24 21M16 3L11 8M16 3L21 8M16 29L11 24M16 29L21 24',BLUE)
    if 'rotate' in n or 'revolve' in n or 'orbit' in n or 'rotating' in n:
        return path('M7 24A12 12 0 1 1 26 7M26 7V15H18',BLUE)+circle(16,16,3,ORANGE)+ (badge('plus') if 'copy' in n or n.endswith('-2') else '')
    if 'stretch' in n or 'skew' in n:
        return path('M5 25L8 8H21L27 25ZM8 8L18 18M5 25L18 18L27 25',BLUE)+arrow('M19 5H29M25 2L29 5L25 9',ORANGE)
    if 'scale' in n or 'resize' in n:
        return rect(4,15,13,13,WHITE)+rect(15,4,13,13,BLUE)+arrow('M11 21L25 7M18 7H25V14',ORANGE)
    if 'mirror' in n or 'flip' in n:
        return path('M16 3V29M4 25L12 7V25ZM28 25L20 7V25Z',BLUE)
    if 'extrude' in n or 'push-pull' in n or 'swipe' in n:
        return BOX+arrow('M16 19V2M11 7L16 2L21 7',ORANGE)
    if 'sweep' in n or 'ribbon' in n:
        return path('M4 26C18 30 8 3 27 8M4 20C14 24 7 1 27 3M4 20V26M27 3V8',BLUE)+arrow('M23 3L28 6L24 11',ORANGE)
    if 'offset' in n or 'double-line' in n:
        return path('M4 24L11 8L25 15M8 28L15 14L29 21',BLUE)+arrow('M8 9L5 5M4 11L9 3',ORANGE)
    if 'chamfer' in n or 'fillet' in n:
        return path('M5 27V5H27',WHITE)+path('M5 17Q5 27 16 27H28' if 'fillet' in n else 'M5 16L16 27H28',ORANGE)
    if 'polyline' in n or 'closed-mesh' in n or 'planar-mesh' in n or 'face-outline' in n:
        result=path('M4 25L9 7L20 12L28 5',ORANGE)
        if any(w in n for w in ('mesh','outline')):result+=path('M28 5L27 27L4 25M9 7L27 27L20 12L4 25',BLUE)
        return result+''.join(circle(x,y,1.6,ORANGE,ORANGE) for x,y in ((4,25),(9,7),(20,12),(28,5)))
    if 'rectangle' in n or 'surface-rect' in n or n.endswith('quad'):
        return path('M5 6H27V26H5ZM5 6L27 26',BLUE)+''.join(rect(x-1.5,y-1.5,3,3,ORANGE,ORANGE,0) for x,y in ((5,6),(27,26)))
    if 'arc' in n or 'circle' in n or 'ring' in n:
        return (path('M5 25A14 14 0 0 1 27 9',BLUE) if 'arc' in n else circle(16,16,11,BLUE))+path('M16 16L25 8',ORANGE)+circle(16,16,1.5,ORANGE,ORANGE)
    if 'point' in n or 'hotspot' in n:
        return path('M16 3V29M3 16H29M7 7L25 25M25 7L7 25',BLUE)+circle(16,16,4,ORANGE,ORANGE)
    if n.endswith('-line') or n in ('line','construction-line'):
        return path('M5 25L27 7',BLUE)+circle(5,25,2,ORANGE,ORANGE)+circle(27,7,2,ORANGE,ORANGE)
    if 'grid' in n or 'guide' in n or 'origin' in n:
        if 'axis' in n or 'axial' in n or 'origin' in n:
            return path('M16 17V3',BLUE)+path('M16 17L3 26',ORANGE)+path('M16 17L29 26',GREEN)+circle(16,17,2,WHITE,WHITE)
        if 'normal' in n: return GRID+arrow('M16 21V2M12 6L16 2L20 6',ORANGE)
        return GRID+ (path('M4 24L16 16L28 24M16 4V28',ORANGE) if 'guide' in n or 'helper' in n else '')
    if 'cuboid' in n or n.endswith('box') or 'object' in n or 'bodies' in n or 'explode' in n:
        return BOX+(badge('plus',GREEN) if any(w in n for w in ('create','make','add')) else badge('arrow') if 'edit' in n else badge('cross') if 'explode' in n else '')
    if 'sphere' in n or 'pill' in n:
        return circle(16,16,12,PURPLE)+path('M16 4C4 8 4 24 16 28C28 24 28 8 16 4M4 16H28',PURPLE)
    if 'cylinder' in n or 'tube' in n:
        return path('M5 8C5 2 27 2 27 8C27 14 5 14 5 8V25C5 31 27 31 27 25V8M5 23C5 29 27 29 27 23',PURPLE)
    if 'cone' in n:
        return path('M4 25L16 3L28 25C26 31 6 31 4 25ZM16 3V28',PURPLE)
    if 'wall' in n or 'slab' in n:
        return path('M4 12L24 5L28 8V24L8 29L4 26ZM4 12L8 15L28 8M8 15V29',PURPLE)
    if 'stair' in n:
        return path('M3 28V22H9V16H15V10H21V4H29M3 28H29V4',PURPLE)
    if any(w in n for w in ('frame','door','window','arch-hole')):
        return rect(5,4,22,25,PURPLE)+rect(9,8,14,17,PURPLE)+path('M16 8V25M9 16H23',PURPLE)
    if any(w in n for w in ('pipe','plumb','duct','ventilat','hvac')):
        return path('M4 27V13C4 7 9 4 15 4H28V12H15C12 12 12 14 12 17V27Z',GREEN)+path('M4 23H12M24 4V12',GREEN)
    if 'dimension' in n or 'distance' in n or 'measure' in n:
        return path('M5 5V28M27 5V28M5 10H27M9 6L5 10L9 14M23 6L27 10L23 14',BLUE)+path('M14 19V26M18 19V26',WHITE)
    if 'text' in n or 'glyph' in n or 'alphabet' in n or 'font' in n:
        return path('M5 27L15 4H18L28 27M9 19H24',WHITE)
    if 'leader' in n:
        return arrow('M4 26L15 10H28M4 26L4 19M4 26L11 24',BLUE)
    if 'paint' in n or 'color' in n or 'material' in n:
        return path('M8 23L23 4L28 8L13 27ZM8 23C2 23 6 30 2 30C10 30 13 29 13 27',ORANGE)+path('M8 23L13 27',WHITE)
    if 'mesh' in n or 'remesh' in n or 'regularize' in n or 'triang' in n:
        return path('M4 24L10 4L28 10L26 29ZM10 4L16 18L4 24M16 18L28 10M16 18L26 29',GREEN)
    if 'voxel' in n:
        return GRID+path('M4 6L11 2L29 2V23L26 27M11 2V24',PURPLE)
    if 'boolean' in n or 'solid-' in n or 'intersect' in n or 'subtract' in n or 'union' in n:
        return rect(3,7,17,19,BLUE,'#294a61')+rect(13,3,16,18,ORANGE,'#674b30')
    if 'plane' in n or 'section' in n or 'cut' in n:
        return BOX+path('M1 21L25 8L31 13L7 26Z',ORANGE,'#674b30')
    if any(w in n for w in ('light','shader','render','shadow','illumination','trace')):
        return circle(16,14,7,ORANGE)+path('M12 21V27H20V21M16 2V4M3 14H5M27 14H29M6 4L9 7M23 7L26 4M6 24L9 21M23 21L26 24',ORANGE)
    if 'antialias' in n:
        return path('M3 27L27 3',BLUE)+path('M6 29L29 6',WHITE)
    if 'zoom' in n or 'fit' in n:
        return circle(13,13,9,BLUE)+path('M20 20L29 29M8 13H18M13 8V18',BLUE)
    if 'pan' in n or 'panning' in n:
        return path('M9 26L4 18Q2 14 6 14L10 18V7Q10 3 13 7V15V4Q16 1 17 5V15V6Q21 3 21 7V16V10Q25 7 25 12V22L22 29H12Z',BLUE)
    if 'camera' in n or n in ('front','top','back','perspective','orthographic'):
        return BOX+path('M5 24L27 9',ORANGE)
    if 'play' in n or 'tutorial' in n or 'lesson' in n:
        return path('M8 4L28 16L8 28Z',GREEN,'#254d40')
    if 'pause' in n: return rect(7,4,6,24,ORANGE,ORANGE)+rect(20,4,6,24,ORANGE,ORANGE)
    if 'stop' in n: return rect(6,6,20,20,ORANGE,ORANGE)
    if 'record' in n: return circle(16,16,11,ORANGE,ORANGE)
    if 'help' in n or 'about' in n or 'shortcut' in n:
        return circle(16,16,13,BLUE)+path('M12 10C12 4 24 6 21 13L16 17V20M16 25V25.2',WHITE)
    if 'random' in n or 'fuzzy' in n or 'scatter' in n:
        return ''.join(circle(x,y,2,BLUE,BLUE) for x,y in ((5,6),(16,4),(27,8),(9,16),(24,19),(5,27),(17,25),(28,29)))
    if any(w in n for w in ('settings','preferences','configure','profile','ui-','workspace','format','reset')):
        return GEAR
    if 'check' in n or 'toggle' in n or 'visible' in n:
        return rect(4,5,24,23,BLUE)+path('M8 17L14 23L25 10',GREEN)
    if 'panel' in n or 'layout' in n or 'inspect' in n or 'browser' in n or 'list' in n:
        return rect(3,4,26,24,BLUE)+path('M21 4V28M7 10H17M7 16H17M7 22H17',WHITE)
    if 'cog' in n or 'nut' in n or 'washer' in n or 'screw' in n or 'mech' in n:
        return GEAR
    if 'line-width' in n:
        width = 5 if 'thick' in n else 2
        return f'<path d="M5 9H27M5 23H27" stroke="{BLUE}" stroke-width="{width}"/>'+path('M16 11V21M12 14L16 10L20 14M12 18L16 22L20 18',WHITE)
    if 'unit-' in n or 'toolbar-size' in n:
        return rect(3,8,26,16,BLUE)+path('M8 8V16M13 8V13M18 8V16M23 8V13',WHITE)
    if 'diameter' in n or 'radius' in n:
        return circle(16,16,11,BLUE)+arrow('M7 25L25 7M18 7H25V14',ORANGE)
    if 'plugin' in n:
        return path('M4 5H12C9 0 23 0 20 5H28V13C21 10 21 23 28 20V28H20C23 21 9 21 12 28H4V20C11 23 11 10 4 13Z',GREEN)
    if 'cleanup' in n:
        return path('M16 17L27 4M16 14L22 20L13 29L3 19ZM7 19L14 26M11 16L18 23',ORANGE)
    if 'beam' in n:
        return path('M4 4H28V10H20V22H28V28H4V22H12V10H4Z',GREEN)
    if 'group' in n:
        return rect(3,3,26,26,BLUE)+rect(6,8,8,13,WHITE)+rect(18,11,8,13,ORANGE)+ (badge('minus') if 'ungroup' in n else '')
    if 'reference' in n:
        return circle(16,16,9,BLUE)+path('M16 2V10M16 22V30M2 16H10M22 16H30',ORANGE)
    if 'place-instance' in n:
        return BOX+badge('arrow',GREEN)
    if 'projection' in n or 'ortho-' in n or '-view' in n:
        return rect(15,4,14,24,BLUE)+path('M3 16L15 4M3 16L15 28M3 16H29',ORANGE)
    if 'finish' in n:
        return path('M5 17L13 26L28 6',GREEN)
    if 'insert' in n:
        return circle(16,16,12,BLUE)+path('M16 8V24M8 16H24',GREEN)
    if n=='view' or n.endswith('-hide'):
        return path('M2 16Q16 -4 30 16Q16 36 2 16Z',BLUE)+circle(16,16,5,WHITE)
    if 'definition' in n:
        return BOX+badge('arrow',ORANGE)
    if 'draw' in n or 'annotation' in n:
        return path('M4 28L7 18L23 3L29 9L13 25ZM7 18L13 25M20 6L26 12',ORANGE)
    if 'planar-construction' in n:
        return path('M4 25L8 7L25 10L28 27ZM8 7L28 27',BLUE)+circle(8,7,2,ORANGE,ORANGE)
    if 'operations' in n:
        return path('M5 12A12 12 0 0 1 26 8L28 11H20M27 20A12 12 0 0 1 6 25L4 21H12',BLUE)
    if n in ('edit','modify','tools','tool-cycle-modification'):
        return path('M5 27L23 9M17 4A8 8 0 0 0 28 15L23 19L13 9Z',BLUE)+circle(7,25,2,ORANGE,ORANGE)
    if n=='arrange-toolbars':
        return rect(3,3,26,26,BLUE)+path('M3 12H29M3 20H29M12 3V29',ORANGE)
    raise ValueError(f'Please design a vector glyph for {name}')



def names():
    existing = [r['name'] for r in csv.DictReader((ASSETS/'icons.mapping.csv').open(),delimiter='|') if not r['name'].startswith('undef') and not r['name'].endswith('.')]
    layout=(ROOT/'apps/desktop/src/gpui_layout.rs').read_text()
    keys=set(re.findall(r'\(\s*"([a-z0-9_.-]+)"\s*,\s*"[^"]+"\s*\)',layout))
    menus=layout[layout.index('pub(super) fn menu_items'):layout.index('const HOME_GROUPS')]
    keys.update(s.rstrip('…').lower().replace(' ','-') for s in re.findall(r'"([A-Za-z][A-Za-z /…-]+)"',menus))
    source=(ROOT/'apps/desktop/src/app.rs').read_text()
    keys.update(re.findall(r'"((?:tool|object|view|edit|file|model|ui|render|selection|geometry|hotspot|help|app|document|paint|import)\.[a-z0-9_.]+)"',source))
    builtin=source[source.index('const LEGACY_BUILTIN_TOOLS'):]
    builtin=builtin[:builtin.index('];')]
    keys.update('tool.builtin.'+k for k in re.findall(r'\("([a-z0-9_]+)",',builtin))
    keys.update(('point','cuboid','antialiasing','accordion-settings-panels','normal-line-width','thick-line-width'))
    keys={k for k in keys if not k.endswith('.')}
    return existing+sorted(keys-set(existing))


def main():
    keys=names(); cols=16; rows=math.ceil(len(keys)/cols); width=cols*32; height=rows*32
    parts=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
           '<title>Meshcraft command icons — transparent dark-theme vector atlas</title>',
           '<desc>Bold geometry, 32 pixel tiles, names and coordinates in icons.mapping.csv. Generated by scripts/design_tool_icons.py.</desc>']
    mapping=[['order','name','line','col','start_x','end_x','start_y','end_y','start_x_48','end_x_48','start_y_48','end_y_48','start_x_64','end_x_64','start_y_64','end_y_64']]
    for i,name in enumerate(keys):
        x,y=(i%cols)*32,(i//cols)*32
        parts.append(f'<g id="icon-{html.escape(name,quote=True)}" transform="translate({x} {y})" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><title>{html.escape(name)}</title>{glyph(name)}</g>')
        row=[i+1,name,i//cols+1,i%cols+1]
        for size in (32,48,64):
            a,b=(i%cols)*size,(i//cols)*size;row.extend((a,a+size-1,b,b+size-1))
        mapping.append(row)
    parts.append('</svg>')
    (ASSETS/'icons.svg').write_text('\n'.join(parts)+'\n')
    with (ASSETS/'icons.mapping.csv').open('w',newline='') as f:csv.writer(f,delimiter='|',lineterminator='\n').writerows(mapping)
    print(f'{len(keys)} icons, {width} × {height} SVG, transparent backgrounds')


if __name__=='__main__':main()
