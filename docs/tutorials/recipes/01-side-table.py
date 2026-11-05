"""Tutorial 1: a small side table. Units, snapping, axis helpers, Push/Pull, Move, Move Copy."""

TOP = 60.0
HALF = TOP / 2
THICK = 5.0
UNDERSIDE = 40.0
LEG = 5.0
OBJECTS = "Objects & Hotspots"
GRID = ("Grid", "Grid Line", "Axis Guide")
ON_HELPER = ("Axis Guide",)
CORNER = ("Endpoint",)


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("view.model_settings")
    d.pose((95, 75, 110), (0, 10, 0))


def counts(d):
    return d.root()["counts"]


def pull(d, low, high, reference, target, kinds):
    """Window-select the face just drawn, then Push/Pull from one snap point to another."""
    d.rpc.select([])
    d.window(box(low, high))
    d.tool("tool.builtin.push_pull", tab="Construction")
    d.pick(reference, CORNER)
    d.pick(target, kinds)
    d.require(counts(d)["triangles"] == 12, "solid after Push/Pull")
    d.escape()


def make_object(d, name, corners):
    d.rpc.select([])
    d.window(corners)
    d.tool("object.create_from_selection", tab=OBJECTS, name=name)
    d.require(counts(d)["triangles"] == 0, f"{name} left root geometry")
    return next(o for o in d.scene()["objects"] if o["name"] == name)["target"]


def box(low, high):
    return [(x, y, z) for x in (low[0], high[0]) for y in (low[1], high[1]) for z in (low[2], high[2])]


def record(d):
    d.title("Your first model: a side table", "Meshcraft tutorials · 1")

    d.say("Before drawing anything, set the units in Model Settings.")
    d.hold()
    d.say("Unit name is the suffix shown on every measurement. Type cm.")
    d.field("Unit name", "cm")
    d.say("Unit size says how long one model unit is. One unit, one centimetre.")
    d.field("Unit size", "1")
    d.say("Grid size is the snap spacing, in units. Five gives a 5 cm grid.")
    d.field("Grid size", "5")
    settings = d.scene()["settings"]
    d.require(settings["unit_name"] == "cm" and settings["grid_size"] == 5.0
              and settings["unit_size"] == 1.0, f"units set: {settings}")
    d.hold()

    d.say("Good. Now move over the grid. The coloured axes on the cursor show where it snaps.")
    d.aim((-HALF, 0, HALF), GRID)
    d.wait(0.6)
    d.aim((HALF, 0, HALF), GRID)
    d.hold()

    d.say("Start with the table top. Choose Rectangle.")
    d.tool("tool.builtin.rectangle")
    d.say("Click two grid points, 60 cm apart in each direction.")
    d.pick((-HALF, 0, -HALF), GRID)
    d.pick((HALF, 0, HALF), GRID)
    d.escape()
    d.require(counts(d)["triangles"] == 2, "top face")
    d.hold()

    d.say("Uh oh, nothing to snap to up in the air. Press T to add an axis helper.")
    d.helper((-HALF, 0, HALF), CORNER)
    d.say("Its blue axis points straight up and snaps at every grid step.")
    d.aim((-HALF, 20, HALF), ON_HELPER)
    d.wait(0.6)
    d.hold()

    d.say("Push/Pull: pick the corner, then a point one step up the helper.")
    pull(d, (-HALF, 0, -HALF), (HALF, 0, HALF), (-HALF, 0, HALF), (-HALF, THICK, HALF), ON_HELPER)
    d.hold()

    d.say("Nice! Make Object keeps this part from merging with what we draw next.")
    make_object(d, "Table top", box((-HALF, 0, -HALF), (HALF, THICK, HALF)))
    d.hold()

    d.say("Now lift the top. Move: pick its corner, then a point up the helper.")
    d.tab("Home")
    d.click((0, THICK, 0))
    d.tool("tool.builtin.move")
    d.pick((-HALF, 0, HALF), CORNER)
    d.pick((-HALF, UNDERSIDE, HALF), ON_HELPER)
    d.escape()
    placed = next(o for o in d.scene()["objects"] if o["name"] == "Table top")
    d.require(abs(placed["transform"]["translation"]["y"] - UNDERSIDE) < 1e-6, "top lifted")
    d.hold()

    d.say("Goooood. Next, one leg: a small square under the corner of the top.")
    d.tool("tool.builtin.rectangle")
    d.pick((-HALF, 0, HALF), GRID)
    d.pick((-HALF + LEG, 0, HALF - LEG), GRID)
    d.escape()
    d.require(counts(d)["triangles"] == 2, "leg face")
    d.say("Pull it up until the cursor snaps to the corner underneath the top.")
    pull(d, (-HALF, 0, HALF - LEG), (-HALF + LEG, 0, HALF), (-HALF, 0, HALF), (-HALF, UNDERSIDE, HALF),
         CORNER + ON_HELPER)
    d.hold()

    d.say("Make it an object too, so all four legs share one definition.")
    make_object(d, "Leg", box((-HALF, 0, HALF - LEG), (-HALF + LEG, UNDERSIDE, HALF)))
    d.hold()

    d.say("Move Copy repeats it. Pick the leg's corner, then a grid point 55 cm along.")
    span = TOP - LEG
    d.tab("Home")
    d.click((-HALF + LEG / 2, UNDERSIDE / 2, HALF))
    d.tool("tool.move_copy")
    d.pick((-HALF, 0, HALF), CORNER)
    d.pick((-HALF + span, 0, HALF), GRID)
    d.escape()
    legs = [o["target"] for o in d.scene()["objects"] if o["name"] == "Leg"]
    d.require(len(legs) == 2, f"expected 2 legs, got {len(legs)}")
    d.rpc.select([])
    d.hold()

    d.say("You can also type a distance. Pick a start, aim, type 55 and press Enter.")
    d.window(box((-HALF, 0, HALF - LEG), (HALF, UNDERSIDE - 2, HALF)))
    d.require(len(d.selected()) == 2, f"window selected both legs: {d.selected()}")
    d.tool("tool.move_copy")
    d.pick((-HALF, 0, HALF), CORNER)
    d.aim((-HALF, 0, HALF - 2 * LEG), GRID)
    d.type(f"{span:g}")
    d.enter()
    d.escape()
    legs = [o for o in d.scene()["objects"] if o["name"] == "Leg"]
    d.require(len(legs) == 4, f"expected 4 legs, got {len(legs)}")
    d.require(len({o["definition_id"] for o in legs}) == 1, "legs share one definition")
    d.rpc.select([])
    d.hold()

    d.say("Niiiice! Clear Guides removes the helper once it has done its job.")
    d.tool("view.guides.clear")
    d.require(d.rpc.status()["constructionGuides"] == 0, "guides cleared")
    d.hold()

    d.say("Right-drag to orbit and check the result from another side.")
    d.orbit(-260, 40)
    d.hold()
    d.say("Cool. That's the whole method: set units, snap to the grid, climb with helpers.")
    d.hold()
    d.finish("Next: walls and openings", "Meshcraft tutorials · 2")
