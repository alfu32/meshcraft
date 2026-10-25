#!/usr/bin/env python3
"""Benchmark all registry tools in a private real desktop. Requires Linux Xvfb/xdotool."""

import argparse
from collections import Counter
import contextlib
import csv
import io
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

from cases import Exercise, recipe, BULK_COMMANDS, validate_observed
from fixtures import TIERS, generate
from report import summarize, write, CLIENT_STAGES
from runtime import Failure, desktop

ROOT = Path(__file__).resolve().parents[2]


def cpu_name():
    text = Path("/proc/cpuinfo").read_text() if Path("/proc/cpuinfo").exists() else ""
    return next(
        (
            line.split(":", 1)[1].strip()
            for line in text.splitlines()
            if line.startswith("model name")
        ),
        platform.processor(),
    )


def reset(rpc, env):
    subprocess.run(
        ["xdotool", "key", "Escape"], env=env, check=True, stdout=subprocess.DEVNULL
    )
    try:
        rpc.command("tool.cancel")
    except Failure as error:
        if "no active tool to cancel" not in str(error):
            raise
    if rpc.status()["editContext"]["kind"] != "document":
        rpc.command("selection.exit_context")
    for _ in range(100):
        status = rpc.status()
        if not status["history"]["cursor"]:
            break
        rpc.command("edit.undo")
    else:
        raise Failure("History reset exceeded 100 edits")
    rpc.select([])
    rpc.command(
        "ui.update_preferences",
        dict(
            menu_bar_visible=True,
            ribbon_visible=True,
            left_panel_visible=True,
            toolbars_visible=True,
            black_model_lines=True,
            ui_text_scale=1,
        ),
    )
    rpc.command(
        "selection.update_display",
        dict(kind="face", property="wireframe", enabled=False),
    )
    rpc.command("view.camera.perspective")
    rpc.command("view.frame_document")
    rpc.frame()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "target/stress-results")
    parser.add_argument("--tiers", nargs="+", choices=TIERS, default=list(TIERS))
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument(
        "--only",
        nargs="+",
        help="Development filter; reports explicitly record partial coverage",
    )
    parser.add_argument(
        "--hardware",
        action="store_true",
        help="Use available hardware GL in private Xvfb instead of forcing Mesa software",
    )
    parser.add_argument("--source-revision", default=None)
    parser.add_argument(
        "--artifact", type=Path, help="Verified release provenance from download.py"
    )
    args = parser.parse_args()
    if not 1 <= args.samples <= 100 or args.timeout <= 0:
        parser.error("positive timeout and 1..100 samples required")
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    supplied_binary = binary
    if not args.artifact:
        with binary.open("rb") as executable:
            if executable.read(4) != b"\x7fELF":
                parser.error(
                    "Local --binary must be a native ELF executable; for a release "
                    "launcher supply --artifact and --source-revision from download.py"
                )
        # A concurrent cargo build replaces target/debug/meshcraft. Freeze local
        # executables before command discovery so restarts cannot change builds
        # halfway through a tier. Release launchers keep their verified package.
        frozen = output / "work/executable/meshcraft"
        frozen.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, frozen)
        binary = frozen
    catalog = json.loads(
        subprocess.check_output([str(binary), "--command-catalog"], text=True)
    )
    inventory = {
        r["command-id"]: r
        for r in csv.DictReader(
            io.StringIO((ROOT / "docs/tools/tools.csv").read_text())
        )
    }
    commands = catalog["commands"]
    if {c["id"] for c in commands} != set(inventory):
        raise Failure("Binary registry and checked-out inventory differ")
    for c in commands:
        recipe(c)  # Fail closed when a new command lacks even a classified recipe.
    if args.only:
        unknown = set(args.only) - set(inventory)
        if unknown:
            parser.error(f"Unknown command filters: {unknown}")
        commands = [c for c in commands if c["id"] in args.only]
    unique_command_count = len(commands)
    commands = [dict(c, variant="standard") for c in commands] + [
        dict(c, variant="fragment") for c in commands if c["id"] in BULK_COMMANDS
    ]
    # Shutdown and document replacement are deliberately last so prior cases keep their model.
    commands.sort(
        key=lambda c: (
            c["id"] in ("app.quit",),
            c["id"] in ("file.new", "edit.reset_scene_for_capture"),
            c["id"],
        )
    )
    provenance = json.loads(args.artifact.read_text()) if args.artifact else None
    if provenance:
        if (
            provenance["tag"] != catalog["version"]
            or args.source_revision != provenance["source_revision"]
        ):
            raise Failure(
                "Released binary, selected source and benchmark version differ"
            )
        native = binary.parent / "bin/meshcraft"
        if (
            hashlib.sha256(binary.read_bytes()).hexdigest()
            != provenance["launcher_sha256"]
            or hashlib.sha256(native.read_bytes()).hexdigest()
            != provenance["binary_sha256"]
        ):
            raise Failure("Verified release executable changed before benchmark launch")
    report = dict(
        schema_version=1,
        version=catalog["version"],
        source_revision=args.source_revision
        or subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        binary_sha256=provenance["binary_sha256"]
        if provenance
        else hashlib.sha256(binary.read_bytes()).hexdigest(),
        supplied_binary=str(supplied_binary),
        release_artifact=provenance,
        source_state="verified released tag"
        if provenance
        else "local supplied binary; source revision is a working-tree base, not an exact release claim",
        started_at=datetime.now(timezone.utc).isoformat(),
        workflow_url=(
            f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}"
            f"/actions/runs/{os.environ['GITHUB_RUN_ID']}"
        )
        if os.environ.get("GITHUB_RUN_ID")
        else None,
        samples_per_case=args.samples,
        tiers=args.tiers,
        inventory_count=len(inventory),
        selected_command_count=unique_command_count,
        benchmark_case_count=len(commands),
        partial=bool(args.only or args.tiers != list(TIERS)),
        environment=dict(
            platform=platform.platform(),
            cpu=cpu_name(),
            cpu_count=os.cpu_count(),
            python=sys.version,
            rendering="hardware requested (actual GL renderer recorded in app log)"
            if args.hardware
            else "Mesa software OpenGL / private Xvfb",
            viewport_window=[1280, 900],
            autosave=False,
            antialiasing="Off",
            activity="dedicated CI tier worker"
            if provenance
            else "local development desktop; normal background activity",
            build="release archive"
            if provenance
            else "local supplied binary (not assumed release optimized)",
        ),
        fixtures=[],
        cases=[],
    )
    write(report, output)
    for tier in args.tiers:
        work = output / "work" / tier
        work.mkdir(parents=True, exist_ok=True)
        fixture = work / "fixture.octd"
        fixture_info = generate(ROOT, fixture, tier)
        report["fixtures"].append(fixture_info)
        stack = contextlib.ExitStack()
        rpc = app = env = None
        generation = None
        serial = 0

        def restart():
            nonlocal stack, rpc, app, env, generation, serial
            stack.close()
            stack = contextlib.ExitStack()
            serial += 1
            rpc, app, env = stack.enter_context(
                desktop(
                    binary, work / f"process-{serial}", args.timeout, not args.hardware
                )
            )
            report["environment"]["gpu_info"] = (
                work / f"process-{serial}" / "gpu.txt"
            ).read_text()
            if rpc.status()["version"] != catalog["version"]:
                raise Failure("Running binary version differs")
            rpc.command("file.open", dict(path=str(fixture)))
            generation = rpc.status()["generation"]
            live = next(
                o
                for o in rpc.call("meshcraft_scene", dict(limit=32))["objects"]
                if o["is_scene_root"]
            )["counts"]
            for kind in ("vertices", "segments", "triangles"):
                if live[kind] != fixture_info["background_" + kind]:
                    raise Failure(f"Live {tier} fixture {kind} differs: {live}")
            fixture_info["live_verified"] = True
            fixture_info["live_counts"] = live
            # Keep the resident background in view so the renderer sees the tier's workload.
            rpc.command("view.frame_document")
            rpc.frame()

        try:
            for command in commands:
                key = command["id"]
                r = recipe(command)
                row = dict(
                    command_id=key,
                    variant=command["variant"],
                    name=command["label"],
                    category=inventory[key]["category"],
                    tier=tier,
                    family=r.family,
                    operation=r.operation,
                    status="unsupported" if r.family == "unsupported" else "pending",
                    reason=r.operation if r.family == "unsupported" else "",
                    samples=[],
                    stats={s: None for s in summarize([])},
                )
                if r.family == "unsupported":
                    report["cases"].append(row)
                    write(report, output)
                    continue
                for sample in range(args.samples):
                    exercise = None
                    start = None
                    try:
                        if app is None or app.poll() is not None:
                            restart()
                        elif rpc.status()["generation"] != generation:
                            restart()
                        else:
                            reset(rpc, env)
                        exercise = Exercise(rpc, work, env)
                        exercise.variant = command["variant"]
                        exercise.catalog = catalog["commands"]
                        exercise.fixture = fixture
                        exercise.external = ROOT / "examples/dynamic-object.octd"
                        exercise.prepare(command)
                        before = rpc.status()
                        start = time.perf_counter_ns()
                        exercise.execute(command)
                        if key == "app.quit":
                            app.wait(timeout=args.timeout)
                            if app.returncode:
                                raise Failure(f"Quit returned {app.returncode}")
                            after = dict(
                                revision=before["revision"],
                                status="Application exited cleanly",
                            )
                        else:
                            frame = exercise.timed("frame_ready_ms", rpc.frame)
                            if frame["procedural"]["diagnostics"]:
                                raise Failure(str(frame["procedural"]["diagnostics"]))
                            after = rpc.status()
                        validate_observed(key, before, after)
                        exercise.timings["workflow_elapsed_ms"] = (
                            time.perf_counter_ns() - start
                        ) / 1e6
                        exercise.timings["total_ms"] = sum(
                            v for k, v in exercise.timings.items() if k in CLIENT_STAGES
                        )
                        row["samples"].append(
                            dict(
                                status="passed",
                                timings=exercise.timings,
                                steps=exercise.steps,
                                before_revision=before["revision"],
                                after_revision=after["revision"],
                                result=after.get("status"),
                                selected_entities=len(after.get("selection", [])),
                            )
                        )
                    except Exception as error:
                        row["samples"].append(
                            dict(
                                status="failed",
                                timings=exercise.timings if exercise else {},
                                failure_elapsed_ms=(time.perf_counter_ns() - start)
                                / 1e6
                                if start is not None
                                else None,
                                error=str(error),
                            )
                        )
                        row["reason"] = str(error)
                        if not isinstance(error, Failure):
                            stack.close()
                            app = None
                        break
                row["status"] = (
                    "passed"
                    if len(row["samples"]) == args.samples
                    and all(s["status"] == "passed" for s in row["samples"])
                    else "failed"
                )
                row["stats"] = summarize(row["samples"])
                report["cases"].append(row)
                write(report, output)
                total = row["stats"]["total_ms"]
                print(
                    f"{tier} {key} [{command['variant']}]: {row['status']}"
                    + (
                        f" {total['median']:.2f} ms"
                        if total
                        else " " + row["reason"][:300]
                    ),
                    flush=True,
                )
        finally:
            stack.close()
    report["finished_at"] = datetime.now(timezone.utc).isoformat()
    report["coverage_complete"] = len(report["cases"]) == len(commands) * len(
        args.tiers
    )
    write(report, output)
    print(dict(Counter(r["status"] for r in report["cases"])), flush=True)
    return (
        1
        if any(r["status"] not in ("passed", "unsupported") for r in report["cases"])
        or not report["coverage_complete"]
        else 0
    )


if __name__ == "__main__":
    raise SystemExit(main())
