#!/usr/bin/env python3
"""Always publish a detailed release report, with explicit diagnostics if incomplete."""

import argparse
from collections import Counter
import csv
from datetime import datetime, timezone
import io
import json
import os
from pathlib import Path
import zipfile

from merge import merge
from publish import ROOT, upload, validate
from report import write


def prepare(directory, output, tag, revision, inventory, workflow_url=None):
    output.mkdir(parents=True, exist_ok=True)
    paths = sorted(directory.glob("performance-linux-x86_64-*/performance.json"))
    workers, errors = [], []
    for path in paths:
        try:
            workers.append(json.loads(path.read_text()))
        except (ValueError, OSError) as error:
            errors.append(f"{path.relative_to(directory)}: {error}")
    try:
        if errors:
            raise ValueError("; ".join(errors))
        result = merge(workers)
        validate(result, tag, revision, inventory)
        result.setdefault("qualification_notes", []).append(
            "Native logs, runner output and original tier reports are available in `performance-diagnostics.zip` on this release. Tool failures and performance measurements do not gate workflow success."
        )
        write(result, output)
    except (ValueError, KeyError, TypeError, AttributeError, IndexError) as error:
        # Preserve raw observations without promoting an interrupted/filtered or
        # unverified dataset to a complete, validated performance measurement.
        result = dict(
            schema_version=1,
            report_kind="incomplete_diagnostics",
            version=tag,
            source_revision=revision,
            coverage_complete=False,
            measurements_validated=False,
            generated_at=datetime.now(timezone.utc).isoformat(),
            workflow_url=workflow_url,
            error=str(error),
            unvalidated_worker_reports=workers,
            unreadable_reports=errors,
        )
        (output / "performance.json").write_text(json.dumps(result, indent=2) + "\n")
        lines = [
            f"# Meshcraft performance — {tag}",
            "",
            "**Incomplete run — diagnostic report.** A complete, validated measurement set is unavailable. No missing or rejected observation is counted as a successful measurement.",
            "",
            f"Source: `{revision}`",
            "",
            f"Report assembly diagnostic: {error}",
            "",
        ]
        if workflow_url:
            lines += [f"[Workflow and job logs]({workflow_url})", ""]
        lines += [
            "All available raw tier reports, runner output and native logs are retained in `performance-diagnostics.zip` on this release. JSON embeds readable worker reports as **unvalidated observations**; CSV lists their reported outcomes without claiming validated latency statistics.",
            "",
            "## Available worker observations (unvalidated)",
            "",
        ]
        stream = io.StringIO(newline="")
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["tier", "command_id", "reported_status", "reason"])
        for worker in workers:
            if not isinstance(worker, dict):
                lines += ["- Worker report has an invalid document shape."]
                continue
            cases = worker.get("cases", [])
            if not isinstance(cases, list):
                lines += ["- Worker report has an invalid case list."]
                continue
            cases = [case for case in cases if isinstance(case, dict)]
            counts = dict(Counter(str(c.get("status", "unknown")) for c in cases))
            lines += [
                f"- Tiers {worker.get('tiers', [])}: {len(cases)} observations; {counts}."
            ]
            for case in cases:
                writer.writerow(
                    [
                        case.get("tier", ""),
                        case.get("command_id", ""),
                        case.get("status", "unknown"),
                        case.get("reason", ""),
                    ]
                )
                if case.get("status") != "passed":
                    reason = str(case.get("reason", "")).replace("\n", " ")[:1200]
                    lines.append(
                        f"  - `{case.get('command_id', 'unknown')}`: {case.get('status', 'unknown')} — {reason}"
                    )
        if not workers:
            lines += [
                "No readable worker reports were produced; no operations are reported as measured."
            ]
        lines += ["", *errors, ""]
        (output / "performance.md").write_text("\n".join(lines))
        (output / "performance.csv").write_text(stream.getvalue())

    archive = output / "performance-diagnostics.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        bundle.writestr(
            "run.json",
            json.dumps(
                dict(
                    tag=tag,
                    source_revision=revision,
                    workflow_url=workflow_url,
                    coverage_complete=result.get("coverage_complete", False),
                ),
                indent=2,
            ),
        )
        for path in sorted(directory.rglob("*")):
            if path.is_file() and not path.is_symlink():
                bundle.write(path, str(Path("workers") / path.relative_to(directory)))
    return [output / f"performance.{suffix}" for suffix in ("md", "csv", "json")] + [
        archive
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args()
    inventory = {
        r["command-id"]: r["availability"]
        for r in csv.DictReader(
            io.StringIO((ROOT / "docs/tools/tools.csv").read_text())
        )
    }
    workflow_url = (
        f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}"
        if os.environ.get("GITHUB_RUN_ID")
        else None
    )
    files = prepare(
        args.input, args.output, args.tag, args.source_revision, inventory, workflow_url
    )
    upload(args.repo, args.tag, files)


if __name__ == "__main__":
    main()
