#!/usr/bin/env python3
"""Exercise object settings in a private 1280x900 software-GL desktop."""
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
    parser.add_argument('--output', type=Path, default=ROOT / 'target/object-preview-validation')
    args = parser.parse_args()
    output = args.output.resolve()
    with desktop(args.binary.resolve(), output, 60, True) as (rpc, app, env):
        window = subprocess.check_output(
            ['xdotool', 'search', '--name', '^Meshcraft$'], env=env, text=True
        ).splitlines()[-1]

        def xd(*args):
            subprocess.run(['xdotool', *map(str, args)], env=env, check=True)

        def shot(name):
            time.sleep(.4)  # Allow the GPUI canvas to present the fenced GPU frame.
            subprocess.run(
                ['import', '-window', window, str(output / (name + '.png'))],
                env=env, check=True,
            )

        def field(y, value):
            xd('mousemove', 350, y)
            xd('click', 1)
            time.sleep(.2)
            xd('key', '--clearmodifiers', 'ctrl+a')
            xd('type', '--delay', 80, '--', str(value))
            time.sleep(.6)  # Draft validation/debounce is intentionally asynchronous.

        before = rpc.call('meshcraft_scene', {})

        def unchanged():
            after = rpc.call('meshcraft_scene', {})
            assert all(after[k] == v for k, v in before.items() if k not in ('status', '_timing'))
            assert after['status']['revision'] == 0
            assert after['status']['history']['entries'] == 0

        observations = []
        # Repeated transitions catch atlas uploads being evicted before presentation.
        for iteration in range(4):
            rpc.command('object.new_tree')
            frame = rpc.frame(True)
            assert frame['preview'] and frame['procedural']['candidateInstances'] > 0
            assert not frame['procedural']['diagnostics']
            unchanged()
            xd('key', 'Escape')
            first = rpc.frame()
            time.sleep(.3)
            second = rpc.frame()
            assert first['frameSequence'] == second['frameSequence'], 'Preview animation leaked'
            assert app.poll() is None
            observations.append(dict(iteration=iteration, preview=frame['procedural'],
                                     idleSequence=second['frameSequence']))

        rpc.command('object.new_tree')
        rpc.frame(True)
        field(754, 0)  # Pause wind, so camera/draft updates can be checked separately.
        rpc.frame(True)
        field(311, 12)
        rpc.frame(True)
        xd('mousemove', 1190, 149)
        xd('click', 1)  # Fit View
        rpc.frame(True)
        shot('object-settings-preview')
        unchanged()
        field(311, -1)
        shot('invalid-draft')  # Inspect visible range error and retained last valid tree.
        unchanged()
        field(311, 12)
        rpc.frame(True)
        for button, end in [(3, (1010, 490)), (2, (930, 460))]:
            xd('mousemove', 900, 440)
            xd('mousedown', button)
            xd('mousemove', '--sync', *end)
            xd('mouseup', button)
            rpc.frame(True)
        xd('click', 4)
        rpc.frame(True)
        shot('navigated-preview')
        xd('mousemove', 400, 620)
        xd('click', '--repeat', 10, '--delay', 80, 5)  # Scroll settings independently.
        time.sleep(.5)
        xd('mousemove', 310, 398)
        xd('click', 1)  # Edit Program / Parameter Schema
        rpc.frame(True)
        shot('shader-editor')
        xd('key', 'Escape')
        rpc.frame(True)
        shot('restored-settings')
        unchanged()
        xd('mousemove', 1200, 822)
        xd('click', 1)  # Save object settings.
        rpc.frame()
        saved = rpc.call('meshcraft_scene', {})
        settings = saved['prototypes'][0]['proceduralObject']['procedural']['parameters']
        assert settings['height'] == 12 and settings['wind'] == 0
        assert saved['status']['revision'] == 1
        assert saved['status']['history']['entries'] == 1
        rpc.command('object.edit_tree')
        rpc.frame(True)
        shot('saved-settings')
        xd('key', 'Escape')
        rpc.frame()
        result = dict(cycles=observations, savedRevision=saved['status']['revision'],
                      savedHistory=saved['status']['history'], alive=app.poll() is None)
        (output / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
