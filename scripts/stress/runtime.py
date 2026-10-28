"""Isolated real desktop process and MCP requests. No user's desktop/config is reused."""

import contextlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.error
import urllib.request


class Failure(RuntimeError):
    pass


class Rpc:
    def __init__(self, port, timeout):
        self.url = f"http://127.0.0.1:{port}/mcp"
        self.timeout = timeout
        self.owner_ms_total = 0.0
        self.owner_count = 0

    def call(self, name, args=None):
        payload = dict(
            jsonrpc="2.0",
            id=1,
            method="tools/call",
            params=dict(name=name, arguments=args or {}),
        )
        req = urllib.request.Request(
            self.url,
            json.dumps(payload).encode(),
            {
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
            },
        )
        with urllib.request.urlopen(req, timeout=self.timeout) as response:
            body = json.load(response)
        if "error" in body:
            raise Failure(str(body["error"]))
        result = body["result"]
        if result.get("isError"):
            raise Failure(
                "; ".join(c.get("text", "") for c in result.get("content", []))
            )
        value = result.get("structuredContent") or json.loads(
            result["content"][0]["text"]
        )
        owner = (
            value.get("_timing", {}).get("ownerThreadMs")
            if isinstance(value, dict)
            else None
        )
        if owner is not None:
            self.owner_ms_total += owner
            self.owner_count += 1
        return value

    def status(self):
        return self.call("k3d_status")

    def command(self, key, args=None):
        result = self.call(
            "k3d_execute_command", dict(commandId=key, arguments=args or {})
        )
        dispatch = result["dispatch"]
        completion = dispatch["result"]["completion"]
        if result.get("status", {}).get("pendingDialog"):
            self.call("meshcraft_resolve_document", dict(action="discard"))
        deadline = time.monotonic() + self.timeout
        while completion["state"] == "pending":
            job = self.call("meshcraft_get_job", dict(job_id=completion["job_id"]))
            completion = job["result"]["completion"]
            if time.monotonic() > deadline:
                raise Failure(f"{key}: job timeout")
            if completion["state"] == "pending":
                time.sleep(0.01)
        dispatch["result"] = job["result"] if "job" in locals() else dispatch["result"]
        if completion["state"] == "failed":
            raise Failure(str(result))
        return result

    def pointer(self, action, p=None):
        args = dict(action=action)
        if p is not None:
            args.update(zip(("worldX", "worldY", "worldZ"), p))
        return self.call("k3d_pointer_event", args)

    def select(self, targets):
        return self.call("meshcraft_select", dict(targets=targets))

    def frame(self, preview=False):
        deadline = time.monotonic() + self.timeout
        while True:
            try:
                result = self.call("meshcraft_capture_view", dict(max_width=64, preview=preview))
                if not result["fresh"]:
                    raise Failure("stale viewport frame")
                return result
            except Failure as error:
                if "retry capture" not in str(error) or time.monotonic() >= deadline:
                    raise
                time.sleep(0.01)


@contextlib.contextmanager
def desktop(binary, directory, timeout, software, size=(1280, 900)):
    width, height = size
    if not (640 <= width <= 7680 and 480 <= height <= 4320):
        raise ValueError("Private desktop size is outside the supported capture range")
    directory.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env.update(
        XDG_CONFIG_HOME=str(directory / "config"),
        XDG_CACHE_HOME=str(directory / "cache"),
        MESHCRAFT_DISABLE_AUTOSAVE="1",
        RUST_BACKTRACE="1",
    )
    config = directory / "config/meshcraft"
    config.mkdir(parents=True, exist_ok=True)
    (config / "preferences.json").write_text(
        json.dumps(
            dict(
                menu_bar_visible=True,
                ribbon_visible=True,
                left_panel_visible=True,
                panels_visible=True,
                toolbars_visible=True,
                ui_text_scale=1,
                toolbar_icon_size=24,
                accordion_settings_panels=True,
                show_permanent_grid=False,
                antialiasing_quality=0,
            )
        )
    )
    # Xvfb allocates an unused display; no fixed display or user process can be replaced.
    display_read, display_write = os.pipe()
    xlog = (directory / "xvfb.log").open("w")
    server = subprocess.Popen(
        [
            "Xvfb",
            "-displayfd",
            str(display_write),
            "-screen",
            "0",
            f"{width}x{height}x24",
            "-ac",
        ],
        pass_fds=(display_write,),
        stdout=xlog,
        stderr=xlog,
    )
    os.close(display_write)
    app = None
    log = (directory / "app.log").open("w")
    try:
        import select

        if not select.select([display_read], [], [], timeout)[0]:
            raise Failure("Xvfb did not allocate a private display")
        env["DISPLAY"] = ":" + os.read(display_read, 32).decode().strip()
        if software:
            env["LIBGL_ALWAYS_SOFTWARE"] = "1"
            drivers = sorted(Path("/usr/share/vulkan/icd.d").glob("lvp_icd*.json"))
            if not drivers:
                raise Failure("Mesa lavapipe Vulkan ICD is required for GPUI in Xvfb")
            env["VK_ICD_FILENAMES"] = env["VK_DRIVER_FILES"] = str(drivers[0])
        with (directory / "gpu.txt").open("w") as gpu:
            subprocess.run(
                ["glxinfo", "-B"], env=env, stdout=gpu, stderr=gpu, check=True
            )
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        app = subprocess.Popen(
            [str(binary), "--size", f"{width}x{height}", "--mcp-port", str(port)],
            env=env,
            stdout=log,
            stderr=log,
        )
        rpc = Rpc(port, timeout)
        deadline = time.monotonic() + timeout
        while True:
            if app.poll() is not None:
                raise Failure(
                    f"Desktop exited ({app.returncode}); see {directory / 'app.log'}"
                )
            try:
                rpc.status()
                break
            except (OSError, Failure):
                if time.monotonic() >= deadline:
                    raise Failure("MCP startup timeout")
                time.sleep(0.1)
        window = subprocess.check_output(
            ["xdotool", "search", "--name", "^Meshcraft$"], env=env, text=True
        ).splitlines()[-1]
        for action in ("windowmap", "windowraise", "windowfocus"):
            subprocess.run(["xdotool", action, window], env=env, check=True)
        yield rpc, app, env
    finally:
        if app is not None and app.poll() is None:
            app.terminate()
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                app.kill()
                app.wait()
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait()
        os.close(display_read)
        log.close()
        xlog.close()
