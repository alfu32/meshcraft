"""Tutorial 7: a 42 × 18 m office floor built from user-defined dynamic structural and facade modules."""

import time

W, D, H, BAY = 140, 60, 12, 20  # Architecture units: 30 cm. Seven by three 6 m bays.
CIVIL, OBJECTS = "Civil", "Objects & Hotspots"
GRID = ("Grid", "Grid Line", "Axis Guide")
CORNER = ("Endpoint",)
UP = ("Axis Guide",)


def setup(d):
    d.video_crf = 24
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("file.home", open=True)


def box(low, high):
    return [(x, y, z) for x in (low[0], high[0])
            for y in (low[1], high[1]) for z in (low[2], high[2])]


def geometry(d, path):
    offset, primitives, vertices = 0, [], []
    while offset is not None:
        page = d.rpc.call("meshcraft_geometry", dict(path=path, offset=offset, limit=1000))
        primitives += page["primitives"]
        vertices += page["vertices"]
        offset = page.get("nextOffset")
    return page, primitives, vertices


def owners(d):
    return geometry(d, d.root()["path"])[0]["architecture"]


def hotspot(path, ident):
    return dict(path=path, kind="semantic_handle", semantic_kind="hotspot", id=ident)


def cuboid(d, low, high, helper=None, clear_guides=True):
    if helper is None:
        d.fly((30, 32, 42), (10, 10, 10))
    d.tool("tool.cuboid", tab="Solids")
    d.pick(low)
    if helper is not None:
        d.helper(helper)
    if helper is None:
        d.fly((high[0] + 10, high[1] + 14, high[2] + 16), (high[0], high[1] - .5, high[2]))
    d.pick(high, UP if helper is not None else ("Endpoint", "Grid", "Line", "Midpoint"))
    d.escape()
    if clear_guides:
        d.tool("view.guides.clear")


def settled(d, path, triangles):
    deadline = time.monotonic() + 20
    while True:
        obj = next(o for o in d.scene()["objects"] if o["path"] == path)
        if obj["counts"]["triangles"] == triangles or time.monotonic() > deadline:
            return obj
        time.sleep(.15)


