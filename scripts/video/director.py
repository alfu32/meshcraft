"""Drive the real app over MCP at human pace and record it.

Everything is done through MCP's native pointer: hovering places the app's snapping 3D cursor, and
presses pick points, draw the selection window and drag handles, through the mouse's own handlers.
The build checks each snap before it is used, so a pick that misses its grid point or helper stops
the build instead of reaching a video. xdotool only moves the visible arrow along with MCP.
"""

import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/stress"))
sys.path.insert(0, str(ROOT / "scripts/video"))
import subtitles  # noqa: E402


NUDGES = [(dx, dy) for radius in (1, 2, 4, 7, 10)
          for dx, dy in ((radius, 0), (-radius, 0), (0, radius), (0, -radius),
                         (radius, radius), (-radius, radius), (radius, -radius), (-radius, -radius))]


class Director:
    def __init__(self, rpc, env, fast=False, voices=None):
        self.rpc, self.env, self.fast = rpc, env, fast
        self.voices = voices or {}
        self.video_crf = 23
        self.origin = None
        self.cues, self.clicks, self.titles, self.keys, self.snaps = [], [], [], [], []
        self.cue_ready_at = 0.0
        self.mouse = (960, 540)
        self.view = None

    # Time ---------------------------------------------------------------
    def clock(self):
        return time.time() - self.origin if self.origin else 0.0

    def wait(self, seconds):
        time.sleep(seconds * (0.02 if self.fast else 1))

    def hold(self):
        """Let the current cue finish being read before anything else starts."""
        if not self.fast:
            remaining = self.cue_ready_at - time.time()
            if remaining > 0:
                time.sleep(remaining)

    def say(self, text):
        self.hold()
        subtitles.wrap(text)
        now = self.clock()
        if self.cues and self.cues[-1]["end"] > now:
            self.cues[-1]["end"] = max(self.cues[-1]["start"], now - 0.05)
        # A generated clip sets the pace when there is one, so speech never overruns its action.
        key = subtitles.voice_key(text)
        said = self.voices.get(key, {}).get("seconds", 0.0)
        seconds = max(subtitles.reading_seconds(text), said + 0.35)
        # It stays up while its action plays out, until the next cue cuts it short.
        self.cues.append(dict(start=now, end=now + seconds + 9.0, text=text, budget=seconds,
                              voice=key if said else None))
        self.cue_ready_at = time.time() + seconds

    def finish(self, title, subtitle=None):
        """End on the finished model: a quiet moment, then a card that stays until the last frame."""
        self.rpc.select([])
        self.hold()
        self.wait(2.5)
        self.title(title, subtitle, seconds=5.0)

    def title(self, title, subtitle=None, seconds=3.6):
        self.hold()
        start = self.clock()
        if self.cues and self.cues[-1]["end"] > start:
            self.cues[-1]["end"] = max(self.cues[-1]["start"], start - 0.05)
        self.titles.append(dict(start=start, end=start + seconds, title=title, subtitle=subtitle))
        self.wait(seconds)

    # Cursor -------------------------------------------------------------
    def _xdo(self, *args):
        subprocess.run(["xdotool", *map(str, args)], env=self.env, check=True)

    def _native(self, action, x=None, y=None, **options):
        x, y = self.mouse if x is None else (x, y)
        view = self.view or self.rpc.status()["workspace"]["viewport"]
        self.view = view
        return self.rpc.call("k3d_pointer_event", dict(
            action=action, viewportX=x - view["x"], viewportY=y - view["y"], **options))

    def _inside(self, x, y):
        view = self.view or self.rpc.status()["workspace"]["viewport"]
        self.view = view
        return (view["x"] <= x < view["x"] + view["width"]
                and view["y"] <= y < view["y"] + view["height"])

    def glide(self, x, y, seconds=None, hover=True, **options):
        """Move the arrow and, inside the viewport, the app's 3D cursor along with it."""
        start_x, start_y = self.mouse
        distance = math.hypot(x - start_x, y - start_y)
        seconds = seconds or min(1.3, 0.35 + distance / 1500)
        if not self.fast and distance > 1:
            begin = time.monotonic()
            while True:
                t = min(1.0, (time.monotonic() - begin) / seconds)
                eased = t * t * (3 - 2 * t)
                here = (round(start_x + (x - start_x) * eased), round(start_y + (y - start_y) * eased))
                self._xdo("mousemove", *here)
                if hover and self._inside(*here):
                    self._native("hover", *here, **options)
                if t >= 1:
                    break
                time.sleep(0.012)
        self._xdo("mousemove", round(x), round(y))
        self.mouse = (round(x), round(y))

    def _ring(self):
        self.clicks.append(dict(t=self.clock(), x=self.mouse[0], y=self.mouse[1]))

    def project(self, point):
        status = self.rpc.status()
        camera, view = status["camera"], status["workspace"]["viewport"]
        position = [camera["position"][k] for k in "xyz"]
        target = [camera["target"][k] for k in "xyz"]
        forward = _unit(_sub(target, position))
        right = _unit(_cross(forward, (0.0, 1.0, 0.0)))
        up = _cross(right, forward)
        relative = _sub(point, position)
        depth = _dot(relative, forward)
        if depth <= 1e-6:
            raise RuntimeError(f"{point} is behind the camera")
        tangent = math.tan(math.radians(camera["fieldOfView"]) / 2)
        aspect = view["width"] / view["height"]
        scale = camera["distance"] if camera["orthographic"] else depth
        x = _dot(relative, right) / (scale * tangent * aspect)
        y = _dot(relative, up) / (scale * tangent)
        return (view["x"] + (x + 1) / 2 * view["width"], view["y"] + (1 - y) / 2 * view["height"])

    # App ----------------------------------------------------------------
    def control(self, key):
        deadline = time.monotonic() + 5
        while True:
            controls = self.rpc.status()["workspace"]["controls"]
            if key in controls:
                break
            if time.monotonic() > deadline:
                raise RuntimeError(f"No visible control {key}")
            time.sleep(0.03)
        box = controls[key]
        return box["x"] + box["width"] / 2, box["y"] + box["height"] / 2

    def tab(self, name):
        if self.rpc.status()["workspace"]["ribbonTab"] == name:
            return
        self.glide(*self.control("ribbon/" + name))
        self.wait(0.15)
        self._ring()
        self._xdo("click", 1)
        deadline = time.monotonic() + 10
        while self.rpc.status()["workspace"]["ribbonTab"] != name:
            if time.monotonic() > deadline:
                raise RuntimeError(f"Ribbon tab {name} did not open")
            time.sleep(0.03)
        self.wait(0.35)

    def tool(self, command, tab="Home", **arguments):
        """Travel to the real ribbon button, then run it over MCP."""
        self.tab(tab)
        self.glide(*self.control(f"tool/{tab}/{command}"))
        self.wait(0.25)
        self._ring()
        result = self.rpc.command(command, arguments)
        self.wait(0.4)
        return result

    def command(self, command, **arguments):
        return self.rpc.command(command, arguments)

    def aim(self, point, kinds=None, dwell=0.75, **options):
        """Place the 3D cursor over a world point; it must snap exactly there, as that kind."""
        x, y = self.project(point)
        if not self._inside(x, y):
            raise RuntimeError(f"{point} is off the viewport at ({x:.0f}, {y:.0f}); reframe the camera")
        self.glide(x, y, **options)
        accepted = lambda s: s and _near(s["point"], point) and (not kinds or s["kind"] in kinds)
        snap = self._native("hover", **options).get("cursorSnap")
        # Close snap points compete, so nudge by a few pixels as a person would.
        for dx, dy in NUDGES:
            if accepted(snap):
                break
            self.mouse = (round(x + dx), round(y + dy))
            self._xdo("mousemove", *self.mouse)
            snap = self._native("hover", **options).get("cursorSnap")
        if not accepted(snap):
            raise RuntimeError(f"Cursor at {point} snapped to {snap}, wanted {kinds or 'any kind'}")
        if os.environ.get("TUTORIAL_TRACE"):
            print("aim", point, snap["kind"], snap["normal"], flush=True)
        self.snaps.append(dict(start=self.clock(), end=self.clock() + dwell + 0.6,
                               x=self.mouse[0], y=self.mouse[1], text=snap["kind"]))
        # Previews trail the cursor under software rendering; let them land before acting.
        self.wait(dwell)
        return snap

    def point(self, point, seconds=None):
        """Aim somewhere without needing a snap, for a direction that a typed value will finish."""
        self.glide(*self.project(point), seconds=seconds)
        self.wait(0.5)

    def _press(self, **options):
        self._ring()
        self._native("down", **options)
        self.wait(0.08)
        self._native("up")

    def pick(self, point, kinds=None, dwell=0.45):
        snap = self.aim(point, kinds)
        self._press()
        self.wait(dwell)
        return snap

    def click(self, point, count=1, add=False):
        """With Select active: click what is under the cursor, and require it to end up selected.

        Three clicks select connected geometry. add holds Shift to extend the selection.
        """
        if not add:
            # Clicking what is already selected would toggle it off.
            self.rpc.select([])
        before = len(self.selected() or [])
        self.glide(*self.project(point))
        self._native("hover")
        self.wait(0.3)
        options = dict(shift=True) if add else {}
        for _ in range(count):
            self._press(**options)
            self.wait(0.09)
        self.wait(0.4)
        self.require(len(self.selected() or []) > before, f"click at {point} selected nothing new")

    def selected(self):
        selection = self.rpc.status()["selection"]
        return selection if isinstance(selection, list) else selection.get("targets", selection.get("count"))

    def window(self, corners, margin=26):
        """Drag the selection window around world points, left to right so it selects what is inside."""
        pixels = [self.project(p) for p in corners]
        left, right = min(x for x, _ in pixels) - margin, max(x for x, _ in pixels) + margin
        top, bottom = min(y for _, y in pixels) - margin, max(y for _, y in pixels) + margin
        self.require(self._inside(left, top) and self._inside(right, bottom), "window fits the viewport")
        self.glide(left, top)
        self.wait(0.2)
        self._native("down")
        self.glide(right, bottom, seconds=1.1)
        self.wait(0.35)
        self._native("up")
        self.wait(0.5)
        self.require(self.selected(), "the selection window selected nothing")

    def helper(self, point, kinds=None, key="T", command="view.axis_guide"):
        """Place a guide at the 3D cursor, so later picks can snap to it."""
        before = self.rpc.status()["constructionGuides"]
        self.aim(point, kinds)
        start = self.clock()
        self.rpc.command(command, {})
        self.wait(0.45)
        self._key(key, start)
        self.require(self.rpc.status()["constructionGuides"] == before + 1,
                     f"{key} placed no guide at {point}")
        self.wait(0.5)

    def grid_helper_on_face(self, point, normal):
        """Place a grid guide on a face at a snapped grid point; its plane then carries snap points."""
        snap = self.aim(point, ("Grid", "Face"))
        self.require(_near(snap["normal"], normal), f"helper wanted the face {normal}, got {snap}")
        start = self.clock()
        before = self.rpc.status()["constructionGuides"]
        self.rpc.command("view.grid_guide", {})
        self.wait(0.45)
        self._key("G", start)
        self.require(self.rpc.status()["constructionGuides"] == before + 1, "G placed no guide")
        self.wait(0.5)
        return snap

    def units(self, name, size, grid):
        """Model Settings first: the suffix, the unit's length, and the snap spacing."""
        self.command("view.model_settings")
        self.wait(0.4)
        self.field("Unit name", name)
        self.field("Unit size", f"{size:g}")
        self.field("Grid size", f"{grid:g}")
        settings = self.scene()["settings"]
        self.require(settings["unit_name"] == name and settings["unit_size"] == size
                     and settings["grid_size"] == grid, f"units set: {settings}")

    def drag(self, start, end, kinds=None):
        """A native drag from one snap point to another: down, hover along the way, up."""
        self.aim(start, dwell=0.3)
        self._ring()
        self._native("down")
        self.wait(0.15)
        snap = self.aim(end, kinds)
        self._native("up")
        self.wait(0.5)
        return snap

    def field(self, label, text):
        """Type into a real settings field, then leave it so the value commits."""
        # Settings use real X input; retain human delays even during a dry run.
        pause = time.sleep
        pause(.3)
        self.glide(*self.control("setting-input/" + label))
        pause(0.2)
        self._ring()
        self._xdo("click", 1)
        pause(0.25)
        self._xdo("key", "ctrl+a")
        pause(0.15)
        for character in text:
            self._xdo("type", character)
            pause(0.14)
        pause(0.3)
        self._xdo("key", "Tab")
        pause(0.4)

    def _key(self, text, start):
        self.keys.append(dict(start=start, end=self.clock() + 0.5, text=text))

    def type(self, text):
        start = self.clock()
        for character in text:
            self.rpc.call("k3d_pointer_event", dict(action="text", text=character))
            self.wait(0.16)
        self.wait(0.45)
        self._key(text, start)

    def enter(self):
        start = self.clock()
        self.rpc.pointer("finish")
        self.wait(0.45)
        self._key("Enter", start)

    def escape(self):
        start = self.clock()
        self.rpc.pointer("cancel")
        self.wait(0.45)
        self._key("Esc", start)

    def select(self, targets, around=None):
        """Selection goes through MCP; the cursor still travels to what is being selected."""
        if around is not None:
            self.glide(*self.project(around))
            self.wait(0.15)
            self._ring()
        self.rpc.select(targets)
        self.wait(0.4)

    def orbit(self, dx, dy, seconds=2.4):
        """A real right-drag, so the viewer sees the gesture the cue teaches."""
        status = self.rpc.status()["workspace"]["viewport"]
        x, y = status["x"] + status["width"] * 0.62, status["y"] + status["height"] * 0.55
        self.glide(x, y)
        self._xdo("mousedown", 3)
        # A real button press: an MCP hover in between would report no button and end the orbit.
        self.glide(x + dx, y + dy, seconds=0.2 if self.fast else seconds, hover=False)
        self._xdo("mouseup", 3)
        # The camera moved, so put the 3D cursor back under the arrow.
        self._native("hover")
        self.wait(0.35)

    def pose(self, position, target, fov=36):
        self.command("view.camera.set_pose", position=list(position), target=list(target),
                     field_of_view=fov)

    def wheel(self, x, y, button, clicks):
        """Real wheel input: 4 zooms in, 5 zooms out."""
        self.glide(x, y)
        self.wait(0.2)
        self._xdo("click", "--repeat", clicks, "--delay", 70, button)
        self.wait(0.5)

    def click_mark(self, x, y):
        """Travel to a button the app does not measure and mark the click; MCP does the work."""
        self.glide(x, y)
        self.wait(0.25)
        self._ring()

    def close_modal(self):
        """Dismiss a finished editor without teaching a keystroke the viewer should not use."""
        self._xdo("key", "Escape")
        self.wait(0.4)

    def key(self, name):
        start = self.clock()
        self._xdo("key", name)
        self.wait(0.4)
        self._key(name, start)

    def fly(self, position, target, seconds=2.5, fov=36):
        """Move the camera smoothly to a new view, then put the 3D cursor back under the arrow."""
        camera = self.rpc.status()["camera"]
        start = ([camera["position"][k] for k in "xyz"], [camera["target"][k] for k in "xyz"])
        if not self.fast:
            begin = time.monotonic()
            while True:
                t = min(1.0, (time.monotonic() - begin) / seconds)
                eased = t * t * (3 - 2 * t)
                self.pose([a + (b - a) * eased for a, b in zip(start[0], position)],
                          [a + (b - a) * eased for a, b in zip(start[1], target)], fov)
                if t >= 1:
                    break
                time.sleep(0.03)
        self.pose(position, target, fov)
        self.wait(0.3)

    def scene(self):
        return self.rpc.call("meshcraft_scene")

    def root(self):
        return next(o for o in self.scene()["objects"] if o["is_scene_root"])

    def primitives(self, path, kinds=("segment", "triangle", "vertex")):
        offset, found = 0, []
        while offset is not None:
            page = self.rpc.call("meshcraft_geometry", dict(path=path, offset=offset, limit=1000))
            found += [p["target"] for p in page["primitives"] if p["target"]["kind"] in kinds]
            offset = page.get("nextOffset")
        return found

    def require(self, condition, message):
        if not condition:
            raise RuntimeError("Tutorial check failed: " + message)

    def finish_checks(self):
        status = self.rpc.status()
        self.require(not status["scriptErrors"], f"script errors {status['scriptErrors']}")

    def save(self, path):
        self.command("file.save_as", path=str(path))
        self.require(not self.rpc.status()["unsavedChanges"], "project did not save")


