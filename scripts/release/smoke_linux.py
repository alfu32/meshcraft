#!/usr/bin/env python3
"""Verify the actual AppImage opens offline in Xvfb, including its GPU-error UI."""

import argparse
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--appimage", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    executable = args.appimage.resolve()
    read, write = os.pipe()
    results = []
    with (output / "xvfb.log").open("w") as log:
        # Polling tools are short-lived X11 clients. Without -noreset, their
        # disconnects can reset the server underneath startup and driver probes.
        server = subprocess.Popen(["Xvfb", "-displayfd", str(write), "-screen", "0", "1280x900x24", "-ac", "-noreset"], pass_fds=(write,), stdout=log, stderr=log)
        os.close(write)
        try:
            if not select.select([read], [], [], 20)[0]:
                raise RuntimeError("Private Xvfb did not start")
            display = ":" + os.read(read, 32).decode().strip()
            for name, title in [("offline", "^Meshcraft$"), ("driver-error", "^Meshcraft - Graphics initialization error$")]:
                directory = output / name
                directory.mkdir(exist_ok=True)
                env = dict(os.environ)
                for key in ["WAYLAND_DISPLAY", "VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "MESHCRAFT_GRAPHICS_RETRY", "LIBGL_ALWAYS_SOFTWARE", "APPIMAGE_EXTRACT_AND_RUN"]:
                    env.pop(key, None)
                env.update(DISPLAY=display, XDG_CONFIG_HOME=str(directory / "config"), XDG_CACHE_HOME=str(directory / "cache"))
                if name == "driver-error":
                    env.update(VK_DRIVER_FILES="/nonexistent/meshcraft-test-driver.json", VK_ICD_FILENAMES="/nonexistent/meshcraft-test-driver.json")
                with (directory / "app.log").open("w") as app_log:
                    # Keep sandbox diagnostics: startup failures must identify
                    # whether Firejail, the runtime, or Meshcraft failed.
                    command = ["firejail", "--noprofile", "--net=none", "--appimage", str(executable)]
                    if name == "offline":
                        for label, prefix in [("runtime", [str(executable), "--appimage-extract-and-run"]), ("sandbox", command)]:
                            with (directory / (label + "-version.log")).open("w") as preflight:
                                probe = subprocess.run(prefix + ["--version"], env=env, stdout=preflight, stderr=preflight, timeout=30)
                            if probe.returncode:
                                raise RuntimeError(f"{label} --version failed: {probe.returncode}; see {directory}")
                    process = subprocess.Popen(command, env=env, stdout=app_log, stderr=app_log, start_new_session=True)
                    try:
                        deadline = time.monotonic() + 30
                        window = None
                        while time.monotonic() < deadline and process.poll() is None:
                            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", title], env=env, text=True, capture_output=True)
                            if found.returncode == 0:
                                window = found.stdout.splitlines()[-1]
                                break
                            time.sleep(.2)
                        if window is None:
                            raise RuntimeError(f"{name}: no expected window; exit={process.poll()}; see {directory / 'app.log'}")
                        for action in ["windowmap", "windowraise", "windowfocus"]:
                            subprocess.run(["xdotool", action, window], env=env, check=True)
                        screenshot = directory / "window.png"
                        colors = 0
                        while time.monotonic() < deadline:
                            subprocess.run(["import", "-display", display, "-window", window, str(screenshot)], check=True)
                            colors = int(subprocess.check_output(["identify", "-format", "%k", str(screenshot)], text=True))
                            if colors > (8 if name == "offline" else 1):
                                break
                            time.sleep(.2)
                        if colors <= (8 if name == "offline" else 1):
                            raise RuntimeError(f"{name}: expected window never painted its contents")
                        if name == "driver-error":
                            subprocess.run(["xdotool", "mousemove", "--window", window, "20", "20", "click", "1"], env=env, check=True)
                            if process.wait(timeout=10) != 1:
                                raise RuntimeError("Missing-driver startup must exit unsuccessfully after acknowledgement")
                        result = dict(case=name, network="disabled by Firejail", colors=colors, window=window, outcome="passed")
                        results.append(result)
                        print(json.dumps(result), flush=True)
                    finally:
                        try:
                            os.killpg(process.pid, signal.SIGTERM)
                        except ProcessLookupError:
                            pass
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
        finally:
            os.close(read)
            server.terminate()
            server.wait(timeout=5)
            (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
