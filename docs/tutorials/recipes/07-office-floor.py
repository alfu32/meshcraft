"""Tutorial 7: a 27 × 15 m office floor with one circulation lobby and repeated components."""

W, D, H = 90, 50, 9  # Architecture units: 30 cm, never metre-sized defaults.
CIVIL = "Civil"
GRID = ("Grid", "Grid Line", "Axis Guide")
CORNER = ("Endpoint",)
ON_HELPER = ("Axis Guide",)


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("file.home", open=True)


def owners(d):
    return d.rpc.call("meshcraft_geometry", dict(path=[], limit=1))["architecture"]


def walls(d):
    return [o for o in owners(d) if "start" in o["record"]]


def chain(d, points):
    d.tool("tool.builtin.arch_wall", tab=CIVIL)
    for x, z in points:
        d.pick((x, 0, z))
    d.enter()
    d.escape()


def record(d):
    d.title("An office floor: 27 × 15 metres", "Meshcraft tutorials · 7")
    d.say("Start with Architecture. One unit is 30 cm, so our floor is 90 by 50 units.")
    d.click_mark(750, 266)
    d.command("file.new", preset="architecture_metric")
    d.command("view.model_settings")
    d.pose((60, 165, 55), (45, 0, 25))
    settings = d.scene()["settings"]
    d.require(settings["unit_size"] == 30 and settings["grid_size"] == 1, "Architecture scale")
    d.hold()

    d.say("Choose Rectangular Slab, and snap out the 27 by 15 metre footprint.")
    d.tool("tool.builtin.arch_slab", tab=CIVIL)
    d.pick((0, 0, 0), GRID)
    d.pick((W, 0, D), GRID)
    d.escape()
    slab = owners(d)[0]["record"]
    d.require(max(p["x"] for p in slab["profile"]["points"]) == W
              and max(p["z"] for p in slab["profile"]["points"]) == D, "27 × 15 m slab")

    d.say("Wall draws the perimeter as a chain. Enter finishes it.")
    chain(d, [(0, 0), (90, 0), (90, 50), (0, 50), (0, 0)])
    d.require(len(walls(d)) == 4, "four perimeter walls")
    d.require(all(o["record"]["height"] == H and o["record"]["thickness"] == 1
                  for o in walls(d)), "template wall defaults retained")

    d.say("Nice. Two cross walls mark the north and south sides of a single central lobby.")
    for z in (15, 35):
        chain(d, [(89, z), (1, z)])
    d.require(len(walls(d)) == 6, "two lobby cross walls")

    d.say("Leave side passages in the lobby walls. All four office zones lead to this node.")
    for x in (30, 60):
        chain(d, [(x, 15), (x, 22)])
        chain(d, [(x, 28), (x, 35)])
    d.require(len(walls(d)) == 10, "central 9 × 6 m lobby with two 1.8 m side passages")

    d.fly((110, 95, 130), (45, 0, 25))
    d.say("Door openings need helpers. Press T on the wall base at each jamb.")
    for z in (15, 35, 50):
        for x in (42, 48):
            d.helper((x, 0, z))
        if z == 15:
            d.say("Add Hole: pick the top of one helper, then the opposite base on the wall.")
        d.tool("tool.builtin.arch_add_hole", tab=CIVIL)
        d.pick((48, 7, z), ON_HELPER)
        d.pick((42, 0, z))
        d.escape()
        d.tool("view.guides.clear")
    perforated = [o for o in walls(d) if o["record"]["holes"]]
    d.require(len(perforated) == 3, "three door openings")
    d.tool("view.guides.clear")

    d.say("Good. Door fits the first opening. These are 1.8 metres wide and 2.1 high.")
    d.tool("tool.builtin.arch_door_frame", tab=CIVIL)
    d.pick((48, 7, 15), CORNER)
    d.pick((42, 0, 15), CORNER)
    d.escape()
    doors = [o for o in owners(d) if o["record"].get("kind") == "DOOR"]
    d.require(len(doors) == 1, "one door module")
    door_target = doors[0]["target"]

    d.say("Select the door, then Move Copy between matching corners of the next opening.")
    d.select([door_target], around=(45, 4, 15))
    d.tool("tool.move_copy")
    d.pick((48, 7, 15), CORNER)
    d.pick((48, 7, 35), CORNER)
    d.escape()
    d.say("Repeat for the entrance. Copy the component instead of drawing each door again.")
    d.select([door_target], around=(45, 4, 15))
    d.tool("tool.move_copy")
    d.pick((48, 7, 15), CORNER)
    d.pick((48, 7, 50), CORNER)
    d.escape()
    doors = [o for o in owners(d) if o["record"].get("kind") == "DOOR"]
    d.require(len(doors) == 3, "three repeated doors")
    contours = [o["record"]["contour"] for o in doors]
    d.require(sorted(round(c[0]["z"]) for c in contours) == [15, 35, 50], "doors on opening planes")
    for contour in contours:
        d.require(max(p["x"] for p in contour) - min(p["x"] for p in contour) == 6
                  and max(p["y"] for p in contour) - min(p["y"] for p in contour) == 7,
                  "repeated doors stay 1.8 × 2.1 m")

    d.rpc.select([])

    d.fly((60, 165, 55), (45, 0, 25))
    d.say("For repeatable office bays, draw one short partition, leaving a shared access strip.")
    chain(d, [(30, 1), (30, 10)])
    partition = next(o for o in walls(d) if o["record"]["start"] == dict(x=30.0, y=0.0, z=1.0))
    d.say("Move Copy repeats the partition nine metres along the floor grid.")
    d.select([partition["target"]], around=(30, 4, 6))
    d.tool("tool.move_copy")
    d.pick((30, 0, 10))
    d.pick((60, 0, 10), GRID)
    d.escape()
    d.require(len(walls(d)) == 12, "two repeated north partitions")

    d.say("Cool. Copy both partitions to the south office zone, using the same grid spacing.")
    partitions = [o["target"] for o in walls(d) if o["record"]["start"]["x"] in (30, 60)
                  and o["record"]["start"]["z"] == 1]
    d.select(partitions, around=(45, 4, 5))
    d.tool("tool.move_copy")
    d.pick((30, 0, 10))
    d.pick((30, 0, 44), GRID)
    d.escape()
    d.require(len(walls(d)) == 14, "four repeatable partitions")
    repeated = [o["record"] for o in walls(d) if abs(o["record"]["end"]["z"]
                - o["record"]["start"]["z"]) == 9]
    d.require(sorted((r["start"]["x"], r["start"]["z"]) for r in repeated)
              == [(30, 1), (30, 35), (60, 1), (60, 35)], "partition bay locations")

    d.rpc.select([])
    d.hold()

    d.say("Now a ceiling module. Zoom in and draw a 60 centimetre square on the floor.")
    d.fly((25, 36, 33), (9, 0, 8))
    before_panel = d.root()["counts"]["triangles"]
    d.tool("tool.builtin.rectangle")
    d.pick((4, 0, 4), GRID)
    d.pick((6, 0, 6), GRID)
    d.escape()
    d.require(d.root()["counts"]["triangles"] == before_panel + 2, "one flat ceiling panel added")
    d.rpc.select([])
    d.window([(x, 0, z) for x in (4, 6) for z in (4, 6)], margin=8)
    d.tool("object.create_from_selection", tab="Objects & Hotspots", name="Ceiling panel 600 mm")
    panel = next(o for o in d.scene()["objects"] if o["name"] == "Ceiling panel 600 mm")
    d.require(d.root()["counts"]["triangles"] == before_panel, "panel is a reusable object")

    d.say("Press T at its corner. Move the panel up the helper to the 2.7 metre wall height.")
    d.helper((4, 0, 4), CORNER)
    d.select([panel["target"]], around=(5, 0, 5))
    d.tool("tool.builtin.move")
    d.pick((4, 0, 4), CORNER)
    d.pick((4, H, 4), ON_HELPER)
    d.escape()
    d.require(abs(next(o for o in d.scene()["objects"] if o["name"] == panel["name"])
                  ["transform"]["translation"]["y"] - H) < 1e-6, "ceiling at 2.7 m")

    d.say("Nice. A helper at ceiling level gives the horizontal pitch for Move Copy.")
    d.helper((4, H, 4), CORNER)
    d.fly((60, 165, 55), (45, 0, 25))
    d.select([panel["target"]], around=(5, H, 5))
    d.tool("tool.move_copy")
    d.pick((4, H, 4), CORNER)
    d.aim((6, H, 4), CORNER + ON_HELPER)
    d.say("For a long repeat, type 2 x39: thirty-nine more panels, two units apart.")
    d.type("2 x39")
    d.enter()
    d.escape()
    panels = [o for o in d.scene()["objects"] if o["name"] == panel["name"]]
    d.require(len(panels) == 40, "forty ceiling panels")
    d.require(len({o["definition_id"] for o in panels}) == 1, "panels share one definition")
    locations = sorted(o["transform"]["translation"]["x"] for o in panels)
    d.require(all(abs(b - a - 2) < 1e-6 for a, b in zip(locations, locations[1:])),
              "panel pitch stays 600 mm")
    d.require(all(abs(o["transform"]["translation"]["y"] - H) < 1e-6 for o in panels),
              "all panels remain at ceiling height")

    d.rpc.select([])
    d.tool("view.guides.clear")
    d.fly((111, 126, 133), (45, 0, 25))

    d.say("We leave the ceiling partial so the office layout stays visible below it.")
    d.hold()
    d.say("One central lobby, repeated partitions and doors, and a reusable ceiling module.")
    d.orbit(-70, 15)
    view = d.rpc.status()["workspace"]["viewport"]
    bounds = [o["bounds"] for o in d.scene()["objects"] if o.get("bounds")]
    low = {axis: min(b["min"][axis] for b in bounds) for axis in "xyz"}
    high = {axis: max(b["max"][axis] for b in bounds) for axis in "xyz"}
    corners = [(x, y, z) for x in (low["x"], high["x"])
               for y in (low["y"], high["y"]) for z in (low["z"], high["z"])]
    pixels = [d.project(point) for point in corners]
    d.require(all(view["x"] + 20 <= x <= view["x"] + view["width"] - 20
                  and view["y"] + 20 <= y <= view["y"] + view["height"] - 20
                  for x, y in pixels), "finished floor and ceiling fit inside the viewport")
    d.finish("Thanks for watching", "Meshcraft tutorials · 7")
