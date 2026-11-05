"""Tutorial 5: a round pavilion. Units, Circle, axis helpers, Rotate Copy with a repeat count."""

import math

R = 4.75
COLUMN_R = 0.25
RING = 4.0
H = 3.0
SLAB = 0.25
OBJECTS = "Objects & Hotspots"
GRID = ("Grid", "Grid Line", "Axis Guide")
ON_HELPER = ("Axis Guide",)
CORNER = ("Endpoint",)


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("view.model_settings")
    d.pose((12, 9, 15), (0, 1.5, 0))


def counts(d):
    return d.root()["counts"]


def disc(d, centre, edge, height, name, select_part, cues):
    """Circle on the grid, a helper at its edge, Push/Pull along it, then Make Object."""
    d.say(cues[0])
    d.tool("tool.builtin.circle")
    d.pick(centre, GRID)
    d.pick(edge, GRID)
    d.escape()
    faces = d.primitives([], kinds=("triangle",))
    d.require(faces, f"{name}: circle made a face")
    d.say(cues[1])
    d.helper(edge, CORNER)
    select_part(0)
    d.tool("tool.builtin.push_pull", tab="Construction")
    d.pick(edge, CORNER)
    d.pick((edge[0], height, edge[2]), ON_HELPER)
    d.escape()
    d.require(counts(d)["triangles"] > len(faces), f"{name}: solid")
    d.say(cues[2])
    select_part(height)
    d.tool("object.create_from_selection", tab=OBJECTS, name=name)
    d.require(counts(d)["triangles"] == 0, f"{name}: object")
    return next(o for o in d.scene()["objects"] if o["name"] == name)


def record(d):
    d.title("Arrays: a round pavilion", "Meshcraft tutorials · 5")
    d.say("Units first: metres, on a quarter metre grid.")
    d.units("m", 1, 0.25)
    d.hold()
    d.say("Ok! Repeating parts is where modeling gets quick. Let's build a pavilion.")
    d.hold()

    edge = (RING + COLUMN_R, 0, 0)
    # Nothing else is drawn yet, so a window takes the whole column.
    def column_part(top):
        d.rpc.select([])
        r = COLUMN_R
        d.window([(RING + x, y, z) for x in (-r, r) for y in (0, top) for z in (-r, r)])

    column = disc(d, (RING, 0, 0), edge, H, "Column", column_part, (
        "One column first. Circle: click the centre, then a grid point on the edge.",
        "Press T on the edge for a helper, then Push/Pull three metres up it.",
        "Good. Make it an object. That's what the copies will share.",
    ))
    d.hold()

    d.say("Rotate Copy spins copies around a centre. Select the column first.")
    d.tab("Home")
    d.click((RING, H / 2, COLUMN_R))
    d.tool("tool.rotate_copy")
    d.say("Pick the centre of the circle, on the grid, then the edge of the column.")
    d.pick((0, 0, 0), GRID)
    d.pick(edge, CORNER)
    d.say("Aim roughly around, then type an angle and a count: 30 x11 adds eleven.")
    angle = math.radians(25)
    d.point((edge[0] * math.cos(angle), 0, -edge[0] * math.sin(angle)))
    d.type("30 x11")
    d.enter()
    d.escape()
    columns = [o for o in d.scene()["objects"] if o["name"] == "Column"]
    d.require(len(columns) == 12, f"12 columns, got {len(columns)}")
    d.require(len({o["definition_id"] for o in columns}) == 1, "columns share one definition")
    d.rpc.select([])
    d.say("Wooooow! Twelve columns from one.")
    d.hold()

    floor_edge = (R, 0, 0)
    # A window here would catch the columns, so select the loose geometry, which is only the floor.
    def floor_part(top):
        d.select(d.primitives([], kinds=("segment", "triangle")), around=(1.5, top, 1.5))

    floor = disc(d, (0, 0, 0), floor_edge, SLAB, "Floor", floor_part, (
        "Now the floor: a big circle around them, on the same centre.",
        "Same again: a helper on its edge, Push/Pull up one grid step.",
        "Make it an object too.",
    ))
    d.hold()

    d.say("The roof is just the floor again. Move Copy it, up that same helper.")
    d.tab("Home")
    d.click((1.5, SLAB, 1.5))
    d.tool("tool.move_copy")
    d.pick(floor_edge, CORNER)
    d.pick((R, H, 0), ON_HELPER)
    d.escape()
    d.require(len([o for o in d.scene()["objects"] if o["name"] == "Floor"]) == 2, "roof copied")
    d.rpc.select([])
    d.hold()

    d.say("Awesome. Clear Guides, and take a look around.")
    d.tool("view.guides.clear", tab="Home")
    d.orbit(-200, 35)
    d.hold()
    d.say("Change that one column's definition later, and all twelve update together.")
    d.finish("Thanks for watching", "Meshcraft tutorials · 5 of 5")