def structure(d):
    d.say("Start with Architecture: one unit is 30 centimetres. Use a half-unit grid for the partitions.")
    d.click_mark(750, 266)
    d.command("file.new", preset="architecture_metric")
    d.command("view.model_settings")
    d.field("Grid size", "0.5")
    settings = d.scene()["settings"]
    d.require(settings["unit_size"] == 30 and settings["grid_size"] == .5, "30 cm units, 15 cm grid")
    d.pose((200, 180, 220), (70, 0, 30))
    d.hold()

    d.say("Draw the slab, forty-two by eighteen metres: 140 by 60 units.")
    d.tool("tool.builtin.arch_slab", tab=CIVIL)
    d.pick((0, 0, 0), GRID)
    d.pick((W, 0, D), GRID)
    d.escape()
    d.require(len(owners(d)) == 1, "one floor slab")
    floor_triangles = d.root()["counts"]["triangles"]
    d.fly((50, 42, 60), (10, 4, 10))

    d.say("Build one six metre square frame. Four corner columns come first, using Cuboid and axis helpers.")
    for x, z in ((0, 0), (19, 0), (0, 19), (19, 19)):
        d.fly((x + 13, 18, z + 22), (x + .5, 4, z + .5))
        if (x, z) == (0, 0):
            d.helper((0, 0, 0), CORNER)
        cuboid(d, (x, 0, z), (x + 1, 11, z + 1), (x + 1, 0, z + 1), clear_guides=False)
    d.tool("view.guides.clear")
    d.require(d.root()["counts"]["triangles"] == floor_triangles + 48, "four corner columns")

    d.fly((30, 32, 42), (10, 10, 10))
    d.say("Nice. Add the beams at the column tops. Twelve units overall gives a height of 3.6 metres.")
    cuboid(d, (0, 11, 0), (20, 12, 1), (20, 11, 1))
    cuboid(d, (0, 11, 19), (20, 12, 20), (20, 11, 20))
    cuboid(d, (0, 11, 1), (1, 12, 19))
    cuboid(d, (19, 11, 1), (20, 12, 19))
    d.require(d.root()["counts"]["triangles"] == floor_triangles + 96, "four columns and four beams")

    d.fly((50, 42, 60), (10, 4, 10))
    d.say("Window-select the frame and Make Object. Then Edit Prototype to define its array handle.")
    d.rpc.select([])
    d.window(box((0, 0, 0), (20, 12, 20)), margin=10)
    d.tool("object.create_from_selection", tab=OBJECTS, name="Structure 6 m bay")
    frame = next(o for o in d.scene()["objects"] if o["name"] == "Structure 6 m bay")
    d.require(frame["counts"]["triangles"] == 96, "the object contains all eight structural members")
    d.require(d.root()["counts"]["triangles"] == floor_triangles and len(owners(d)) == 1,
              "slab remains separate; no structural geometry left in root")
    d.select([frame["target"]], around=(10, 8, 20))
    d.tool("selection.edit_object", tab=OBJECTS)
    d.command("view.hotspot_settings")
    d.fly((85, 65, 100), (20, 3, 20))

    d.say("Place the hotspot above the second grid diagonal, on an axis helper. Choose Multiply Volumetric.")
    d.helper((40, 0, 40), GRID)
    d.tool("hotspot.begin_placement", tab=OBJECTS)
    d.pick((40, H, 40), UP)
    handle = geometry(d, frame["path"])[0]["hotspots"][0]
    d.command("hotspot.update", name="Structural grid", operation="MULTIPLY_VOLUMETRIC", shape="SQUARE")

    d.say("Attach every beam and column. Pick Reference at the far corner of the first six metre bay.")
    page, primitives, _ = geometry(d, frame["path"])
    parts = [p["target"] for p in primitives if p["target"]["kind"] in ("segment", "triangle")]
    d.select([hotspot(frame["path"], handle["id"])] + parts, around=(10, 8, 10))
    d.tool("hotspot.attach", tab=OBJECTS)
    d.rpc.select([hotspot(frame["path"], handle["id"])])
    d.tool("hotspot.reference", tab=OBJECTS)
    d.pick((20, 0, 20), CORNER)
    stored = geometry(d, frame["path"])[0]["hotspots"][0]
    ref = stored["referencePosition"]
    d.require(abs(stored["position"]["x"] - ref["x"] - BAY) < 1e-6
              and abs(stored["position"]["z"] - ref["z"] - BAY) < 1e-6
              and abs(stored["position"]["y"] - ref["y"] - H) < 1e-6,
              "volumetric step is 6 × 6 m in plan and 3.6 m vertically")
    d.tool("selection.exit_context", tab=OBJECTS)
    d.rpc.select([])

    d.say("Good. Select the instance and drag its hotspot to the opposite corner of the slab.")
    d.fly((150, 130, 170), (70, 0, 30))
    d.select([frame["target"]], around=(10, 8, 20))
    d.drag((40, H, 40), (W, 0, D), CORNER)
    placed = settled(d, frame["path"], 96 * 21)
    d.require(placed["counts"]["triangles"] == 96 * 21, "exactly 7 × 3 complete frame modules")
    b = placed["bounds"]
    d.require(abs(b["min"]["x"]) + abs(b["min"]["z"])
              + abs(b["max"]["x"] - W) + abs(b["max"]["z"] - D)
              + abs(b["max"]["y"] - H) < 1e-6, "structure fills 42 × 18 m at 3.6 m height")
    d.rpc.select([])
    d.tool("view.guides.clear")
    d.say("Twenty-one structural modules: seven across and three deep, all driven by one instance hotspot.")
    d.hold()
    return frame


