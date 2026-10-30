#!/usr/bin/env python3
"""Capture list management actions in the actual isolated native inspectors."""
import argparse
import json
from pathlib import Path
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REF = ROOT / 'docs/rewrite/references/panel-actions'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REF.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/panel-actions/runtime',
                 120, True, size=(1920, 1080)) as (rpc, app, env):
        c = Capture(rpc, env, REF)
        c.cmd('geometry.add_cuboid', dimensions=[3, 2, 2], origin=[0, 0, 0])
        c.cmd('file.save_as', path=str(REF / 'catalogs.octd'))
        c.pose([12, 10, 15], [1.5, 1, 1], 36)
        cube = next(o for o in c.scene()['objects'] if o['path'])
        clicks = []

        def wait_for(check):
            deadline = time.monotonic() + 30
            while not check():
                if time.monotonic() > deadline:
                    raise RuntimeError('Native panel action did not complete')
                time.sleep(.05)

        def click(name, x, y):
            # Coordinates measured from the unaltered 1920x1080 captures.
            clicks.append(dict(action=name, x=x, y=y))
            c.xd('mousemove', x, y, 'click', 1)
            time.sleep(.15)

        c.cmd('view.objects')
        time.sleep(.2)
        click('Select prototype list item', 1670, 315)
        assert not rpc.status()['selection']
        c.shot('prototypes', destination=REF / 'prototypes.png')
        before_delete = rpc.status()['revision']
        click('Delete Prototype', 1560, 437)
        wait_for(lambda: rpc.status()['prototypeDeletion'] is not None)
        assert rpc.status()['prototypeDeletion']['instances'] == 1
        c.shot('delete-confirmation', destination=REF / 'delete-confirmation.png')
        c.xd('key', 'Escape')
        wait_for(lambda: rpc.status()['prototypeDeletion'] is None)
        assert rpc.status()['revision'] == before_delete
        click('New Scripted', 1670, 370)
        assert not rpc.frame(True).get('diagnostics')
        c.xd('key', 'Escape')
        click('Place Instance', 1760, 405)
        wait_for(lambda: 'Place' in rpc.status()['activeTool'])
        rpc.pointer('cancel')
        assert [o['path'] for o in c.scene()['objects'] if o['path']] == [cube['path']]

        rpc.select([cube['target']])
        c.cmd('view.panel.materials.toggle')
        time.sleep(.2)
        c.shot('materials', destination=REF / 'materials.png')
        click('Edit first material without an explicit list selection', 1660, 494)
        c.shot('material-editor', True, destination=REF / 'material-editor.png')
        c.xd('key', 'Escape')
        click('Apply to Selection', 1560, 529)
        wait_for(lambda: rpc.status()['workspace']['selectionMaterials'] == ['ansi-black'])
        c.cmd('edit.undo')
        click('New Material', 1560, 494)
        assert not rpc.frame(True).get('diagnostics')
        c.xd('key', 'Escape')
        c.cmd('view.object_info')
        time.sleep(.2)
        c.shot('instance', destination=REF / 'instance.png')
        (REF / 'native-validation.json').write_text(json.dumps(dict(
            version=c.version, prototypeDeleteConfirmationAndCancel=True,
            newScriptedEditorFromPanel=True, instancePlacementFromPanel=True,
            editMaterialFromPanel=True, applyMaterialFromPanel='ansi-black',
            newMaterialEditorFromPanel=True, clicks=clicks,
            platform='Actual GPUI app; isolated Linux Xvfb/Mesa desktop',
        ), indent=2) + '\n')
        c.receipt()
        print('Native inspector actions and deletion safeguards verified', flush=True)


if __name__ == '__main__':
    main()
