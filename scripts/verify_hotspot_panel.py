#!/usr/bin/env python3
"""Capture and exercise the native hotspot inspector in a private desktop."""
import argparse
import json
from pathlib import Path
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REF = ROOT / 'docs/rewrite/references/hotspot-panel'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REF.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/hotspot-panel/runtime',
                 120, True, size=(1920, 1080)) as (rpc, app, env):
        c = Capture(rpc, env, REF)
        c.cmd('tool.builtin.line')
        rpc.pointer('click', [0, 0, 0])
        rpc.pointer('click', [4, 0, 0])
        rpc.pointer('cancel')
        c.cmd('hotspot.add', position=dict(x=2, y=0, z=2))
        c.cmd('view.hotspot_settings')
        c.pose([12, 10, 15], [2, 0, 1], 36)
        time.sleep(.2)
        c.shot('hotspot-panel', destination=REF / 'hotspot-panel.png')
        clicks = []

        def geometry():
            return rpc.call('meshcraft_geometry', dict(path=[], limit=1000))

        def wait_for(check):
            deadline = time.monotonic() + 30
            while not check():
                if time.monotonic() > deadline:
                    raise RuntimeError('Native hotspot action did not complete')
                time.sleep(.05)

        def click(name, x, y):
            # Measured from the unaltered 1920x1080 native capture.
            clicks.append(dict(action=name, x=x, y=y))
            c.xd('mousemove', x, y, 'click', 1)
            time.sleep(.15)

        original = [p['record'] for p in geometry()['primitives']]
        line = geometry()['primitives'][0]['target']
        rpc.select([line])
        click('Attach Selection', 1760, 746)
        wait_for(lambda: len(geometry()['hotspots'][0]['attachedSegments']) == 1)
        rpc.select([])
        click('Select Attached', 1560, 779)
        wait_for(lambda: line in rpc.status()['selection'])
        click('Pick Reference', 1660, 779)
        wait_for(lambda: rpc.status()['activeTool'] == 'Hotspot Reference')
        rpc.pointer('click', [0, 0, 0])
        wait_for(lambda: geometry()['hotspots'][0]['referencePosition'] == dict(x=0, y=0, z=0))
        c.cmd('file.save_as', path=str(REF / 'hotspot.octd'))
        c.shot('configured-hotspot', destination=REF / 'configured-hotspot.png')
        click('Clear Reference', 1760, 779)
        wait_for(lambda: geometry()['hotspots'][0]['referencePosition'] is None)
        click('Delete Hotspot', 1560, 813)
        wait_for(lambda: not geometry()['hotspots'])
        click('Place Hotspot', 1560, 746)
        rpc.pointer('click', [2, 0, 2])
        wait_for(lambda: len(geometry()['hotspots']) == 1)
        rpc.pointer('move', [5, 0, 2])
        click('Add at Cursor', 1660, 746)
        wait_for(lambda: len(geometry()['hotspots']) == 2)
        assert any(h['position'] == dict(x=5, y=0, z=2) for h in geometry()['hotspots'])
        click('Delete Hotspot at Cursor', 1560, 813)
        wait_for(lambda: len(geometry()['hotspots']) == 1)
        assert original == [p['record'] for p in geometry()['primitives']]
        (REF / 'native-validation.json').write_text(json.dumps(dict(
            version=c.version, attachSelection=True, selectAttached=True,
            referencePickAndClear=True, deleteHotspot=True, placeHotspot=True,
            addAtCursor=[5, 0, 2], sourceGeometryUnchanged=True, clicks=clicks,
            platform='Actual GPUI app; isolated Linux Xvfb/Mesa desktop',
        ), indent=2) + '\n')
        c.receipt()
        print('All seven native hotspot panel actions verified', flush=True)


if __name__ == '__main__':
    main()
