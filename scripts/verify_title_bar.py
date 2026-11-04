#!/usr/bin/env python3
"""Drive the title bar buttons and recomposed ribbon in the actual native app.

Every button below is pressed with a real pointer event at its measured bounds.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
sys.path.insert(0, str(ROOT / 'scripts/stress'))
from capture_store_features import Capture, SOURCE  # noqa: E402
from runtime import desktop  # noqa: E402

REFERENCE = ROOT / 'docs/rewrite/references/title-bar'
TABS = ['Home', 'Construction', 'Solids', 'Civil', 'Materials', 'Objects & Hotspots', 'Settings']
TOOLS = ['file.new', 'file.open', 'file.open_recent', 'file.save', 'file.save_as',
         'file.cancel_open', 'app.quit', 'import.mesh', 'export.mesh', 'export.ifc_model',
         'export.svg_view', 'export.screenshot', 'interchange.cancel', 'edit.undo', 'edit.redo',
         'edit.cut_selection', 'edit.copy_selection', 'edit.paste', 'edit.paste_in_place',
         'edit.duplicate_selection', 'edit.delete_selection']


def wait_for(check, what, timeout=60):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise RuntimeError('Timed out waiting for ' + what)
        time.sleep(.05)


class Run:
    def __init__(self, rpc, env, reference):
        self.rpc = rpc
        self.env = env
        self.c = Capture(rpc, env, reference)
        self.reference = reference
        self.checks = {}

    def expect(self, name, passed, detail=''):
        self.checks[name] = bool(passed)
        print(('ok   ' if passed else 'FAIL ') + name + (f' ({detail})' if detail else ''), flush=True)

    def controls(self):
        return self.rpc.status()['workspace']['controls']

    def press(self, control):
        bounds = self.controls()[control]
        self.c.xd('mousemove', round(bounds['x'] + bounds['width'] / 2),
                  round(bounds['y'] + bounds['height'] / 2), 'click', 1)
        time.sleep(.25)

    def window_shot(self, name):
        # Works while a modal editor keeps the viewport from presenting a frame.
        time.sleep(.5)
        subprocess.run(['import', '-window', 'root', str(self.reference / (name + '.png'))],
                       env=self.env, check=True)

    def revision(self):
        return self.rpc.status()['revision']

    def layout(self, width):
        controls = self.controls()
        missing = [tool for tool in TOOLS if 'title/' + tool not in controls]
        self.expect(f'{width}: all 21 title buttons are measured', not missing, str(missing))
        lefts = [controls['title/' + tool]['x'] for tool in TOOLS if tool not in missing]
        self.expect(f'{width}: title buttons keep their order', lefts == sorted(lefts))
        right = max(c['x'] + c['width'] for key, c in controls.items() if key.startswith('title/'))
        self.expect(f'{width}: title buttons leave room for the title and window controls',
                    right + 130 + 100 <= width, f'buttons end at {right}')
        title = controls['window/title']
        cluster = controls['search/cluster']
        self.expect(f'{width}: title sits between the buttons and the search box',
                    title['x'] >= right and title['x'] + title['width'] <= cluster['x'],
                    f"title spans {title['x']} to {title['x'] + title['width']}")
        self.expect(f'{width}: search box is right-aligned before the window controls',
                    cluster['x'] + cluster['width'] <= width - 90
                    and cluster['x'] + cluster['width'] >= width - 160,
                    f"search spans {cluster['x']} to {cluster['x'] + cluster['width']}")
        self.expect(f'{width}: search box and its buttons are usable',
                    controls['search/box']['width'] >= 90
                    and controls['search/button']['width'] > 0
                    and controls['search/chevron']['width'] > 0)
        self.expect(f'{width}: title buttons stay on the title bar',
                    all(controls['title/' + t]['y'] + controls['title/' + t]['height'] <= 34
                        for t in TOOLS if t not in missing))

    def ribbon(self):
        self.expect('ribbon has the seven requested tabs',
                    self.rpc.status()['workspace']['ribbonTabs'] == TABS)
        for tab in TABS:
            self.c.group(tab)
            self.c.shot('tab', destination=self.reference / ('ribbon-' + tab.lower().replace(' & ', '-') + '.png'))
        self.c.group('Home')

    def clipboard_and_history(self):
        self.c.cmd('file.open_sample', sample='kitchen')
        wait_for(lambda: self.rpc.status()['revision'] and len(self.c.scene()['objects']) > 0, 'sample')
        wait_for(lambda: not self.rpc.status()['workspace']['titleToolsBlocked'], 'sample to settle')
        self.rpc.select([])
        start = self.revision()
        self.press('title/edit.delete_selection')
        self.expect('disabled title button does nothing', self.revision() == start)
        objects = self.c.scene()['objects']
        count = len(objects)
        self.rpc.select([objects[1]['target']])
        self.press('title/edit.delete_selection')
        wait_for(lambda: len(self.c.scene()['objects']) == count - 1, 'Delete button')
        self.expect('Delete button removes the selection', True)
        self.press('title/edit.undo')
        wait_for(lambda: len(self.c.scene()['objects']) == count, 'Undo button')
        self.expect('Undo button restores it', True)
        self.press('title/edit.redo')
        wait_for(lambda: len(self.c.scene()['objects']) == count - 1, 'Redo button')
        self.expect('Redo button deletes it again', True)
        self.press('title/edit.undo')
        wait_for(lambda: len(self.c.scene()['objects']) == count, 'second Undo')
        objects = self.c.scene()['objects']
        self.rpc.select([objects[1]['target']])
        self.press('title/edit.copy_selection')
        self.press('title/edit.paste_in_place')
        wait_for(lambda: len(self.c.scene()['objects']) == count + 1, 'Copy and Paste in Place buttons')
        self.expect('Copy and Paste in Place buttons add a copy', True)
        self.press('title/edit.undo')
        wait_for(lambda: len(self.c.scene()['objects']) == count, 'Undo of paste')

    def modal(self):
        self.c.xd('key', 'ctrl+shift+p')
        self.c.xd('type', '--clearmodifiers', 'material.new_shader')
        time.sleep(.2)
        self.c.xd('key', 'Return')
        wait_for(lambda: self.rpc.status()['workspace']['titleToolsBlocked'], 'shader editor')
        self.window_shot('modal-blocks-title-buttons')
        self.search_behind_modal()
        count = len(self.c.scene()['objects'])
        self.press('title/edit.undo')
        self.press('title/file.new')
        self.expect('title buttons do nothing behind a modal editor',
                    len(self.c.scene()['objects']) == count
                    and self.rpc.status()['workspace']['titleToolsBlocked'])
        self.c.xd('key', 'Escape')
        wait_for(lambda: not self.rpc.status()['workspace']['titleToolsBlocked'], 'editor to close')

    def hidden_surfaces(self):
        self.c.cmd('ui.update_preferences', menu_bar_visible=False, ribbon_visible=False,
                   left_panel_visible=False)
        time.sleep(.3)
        self.rpc.frame()
        controls = self.controls()
        self.expect('title buttons remain when menus, ribbon and left panel are hidden',
                    all('title/' + tool in controls for tool in TOOLS))
        self.window_shot('surfaces-hidden')
        self.c.cmd('ui.update_preferences', menu_bar_visible=True, ribbon_visible=True,
                   left_panel_visible=True)
        time.sleep(.3)

    def recent(self):
        # Reopening a saved project: the Recent button replaces the Files submenu.
        project = SOURCE / 'captures/work/title-bar-recent.octd'
        self.c.cmd('file.save_as', path=str(project))
        wait_for(lambda: not self.rpc.status().get('dirty'), 'save')
        time.sleep(.5)
        self.c.cmd('file.new', preset='product_metric')
        time.sleep(.5)
        self.rpc.frame()
        self.press('title/file.open_recent')
        self.window_shot('recent-files')
        bounds = self.controls()['title/file.open_recent']
        self.c.xd('mousemove', round(bounds['x'] + 60), round(bounds['y'] + bounds['height'] + 25),
                  'click', 1)
        def opened():
            title = subprocess.check_output(['xdotool', 'getwindowname', self.c.window],
                                            env=self.env, text=True)
            return str(project) in title
        wait_for(opened, 'Recent Files row to open the project')
        self.expect('Recent button opens a list whose row opens the project', True)

    def workspace(self):
        return self.rpc.status()['workspace']

    def command_search(self):
        status = self.rpc.status
        self.press('search/box')
        self.expect('clicking the search box opens the list', self.workspace()['commandListOpen'])
        tool = status()['activeTool']
        self.c.xd('type', '--clearmodifiers', 'tool.builtin.line')
        time.sleep(.3)
        self.expect('typing filters without reaching tool hotkeys',
                    self.workspace()['commandQuery'] == 'tool.builtin.line'
                    and status()['activeTool'] == tool)
        self.window_shot('command-search')
        self.press('search/box')
        self.expect('clicking the box again keeps the list and text',
                    self.workspace()['commandListOpen']
                    and self.workspace()['commandQuery'] == 'tool.builtin.line')
        self.press('search/chevron')
        self.expect('chevron hides the list and keeps the text',
                    not self.workspace()['commandListOpen']
                    and self.workspace()['commandQuery'] == 'tool.builtin.line')
        self.press('search/chevron')
        self.expect('chevron shows the list again', self.workspace()['commandListOpen'])
        self.press('search/chevron')
        self.press('search/button')
        self.expect('search button shows the matches for the text',
                    self.workspace()['commandListOpen']
                    and self.workspace()['commandQuery'] == 'tool.builtin.line')
        # Linear Dimension sorts first, so this also checks the selection moves.
        self.c.xd('key', 'Down', 'Return')
        wait_for(lambda: status()['activeTool'] == 'Line', 'Enter to run the match')
        self.expect('Enter runs the selected command and closes the list',
                    not self.workspace()['commandListOpen'])
        time.sleep(.2)
        self.expect('running a command clears the text', self.workspace()['commandQuery'] == '')
        self.rpc.pointer('cancel')
        self.c.cmd('tool.select')
        self.c.xd('key', 'ctrl+shift+p')
        time.sleep(.3)
        self.expect('Ctrl+Shift+P opens the list', self.workspace()['commandListOpen'])
        self.c.xd('type', '--clearmodifiers', 'view.ortho.')
        time.sleep(.2)
        self.c.xd('key', 'Down', 'Return')
        time.sleep(.3)
        self.expect('Down and Enter run the second match', 'bottom orthographic' in status()['status'],
                    status()['status'])
        self.c.cmd('view.camera.perspective')
        self.c.xd('key', 'ctrl+shift+p')
        time.sleep(.2)
        self.c.xd('key', 'ctrl+shift+p')
        time.sleep(.3)
        self.expect('Ctrl+Shift+P closes the list', not self.workspace()['commandListOpen'])
        self.press('search/box')
        self.c.xd('type', '--clearmodifiers', 'abc')
        time.sleep(.2)
        self.c.xd('key', 'Escape')
        time.sleep(.3)
        self.expect('Escape closes the list and clears the text',
                    not self.workspace()['commandListOpen'] and self.workspace()['commandQuery'] == '')
        tool = status()['activeTool']
        self.press('search/box')
        # The list only reacts to outside clicks once painted, and frames are slow here.
        time.sleep(1)
        self.c.xd('mousemove', 800, 600, 'click', 1)
        time.sleep(.5)
        self.expect('clicking elsewhere closes the list', not self.workspace()['commandListOpen'])
        # G only starts a guide with nothing selected, and that click selected an object.
        # After a viewport click it also needs an Escape first, with or without search.
        self.c.xd('key', 'Escape')
        self.rpc.select([])
        time.sleep(.3)
        self.c.xd('key', 'g')
        wait_for(lambda: status()['activeTool'] == 'Grid Guide', 'G hotkey after closing the list')
        self.expect('tool hotkeys work again after closing the list', True)
        self.c.xd('key', 'Escape')
        wait_for(lambda: status()['activeTool'] == tool, 'Escape to cancel the guide tool')

    def search_behind_modal(self):
        self.press('search/box')
        self.expect('search box does nothing behind a modal editor',
                    not self.workspace()['commandListOpen'])

    def home(self):
        self.c.cmd('file.home', open=True)
        time.sleep(.5)
        self.rpc.frame()
        self.expect('Home screen hides the title buttons',
                    not any(key.startswith('title/') for key in self.controls()))
        self.press('search/box')
        self.c.xd('type', '--clearmodifiers', 'view.ortho.top')
        time.sleep(.2)
        self.expect('command search works on the Home screen', self.workspace()['commandListOpen'])
        self.window_shot('command-search-on-home')
        self.c.xd('key', 'Escape')
        time.sleep(.3)
        self.c.cmd('file.home', open=False)
        time.sleep(.5)
        self.rpc.frame()
        self.expect('title buttons return after Home',
                    all('title/' + tool in self.controls() for tool in TOOLS))

    def left_panel_search(self):
        search = self.controls()['left/search']
        self.c.xd('mousemove', round(search['x'] + search['width'] / 2),
                  round(search['y'] + search['height'] / 2), 'click', 1)
        self.c.xd('key', 'ctrl+a', 'BackSpace')
        self.c.xd('type', '--clearmodifiers', 'edit.undo')
        count = len(self.c.scene()['objects'])
        revision = self.revision()
        self.c.xd('key', 'Return')
        time.sleep(.3)
        self.expect('left panel search no longer runs title bar commands',
                    self.revision() == revision and len(self.c.scene()['objects']) == count)
        self.c.xd('key', 'ctrl+a', 'BackSpace', 'Escape')
        time.sleep(.2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/meshcraft')
    args = parser.parse_args()
    REFERENCE.mkdir(parents=True, exist_ok=True)
    results = {}
    work = SOURCE / 'captures/work'
    with desktop(args.binary.resolve(), work / 'title-bar-1880', 120, True, size=(1880, 1060)) as (rpc, app, env):
        run = Run(rpc, env, REFERENCE)
        run.c.cmd('file.open_sample', sample='residence')
        time.sleep(1)
        rpc.frame()
        run.layout(1880)
        run.ribbon()
        run.clipboard_and_history()
        run.left_panel_search()
        run.command_search()
        run.modal()
        run.hidden_surfaces()
        run.home()
        run.recent()
        results.update(run.checks)
    with desktop(args.binary.resolve(), work / 'title-bar-900', 120, True, size=(900, 600)) as (rpc, app, env):
        run = Run(rpc, env, REFERENCE)
        run.c.cmd('file.open_sample', sample='residence')
        time.sleep(1)
        rpc.frame()
        run.layout(900)
        run.window_shot('minimum-window')
        long_path = work / 'title-bar-a-project-with-a-rather-long-file-name-for-truncation.octd'
        run.c.cmd('file.save_as', path=str(long_path))
        time.sleep(1)
        run.checks = {k + ' (short title)': v for k, v in run.checks.items()}
        run.layout(900)
        run.window_shot('minimum-window-long-path')
        results.update(run.checks)
    with desktop(args.binary.resolve(), work / 'title-bar-fresh', 120, True, size=(1880, 1060),
                 fresh_profile=True) as (rpc, app, env):
        run = Run(rpc, env, REFERENCE)
        rpc.frame()
        controls = run.controls()
        run.expect('fresh profile hides the menu bar', controls['ribbon/Home']['y'] < 50,
                   f"first ribbon tab at y={controls['ribbon/Home']['y']}")
        run.expect('fresh profile shows the ribbon and left panel',
                   'ribbon/Home' in controls and 'left/search' in controls)
        run.window_shot('fresh-profile')
        results.update(run.checks)
    (REFERENCE / 'native-validation.json').write_text(json.dumps(dict(
        version=run.c.version, checks=results, platform='Linux Xvfb/Mesa; actual GPUI app',
        notVerified=['window dragging beside the buttons (no window manager under Xvfb)',
                     'Windows and macOS title bars', 'native file dialogs'],
    ), indent=2) + '\n')
    failed = [name for name, passed in results.items() if not passed]
    print(f'{len(results) - len(failed)} of {len(results)} checks passed', flush=True)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
