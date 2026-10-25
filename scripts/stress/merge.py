#!/usr/bin/env python3
"""Merge four separately isolated CI tier runs without hiding hardware or failures."""

import argparse
import copy
import json
from pathlib import Path
from report import write

TIERS = ["small", "medium", "high", "heavy"]


def merge(reports):
    if len(reports) != 4:
        raise ValueError("Exactly four tier reports are required")
    indexed = {}
    identity = (
        "version",
        "source_revision",
        "binary_sha256",
        "samples_per_case",
        "inventory_count",
        "selected_command_count",
        "release_artifact",
    )
    for report in reports:
        if len(report["tiers"]) != 1 or report["tiers"][0] not in TIERS:
            raise ValueError("Expected a single known tier per worker")
        tier = report["tiers"][0]
        if tier in indexed:
            raise ValueError("Duplicate tier report")
        if report["selected_command_count"] != report["inventory_count"]:
            raise ValueError(
                "Command-filtered reports cannot be merged as complete coverage"
            )
        if not report.get("coverage_complete"):
            raise ValueError("Incomplete worker coverage")
        for key in identity:
            if report[key] != reports[0][key]:
                raise ValueError("Worker identity differs: " + key)
        indexed[tier] = report
    if set(indexed) != set(TIERS):
        raise ValueError("Missing tier report")
    result = copy.deepcopy(indexed["small"])
    result.update(
        tiers=TIERS,
        partial=False,
        fixtures=[],
        cases=[],
        environments={},
        started_at=min(r["started_at"] for r in reports),
        finished_at=max(r["finished_at"] for r in reports),
    )
    for tier in TIERS:
        report = indexed[tier]
        result["fixtures"].extend(report["fixtures"])
        result["cases"].extend(report["cases"])
        result["environments"][tier] = report["environment"]
    result["environment"] = dict(
        platform="Four dedicated Ubuntu runners, one per model tier"
        if result["release_artifact"]
        else indexed["small"]["environment"]["platform"],
        cpu="See per-tier environments in JSON",
        rendering="See per-tier renderer identities in JSON",
        build="verified release archive"
        if result["release_artifact"]
        else "local development executable, frozen before testing",
        activity="dedicated CI tier workers"
        if result["release_artifact"]
        else "four local private desktop processes measured concurrently; shared host CPU/GPU and normal desktop load",
    )
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--input", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    paths = sorted(a.input.glob("performance-linux-x86_64-*/performance.json"))
    write(merge([json.loads(p.read_text()) for p in paths]), a.output)


if __name__ == "__main__":
    main()
