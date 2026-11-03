#!/usr/bin/env python3
"""Drive the Home screen in the actual native app: startup, cards, recents and samples.

File dialogs need a desktop portal, so Open and Import are exercised through their
command paths here; every other interaction is a real pointer or key event.
With --thumbnails, regenerates assets/samples/*.png from the real renderer instead.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts/stress"))
sys.path.insert(0, str(ROOT / "scripts"))
from prepare_samples import SAMPLES  # noqa: E402
from runtime import desktop  # noqa: E402

REF = ROOT / "docs/rewrite/references/home-screen"
SAMPLE_TARGET = ROOT / "assets/samples"
WIDTH, HEIGHT = 1600, 1000
SIDEBAR = "srgb(29,85,134)"
PROBE = (126, 700)
HOME_BUTTON = (53, 17)
PRESETS = {
    "urban_planning": ((440, 266), "m", 1.0),
    "architecture_metric": ((750, 266), "cm", 30.0),
    "architecture_imperial": ((1060, 266), "ft", 1.0),
    "product_metric": ((1370, 266), "cm", 1.0),
    "product_imperial": ((440, 358), "in", 1.0),
}
FIRST_RECENT = (900, 495)
SECOND_SAMPLE_WITH_RECENT = (640, 680)


def capture_png(rpc, path, max_width=1024):
    payload = dict(jsonrpc="2.0", id=1, method="tools/call", params=dict(
        name="meshcraft_capture_view", arguments=dict(max_width=max_width)))
    request = urllib.request.Request(rpc.url, json.dumps(payload).encode(), {
        "Content-Type": "application/json", "Accept": "application/json"})
    with urllib.request.urlopen(request, timeout=rpc.timeout) as response:
        body = json.load(response)
    image = next(c for c in body["result"]["content"] if c.get("type") == "image")
    path.write_bytes(base64.b64decode(image["data"]))


def wait_for(check, what, timeout=90):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise RuntimeError("Timed out waiting for " + what)
        time.sleep(.05)


class Session:
    def __init__(self, env):
        self.env = env
        self.results = {}

    def shot(self, name, settle=1.0):
        time.sleep(settle)
        path = REF / (name + ".png")
        subprocess.run(["import", "-window", "root", str(path)], env=self.env, check=True)
        return path

    def click(self, position):
        x, y = position
        subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"],
                       env=self.env, check=True)

    def key(self, *names):
        for name in names:
            subprocess.run(["xdotool", "key", name], env=self.env, check=True)
            time.sleep(.12)

    def home_visible(self):
        path = Path(tempfile.mkstemp(suffix=".png")[1])
        try:
            time.sleep(.5)
            subprocess.run(["import", "-window", "root", str(path)], env=self.env, check=True)
            pixel = subprocess.check_output(
                ["convert", str(path), "-format", f"%[pixel:p{{{PROBE[0]},{PROBE[1]}}}]", "info:"],
                text=True).strip()
        finally:
            path.unlink()
        return pixel == SIDEBAR

    def expect(self, name, condition, detail=None):
        self.results[name] = dict(passed=bool(condition), detail=detail)
        print(("PASS " if condition else "FAIL ") + name, detail if detail is not None else "")


def settings(rpc):
    return rpc.call("meshcraft_scene", dict(limit=1))["settings"]


def thumbnails(binary, work):
    with desktop(binary, work / "thumbnails", 180, True, size=(WIDTH, HEIGHT)) as (rpc, _, _env):
        for name in SAMPLES:
            before = rpc.status()["generation"]
            rpc.call("k3d_execute_command", dict(commandId="file.open_sample",
                                                 arguments=dict(sample=name)))
            wait_for(lambda: rpc.status()["generation"] != before, name)
            time.sleep(4)
            rpc.frame()
            raw = work / f"{name}-raw.png"
            capture_png(rpc, raw, 1280)
            subprocess.run(["convert", str(raw), "-resize", "408x254^", "-gravity", "center",
                            "-extent", "408x254", "-strip", "-define", "png:compression-level=9",
                            str(SAMPLE_TARGET / f"{name}.png")], check=True)
            print(name, (SAMPLE_TARGET / f"{name}.png").stat().st_size)


def automation_run(binary, work):
    with desktop(binary, work / "mcp", 120, True, size=(WIDTH, HEIGHT)) as (rpc, app, env):
        s = Session(env)
        s.expect("automation launch skips Home", not s.home_visible())
        rpc.command("file.home", dict(open=True))
        s.expect("file.home opens Home", s.home_visible())
        rpc.frame()
        s.expect("renderer still answers while Home is open", True)
        s.shot("home-no-recents")

        for preset, (position, name, size) in PRESETS.items():
            rpc.command("file.home", dict(open=True))
            s.home_visible()
            s.click(position)
            wait_for(lambda: settings(rpc)["unit_name"] == name, preset)
            value = settings(rpc)
            s.expect(f"preset {preset}", value["unit_name"] == name
                     and value["unit_size"] == size and not s.home_visible(),
                     f'1 unit = {value["unit_size"]} {value["unit_name"]}')

        rpc.command("geometry.add_cuboid", dict(dimensions=[2, 2, 2], origin=[0, 0, 0]))
        edited = rpc.status()["revision"]
        rpc.command("file.home", dict(open=True))
        s.home_visible()
        s.click(PRESETS["urban_planning"][0])
        wait_for(lambda: rpc.status()["pendingDialog"], "unsaved prompt")
        s.shot("home-unsaved-prompt")
        rpc.call("meshcraft_resolve_document", dict(action="cancel"))
        status = rpc.status()
        s.expect("unsaved edits prompt and Cancel keeps them",
                 status["revision"] == edited and status["unsavedChanges"] and s.home_visible())
        s.key("Escape")
        s.expect("Escape returns to the workspace", not s.home_visible())

        saved = work / "recent-project.octd"
        rpc.command("file.save_as", dict(path=str(saved)))
        rpc.command("file.new")
        rpc.command("file.home", dict(open=True))
        s.home_visible()
        s.shot("home-with-recent")
        s.click(FIRST_RECENT)
        wait_for(lambda: rpc.status()["projectPath"] == str(saved), "recent project")
        s.expect("Recent row opens the project", not s.home_visible())

        rpc.command("file.new")
        rpc.command("file.home", dict(open=True))
        s.home_visible()
        # One Recent row: Tab through 5 presets, Open, Import and the row to the first sample.
        s.key(*["Tab"] * 8)
        s.shot("home-keyboard-selection")
        s.key("l")
        s.expect("tool hotkeys stay inactive on Home", rpc.status()["activeTool"] == "Select")
        before = rpc.status()["generation"]
        s.key("Return")
        wait_for(lambda: rpc.status()["generation"] != before, "keyboard sample")
        wait_for(lambda: rpc.status()["status"].startswith("Opened sample"), "sample status")
        status = rpc.status()
        scene = rpc.call("meshcraft_scene", dict(limit=1))
        s.expect("Tab reaches the first sample and Enter opens it unbound and clean",
                 "Instrument Controller" in status["status"] and status["projectPath"] is None
                 and not status["unsavedChanges"] and scene["prototypes"] and not s.home_visible(),
                 status["status"])
        s.expect("sample opens at its authored viewpoint",
                 max(abs(v) for v in status["camera"]["position"].values()) > 50,
                 status["camera"]["position"])
        s.shot("sample-opened", 3)

        rpc.command("file.new")
        rpc.command("file.home", dict(open=True))
        s.home_visible()
        s.click(SECOND_SAMPLE_WITH_RECENT)
        wait_for(lambda: rpc.status()["status"].startswith("Opened sample"), "clicked sample")
        s.expect("clicking a card opens that sample", "Fitted Kitchen" in rpc.status()["status"],
                 rpc.status()["status"])

        mesh = work / "triangle.obj"
        mesh.write_text("v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 2 3\n")
        rpc.command("file.home", dict(open=True))
        result = rpc.command("import.mesh", dict(path=str(mesh), new_document=True))
        status = rpc.status()
        s.expect("Import starts a new project",
                 status["projectPath"] is None and not s.home_visible()
                 and rpc.call("meshcraft_scene", dict(limit=1))["total"] >= 1,
                 result["dispatch"]["result"]["completion"]["state"])

        # Preset units are stored in the project, not only in the session.
        rpc.command("file.new", dict(preset="architecture_metric"))
        units = work / "units.octd"
        rpc.command("file.save_as", dict(path=str(units)))
        rpc.command("file.new")
        s.expect("new document is back to generic units", settings(rpc)["unit_name"] == "unit")
        rpc.command("file.open", dict(path=str(units)))
        wait_for(lambda: rpc.status()["projectPath"] == str(units), "reopen")
        value = settings(rpc)
        s.expect("preset units survive save and reopen",
                 (value["unit_name"], value["unit_size"]) == ("cm", 30.0),
                 f'1 unit = {value["unit_size"]} {value["unit_name"]}')

        # The most recent file disappears: the row says so and a click does nothing.
        rpc.command("file.new")
        units.unlink()
        rpc.command("file.home", dict(open=True))
        s.home_visible()
        s.shot("home-missing-recent")
        generation = rpc.status()["generation"]
        s.click(FIRST_RECENT)
        time.sleep(1)
        status = rpc.status()
        s.expect("a missing Recent file cannot be activated",
                 status["generation"] == generation and status["projectPath"] is None
                 and s.home_visible())

        app.terminate()
        app.wait(timeout=20)
        return s.results


def startup_run(binary, work):
    """A plain launch has no automation server, so it observes the user-visible path."""
    directory = work / "startup"
    with desktop(binary, directory, 120, True, size=(WIDTH, HEIGHT)) as (_rpc, app, env):
        app.terminate()
        app.wait(timeout=20)
        s = Session(env)
        env = dict(env)
        # A real session: previews of saved projects are enabled.
        env.pop("MESHCRAFT_DISABLE_AUTOSAVE", None)
        s.env = env
        project = work / "preview-project.octd"
        shutil.copy(ROOT / "examples/dynamic-object.octd", project)

        def launch(*args):
            log = (directory / "plain-app.log").open("a")
            process = subprocess.Popen([str(binary), "--size", f"{WIDTH}x{HEIGHT}", *args],
                                       env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 60
            while subprocess.run(["xdotool", "search", "--name", "^Meshcraft [0-9]"], env=env,
                                 capture_output=True).returncode:
                if time.monotonic() > deadline:
                    raise RuntimeError("Window did not appear")
                time.sleep(.2)
            window = subprocess.check_output(
                ["xdotool", "search", "--name", "^Meshcraft [0-9]"], env=env, text=True
            ).splitlines()[-1]
            # Xvfb has no window manager to map and focus the window for us.
            for action in ("windowmap", "windowraise", "windowfocus"):
                subprocess.run(["xdotool", action, window], env=env, check=True)
            time.sleep(3)
            return process

        def stop(process):
            process.terminate()
            process.wait(timeout=20)
            wait_for(lambda: subprocess.run(["xdotool", "search", "--name", "^Meshcraft [0-9]"],
                                            env=env, capture_output=True).returncode, "exit", 30)

        process = launch()
        s.expect("plain launch starts on Home", s.home_visible())
        s.shot("home-at-startup")
        s.key("Escape")
        s.expect("Escape leaves startup Home", not s.home_visible())
        s.click(HOME_BUTTON)
        s.expect("title-bar button reopens Home", s.home_visible())
        s.key("Right", "Return")
        s.expect("arrow and Enter create a project", not s.home_visible())
        s.shot("architecture-metric-project")
        stop(process)

        process = launch(str(project))
        s.expect("launching with a project skips Home", not s.home_visible())
        time.sleep(4)
        stop(process)
        previews = list((directory / "config/meshcraft/recent-thumbnails").glob("*.png"))
        s.expect("saved project gets a Recent preview", len(previews) == 1,
                 [p.name for p in previews])

        process = launch()
        s.expect("next launch lists the project", s.home_visible())
        s.shot("home-recent-preview")
        s.click(FIRST_RECENT)
        s.expect("Recent row opens from Home", not s.home_visible())
        stop(process)
        return s.results


def small_window(binary, work):
    with desktop(binary, work / "small", 120, True, size=(1000, 640)) as (rpc, _app, env):
        s = Session(env)
        rpc.command("file.home", dict(open=True))
        s.shot("home-small-window")
        s.key(*["Tab"] * 15)
        s.shot("home-small-window-scrolled")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/meshcraft")
    parser.add_argument("--thumbnails", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve()
    with tempfile.TemporaryDirectory(prefix="meshcraft-home-") as temporary:
        work = Path(temporary)
        if args.thumbnails:
            thumbnails(binary, work)
            return 0
        REF.mkdir(parents=True, exist_ok=True)
        results = {**automation_run(binary, work), **startup_run(binary, work)}
        small_window(binary, work)
    version = subprocess.check_output([str(binary), "--version"], text=True).strip()
    (REF / "native-validation.json").write_text(json.dumps(
        dict(version=version, method="Real pointer and key events in private Xvfb; see script",
             results=results), indent=2) + "\n")
    failed = [name for name, result in results.items() if not result["passed"]]
    print(f"{len(results) - len(failed)} of {len(results)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
