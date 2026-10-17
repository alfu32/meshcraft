#!/usr/bin/env python3
"""Capture a hash-verified manifest and small samples from the pinned legacy corpus."""

import gzip
import hashlib
import json
import shutil
import subprocess
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parents[3]
REWRITE = WORKSPACE / "docs" / "rewrite"
SAMPLE_LIMIT = 2_000_000
SELECTED_FIXTURES = {
    "examples/test.1.k3d": ["empty-document"],
    "docs/tutorial/000_construction.octd": ["sparse-current-document", "geometry-lines"],
    "examples/test.0.k3d": ["prototype-instance-document"],
    "examples/test.2.k3d": ["root-group-document"],
    "examples/test-shortcuts.0.octd": ["architecture", "dimensions", "faces", "segments"],
    "examples/hotspots.demo.2.k3d": ["hotspots"],
    "assets/glyphs/alphabet.octd": ["glyph-catalog"],
    "examples/arch_frame.octd": ["external-reference-definition"],
    "examples/xrefs.octd": ["external-reference-bundle"],
    "examples/xrefs_ARCH_CHAIR.octd": ["external-reference-companion"],
    "examples/xrefs_ARCH_COLUMN.octd": ["external-reference-companion"],
    "examples/spaceship.octd": ["gzip-and-undo-history"],
    "examples/hvac.demo.k3d": ["hvac", "text"],
    "examples/logo.5.octd": ["voxels"],
}
HISTORICAL_FIXTURES = [
    {
        "role": "root-group-document",
        "commit": "cf5f3c64819d246bbfb77c066a24fd8e63393459",
        "path": "examples/test.2.k3d",
        "local_path": "fixtures/legacy/historical/root-group-test-2.k3d",
    },
]
UNAVAILABLE_HISTORICAL_SHAPES = [
    {
        "role": "flat-top-level-segments-faces",
        "inspected_commit": "2438532a507cc9931eb734a5e5628d5ff34779d2",
        "reason": "The pre-scene-graph source is present, but its Git tree has no saved model fixture to recover.",
    },
    {
        "role": "prototype-only-recovery",
        "inspected_commit": "c85b8b9fa1a4e5d2989bdacfdbbb15aeeaf683af",
        "reason": "The prototype loader path is present, but its Git tree has no saved model fixture to recover.",
    },
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_document(data, path):
    payload = gzip.decompress(data) if data[:2] == b"\x1f\x8b" else data
    try:
        return json.loads(payload)
    except json.JSONDecodeError as error:
        raise SystemExit(f"Invalid JSON fixture {path}: {error}") from error


def fixture_features(path, document, compressed):
    prototypes = document.get("prototypes", [])
    if isinstance(prototypes, dict):
        prototype_values = list(prototypes.values())
    elif isinstance(prototypes, list):
        prototype_values = prototypes
    else:
        prototype_values = []

    fields = (
        "prototypeVertices",
        "segments",
        "faces",
        "architectureWalls",
        "architectureFrames",
        "architectureStairs",
        "architectureSlabs",
        "hvacVentilationDucts",
        "hotspots",
        "dimensions",
        "texts",
        "voxels",
        "externalReferencePath",
    )
    payload_fields = sorted(
        {
            field
            for prototype in prototype_values
            if isinstance(prototype, dict)
            for field in fields
            if prototype.get(field) not in (None, [], {}, "")
        }
    )
    features = []
    if compressed:
        features.append("gzip")
    if document.get("undoHistory"):
        features.append("undo-history")
    if "rootGroup" in document:
        features.append("root-group")
    if "segments" in document or "faces" in document:
        features.append("flat-top-level-geometry")
    if prototype_values and "rootInstance" in document:
        features.append("prototype-instance")
    elif prototype_values:
        features.append("prototype-only")
    if path.startswith("assets/glyphs/"):
        features.append("glyph-catalog")
    if "externalReferencePath" in payload_fields:
        features.append("external-reference")
    features.extend(f"payload:{field}" for field in payload_fields)
    return sorted(set(features)), len(prototype_values), payload_fields


def extract_historical(source, fixture):
    revision = fixture["commit"] + ":" + fixture["path"]
    result = subprocess.run(
        ["git", "-C", str(source), "show", revision],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise SystemExit(f"Cannot recover {revision}: {detail}")
    return result.stdout


def main():
    inventory = json.loads((REWRITE / "source-inventory.json").read_text())
    source = Path(inventory["source"])
    if not source.is_dir():
        raise SystemExit("Pinned legacy checkout missing: " + str(source))
    head = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    if head != inventory["head"]:
        raise SystemExit(f"Legacy checkout drift: expected {inventory['head']}, found {head}")
    dirty = subprocess.check_output(["git", "-C", str(source), "status", "--porcelain"], text=True).strip()
    if dirty:
        raise SystemExit("Legacy checkout is dirty; refusing to capture from it")

    records = []
    by_path = {}
    for indexed in sorted(inventory["fixtures"], key=lambda fixture: fixture["path"]):
        source_path = source / indexed["path"]
        data = source_path.read_bytes()
        actual_hash = digest(data)
        if len(data) != indexed["bytes"] or actual_hash != indexed["sha256"]:
            raise SystemExit("Fixture differs from pinned inventory: " + indexed["path"])
        compressed = data[:2] == b"\x1f\x8b"
        document = read_document(data, indexed["path"])
        features, prototype_count, payload_fields = fixture_features(indexed["path"], document, compressed)
        roles = SELECTED_FIXTURES.get(indexed["path"], [])
        local_path = None
        if roles and len(data) <= SAMPLE_LIMIT:
            relative = Path("fixtures/legacy/samples") / indexed["path"]
            destination = WORKSPACE / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source_path, destination)
            local_path = relative.as_posix()
        record = {
            "path": indexed["path"],
            "bytes": len(data),
            "sha256": actual_hash,
            "compressed": compressed,
            "version_field_present": "version" in document,
            "root_keys": sorted(document.keys()),
            "prototype_count": prototype_count,
            "payload_fields": payload_fields,
            "features": features,
            "selected_for": roles,
            "opt_in": len(data) > SAMPLE_LIMIT,
            "local_copy": local_path,
        }
        records.append(record)
        by_path[indexed["path"]] = record

    if set(SELECTED_FIXTURES) - set(by_path):
        missing = sorted(set(SELECTED_FIXTURES) - set(by_path))
        raise SystemExit("Selected fixture paths missing from inventory: " + ", ".join(missing))

    historical = []
    for fixture in HISTORICAL_FIXTURES:
        data = extract_historical(source, fixture)
        document = read_document(data, fixture["path"])
        relative = Path(fixture["local_path"])
        destination = WORKSPACE / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        features, prototype_count, payload_fields = fixture_features(fixture["path"], document, data[:2] == b"\x1f\x8b")
        historical.append(
            {
                "role": fixture["role"],
                "commit": fixture["commit"],
                "path": fixture["path"],
                "bytes": len(data),
                "sha256": digest(data),
                "root_keys": sorted(document.keys()),
                "prototype_count": prototype_count,
                "payload_fields": payload_fields,
                "features": features,
                "local_path": relative.as_posix(),
            }
        )

    manifest = {
        "schema_version": 1,
        "baseline_commit": inventory["head"],
        "source_repository": "k3d (Octodraw legacy reference)",
        "fixture_count": len(records),
        "sample_copy_limit_bytes": SAMPLE_LIMIT,
        "large_fixture_policy": "Opt-in by source path and SHA-256; do not copy large undo histories into routine runs.",
        "fixtures": records,
        "historical_fixtures": historical,
        "unavailable_historical_shapes": UNAVAILABLE_HISTORICAL_SHAPES,
    }
    output = WORKSPACE / "fixtures" / "legacy" / "manifest.json"
    output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"Wrote {output.relative_to(WORKSPACE)}: {len(records)} pinned fixtures, {len(historical)} historical captures.")


if __name__ == "__main__":
    main()
