#!/usr/bin/env python3
"""Publish complete honest coverage, including failures, to its exact existing release."""

import argparse
import csv
import io
import json
from pathlib import Path
import subprocess
from cases import BULK_COMMANDS

ROOT = Path(__file__).resolve().parents[2]


def validate(report, tag, revision, inventory):
    if report["version"] != tag or report["source_revision"] != revision:
        raise ValueError("Report tag/source mismatch")
    artifact = report.get("release_artifact")
    if (
        not artifact
        or artifact["tag"] != tag
        or artifact["source_revision"] != revision
        or artifact["binary_sha256"] != report["binary_sha256"]
    ):
        raise ValueError("Report is not from the verified same-tag release binary")
    if report.get("partial") or report["tiers"] != ["small", "medium", "high", "heavy"]:
        raise ValueError("Partial report cannot replace the release benchmark")
    fixtures = report.get("fixtures", [])
    if (
        len(fixtures) != 4
        or {f["tier"] for f in fixtures} != set(report["tiers"])
        or any(not f.get("live_verified") for f in fixtures)
    ):
        raise ValueError("Every tier requires verified live fixture counts")
    pairs = [
        (c["command_id"], c["tier"], c.get("variant", "standard"))
        for c in report["cases"]
    ]
    expected = {
        (key, tier, variant)
        for key in inventory
        for tier in report["tiers"]
        for variant in (
            ["standard", "fragment"] if key in BULK_COMMANDS else ["standard"]
        )
    }
    if len(pairs) != len(set(pairs)) or set(pairs) != expected:
        raise ValueError("Missing or duplicate tool/tier coverage")
    if any(
        c["status"] not in ("passed", "failed", "unsupported") for c in report["cases"]
    ):
        raise ValueError("Pending cases cannot be published")
    for c in report["cases"]:
        if (
            isinstance(inventory, dict)
            and c["status"] == "unsupported"
            and inventory[c["command_id"]] != "deferred"
        ):
            raise ValueError("Registered command cannot be hidden as unsupported")
        if c["status"] == "passed" and (
            len(c["samples"]) != report["samples_per_case"]
            or any(s["status"] != "passed" for s in c["samples"])
        ):
            raise ValueError("Passing case lacks complete successful samples")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--repo", required=True)
    p.add_argument("--tag", required=True)
    p.add_argument("--source-revision", required=True)
    p.add_argument("--directory", required=True, type=Path)
    a = p.parse_args()
    report = json.loads((a.directory / "performance.json").read_text())
    inventory = {
        r["command-id"]: r["availability"]
        for r in csv.DictReader(
            io.StringIO((ROOT / "docs/tools/tools.csv").read_text())
        )
    }
    validate(report, a.tag, a.source_revision, inventory)
    files = [
        a.directory / name
        for name in ("performance.md", "performance.csv", "performance.json")
    ]
    if not all(p.is_file() and p.stat().st_size for p in files):
        raise ValueError("Report asset is missing")
    subprocess.run(
        [
            "gh",
            "release",
            "upload",
            a.tag,
            "--repo",
            a.repo,
            "--clobber",
            *map(str, files),
        ],
        check=True,
    )


if __name__ == "__main__":
    main()