def flight_from_contour(d, contour, path):
    d.rpc.select([])
    d.tool("tool.builtin.line")
    for point in path:
        d.pick(point)
    d.escape()
    walking = d.selected()
    d.select(contour)
    d.tool("architecture.stair_contour", tab=CIVIL)
    d.select(walking)
    d.tool("architecture.stair_path", tab=CIVIL)
    stair = d.selected()
    d.require(len(stair) == 1 and stair[0].get("semantic_kind") == "stair", "one parametric flight")
    d.wait(1.2)
    d.select(walking)
    d.command("edit.delete_selection")
    d.rpc.select([])
    return stair[0]


def stairs(d):
    d.say("For the core stairs, set flight height to six units and the step count to twelve.")
    d.fly((53, 60, 30.5), (53, 0, 30))
    d.command("view.architecture_settings")
    d.command("architecture.settings", stair_height=6, stair_steps=12, stair_support=.5,
              stair_left=False, stair_right=False)
    d.say("Draw a rectangular contour C and Make Object, so the same outline can define both flights.")
    d.tool("tool.builtin.rectangle")
    d.pick((48, 0, 22))
    d.pick((52, 0, 34))
    d.escape()
    d.tool("object.create_from_selection", tab=OBJECTS, name="Stair contour C")
    lower_c = d.selected()
    d.say("Add helpers for the contour copy and the landing. Copy the contour six units across and up.")
    d.helper((54, 0, 22))
    d.helper((48, 0, 38))
    d.helper((58, 0, 34))
    d.select(lower_c, around=(50, 0, 28))
    d.tool("tool.move_copy")
    d.pick((48, 0, 22), CORNER)
    d.pick((54, 6, 22), UP)
    d.escape()
    upper_c = d.selected()
    d.require(upper_c != lower_c and len(upper_c) == 1, "independent elevated contour instance")
    d.say("Nice. Draw walking path P in the lower outline. Set C, then P, to create twelve risers.")
    lower = flight_from_contour(d, lower_c, ((50, 0, 22.5), (50, 0, 33.5)))
    d.say("Use the landing helpers at six units: half the 3.6 metre storey height.")
    d.tool("tool.builtin.arch_slab", tab=CIVIL)
    d.pick((48, 6, 38), UP)
    d.pick((58, 6, 34), CORNER + UP)
    landing = d.selected()[0]
    d.escape()
    d.tool("view.guides.clear")
    d.say("Reverse the walking path in the upper contour, then set C and P for the return flight.")
    upper = flight_from_contour(d, upper_c, ((56, 6, 33.5), (56, 6, 22.5)))
    d.select(lower_c + upper_c)
    d.command("edit.delete_selection")
    flights = [o["record"] for o in owners(d) if "walkingPath" in o["record"]]
    d.require(len(flights) == 2 and all(o["height"] == 6 and o["stepCount"] == 12 for o in flights),
              "two half-storey flights, each with twelve 150 mm risers")
    d.require(sorted(round(o["contour"][0]["y"]) for o in flights) == [0, 6],
              "return flight starts at the landing and reaches 3.6 m")
    d.say("Hide Objects in the selection filters while drafting the core, to keep the floor grid accessible.")
    d.command("view.selection")
    d.click_mark(1840, 372)
    d.command("selection.update_display", kind="object", property="display", enabled=False)
    d.say("Select both flights and the landing. Move Copy places the second evacuation stair in the east bay.")
    d.fly((70, 110, 30.5), (70, 0, 30))
    d.select([lower, upper, landing], around=(53, 6, 30))
    d.tool("tool.move_copy")
    d.pick((46, 0, 22), GRID)
    d.pick((81, 0, 22), GRID)
    d.escape()
    flights = [o["record"] for o in owners(d) if "walkingPath" in o["record"]]
    d.require(len(flights) == 4, "two complete two-flight evacuation staircases")
    d.require(sorted(round(min(p["x"] for p in o["contour"])) for o in flights) == [48, 54, 83, 89],
              "stairs occupy the C–D and E–F bays")
    d.rpc.select([])


