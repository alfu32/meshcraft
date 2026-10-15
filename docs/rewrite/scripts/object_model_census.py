#!/usr/bin/env python3
import argparse
import gzip
import hashlib
import json
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GEOMETRY = ("segments", "faces", "dimensions", "texts")
OVERRIDES = ("overrideSegments", "overrideFaces", "overrideDimensions", "overrideTexts")
DOMAINS = ("voxels", "architectureWalls", "architectureSlabs", "architectureStairs", "architectureFrames", "hvacPlumbingRuns", "hvacVentilationDucts")


def vector(value):
    return tuple(value.get(axis, 0) for axis in ("x", "y", "z"))


def nonidentity_frame(prototype):
    return vector(prototype.get("definitionOrigin", {})) != (0, 0, 0) or any(vector(prototype.get("definitionAxis" + axis, {})) != expected for axis, expected in (("U", (1, 0, 0)), ("V", (0, 1, 0)), ("W", (0, 0, 1))))


def hierarchy_shape(node):
    return (node.get("prototypeId"), tuple(hierarchy_shape(child) for child in node.get("children", [])))


def collect(inventory):
    fixtures = []
    totals = Counter()
    for fixture in inventory["fixtures"]:
        payload = (Path(inventory["source"]) / fixture["path"]).read_bytes()
        if hashlib.sha256(payload).hexdigest() != fixture["sha256"]:
            raise ValueError("Fixture drift: " + fixture["path"])
        model = json.loads(gzip.decompress(payload) if fixture["gzip"] else payload)
        prototypes = {prototype.get("id"): prototype for prototype in model.get("prototypes", [])}
        counts = Counter({"prototypes": len(prototypes)})
        for prototype in prototypes.values():
            counts["nonidentity_definition_frames"] += nonidentity_frame(prototype)
            counts["definitions_with_hotspots"] += bool(prototype.get("hotspots"))
            counts["definitions_with_domain_and_stored_mesh"] += any(prototype.get(key) for key in DOMAINS) and any(prototype.get(key) for key in ("segments", "faces"))
            points = []
            for segment in prototype.get("segments", []):
                points.extend(vector(segment.get(key, {})) for key in ("start", "end"))
            for face in prototype.get("faces", []):
                points.extend(vector(face.get(key, {})) for key in ("a", "b", "c"))
            counts["stored_mesh_point_slots"] += len(points)
            counts["distinct_coordinate_values_per_definition"] += len(set(points))
        instances = Counter()
        child_shapes = defaultdict(set)
        pending = [model["rootInstance"]] if model.get("rootInstance") else []
        while pending:
            node = pending.pop()
            counts["placements"] += 1
            identifier = node.get("prototypeId")
            instances[identifier] += 1
            children = node.get("children", [])
            counts["placements_with_children"] += bool(children)
            child_shapes[identifier].add(hierarchy_shape(node))
            prototype = prototypes.get(identifier, {})
            has_override = any(node.get(key) for key in OVERRIDES)
            counts["placements_with_nonempty_geometry_overrides"] += has_override
            if has_override:
                counts["override_bundles_equal_to_definition_serialized_arrays"] += all(node.get(override, []) == prototype.get(base, []) for base, override in zip(GEOMETRY, OVERRIDES))
                counts["overrides_without_hotspot_position_changes"] += not bool(node.get("hotspotPositions"))
            counts["placements_with_hotspot_position_changes"] += bool(node.get("hotspotPositions"))
            pending.extend(children)
        counts["definitions_with_multiple_placements"] = sum(count > 1 for count in instances.values())
        counts["shared_definitions_with_different_child_prototype_shapes"] = sum(len(shapes) > 1 for identifier, shapes in child_shapes.items() if instances[identifier] > 1)
        counts["repeated_coordinate_slots"] = counts["stored_mesh_point_slots"] - counts["distinct_coordinate_values_per_definition"]
        totals.update(counts)
        fixtures.append({"path": fixture["path"], "sha256": fixture["sha256"], "counts": dict(counts)})
    return {
        "baseline_commit": inventory["head"],
        "method": "Read-only current top-level payload census; excludes undo entries, regenerated geometry and untracked files. Counts aggregate example snapshots and are not unique project entities. Coordinate coincidence is not evidence of shared topological identity. Equal serialized overrides are redundancy candidates, not permission to discard all overrides. Child shapes compare prototype nesting only, not transforms or authored per-occurrence edits.",
        "fixture_count": len(fixtures),
        "totals": dict(totals),
        "fixtures": fixtures,
    }


def main():
    parser = argparse.ArgumentParser(description="Measure legacy object-model repetition without changing legacy files.")
    parser.add_argument("--inventory", type=Path, default=ROOT / "source-inventory.json")
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    result = collect(json.loads(arguments.inventory.read_text()))
    arguments.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"fixture_count": result["fixture_count"], **result["totals"]}))


if __name__ == "__main__":
    main()
