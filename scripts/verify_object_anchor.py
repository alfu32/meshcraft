#!/usr/bin/env python3
"""Verify Make Object coordinates and subsequent insertion in an isolated native app."""
import argparse
import json
from pathlib import Path
import time
from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REF = ROOT / 'docs/rewrite/references/object-anchor'


def xyz(value):
    return [value[k] for k in ('x', 'y', 'z')]


def points(geometry):
    vertices = {v['id']: xyz(v['position']) for v in geometry['vertices']}
    result = []
    for primitive in geometry['primitives']:
        record = primitive['record']
        for reference in record.get('endpoints', record.get('vertices', [])):
            offset = xyz(reference.get('position_offset', dict(x=0, y=0, z=0)))
            result.append([p + d for p, d in zip(vertices[str(reference['id'])], offset)])
    return result


def world_points(geometry):
    matrix = geometry['transform']
    return [[sum(matrix['linear'][i][j] * p[j] for j in range(3))
             + xyz(matrix['translation'])[i] for i in range(3)] for p in points(geometry)]


def anchor(values):
    return [min(p[0] for p in values), min(p[1] for p in values), max(p[2] for p in values)]


def close(a, b):
    assert all(abs(x - y) < 1e-8 for x, y in zip(a, b)), (a, b)


def same(a, b):
    assert len(a) == len(b), (len(a), len(b))
    for p in a:
        assert any(sum((x - y) ** 2 for x, y in zip(p, q)) < 1e-16 for q in b), p


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REF.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), SOURCE / 'captures/work/object-anchor/runtime',
                 120, True, size=(1920, 1080)) as (rpc, app, env):
        c = Capture(rpc, env, REF)
        c.group('Objects')
        c.cmd('tool.builtin.rectangle')
        rpc.pointer('click', [4, 2, 8])
        rpc.pointer('click', [9, 2, 12])
        rpc.pointer('cancel')
        source = rpc.call('meshcraft_geometry', dict(path=[], limit=1000))
        before = world_points(source)
        assert before
        rpc.select([p['target'] for p in source['primitives']])
        c.cmd('object.create_from_selection', name='Anchored plate')
        plate = next(o for o in c.scene()['objects'] if o['name'] == 'Anchored plate')
        geometry = rpc.call('meshcraft_geometry', dict(path=plate['path'], limit=1000))
        close(xyz(geometry['transform']['translation']), [4, 2, 12])
        close(anchor(points(geometry)), [0, 0, 0])
        same(before, world_points(geometry))
        c.pose([18, 13, 24], [8, 2, 10], 36)
        c.shot('created-object', destination=REF / 'created-object.png')
        c.cmd('edit.undo')
        same(before, world_points(rpc.call('meshcraft_geometry', dict(path=[], limit=1000))))
        c.cmd('edit.redo')
        plate = next(o for o in c.scene()['objects'] if o['name'] == 'Anchored plate')
        rpc.select([plate['target']])
        c.pose([24, 20, 32], [9, 3, 9], 45)
        c.cmd('object.place_selected_definition', definition_id=plate['definition_id'])
        rpc.pointer('move', [14, 4, 7])
        c.shot('insertion-preview', destination=REF / 'insertion-preview.png')
        rpc.pointer('click', [14, 4, 7])
        rpc.pointer('cancel')
        plates = [o for o in c.scene()['objects'] if o['name'] == 'Anchored plate']
        assert len(plates) == 2
        inserted = next(o for o in plates if o['path'] != plate['path'])
        close(anchor(world_points(rpc.call('meshcraft_geometry', dict(path=inserted['path'], limit=1000)))), [14, 4, 7])
        original = [p for o in plates for p in world_points(rpc.call('meshcraft_geometry', dict(path=o['path'], limit=1000)))]
        rpc.select([o['target'] for o in plates])
        c.cmd('object.create_from_selection', name='Combined plates')
        combined = next(o for o in c.scene()['objects'] if o['name'] == 'Combined plates')
        combined_geometry = rpc.call('meshcraft_geometry', dict(path=combined['path'], limit=1000))
        close(xyz(combined_geometry['transform']['translation']), anchor(original))

        def subtree(path):
            return [p for o in c.scene()['objects'] if o['path'][:len(path)] == path
                    for p in world_points(rpc.call('meshcraft_geometry', dict(path=o['path'], limit=1000)))]

        same(original, subtree(combined['path']))
        c.cmd('object.place_selected_definition', definition_id=combined['definition_id'])
        rpc.pointer('click', [-10, 1, 0])
        rpc.pointer('cancel')
        second = next(o for o in c.scene()['objects'] if o['name'] == 'Combined plates' and o['path'] != combined['path'])
        close(anchor(subtree(second['path'])), [-10, 1, 0])
        c.cmd('file.save_as', path=str(REF / 'anchor.octd'))
        c.cmd('file.open', path=str(REF / 'anchor.octd'))
        groups = [o for o in c.scene()['objects'] if o['name'] == 'Combined plates']
        assert len(groups) == 2
        corners = [anchor(subtree(o['path'])) for o in groups]
        assert any(all(abs(x - y) < 1e-8 for x, y in zip(corner, [4, 2, 12])) for corner in corners)
        assert any(all(abs(x - y) < 1e-8 for x, y in zip(corner, [-10, 1, 0])) for corner in corners)
        (REF / 'native-validation.json').write_text(json.dumps(dict(
            version=c.version, cornerConvention='X min, Y min, Z max; Y up, +Z front',
            rawMeshAnchor=[4, 2, 12], localGeometryCorner=[0, 0, 0],
            worldGeometryUnchanged=True, undoRedo=True, newInsertionCorner=[14, 4, 7],
            groupedInstancesUseBoundingBox=True, nestedInsertionCorner=[-10, 1, 0],
            saveOpenRetainsAnchors=True, platform='Actual GPUI app; isolated Linux Xvfb/Mesa',
        ), indent=2) + '\n')
        c.receipt()
        print('Native Make Object anchor and insertion checks passed', flush=True)


if __name__ == '__main__':
    main()
