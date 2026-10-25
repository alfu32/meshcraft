"""Publish the same complete coverage ledger as JSON, CSV and readable Markdown."""

import csv
import io
import json
import math
import statistics
from collections import Counter

CLIENT_STAGES = ("start_ms", "input_ms", "preview_ms", "commit_ms", "frame_ready_ms")
STAGES = (
    "start_ms",
    "input_ms",
    "preview_ms",
    "commit_ms",
    "frame_ready_ms",
    "total_ms",
    "mcp_owner_thread_ms",
    "render_frame_ms",
)


def percentile(values, p):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * p) - 1)] if values else None


def summarize(samples):
    stages = {}
    for stage in STAGES:
        values = [
            s["timings"][stage]
            for s in samples
            if s["status"] == "passed" and stage in s["timings"]
        ]
        stages[stage] = (
            dict(
                median=statistics.median(values),
                p95=percentile(values, 0.95),
                maximum=max(values),
            )
            if values
            else None
        )
    return stages


def render(report):
    counts = Counter(r["status"] for r in report["cases"])
    lines = [
        f"# Meshcraft performance — {report['version']}",
        "",
        *(
            [f"[Workflow and diagnostic artifacts]({report['workflow_url']})", ""]
            if report.get("workflow_url")
            else []
        ),
        f"Source revision / working-tree base: `{report.get('source_revision', 'unknown')}` · Binary SHA-256: `{report['binary_sha256']}`",
        "",
        f"Environment: {report['environment']['platform']} · {report['environment']['cpu']} · {report['environment']['rendering']}",
        f"Provenance: {report.get('source_state', 'see raw report')} · Build: {report['environment'].get('build', 'see per-tier environments')} · Activity: {report['environment'].get('activity', 'see per-tier environments')}",
        f"Samples per tool/tier: **{report['samples_per_case']}** · Started: {report['started_at']}",
        "",
        f"Coverage: **{len(report['cases'])} cases** — "
        + ", ".join(f"{v} {k}" for k, v in sorted(counts.items())),
        "",
        "## What is measured",
        "",
        *[note + "\n" for note in report.get("qualification_notes", [])],
        "Real desktop operations through MCP, including world-point input, phantom feedback, committed edits, asynchronous jobs and fresh GPU viewport readiness. Timings use a monotonic client clock and include localhost transport/main-thread scheduling. Selection uses exact entity IDs; these numbers do not measure screen hit-testing or mouse snapping. Session commands measure their intended state transition, not construction geometry. Deferred commands are explicitly unsupported, never successful zero-ms tools.",
        "",
        "Setup, fixture loading, prerequisite selection, correctness queries, undo/reset and cleanup are excluded. `total` sums the timed operation stages and wait for a fresh viewport (untimed prerequisite/correctness calls are excluded); it includes the small PNG capture used as a readiness fence, not a GPUI window-paint acknowledgement or GPU-only timer. `commit` includes asynchronous completion where applicable. Initial/final point input is distinguished from preview. Autosave is disabled only in the private benchmark process. Fixed-size operation inputs run against increasingly large resident unique meshes; this is not a claim about transforming all background faces. Supplemental fragment probes for Select, Move and Rotate grow the selection to 24 / 192 / 1,536 / 6,144 primitives across the tiers, and operate on primitive IDs in prototype edit mode.",
        "",
        "Hardware and software runs must be compared separately. These are observations, not portable throughput guarantees. No threshold is inferred from a different GPU or CI runner. All raw samples, steps and failure diagnostics are in performance.json; CSV contains every case including failures/unsupported entries.",
        "",
        "## Model tiers",
        "",
        "| Tier | Stored background vertices | Segments | Triangles | Rendered primitives |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for f in report["fixtures"]:
        lines.append(
            f"| {f['tier']} | {f['background_vertices']:,} | {f['background_segments']:,} | {f['background_triangles']:,} | {f['background_primitives']:,} |"
        )
    lines += [
        "",
        "Workpieces add two shared cube placements and small contour/profile/path objects. Fixture SHA-256 hashes, exact bytes and workload details are recorded in JSON.",
        "",
        "## Slowest completed operations",
        "",
        "| Tier | Tool / command | Total median (ms) | Total p95 (ms) | Preview median (ms) | Commit median (ms) | MCP owner median (ms) | Viewport frame median (ms) |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    completed = [
        r for r in report["cases"] if r["status"] == "passed" and r["stats"]["total_ms"]
    ]

    def number(value):
        return f"{value['median']:.3f}" if value else "—"

    for r in sorted(
        completed, key=lambda x: x["stats"]["total_ms"]["median"], reverse=True
    )[:30]:
        lines.append(
            f"| {r['tier']} | {r['name']} (`{r['command_id']}`, {r.get('variant', 'standard')}) | {number(r['stats']['total_ms'])} | {r['stats']['total_ms']['p95']:.3f} | {number(r['stats']['preview_ms'])} | {number(r['stats']['commit_ms'])} | {number(r['stats'].get('mcp_owner_thread_ms'))} | {number(r['stats'].get('render_frame_ms'))} |"
        )
    lines += [
        "",
        "## Complete tool coverage",
        "",
        "Each row below represents an inventory command. Cells show **total median ms**, `unsupported`, or the failure state. Every listed tier must have a result; missing recipes and missing cells fail the runner.",
        "",
        "| Command | " + " | ".join(report["tiers"]) + " |",
        "| --- | " + " | ".join("---:" for _ in report["tiers"]) + " |",
    ]
    lookup = {
        (r["command_id"], r.get("variant", "standard"), r["tier"]): r
        for r in report["cases"]
    }
    for key, variant in sorted(
        {(r["command_id"], r.get("variant", "standard")) for r in report["cases"]}
    ):
        values = []
        for tier in report["tiers"]:
            r = lookup.get((key, variant, tier))
            values.append(
                number(r["stats"]["total_ms"])
                if r and r["status"] == "passed"
                else r["status"]
                if r
                else "**MISSING**"
            )
        lines.append("| `" + key + "` / " + variant + " | " + " | ".join(values) + " |")
    failures = [
        r for r in report["cases"] if r["status"] not in ("passed", "unsupported")
    ]
    if failures:
        lines += ["", "## Failures", ""]
        for r in failures:
            lines.append(
                f"- `{r['command_id']}` / {r['tier']}: {r.get('reason', '').replace(chr(10), ' ')[:1200]}"
            )
    return "\n".join(lines) + "\n"


def write(report, directory):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "performance.json").write_text(
        json.dumps(report, indent=2, allow_nan=False) + "\n"
    )
    (directory / "performance.md").write_text(render(report))
    stream = io.StringIO(newline="")
    columns = [
        "command_id",
        "variant",
        "name",
        "category",
        "tier",
        "family",
        "status",
        "reason",
        "samples",
        "failure_elapsed_ms",
    ] + [f"{stage}_{stat}" for stage in STAGES for stat in ("median", "p95", "maximum")]
    writer = csv.DictWriter(stream, columns, lineterminator="\n")
    writer.writeheader()
    for case in report["cases"]:
        row = {key: case.get(key, "") for key in columns[:8]}
        row["samples"] = len(case["samples"])
        failures = [
            sample.get("failure_elapsed_ms")
            for sample in case["samples"]
            if sample.get("failure_elapsed_ms") is not None
        ]
        row["failure_elapsed_ms"] = max(failures) if failures else ""
        for stage, values in case["stats"].items():
            if values:
                for stat, value in values.items():
                    row[f"{stage}_{stat}"] = f"{value:.6f}"
        writer.writerow(row)
    (directory / "performance.csv").write_text(stream.getvalue())
