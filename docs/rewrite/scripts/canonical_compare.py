#!/usr/bin/env python3
"""Normalize and compare authored canonical model snapshots."""

from __future__ import annotations

import argparse
import gzip
import json
import math
import sys
from pathlib import Path
from typing import Any


ENTITY_COLLECTIONS = {
    "definitions",
    "child_layouts",
    "occurrences",
    "entries",
    "vertices",
    "segments",
    "triangles",
    "dimensions",
    "texts",
    "hotspots",
    "walls",
    "slabs",
    "stairs",
    "frames",
    "annotations",
}
MAX_DIFFERENCES = 100


class SnapshotError(ValueError):
    """Raised when a snapshot or its comparison rules are malformed."""


def _id_sort_key(identifier: Any) -> tuple[int, Any]:
    if isinstance(identifier, bool):
        raise SnapshotError("entity IDs must be strings or integers, not booleans")
    if isinstance(identifier, int):
        return (0, identifier)
    if isinstance(identifier, str):
        return (1, identifier)
    raise SnapshotError(f"entity ID must be a string or integer, got {type(identifier).__name__}")


def _pointer(path: tuple[str, ...]) -> str:
    escaped = (part.replace("~", "~0").replace("/", "~1") for part in path)
    return "/" + "/".join(escaped)


def canonicalize(value: Any, path: tuple[str, ...] = ()) -> Any:
    """Sort ID-bearing entity arrays while preserving all authored value order."""
    if isinstance(value, dict):
        return {key: canonicalize(child, path + (str(key),)) for key, child in value.items()}
    if isinstance(value, list):
        items = [canonicalize(child, path + (str(index),)) for index, child in enumerate(value)]
        if path and path[-1] in ENTITY_COLLECTIONS and any(
            isinstance(item, dict) for item in items
        ):
            if not all(isinstance(item, dict) for item in items):
                raise SnapshotError(f"{_pointer(path)}: entity collection mixes records and values")
            ids: list[tuple[tuple[int, Any], dict[str, Any]]] = []
            seen: set[tuple[int, Any]] = set()
            for index, item in enumerate(items):
                if not isinstance(item, dict) or "id" not in item:
                    raise SnapshotError(
                        f"{_pointer(path + (str(index),))}: canonical {path[-1]} entries need an id"
                    )
                sort_key = _id_sort_key(item["id"])
                if sort_key in seen:
                    raise SnapshotError(
                        f"{_pointer(path)}: duplicate entity id {item['id']!r}"
                    )
                seen.add(sort_key)
                ids.append((sort_key, item))
            items = [item for _, item in sorted(ids, key=lambda pair: pair[0])]
        return items
    if isinstance(value, float) and not math.isfinite(value):
        raise SnapshotError(f"{_pointer(path)}: non-finite numbers are not valid JSON model data")
    return value


def _reject_constant(value: str) -> None:
    raise SnapshotError(f"non-standard JSON number {value!r} is not supported")


