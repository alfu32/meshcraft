#!/usr/bin/env python3
"""Material textures and the texture edit mode in the actual native app.

A four-colour test image goes on a box through the material catalog. Pixels sampled from the
window prove which quadrant lands where. The edit mode is then driven with the same native
pointer handlers the mouse uses: a click on the face, a corner drag and a middle drag. Each
drag must change the material itself, as one undoable edit.
"""
import argparse
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import zlib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from capture_store_features import Capture, SOURCE  # noqa: E402
from runtime import desktop  # noqa: E402

REFERENCE = ROOT / 'docs/rewrite/references/textures'
WORK = SOURCE / 'captures/work/textures'
# The project lives outside the checkout so no home path shows in the captured title bar.
PROJECT = Path(tempfile.gettempdir()) / 'meshcraft-textures'
# Lower-left red, lower-right green, upper-left blue, upper-right yellow, as the image is seen.
QUADRANTS = {'red': (220, 30, 30), 'green': (30, 200, 30), 'blue': (30, 30, 220),
             'yellow': (230, 220, 30)}


def write_quadrants(path, size=64):
    rows = []
    for y in range(size):
        top = y < size // 2
        row = bytearray([0])
        for x in range(size):
            left = x < size // 2
            name = ('blue' if left else 'yellow') if top else ('red' if left else 'green')
            row += bytes(QUADRANTS[name]) + b'\xff'
        rows.append(bytes(row))

    def chunk(kind, data):
        return (struct.pack('>I', len(data)) + kind + data
                + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff))
    path.write_bytes(b'\x89PNG\r\n\x1a\n'
                     + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(b''.join(rows)))
                     + chunk(b'IEND', b''))


def colour(rgb):
    """The test colour a lit pixel shows, by which channels stand out."""
    r, g, b = rgb
    high = max(rgb)
    if high < 40:
        return 'dark'
    strong = [c > high * 0.6 for c in (r, g, b)]
    return {(True, False, False): 'red', (False, True, False): 'green',
            (False, False, True): 'blue', (True, True, False): 'yellow'}.get(tuple(strong), 'other')


class Run:
    def __init__(self, rpc, env):
        self.rpc = rpc
        self.env = env
        self.c = Capture(rpc, env, WORK)
        self.checks = {}

    def expect(self, name, passed, detail=''):
        self.checks[name] = bool(passed)
        print(('ok   ' if passed else 'FAIL ') + name + (f' ({detail})' if detail else ''), flush=True)

    def material(self, id='material-1'):
        return next(m for m in self.c.scene()['materials'] if m['id'] == id)

    def project(self, point):
        status = self.rpc.status()
        camera, view = status['camera'], status['workspace']['viewport']
        position = [camera['position'][k] for k in 'xyz']
        target = [camera['target'][k] for k in 'xyz']
        sub = lambda a, b: [x - y for x, y in zip(a, b)]  # noqa: E731
        dot = lambda a, b: sum(x * y for x, y in zip(a, b))  # noqa: E731
        cross = lambda a, b: [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]  # noqa: E731
        unit = lambda a: [x / math.sqrt(dot(a, a)) for x in a]  # noqa: E731
        forward = unit(sub(target, position))
        right = unit(cross(forward, [0, 1, 0]))
        up = cross(right, forward)
        relative = sub(point, position)
        depth = dot(relative, forward)
        tangent = math.tan(math.radians(camera['fieldOfView']) / 2)
        scale = camera['distance'] if camera['orthographic'] else depth
        x = dot(relative, right) / (scale * tangent * view['width'] / view['height'])
        y = dot(relative, up) / (scale * tangent)
        return (view['x'] + (x + 1) / 2 * view['width'], view['y'] + (1 - y) / 2 * view['height'])

    def native(self, action, point):
        view = self.rpc.status()['workspace']['viewport']
        x, y = self.project(point)
        self.rpc.call('k3d_pointer_event', dict(action=action, viewportX=x - view['x'],
                                                 viewportY=y - view['y']))

    def drag(self, start, end, steps=6):
        self.native('hover', start)
        self.native('down', start)
        for i in range(1, steps + 1):
            t = i / steps
            self.native('hover', [a + (b - a) * t for a, b in zip(start, end)])
        self.native('up', end)

    def shot(self, name, preview=False):
        path = REFERENCE / f'{name}.png'
        self.c.shot(name, preview=preview, destination=path)
        return path

    def sample(self, image, points):
        found = {}
        for label, point in points.items():
            x, y = (round(v) for v in self.project(point))
            text = subprocess.check_output(
                ['convert', str(image), '-format', f'%[pixel:p{{{x},{y}}}]', 'info:'],
                env=self.env, text=True)
            values = [float(v) for v in text[text.index('(') + 1:text.index(')')].split(',')[:3]]
            if max(values) <= 1:
                values = [v * 255 for v in values]
            found[label] = colour(values)
        return found


