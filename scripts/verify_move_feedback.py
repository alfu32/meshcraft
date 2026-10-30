#!/usr/bin/env python3
"""Capture Home ribbon and Move/Move Copy feedback in an isolated native app."""
import argparse
from pathlib import Path

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REF = ROOT / 'docs/rewrite/references/move-feedback'


def mesh_snapshot(geometry):
    # Tool activity changes the query's transient "editable" flags.
    return (geometry['transform'], geometry['vertices'],
            [p['record'] for p in geometry['primitives']])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REF.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/move-feedback/runtime',
                 120, True, size=(1920, 1080)) as (rpc, app, env):
        c = Capture(rpc, env, REF)
        c.group('Home')
        c.cmd('geometry.add_cuboid', dimensions=[3, 2, 2], origin=[0, 0, 0])
        cube = c.scene()['objects'][0]
        rpc.select([cube['target']])
        c.cmd('file.save_as', path=str(REF / 'move.octd'))
        c.pose([16, 13, 22], [4, 1, 3], 42)
        original = mesh_snapshot(rpc.call('meshcraft_geometry', dict(path=cube['path'], limit=1000)))
        for command, name in [('tool.builtin.move', 'move'), ('tool.move_copy', 'move-copy')]:
            c.cmd(command)
            rpc.pointer('click', [0, 0, 0])
            rpc.pointer('move', [7, 0, 5])
            c.shot(name, destination=REF / (name + '.png'))
            assert original == mesh_snapshot(rpc.call('meshcraft_geometry', dict(path=cube['path'], limit=1000)))
            rpc.pointer('cancel')
        c.receipt()
        print('Native Move/Move Copy previews captured; source geometry unchanged', flush=True)


if __name__ == '__main__':
    main()
