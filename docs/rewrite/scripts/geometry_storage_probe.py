#!/usr/bin/env python3
"""Compare geometry position-storage layouts against the pinned fixture corpus."""

import argparse
import gzip
import hashlib
import json
import math
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COORDINATE_BYTES = 3 * 8
STABLE_ID_BYTES = 8
REFERENCE_BYTES = 8
LEGACY_EPSILON = 1e-3


def coordinates(value):
    return tuple(float(value.get(axis, 0.0)) for axis in ("x", "y", "z"))


def quantized_key(point, epsilon=LEGACY_EPSILON):
    """Match Kotlin roundToInt for the legacy prototype vertex key."""
    scaled = (coordinate / epsilon for coordinate in point)
    return tuple(
        math.floor(value + 0.5) if value >= 0 else math.ceil(value - 0.5)
        for value in scaled
    )


def point_metrics(points):
    exact = set(points)
    by_quantized_key = defaultdict(Counter)
    for point in points:
        by_quantized_key[quantized_key(point)][point] += 1
    ambiguous_keys = {
        key for key, positions in by_quantized_key.items() if len(positions) > 1
    }
    ambiguous_slots = sum(
        1 for point in points if quantized_key(point) in ambiguous_keys
    )
    residual_slots = sum(
        sum(positions.values()) - max(positions.values())
        for positions in by_quantized_key.values()
        if len(positions) > 1
    )
    return {
        "point_slots": len(points),
        "exact_positions": len(exact),
        "quantized_keys": len(by_quantized_key),
        "ambiguous_quantized_keys": len(ambiguous_keys),
        "slots_in_ambiguous_keys": ambiguous_slots,
        "minimum_corner_residual_slots": residual_slots,
    }


def prototype_points(prototype):
    points = []
    for segment in prototype.get("segments", []):
        points.extend(coordinates(segment.get(key, {})) for key in ("start", "end"))
    for face in prototype.get("faces", []):
        points.extend(coordinates(face.get(key, {})) for key in ("a", "b", "c"))
    return points


def add_metrics(total, current):
    for key, value in current.items():
        total[key] += value


def packed_layouts(metrics):
    slots = metrics["point_slots"]
    exact = metrics["exact_positions"]
    quantized = metrics["quantized_keys"]
    inline = slots * COORDINATE_BYTES

    def indexed_bytes(vertex_count):
        return (
            vertex_count * (COORDINATE_BYTES + STABLE_ID_BYTES)
            + slots * REFERENCE_BYTES
        )

    exact_bytes = indexed_bytes(exact)
    quantized_bytes = indexed_bytes(quantized)
    no_weld_bytes = indexed_bytes(slots)
    source_alias_vertices = metrics.get("mapped_keys", 0) + metrics.get(
        "unmapped_slots", 0
    )
    source_alias_bytes = (
        source_alias_vertices * (COORDINATE_BYTES + STABLE_ID_BYTES)
        + slots * REFERENCE_BYTES
        + metrics.get("mapped_residual_slots", 0) * COORDINATE_BYTES
    )
    return {
        "scope": "position and vertex-reference columns only; primitive IDs/colors, alias strings, opaque metadata, allocator and container overhead excluded",
        "inline_coordinate_bytes": inline,
        "indexed_exact_coordinate_lower_bound_bytes": exact_bytes,
        "indexed_legacy_quantized_key_bytes": quantized_bytes,
        "indexed_no_weld_bytes": no_weld_bytes,
        "indexed_source_alias_preserving_bytes": source_alias_bytes,
        "exact_indexed_saving_percent": round((inline - exact_bytes) * 100 / inline, 2)
        if inline
        else 0.0,
        "quantized_indexed_saving_percent": round(
            (inline - quantized_bytes) * 100 / inline, 2
        )
        if inline
        else 0.0,
        "no_weld_indexed_overhead_percent": round(
            (no_weld_bytes - inline) * 100 / inline, 2
        )
        if inline
        else 0.0,
        "source_alias_preserving_saving_percent": round(
            (inline - source_alias_bytes) * 100 / inline, 2
        )
        if inline
        else 0.0,
    }


