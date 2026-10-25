#!/usr/bin/env python3
"""Diagnose saved CI AppImages without rebuilding or claiming release qualification."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import shlex
import signal
import subprocess


def run_case(name, command, env, output, timeout=12):
    with (output / (name + ".log")).open("w") as log:
        process = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
            return dict(case=name, exit=code)
        except subprocess.TimeoutExpired:
            return dict(case=name, exit=None, observation="still running at diagnostic deadline")
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    images = list(args.input.resolve().glob("*.AppImage"))
    if len(images) != 1:
        raise RuntimeError("Exactly one saved AppImage is required")
    image = images[0]
    expected = Path(str(image) + ".sha256").read_text().split()
    actual = hashlib.sha256(image.read_bytes()).hexdigest()
    if expected != [actual, image.name]:
        raise RuntimeError("Saved AppImage checksum differs")
    image.chmod(0o755)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = dict(qualification=False, appimage=image.name, sha256=actual, cases=[])
    with (output / "system.log").open("w") as log:
        subprocess.run(["uname", "-a"], stdout=log, stderr=log, check=True)
        subprocess.run(["dpkg-query", "-W"], stdout=log, stderr=log, check=True)
    extract = output / "extracted"
    extract.mkdir()
    with (output / "extraction.log").open("w") as log:
        subprocess.run([str(image), "--appimage-extract"], cwd=extract, stdout=log, stderr=log, check=True)
    app = extract / "squashfs-root"
    read, write = os.pipe()
    with (output / "xvfb.log").open("w") as log:
        server = subprocess.Popen(["Xvfb", "-displayfd", str(write), "-screen", "0", "1280x900x24", "-ac", "-noreset"], pass_fds=(write,), stdout=log, stderr=log)
        os.close(write)
        try:
            if not select.select([read], [], [], 20)[0]:
                raise RuntimeError("Private Xvfb did not start")
            env = dict(os.environ)
            for key in ["WAYLAND_DISPLAY", "VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "MESHCRAFT_GRAPHICS_RETRY", "LIBGL_ALWAYS_SOFTWARE", "APPIMAGE_EXTRACT_AND_RUN"]:
                env.pop(key, None)
            env.update(DISPLAY=":" + os.read(read, 32).decode().strip(), XDG_CONFIG_HOME=str(output / "config"), XDG_CACHE_HOME=str(output / "cache"))
            sandbox = ["firejail", "--noprofile", "--net=none"]
            cases = [
                ("appimage-sandbox", sandbox + ["--appimage", str(image)]),
                ("extracted-direct", [str(app / "AppRun")]),
                ("extracted-sandbox", sandbox + [str(app / "AppRun")]),
            ]
            for name, command in cases:
                result = run_case(name, command, env, output)
                report["cases"].append(result)
                print(json.dumps(result), flush=True)
            drivers = sorted(Path("/usr/share/vulkan/icd.d").glob("lvp_icd*.json"))
            if not drivers:
                raise RuntimeError("Mesa software driver is required for native diagnosis")
            env.update(VK_DRIVER_FILES=str(drivers[0]), VK_ICD_FILENAMES=str(drivers[0]), LIBGL_ALWAYS_SOFTWARE="1")
            # Preserve the audited launcher environment, but insert GDB immediately
            # around the real executable. This extracted tree is diagnostic only.
            binary = app / "usr/lib/meshcraft/bin/meshcraft"
            original = binary.with_name("meshcraft-native")
            binary.rename(original)
            debugger = ["timeout", "--signal=INT", "--kill-after=10", "20", "gdb", "--batch", "-ex", "set pagination off", "-ex", "run", "-ex", "thread apply all bt", "-ex", "info sharedlibrary", "--args", str(original)]
            binary.write_text("#!/bin/sh\nexec " + shlex.join(debugger) + ' "$@"\n')
            binary.chmod(0o755)
            for name, command in [
                ("gdb-direct", [str(app / "AppRun")]),
                ("gdb-sandbox", sandbox + ["--allow-debuggers", str(app / "AppRun")]),
            ]:
                result = run_case(name, command, env, output, timeout=40)
                report["cases"].append(result)
                print(json.dumps(result), flush=True)
        finally:
            os.close(read)
            server.terminate()
            server.wait(timeout=5)
            (output / "diagnosis.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
