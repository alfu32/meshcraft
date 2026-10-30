#!/usr/bin/env python3
"""Open Meshcraft, operate its MCP API, and capture genuine Store demonstrations.

Projects, images, MCP receipts and recipes are durable source assets outside
Cargo's target directory. Only the executable is read from target by default.
Linux capture needs Xvfb, Mesa, xdotool and ImageMagick (import).
"""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'packaging/microsoft-store'
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from runtime import desktop

NAMES = ['01-direct-modeling', '02-dynamic-instances', '03-architecture',
         '04-javascript-editor', '05-shader-editor', '06-procedural-materials',
         '07-branching-preview', '08-architectural-stairs']


class Capture:
    def __init__(self, rpc, env, output):
        self.rpc, self.env, self.output = rpc, env, output
        self.actions = []
        self.version = rpc.status()['version']
        self.original_call = rpc.call
        self.rpc.call = self.call
        self.window = subprocess.check_output(
            ['xdotool', 'search', '--name', '^Meshcraft [0-9]+\\.[0-9]+\\.[0-9]+'], env=env, text=True
        ).splitlines()[-1]
        self.rpc.call('k3d_contract')
        self.rpc.call('k3d_list_commands')
        self.cmd('ui.update_preferences', show_construction_guides=False,
                 show_permanent_grid=False, antialiasing_quality=2)

    def call(self, name, args=None):
        result = self.original_call(name, args)
        if name not in ['meshcraft_get_job', 'k3d_status', 'meshcraft_capture_view']:
            self.actions.append(dict(tool=name, arguments=args or {}))
        return result

    def cmd(self, key, **args):
        return self.rpc.command(key, args)

    def xd(self, *args):
        subprocess.run(['xdotool', *map(str, args)], env=self.env, check=True)

    def scene(self):
        return self.rpc.call('meshcraft_scene')

    def new(self):
        previous = self.rpc.status()['generation']
        self.cmd('file.new')
        deadline = time.monotonic() + 60
        while self.rpc.status()['generation'] == previous:
            if time.monotonic() > deadline:
                raise RuntimeError('Document replacement did not complete')
            time.sleep(.05)

    def open(self, name):
        path = self.output / (name+'.octd')
        self.cmd('file.open', path=str(path))
        deadline = time.monotonic()+60
        while self.rpc.status()['projectPath'] != str(path):
            if time.monotonic() > deadline:
                raise RuntimeError('Project open did not complete')
            time.sleep(.05)
        self.rpc.select([])
        self.rpc.frame()

    def pose(self, position, target, fov=36):
        self.cmd('view.camera.set_pose', position=position, target=target, field_of_view=fov)

    def run(self, source, name, properties=None):
        job = self.rpc.call('meshcraft_run_script', dict(
            source=source, name=name, properties=properties or {}))
        deadline = time.monotonic()+120
        while True:
            result = self.rpc.call('meshcraft_get_job', dict(job_id=job['job_id']))['result']
            if result['completion']['state'] != 'pending':
                break
            if time.monotonic() > deadline:
                raise RuntimeError('Script job timed out; inspect its state before repeating it')
            time.sleep(.05)
        if result['completion']['state'] != 'completed':
            raise RuntimeError(result)
        return result['outputs']

    def save(self, name):
        self.cmd('file.save_as', path=str(self.output/(name+'.octd')))
        self.receipt()

    def receipt(self):
        (self.output/'mcp-actions.json').write_text(json.dumps(self.actions, indent=2)+'\n')

    def place(self, definition, position):
        self.cmd('object.place_selected_definition', definition_id=definition)
        self.rpc.pointer('click', position)
        self.rpc.pointer('cancel')
        return self.scene()['objects'][-1]['target']

    def move(self, target, delta):
        self.rpc.select([target])
        self.cmd('tool.builtin.move')
        self.rpc.pointer('click', [0, 0, 0])
        self.rpc.pointer('click', delta)
        self.rpc.pointer('cancel')

    def material(self, id, name, color, program=None, params=None):
        record = dict(id=id, name=name, color=dict(zip(['red','green','blue','alpha'], color)),
                      shadingType='phong', textureScaleU=1, textureScaleV=1)
        if program:
            record['procedural'] = dict(program=program, parameters=params or {},
                                       maxInstances=20000, lodDistance=250, showBounds=False)
        self.cmd('material.save', material=record)
        return record

    def tree(self, name, position, form=0, height=7, radius=3, seed=17, leaves=1600):
        self.cmd('object.configure_procedural', settings=dict(name=name,
            color=dict(red=80, green=140, blue=45, alpha=255),
            procedural=dict(program='meshcraft.branching.v1', parameters=dict(
                form=['broadleaf','conifer','weeping','shrub','coral','roots'][form],
                height=height, radius=radius, seed=seed, wind=0, leaves=leaves),
                maxInstances=12000, lodDistance=250)))
        self.rpc.pointer('click', position)
        self.rpc.pointer('cancel')
        return self.scene()['objects'][-1]['target']

    def shot(self, name, preview=False, destination=None):
        frame = self.rpc.frame(preview)
        self.xd('mousemove', 1100, 17)  # neutral title-bar location, no tooltip
        time.sleep(.6)
        path = destination or SOURCE/'assets/screenshots'/(name+'.png')
        subprocess.run(['import', '-window', self.window, str(path)], env=self.env, check=True)
        receipts = self.output/'capture-receipts.json'
        records = json.loads(receipts.read_text()) if receipts.exists() else {}
        records[name] = dict(version=self.version, project=self.rpc.status()['projectPath'],
            preview=preview, revision=frame['revision'], generation=frame['generation'],
            frameSequence=frame['frameSequence'], sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
            method='Meshcraft MCP modeling/camera/preview; native unaltered app-window capture')
        receipts.write_text(json.dumps(records, indent=2)+'\n')
        if destination is None:
            manifest_path = SOURCE/'assets-manifest.json'
            manifest = json.loads(manifest_path.read_text())
            shot = next(s for s in manifest['screenshots'] if s['file'] == path.name)
            project = Path(records[name]['project']).resolve()
            if not project.is_relative_to(SOURCE):
                raise RuntimeError('Store source projects must remain in packaging/microsoft-store')
            shot.update(source=str(project.relative_to(ROOT)), recipe='scripts/capture_store_features.py',
                        captureTool='Meshcraft MCP + native app-window capture',
                        applicationVersion=self.version, edited=False,
                        platform='Linux GPUI app; isolated 1920x1080 Xvfb/Mesa desktop',
                        sha256=records[name]['sha256'])
            manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
        self.receipt()
        print('Captured', name, flush=True)

    def viewport(self, name, preview=False):
        self.rpc.frame(preview)
        request = urllib.request.Request(self.rpc.url, json.dumps(dict(jsonrpc='2.0', id=1,
            method='tools/call', params=dict(name='meshcraft_capture_view',
            arguments=dict(max_width=2048, preview=preview)))).encode(),
            {'Content-Type':'application/json', 'Accept':'application/json, text/event-stream'})
        with urllib.request.urlopen(request, timeout=120) as response:
            result = json.load(response)['result']
        if result.get('isError'):
            raise RuntimeError(result)
        image = next(c for c in result['content'] if c['type']=='image')
        (self.output/(name+'.png')).write_bytes(base64.b64decode(image['data']))

    def recipe(self, name):
        # These checked-in recipes are ordinary reviewed Python; all model
        # mutations go through the application's published MCP commands/SDK.
        scope = dict(root=ROOT, output=self.output, rpc=self.rpc, Path=Path, time=time)
        for key in ['cmd','scene','new','pose','run','save','place','move','material',
                    'viewport','tree','shot','xd']:
            scope[key] = getattr(self, key)
        path = SOURCE/'scenes'/(name+'.py')
        exec(compile(path.read_text(), str(path), 'exec'), scope)

    def group(self, name):
        # Use measured native tab bounds so captions/layout changes do not break captures.
        name = {'2D Construction': 'Construction', 'Selection': 'Construction'}.get(name, name)
        state = self.rpc.status().get('workspace')
        if state:
            bounds = state['controls']['ribbon/' + name]
            self.xd('mousemove', round(bounds['x'] + bounds['width'] / 2),
                    round(bounds['y'] + bounds['height'] / 2), 'click', 1)
            deadline = time.monotonic() + 5
            while self.rpc.status()['workspace']['ribbonTab'] != name:
                if time.monotonic() > deadline:
                    raise RuntimeError('Native ribbon tab did not activate: ' + name)
                time.sleep(.02)
        else:
            x = {'Files': 35, 'Clipboard & History': 145, 'Construction': 370,
                 'Meshing': 480, 'Solids': 555, 'Civil': 618, 'Materials': 690,
                 'Objects': 775, 'Hotspots': 855, 'Settings': 944}[name]
            self.xd('mousemove', x, 82, 'click', 1)
        self.rpc.frame()

    def capture(self, name):
        project = {'04-javascript-editor':'02-dynamic-instances',
                   '05-shader-editor':'02-dynamic-instances'}.get(name, name)
        self.open(project)
        self.group({'01-direct-modeling':'Meshing','02-dynamic-instances':'Objects',
                    '03-architecture':'Civil','04-javascript-editor':'Objects',
                    '05-shader-editor':'Materials','06-procedural-materials':'Materials',
                    '07-branching-preview':'Objects','08-architectural-stairs':'Civil'}[name])
        if name == '04-javascript-editor':
            self.rpc.select([self.scene()['objects'][1]['target']])
            self.cmd('script.edit')
            self.shot(name, True)
            self.xd('key', 'Escape')
        elif name == '05-shader-editor':
            self.cmd('material.new_water')
            time.sleep(.5)
            self.xd('mousemove', 600, 850, 'click', '--repeat', 12, '--delay', 60, 5)
            time.sleep(.4)
            self.xd('mousemove', 480, 510, 'click', 1)
            self.rpc.frame(True)
            self.xd('mousemove', 380, 151, 'click', 1)
            self.shot(name, True)
            self.xd('key', 'Escape')
            self.xd('key', 'Escape')
        elif name == '07-branching-preview':
            self.rpc.select([self.scene()['objects'][1]['target']])
            self.cmd('object.edit_tree')
            self.rpc.frame(True)
            self.xd('mousemove', 1230, 530, 'click', '--repeat', 2, '--delay', 200, 4)
            self.shot(name, True)
            self.xd('key', 'Escape')
        else:
            panel = {'01-direct-modeling':'selection', '02-dynamic-instances':'object',
                     '03-architecture':'materials', '06-procedural-materials':'materials',
                     '08-architectural-stairs':'architecture_stairs'}[name]
            if name == '01-direct-modeling':
                self.pose([18,14,24],[0,2,-1.5],36)
                self.rpc.select([self.scene()['objects'][1]['target']])
            if name == '02-dynamic-instances':
                self.rpc.select([self.scene()['objects'][2]['target']])
            if name == '08-architectural-stairs':
                self.pose([30,16,-24], [10,3,6])
                geometry = self.rpc.call('meshcraft_geometry', dict(path=self.scene()['objects'][0]['path'], limit=1))
                owner = next(a for a in geometry['architecture'] if a['record']['name']=='Straight companion')
                self.rpc.select([owner['target']])
            self.cmd('view.panel.'+panel+'.toggle')
            if panel == 'materials':
                self.xd('mousemove', 1700, 380, 'click', '--repeat', 25, '--delay', 40, 5)
            self.shot(name)


