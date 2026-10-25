"""Guard tomorrow's menu assignments against accidental loss on regeneration."""

import tempfile
from pathlib import Path
import unittest
from export_tool_inventory import csv_text, inventory, read_reviews


def catalog(*keys):
    return {"commands": [dict(
        id=key, label=key, category="Draw", kind="tool_activation", capability="core",
        presentation=dict(toolbar_group="Drawing", icon_key="line", menu_path=["Draw", key], menu_aliases=[]),
    ) for key in keys]}


class InventoryTests(unittest.TestCase):
    def test_regeneration_preserves_quoted_multiline_review_fields_by_id(self):
        rows = inventory(catalog("b", "a"), {}, {"line": {}})
        rows[0].update({"ribbon-tab": "Construct", "sub-panel": "Edges", "comments": 'Keep, with "icon"\nand note', "icon-comments": "larger"})
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "tools.csv"
            path.write_text(csv_text(rows))
            reviews = read_reviews(path)
            updated = inventory(catalog("a", "b", "c"), reviews, {"line": {}})
        self.assertEqual(updated[0]["comments"], rows[0]["comments"])
        self.assertEqual(updated[0]["ribbon-tab"], "Construct")
        self.assertEqual(updated[2]["ribbon-tab"], "")

    def test_duplicate_unknown_icon_and_lost_review_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            inventory(catalog("a", "a"), {}, {"line": {}})
        with self.assertRaisesRegex(ValueError, "Unknown icon"):
            inventory(catalog("a"), {}, {})
        with self.assertRaisesRegex(ValueError, "review data disappeared"):
            inventory(catalog("b"), {"a": {"comments": "keep"}}, {"line": {}})

    def test_command_id_icon_fallback_matches_native_toolbar(self):
        rows = inventory(catalog("a"), {}, {"a": {}})
        self.assertEqual(rows[0]["icon"], "a")

    def test_category_uses_first_existing_toolbar_without_assigning_review_fields(self):
        data = catalog("a")
        data["toolbar_groups"] = [("Point Construction", ["a"]), ("Primitives", ["a"])]
        row = inventory(data, {}, {"line": {}})[0]
        self.assertEqual(row["category"], "Point Construction")
        self.assertEqual(row["ribbon-tab"], "")


if __name__ == "__main__":
    unittest.main()