def walls(d):
    return [o for o in owners(d) if "start" in o["record"]]


def chain(d, points):
    cx = sum(x for x, z in points) / len(points)
    cz = sum(z for x, z in points) / len(points)
    d.fly((cx, 70, cz + .5), (cx, 0, cz))
    d.tool("tool.builtin.arch_wall", tab=CIVIL)
    for x, z in points:
        d.pick((x, 0, z))
    d.enter()
    d.escape()


def core(d):
    d.say("Set partition thickness to half a unit: fifteen centimetres. The offset is a quarter unit.")
    d.command("architecture.settings", wall_height=12, wall_thickness=.5, wall_offset=.25)
    d.say("Draw the two lift cases on the left of the central lift hall.")
    chain(d, [(61, 30), (67, 30)])
    chain(d, [(61, 21), (67, 21), (67, 39), (61, 39), (61, 21)])
    bank = [o["target"] for o in walls(d)]
    d.require(len(bank) == 5, "one divided two-lift bank")
    d.say("Move Copy repeats the partition assembly on the right: four lift cases around one shared hall.")
    d.fly((70, 90, 30.5), (70, 0, 30))
    d.select(bank, around=(64, 5, 30))
    d.tool("tool.move_copy")
    d.pick((64, 0, 18), GRID)
    d.pick((76, 0, 18), GRID)
    d.escape()
    d.require(len(walls(d)) == 10, "two matching lift banks, five partition pieces each")
    d.say("Good. Parallel partitions separate the stair enclosures from their outer side halls.")
    for x in (46, 94, 41, 99):
        chain(d, [(x, 21), (x, 39)])
    d.say("Enclose the stair bays at north and south. The middle hall remains open at both ends.")
    for points in ([(46, 21), (61, 21)], [(46, 39), (61, 39)],
                   [(79, 21), (94, 21)], [(79, 39), (94, 39)]):
        chain(d, points)
    d.say("To the west, divide the service bay into women's and men's toilets, accessed from the side hall.")
    for points in ([(21, 30), (41, 30)], [(21, 21), (41, 21)],
                   [(21, 39), (41, 39)], [(21, 21), (21, 39)]):
        chain(d, points)
    d.say("To the east, put storage and copying north of the kitchen, then a meeting room in the end bay.")
    for points in ([(99, 30), (121, 30)], [(110, 21), (110, 30)],
                   [(121, 21), (121, 39)], [(99, 21), (139, 21)], [(99, 39), (139, 39)]):
        chain(d, points)
    d.require(len(walls(d)) == 27 and all(o["record"]["thickness"] == .5 for o in walls(d)),
              "27 core partition pieces, all 15 cm thick")
    d.rpc.select([])


def vertical_opening(d, x, z, side):
    d.fly((x + side * 4, 25, z + 6), (x, 4, z + 1.5))
    d.rpc.select([])
    for jamb in (z, z + 3):
        d.helper((x, 0, jamb))
    d.tool("tool.builtin.arch_add_hole", tab=CIVIL)
    d.pick((x, 7, z + 3), UP)
    d.pick((x, 0, z))
    d.escape()
    d.tool("view.guides.clear")


