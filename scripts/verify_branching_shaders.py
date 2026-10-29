#!/usr/bin/env python3
"""Validate the persisted shader-family example and capture native GPU previews."""
import argparse
import base64
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/branching-validation')
    args = parser.parse_args()
    output = args.output.resolve()
    with desktop(args.binary.resolve(), output, 90, True) as (rpc, app, env):
        rpc.command('file.open', dict(path=str(ROOT / 'examples/branching-families.octd')))
        rpc.select([])
        rpc.command('view.frame_document')
        rpc.command('ui.update_preferences', dict(left_panel_visible=False, ribbon_visible=False,
                                                show_permanent_grid=False, show_construction_guides=False))
        window = subprocess.check_output(['xdotool', 'search', '--name', '^Meshcraft [0-9]+\\.[0-9]+\\.[0-9]+'], env=env, text=True).splitlines()[-1]
        time.sleep(.5)
        subprocess.run(['xdotool', 'mousemove', '430', '400'], env=env, check=True)
        subprocess.run(['xdotool', 'click', '--repeat', '3', '--delay', '80', '4'], env=env, check=True)
        time.sleep(.5)
        first = rpc.frame()
        assert not first['procedural']['diagnostics']
        assert first['procedural']['candidateInstances'] > 0
        scene = rpc.call('meshcraft_scene', {})
        objects = [o for o in scene['objects'] if not o['is_scene_root']]
        assert len(objects) == 6
        assert all(not o['counts']['vertices'] and not o['counts']['triangles'] for o in objects)
        program = next(p for p in scene['proceduralPrograms'] if p['id'] == 'meshcraft.branching.v1')
        assert program['generation'] == 'branching'
        assert program['branchingLimits']['maxBranchCandidates'] == 16384
        assert 'vec3 parentControl' in program['branchingLimits']['ruleType']
        assert 'parentEnd' in program['branchingLimits']['ruleFields']
        assert 'branchingRule' in program['vertex']
        time.sleep(1)
        second = rpc.frame()
        assert first['frameSequence'] == second['frameSequence'], 'Zero wind must idle'
        # Capture the actual viewport image published by the app, without UI cropping or reconstruction.
        request = urllib.request.Request(rpc.url, json.dumps(dict(jsonrpc='2.0', id=1,
            method='tools/call', params=dict(name='meshcraft_capture_view', arguments=dict(max_width=2048)))).encode(),
            {'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream'})
        with urllib.request.urlopen(request, timeout=90) as response:
            result = json.load(response)['result']
        assert not result.get('isError')
        image = next(c for c in result['content'] if c['type'] == 'image')
        (output / 'families.png').write_bytes(base64.b64decode(image['data']))
        rpc.select([objects[0]['target']])
        rpc.command('object.edit_tree')
        preview = rpc.frame(True)
        assert not preview['procedural']['diagnostics']
        time.sleep(.5)
        subprocess.run(['import', '-window', window, str(output / 'settings.png')], env=env, check=True)
        subprocess.run(['xdotool', 'key', 'Escape'], env=env, check=True)
        rpc.frame()
        observation = dict(version=scene['status']['version'], objects=len(objects),
            authoredVertices=sum(o['counts']['vertices'] for o in objects),
            authoredTriangles=sum(o['counts']['triangles'] for o in objects),
            idleFrameSequence=second['frameSequence'], procedural=second['procedural'],
            preview=preview['procedural'], alive=app.poll() is None)
        (output / 'observations.json').write_text(json.dumps(observation, indent=2) + '\n')
        print(json.dumps(observation), flush=True)


if __name__ == '__main__':
    main()
