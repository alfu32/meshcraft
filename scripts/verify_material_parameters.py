#!/usr/bin/env python3
"""Qualify material parameter forms and procedural bindings in an isolated app."""
import argparse,json,time
from pathlib import Path
from capture_store_features import Capture,ROOT,SOURCE
from runtime import desktop,Failure


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=ROOT/'target/debug/meshcraft')
    args=parser.parse_args()
    ref=ROOT/'docs/rewrite/references/material-parameters'
    with desktop(args.binary.resolve(),SOURCE/'captures/work/material-parameters',120,True,size=(1920,1080)) as (rpc,app,env):
        c=Capture(rpc,env,ref)
        result=rpc.call('meshcraft_object_script',dict(action='create',api_version=2,source=(ref/'material-component.js').read_text()))
        definition=result['definition_id']
        for _ in range(100):
            scene=c.scene();objects=[o for o in scene['objects'] if o['definition_id']==definition]
            if objects and objects[0]['counts']['triangles']==12:break
            time.sleep(.05)
        first=objects[0]
        wood=dict(id='parameter-wood',name='Parameter Wood',color=dict(red=171,green=99,blue=41,alpha=255),procedural=dict(program='meshcraft.wood.v1',parameters={}))
        c.cmd('material.save',material=wood)
        rpc.call('meshcraft_parameters',dict(action='set',definition_id=definition,paths=[first['path']],id='finish',value=wood['id']))
        for _ in range(100):
            g=rpc.call('meshcraft_geometry',dict(path=first['path'],limit=100))
            faces=[p['record'] for p in g['primitives'] if 'vertices' in p['record']]
            if faces and all(f['extensions'].get('k3d.meshcraftMaterialId')==wood['id'] for f in faces):break
            if rpc.status()['scriptErrors']:raise RuntimeError(rpc.status()['scriptErrors'])
            time.sleep(.05)
        else:raise RuntimeError('Generated faces did not acquire catalog binding')
        rpc.select([first['target']]);rows=rpc.call('meshcraft_parameters',dict(action='get'))['parameters']
        assert {row['scope'] for row in rows if row['kind']=='material'}=={'instance','prototype'}
        c.pose([6,5,7],[1.5,1,1],36);c.cmd('view.object_info')
        c.shot('material-form',destination=ref/'material-form.png')
        # A fresh rendered shader frame must be healthy and its wood pattern is
        # retained as native visual evidence; face bindings are checked above.
        frame=rpc.frame();assert not frame.get('proceduralDiagnostics'),frame
        # Add/rename material after onCreate has already run; forms use catalog IDs.
        blue=dict(id='parameter-blue',name='New Blue',color=dict(red=35,green=85,blue=180,alpha=255))
        c.cmd('material.save',material=blue)
        blue['name']='Renamed Blue';c.cmd('material.save',material=blue)
        rpc.call('meshcraft_parameters',dict(action='set',definition_id=definition,paths=[first['path']],id='finish',value=blue['id']))
        for _ in range(100):
            geometry=rpc.call('meshcraft_geometry',dict(path=first['path'],limit=100))
            faces=[p['record'] for p in geometry['primitives'] if 'vertices' in p['record']]
            if faces and all(f['extensions'].get('k3d.meshcraftMaterialId')==blue['id'] for f in faces):break
            time.sleep(.05)
        else:raise RuntimeError('Renamed catalog material did not update evaluated geometry')
        c.shot('material-catalog-update' ,destination=ref/'material-catalog-update.png')
        c.cmd('object.place_selected_definition',definition_id=definition)
        rpc.pointer('click',[4,0,0]);rpc.pointer('cancel')
        objects=[o for o in c.scene()['objects'] if o['definition_id']==definition];assert len(objects)==2
        rpc.select([o['target'] for o in objects])
        rows=rpc.call('meshcraft_parameters',dict(action='get'))['parameters'];finish=next(row for row in rows if row['id']=='finish');assert finish['value'] is None
        paths=[o['path'] for o in objects]
        rpc.call('meshcraft_parameters',dict(action='set',definition_id=definition,paths=paths,id='finish',value=wood['id']))
        for _ in range(100):
            rpc.select([o['target'] for o in objects]);rows=rpc.call('meshcraft_parameters',dict(action='get'))['parameters']
            if next(row for row in rows if row['id']=='finish')['value']==wood['id']:break
            time.sleep(.05)
        else:raise RuntimeError('Selection-wide material edit failed')
        try:c.cmd('material.delete',id=wood['id'])
        except Failure:pass
        else:raise RuntimeError('Used material was deleted')
        c.cmd('file.save_as',path=str(ref/'material-parameters.octd'))
        c.cmd('file.open',path=str(ref/'material-parameters.octd'))
        time.sleep(.2)
        objects=[o for o in c.scene()['objects'] if o['definition_id']==definition]
        # Native file IDs may remap across serialization; select by scripted name.
        if not objects:objects=[o for o in c.scene()['objects'] if o['name']=='Scripted Object']
        rpc.select([o['target'] for o in objects]);rows=rpc.call('meshcraft_parameters',dict(action='get'))['parameters']
        assert next(row for row in rows if row['id']=='finish')['value']==wood['id']
        (ref/'native-validation.json').write_text(json.dumps(dict(version=c.version,prototypeAndInstancePickers=True,materialAddedAndRenamed=True,proceduralFaceBinding=wood['id'],mixedSelection=True,selectionWideEdit=True,usedMaterialDeletionRejected=True,saveOpenPreservesMaterial=True,platform='Linux private Xvfb/Mesa; actual GPUI app'),indent=2)+'\n')
        c.receipt();print('Native material parameter and procedural binding checks passed',flush=True)


if __name__=='__main__':main()