def doors(d):
    d.say("Cut the lift openings on the wall faces. Axis helpers carry their 2.1 metre heads.")
    positions = [(67, 24, 1), (67, 33, 1), (73, 24, -1), (73, 33, -1)]
    for x, z, side in positions:
        vertical_opening(d, x, z, side)
    d.say("Give each toilet its own door from the west side hall.")
    for z in (24, 33):
        vertical_opening(d, 41, z, 1)
        positions.append((41, z, 1))
    d.say("Good. Add doors between the side halls and the two protected staircase enclosures.")
    for x, side in ((45.5, -1), (94, 1)):
        vertical_opening(d, x, 24, side)
        positions.append((x, 24, side))
    d.say("Storage and kitchen open from the east hall; the copier room connects through storage.")
    for x, z, side in ((98.5, 24, -1), (98.5, 33, -1), (109.5, 24, -1)):
        vertical_opening(d, x, z, side)
        positions.append((x, z, side))
    d.require(sum(len(o["record"]["holes"]) for o in walls(d)) == 11,
              "eleven independent wall openings")
    d.say("Door fits the first lift opening: ninety centimetres wide and 2.1 metres high.")
    d.fly((71, 25, 30), (67, 4, 27))
    d.tool("tool.builtin.arch_door_frame", tab=CIVIL)
    d.pick((67, 7, 27), CORNER)
    d.pick((67, 0, 24), CORNER)
    d.escape()
    prototype = [o["target"] for o in owners(d) if o["record"].get("kind") == "DOOR"]
    d.require(len(prototype) == 1, "one canonical door before copying")
    d.say("Move Copy repeats that door. Use matching floor-grid references in the clear open space.")
    d.fly((70, 140, 50.5), (70, 0, 50))
    for x, z, _ in positions[1:]:
        d.select(prototype, around=(67, 4, 25.5))
        d.tool("tool.move_copy")
        d.pick((67, 0, 45), GRID)
        d.pick((x, 0, 45 + z - 24), GRID)
        d.escape()
    repeated = [o["record"] for o in owners(d) if o["record"].get("kind") == "DOOR"]
    d.require(len(repeated) == 11 and all(abs(max(p["z"] for p in o["contour"])
                  - min(p["z"] for p in o["contour"]) - 3) < 1e-6 for o in repeated),
              "eleven matching 900 mm door frames")
    d.say("The meeting room gets a wider entrance from the north open space.")
    d.fly((130, 28, 15), (130, 4, 21))
    for x in (128, 134):
        d.helper((x, 0, 21))
    d.tool("tool.builtin.arch_add_hole", tab=CIVIL)
    d.pick((134, 7, 21), UP)
    d.pick((128, 0, 21))
    d.escape()
    d.tool("view.guides.clear")
    d.tool("tool.builtin.arch_door_frame", tab=CIVIL)
    d.pick((134, 7, 21), CORNER)
    d.pick((128, 0, 21), CORNER)
    d.escape()
    d.require(sum(len(o["record"]["holes"]) for o in walls(d)) == 12
              and len([o for o in owners(d) if o["record"].get("kind") == "DOOR"]) == 12,
              "all twelve openings have separate door components")
    d.rpc.select([])


