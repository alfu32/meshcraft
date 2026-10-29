#!/usr/bin/env python3
"""Inspect real MCP stair geometry for continuous, collinear tread fronts."""
import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import sys
import time

from capture_store_features import Capture, ROOT
sys.path.insert(0, str(ROOT/'scripts/stress'))
from runtime import desktop

REFERENCE = ROOT/'docs/rewrite/references/stair-fronts'


def inspect(rpc):
    scene = rpc.call('meshcraft_scene')
    objects = scene['objects']
    reports = []
    for obj in objects:
        offset = 0
        vertices, primitives, architecture = {}, [], []
        while True:
            data = rpc.call('meshcraft_geometry', dict(path=obj['path'], offset=offset, limit=1000))
            vertices.update({v['id']:v['position'] for v in data['vertices']})
            primitives.extend(data['primitives'])
            architecture = data['architecture']
            if data['nextOffset'] is None:
                break
            offset = data['nextOffset']
        for owner in architecture:
            if owner['target'].get('semantic_kind') != 'stair':
                continue
            record = owner['record']
            count, height = record['stepCount'], record['height']
            boundaries = {}
            triangle_count = 0
            for item in primitives:
                face = item['record']
                if item['target'] != owner['target'] or len(face.get('vertices', [])) != 3:
                    continue
                triangle_count += 1
                p = []
                for v in face['vertices']:
                    point, delta = vertices[v['id']], v.get('position_offset', {})
                    p.append(tuple(point[k]+delta.get(k,0) for k in ['x','y','z']))
                base = record['contour'][0]['y']
                h = p[0][1]-base
                level = round(h/height*count)
                if level < 1 or level > count or abs(h-level*height/count) > 1e-5 \
                        or any(abs(v[1]-p[0][1])>1e-5 for v in p):
                    continue
                normal_y = (p[1][2]-p[0][2])*(p[2][0]-p[0][0])-(p[1][0]-p[0][0])*(p[2][2]-p[0][2])
                if normal_y <= 1e-9:
                    continue
                for i in range(3):
                    a,b=(p[i][0],p[i][2]),(p[(i+1)%3][0],p[(i+1)%3][2])
                    key=tuple(sorted(tuple(round(v*1e6) for v in q) for q in [a,b]))
                    entry=boundaries.setdefault(key,dict(a=a,b=b,levels=defaultdict(int)))
                    entry['levels'][level-1]+=1
            fronts=defaultdict(list)
            for edge in boundaries.values():
                levels=sorted(i for i,n in edge['levels'].items() if n%2)
                if len(levels)>1:
                    fronts[levels[-1]].append((edge['a'],edge['b']))
            failures=[]
            for step in range(1,count):
                edges=fronts[step]
                if not edges:
                    failures.append(dict(step=step,reason='missing front'))
                    continue
                origin,end=edges[0]
                delta=(end[0]-origin[0],end[1]-origin[1]);length=math.hypot(*delta)
                direction=(delta[0]/length,delta[1]/length)
                maximum=max(abs(direction[0]*(p[1]-origin[1])-direction[1]*(p[0]-origin[0])) for e in edges for p in e)
                if maximum>1e-5:
                    failures.append(dict(step=step,reason='zigzag',maximumDeviation=maximum))
                    continue
                intervals=sorted(sorted(direction[0]*(p[0]-origin[0])+direction[1]*(p[1]-origin[1]) for p in e) for e in edges)
                right=intervals[0][1]
                for a,b in intervals[1:]:
                    if a>right+1e-5:
                        failures.append(dict(step=step,reason='disconnected front'))
                        break
                    right=max(right,b)
            reports.append(dict(name=record['name'],steps=count,fronts=count-1,
                                triangles=triangle_count,failures=failures))
    return dict(version=scene['status']['version'],stairs=reports,
                failures=sum(len(r['failures']) for r in reports))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=ROOT/'target/debug/meshcraft')
    parser.add_argument('--output',type=Path,default=REFERENCE)
    args=parser.parse_args();output=args.output.resolve();output.mkdir(parents=True,exist_ok=True)
    with desktop(args.binary.resolve(),output/'.runtime',120,True,size=(1920,1080)) as (rpc,app,env):
        capture=Capture(rpc,env,output)
        capture.cmd('file.open',path=str(REFERENCE/'multiple-turns.octd'))
        time.sleep(.5);rpc.select([]);capture.pose([40,30,-34],[15,3,8])
        rpc.frame()
        report=inspect(rpc)
        (output/'native-validation.json').write_text(json.dumps(report,indent=2)+'\n')
        capture.shot('after-multiple-turns', destination=output/'after.png')
        capture.receipt()
        assert len(report['stairs'])==2 and report['failures']==0,report
        print(json.dumps(report),flush=True)


if __name__=='__main__':
    main()
