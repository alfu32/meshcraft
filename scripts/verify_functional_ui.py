#!/usr/bin/env python3
"""Verify native functional groups and a GUI-dispatched tool in a private desktop."""
import argparse
import json
from pathlib import Path
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

REFERENCE=ROOT/'docs/rewrite/references/functional-menus'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=ROOT/'target/debug/meshcraft')
    args=parser.parse_args()
    REFERENCE.mkdir(parents=True,exist_ok=True)
    with desktop(args.binary.resolve(),SOURCE/'captures/work/functional-ui-verification',120,True,size=(1920,1080)) as (rpc,app,env):
        capture=Capture(rpc,env,REFERENCE)
        capture.open(str(ROOT/'docs/rewrite/references/segment-feedback/model'))
        before=rpc.status()
        capture.pose([10,15,15],[0,0,0],36)
        for name in ['2D Construction','Meshing','Solids','Civil','Materials','Objects','Files','Settings']:
            capture.group(name)
        capture.group('2D Construction')
        # The native left-panel Line button, not a direct activation request.
        capture.xd('mousemove',50,610,'click',1)
        time.sleep(.2)
        assert rpc.status()['activeTool']=='Line',rpc.status()
        rpc.pointer('cancel')
        capture.group('Civil')
        capture.shot('civil',destination=REFERENCE/'civil.png')
        capture.xd('mousemove',350,51,'click',1)
        capture.shot('civil-menu',destination=REFERENCE/'civil-menu.png')
        capture.xd('key','Escape')
        capture.group('Files')
        capture.xd('mousemove',524,51,'click',1)
        capture.shot('files-menu',destination=REFERENCE/'files-menu.png')
        capture.xd('key','Escape')
        after=rpc.status()
        assert before['revision']==after['revision']
        assert before['history']==after['history']
        (REFERENCE/'native-validation.json').write_text(json.dumps(dict(
            version=capture.version,groupsVisited=8,lineActivatedThroughNativeButton=True,
            revisionUnchanged=True,historyUnchanged=True,platform='Linux Xvfb/Mesa; actual GPUI app',
        ),indent=2)+'\n')
        capture.receipt()
        print('Native functional menu checks passed',flush=True)


if __name__=='__main__':
    main()
