#!/usr/bin/env python3
"""Reopen tutorial 7, check its live geometry and capture the saved office model."""

import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts/video"))
from director import Director
from runtime import desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/meshcraft")
    parser.add_argument("--project", type=Path, default=ROOT / "docs/tutorials/projects/07-office-floor.octd")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/tutorials/review")
    parser.add_argument("--checkpoint", action="store_true", help="also replay the structural stage into its source model")
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("office_recipe", ROOT / "docs/tutorials/recipes/07-office-floor.py")
    recipe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(recipe)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="meshcraft-tutorial-") as temporary:
        neutral = Path(temporary) / "office-floor.octd"
        shutil.copyfile(args.project, neutral)
        with desktop(args.binary.resolve(), ROOT / "target/tutorial-work/office-model-checks",
                     120, True, size=(1920, 1080), fresh_profile=True) as (rpc, _, env):
            d = Director(rpc, env, fast=True)
            d.command("file.open", path=str(neutral))
            d.command("ui.update_preferences", accordion_settings_panels=True, show_permanent_grid=False)
            d.command("selection.update_display", kind="object", property="display", enabled=True)
            objects = d.scene()["objects"]
            frame = next(o for o in objects if o["name"] == "Structure 6 m bay")
            facades = [o for o in objects if o["name"] == "Curtain wall 1.5 m panel"]
            panel = next(o for o in objects if o["name"] == "Ceiling panel 600 mm")
            d.require(len(objects) == 5 and len(facades) == 2, "four component instances plus the scene root")
            for obj, triangles in [(frame, 2016), (panel, 80)] + [(o, 728) for o in facades]:
                d.require(recipe.settled(d, obj["path"], triangles)["counts"]["triangles"] == triangles,
                          f"saved {obj['name']} regenerates {triangles} triangles")
            page = recipe.geometry(d, frame["path"])[0]
            handle = page["hotspots"][0]
            step = {axis: handle["position"][axis] - handle["referencePosition"][axis] for axis in "xyz"}
            d.require(all(abs(step[k] - v) < 1e-6 for k, v in dict(x=20, y=12, z=20).items()),
                      "saved volumetric reference preserves all three steps")
            counts = Counter(o["target"]["semantic_kind"] for o in recipe.owners(d))
            d.require(counts == dict(wall=29, frame=12, stair=4, slab=3), "saved core, doors, stairs and slabs")
            saved_annotations = recipe.geometry(d, d.root()["path"])[0]["annotations"]
            d.require(len(saved_annotations) == 31, "saved grid and room labels")
            settings = d.scene()["settings"]
            d.require(settings["unit_size"] == 30 and settings["grid_size"] == .25, "saved physical scale")
            d.command("view.selection")

            def shot(name, position, target):
                d.pose(position, target)
                d.escape()
                d.glide(960, 17, hover=False)
                rpc.frame()
                time.sleep(.6)
                subprocess.run(["import", "-window", "root", str(output / name)], env=env, check=True)

            shot("07-office-floor-preview.png", (180, 180, 190), (70, 3, 30))
            d.command("selection.update_display", kind="object", property="display", enabled=False)
            shot("07-office-floor-preview-core.png", (80, 180, 30.5), (80, 0, 30))
            report = dict(app_version=rpc.status()["version"], source_model=args.project.name,
                          sha256=hashlib.sha256(args.project.read_bytes()).hexdigest(),
                          architecture=dict(counts), annotations=31, frame_triangles=2016,
                          facade_triangles=[728, 728], ceiling_triangles=80, volumetric_step=step,
                          method="Saved model reopened in the native app on private Linux Xvfb/Mesa", passed=True)
            (output / "07-office-floor-model-checks.json").write_text(json.dumps(report, indent=2) + "\n")
            if args.checkpoint:
                d.command("selection.update_display", kind="object", property="display", enabled=True)
                recipe.setup(d)
                recipe.structure(d)
                d.save(ROOT / "docs/tutorials/projects/07-office-floor-structure.octd")
            print(json.dumps(report, indent=2), flush=True)


if __name__ == "__main__":
    main()