def facade(d):
    d.say("Show Objects and lock the labels. On a quarter-unit grid, build two mullions with no top rail.")
    d.command("view.selection")
    d.click_mark(1840, 372)
    d.command("selection.update_display", kind="object", property="display", enabled=True)
    d.click_mark(1866, 400)
    d.command("selection.update_display", kind="dimensions_texts", property="modifiable", enabled=False)
    d.command("view.model_settings")
    d.field("Grid size", "0.25")
    d.require(d.scene()["settings"]["grid_size"] == .25, "7.5 cm facade construction grid")
    d.fly((12, 17, 90), (2.5, 5, 65))
    before = d.root()["counts"]["triangles"]
    for x in (0, 4.75):
        cuboid(d, (x, 0, 65), (x + .25, H, 65.5), (x + .25, 0, 65.5))
    d.say("Join their inner corners with a closed Polyline. The glazing fills the full height.")
    d.tool("tool.builtin.polyline")
    for point in ((.25, 0, 65.25), (4.75, 0, 65.25), (4.75, H, 65.25), (.25, H, 65.25), (.25, 0, 65.25)):
        d.pick(point)
    d.escape()
    glass = [t for t in d.selected() if t["kind"] == "triangle"]
    d.require(len(glass) == 2, "one glazing quad, without a duplicate fill")
    d.select(glass, around=(2.5, 6, 65.5))
    d.tool("material.apply", tab="Materials", id="glass")
    d.require(d.root()["counts"]["triangles"] == before + 26, "two mullions and one glazing face")
    d.say("Make Object, and Edit Prototype. This panel is one and a half metres wide.")
    d.rpc.select([])
    d.window(box((0, 0, 65), (5, H, 66)), margin=8)
    d.tool("object.create_from_selection", tab=OBJECTS, name="Curtain wall 1.5 m panel")
    panel = next(o for o in d.scene()["objects"] if o["name"] == "Curtain wall 1.5 m panel")
    d.require(panel["counts"]["triangles"] == 26, "panel has only the two vertical members and glass")
    d.fly((30, 25, 98), (10, 3, 65))
    d.select([panel["target"]], around=(2.5, 6, 65.5))
    d.tool("selection.edit_object", tab=OBJECTS)
    d.say("Add a Multiply Linear hotspot, attach the panel, and pick a reference one panel width back.")
    d.tool("hotspot.begin_placement", tab=OBJECTS)
    d.pick((10, 0, 68), GRID)
    handle = geometry(d, panel["path"])[0]["hotspots"][0]
    d.command("hotspot.update", name="Facade panels", operation="MULTIPLY_LINEAR", shape="SQUARE")
    parts = [p["target"] for p in geometry(d, panel["path"])[1]
             if p["target"]["kind"] in ("segment", "triangle")]
    d.select([hotspot(panel["path"], handle["id"])] + parts, around=(2.5, 6, 65.5))
    d.tool("hotspot.attach", tab=OBJECTS)
    d.rpc.select([hotspot(panel["path"], handle["id"])])
    d.tool("hotspot.reference", tab=OBJECTS)
    d.pick((5, 0, 68), GRID)
    d.tool("selection.exit_context", tab=OBJECTS)
    d.rpc.select([])
    d.say("Nice. Drag the instance handle across the frontage: twenty-eight glazed panels.")
    d.fly((150, 130, 190), (70, 0, 65))
    d.select([panel["target"]], around=(3, 6, 65.5))
    d.drag((10, 0, 68), (W, 0, 68), GRID)
    placed = settled(d, panel["path"], 26 * 28)
    d.require(placed["counts"]["triangles"] == 26 * 28, "28 panels across the 42 m frontage")
    d.say("Move the assembled curtain wall onto the south edge, using its corner and a helper.")
    d.fly((158, 25, 80), (140, 0, 63))
    d.helper((140, 0, 65.5))
    d.select([panel["target"]], around=(139, 5, 65.25))
    d.tool("tool.builtin.move")
    d.pick((140, 0, 65.5))
    d.pick((140, 0, 60.5), UP)
    d.escape()
    d.tool("view.guides.clear")
    moved = next(o for o in d.scene()["objects"] if o["path"] == panel["path"])
    d.require(abs(moved["bounds"]["min"]["z"] - D) < 1e-6, "south facade on row 4")
    d.say("Copy the wall to the north edge. Both facades share one definition. Then unlock the labels.")
    d.fly((180, 130, 100), (130, 0, 30))
    d.select([panel["target"]], around=(139, 5, 60.25))
    d.tool("tool.move_copy")
    d.pick((145, 0, 65.5), GRID)
    d.pick((145, 0, 5), GRID)
    d.escape()
    facades = [o for o in d.scene()["objects"] if o["name"] == panel["name"]]
    d.require(len(facades) == 2 and len({o["definition_id"] for o in facades}) == 1,
              "north and south facades share the panel definition")
    d.require(sorted(round(o["bounds"]["min"]["z"], 3) for o in facades) == [-.5, 60]
              and all(o["counts"]["triangles"] == 26 * 28 for o in facades),
              "both complete curtain walls occupy only north and south edges")
    d.require(len(geometry(d, d.root()["path"])[0]["annotations"]) == 31,
              "facade object creation preserves every grid and room label in the document")
    d.rpc.select([])
    d.command("view.selection")
    d.click_mark(1866, 400)
    d.command("selection.update_display", kind="dimensions_texts", property="modifiable", enabled=True)
    d.fly((180, 180, 190), (70, 3, 30))
    d.wait(1)
    return facades


