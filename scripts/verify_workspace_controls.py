#!/usr/bin/env python3
"""Exercise the actual GPUI shelf, ribbon pickers and instance paint in isolation."""
import argparse
import json
from pathlib import Path
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REFERENCE = ROOT / 'docs/rewrite/references/workspace-controls'
SCRIPT = """function onCreate() { $MATERIAL('finish', 'Finish', 'ansi-grey'); }
function onAnimationFrame(params) {
  return cuboid(new Point(), new Point(2, 2, 2), 'box', params.finish);
}
"""


def wait_for(check, message, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if check():
            return
        time.sleep(.05)
    raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REFERENCE.mkdir(parents=True, exist_ok=True)
    runtime = SOURCE / 'captures/work/workspace-controls/runtime'
    with desktop(args.binary.resolve(), runtime, 120, True,
                 size=(1920, 1080)) as (rpc, app, env):
        c = Capture(rpc, env, REFERENCE)
        try:
            before = rpc.status()
            c.shot('home', destination=REFERENCE / 'home.png')
            def bounds(key):
                wait_for(lambda: key in rpc.status()['workspace']['controls'],
                         'Missing native control bounds: ' + key)
                return rpc.status()['workspace']['controls'][key]

            def click(key):
                box = bounds(key)
                c.xd('mousemove', round(box['x'] + box['width'] / 2),
                     round(box['y'] + box['height'] / 2), 'click', 1)

            # Search through the native input and execute its result with Enter.
            click('left/search')
            c.xd('type', '--clearmodifiers', 'tool.builtin.line')
            time.sleep(.2)
            c.shot('shelf-search', destination=REFERENCE / 'shelf-search.png')
            c.xd('key', 'Return')
            assert rpc.status()['activeTool'] == 'Line', rpc.status()
            rpc.pointer('cancel')
            click('left/search')
            c.xd('key', 'ctrl+a', 'BackSpace')
            c.xd('type', '--clearmodifiers', 'no-such-command')
            c.xd('key', 'Escape')
            wait_for(lambda: rpc.status()['workspace']['leftPanelQuery'] == '',
                     'Escape did not clear shelf search')
            grip = bounds('left/resize')
            old_width = rpc.status()['workspace']['leftPanelWidth']
            start_x = round(grip['x'] + grip['width'] / 2)
            start_y = round(grip['y'] + grip['height'] / 2)
            c.xd('mousemove', start_x, start_y, 'mousedown', 1)
            c.xd('mousemove', round(start_x + 380 - old_width), start_y)
            time.sleep(.15)
            c.xd('mouseup', 1)
            time.sleep(.2)
            prefs = json.loads((runtime / 'config/meshcraft/preferences.json').read_text())
            width = prefs['left_panel_width']
            assert 375 <= width <= 390, width
            rpc.frame()
            frame = rpc.call('meshcraft_capture_view', dict(max_width=1920))
            assert abs(frame['width'] - (1920 - width - 420)) <= 1, frame['width']
            assert rpc.status()['history'] == before['history']
            c.shot('shelf-resized', destination=REFERENCE / 'shelf-resized.png')

            created = rpc.call('meshcraft_object_script', dict(
                action='create', api_version=2, source=SCRIPT))
            definition = created['definition_id']

            def objects():
                return [o for o in c.scene()['objects'] if o['definition_id'] == definition]

            wait_for(lambda: objects() and objects()[0]['counts']['triangles'] == 12,
                     'Initial script geometry did not evaluate')
            c.cmd('object.place_selected_definition', definition_id=definition)
            rpc.pointer('click', [4, 0, 0])
            rpc.pointer('cancel')
            wait_for(lambda: len(objects()) == 2, 'Second instance missing')
            first_two = objects()
            c.pose([10, 7, 14], [3, 1, 0], 36)
            red = c.material('workspace-red', 'Workspace Paint Red', [190, 30, 35, 255])
            blue = c.material('workspace-blue', 'Workspace Paint Blue', [30, 90, 195, 255])
            rpc.select([first_two[1]['target']])
            c.cmd('material.apply', id=blue['id'])
            rpc.select([o['target'] for o in first_two])
            wait_for(lambda: len(rpc.status()['workspace']['selectionMaterials']) == 2,
                     'Mixed material state did not reach the picker')
            c.shot('material-multiple', destination=REFERENCE / 'material-multiple.png')

            def pick(key, query):
                click(key)
                c.xd('type', '--clearmodifiers', query)
                time.sleep(.2)
                c.xd('key', 'Down', 'Return')
                time.sleep(.2)

            def bound(obj, material):
                geometry = rpc.call('meshcraft_geometry', dict(path=obj['path'], limit=100))
                faces = [p['record'] for p in geometry['primitives'] if 'vertices' in p['record']]
                return bool(faces) and all(f['extensions'].get('k3d.meshcraftMaterialId') == material
                                           for f in faces)

            pick('home/material', red['name'])
            wait_for(lambda: all(bound(o, red['id']) for o in first_two),
                     'Native material picker did not assign both instances')
            c.shot('material-assignment', destination=REFERENCE / 'material-assignment.png')
            c.group('Materials')
            assert rpc.status()['workspace']['selectionMaterials'] == [red['id']]
            pick('materials/material', blue['name'])
            wait_for(lambda: all(bound(o, blue['id']) for o in first_two),
                     'Materials ribbon picker did not assign the selection')
            c.shot('materials-ribbon', destination=REFERENCE / 'materials-ribbon.png')
            c.cmd('edit.undo')
            wait_for(lambda: all(bound(o, red['id']) for o in first_two),
                     'Undo of Materials picker failed')
            c.group('Home')
            c.cmd('edit.undo')
            wait_for(lambda: all(not bound(o, red['id']) for o in first_two),
                     'Undo did not revert both finishes')
            c.cmd('edit.redo')
            wait_for(lambda: all(bound(o, red['id']) for o in first_two),
                     'Redo did not restore both finishes')
            rpc.select([])
            # Choose a prototype with the native Home picker: no insertion yet.
            pick('home/object', 'Scripted Object')
            assert len(objects()) == 2
            rpc.pointer('move', [8, 0, 0])
            c.shot('object-insertion', destination=REFERENCE / 'object-insertion.png')
            assert len(objects()) == 2
            rpc.pointer('click', [8, 0, 0])
            rpc.pointer('cancel')
            wait_for(lambda: len(objects()) == 3, 'Picker insertion did not commit')
            third = next(o for o in objects() if o['path'] not in [p['path'] for p in first_two])
            rpc.select([])
            pick('home/material', blue['name'])
            c.pose([9, 1, 12], [9, 1, 0], 36)
            c.cmd('tool.builtin.paint')
            # Camera looks straight at the new box; click the viewport's center.
            started = time.monotonic()
            c.xd('mousemove', round(width + (1920 - width - 420) / 2), 624, 'click', 1)
            wait_for(lambda: bound(third, blue['id']), 'Native Paint did not paint the whole instance')
            paint_seconds = time.monotonic() - started
            assert all(bound(o, red['id']) for o in first_two)
            rpc.pointer('cancel')
            c.pose([12, 8, 16], [5, 1, 0], 36)
            c.shot('instance-paint', destination=REFERENCE / 'instance-paint.png')
            assert not rpc.status()['scriptErrors'], rpc.status()['scriptErrors']
            c.cmd('file.save_as', path=str(REFERENCE / 'workspace.octd'))
            c.cmd('file.open', path=str(REFERENCE / 'workspace.octd'))
            # Reopening remaps runtime IDs; query the serialized instances anew.
            wait_for(lambda: len([o for o in c.scene()['objects'] if o['name'] == 'Scripted Object']) == 3,
                     'Saved instances missing')
            reopened = [o for o in c.scene()['objects'] if o['name'] == 'Scripted Object']
            wait_for(lambda: sum(bound(o, red['id']) for o in reopened) == 2
                     and sum(bound(o, blue['id']) for o in reopened) == 1,
                     'Instance finishes did not survive save/reopen')
            (REFERENCE / 'native-validation.json').write_text(json.dumps(dict(
                version=c.version, nativeSearchEnter=True, nativeSearchEscape=True, persistedLeftWidth=width,
                viewportTracksResize=True, searchAndResizeLeaveHistoryUnchanged=True,
                nativeMaterialPickerAssignsEntireSelection=True, materialsRibbonPicker=True, materialUndoRedo=True,
                nativePrototypePickerStartsFloatingInsertion=True,
                nativePaintTargetsWholeScriptInstance=True, siblingsUnchanged=True,
                saveOpenPreservesFinishes=True, paintSmallFixtureSeconds=paint_seconds,
                platform='Actual GPUI app on isolated Linux Xvfb/Mesa',
            ), indent=2) + '\n')
            c.receipt()
            print('Native workspace controls and instance painting passed', flush=True)
        except Exception:
            c.shot('diagnostic', destination=REFERENCE / 'diagnostic.png')
            raise


if __name__ == '__main__':
    main()