def textured_box(run):
    rpc = run.rpc
    run.c.new()
    shutil.rmtree(PROJECT, ignore_errors=True)
    image = PROJECT / 'textures/quadrants.png'
    image.parent.mkdir(parents=True, exist_ok=True)
    write_quadrants(image)
    shutil.copy(image, REFERENCE / 'quadrants.png')
    # Save first so the image is stored relative to the project, as Browse stores it.
    run.c.cmd('file.save_as', path=str(PROJECT / 'textured-box.octd'))
    run.c.cmd('geometry.add_cuboid', dimensions=[2, 2, 2], origin=[-1, 0, -1])
    box = rpc.status()['selection']
    record = dict(id='material-1', name='Quadrants',
                  color=dict(red=255, green=255, blue=255, alpha=255), shadingType='phong',
                  texturePath='textures/quadrants.png', textureScaleU=2, textureScaleV=2)
    run.c.cmd('material.save', material=record)
    rpc.select(box)
    run.c.cmd('material.apply', id='material-1')
    rpc.select([])
    run.c.pose([3.4, 3.6, 6.4], [0, 1, 0], 40)
    stored = run.material()
    run.expect('the texture path is stored relative to the project',
               stored['texturePath'] == 'textures/quadrants.png', stored['texturePath'])
    time.sleep(1.0)  # the image decodes off the render thread
    shot = run.shot('textured-box')
    # Textures map in the box's own space, which runs 0..2 with the box placed at -1, so one
    # tile spans the whole front face: x -1..1 and y 0..2 at z = 1.
    seen = run.sample(shot, {'red': [-0.5, 0.5, 1], 'green': [0.5, 0.5, 1],
                             'blue': [-0.5, 1.5, 1], 'yellow': [0.5, 1.5, 1]})
    run.expect('the image covers the face upright, lower-left quadrant at lower left',
               all(k == v for k, v in seen.items()), str(seen))
    run.c.cmd('material.edit', id='material-1')
    time.sleep(1.5)
    run.shot('material-editor', preview=True)
    run.c.xd('key', 'Escape')
    time.sleep(0.3)
    return box


def edit_mode(run, box):
    rpc = run.rpc
    run.c.cmd('material.edit_texture')
    run.expect('the command turns the edit mode on', rpc.status()['textureEdit']['active'])
    run.native('hover', [0.3, 0.7, 1])
    run.native('down', [0.3, 0.7, 1])
    run.native('up', [0.3, 0.7, 1])
    tile = rpc.status()['textureEdit']['tile']
    corners = [[c[k] for k in 'xyz'] for c in (tile or {}).get('corners', [])]
    expected = [[-1, 0, 1], [1, 0, 1], [1, 2, 1], [-1, 2, 1]]
    run.expect('clicking the face shows its tile, corner to corner',
               len(corners) == 4 and all(math.dist(a, b) < 1e-6 for a, b in zip(corners, expected)),
               str(corners))
    run.shot('edit-mode')

    # Upper-right corner to the middle of the face: the lower-left corner stays put.
    run.drag([1, 2, 1], [0, 1, 1])
    material = run.material()
    run.expect('a corner drag resizes the material, keeping the opposite corner',
               abs(material['textureScaleU'] - 1) < 1e-6 and abs(material['textureScaleV'] - 1) < 1e-6
               and abs(material.get('textureOffsetU', 0)) < 1e-6
               and abs(material.get('textureOffsetV', 0)) < 1e-6, str(material))
    time.sleep(0.5)
    shot = run.shot('corner-dragged')
    # Tiles now repeat every unit from x = -1: red, green, red, green along the bottom row.
    seen = run.sample(shot, {'a': [-0.75, 0.25, 1], 'b': [-0.25, 0.25, 1],
                             'c': [0.25, 0.25, 1], 'd': [0.25, 0.75, 1]})
    run.expect('every face with the material shows the smaller tiles',
               seen == {'a': 'red', 'b': 'green', 'c': 'red', 'd': 'blue'}, str(seen))
    tile = rpc.status()['textureEdit']['tile']
    run.expect('the outline stays on the dragged tile',
               tile and math.dist([tile['corners'][2][k] for k in 'xyz'], [0, 1, 1]) < 1e-6, str(tile))

    # The middle handle slides the image a quarter unit right without resizing it.
    run.drag([-0.5, 0.5, 1], [-0.25, 0.5, 1])
    material = run.material()
    run.expect('a middle drag moves the image and keeps its size',
               abs(material['textureOffsetU'] - 0.25) < 1e-6
               and abs(material['textureScaleU'] - 1) < 1e-6, str(material))
    run.c.cmd('edit.undo')
    material = run.material()
    run.expect('undo restores the placement before the drag',
               abs(material.get('textureOffsetU', 0)) < 1e-6
               and abs(material['textureScaleU'] - 1) < 1e-6, str(material))
    run.c.cmd('edit.undo')
    material = run.material()
    run.expect('a second undo restores the original size',
               abs(material['textureScaleU'] - 2) < 1e-6
               and abs(material.get('textureOffsetU', 0)) < 1e-6, str(material))
    rpc.call('k3d_pointer_event', dict(action='cancel'))
    status = rpc.status()['textureEdit']
    run.expect('Esc ends the edit mode and hides the outline',
               not status['active'] and status['tile'] is None, str(status))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REFERENCE.mkdir(parents=True, exist_ok=True)
    WORK.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), WORK / 'runtime', 120, True, size=(1600, 900)) as (rpc, app, env):
        run = Run(rpc, env)
        box = textured_box(run)
        edit_mode(run, box)
        (REFERENCE / 'native-validation.json').write_text(json.dumps(dict(
            version=run.c.version, checks=run.checks,
            platform='Actual GPUI app; isolated Linux Xvfb/Mesa desktop',
        ), indent=2) + '\n')
    failed = [name for name, passed in run.checks.items() if not passed]
    print(f'{len(run.checks) - len(failed)} of {len(run.checks)} checks passed', flush=True)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
