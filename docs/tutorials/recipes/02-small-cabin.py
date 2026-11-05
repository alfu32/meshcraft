"""Tutorial 2: a small cabin. The Architecture template, Slab, Walls, helpers, Hole, Door, Window."""

# In template units: one unit is 30 cm, so this is 9 m by 6 m with 2.7 m walls.
W, D, H = 30.0, 20.0, 9.0
CIVIL = "Civil"
GRID = ("Grid", "Grid Line", "Axis Guide")
ON_HELPER = ("Axis Guide",)
CORNER = ("Endpoint",)
TEMPLATE_CARD = (750, 266)
DOOR = ((8.0, 0.0), (11.0, 7.0))
WINDOW = ((18.0, 3.0), (24.0, 7.0))


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("file.home", open=True)


def owners(d):
    return d.rpc.call("meshcraft_geometry", dict(path=[], limit=1))["architecture"]


def front_wall(d):
    return next(o for o in owners(d) if o["record"].get("start", {}).get("z") == D
                and o["record"].get("end", {}).get("z") == D)


def record(d):
    d.title("A small cabin: walls and openings", "Meshcraft tutorials · 2")
    d.say("Start on the Home screen. The Architecture template sets one unit to 30 cm.")
    d.click_mark(*TEMPLATE_CARD)
    d.command("file.new", preset="architecture_metric")
    d.command("view.model_settings")
    d.pose((60, 42, 66), (15, 3, 10))
    settings = d.scene()["settings"]
    d.require(settings["unit_name"] == "cm" and settings["unit_size"] == 30.0
              and settings["grid_size"] == 1.0, f"template units: {settings}")
    d.hold()
    d.say("Grid and walls follow it: walls one unit thick and nine units high.")
    d.hold()

    d.say("Ok, lay the floor. On the Civil tab, choose Rectangular Slab.")
    d.tool("tool.builtin.arch_slab", tab=CIVIL)
    d.say("Click two grid points: nine metres by six.")
    d.pick((0, 0, 0), GRID)
    d.pick((W, 0, D), GRID)
    d.escape()
    d.require(len(owners(d)) == 1, f"floor slab, got {len(owners(d))}")
    d.hold()

    d.say("Now the walls. The Wall tool draws a connected chain.")
    d.tool("tool.builtin.arch_wall", tab=CIVIL)
    d.say("Snap to each corner of the floor. Walls join by themselves.")
    for point in ((0, 0, 0), (W, 0, 0), (W, 0, D), (0, 0, D), (0, 0, 0)):
        d.pick(point, dwell=0.25)
    d.say("Press Enter to finish the chain.")
    d.enter()
    d.escape()
    walls = [o for o in owners(d) if "start" in o["record"]]
    d.require(len(walls) == 4, f"four walls, got {len(walls)}")
    for wall in walls:
        d.require(wall["record"]["thickness"] == 1.0 and wall["record"]["height"] == H,
                  f"template wall size: {wall['record']['thickness']}, {wall['record']['height']}")
    d.hold()

    d.say("Openings need points up the wall, so place axis helpers along its base.")
    for x in (DOOR[0][0], DOOR[1][0], WINDOW[0][0], WINDOW[1][0]):
        d.helper((x, 0, D))
    d.hold()

    d.say("Add Hole: click up one helper on the wall, then the base of the other.")
    d.tool("tool.builtin.arch_add_hole", tab=CIVIL)
    d.pick((DOOR[1][0], DOOR[1][1], D), ON_HELPER)
    d.pick((DOOR[0][0], DOOR[0][1], D))
    d.say("Same again for the window, on its own two helpers.")
    d.tool("tool.builtin.arch_add_hole", tab=CIVIL)
    d.pick((WINDOW[0][0], WINDOW[0][1], D), ON_HELPER)
    d.pick((WINDOW[1][0], WINDOW[1][1], D), ON_HELPER)
    d.escape()
    holes = front_wall(d)["record"]["holes"]
    d.require(len(holes) == 2, f"two holes in the front wall, got {len(holes)}")
    d.hold()

    d.say("Good. The helpers have done their job, so Clear Guides removes them.")
    d.tool("view.guides.clear", tab="Home")
    d.require(d.rpc.status()["constructionGuides"] == 0, "guides cleared")
    d.hold()

    d.say("Holes are only openings. Doors and windows are separate objects.")
    d.tool("tool.builtin.arch_door_frame", tab=CIVIL)
    d.say("Door: snap to two opposite corners of the opening.")
    d.pick((DOOR[1][0], DOOR[1][1], D), CORNER)
    d.pick((DOOR[0][0], DOOR[0][1], D), CORNER)
    d.escape()
    d.say("Ooh, nice. Now the same with the Window tool.")
    d.tool("tool.builtin.arch_window_frame", tab=CIVIL)
    d.pick((WINDOW[1][0], WINDOW[1][1], D), CORNER)
    d.pick((WINDOW[0][0], WINDOW[0][1], D), CORNER)
    d.escape()
    frames = {o["record"]["kind"]: o["record"] for o in owners(d) if "kind" in o["record"]}
    d.require(set(frames) == {"DOOR", "WINDOW"}, f"door and window, got {sorted(frames)}")
    for frame, low, high in ((frames["DOOR"], *DOOR), (frames["WINDOW"], *WINDOW)):
        xs = [p["x"] for p in frame["contour"]]
        ys = [p["y"] for p in frame["contour"]]
        d.require(abs(min(xs) - low[0]) + abs(max(xs) - high[0]) + abs(min(ys) - low[1])
                  + abs(max(ys) - high[1]) < 1e-6, f"{frame['kind']} fits its opening")
        d.require(all(abs(p["z"] - D) < 1e-6 for p in frame["contour"]), "frame on the wall surface")
    d.hold()

    d.say("Finish with a roof: one more slab, snapped to the top corners of the walls.")
    d.tool("tool.builtin.arch_slab", tab=CIVIL)
    d.pick((0, H, 0), CORNER)
    d.pick((W, H, D), CORNER)
    d.escape()
    slabs = [o for o in owners(d) if "profile" in o["record"]]
    d.require(len(slabs) == 2, f"roof slab, got {len(slabs)}")
    d.rpc.select([])
    d.hold()

    d.say("Niiiice! Right-drag to look around the finished cabin.")
    d.orbit(-220, 30)
    d.hold()
    d.say("Everything stays editable: select a wall later to change its height.")
    d.finish("Next: materials", "Meshcraft tutorials · 3")