class Recorder:
    def __init__(self, env, path, log, size=(1920, 1080)):
        self.path, self.log = path, log
        self.process = subprocess.Popen(
            ["ffmpeg", "-y", "-hide_banner", "-f", "x11grab", "-draw_mouse", "1",
             "-framerate", "30", "-video_size", f"{size[0]}x{size[1]}", "-i", env["DISPLAY"],
             "-c:v", "libx264", "-preset", "ultrafast", "-crf", "10", "-pix_fmt", "yuv444p",
             str(path)],
            env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=log.open("w"),
        )

    def origin(self):
        """Wall-clock time of the first captured frame, as ffmpeg reports it."""
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            found = re.search(r"start: (\d+\.\d+)", self.log.read_text())
            if found:
                value = float(found.group(1))
                if abs(value - time.time()) > 60:
                    raise RuntimeError(f"Capture clock is not wall time: {value}")
                return value
            time.sleep(0.05)
        raise RuntimeError(f"ffmpeg did not start: {self.log.read_text()[-400:]}")

    def stop(self):
        self.process.communicate(b"q", timeout=120)
        if self.process.returncode != 0:
            raise RuntimeError(f"Capture failed: {self.log.read_text()[-400:]}")


def media_duration(path):
    return float(subprocess.check_output(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        text=True))


