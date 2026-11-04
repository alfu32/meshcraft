#!/usr/bin/env python3
"""Transform, explode and group hotspots, dimensions and text in the actual native app.

Tools run through the real command and pointer path over MCP, with one Undo step per commit.
"""
import argparse
import json
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from capture_store_features import Capture, SOURCE  # noqa: E402
from runtime import desktop  # noqa: E402

REFERENCE = ROOT / 'docs/rewrite/references/hotspot-objects'


def near(a, b, tolerance=1e-6):
    return all(abs(a[k] - v) <= tolerance for k, v in zip('xyz', b))


class Run:
    def __init__(self, rpc, env):
        self.rpc = rpc
        self.c = Capture(rpc, env, REFERENCE)
        self.checks = {}

    def expect(self, name, passed, detail=''):
        self.checks[name] = bool(passed)
        print(('ok   ' if passed else 'FAIL ') + name + (f' ({detail})' if detail else ''), flush=True)

    def geometry(self, path=()):
        return self.rpc.call('meshcraft_geometry', dict(path=list(path), limit=1000))

    def hotspots(self, path=()):
        return self.geometry(path)['hotspots']

    def world(self, path, local):
        # Placements made here only translate.
        translation = self.geometry(path)['transform']['translation']
        return [local[axis] + translation[axis] for axis in 'xyz']

    def handle(self, path, id):
        return dict(path=list(path), kind='semantic_handle', semantic_kind='hotspot', id=id)

    def tool(self, name, *points):
        self.c.cmd(name)
        for point in points:
            self.rpc.pointer('move', point)
            self.rpc.pointer('click', point)
        self.rpc.pointer('cancel')

    def add_hotspot(self, position):
        self.c.cmd('hotspot.add', position=dict(zip('xyz', position)))

    def new(self):
        self.c.new()


def transforms(run):
    rpc = run.rpc
    run.new()
    run.c.pose([8, 7, 12], [2, 1, 0], 40)
    run.tool('tool.builtin.line', [0, 0, 0], [4, 0, 0])
    line = run.geometry()['primitives'][0]['target']
    run.add_hotspot([2, 0, 0])
    hotspot = run.hotspots()[0]['id']
    target = run.handle([], hotspot)

    rpc.select([target])
    run.c.cmd('tool.builtin.move')
    rpc.pointer('click', [0, 0, 0])
    rpc.pointer('move', [0, 1.5, 0])
    run.c.shot('move-preview', destination=REFERENCE / 'move-preview.png')
    rpc.pointer('click', [0, 3, 0])
    rpc.pointer('cancel')
    run.expect('move tool moves the hotspot', near(run.hotspots()[0]['position'], [2, 3, 0]))
    run.expect('move leaves its neighbour alone', run.geometry()['primitives'][0]['target'] == line)
    run.expect('moved hotspot stays selected', target in rpc.status()['selection'])

    run.c.cmd('edit.undo')
    run.expect('undo restores the hotspot in one step', near(run.hotspots()[0]['position'], [2, 0, 0]))
    run.c.cmd('edit.redo')

    # Center, axis point, reference and target: a quarter turn about Y.
    rpc.select([target])
    run.tool('tool.builtin.rotate', [0, 0, 0], [0, 1, 0], [1, 0, 0], [0, 0, 1])
    position = run.hotspots()[0]['position']
    run.expect('rotate tool turns the hotspot', near(position, [0, 3, 2]), str(position))

    # Center, reference and target: double along Z.
    rpc.select([target])
    run.tool('tool.builtin.scale', [0, 0, 0], [0, 0, 1], [0, 0, 2])
    position = run.hotspots()[0]['position']
    run.expect('scale tool maps the hotspot', near(position, [0, 3, 4]), str(position))

    # Origin and normal: reflect across the XZ plane through the origin.
    rpc.select([target])
    run.tool('tool.mirror', [0, 0, 0], [0, 1, 0])
    position = run.hotspots()[0]['position']
    run.expect('mirror tool reflects the hotspot', near(position, [0, -3, 4]), str(position))

    rpc.select([target])
    run.tool('tool.builtin.stretch', [0, 0, 0], [0, 3, 0])
    position = run.hotspots()[0]['position']
    run.expect('stretch tool moves the hotspot', near(position, [0, 0, 4]), str(position))

    rpc.select([target])
    before = len(run.hotspots())
    run.tool('tool.move_copy', [0, 0, 0], [5, 0, 0])
    after = run.hotspots()
    run.expect('move copy adds a separate hotspot', len(after) == before + 1 and after[-1]['id'] != hotspot)
    run.expect('the copy lands at the picked offset',
               near(after[-1]['position'], [position['x'] + 5, position['y'], position['z']]))

    rpc.select([run.handle([], after[-1]['id'])])
    run.c.cmd('edit.copy_selection')
    run.c.cmd('edit.paste_in_place')
    run.expect('copy and paste adds another hotspot', len(run.hotspots()) == before + 2)
    run.c.shot('transformed-hotspots', destination=REFERENCE / 'transformed-hotspots.png')


