#!/usr/bin/env python3
"""Shader objects written from scratch, in the real native app (T82).

Drives a private 1280x900 software-GL desktop over MCP and xdotool:

- the renamed commands in the live command catalog;
- Objects > New Shader Object opening the shader editor on a blank object, with
  its starting-point list, its read-only Full Source tab and a GPU preview;
- Create Object saving the program and the object as one Undo step, then placing it;
- a custom object counted from a parameter, its settings dialog, and both bakes;
- Materials > New Shader starting from a blank surface.

Screenshots and observations go to docs/rewrite/references/shader-objects/.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop  # noqa: E402

OUTPUT = ROOT / 'docs/rewrite/references/shader-objects'

# Shader editor controls at 1280x900, measured from captures.
TAB_VERTEX = (253, 177)
TAB_FULL = (536, 177)
STARTER = (458, 74)
STARTER_ITEMS = {'blank-object': 116, 'tree': 178, 'branching': 209}
CREATE = (1188, 859)

# A stack of boxes: instances come from the integer parameter "count".
STACK_VERTEX = (ROOT / 'crates/meshcraft-core/src/document/procedural/blank_object.vs').read_text().replace(
    'position *= u_size;', 'position *= u_size;\n    position.y += float(instanceId) * u_size.y;')


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
        print(('ok   ' if passed else 'FAIL ') + name + (f' ({detail})' if detail else ''), flush=True)

    def xd(self, *args):
        subprocess.run(['xdotool', *map(str, args)], env=self.env, check=True)

    def click(self, point, settle=.6):
        self.xd('mousemove', *point)
        self.xd('click', 1)
        time.sleep(settle)

    def shot(self, name):
        time.sleep(.6)  # Let the GPUI canvas present the fenced GPU frame.
        subprocess.run(['import', '-window', self.window, str(self.output / (name + '.png'))],
                       env=self.env, check=True)

    def scene(self):
        return self.rpc.call('meshcraft_scene', {})

    def history(self):
        status = self.scene()['status']
        return status['revision'], status['history']['entries']

    def objects(self, classification=None):
        return [o for o in self.scene()['objects']
                if classification is None or o['classification'] == classification]

    def place(self, point):
        self.rpc.pointer('move', point)
        self.rpc.frame()
        return self.rpc.pointer('click', point)


def catalog(run, binary):
    commands = {c['id']: c for c in json.loads(subprocess.check_output(
        [str(binary), '--command-catalog'], text=True))['commands']}
    labels = {k: commands[k]['label'] for k in
              ('object.new_tree', 'object.edit_tree', 'object.bake_to_shader', 'material.new_shader')}
    run.observations['labels'] = labels
    run.expect('catalog labels renamed, command IDs kept',
               labels == {'object.new_tree': 'New Shader Object',
                          'object.edit_tree': 'Shader Object Settings',
                          'object.bake_to_shader': 'Bake to GPU Mesh',
                          'material.new_shader': 'New Shader'}, labels)


def new_shader_object(run):
    before = run.history()
    run.rpc.command('object.new_tree')
    frame = run.rpc.frame(True)
    run.shot('01-new-shader-object')
    run.expect('New Shader Object previews the blank object on the GPU',
               frame['preview'] and frame['procedural']['candidateInstances'] == 1
               and frame['procedural']['drawCalls'] == 1 and not frame['procedural']['diagnostics'],
               frame['procedural'])
    run.click(TAB_VERTEX)
    run.shot('02-vertex-tab')
    run.click(TAB_FULL)
    run.shot('03-full-source')
    # Typing into the read-only tab must not change the draft.
    run.click((300, 500))
    run.xd('type', '--delay', 40, 'zzz')
    frame = run.rpc.frame(True)
    run.xd('key', '--clearmodifiers', 'ctrl+a')
    run.xd('key', '--clearmodifiers', 'ctrl+c')
    time.sleep(.5)
    copied = subprocess.run(['xclip', '-o', '-selection', 'clipboard'], env=run.env,
                            capture_output=True, text=True, timeout=10).stdout
    (run.output / 'full-source.glsl').write_text(copied)
    run.expect('Full Source copies the complete compiled code, with the blank object inside',
               copied.count('#version 330') == 2 and 'void customVertex(int vertexId' in copied
               and 'customVertex(gl_VertexID,gl_InstanceID,position,normal,uv)' in copied
               and 'treeVertex' not in copied, f'{len(copied)} characters')
    run.expect('Full Source is read-only and keeps the preview valid',
               'zzz' not in copied and not frame['procedural']['diagnostics']
               and frame['procedural']['candidateInstances'] == 1)
    run.click(STARTER)
    run.shot('04-starting-points')
    run.click((450, STARTER_ITEMS['tree']), settle=1.2)
    frame = run.rpc.frame(True)
    run.shot('05-tree-starter')
    run.expect('choosing the built-in Tree replaces the draft with a tree program',
               frame['procedural']['candidateInstances'] > 100 and not frame['procedural']['diagnostics'],
               frame['procedural'])
    run.click(STARTER)
    run.click((450, STARTER_ITEMS['blank-object']), settle=1.2)
    frame = run.rpc.frame(True)
    run.expect('choosing Blank object again restores the blank draft',
               frame['procedural']['candidateInstances'] == 1, frame['procedural'])
    run.expect('drafts never change the drawing', run.history() == before, run.history())
    run.click(CREATE, settle=1.0)
    after = run.history()
    scene = run.scene()
    program = next((p for p in scene['proceduralPrograms'] if p['id'] == 'custom.object.1'), None)
    prototype = next((p for p in scene['prototypes'] if (p.get('proceduralObject') or {}).get(
        'procedural', {}).get('program') == 'custom.object.1'), None)
    run.expect('Create Object saves the program and the object as one Undo step',
               program and prototype and after == (before[0] + 1, before[1] + 1),
               dict(before=before, after=after))
    run.expect('the saved program is a custom object with no tree engine',
               program and program['generation'] == 'custom' and program['vertexCount'] == 36)
    run.place((0, 0, 0))
    placed = run.objects('procedural_object')
    run.expect('Create Object starts placement: one click places an instance',
               len(placed) == 1, len(placed))
    run.rpc.command('view.frame_document')
    frame = run.rpc.frame()
    run.shot('06-placed-object')
    run.expect('the placed object draws with no diagnostics',
               frame['procedural']['candidateInstances'] >= 1 and not frame['procedural']['diagnostics'],
               frame['procedural'])
    run.observations['created'] = dict(program=program['id'], prototype=prototype['id'],
                                       history=dict(before=before, after=after))
    return placed[0]


def bakes(run, placed):
    run.rpc.select([placed['target']])
    run.rpc.command('object.bake_to_mesh')
    counts = [o['counts']['triangles'] for o in run.scene()['objects']]
    run.expect('Bake to Mesh turns the custom box into 12 editable triangles', 12 in counts, counts)
    run.rpc.command('edit.undo')
    run.expect('Undo restores the shader object', len(run.objects('procedural_object')) == 1)
    run.rpc.select([run.objects('procedural_object')[0]['target']])
    run.rpc.command('object.bake_to_shader')
    gpu = run.objects('gpu_mesh')
    run.expect('Bake to GPU Mesh freezes it as a 12-triangle GPU mesh',
               len(gpu) == 1 and gpu[0]['counts']['triangles'] == 12,
               [o['counts'] for o in gpu])
    run.rpc.command('edit.undo')
    run.observations['bakes'] = dict(mesh=counts, gpu=[o['counts'] for o in gpu])


def counted_stack(run):
    program = dict(
        id='custom.stack', name='Box stack', generation='custom',
        backend='vertex_instancing_gl33', vertexCount=36, instances='count',
        objectBounds=dict(min=['-size.x / 2', 0, '-size.z / 2'],
                          max=['size.x / 2', 'count * size.y', 'size.z / 2']),
        parameters=[
            dict(id='size', kind='vec3', default=[1, 1, 1], label='Size', min=0.001),
            dict(id='count', kind='integer', default=4, label='Boxes', min=0, max=64),
        ],
        vertex=STACK_VERTEX, fragment='')
    run.rpc.command('material.program', dict(program=program))
    run.rpc.select([])
    run.rpc.command('object.configure_procedural', dict(settings=dict(
        name='Box stack',
        procedural=dict(program='custom.stack', parameters=dict(count=5)))))
    run.place((4, 0, 0))
    stack = [o for o in run.objects('procedural_object') if o['name'] == 'Box stack']
    run.expect('a parameter-counted custom object is placed', len(stack) == 1)
    bounds = stack[0]['bounds'] if stack else None
    run.rpc.command('view.frame_document')
    frame = run.rpc.frame()
    run.shot('07-counted-stack')
    # One blank box plus five stacked boxes; the bounds expression grows with the count.
    run.expect('instances follow the count parameter',
               frame['procedural']['candidateInstances'] == 6 and not frame['procedural']['diagnostics'],
               frame['procedural'])
    run.expect('bounds follow the count, so Fit Model frames the whole stack',
               bounds and bounds['min']['y'] == 0 and bounds['max']['y'] == 5
               and bounds['max']['x'] - bounds['min']['x'] == 1, bounds)
    run.rpc.select([stack[0]['target']])
    run.rpc.command('object.edit_tree')
    run.rpc.frame(True)
    run.shot('08-shader-object-settings')
    run.xd('key', 'Escape')
    run.rpc.frame()
    run.observations['stack'] = dict(bounds=bounds, frame=frame['procedural'])


def new_shader(run):
    before = run.history()
    run.rpc.command('material.new_shader')
    frame = run.rpc.frame(True)
    run.shot('09-new-shader-material')
    run.expect('Materials > New Shader starts from a blank surface and previews it',
               frame['preview'] and not frame['procedural']['diagnostics'], frame['procedural'])
    run.xd('key', 'Escape')
    run.rpc.frame()
    run.expect('Escape closes New Shader without changing the drawing', run.history() == before)


def builtin_tree(run):
    """An unchanged built-in creates an object that uses it, with no program copy."""
    programs = len(run.scene()['proceduralPrograms'])
    before = run.history()
    run.rpc.command('object.new_tree')
    run.rpc.frame(True)
    run.click(STARTER)
    run.click((450, STARTER_ITEMS['branching']), settle=1.2)
    run.rpc.frame(True)
    run.click(CREATE, settle=1.0)
    run.place((-4, 0, 0))
    scene = run.scene()
    trees = [p for p in scene['prototypes'] if (p.get('proceduralObject') or {}).get(
        'procedural', {}).get('program') == 'meshcraft.branching.v1']
    run.expect('an unchanged built-in tree uses the built-in program, without a copy',
               len(trees) == 1 and len(scene['proceduralPrograms']) == programs,
               dict(trees=len(trees), programs=[programs, len(scene['proceduralPrograms'])]))
    run.expect('creating and placing it takes two Undo steps',
               run.history()[1] == before[1] + 2, dict(before=before, after=run.history()))
    run.rpc.command('view.frame_document')
    frame = run.rpc.frame()
    run.shot('10-builtin-tree')
    run.expect('the built-in tree draws beside the custom objects',
               frame['procedural']['candidateInstances'] > 100 and not frame['procedural']['diagnostics'],
               frame['procedural'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=OUTPUT)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    work = ROOT / 'target/shader-objects-validation'
    with desktop(args.binary.resolve(), work, 60, True) as (rpc, app, env):
        run = Run(rpc, app, env, output)
        catalog(run, args.binary.resolve())
        placed = new_shader_object(run)
        bakes(run, placed)
        counted_stack(run)
        new_shader(run)
        builtin_tree(run)
        run.expect('the application is still running', app.poll() is None)
        result = dict(checks=run.checks, observations=run.observations,
                      passed=all(run.checks.values()))
        (output / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(dict(passed=result['passed'])), flush=True)
        if not result['passed']:
            sys.exit(1)


if __name__ == '__main__':
    main()
