#!/usr/bin/env python3
"""Export the live registry for menu review without discarding review columns."""

import argparse
import csv
import html
import io
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REVIEW_COLUMNS = ("ribbon-tab", "sub-panel", "comments", "icon-comments")
COLUMNS = (
    "category", "name", "icon", "command-id", "kind", "capability",
    "availability", "current-menu", "current-menu-aliases", *REVIEW_COLUMNS,
)


def read_reviews(path):
    if not path.exists():
        return {}
    with path.open(newline="", encoding="utf-8") as stream:
        reader = csv.DictReader(stream)
        if reader.fieldnames != list(COLUMNS):
            raise ValueError("Unexpected CSV header; refusing to overwrite review data")
        reviews = {}
        for row in reader:
            key = row["command-id"]
            if not key or key in reviews:
                raise ValueError(f"Missing/duplicate command ID in review CSV: {key}")
            reviews[key] = {column: row[column] for column in REVIEW_COLUMNS}
        return reviews


def inventory(catalog, reviews, icons):
    rows = []
    seen = set()
    categories = {}
    for name, keys in catalog.get("toolbar_groups", []):
        for key in keys:
            categories.setdefault(key, name)
    for command in catalog["commands"]:
        key = command["id"]
        if key in seen:
            raise ValueError(f"Duplicate registry ID: {key}")
        seen.add(key)
        presentation = command["presentation"]
        # Match the native toolbar's explicit-key, then command-ID fallback.
        requested_icon = presentation["icon_key"]
        icon = requested_icon if requested_icon in icons else (key if key in icons else None)
        if requested_icon and icon is None:
            raise ValueError(f"Unknown icon {requested_icon} for {key}")
        row = dict(zip(COLUMNS, [""] * len(COLUMNS)))
        row.update({
            "category": categories.get(key) or presentation["toolbar_group"] or command["category"],
            "name": command["label"],
            "icon": icon or "(none)",
            "command-id": key,
            "kind": command["kind"],
            "capability": command["capability"],
            "availability": "deferred" if command["capability"] == "deferred" else "registered",
            "current-menu": " > ".join(presentation["menu_path"]),
            "current-menu-aliases": "; ".join(" > ".join(p) for p in presentation["menu_aliases"]),
        })
        row.update(reviews.get(key, {}))
        rows.append(row)
    removed = [key for key, values in reviews.items() if key not in seen and any(values.values())]
    if removed:
        raise ValueError(f"Commands with review data disappeared; migrate them first: {removed}")
    return sorted(rows, key=lambda row: (row["category"].casefold(), row["name"].casefold(), row["command-id"]))


def csv_text(rows):
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, COLUMNS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


def gallery(rows, version, icons):
    escape = html.escape
    content = []
    for row in rows:
        region = icons.get(row["icon"])
        glyph = "<span class='icon missing'>—</span>"
        if region:
            glyph = f"<span class='icon' style='background-position:-{region['start_x']}px -{region['start_y']}px' title='{escape(row['icon'])}'></span>"
        cells = [glyph] + [escape(row[column]) for column in ("category", "name", "command-id", "availability", *REVIEW_COLUMNS)]
        content.append("<tr>" + "".join(f"<td>{cell}</td>" for cell in cells) + "</tr>")
    headings = ("Icon", "Category", "Name", "Command ID", "Availability", *REVIEW_COLUMNS)
    return """<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Meshcraft tool inventory</title>
<style>
body{font:15px system-ui;background:#202730;color:#e8eff7;margin:24px}
a{color:#83cdf5}table{border-collapse:collapse;width:100%}th{position:sticky;top:0;background:#284962;text-align:left}
td,th{padding:8px;border-bottom:1px solid #43515f}td:nth-child(4){font:12px monospace}
.icon{display:block;width:32px;height:32px;background-image:url('../../assets/tool-icons/icons.svg');flex-shrink:0}
.missing{background:none;color:#9eacbb}tr:hover{background:#31465b}
</style>
""" + f"<h1>Meshcraft {escape(version)} — {len(rows)} tools and commands</h1>\n" + """<p>Edit <a href="tools.csv">tools.csv</a> to assign menus. The last four columns are your review fields.
This gallery is a generated, read-only view. Deferred entries remain listed; registered commands can require a selection or other context.</p>
<table><thead><tr>""" + "".join(f"<th>{escape(label)}</th>" for label in headings) + "</tr></thead><tbody>\n" + "\n".join(content) + "\n</tbody></table></html>\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/meshcraft")
    parser.add_argument("--check", action="store_true", help="Fail if checked-in inventory differs")
    args = parser.parse_args()
    catalog = json.loads(subprocess.check_output([str(args.binary.resolve()), "--command-catalog"], text=True))
    with (ROOT / "assets/tool-icons/icons.mapping.csv").open(newline="") as stream:
        icons = {row["name"]: row for row in csv.DictReader(stream, delimiter="|")}
    destination = ROOT / "docs/tools"
    rows = inventory(catalog, read_reviews(destination / "tools.csv"), icons)
    outputs = {"tools.csv": csv_text(rows), "index.html": gallery(rows, catalog["version"], icons)}
    for name, value in outputs.items():
        path = destination / name
        if args.check:
            if not path.exists() or path.read_text() != value:
                raise SystemExit(f"Stale inventory: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(value, encoding="utf-8")
    print(f"{'Checked' if args.check else 'Exported'} {len(rows)} registry entries; preserved review columns")


if __name__ == "__main__":
    main()
