#!/usr/bin/env python3
"""Verify the Materials panel redesign: preview tiles, Model and Library tabs (T84).

Drives the real application in a private software-GL desktop. Checks that model
materials show rendered sphere previews that follow edits, that the Library tab
lists the shipped sections, that a double-click adds a library material, that
Apply to Selection adds and applies one as a single Undo step, and that the menu
commands open the library when nothing is chosen. Screenshots and
observations.json go to docs/rewrite/references/materials-panel/.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from PIL import Image, ImageStat

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop, Failure  # noqa: E402

OUTPUT = ROOT / 'docs/rewrite/references/materials-panel'
LIBRARY_COMMANDS = ['material.add_from_library', 'material.apply_from_library']

# The Materials panel at 1280x900, measured from captures.
MODEL_TAB = (914, 251)
LIBRARY_TAB = (994, 251)
# Centres of the three sphere pictures in the first row of tiles.
COLUMNS = [938, 1066, 1194]
FIRST_ROW = 330
GRID = (1066, 500)
ADD_TO_MODEL = (913, 766)
APPLY_TO_SELECTION = (1018, 766)


class Run:
    def __init__(self, rpc, app, env, output):
        self.rpc, self.app, self.env, self.output = rpc, app, env, output
        self.window = subprocess.check_output(
            ['xdotool', 'search', '--name', '^Meshcraft [0-9]+\\.[0-9]+\\.[0-9]+'], env=env, text=True
        ).splitlines()[-1]
        self.checks = {}
        self.observations = {}

    def expect(self, name, passed, detail=''):
        self.checks[name] = bool(passed)
        print(('PASS ' if passed else 'FAIL ') + name + ('' if passed else f' · {detail}'), flush=True)

    def click(self, point, count=1, settle=.8):
        subprocess.run(['xdotool', 'mousemove', str(point[0]), str(point[1]),
                        'click', '--repeat', str(count), '--delay', '90', '1'],
                       env=self.env, check=True)
        time.sleep(settle)

    def wheel(self, point, clicks):
        subprocess.run(['xdotool', 'mousemove', str(point[0]), str(point[1]),
                        'click', '--repeat', str(clicks), '--delay', '20', '5'], env=self.env, check=True)
        time.sleep(.8)

    def capture(self, path):
        subprocess.run(['xdotool', 'mousemove', '1100', '17'], env=self.env, check=True)
        time.sleep(.5)
        subprocess.run(['import', '-window', self.window, str(path)], env=self.env, check=True)
        return Image.open(path).convert('RGB')

    def shot(self, name):
        return self.capture(self.output / (name + '.png'))

    def picture(self, centre, timeout=40, predicate=lambda stats: True):
        """Wait until the tile picture at `centre` is a shaded sphere, not a flat swatch."""
        deadline = time.monotonic() + timeout
        probe = self.output.parent / '.materials-panel-probe.png'
        stats = None
        while time.monotonic() < deadline:
            image = self.capture(probe)
            x, y = centre
            region = image.crop((x - 26, y - 26, x + 26, y + 26))
            stats = ImageStat.Stat(region)
            shaded = max(stats.stddev) > 6
            if shaded and predicate(stats):
                probe.unlink(missing_ok=True)
                return dict(mean=[round(v) for v in stats.mean], stddev=[round(v, 1) for v in stats.stddev])
            time.sleep(1)
        probe.unlink(missing_ok=True)
        return None if stats is None else dict(
            failed=True, mean=[round(v) for v in stats.mean], stddev=[round(v, 1) for v in stats.stddev])

    def scene(self):
        return self.rpc.call('meshcraft_scene', {})

    def material_ids(self):
        return {m['id'] for m in self.scene()['materials']}


def catalog(run, binary):
    commands = {c['id']: c for c in json.loads(subprocess.check_output(
        [str(binary), '--command-catalog'], text=True))['commands']}
    menus = {i: commands[i]['presentation']['menu_path'] for i in LIBRARY_COMMANDS}
    run.observations['library_menu_paths'] = menus
    run.expect('Materials › Library lists Add and Apply Library Material',
               all(path == ['Materials', 'Library'] for path in menus.values()), menus)


def model_tab(run):
    rpc = run.rpc
    rpc.command('view.panel.materials.toggle')
    time.sleep(1)
    run.click(MODEL_TAB)
    red = run.picture((COLUMNS[1], FIRST_ROW), predicate=lambda s: s.mean[0] > s.mean[2] + 30)
    run.observations['model_red_picture'] = red
    run.expect('Model tab shows each material as a rendered sphere',
               red and not red.get('failed'), red)
    run.shot('01-model-tab')
    material = next(m for m in run.scene()['materials'] if m['id'] == 'ansi-red')
    material['color'] = dict(red=40, green=90, blue=230, alpha=255)
    rpc.command('material.save', dict(material=material))
    blue = run.picture((COLUMNS[1], FIRST_ROW), predicate=lambda s: s.mean[2] > s.mean[0] + 30)
    run.observations['model_recolored_picture'] = blue
    run.expect('editing a material renders its picture again',
               blue and not blue.get('failed'), blue)
    run.shot('02-model-tile-follows-edit')


def library_tab(run):
    rpc = run.rpc
    run.click(LIBRARY_TAB)
    red = run.picture((COLUMNS[1], FIRST_ROW), predicate=lambda s: s.mean[0] > s.mean[2] + 60)
    run.observations['library_red_picture'] = red
    run.expect('Library tab opens on plain colors with rendered spheres',
               red and not red.get('failed'), red)
    run.shot('03-library-plain-colors')
    controls = run.rpc.status()['workspace']['controls']
    keys = sorted(k for k in controls if k.startswith('panel/'))
    tile = controls.get('panel/materials/library/library.color.red-50')
    run.observations['panel_controls'] = dict(count=len(keys), red_50=tile)
    run.expect('panel tabs, library tiles and buttons are measured, and model tiles are not',
               'panel/materials/tab/library' in keys and 'panel/materials/scroll' in keys
               and 'panel/button/material.apply_from_library' in keys and tile is not None
               and abs(tile['x'] + tile['width'] / 2 - COLUMNS[1]) < 12
               and not any(k.startswith('panel/materials/material/') for k in keys), keys)

    run.click((COLUMNS[1], FIRST_ROW), count=2, settle=1.5)
    added = 'library.color.red-50' in run.material_ids()
    run.expect('double-clicking a library tile adds it to the model', added)
    run.shot('04-library-added-mark')

    box = rpc.command('geometry.add_cuboid', dict(dimensions=[2, 2, 2], origin=[-1, 0, -1]))
    selection = rpc.status()['selection']
    run.observations['box'] = box
    run.click((COLUMNS[2], FIRST_ROW))
    rpc.select(selection)
    run.click(APPLY_TO_SELECTION, settle=1.5)
    applied = 'library.color.red-75' in run.material_ids()
    try:
        rpc.command('material.delete', dict(id='library.color.red-75'))
        in_use = False
    except Failure:
        in_use = True
    run.expect('Apply to Selection adds the library material and paints the selection',
               applied and in_use, dict(applied=applied, in_use=in_use))
    rpc.command('edit.undo')
    run.expect('one Undo removes both the paint and the added material',
               'library.color.red-75' not in run.material_ids())

    for name, clicks in [('05-library-vegetation', 22), ('06-library-construction', 40)]:
        run.wheel(GRID, clicks)
        time.sleep(12)
        run.shot(name)
    frame = rpc.frame()
    run.observations['viewport_after_library'] = frame.get('procedural')
    run.expect('the viewport keeps drawing while library pictures render',
               frame.get('procedural') is not None and not frame['procedural']['diagnostics'],
               frame.get('procedural'))


def menu_fallback(run):
    run.click(MODEL_TAB)
    try:
        run.rpc.command('material.add_from_library')
        message = ''
    except Failure as error:
        message = str(error)
    time.sleep(1.5)
    image = run.shot('07-menu-opens-library')
    tab = image.getpixel((LIBRARY_TAB[0] - 30, LIBRARY_TAB[1] - 8))
    run.observations['menu_fallback'] = dict(message=message, library_tab_pixel=tab)
    run.expect('a library command with nothing chosen opens the Library tab',
               'Library' in message and tab[0] > 0x26, run.observations['menu_fallback'])
    # Accordion panels: opening Model Settings collapses Materials.
    run.rpc.command('view.model_settings')
    time.sleep(1.5)
    left = [k for k in run.rpc.status()['workspace']['controls'] if k.startswith('panel/materials/')]
    run.shot('08-panel-collapsed')
    run.expect('collapsing the panel removes its measured controls', not left, left[:5])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=OUTPUT)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = ROOT / 'target/materials-panel-validation'
    with desktop(args.binary.resolve(), work, 60, True) as (rpc, app, env):
        run = Run(rpc, app, env, output)
        rpc.command('ui.update_preferences', dict(show_permanent_grid=False))
        catalog(run, args.binary.resolve())
        model_tab(run)
        library_tab(run)
        menu_fallback(run)
        run.expect('the application is still running', app.poll() is None)
        result = dict(checks=run.checks, observations=run.observations,
                      passed=all(run.checks.values()))
        (output / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(dict(passed=result['passed'])), flush=True)
        if not result['passed']:
            sys.exit(1)


if __name__ == '__main__':
    main()
