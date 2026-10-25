"""Deterministic legacy-format fixtures; all background facets are unique geometry."""

import copy
import hashlib
import json
import math

TIERS = {"small": 128, "medium": 2048, "high": 16384, "heavy": 65536}
FRAGMENT_CUBES = {"small": 1, "medium": 8, "high": 64, "heavy": 256}


def point(x, y=0, z=0):
    return dict(x=x, y=y, z=z)


def prototype(key, name, segments=(), faces=()):
    return dict(
        id=key,
        name=name,
        kind="MESH",
        definitionOrigin=point(0),
        definitionAxisU=point(1),
        definitionAxisV=point(0, 1),
        definitionAxisW=point(0, 0, 1),
        segments=list(segments),
        faces=list(faces),
    )


def instance(key, proto, origin=None):
    return dict(
        id=key,
        prototypeId=proto,
        instanceOrigin=origin or point(0),
        instanceAxisU=point(1),
        instanceAxisV=point(0, 1),
        instanceAxisW=point(0, 0, 1),
        children=[],
    )


def polyline(key, name, points, closed=False):
    segments = [
        dict(id=f"{key}-{i}", start=a, end=b)
        for i, (a, b) in enumerate(
            zip(points, points[1:] + (points[:1] if closed else []))
        )
    ]
    return prototype(key, name, segments)


def generate(root, destination, tier):
    """Dense loose mesh + separately selectable workpieces and two shared placements."""
    quads = TIERS[tier]
    segments, faces = [], []
    width = math.ceil(math.sqrt(quads))
    for i in range(quads):
        x, z = 30 + i % width * 1.25, 30 + i // width * 1.25
        corners = [
            point(x, 0, z),
            point(x + 1, 0, z),
            point(x + 1, 0, z + 1),
            point(x, 0, z + 1),
        ]
        segments.extend(
            dict(id=f"background-e-{i}-{k}", start=corners[k], end=corners[(k + 1) % 4])
            for k in range(4)
        )
        faces.extend(
            dict(id=f"background-f-{i}-{k}", a=corners[a], b=corners[b], c=corners[c])
            for k, (a, b, c) in enumerate([(0, 2, 1), (0, 3, 2)])
        )
    cube = copy.deepcopy(
        json.loads((root / "examples/dynamic-object.octd").read_text())["prototypes"][1]
    )
    cube.update(id="cube", name="Benchmark Cube", hotspots=[])
    fragment = copy.deepcopy(cube)
    fragment.update(id="fragment", name="Benchmark Fragment", segments=[], faces=[])
    for i in range(FRAGMENT_CUBES[tier]):
        for field, coords in [
            ("segments", ("start", "end")),
            ("faces", ("a", "b", "c")),
        ]:
            for record in cube[field]:
                record = copy.deepcopy(record)
                record["id"] = f"fragment-{i}-" + record["id"]
                for coordinate in coords:
                    record[coordinate]["x"] += -50 + (i % 16) * 3
                    record[coordinate]["z"] += -50 + (i // 16) * 3
                fragment[field].append(record)
    # Small sources separate background growth from the supplemental fragment probes.
    protos = [
        prototype("root", "Benchmark Scene", segments, faces),
        cube,
        fragment,
        polyline(
            "profile",
            "Benchmark Profile",
            [point(0), point(0, 2), point(0, 2, 1), point(0, 0, 1)],
            True,
        ),
        polyline(
            "revolve",
            "Benchmark Revolve",
            [point(2), point(3), point(3, 2), point(2, 2)],
            True,
        ),
        polyline(
            "contour",
            "Benchmark Contour",
            [point(-8, 0, -2), point(-4, 0, -2), point(-4, 0, 4), point(-8, 0, 4)],
            True,
        ),
        polyline("walking", "Benchmark Walking", [point(-6, 0, -2), point(-6, 0, 4)]),
        polyline(
            "fill",
            "Benchmark Fill",
            [point(5), point(8), point(8, 0, 3), point(5, 0, 3)],
            True,
        ),
    ]
    scene = instance("scene", "root")
    scene["children"] = [
        instance("cube-a", "cube"),
        instance("cube-b", "cube", point(1, 0, 1)),
    ]
    scene["children"] += [
        instance(key, key)
        for key in ("profile", "revolve", "contour", "walking", "fill", "fragment")
    ]
    data = dict(
        schemaVersion=17,
        version=17,
        prototypes=protos,
        rootInstance=scene,
        modelUnit=dict(name="unit", size=1),
        gridSpacing=1,
        circleSegments=24,
        snapEpsilon=0.01,
    )
    destination.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(data, separators=(",", ":")).encode()
    destination.write_bytes(payload)
    return dict(
        tier=tier,
        quads=quads,
        background_vertices=14 * quads,
        background_unique_positions=4 * quads,
        background_segments=4 * quads,
        background_triangles=2 * quads,
        background_primitives=6 * quads,
        shared_cube_instances=2,
        fragment_primitives=24 * FRAGMENT_CUBES[tier],
        sha256=hashlib.sha256(payload).hexdigest(),
        bytes=len(payload),
        selected_workload="Fixed-size source per recipe; background consists of unique, non-instanced quads",
    )
