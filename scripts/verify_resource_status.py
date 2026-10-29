#!/usr/bin/env python3
"""Live fixed-layout/idle-render check in an isolated Linux desktop (Xvfb + ImageMagick)."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/resource-status-validation')
    args = parser.parse_args()
    output = args.output.resolve()
    with desktop(args.binary.resolve(), output, 60, True) as (rpc, app, env):
        time.sleep(3)
        first = rpc.frame()
        window = subprocess.check_output(
            ['xdotool', 'search', '--name', '^Meshcraft [0-9]+\\.[0-9]+\\.[0-9]+'], env=env, text=True
        ).splitlines()[-1]
        before, after = output / 'status-before.png', output / 'status-after.png'
        subprocess.run(['import', '-window', window, str(before)], env=env, check=True)
        time.sleep(3.2)
        second = rpc.frame()
        subprocess.run(['import', '-window', window, str(after)], env=env, check=True)
        assert first['frameSequence'] == second['frameSequence'], 'Telemetry redrew the viewport'
        assert first['revision'] == second['revision'], 'Telemetry edited the drawing'
        anchors = {}
        # The private desktop is 1280x900 at scale 1. Assert actual label pixels,
        # not just equal background rectangles, at the fixed status cell origins.
        for name, x, width in [('RAM', 413, 22), ('CPU', 659, 22),
                               ('VRAM', 763, 27), ('Frame', 1051, 33)]:
            region = f'{width}x25+{x}+875'
            colors = subprocess.check_output(
                ['convert', str(before), '-crop', region, '-format', '%k', 'info:'], text=True
            )
            assert int(colors) > 1, f'{name} label was not visible'
            compared = subprocess.run(
                ['compare', '-metric', 'AE', '-extract', region, str(before), str(after), 'null:'],
                capture_output=True, text=True,
            )
            assert compared.returncode == 0, f'{name} anchor moved: {compared.stderr}'
            anchors[name] = {'x': x, 'changed_pixels': 0}
        result = dict(frameBefore=first['frameSequence'], frameAfter=second['frameSequence'],
                      revision=second['revision'], idleSeconds=3.2,
                      appAlive=app.poll() is None, anchors=anchors)
        (output / 'idle.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
