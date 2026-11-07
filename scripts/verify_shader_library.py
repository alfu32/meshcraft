#!/usr/bin/env python3
"""Verify the standard shader library, one New Material flow and the Shader Programs panel (T83).

Drives the real application in a private software-GL desktop: renders the six new
built-in surface shaders on boxes and spheres, picks a built-in from the material
editor's Program list, writes a new shader from the material editor and returns to
the material, opens the Shader Programs panel, edits a built-in from it, and deletes
custom programs over MCP. Screenshots and observations.json go to
docs/rewrite/references/shader-library/.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop, Failure  # noqa: E402

OUTPUT = ROOT / 'docs/rewrite/references/shader-library'
LIBRARY = ['emboss.rough_texture', 'emboss.tiling', 'emboss.rough_tiling',
           'metal.polished', 'glass.clear', 'noise.perlin']
RETIRED = ['material.new_grass', 'material.new_foliage', 'material.new_water',
           'material.new_brick', 'material.new_wood']

# Material editor controls at 1280x900, measured from captures.
PROGRAM_SELECT = (283, 224)
PROGRAM_ITEM_METAL = (230, 545)
NEW_SHADER = (517, 224)
MATERIAL_SAVE = (1201, 821)
SAVE_PROGRAM = (1188, 859)


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

    def click(self, point, settle=.8):
        subprocess.run(['xdotool', 'mousemove', str(point[0]), str(point[1]), 'click', '1'],
                       env=self.env, check=True)
        time.sleep(settle)

    def key(self, name):
        subprocess.run(['xdotool', 'key', name], env=self.env, check=True)
        time.sleep(.5)

    def shot(self, name):
        try:
            self.rpc.frame()
        except Failure:
            time.sleep(1)  # a modal or open list can hold the viewport capture
        subprocess.run(['xdotool', 'mousemove', '1100', '17'], env=self.env, check=True)
        time.sleep(.8)
        path = self.output / (name + '.png')
        subprocess.run(['import', '-window', self.window, str(path)], env=self.env, check=True)
        return path

    def scene(self):
        return self.rpc.call('meshcraft_scene', {})

    def material(self, predicate):
        return [m for m in self.scene()['materials'] if predicate(m)]

    def programs(self):
        return {p['id'] for p in self.scene()['proceduralPrograms']}

    def wait(self, condition, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if condition():
                return True
            time.sleep(.1)
        return False


def catalog(run, binary):
    commands = {c['id']: c for c in json.loads(subprocess.check_output(
        [str(binary), '--command-catalog'], text=True))['commands']}
    menus = {i: commands[i]['presentation']['menu_path'] for i in RETIRED}
    run.observations['retired_menu_paths'] = menus
    run.expect('the dedicated New Grass/Foliage/Water/Brick/Wood buttons left the menus',
               all(path == [] for path in menus.values()), menus)
    run.expect('their command IDs remain for MCP clients and scripts',
               all(i in commands for i in RETIRED))
    new = {i: commands[i]['presentation']['menu_path'] for i in
           ['program.edit', 'program.delete', 'view.panel.programs.toggle', 'material.new_shader']}
    run.observations['program_menu_paths'] = new
    run.expect('Materials › Shader Programs lists the panel, New Shader, Edit and Delete',
               all(path == ['Materials', 'Shader Programs'] for path in new.values()), new)


def library(run):
    rpc = run.rpc
    run.expect('the six standard shaders are built-ins', set(LIBRARY) <= run.programs())
    for i, program in enumerate(LIBRARY):
        x = i * 3.0
        rpc.command('geometry.add_cuboid', dict(dimensions=[2, 2, 2], origin=[x - 1, 0, -1]))
        box = rpc.status()['selection']
        rpc.select([])
        rpc.command('tool.builtin.primitive_sphere')
        rpc.pointer('click', (x, 1.2, 4))
        rpc.pointer('click', (x + 1.2, 1.2, 4))
        rpc.pointer('cancel')
        sphere = rpc.status()['selection']
        rpc.command('material.save', dict(material=dict(
            id=f'library-{i}', name=program,
            color=dict(red=205, green=208, blue=212, alpha=255),
            procedural=dict(program=program, parameters={}))))
        rpc.select(box + sphere)
        rpc.command('material.apply', dict(id=f'library-{i}'))
    rpc.select([])
    rpc.command('ui.update_preferences', dict(show_construction_guides=False))
    rpc.command('view.camera.set_pose', dict(position=[7.5, 9, 23], target=[7.5, 0.8, 1.5],
                                             field_of_view=45))
    time.sleep(2)
    frame = rpc.frame()
    run.observations['library_frame'] = frame['procedural']
    run.expect('all six shaders compile and draw with no diagnostics',
               not frame['procedural']['diagnostics'] and frame['procedural']['drawCalls'] >= 6,
               frame['procedural'])
    run.shot('01-library-row')
    rpc.command('view.camera.set_pose', dict(position=[10.5, 3.2, 9], target=[10.5, 1, 2],
                                             field_of_view=40))
    time.sleep(1.5)
    shot = run.shot('02-metal-and-glass')
    # The metal sphere fills the left half of the viewport: mirrored strips and the
    # sun give near-white pixels, the mirrored ground dark ones.
    image = Image.open(shot).convert('L')
    crop = image.crop((240, 420, 480, 760))
    values = list(crop.get_flattened_data())
    bright = sum(v > 235 for v in values) / len(values)
    dark = sum(v < 110 for v in values) / len(values)
    run.observations['metal_sphere'] = dict(bright=round(bright, 3), dark=round(dark, 3))
    run.expect('polished metal shows hard bright reflections and a dark mirrored ground',
               bright > .03 and dark > .15, run.observations['metal_sphere'])


def picker(run):
    rpc = run.rpc
    before = {m['id'] for m in run.scene()['materials']}
    rpc.command('material.new')
    time.sleep(2)
    run.click(PROGRAM_SELECT)
    run.shot('03-program-picker')
    run.click(PROGRAM_ITEM_METAL, settle=1.5)
    run.shot('04-new-metal-material')
    run.click(MATERIAL_SAVE, settle=1.5)
    added = run.material(lambda m: m['id'] not in before)
    run.observations['picked_material'] = added
    m = added[0] if added else {}
    run.expect('New Material › Program › Polished metal saves a metal material',
               len(added) == 1 and m.get('procedural', {}).get('program') == 'metal.polished', added)
    run.expect('choosing the built-in also takes its name, colour and PBR settings',
               m.get('name') == 'Polished metal'
               and (m.get('pbr') or {}).get('metallic') == 1
               and m.get('color', {}).get('red') == 205, m)


def new_shader_from_material(run):
    rpc = run.rpc
    before = {m['id'] for m in run.scene()['materials']}
    rpc.command('material.new')
    time.sleep(2)
    run.click(NEW_SHADER, settle=2.5)
    run.shot('05-new-shader-from-material')
    run.click(SAVE_PROGRAM, settle=2.5)
    run.expect('Save Program stores the new program',
               run.wait(lambda: 'custom.surface.1' in run.programs()), run.programs())
    run.shot('06-back-in-material-editor')
    run.click(MATERIAL_SAVE, settle=1.5)
    added = run.material(lambda m: m['id'] not in before)
    run.observations['material_with_new_shader'] = added
    run.expect('the material editor returns with the new program chosen',
               len(added) == 1
               and (added[0].get('procedural') or {}).get('program') == 'custom.surface.1', added)
    return added[0]['id'] if added else None


def panel(run):
    rpc = run.rpc
    rpc.command('view.camera.set_pose', dict(position=[7.5, 9, 23], target=[7.5, 0.8, 1.5],
                                             field_of_view=45))
    rpc.command('view.panel.programs.toggle')
    time.sleep(1)
    run.shot('07-shader-programs-panel')
    rpc.command('program.edit', dict(id='metal.polished'))
    time.sleep(3)
    frame = rpc.frame(True)
    run.shot('08-edit-built-in-from-panel')
    run.expect('Edit opens a built-in in the shader editor with a working preview',
               frame.get('preview') and not frame['procedural']['diagnostics'], frame.get('procedural'))
    run.key('Escape')
    run.expect('cancelling leaves the built-in unchanged',
               next(p for p in run.scene()['proceduralPrograms'] if p['id'] == 'metal.polished')
               ['name'] == 'Polished metal')


def delete(run, material_id):
    rpc = run.rpc
    rpc.command('program.delete', dict(id='custom.surface.1'))
    reset = run.material(lambda m: m['id'] == material_id)
    run.expect('deleting a program removes it and makes its material conventional',
               'custom.surface.1' not in run.programs() and reset and reset[0].get('procedural') is None,
               reset)
    run.observations['delete_status'] = rpc.status()['status']
    run.expect('the status names the materials that changed',
               'conventional shading' in rpc.status()['status'], rpc.status()['status'])
    rpc.command('edit.undo')
    restored = run.material(lambda m: m['id'] == material_id)
    run.expect('a single Undo restores the program and the material that used it',
               'custom.surface.1' in run.programs()
               and (restored[0].get('procedural') or {}).get('program') == 'custom.surface.1', restored)
    try:
        rpc.command('program.delete', dict(id='metal.polished'))
        refused = False
    except Failure as error:
        refused = 'Built-in' in str(error)
    run.expect('built-in programs cannot be deleted', refused and 'metal.polished' in run.programs())
    # A program that a shader object generates from is kept, with a reason.
    program = dict(
        id='custom.cube', name='Cube', generation='custom', vertexCount=36, instances=1,
        objectBounds=dict(min=[-.5, 0, -.5], max=[.5, 1, .5]),
        vertex=(ROOT / 'crates/meshcraft-core/src/document/procedural/blank_object.vs').read_text()
        .replace('position *= u_size;', ''),
        fragment='')
    rpc.command('material.program', dict(program=program))
    rpc.select([])
    rpc.command('object.configure_procedural', dict(settings=dict(
        name='Cube', procedural=dict(program='custom.cube', parameters={}))))
    rpc.pointer('click', (0, 0, 8))
    rpc.pointer('cancel')
    try:
        rpc.command('program.delete', dict(id='custom.cube'))
        message = ''
    except Failure as error:
        message = str(error)
    run.observations['object_program_refusal'] = message
    run.expect('a program used by a shader object is kept, with the reason',
               'shader object still uses' in message and 'custom.cube' in run.programs(), message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=OUTPUT)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = ROOT / 'target/shader-library-validation'
    with desktop(args.binary.resolve(), work, 60, True) as (rpc, app, env):
        run = Run(rpc, app, env, output)
        rpc.command('ui.update_preferences', dict(show_permanent_grid=False))
        catalog(run, args.binary.resolve())
        library(run)
        picker(run)
        material_id = new_shader_from_material(run)
        panel(run)
        delete(run, material_id)
        run.expect('the application is still running', app.poll() is None)
        result = dict(checks=run.checks, observations=run.observations,
                      passed=all(run.checks.values()))
        (output / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(dict(passed=result['passed'])), flush=True)
        if not result['passed']:
            sys.exit(1)


if __name__ == '__main__':
    main()
