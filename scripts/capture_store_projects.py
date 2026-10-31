#!/usr/bin/env python3
"""Build project-scale Store scenes through Meshcraft MCP and capture the real UI."""
import argparse
import json
from pathlib import Path
import time

from capture_store_features import Capture, ROOT, SOURCE
from runtime import desktop

RECIPES = SOURCE / 'scenes/projects'
PROJECTS = ['instrument', 'kitchen', 'residence', 'casework', 'rooftop', 'park', 'loft', 'plantroom', 'studio']
SHOTS = {
    '01-direct-modeling': ('instrument','Home','view.selection'),
    '02-dynamic-instances': ('kitchen','Objects','view.object_info'),
    '03-architecture': ('residence','Civil','view.architecture_settings'),
    '04-javascript-editor': ('casework','Objects','script'),
    '05-shader-editor': ('park','Materials','shader'),
    '06-procedural-materials': ('rooftop','Materials','view.panel.materials.toggle'),
    '07-branching-preview': ('park','Objects','tree'),
    '08-architectural-stairs': ('loft','Civil','view.panel.architecture_stairs.toggle'),
    '09-mechanical-assembly': ('plantroom','Meshing','view.objects'),
    '10-technical-workspace': ('studio','Construction','view.selection'),
}


class Projects(Capture):
    def shot(self, name, preview=False, destination=None):
        frame = self.rpc.frame(preview)
        if frame.get('diagnostics') or self.rpc.status().get('scriptErrors'):
            raise RuntimeError('Cannot accept a screenshot with renderer/script diagnostics')
        super().shot(name, preview, destination)
        if destination is None:
            path = SOURCE / 'assets-manifest.json'
            manifest = json.loads(path.read_text())
            shot = next(shot for shot in manifest['screenshots'] if shot['file'] == name + '.png')
            shot['recipe'] = str(Path(__file__).resolve().relative_to(ROOT))
            path.write_text(json.dumps(manifest, indent=2) + '\n')

    def pose(self, position, target, fov=36):
        self.cmd('view.camera.set_pose', position=position, target=target,
                 field_of_view=fov, near=.05, far=5000)

    def root_geometry(self, limit=1):
        root=next(o for o in self.scene()['objects'] if o['is_scene_root'])
        return self.rpc.call('meshcraft_geometry',dict(path=root['path'],limit=limit))

    def tree(self, name, position, form=0, height=60, radius=18, seed=17, leaves=2200):
        parameters = dict(form=['broadleaf','conifer','weeping','shrub','coral','roots'][form],
                          height=height, radius=radius, seed=seed, wind=0, leaves=leaves,
                          trunkRadius=height*.025, leafSize=min(3,height*.04),
                          branches=6, forks=3, levels=3, minPixels=.25,
                          barkColor=[.27,.20,.14,1], leafColor=[.22,.38,.11,1])
        self.cmd('object.configure_procedural', settings=dict(name=name,
                 color=dict(red=73,green=114,blue=45,alpha=255),
                 procedural=dict(program=getattr(self,'tree_program','meshcraft.branching.v1'),
                                 parameters=parameters,maxInstances=12000,lodDistance=1000)))
        self.rpc.pointer('click', position)
        self.rpc.pointer('cancel')
        return self.scene()['objects'][-1]['target']

    def wait_for(self, check, message, seconds=30):
        deadline = time.monotonic() + seconds
        while not check():
            if time.monotonic() > deadline:
                raise RuntimeError(message)
            time.sleep(.05)

    def rename(self, target, name):
        self.rpc.select([target])
        self.cmd('view.object_info')
        time.sleep(.2)
        self.xd('mousemove', 1650, 296, 'click', 1, 'key', 'ctrl+a')
        self.xd('type', '--clearmodifiers', name)
        self.xd('key', 'Tab')
        self.wait_for(lambda: any(o['name'] == name for o in self.scene()['objects']), 'Object rename failed')

    def prototype(self, label, source=None):
        source = source or (RECIPES / 'cabinet.js').read_text()
        source += '\n' + (RECIPES / 'common.js').read_text()
        result = self.rpc.call('meshcraft_object_script', dict(action='create', api_version=2, source=source))
        definition = result['definition_id']
        self.wait_for(lambda: any(o['definition_id'] == definition and o['counts']['triangles'] > 0
                                 for o in self.scene()['objects']), 'Prototype geometry did not evaluate')
        target = next(o['target'] for o in self.scene()['objects'] if o['definition_id'] == definition)
        self.rename(target, label)
        return definition, target

    def parameters(self, definition, target, **values):
        for field, value in values.items():
            self.rpc.call('meshcraft_parameters', dict(action='set', definition_id=definition,
                          paths=[target['path']], id=field, value=value))

    def part(self, recipe, name, **properties):
        source = (RECIPES / 'common.js').read_text() + '\n' + (RECIPES / (recipe + '.js')).read_text()
        result = self.run(source, name, properties)
        self.receipt()
        print('Modeled:', name, flush=True)
        return result

    def begin(self, unit='cm', scale=10):
        self.new()
        self.cmd('model.update_settings', unit_name=unit, unit_size=scale, grid_size=.5)
        self.cmd('ui.update_preferences', accordion_settings_panels=True)
        for key, name, color in [
            ('ivory', 'Porcelain enamel', '#e7e3d8'), ('graphite', 'Graphite powdercoat', '#303d45'),
            ('steel', 'Brushed aluminium', '#b8c4c9'), ('navy', 'Midnight blue enamel', '#284b60'),
            ('amber', 'Safety amber', '#dc922d'), ('cyan', 'Indicator aqua', '#4bbec6'),
            ('screen', 'Instrument display', '#172e3c'), ('rubber', 'Black rubber', '#22282c'),
            ('pcb', 'Circuit laminate', '#3b735b'), ('oak', 'Quarter-sawn oak', '#b68d5e'),
            ('stone', 'Warm limestone', '#c8c4b7'), ('concrete', 'Architectural concrete', '#a9aead'),
            ('linen', 'Warm linen', '#d2cabc'), ('sage', 'Sage lacquer', '#7f9584'),
            ('brass', 'Satin brass', '#b49b61'), ('clay', 'Terracotta', '#b76d51'),
        ]:
            self.material(key, name, list(bytes.fromhex(color[1:])) + [255])

    def build_instrument(self):
        self.begin('mm', 1)
        for part, name in [('chassis', 'Controller | folded chassis'),
                           ('fascia', 'Controller | display fascia'),
                           ('vents', 'Controller | side ventilation'),
                           ('controls', 'Controller | controls and indicators'),
                           ('electronics', 'Controller | internal electronics'),
                           ('lid', 'Controller | ventilated service cover'),
                           ('terminals', 'Controller | terminal blocks'),
                           ('rear', 'Controller | power inlet')]:
            self.part('instrument', name, part=part)
        for part in ['feet', 'capacitors', 'lid-bolts', 'connector']:
            for side in range(3 if part == 'connector' else 2):
                self.part('instrument', f'Controller | {part} {side+1}', part=part, side=side)
        self.rpc.select([])
        self.pose([365, 275, 380], [118, 48, 75], 38)
        self.group('Solids')
        self.save('projects/instrument')

    def build_kitchen(self):
        self.begin()
        self.material('oak', 'Quarter-sawn oak', [182,141,94,255], 'meshcraft.wood.v1',
                      dict(axis='y', grain=35, ringScale=1.5, relief=.02,
                           lightColor=[.72,.58,.39,1], darkColor=[.54,.41,.25,1]))
        self.part('kitchen', 'Kitchen | limestone floor and room', part='room')
        definition, first = self.prototype('C-series | configurable casework')
        self.parameters(definition, first, layout='drawers')
        configurations = [
            ([6, 0, 0], dict(width=14, layout='open', shelves=0, top=False)),
            ([20, 0, 0], dict(width=5, layout='drawers')),
            ([25, 0, 0], dict(width=9, top=False)),
            ([34, 0, 0], dict(width=8, opening=23, front='oak')),
            ([0, 14.7, 0], dict(width=7, height=6, depth=3.4, feet=False, front='oak')),
            ([20, 14.7, 0], dict(width=7, height=6, depth=3.4, feet=False, front='oak')),
            ([27, 14.7, 0], dict(width=7, height=6, depth=3.4, feet=False, front='oak')),
            ([34, 14.7, 0], dict(width=7, height=6, depth=3.4, feet=False, layout='open', carcass='oak', shelves=2)),
            ([7, 0, 16], dict(width=12, front='sage')),
            ([19, 0, 16], dict(width=12, front='sage')),
        ]
        for position, values in configurations:
            target = self.place(definition, position)
            self.parameters(definition, target, **values)
        for part, name in [('tops', 'Kitchen | stone worktops and backsplash'),
                           ('sink', 'Kitchen | inset sink and tap'),
                           ('hob', 'Kitchen | induction hob and hood'),
                           ('appliance', 'Kitchen | refrigeration and built-in oven'),
                           ('styling', 'Kitchen | lighting and countertop accessories')]:
            self.part('kitchen', name, part=part)
        for x in [10, 22]:
            self.part('kitchen', f'Kitchen | bar stool {x}', part='stool', x=x)
        for x in [10.6, 16.3]:
            for z in [1.9, 4.1]:
                self.part('kitchen', f'Kitchen | hob zone {x} {z}', part='burner', x=x, z=z)
        self.rpc.select([first])
        self.pose([78, 52, 78], [24, 9, 10], 40)
        self.group('Objects')
        self.cmd('view.object_info')
        self.save('projects/kitchen')

    def build(self, project):
        method = getattr(self, 'build_' + project, None)
        if method:
            method()
        else:
            path = RECIPES / (project + '.py')
            scope = {"__file__": str(path)}
            exec(compile(path.read_text(), str(path), 'exec'), scope)
            scope['build'](self)

    def review(self, project):
        views = dict(instrument=([365,275,380],[118,48,75],38),
                     kitchen=([78,52,78],[24,9,10],40), residence=([343,230,365],[105,27,76],42),
                     loft=([116,92,137],[29,23,21],40), park=([151,105,174],[23,25,12],40),
                     rooftop=([134,108,143],[44,10,34],40),casework=([65,36,65],[22,8,3],38),
                     plantroom=([132,89,138],[44,16,25],38),studio=([54,45,59],[13,13,11],38))
        if project in ['kitchen','residence']:
            material_id = 'oak' if project=='kitchen' else 'house-deck'
            self.material(material_id, 'Quarter-sawn oak' if project=='kitchen' else 'Oiled oak decking',
                          [182,141,94,255], 'meshcraft.wood.v1',
                          dict(axis='y' if project=='kitchen' else 'x',grain=35,ringScale=1.5,
                               relief=.02,lightColor=[.72,.58,.39,1],darkColor=[.54,.41,.25,1]))
        if project=='residence':
            for o in self.scene()['objects']:
                if o['proceduralObject']:
                    settings=o['proceduralObject'];p=settings['procedural']['parameters'];h=p['height']
                    p.update(trunkRadius=h*.025,leafSize=min(3,h*.04),branches=6,forks=3,levels=3,
                             leaves=2200,minPixels=.25,barkColor=[.27,.20,.14,1],leafColor=[.22,.38,.11,1])
                    self.cmd('object.configure_procedural',definition_id=o['definition_id'],instance_path=o['path'],settings=settings)
        if project=='rooftop':
            self.material('roof-foliage','Lush ground foliage',[69,114,46,255],'meshcraft.foliage.v1',
                          dict(density=.7,size=3.5,width=1.3,height=.35,tilt=40,wind=0,seed=7,
                               leafColor=[.18,.35,.08,1],alternateColor=[.31,.46,.13,1]))
            self.material('oak','Terrace oak',[164,126,86,255],'meshcraft.wood.v1',
                          dict(axis='x',ringScale=.35,grain=8,relief=.01,
                               lightColor=[.67,.53,.36,1],darkColor=[.60,.46,.30,1]))
        if project=='plantroom' and not any(o['name']=='Plant | service rail posts' for o in self.scene()['objects']):
            self.part('plantroom','Plant | service rail posts',part='frame-posts')
        if project in views:
            self.pose(*views[project])
        self.save('projects/'+project)

    def studio_axonometry(self):
        self.cmd('view.camera.orthographic')
        self.xd('mousemove',950,660,'mousedown',3,'mousemove',808,437,'mouseup',3)
        time.sleep(.2)
        assert self.rpc.status()['camera']['orthographic']

    def capture_project(self, name):
        project,tab,panel=SHOTS[name]
        self.open('projects/'+project)
        self.review(project)
        self.group(tab)
        if project=='kitchen':
            self.rpc.select([next(o['target'] for o in self.scene()['objects'] if o['name'].startswith('C-series'))])
        elif project=='loft':
            g=self.root_geometry()
            self.rpc.select([next(a['target'] for a in g['architecture'] if a['record']['name']=='Loft | turning access stair')])
        elif project=='residence':
            g=self.root_geometry()
            self.rpc.select([next(a['target'] for a in g['architecture'] if a['record']['name']=='Living frontage | glazing 2')])
        if panel in ['script','shader','tree']:
            self.editor_draft(panel, name)
        else:
            self.cmd(panel)
            if project=='rooftop':
                self.xd('mousemove',1700,360,'click','--repeat',35,'--delay',30,5)
            if project=='studio':
                self.studio_axonometry()
                self.cmd(panel)
            time.sleep(.3)
            self.shot(name)
        evidence_path=self.output/'project-screenshot-evidence.json'
        evidence=json.loads(evidence_path.read_text()) if evidence_path.exists() else {}
        data=self.scene()
        evidence[name]=dict(project='projects/'+project+'.octd',version=self.version,
                           objects=len([o for o in data['objects'] if not o['is_scene_root']]),
                           authoredFaces=sum(o['counts']['triangles'] for o in data['objects']),
                           authoredSegments=sum(o['counts']['segments'] for o in data['objects']),
                           mainCamera=self.rpc.status()['camera'],viewMode=panel if panel in ['script','shader','tree'] else 'main',
                           scriptErrors=self.rpc.status()['scriptErrors'])
        evidence_path.write_text(json.dumps(evidence,indent=2)+'\n')

    def editor_draft(self, mode, name=None):
        if mode=='script':
            target=next(o['target'] for o in self.scene()['objects'] if o['name'].startswith('Atelier'))
            self.rpc.select([target]);self.cmd('script.edit')
        else:
            target=next(o['target'] for o in self.scene()['objects'] if o['name']=='Canopy oak')
            self.rpc.select([target]);self.cmd('object.edit_tree')
        self.rpc.frame(True)
        if mode=='script':
            self.xd('mousemove',1450,570,'click','--repeat',3,'--delay',150,5)
            self.xd('mousemove',1450,560,'mousedown',3,'mousemove',1555,560,'mouseup',3)
            self.xd('mousemove',1550,560,'mousedown',2,'mousemove',1353,637,'mouseup',2)
        if mode in ['shader-bottom','shader']:
            self.xd('mousemove',500,800,'click','--repeat',30,'--delay',40,5)
            time.sleep(.3)
        if mode=='shader':
            self.xd('mousemove',490,639,'click',1)
            self.rpc.frame(True)
            self.shot('shader-json-draft',True,destination=self.output/'work'/'shader-json-draft.png')
            self.xd('mousemove',250,145,'click',1)
        if mode in ['tree','shader']:
            self.xd('mousemove',1220,570,'click','--repeat',2,'--delay',150,4)
        self.shot(name or mode,True,destination=None if name else self.output/'work'/(mode+'-draft.png'))
        self.xd('key','Escape','Escape')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    parser.add_argument('--project', choices=PROJECTS)
    parser.add_argument('--capture-only', action='store_true')
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--editor-draft',choices=['script','tree','shader-bottom','shader'])
    parser.add_argument('--shot',choices=list(SHOTS))
    args = parser.parse_args()
    output = SOURCE / 'captures'
    (output / 'projects').mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), output / 'work/projects-runtime', 120, True,
                 size=(1920, 1080)) as (rpc, app, env):
        c = Projects(rpc, env, output)
        if not args.capture_only:
            for project in [args.project] if args.project else PROJECTS:
                try:
                    c.build(project)
                except Exception:
                    c.cmd('file.save_as', path=str(output / 'work' / (project + '-partial.octd')))
                    c.receipt()
                    raise
                c.shot(project, destination=output / 'work' / (project + '-draft.png'))
                journal = output / 'project-modeling-actions.json'
                records = json.loads(journal.read_text()) if journal.exists() else {}
                records[project] = dict(version=c.version, recipe=str(RECIPES.relative_to(ROOT)), actions=c.actions)
                journal.write_text(json.dumps(records, indent=2) + '\n')
                c.actions = []
        elif args.project:
            c.open('projects/'+args.project)
            c.review(args.project)
            if args.editor_draft:
                c.editor_draft(args.editor_draft)
            else:
                c.shot(args.project, destination=output / 'work' / (args.project + '-draft.png'))
        if not args.build_only:
            for name in [args.shot] if args.shot else SHOTS:
                if not args.project or SHOTS[name][0]==args.project:
                    c.capture_project(name)


if __name__ == '__main__':
    main()
