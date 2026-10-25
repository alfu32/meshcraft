"""Coverage/provenance/report checks run without a display or native application."""

import copy
import csv
import io
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from cases import Exercise, recipe, TRANSFORMS, ARRAYS, validate_observed
from download import verify_download
from fixtures import generate
from publish import validate
from report import summarize, write
from merge import merge
from download import require_release

ROOT = Path(__file__).resolve().parents[2]


class StressTests(unittest.TestCase):
    def test_every_live_inventory_entry_has_a_recipe_and_concrete_tool_dispatch(self):
        # Checked-in inventory comes from the actual binary; future command additions
        # must at least be classified, and concrete tool IDs must not vanish.
        rows = list(
            csv.DictReader(io.StringIO((ROOT / "docs/tools/tools.csv").read_text()))
        )
        for row in rows:
            r = recipe(
                dict(
                    id=row["command-id"], kind=row["kind"], capability=row["capability"]
                )
            )
            self.assertEqual(
                r.family == "unsupported",
                row["availability"] == "deferred",
                row["command-id"],
            )
        with self.assertRaisesRegex(RuntimeError, "No performance recipe"):
            recipe(
                dict(id="unknown.command", kind="document_mutation", capability="core")
            )
        self.assertTrue(all(len(points) >= 2 for points in TRANSFORMS.values()))
        self.assertTrue(all(len(points) >= 3 for points in ARRAYS.values()))

    def test_fixtures_are_deterministic_unique_geometry_with_shared_workpieces(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch("fixtures.TIERS", {"small": 8}),
        ):
            a = Path(directory) / "a.octd"
            b = Path(directory) / "b.octd"
            meta = generate(ROOT, a, "small")
            other = generate(ROOT, b, "small")
            self.assertEqual(a.read_bytes(), b.read_bytes())
            self.assertEqual(meta, other)
            self.assertEqual(meta["sha256"], hashlib.sha256(a.read_bytes()).hexdigest())
            data = json.loads(a.read_bytes())
            root = data["prototypes"][0]
            self.assertEqual(len(root["segments"]), 32)
            self.assertEqual(len(root["faces"]), 16)
            self.assertEqual(
                len({tuple(e["start"].values()) for e in root["segments"]}), 32
            )
            children = data["rootInstance"]["children"]
            self.assertEqual(sum(c["prototypeId"] == "cube" for c in children), 2)

    def make_report(self):
        sample = dict(status="passed", timings={"commit_ms": 2.0, "total_ms": 3.0})
        tiers = ["small", "medium", "high", "heavy"]
        return dict(
            version="1.2.3",
            source_revision="a" * 40,
            binary_sha256="b" * 64,
            release_artifact=dict(
                tag="1.2.3", source_revision="a" * 40, binary_sha256="b" * 64
            ),
            partial=False,
            tiers=tiers,
            samples_per_case=1,
            started_at="2026-01-01",
            environment=dict(platform="test", cpu="test", rendering="test"),
            fixtures=[
                dict(
                    tier=tier,
                    live_verified=True,
                    background_vertices=14,
                    background_segments=4,
                    background_triangles=2,
                    background_primitives=6,
                )
                for tier in tiers
            ],
            inventory_count=1,
            selected_command_count=1,
            coverage_complete=True,
            finished_at="2026-01-02",
            cases=[
                dict(
                    command_id="tool.line",
                    tier=tier,
                    name="Line",
                    category="Draw",
                    family="draw",
                    status="passed",
                    reason="",
                    samples=[copy.deepcopy(sample)],
                    stats=summarize([sample]),
                )
                for tier in tiers
            ],
        )

    def test_publication_rejects_partial_duplicate_missing_mismatched_and_forged_reports(
        self,
    ):
        good = self.make_report()
        validate(good, "1.2.3", "a" * 40, {"tool.line"})
        for field, value in [
            ("version", "1.2.4"),
            ("source_revision", "c" * 40),
            ("partial", True),
            ("release_artifact", None),
            ("binary_sha256", "d" * 64),
        ]:
            bad = copy.deepcopy(good)
            bad[field] = value
            with self.assertRaises(ValueError):
                validate(bad, "1.2.3", "a" * 40, {"tool.line"})
        for cases in [good["cases"][:-1], good["cases"] + [good["cases"][0]]]:
            bad = copy.deepcopy(good)
            bad["cases"] = cases
            with self.assertRaisesRegex(ValueError, "coverage"):
                validate(bad, "1.2.3", "a" * 40, {"tool.line"})
        bad = copy.deepcopy(good)
        bad["cases"][0]["samples"] = []
        with self.assertRaisesRegex(ValueError, "samples"):
            validate(bad, "1.2.3", "a" * 40, {"tool.line"})

    def test_reports_keep_failures_and_unsupported_distinct_from_measured_success(self):
        report = self.make_report()
        report["cases"][0].update(
            status="failed",
            reason="Actual commit failed",
            samples=[dict(status="failed", timings={})],
            stats=summarize([]),
        )
        report["cases"][1].update(
            status="unsupported", reason="Deferred", samples=[], stats=summarize([])
        )
        # Failures remain publishable as honest complete results; workflow still fails.
        validate(report, "1.2.3", "a" * 40, {"tool.line"})
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            write(report, directory)
            rows = list(
                csv.DictReader(io.StringIO((directory / "performance.csv").read_text()))
            )
            self.assertEqual(
                [r["status"] for r in rows],
                ["failed", "unsupported", "passed", "passed"],
            )
            self.assertEqual(rows[0]["total_ms_median"], "")
            self.assertIn(
                "Actual commit failed", (directory / "performance.md").read_text()
            )
            self.assertEqual(
                json.loads((directory / "performance.json").read_text())["cases"][0][
                    "status"
                ],
                "failed",
            )

    def test_http_success_does_not_hide_rejected_world_point_commits(self):
        before = dict(revision=7)
        with self.assertRaisesRegex(RuntimeError, "undo history needs"):
            validate_observed(
                "tool.copy_mode",
                before,
                dict(
                    revision=7,
                    status="undo history needs 451489000 snapshot bytes, over its 268435456 byte budget",
                ),
            )
        with self.assertRaisesRegex(RuntimeError, "did not commit"):
            validate_observed(
                "hotspot.begin_placement",
                before,
                dict(revision=7, status="Pick a point"),
            )
        validate_observed(
            "hotspot.begin_placement", before, dict(revision=8, status="Added hotspot")
        )

    def test_final_commit_timing_rejects_extra_picks_after_early_completion(self):
        class RpcStub:
            owner_ms_total = 0.0
            owner_count = 0

            def __init__(self):
                self.revision = 0
                self.clicks = 0

            def status(self):
                return dict(
                    revision=self.revision, activeTool="Quad", status="pick point"
                )

            def command(self, *args):
                return {}

            def pointer(self, action, point=None):
                if action == "click":
                    self.clicks += 1
                    if self.clicks == 3:
                        self.revision += 1
                return self.status()

            def frame(self):
                return dict(fresh=True, revision=self.revision, renderFrameMs=0.1)

        exercise = Exercise(RpcStub(), Path("."), {})
        with self.assertRaisesRegex(RuntimeError, "Final pick did not commit"):
            exercise.picks(
                "tool.builtin.quad", [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)]
            )
        exercise = Exercise(RpcStub(), Path("."), {})
        exercise.picks("tool.builtin.quad", [(0, 0, 0), (1, 0, 0), (1, 0, 1)])

    def test_statistics_use_successful_samples_and_nearest_rank_percentiles(self):
        samples = [
            dict(status="passed", timings=dict(total_ms=v)) for v in [3, 1, 2, 4]
        ]
        samples.append(dict(status="failed", timings=dict(total_ms=9999)))
        result = summarize(samples)["total_ms"]
        self.assertEqual(result, dict(median=2.5, p95=4, maximum=4))
        self.assertIsNone(summarize([])["total_ms"])

    def test_prerequisite_failure_cannot_leak_setup_latency_into_tool_metrics(self):
        exercise = Exercise(None, Path("."), {})
        exercise.timings = {"start_ms": 2.0}

        def prerequisite():
            exercise.timings["commit_ms"] = 1000.0
            raise RuntimeError("setup failed before the tool could run")

        with self.assertRaisesRegex(RuntimeError, "setup failed"):
            exercise.setup(prerequisite)
        self.assertEqual(exercise.timings, {"start_ms": 2.0})

    def test_tool_cycle_cannot_count_deferred_activation_as_an_operation(self):
        class RpcStub:
            owner_ms_total = 0.0
            owner_count = 0

            def command(self, *args):
                return {}

            def status(self):
                return {"activeTool": "Arc"}

        exercise = Exercise(RpcStub(), Path("."), {})
        exercise.catalog = [
            dict(id="tool.builtin.arc", label="Arc", capability="deferred")
        ]
        with self.assertRaisesRegex(RuntimeError, "unsupported tool"):
            exercise.execute(
                dict(id="tool.cycle.planar_construction", capability="core")
            )

    def test_merge_requires_four_identical_binaries_but_keeps_each_worker_environment(
        self,
    ):
        base = self.make_report()
        workers = []
        for tier in base["tiers"]:
            worker = copy.deepcopy(base)
            worker["tiers"] = [tier]
            worker["partial"] = True
            worker["fixtures"] = [f for f in base["fixtures"] if f["tier"] == tier]
            worker["cases"] = [c for c in base["cases"] if c["tier"] == tier]
            worker["environment"]["cpu"] = tier + " CPU"
            workers.append(worker)
        result = merge(workers)
        validate(result, "1.2.3", "a" * 40, {"tool.line"})
        self.assertEqual(len(result["cases"]), 4)
        self.assertEqual(result["environments"]["heavy"]["cpu"], "heavy CPU")
        with self.assertRaisesRegex(ValueError, "four"):
            merge(workers[:3])
        bad = copy.deepcopy(workers)
        bad[-1]["binary_sha256"] = "c" * 64
        with self.assertRaisesRegex(ValueError, "identity"):
            merge(bad)
        bad = copy.deepcopy(workers)
        bad[-1]["tiers"] = ["small"]
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            merge(bad)

    def test_missing_release_binary_fails_preflight_without_a_build_or_download(self):
        with (
            patch(
                "download.subprocess.check_output",
                return_value=json.dumps(
                    dict(tagName="1.2.3", isDraft=False, assets=[])
                ),
            ),
            patch("download.subprocess.run") as run,
        ):
            with self.assertRaisesRegex(ValueError, "Missing same-tag release binary"):
                require_release("owner/repo", "1.2.3", "x86_64-unknown-linux-gnu")
            run.assert_not_called()

    def test_publication_requires_bulk_variants_and_verified_models(self):
        report = self.make_report()
        with self.assertRaisesRegex(ValueError, "coverage"):
            validate(report, "1.2.3", "a" * 40, {"tool.line", "tool.select"})
        bad = copy.deepcopy(report)
        bad["fixtures"][0]["live_verified"] = False
        with self.assertRaisesRegex(ValueError, "live fixture"):
            validate(bad, "1.2.3", "a" * 40, {"tool.line"})
        bad = copy.deepcopy(report)
        bad["cases"][0]["status"] = "unsupported"
        with self.assertRaisesRegex(ValueError, "hidden as unsupported"):
            validate(bad, "1.2.3", "a" * 40, {"tool.line": "registered"})

    def test_release_archive_checksum_failure_prevents_extraction_or_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            archive = directory / "release.tar.gz"
            checksum = directory / "release.tar.gz.sha256"
            archive.write_bytes(b"wrong executable")
            checksum.write_text("0" * 64 + "  release.tar.gz\n")
            with patch("download.payload") as extract:
                with self.assertRaisesRegex(ValueError, "checksum"):
                    verify_download(
                        archive,
                        checksum,
                        "1.2.3",
                        "a" * 40,
                        directory / "extract",
                        "x86_64-unknown-linux-gnu",
                    )
                extract.assert_not_called()


if __name__ == "__main__":
    unittest.main()
