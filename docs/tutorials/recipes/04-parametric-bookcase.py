"""Tutorial 4: a parametric bookcase. A script object, a hotspot and instance parameters."""

import time

OBJECTS = "Objects & Hotspots"
W, H, DEPTH = 80.0, 180.0, 30.0
SHORT = 100.0
GRID = ("Grid", "Grid Line", "Grid Guide")
ON_GRID = ("Grid Guide",)
CORNER = ("Endpoint",)
SOURCE = """// A bookcase rebuilt from its parameters on every change.
function onCreate() {
  $NUMBER('shelves', 'Shelves', 3);
  $HOTSPOT(new Point(80, 180, 30), 'size', 'Size');
}

function onAnimationFrame(params) {
  const width = Math.max(30, params.size.position.x);
  const height = Math.max(40, params.size.position.y);
  const depth = 30, t = 3;
  const wood = phong('#b98751ff');
  const box = (id, a, b) => cuboid(new Point(...a), new Point(...b), id, wood);
  const parts = [
    box('left', [0, 0, 0], [t, height, depth]),
    box('right', [width - t, 0, 0], [width, height, depth]),
    box('top', [t, height - t, 0], [width - t, height, depth]),
    box('base', [t, 0, 0], [width - t, 30, depth]),
  ];
  for (let i = 1; i <= params.shelves; i++) {
    const y = 30 + (height - 33) * i / (params.shelves + 1);
    parts.push(box('shelf' + i, [t, y, 0], [width - t, y + 2, depth]));
  }
  return parts;
}
"""


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    d.command("view.model_settings")
    d.pose((400, 290, 500), (115, 100, 15))


def objects(d):
    return [o for o in d.scene()["objects"] if not o["is_scene_root"]]


def wait_for_triangles(d, path, check, message):
    deadline = time.monotonic() + 20
    while True:
        o = next(o for o in d.scene()["objects"] if o["path"] == path)
        if check(o["counts"]["triangles"]):
            return o
        d.require(time.monotonic() < deadline, message)
        time.sleep(0.05)


def height_of(d, path):
    # Evaluation follows the drag asynchronously, so wait for it to settle.
    deadline, last = time.monotonic() + 10, None
    while True:
        page = d.rpc.call("meshcraft_geometry", dict(path=path, limit=1000))
        height = max(v["position"]["y"] for v in page["vertices"])
        if height == last or time.monotonic() > deadline:
            return height
        last = height
        time.sleep(0.3)


def on_grid(snap, near, step=20.0):
    """The grid helper's point nearest a wanted spot; its grid starts where it was placed."""
    origin = snap["point"]
    return (origin["x"] + round((near[0] - origin["x"]) / step) * step,
            origin["y"] + round((near[1] - origin["y"]) / step) * step,
            origin["z"])


def handle(path):
    return dict(path=path, kind="semantic_handle", semantic_kind="hotspot", id="size")


def record(d):
    d.title("A parametric bookcase", "Meshcraft tutorials · 4")
    d.say("Units first: centimetres, on a twenty centimetre grid.")
    d.units("cm", 1, 20)
    d.hold()
    d.say("Script objects are built by code, so one design can take many sizes.")
    result = d.rpc.call("meshcraft_object_script", dict(action="create", api_version=2, source=SOURCE))
    definition = result["definition_id"]
    instance = objects(d)
    d.require(len(instance) == 1, "script object placed")
    first = instance[0]
    first = wait_for_triangles(d, first["path"], lambda n: n > 0, "script evaluated")
    shelves_3 = first["counts"]["triangles"]
    d.hold()

    d.say("Edit Script shows the code: parameters first, then the geometry.")
    d.select([first["target"]], around=(W / 2, H / 2, DEPTH))
    d.tool("script.edit", tab=OBJECTS)
    d.require(d.rpc.status()["workspace"]["titleToolsBlocked"], "script editor opened")
    # The preview opens close up, so pull back with the wheel as a user would.
    d.wheel(1460, 570, 5, 10)
    d.wait(1.0)
    d.say("$NUMBER adds a field to the panel. $HOTSPOT adds a handle you can drag.")
    d.hold()
    d.wait(1.0)
    d.say("Close the editor to get back to the model.")
    d.key("Escape")
    d.hold()
    d.require(not d.rpc.status()["workspace"]["titleToolsBlocked"], "script editor closed")

    d.say("To size it, press G for a grid helper on the face of the bookcase.")
    base = d.grid_helper_on_face((20, 20, DEPTH), (0, 0, 1))
    d.say("Now drag the hotspot along that grid. The bookcase is rebuilt as it moves.")
    tall = on_grid(base, (120, 220))
    d.drag((W, H, DEPTH), tall, ON_GRID)
    d.require(abs(height_of(d, first["path"]) - tall[1]) < 1e-6, "hotspot reached the grid point")
    d.rpc.select([])
    d.hold()

    d.say("Parameters sit in the Object Instance panel. Set this one to 5 shelves.")
    d.click((60, 15, DEPTH))
    d.command("view.object_info")
    d.wait(1.0)
    d.rpc.call("meshcraft_parameters", dict(action="set", definition_id=definition,
                                            paths=[first["path"]], id="shelves", value=5))
    wait_for_triangles(d, first["path"], lambda n: n > shelves_3, "more shelves")
    d.hold()

    d.say("Awesome. Move Copy it: pick its corner, then a grid point further along.")
    d.tool("tool.move_copy")
    d.pick((0, 0, DEPTH), CORNER)
    d.pick((160, 0, DEPTH), GRID)
    d.escape()
    placed = objects(d)
    d.require(len(placed) == 2, f"two bookcases, got {len(placed)}")
    second = next(o for o in placed if o["path"] != first["path"])
    d.say("Make the copy shorter, on that same grid helper. The first stays as it was.")
    d.click((160 + 60, 15, DEPTH))
    short = on_grid(base, (240, SHORT))
    d.drag((160 + tall[0], tall[1], DEPTH), short, ON_GRID)
    d.rpc.select([])
    d.hold()
    first_height, copy_height = height_of(d, first["path"]), height_of(d, second["path"])
    d.require(abs(first_height - tall[1]) < 1e-6, f"first bookcase unchanged, got {first_height}")
    d.require(abs(copy_height - short[1]) < 1e-6, f"copy resized to {short[1]}, got {copy_height}")

    d.say("Clear Guides to tidy up.")
    d.tool("view.guides.clear", tab="Home")
    d.say("Wooow! One script, two independent bookcases, both still adjustable.")
    d.orbit(-140, 20)
    d.hold()
    d.finish("Next: arrays and a round pavilion", "Meshcraft tutorials · 5")
