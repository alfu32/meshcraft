"""Tutorial 6: a dynamic curtain wall panel, defined with hotspots and no code."""

import time

# In template units: one unit is 30 cm, so a panel starts 1.8 m wide and 2.4 m high.
W, H = 6.0, 8.0
WIDER = 8.0
PANELS = 5
CIVIL = "Civil"
OBJECTS = "Objects & Hotspots"
TEMPLATE_CARD = (750, 266)
GRID = ("Grid", "Grid Line", "Axis Guide")
ON_HELPER = ("Axis Guide",)
AHEAD = 3.0
STILE = 0.3
DEPTH = 0.2


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("file.home", open=True)


def owners(d):
    return d.rpc.call("meshcraft_geometry", dict(path=d.root()["path"], limit=1))["architecture"]


def box(low, high):
    return [(x, y, z) for x in (low[0], high[0]) for y in (low[1], high[1]) for z in (low[2], high[2])]


def geometry(d, path):
    offset, primitives, vertices = 0, [], []
    while offset is not None:
        page = d.rpc.call("meshcraft_geometry", dict(path=path, offset=offset, limit=1000))
        primitives += page["primitives"]
        vertices += page["vertices"]
        offset = page.get("nextOffset")
    return page, primitives, vertices


def settled_max_x(d, path, wanted):
    """Largest world x of an instance, once evaluation has caught up with a drag."""
    deadline = time.monotonic() + 15
    while True:
        offset, top = 0, None
        while offset is not None:
            page = d.rpc.call("meshcraft_geometry", dict(path=path, offset=offset, limit=1000))
            shift = page["transform"]["translation"]["x"]
            xs = [v["position"]["x"] + shift for v in page["vertices"]]
            top = max(xs + ([top] if top is not None else []))
            offset = page.get("nextOffset")
        if abs(top - wanted) < 1e-6 or time.monotonic() > deadline:
            return top
        time.sleep(0.2)


def hotspot(path, ident):
    return dict(path=path, kind="semantic_handle", semantic_kind="hotspot", id=ident)