def explode_and_group(run):
    rpc = run.rpc
    run.new()
    run.c.cmd('geometry.add_cuboid')
    box = rpc.status()['selection'][0]
    run.c.cmd('selection.edit_object')
    run.add_hotspot([1, 1, 1])
    world = run.world(box['path'], run.hotspots(box['path'])[0]['position'])
    run.c.cmd('selection.exit_context')
    rpc.select([box])
    run.c.cmd('object.create_from_selection', name='Group')
    group = rpc.status()['selection'][0]
    run.expect('a group holds the object with a hotspot', len(group['path']) == 1)
    run.c.shot('group-before-explode', destination=REFERENCE / 'group-before-explode.png')

    result = run.c.cmd('object.explode_selection')
    run.expect('explode accepts a group with a hotspot', not rpc.status()['selection'] == [group], str(result)[:200])
    exploded = run.hotspots()
    run.expect('the hotspot survives the explode', len(exploded) == 1)
    run.expect('it keeps its world position', near(exploded[0]['position'], world),
               str(exploded[0]['position']))
    run.expect('the geometry is in the parent', len(run.geometry()['primitives']) > 0)
    run.expect('the hotspot is in the result selection',
               run.handle([], exploded[0]['id']) in rpc.status()['selection'])
    run.c.shot('after-explode', destination=REFERENCE / 'after-explode.png')
    run.c.cmd('edit.undo')
    run.expect('undo restores the group', len(run.hotspots()) == 0)


def add_to_objects(run):
    rpc = run.rpc
    run.new()
    run.c.cmd('geometry.add_cuboid')
    box = rpc.status()['selection'][0]

    # Added while the object is open: they belong to it.
    run.c.cmd('selection.edit_object')
    run.add_hotspot([5, 0, 0])
    run.tool('tool.builtin.text', [0, 5, 0])
    run.tool('tool.builtin.linear_dimension', [0, 0, 0], [1, 0, 0], [0, 1, 0])
    inside = run.geometry(box['path'])
    run.expect('Escape after a tool stays inside the object', len(inside['annotations']) == 2)
    run.expect('a hotspot added inside belongs to the object', len(inside['hotspots']) == 1)
    run.expect('text and a dimension added inside belong to the object', len(inside['annotations']) == 2,
               str(len(inside['annotations'])))
    run.c.shot('added-inside-object', destination=REFERENCE / 'added-inside-object.png')
    rpc.pointer('cancel')
    run.tool('tool.builtin.text', [0, 7, 0])
    run.expect('a further Escape leaves the object', len(run.geometry()['annotations']) == 1
               and len(run.geometry(box['path'])['annotations']) == 2)
    run.c.cmd('edit.undo')

    # Loose content and an object made into one object.
    run.add_hotspot([0, 3, 0])
    loose_hotspot = run.hotspots()[0]['id']
    run.tool('tool.builtin.text', [0, 6, 0])
    text = next(a['target'] for a in run.geometry()['annotations'])
    rpc.select([box, text, run.handle([], loose_hotspot)])
    run.c.cmd('object.create_from_selection', name='Assembly')
    assembly = rpc.status()['selection'][0]
    made = run.geometry(assembly['path'])
    run.expect('make object takes an object, text and a hotspot', len(assembly['path']) == 1)
    run.expect('the hotspot moved into the new object', len(made['hotspots']) == 1 and not run.hotspots())
    run.expect('the text moved into the new object', len(made['annotations']) == 1
               and not run.geometry()['annotations'])
    world = run.world(assembly['path'], made['hotspots'][0]['position'])
    run.expect('the hotspot keeps its world position', near(dict(zip('xyz', world)), [0, 3, 0]), str(world))
    run.c.shot('made-object', destination=REFERENCE / 'made-object.png')

    # Hotspot alone, then Make Object from it and the line it drives.
    run.new()
    run.tool('tool.builtin.line', [4, 0, 0], [6, 0, 0])
    line = run.geometry()['primitives'][0]['target']
    run.add_hotspot([4, 1, 0])
    id = run.hotspots()[0]['id']
    rpc.select([line])
    run.c.cmd('hotspot.attach')
    run.expect('the hotspot drives the line', len(run.hotspots()[0]['attachedSegments']) == 1)
    rpc.select([run.handle([], id), line])
    run.c.cmd('object.create_from_selection', name='Handle')
    handle = rpc.status()['selection'][0]
    owned = run.geometry(handle['path'])
    run.expect('make object takes a hotspot with mesh', len(owned['hotspots']) == 1
               and len(owned['primitives']) == 1 and not run.geometry()['primitives'])

    # The handle still operates on the new object.
    local = owned['hotspots'][0]['position']
    start = run.world(handle['path'], local)
    rpc.select([run.handle(handle['path'], id)])
    run.tool('tool.builtin.move', start, [start[0], start[1] + 1, start[2]])
    driven = run.geometry(handle['path'])
    origin = driven['transform']['translation']['y']
    heights = {round(v['position']['y'] + origin, 6) for v in driven['vertices']}
    run.expect('moving the closed handle deforms the new object', heights == {1.0}, str(heights))
    run.c.shot('operated-handle', destination=REFERENCE / 'operated-handle.png')