def side_walls(d):
    d.say("The east and west sides are plain walls, one unit thick: thirty centimetres, with no windows.")
    d.command("architecture.settings", wall_height=H, wall_thickness=1, wall_offset=.5)
    d.fly((70, 230, 30.5), (70, 0, 30))
    for points in (((0, 0, D), (0, 0, 0)), ((W, 0, 0), (W, 0, D))):
        d.tool("tool.builtin.arch_wall", tab=CIVIL)
        for point in points:
            d.pick(point)
        d.enter()
        d.escape()
    plain = [o["record"] for o in walls(d) if o["record"]["thickness"] == 1]
    d.require(len(plain) == 2 and all(not o["holes"] and o["height"] == H for o in plain),
              "two solid 30 cm side walls at storey height")
    bounds = d.root()["bounds"]
    d.require(bounds["min"]["x"] >= -1e-6 and bounds["max"]["x"] <= W + 1e-6,
              "the plain walls stay inside the slab width")


def labels(d):
    d.say("Label axes A to H and rows one to four. Each bay is six metres square.")
    d.fly((70, 250, 30.5), (70, 0, 30))
    grid = [(letter, (i * BAY, 0, -6)) for i, letter in enumerate("ABCDEFGH")]
    grid += [(str(i + 1), (-6, 0, i * BAY)) for i in range(4)]
    rooms = [("WC Women", (30, 0, 25)), ("WC Men", (30, 0, 35)),
             ("H FR30", (43.5, 0, 31.5)), ("St FR90", (53, 0, 29)),
             ("L1", (64, 0, 25)), ("L2", (64, 0, 35)), ("HL FR90", (70, 0, 30)),
             ("L4", (76, 0, 25)), ("L3", (76, 0, 35)), ("St FR90", (88, 0, 29)),
             ("H FR30", (97, 0, 31.5)), ("Storage", (104.5, 0, 25)),
             ("Copier", (115, 0, 25)), ("Kitchen", (110, 0, 35)), ("Meeting", (130, 0, 30)),
             ("Open office", (70, 0, 10)), ("Open office", (70, 0, 50)),
             ("CWN", (70, 0, -3)), ("CWS", (70, 0, 63))]
    for index, (text, point) in enumerate(grid):
        if index == 8:
            d.fly((-30, 130, 30.5), (0, 0, 30))
        d.rpc.select([])
        d.command("annotation.update", text=text, size=1)
        d.tool("tool.builtin.text")
        d.pick(point, GRID)
        d.escape()
    d.say("Mark the lift and stair enclosures FR90, and the side halls FR30, following the plan.")
    d.fly((70, 250, 30.5), (70, 0, 30))
    for text, point in rooms:
        d.fly((point[0], 50, point[2] + .25), point)
        d.rpc.select([])
        d.command("annotation.update", text=text, size=.65)
        d.tool("tool.builtin.text")
        d.pick(point)
        d.escape()
    annotations = geometry(d, d.root()["path"])[0]["annotations"]
    d.require(len(annotations) == len(grid) + len(rooms), "all grid and room labels are present")
    d.fly((70, 180, 30.5), (70, 0, 30))
    d.say("Keep the north and south bands as open offices, connected around this central service core.")
    d.rpc.select([])