def inspect(inventory):
    source = Path(inventory["source"])
    totals = defaultdict(int)
    prototype_count = 0
    legacy_vertex_records = 0
    mapped_key_hits = 0
    unmapped_keys = 0
    unmapped_key_slots = 0
    mapped_residual_slots = 0
    extra_legacy_keys = 0

    for fixture in inventory["fixtures"]:
        payload = (source / fixture["path"]).read_bytes()
        if hashlib.sha256(payload).hexdigest() != fixture["sha256"]:
            raise ValueError(f"fixture hash mismatch: {fixture['path']}")
        decoded = gzip.decompress(payload) if fixture.get("gzip") else payload
        model = json.loads(decoded)
        for prototype in model.get("prototypes", []):
            points = prototype_points(prototype)
            if not points:
                continue
            prototype_count += 1
            add_metrics(totals, point_metrics(points))
            point_buckets = defaultdict(Counter)
            for point in points:
                point_buckets[quantized_key(point)][point] += 1
            current_keys = set(point_buckets)
            legacy_keys = {
                tuple(vertex.get(axis, 0) for axis in ("x", "y", "z"))
                for vertex in prototype.get("prototypeVertices", [])
            }
            legacy_vertex_records += len(prototype.get("prototypeVertices", []))
            mapped_key_hits += len(current_keys & legacy_keys)
            unmapped_keys += len(current_keys - legacy_keys)
            unmapped_key_slots += sum(
                sum(point_buckets[key].values())
                for key in current_keys - legacy_keys
            )
            mapped_residual_slots += sum(
                sum(point_buckets[key].values()) - max(point_buckets[key].values())
                for key in current_keys & legacy_keys
                if len(point_buckets[key]) > 1
            )
            extra_legacy_keys += len(legacy_keys - current_keys)

    result = {
        "baseline_commit": inventory["head"],
        "fixture_count": len(inventory["fixtures"]),
        "geometry_prototype_count": prototype_count,
        "measurements": dict(totals),
        "packed_layouts": packed_layouts(
            {**totals, "mapped_keys": mapped_key_hits, "unmapped_slots": unmapped_key_slots, "mapped_residual_slots": mapped_residual_slots}
        ),
        "legacy_vertex_alias_coverage": {
            "serialized_prototype_vertex_records": legacy_vertex_records,
            "geometry_quantized_keys_with_record": mapped_key_hits,
            "geometry_quantized_keys_without_record": unmapped_keys,
            "slots_without_record": unmapped_key_slots,
            "mapped_slots_requiring_position_residual": mapped_residual_slots,
            "serialized_keys_not_in_base_geometry": extra_legacy_keys,
        },
        "interpretation": [
            "Exact-coordinate indexing is a storage lower bound, not an identity-safe weld rule.",
            "Quantized indexing uses the legacy 1e-3 key; ambiguous keys preserve multiple exact positions and cannot be collapsed without corner residuals.",
            "The source-alias-preserving estimate shares mapped legacy keys, stores corner residuals for differing exact positions, and allocates a unique vertex to each slot without a legacy key.",
            "No-weld indexing preserves every position slot and adds persistent IDs/references, so its overhead is the cost of identity without proven sharing.",
            "All byte estimates use packed f64 coordinates and u64 IDs/references; they are not Rust heap measurements.",
            "Compatibility alias strings and opaque legacy metadata are excluded, so source-alias-preserving bytes are a lower bound.",
            "Legacy prototypeVertices coverage is measured against base segment/face positions only; unmatched entries remain compatibility data, not garbage.",
        ],
    }
    return result


def main():
    parser = argparse.ArgumentParser(
        description="Measure packed geometry position layouts without changing fixtures."
    )
    parser.add_argument("--inventory", type=Path, default=ROOT / "source-inventory.json")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = inspect(json.loads(args.inventory.read_text()))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"fixture_count": result["fixture_count"], **result["measurements"]}))


if __name__ == "__main__":
    main()