def selected_feedback(run):
    """Compare captures with and without the selection; only the feedback may differ."""
    rpc = run.rpc
    run.new()
    run.c.pose([8, 7, 12], [2, 1, 0], 40)
    run.tool('tool.builtin.line', [0, 0, 0], [4, 0, 0])
    run.tool('tool.builtin.line', [0, 0, 3], [4, 0, 3])
    line = run.geometry()['primitives'][0]['target']
    run.add_hotspot([2, 2, 0])
    hotspot = run.hotspots()[0]['id']
    target = run.handle([], hotspot)
    rpc.select([line])
    run.c.cmd('hotspot.attach')
    rpc.select([target])
    run.c.cmd('hotspot.reference')
    rpc.pointer('click', [0, 0, 3])
    def frame(name):
        time.sleep(.5)
        run.c.viewport(name)
        return (REFERENCE / (name + '.png')).read_bytes()

    rpc.select([])
    bare = frame('feedback-off')
    rpc.select([target])
    selected = frame('feedback-on')
    run.expect('selecting draws the feedback', bare != selected)
    run.c.shot('selected-feedback', destination=REFERENCE / 'selected-feedback.png')
    rpc.select([])
    run.expect('clearing the selection removes it', frame('feedback-cleared') == bare)


def reported_scenario(run):
    """Draw a box and a hotspot, window-select them with the mouse, and press Make Object."""
    rpc = run.rpc
    run.new()
    run.c.pose([8, 7, 12], [1, 0, 1], 40)
    run.tool('tool.cuboid', [0, 0, 0], [3, 1, 2])
    run.add_hotspot([5, 1, 0])
    hotspot = run.hotspots()[0]['id']
    run.c.cmd('tool.select')
    rpc.select([])
    before = run.geometry()
    # Measured from the unaltered 1600x900 native capture: empty viewport corners.
    left, top, right, bottom = 250, 290, 1165, 860
    run.c.xd('mousemove', round(left), round(top), 'mousedown', 1)
    time.sleep(.2)
    run.c.xd('mousemove', round((left + right) / 2), round((top + bottom) / 2))
    time.sleep(.2)
    run.c.xd('mousemove', round(right), round(bottom))
    time.sleep(.2)
    run.c.xd('mouseup', 1)
    time.sleep(.5)
    selected = rpc.status()['selection']
    kinds = sorted({t.get('semantic_kind', t['kind']) for t in selected})
    run.expect('the window selects the mesh and the hotspot', 'hotspot' in kinds and 'triangle' in kinds, str(kinds))
    run.c.shot('reported-selection', destination=REFERENCE / 'reported-selection.png')
    run.c.cmd('object.create_from_selection', name='Object')
    made = rpc.status()['selection'][0]
    owned = run.geometry(made['path'])
    run.expect('make object succeeds on that selection', made['kind'] == 'occurrence'
               and any(h['id'] == hotspot for h in owned['hotspots']), str(made)[:120])
    run.expect('the object holds all the mesh', len(owned['primitives']) == len(before['primitives'])
               and not run.geometry()['primitives'],
               f"{len(owned['primitives'])} of {len(before['primitives'])}")
    run.c.shot('reported-object', destination=REFERENCE / 'reported-object.png')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REFERENCE.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/hotspot-objects/runtime',
                 120, True, size=(1600, 900)) as (rpc, app, env):
        run = Run(rpc, env)
        transforms(run)
        explode_and_group(run)
        add_to_objects(run)
        reported_scenario(run)
        selected_feedback(run)
        (REFERENCE / 'native-validation.json').write_text(json.dumps(dict(
            version=run.c.version, checks=run.checks,
            platform='Actual GPUI app; isolated Linux Xvfb/Mesa desktop',
        ), indent=2) + '\n')
        run.c.receipt()
    failed = [name for name, passed in run.checks.items() if not passed]
    print(f'{len(run.checks) - len(failed)} of {len(run.checks)} checks passed', flush=True)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
