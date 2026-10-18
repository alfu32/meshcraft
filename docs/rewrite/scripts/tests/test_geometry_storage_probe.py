import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from geometry_storage_probe import packed_layouts, point_metrics, quantized_key


class GeometryStorageProbeTests(unittest.TestCase):
    def test_kotlin_quantized_key_rounds_ties_away_from_zero(self):
        self.assertEqual(quantized_key((0.0005, -0.0005, 0.0)), (1, -1, 0))

    def test_exact_indexing_is_only_a_lower_bound_when_vertices_are_shared(self):
        metrics = point_metrics(
            [(0, 0, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)] * 8
        )
        packed = packed_layouts(metrics)
        self.assertLess(
            packed["indexed_exact_coordinate_lower_bound_bytes"],
            packed["inline_coordinate_bytes"],
        )

    def test_no_weld_indexing_keeps_identity_but_adds_reference_overhead(self):
        metrics = point_metrics([(0, 0, 0), (0, 0, 0)])
        packed = packed_layouts(metrics)
        self.assertGreater(
            packed["indexed_no_weld_bytes"], packed["inline_coordinate_bytes"]
        )

    def test_quantized_alias_keeps_distinct_exact_positions_visible(self):
        metrics = point_metrics([(0.0001, 0, 0), (0.0002, 0, 0)])
        self.assertEqual(metrics["quantized_keys"], 1)
        self.assertEqual(metrics["exact_positions"], 2)
        self.assertEqual(metrics["ambiguous_quantized_keys"], 1)


if __name__ == "__main__":
    unittest.main()
