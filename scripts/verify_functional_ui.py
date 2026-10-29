#!/usr/bin/env python3
"""Verify native menus, command search/navigation and titles in a private desktop."""
import argparse
import json
from pathlib import Path
import subprocess
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REFERENCE = ROOT / 'docs/rewrite/references/command-panel'
GROUPS = ['Files', 'Clipboard & History', 'Selection', '2D Construction',
          'Meshing', 'Solids', 'Civil', 'Materials', 'Objects', 'Hotspots', 'Settings']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--output', type=Path, default=REFERENCE)
    args = parser.parse_args()
    reference = args.output.resolve()
    reference.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/command-panel-verification',
                 120, True, size=(1920, 1080)) as (rpc, app, env):
        capture = Capture(rpc, env, reference)
        project = ROOT / 'docs/rewrite/references/segment-feedback/model.octd'
        capture.open(str(project.with_suffix('')))
        before = rpc.status()
        capture.pose([10, 15, 15], [0, 0, 0], 36)
        title = subprocess.check_output(['xdotool', 'getwindowname', capture.window],
                                        env=env, text=True).strip()
        assert title == f'Meshcraft {capture.version} {project}', title
        for name in GROUPS:
            capture.group(name)
        capture.group('Hotspots')
        capture.shot('hotspots', destination=reference / 'hotspots.png')
        # Native left-panel Line button, below the priority file/clipboard/selection sections.
        capture.xd('mousemove', 50, 840, 'click', 1)
        time.sleep(.2)
        assert rpc.status()['activeTool'] == 'Line', rpc.status()
        rpc.pointer('cancel')

        def palette(query=''):
            capture.xd('key', 'ctrl+shift+p')
            if query:
                capture.xd('type', '--clearmodifiers', query)
            time.sleep(.2)

        palette('Archi')
        capture.shot('architecture-search', destination=reference / 'architecture-search.png')
        capture.xd('key', '--repeat', 27, '--delay', 80, 'Down')
        capture.shot('architecture-search-end', destination=reference / 'architecture-search-end.png')
        capture.xd('key', 'Escape')
        palette('view.ortho.')
        capture.xd('key', 'Down')
        capture.shot('keyboard-selection', destination=reference / 'keyboard-selection.png')
        capture.xd('key', 'Return')
        time.sleep(.2)
        assert 'bottom orthographic' in rpc.status()['status'], rpc.status()
        palette('view.ortho.')
        capture.xd('mousemove', 820, 174, 'click', 1)
        time.sleep(.2)
        assert 'back orthographic' in rpc.status()['status'], rpc.status()
        palette()
        capture.shot('all-commands', destination=reference / 'all-commands.png')
        capture.xd('key', '--repeat', 40, '--delay', 80, 'Down')
        capture.shot('all-commands-end', destination=reference / 'all-commands-end.png')
        capture.xd('key', 'Escape')
        palette('material.new_shader')
        capture.xd('key', 'Return')
        time.sleep(.5)
        preview = rpc.frame(True)
        assert not preview.get('diagnostics'), preview
        capture.xd('key', 'Escape')
        after = rpc.status()
        assert before['revision'] == after['revision'], after
        assert before['history'] == after['history'], after
        saved = SOURCE / 'captures/work/command-panel-title.octd'
        capture.cmd('file.save_as', path=str(saved))
        time.sleep(.3)
        saved_title = subprocess.check_output(['xdotool', 'getwindowname', capture.window],
                                              env=env, text=True).strip()
        assert saved_title == f'Meshcraft {capture.version} {saved}', saved_title
        capture.new()
        time.sleep(.3)
        new_title = subprocess.check_output(['xdotool', 'getwindowname', capture.window],
                                            env=env, text=True).strip()
        assert new_title == f'Meshcraft {capture.version} Untitled', new_title
        (reference / 'native-validation.json').write_text(json.dumps(dict(
            version=capture.version, groupsVisited=len(GROUPS),
            lineActivatedThroughNativeButton=True, categorySearch='Archi',
            arrowEnterExecuted='view.ortho.bottom', mouseExecuted='view.ortho.back',
            arrowScrollCaptured=True, shaderEditorPreview=True,
            openedTitle=title, savedTitle=saved_title, newTitle=new_title,
            revisionUnchangedByNavigation=True, historyUnchangedByNavigation=True,
            platform='Linux Xvfb/Mesa; actual GPUI app',
        ), indent=2) + '\n')
        capture.receipt()
        print('Native command panel/menu/title checks passed', flush=True)


if __name__ == '__main__':
    main()