def record(d):
    d.title("A curtain wall with no code", "Meshcraft tutorials · 6")
    d.say("Start with the Architecture template.")
    d.click_mark(*TEMPLATE_CARD)
    d.command("file.new", preset="architecture_metric")
    d.command("view.selection")
    d.pose((19, 13, 29), (8, 3.5, 0))
    d.hold()

    d.say("Draw a wall to hold the panel while we build it.")
    d.tool("tool.builtin.arch_wall", tab=CIVIL)
    d.pick((12, 0, 0), GRID)
    d.pick((0, 0, 0), GRID)
    d.enter()
    d.escape()
    d.require(len(owners(d)) == 1, "the holding wall")

    d.say("Place axis helpers at the base, one at each side of the panel.")
    d.helper((0, 0, 0))
    d.helper((W, 0, 0))
    d.say("The Window tool gives us a frame and glass in two clicks.")
    d.tool("tool.builtin.arch_window_frame", tab=CIVIL)
    d.pick((W, H, 0), ON_HELPER)
    d.pick((0, 0, 0))
    d.escape()
    frame = next(o for o in owners(d) if "kind" in o["record"])
    d.require(frame["record"]["kind"] == "WINDOW", "the window frame")
    d.hold()

    d.say("Good. Explode turns the window into plain geometry that hotspots can drive.")
    d.select([frame["target"]], around=(W / 2, H / 2, 0))
    d.tool("object.explode_selection", tab=OBJECTS)
    d.require(not [o for o in owners(d) if "kind" in o["record"]], "window exploded")
    d.hold()

    d.say("Delete the wall and clear the helpers, and we're left with the panel.")
    wall = owners(d)[0]
    d.select([wall["target"]], around=(9, H / 2, 0))
    d.command("edit.delete_selection")
    d.require(not owners(d), "wall deleted")
    d.tool("view.guides.clear", tab="Home")
    d.require(d.rpc.status()["constructionGuides"] == 0, "guides cleared")
    d.hold()

    d.say("Make Object, then Edit Prototype. Hotspots are defined inside the object.")
    d.rpc.select([])
    d.window(box((0, 0, -DEPTH), (W, H, DEPTH)))
    d.tool("object.create_from_selection", tab=OBJECTS, name="Curtain wall panel")
    panel = next(o for o in d.scene()["objects"] if o["name"] == "Curtain wall panel")
    path = panel["path"]
    d.require(d.root()["counts"]["triangles"] == 0, "all geometry went into the object")
    d.select([panel["target"]], around=(W / 2, H / 2, 0))
    d.tool("selection.edit_object", tab=OBJECTS)
    d.command("view.hotspot_settings")
    d.hold()

    d.say("First the size handle. Place Hotspot on the panel's right corner, set to Stretch.")
    d.tool("hotspot.begin_placement", tab=OBJECTS)
    d.pick((W, 0, 0))
    handles = geometry(d, path)[0]["hotspots"]
    d.require(len(handles) == 1, "the size hotspot")
    size_id = handles[0]["id"]
    d.command("hotspot.update", name="Size", operation="STRETCH", shape="DIAMOND")
    d.hold()

    d.say("Now the multiplier: another hotspot on the grid in front, named Count, set to Multiply Linear.")
    d.rpc.select([])
    d.tool("hotspot.begin_placement", tab=OBJECTS)
    d.pick((W, 0, AHEAD), GRID)
    count_id = next(h["id"] for h in geometry(d, path)[0]["hotspots"] if h["id"] != size_id)
    d.command("hotspot.update", name="Count", operation="MULTIPLY_LINEAR", shape="SQUARE")
    d.hold()

    d.say("Window-select the whole panel and attach it. This is what gets copied.")
    d.rpc.select([])
    d.window(box((0, 0, -DEPTH), (W, H, DEPTH)))
    _, primitives, _ = geometry(d, path)
    parts = [p["target"] for p in primitives if p["target"]["kind"] in ("segment", "triangle")]
    d.rpc.select([hotspot(path, count_id)] + parts)
    d.tool("hotspot.attach", tab=OBJECTS)
    d.hold()

    d.say("Pick Reference, one panel width back. That distance is the step between copies.")
    d.rpc.select([hotspot(path, count_id)])
    d.tool("hotspot.reference", tab=OBJECTS)
    d.pick((0, 0, AHEAD), GRID)
    d.hold()

    d.say("Back to Size: select the panel's right side and the Count hotspot too, then attach them.")
    d.rpc.select([])
    d.window(box((W - STILE, 0, -DEPTH), (W, H, DEPTH)), margin=8)
    right_side = [t for t in d.selected() if t["kind"] in ("segment", "triangle", "vertex")]
    d.require(right_side, "the right side of the panel")
    # The window takes only what lies wholly inside it, so add the glass's right corners too;
    # without them the frame would stretch and leave the glass behind.
    corners = [v["target"] for v in geometry(d, path)[2]
               if v["position"]["x"] >= W - STILE - 1e-6]
    d.require(corners, "the corners on the right side")
    d.rpc.select([hotspot(path, size_id)] + right_side + corners + [hotspot(path, count_id)])
    d.tool("hotspot.attach", tab=OBJECTS)
    d.say("Attaching Count means the copies will follow when the panel gets wider.")
    handles = {h["name"]: h for h in geometry(d, path)[0]["hotspots"]}
    size, count = handles["Size"], handles["Count"]
    d.require(size["attachedHotspotIds"] == [count_id] and size["attachedSegments"], f"Size holds: {size}")
    d.require(len(count["attachedSegments"]) > len(size["attachedSegments"]) and not count["attachedHotspotIds"],
              f"Count holds the panel: {count}")
    # Stored in the object's local frame, so compare the reference with the marker.
    reference = count.get("referencePosition") or {}
    d.require(reference.get("z") == count["position"]["z"]
              and abs(count["position"]["x"] - reference.get("x", 0) - W) < 1e-6, f"reference: {count}")
    d.hold()

    d.say("Nice. Finish Editing. That's the whole definition, with no code at all.")
    d.tool("selection.exit_context", tab=OBJECTS)
    d.rpc.select([])
    d.hold()

    d.say("Select the panel, and both handles appear. Drag Size to make it wider.")
    d.click((W - 2, H / 2, 0))
    d.drag((W, 0, 0), (WIDER, 0, 0), GRID)
    top = settled_max_x(d, path, WIDER)
    d.require(abs(top - WIDER) < 1e-6, f"panel widened to {WIDER}, got {top}")
    # Everything on the right side moved, so nothing is left at the old width, glass included.
    left_behind = sorted({round(v["position"]["x"], 3) for v in geometry(d, path)[2]
                          if W - 2 * STILE < v["position"]["x"] < WIDER - 2 * STILE})
    d.require(not left_behind, f"geometry left at the old width: {left_behind}")
    d.hold()

    d.say("Now pull back to see more, and drag Count along the grid. Each panel width adds a panel.")
    d.fly((30, 26, 70), (18, 2, 6))
    d.drag((WIDER, 0, AHEAD), (WIDER * (PANELS - 1), 0, AHEAD), GRID)
    top = settled_max_x(d, path, WIDER * PANELS)
    d.require(abs(top - WIDER * PANELS) < 1e-6, f"{PANELS} panels wide, got {top}")
    d.say("Wow! A whole curtain wall from one drag.")
    d.hold()

    d.say("Move Copy it, and each copy keeps its own size and count.")
    d.click((WIDER / 2, H / 2, 0))
    d.tool("tool.move_copy")
    d.pick((0, 0, 0))
    d.pick((0, 0, 16), GRID)
    d.escape()
    placed = [o for o in d.scene()["objects"] if o["name"] == "Curtain wall panel"]
    d.require(len(placed) == 2, f"two curtain walls, got {len(placed)}")
    second = next(o for o in placed if o["path"] != path)
    d.say("Drag this one back to three panels. The first doesn't change.")
    d.click((WIDER / 2, H / 2, 16))
    d.drag((WIDER * (PANELS - 1), 0, AHEAD + 16), (WIDER * 2, 0, AHEAD + 16), GRID)
    d.require(abs(settled_max_x(d, second["path"], WIDER * 3) - WIDER * 3) < 1e-6, "copy is three panels")
    d.require(abs(settled_max_x(d, path, WIDER * PANELS) - WIDER * PANELS) < 1e-6, "first wall unchanged")
    d.rpc.select([])
    d.hold()

    d.say("That's a dynamic object with no code: one size handle and one multiply handle.")
    d.orbit(-120, 25)
    d.finish("Next: an office floor", "Meshcraft tutorials · 7")
