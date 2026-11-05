#!/usr/bin/env python3
"""Build tutorial videos locally: python3 scripts/video/build.py [--only 01] [--dry-run]"""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from director import Director, Recorder, ROOT, write_outputs  # noqa: E402
from runtime import desktop  # noqa: E402

TUTORIALS = ROOT / "docs/tutorials"
WORK = ROOT / "target/tutorial-work"
LINES = WORK / "lines"


def load(path):
    spec = importlib.util.spec_from_file_location(path.stem.replace("-", "_"), path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build(path, binary, dry_run):
    name = path.stem
    recipe = load(path)
    work = WORK / name
    shutil.rmtree(work, ignore_errors=True)
    os.environ.update(XCURSOR_THEME="DMZ-White", XCURSOR_SIZE="24", XCURSOR_PATH="/usr/share/icons")
    with desktop(binary, work, 120, True, size=(1920, 1080), fresh_profile=True) as (rpc, _, env):
        index = TUTORIALS / "voice/index.json"
        voices = json.loads(index.read_text()) if index.exists() else {}
        director = Director(rpc, env, fast=dry_run, voices=voices)
        recipe.setup(director)
        time.sleep(0.5 if dry_run else 1.5)
        # Cursor names the theme lacks fall through to the root cursor, an X by default.
        subprocess.run(["xsetroot", "-cursor_name", "left_ptr"], env=env, check=True)
        director.glide(960, 620, seconds=0.01)
        time.sleep(0.5)
        status = rpc.status()
        viewport, version = status["workspace"]["viewport"], status["version"]
        recorder = None
        if not dry_run:
            recorder = Recorder(env, work / "raw.mkv", work / "capture.log")
            director.origin = recorder.origin()
        try:
            recipe.record(director)
            director.finish_checks()
            project = TUTORIALS / "projects" / f"{name}.octd"
            project.parent.mkdir(parents=True, exist_ok=True)
            if not dry_run:
                time.sleep(1.0)
                duration_end = director.clock()
        finally:
            if recorder:
                recorder.stop()
        director.save(project if not dry_run else work / "dry-run.octd")
    work.mkdir(parents=True, exist_ok=True)
    LINES.mkdir(parents=True, exist_ok=True)
    (LINES / f"{name}.json").write_text(json.dumps([c["text"] for c in director.cues], indent=1))
    if not dry_run:
        write_outputs(director, work / "raw.mkv", TUTORIALS / "videos", name, viewport,
                      str(path.relative_to(ROOT)), version)
        print(f"{name}: {duration_end:.1f}s, {len(director.cues)} cues, "
              f"{(TUTORIALS / 'videos' / (name + '.mp4')).stat().st_size / 1e6:.1f} MB", flush=True)
    else:
        print(f"{name}: dry run passed, {len(director.cues)} cues", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/meshcraft")
    parser.add_argument("--only", help="recipe name prefix")
    parser.add_argument("--dry-run", action="store_true", help="run recipe checks without recording")
    args = parser.parse_args()
    recipes = sorted((TUTORIALS / "recipes").glob("[0-9]*.py"))
    if args.only:
        recipes = [r for r in recipes if r.stem.startswith(args.only)]
    if not recipes:
        parser.error("no matching recipes")
    for path in recipes:
        build(path, args.binary.resolve(), args.dry_run)


if __name__ == "__main__":
    main()