def render(raw, script, subtitle_track, output, duration, crf=23):
    """Rings, keycaps and title cards go in the picture; subtitles stay a selectable track."""
    filter_path = str(script).replace("\\", "\\\\").replace(":", "\\:").replace("'", "\\'")
    subprocess.run(
        ["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", str(raw), "-i", str(subtitle_track),
         "-map", "0:v", "-map", "1:s", "-t", f"{duration:.3f}",
         "-vf", f"ass='{filter_path}',format=yuv420p", "-c:v", "libx264", "-preset", "slow",
         "-crf", str(crf), "-r", "30", "-g", "150", "-c:s", "mov_text",
         "-metadata:s:s:0", "language=eng", "-disposition:s:0", "default",
         "-movflags", "+faststart", "-an", str(output)],
        check=True,
    )


def write_outputs(director, raw, directory, name, viewport, recipe, version):
    cues = [dict(start=round(c["start"], 3), end=round(c["end"], 3), text=c["text"],
                 reading_seconds=round(c["budget"], 3), voice=c["voice"]) for c in director.cues]
    problems = subtitles.audit(cues)
    if problems:
        raise RuntimeError("Subtitle audit failed:\n" + "\n".join(problems))
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{name}.srt").write_text(subtitles.srt(cues))
    (directory / f"{name}.vtt").write_text(subtitles.vtt(cues))
    script = raw.with_suffix(".ass")
    script.write_text(subtitles.overlay(director.clicks, director.titles, viewport, director.keys, director.snaps))
    raw.with_suffix(".timeline.json").write_text(json.dumps(
        dict(clicks=director.clicks, keys=director.keys, titles=director.titles, snaps=director.snaps), indent=1))
    duration = director.clock()
    render(raw, script, directory / f"{name}.srt", directory / f"{name}.mp4", duration,
           crf=director.video_crf)
    duration = media_duration(directory / f"{name}.mp4")
    (directory / f"{name}.cues.json").write_text(json.dumps(dict(
        video=f"{name}.mp4", app_version=version, recipe=recipe, duration=round(duration, 3),
        chars_per_second=subtitles.CHARS_PER_SECOND, cues=cues), indent=2) + "\n")


def _near(a, b, tolerance=1e-4):
    return all(abs(a[k] - v) < tolerance for k, v in zip("xyz", b))


def _sub(a, b):
    return [p - q for p, q in zip(a, b)]


def _dot(a, b):
    return sum(p * q for p, q in zip(a, b))


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _unit(a):
    length = math.sqrt(_dot(a, a))
    return [p / length for p in a]