def load_snapshot(filename: str | Path) -> Any:
    path = Path(filename)
    payload = path.read_bytes()
    if payload.startswith(b"\x1f\x8b"):
        try:
            payload = gzip.decompress(payload)
        except (OSError, EOFError) as error:
            raise SnapshotError(f"{path}: invalid gzip stream: {error}") from error
    try:
        value = json.loads(payload.decode("utf-8"), parse_constant=_reject_constant)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SnapshotError(f"{path}: invalid UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise SnapshotError(f"{path}: canonical snapshot root must be an object")
    return canonicalize(value)


def parse_tolerance(specification: str) -> tuple[tuple[str, ...], tuple[float, float]]:
    try:
        pointer, raw_values = specification.rsplit("=", 1)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected JSON-pointer=absolute[,relative]") from error
    if not pointer.startswith("/"):
        raise argparse.ArgumentTypeError("tolerance paths must be absolute JSON pointers")
    segments = tuple(
        part.replace("~1", "/").replace("~0", "~") for part in pointer[1:].split("/")
    )
    try:
        values = [float(value) for value in raw_values.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("tolerances must be finite numbers") from error
    if len(values) not in (1, 2) or any(not math.isfinite(value) or value < 0 for value in values):
        raise argparse.ArgumentTypeError("use nonnegative finite absolute[,relative] tolerances")
    if len(values) == 1:
        values.append(0.0)
    if _is_exact_path(segments):
        raise argparse.ArgumentTypeError("IDs, vertex references, and colors must compare exactly")
    return segments, (values[0], values[1])


def _is_exact_path(path: tuple[str, ...]) -> bool:
    for segment in path:
        lower = segment.lower()
        if lower in {"id", "color", "rgba", "endpoints"} or lower.endswith("_id"):
            return True
    return len(path) >= 2 and path[-2] in {"vertices", "endpoints"}


def _matches(pattern: tuple[str, ...], path: tuple[str, ...]) -> bool:
    return len(pattern) == len(path) and all(
        expected == "*" or expected == actual for expected, actual in zip(pattern, path)
    )


def _tolerance_for(
    path: tuple[str, ...], tolerances: list[tuple[tuple[str, ...], tuple[float, float]]]
) -> tuple[float, float] | None:
    if _is_exact_path(path):
        return None
    matches = [
        (sum(part != "*" for part in pattern), index, tolerance)
        for index, (pattern, tolerance) in enumerate(tolerances)
        if _matches(pattern, path)
    ]
    if not matches:
        return None
    return max(matches, key=lambda match: (match[0], match[1]))[2]


def compare_snapshots(
    expected: Any,
    actual: Any,
    tolerances: list[tuple[tuple[str, ...], tuple[float, float]]] | None = None,
) -> list[str]:
    """Return JSON-pointer diagnostics for authored-state differences."""
    tolerances = tolerances or []
    differences: list[str] = []

    def report(path: tuple[str, ...], message: str) -> None:
        if len(differences) < MAX_DIFFERENCES:
            differences.append(f"{_pointer(path)}: {message}")

    def brief(value: Any) -> str:
        rendered = repr(value)
        if len(rendered) <= 160:
            return rendered
        return rendered[:157] + "..."

    def visit(left: Any, right: Any, path: tuple[str, ...]) -> None:
        if len(differences) >= MAX_DIFFERENCES:
            return
        if isinstance(left, dict) and isinstance(right, dict):
            for key in sorted(left.keys() - right.keys()):
                report(path + (str(key),), "missing from actual snapshot")
            for key in sorted(right.keys() - left.keys()):
                report(path + (str(key),), "unexpected in actual snapshot")
            for key in sorted(left.keys() & right.keys()):
                visit(left[key], right[key], path + (str(key),))
            return
        if isinstance(left, list) and isinstance(right, list):
            if len(left) != len(right):
                report(path, f"length {len(left)} != {len(right)}")
            for index, (left_item, right_item) in enumerate(zip(left, right)):
                visit(left_item, right_item, path + (str(index),))
            return
        if _is_number(left) and _is_number(right):
            tolerance = _tolerance_for(path, tolerances)
            if tolerance is None:
                equal = left == right
            else:
                absolute, relative = tolerance
                equal = abs(left - right) <= absolute + relative * max(abs(left), abs(right))
            if not equal:
                suffix = ""
                if tolerance is not None:
                    suffix = f" (tolerance abs={tolerance[0]:g}, rel={tolerance[1]:g})"
                report(path, f"{left!r} != {right!r}{suffix}")
            return
        if type(left) is not type(right) or left != right:
            report(path, f"{brief(left)} != {brief(right)}")

    visit(expected, actual, ())
    if len(differences) == MAX_DIFFERENCES:
        differences.append(f"... additional differences omitted (limit {MAX_DIFFERENCES})")
    return differences


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _write_normalized(args: argparse.Namespace) -> int:
    value = load_snapshot(args.source)
    rendered = json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        Path(args.output).write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


def _compare(args: argparse.Namespace) -> int:
    expected = load_snapshot(args.expected)
    actual = load_snapshot(args.actual)
    differences = compare_snapshots(expected, actual, args.tolerance)
    if not differences:
        print("Canonical authored state matches.")
        return 0
    for difference in differences:
        print(difference)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(required=True)
    normalize_parser = subparsers.add_parser("normalize", help="emit canonical JSON")
    normalize_parser.add_argument("source")
    normalize_parser.add_argument("--output", "-o")
    normalize_parser.set_defaults(handler=_write_normalized)
    compare_parser = subparsers.add_parser("compare", help="compare authored model snapshots")
    compare_parser.add_argument("expected")
    compare_parser.add_argument("actual")
    compare_parser.add_argument(
        "--tolerance",
        action="append",
        type=parse_tolerance,
        default=[],
        metavar="JSON_POINTER=ABS[,REL]",
        help="numeric tolerance for a path; '*' matches one path segment, repeat as needed",
    )
    compare_parser.set_defaults(handler=_compare)
    args = parser.parse_args()
    try:
        return args.handler(args)
    except (OSError, SnapshotError) as error:
        print(f"canonical_compare: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
