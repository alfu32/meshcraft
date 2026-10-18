import argparse
import gzip
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from canonical_compare import (  # noqa: E402
    SnapshotError,
    canonicalize,
    compare_snapshots,
    load_snapshot,
    parse_tolerance,
)

SCRIPT = Path(__file__).resolve().parents[1] / "canonical_compare.py"


class CanonicalSnapshotTests(unittest.TestCase):
    def test_sorts_entity_collections_but_preserves_primitive_vertex_order(self):
        snapshot = {
            "definitions": [
                {
                    "id": "z",
                    "geometry": {
                        "vertices": [{"id": 2}, {"id": 1}],
                        "triangles": [{"id": "b", "vertices": [2, 1, 3]}],
                    },
                },
                {"id": "a", "geometry": {"vertices": [], "triangles": []}},
            ]
        }

        result = canonicalize(snapshot)

        self.assertEqual(["a", "z"], [item["id"] for item in result["definitions"]])
        geometry = result["definitions"][1]["geometry"]
        self.assertEqual([1, 2], [item["id"] for item in geometry["vertices"]])
        self.assertEqual([2, 1, 3], geometry["triangles"][0]["vertices"])

    def test_rejects_ambiguous_entity_collections(self):
        with self.assertRaisesRegex(SnapshotError, "duplicate entity id"):
            canonicalize({"definitions": [{"id": 7}, {"id": 7}]})
        with self.assertRaisesRegex(SnapshotError, "need an id"):
            canonicalize({"segments": [{"endpoints": [1, 2]}]})

    def test_compare_ignores_entity_array_order_but_compares_ids_exactly(self):
        expected = {"definitions": [{"id": "a"}, {"id": "b"}]}
        reordered = {"definitions": [{"id": "b"}, {"id": "a"}]}
        changed = {"definitions": [{"id": "a"}, {"id": "c"}]}

        self.assertEqual([], compare_snapshots(canonicalize(expected), canonicalize(reordered)))
        self.assertTrue(compare_snapshots(canonicalize(expected), canonicalize(changed)))

    def test_tolerance_applies_only_to_selected_numeric_paths(self):
        expected = {
            "definitions": [
                {
                    "id": "box",
                    "geometry": {
                        "vertices": [{"id": 1, "position": [1.0, 2.0, 3.0]}],
                        "triangles": [{"id": 4, "color": [255, 0, 0, 255]}],
                    },
                }
            ]
        }
        actual = json.loads(json.dumps(expected))
        actual["definitions"][0]["geometry"]["vertices"][0]["position"][0] += 1e-10
        tolerances = [
            parse_tolerance(
                "/definitions/*/geometry/vertices/*/position/*=1e-9,1e-12"
            )
        ]
        self.assertEqual([], compare_snapshots(expected, actual, tolerances))

        actual["definitions"][0]["geometry"]["triangles"][0]["color"][0] = 254
        self.assertTrue(compare_snapshots(expected, actual, tolerances))

    def test_tolerances_cannot_be_attached_to_identity_or_color_fields(self):
        with self.assertRaisesRegex(argparse.ArgumentTypeError, "must compare exactly"):
            parse_tolerance("/definitions/*/id=1e-6")
        with self.assertRaisesRegex(argparse.ArgumentTypeError, "must compare exactly"):
            parse_tolerance("/definitions/*/geometry/triangles/*/color/*=1")

    def test_wildcard_tolerance_still_cannot_mask_identity_differences(self):
        expected = {"definitions": [{"id": 10}]}
        actual = {"definitions": [{"id": 11}]}
        broad_tolerance = [parse_tolerance("/definitions/*/*=100")]

        self.assertTrue(compare_snapshots(expected, actual, broad_tolerance))

    def test_loads_gzip_snapshots_by_magic(self):
        payload = json.dumps({"definitions": [{"id": "b"}, {"id": "a"}]}).encode()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "snapshot.bin"
            path.write_bytes(gzip.compress(payload))
            value = load_snapshot(path)

        self.assertEqual(["a", "b"], [item["id"] for item in value["definitions"]])

    def test_rejects_truncated_gzip(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "truncated.gz"
            path.write_bytes(b"\x1f\x8b\x08\x00broken")
            with self.assertRaisesRegex(SnapshotError, "invalid gzip stream"):
                load_snapshot(path)

    def test_cli_normalizes_and_compares_snapshots_end_to_end(self):
        expected = {
            "definitions": [
                {"id": "part", "geometry": {"vertices": [{"id": 1, "position": [0, 0, 0]}]}}
            ]
        }
        actual = {
            "definitions": [
                {
                    "id": "part",
                    "geometry": {"vertices": [{"id": 1, "position": [1e-10, 0, 0]}]},
                }
            ]
        }
        with tempfile.TemporaryDirectory() as directory:
            directory_path = Path(directory)
            expected_path = directory_path / "expected.json"
            actual_path = directory_path / "actual.json"
            expected_path.write_text(json.dumps(expected), encoding="utf-8")
            actual_path.write_text(json.dumps(actual), encoding="utf-8")
            normalized_expected = directory_path / "expected.normalized.json"
            normalized_actual = directory_path / "actual.normalized.json"

            for source, destination in (
                (expected_path, normalized_expected),
                (actual_path, normalized_actual),
            ):
                result = subprocess.run(
                    [
                        sys.executable,
                        str(SCRIPT),
                        "normalize",
                        str(source),
                        "-o",
                        str(destination),
                    ],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(0, result.returncode, result.stderr)

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "compare",
                    str(normalized_expected),
                    str(normalized_actual),
                    "--tolerance",
                    "/definitions/*/geometry/vertices/*/position/*=1e-9",
                ],
                check=False,
                capture_output=True,
                text=True,
            )

        self.assertEqual(0, result.returncode, result.stdout + result.stderr)
        self.assertIn("Canonical authored state matches", result.stdout)


if __name__ == "__main__":
    unittest.main()