def refresh_readme():
    """Refresh stable README paths with unaltered native app-window images."""
    import shutil
    destination=ROOT/'docs/screenshots'
    manifest=json.loads((destination/'manifest.json').read_text())
    source_names=['01-direct-modeling','03-architecture','02-dynamic-instances',
                  '04-javascript-editor','07-branching-preview','06-procedural-materials',
                  '08-architectural-stairs','05-shader-editor']
    receipts=json.loads((SOURCE/'captures/capture-receipts.json').read_text())
    for shot,name in zip(manifest['screenshots'],source_names,strict=True):
        source=SOURCE/'assets/screenshots'/(name+'.png')
        shutil.copyfile(source,destination/shot['file'])
        shot.update(source=str(source.relative_to(ROOT)),capture='Live Meshcraft MCP workflow; unaltered native app-window capture',
                    applicationVersion=receipts[name]['version'],refresh='complete',sha256=receipts[name]['sha256'])
    manifest.update(regenerate_after='Future menu/ribbon changes',
                    policy='Actual native app captures only. Keep filenames stable and refresh after UI changes.',
                    refreshed_for=f"Meshcraft {receipts[source_names[0]]['version']} command icons and workspace")
    (destination/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/debug/meshcraft')
    parser.add_argument('--scene', choices=NAMES)
    parser.add_argument('--rebuild-scenes', action='store_true')
    parser.add_argument('--refresh-readme', action='store_true')
    args = parser.parse_args()
    output = SOURCE/'captures'
    output.mkdir(parents=True, exist_ok=True)
    with desktop(args.binary.resolve(), output/'.runtime', 120, True, size=(1920,1080)) as (rpc, app, env):
        capture = Capture(rpc, env, output)
        capture.cmd('ui.update_preferences', accordion_settings_panels=True)
        if args.rebuild_scenes:
            for recipe in ['direct', 'sweep-3d', 'dynamic', 'architecture', 'materials', 'stairs', 'trees']:
                capture.recipe(recipe)
        for name in [args.scene] if args.scene else NAMES:
            capture.capture(name)
        capture.receipt()
        print('Preserved captures and MCP actions in', output, flush=True)
    if args.refresh_readme:
        refresh_readme()


if __name__ == '__main__':
    main()
