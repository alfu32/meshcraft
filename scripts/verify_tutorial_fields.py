#!/usr/bin/env python3
"""Verify native settings entry at measured editor centres across ribbon changes."""

import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts/video"))
from director import Director
from runtime import desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/meshcraft")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/rewrite/references/tutorial-settings")
    parser.add_argument("--label", choices=("before", "after"), default="after")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cases = []
    with desktop(args.binary.resolve(), ROOT / "target/tutorial-work/settings-field-regression",
                 120, True, size=(1920, 1080), fresh_profile=True) as (rpc, _, env):
        d = Director(rpc, env, fast=True)
        d.command("ui.update_preferences", accordion_settings_panels=True)
        d.command("file.new", preset="architecture_metric")
        version = rpc.status()["version"]
        for cycle in range(4):
            for value in ("0.25", "0.5"):
                d.command("view.model_settings")
                d.field("Grid size", value)
                settings = d.scene()["settings"]
                actual = settings["grid_size"]
                cases.append(dict(cycle=cycle + 1, requested=float(value), actual=actual,
                                  unit_size=settings["unit_size"],
                                  passed=abs(actual - float(value)) < 1e-9 and settings["unit_size"] == 30,
                                  container=rpc.status()["workspace"]["controls"].get("setting/Grid size"),
                                  editor=rpc.status()["workspace"]["controls"].get("setting-input/Grid size")))
                (output / f"{args.label}.json").write_text(json.dumps(dict(
                    version=version, cases=cases, complete=len(cases) == 8,
                    passed=len(cases) == 8 and all(c["passed"] for c in cases)), indent=2) + "\n")
                if cycle == 0 or not cases[-1]["passed"]:
                    rpc.frame()
                    subprocess.run(["import", "-window", "root", str(output / f"{args.label}-{value}.png")],
                                   env=env, check=True)
                d.require(cases[-1]["passed"], f"native Grid size entry: wanted {value}, got {actual}")
                d.tool("tool.cuboid", tab="Solids")
                d.require(d.scene()["settings"]["grid_size"] == actual,
                          "grid value survives leaving the inspector")
                d.escape()
        d.require(d.scene()["settings"]["unit_size"] == 30, "Architecture scale preserved")
    print(f"{len(cases)} native grid edits passed, including ribbon/focus changes", flush=True)


if __name__ == "__main__":
    main()