def ceiling(d):
    d.say("For the ceiling, make one reusable sixty centimetre square panel in the open office zone.")
    d.fly((16, 42, 15), (8, 0, 6))
    before = d.root()["counts"]["triangles"]
    d.tool("tool.builtin.rectangle")
    d.pick((4, 0, 4), GRID)
    d.pick((6, 0, 6), GRID)
    d.escape()
    d.tool("object.create_from_selection", tab=OBJECTS, name="Ceiling panel 600 mm")
    panel = next(o for o in d.scene()["objects"] if o["name"] == "Ceiling panel 600 mm")
    d.require(panel["counts"]["triangles"] == 2 and d.root()["counts"]["triangles"] == before,
              "one reusable ceiling quad")
    d.say("Edit Prototype. A volumetric array hotspot repeats the panel in both plan directions.")
    d.tool("selection.edit_object", tab=OBJECTS)
    d.tool("hotspot.begin_placement", tab=OBJECTS)
    d.pick((8, 0, 8), GRID)
    handle = geometry(d, panel["path"])[0]["hotspots"][0]
    d.command("hotspot.update", name="Ceiling grid", operation="MULTIPLY_VOLUMETRIC", shape="SQUARE")
    parts = [p["target"] for p in geometry(d, panel["path"])[1]
             if p["target"]["kind"] in ("segment", "triangle")]
    d.select([hotspot(panel["path"], handle["id"])] + parts, around=(5, 0, 5))
    d.tool("hotspot.attach", tab=OBJECTS)
    d.rpc.select([hotspot(panel["path"], handle["id"])])
    d.tool("hotspot.reference", tab=OBJECTS)
    d.pick((6, 0, 6), CORNER)
    d.tool("selection.exit_context", tab=OBJECTS)
    d.rpc.select([])
    d.say("Drag the instance handle to form two rows of twenty panels, then lift the assembly on a helper.")
    d.fly((44, 100, 8.5), (44, 0, 8))
    d.select([panel["target"]], around=(5, 0, 5))
    d.drag((8, 0, 8), (44, 0, 8), GRID)
    placed = settled(d, panel["path"], 80)
    d.require(placed["counts"]["triangles"] == 80, "40 ceiling panels at 600 mm pitch")
    d.fly((25, 65, 30), (24, 5, 6))
    d.helper((4, 0, 4), CORNER)
    d.select([panel["target"]], around=(5, 0, 5))
    d.tool("tool.builtin.move")
    d.pick((4, 0, 4), CORNER)
    d.pick((4, 10, 4), UP)
    d.escape()
    d.tool("view.guides.clear")
    placed = next(o for o in d.scene()["objects"] if o["path"] == panel["path"])
    d.require(abs(placed["bounds"]["min"]["y"] - 10) < 1e-6
              and abs(placed["bounds"]["max"]["y"] - 10) < 1e-6,
              "ceiling strip at 3 m, below the structural beams")
    d.rpc.select([])


def record(d):
    d.title("An office floor: 42 × 18 metres", "Meshcraft tutorials · 7 · dynamic structural grid")
    structure(d)
    d.title("The central circulation core", "Four lifts, two stairs and service rooms")
    stairs(d)
    core(d)
    doors(d)
    side_walls(d)
    labels(d)
    d.title("The repeating curtain walls", "A user-defined glazed panel")
    facade(d)
    d.title("A partial ceiling grid", "Reusable 600 mm panels")
    ceiling(d)
    d.tool("view.grid.toggle")
    d.escape()
    d.command("view.selection")
    d.fly((180, 180, 190), (70, 3, 30))
    d.say("Good. Seven by three structural bays, four lifts, two staircases and open offices around one central core.")
    d.hold()
    d.orbit(-50, 12)
    d.escape()
    d.say("The partial ceiling leaves the layout visible. The structural and facade grids remain hotspot-driven.")
    d.hold()
    bounds = [o["bounds"] for o in d.scene()["objects"] if o.get("bounds")]
    low = tuple(min(b["min"][axis] for b in bounds) for axis in "xyz")
    high = tuple(max(b["max"][axis] for b in bounds) for axis in "xyz")
    view = d.rpc.status()["workspace"]["viewport"]
    d.require(all(view["x"] + 20 <= x <= view["x"] + view["width"] - 20
                  and view["y"] + 20 <= y <= view["y"] + view["height"] - 20
                  for x, y in (d.project(p) for p in box(low, high))),
              "the complete floor, facades and partial ceiling stay within the final viewport")
    d.finish("Thanks for watching", "Meshcraft tutorials · 7")
