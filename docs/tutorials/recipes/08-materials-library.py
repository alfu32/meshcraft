"""Tutorial 8: the materials features from 0.55 to 0.60 on the cabin from tutorial 2.

The Material Manager's sphere previews and its Model and Library tabs (0.60), Apply to Selection
from the library, a textured material and Edit Texture Position (0.55), New Material with a
built-in shader such as Polished metal (0.59), the Shader Programs panel and the shader editor's
Full Source tab (0.58 and 0.59), and library grass for the lawn.
"""

from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from director import ROOT

GRID = ("Grid", "Grid Line")
ON_HELPER = ("Axis Guide",)
SOURCE = ROOT / "docs/tutorials/projects/02-small-cabin.octd"
TEXTURE = ROOT / "docs/tutorials/projects/textures/decking.png"
# The title bar shows the open path, so a neutral copy keeps home directories out of the video.
FOLDER = Path(tempfile.gettempdir()) / "meshcraft-tutorials" / "cabin-materials"
PROJECT = FOLDER / "small-cabin.octd"

# The Material Editor and the shader editor are modal and do not report their controls, so these
# positions are measured from 1920 x 1080 captures of this release.
EDITOR_NAME = (495, 189)
EDITOR_PROGRAM = (470, 233)
PROGRAM_POLISHED_METAL = (417, 554)
EDITOR_TEXTURE = (416, 488)
EDITOR_BROWSE = (652, 488)
EDITOR_WIDTH = (355, 559)
EDITOR_HEIGHT = (635, 559)
EDITOR_SAVE = (1658, 991)
SHADER_FULL_SOURCE = (538, 177)
SHADER_CANCEL = (1614, 1039)

ROOF = (15, 10, 10)
FRONT_WALL = (14, 5, 20)
SIDE_WALL = (30, 5, 10)
BALL = (38, 0, 28)
BALL_RADIUS = 3
LAWN = ((-8, 0, 34), (40, 0, -6))


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    shutil.rmtree(FOLDER, ignore_errors=True)
    (FOLDER / "textures").mkdir(parents=True)
    shutil.copyfile(SOURCE, PROJECT)
    shutil.copyfile(TEXTURE, FOLDER / "textures/decking.png")
    d.command("file.open", path=str(PROJECT))
    deadline = time.monotonic() + 60
    while d.rpc.status()["projectPath"] != str(PROJECT):
        d.require(time.monotonic() < deadline, "project opened")
        time.sleep(0.05)
    # Tutorial 2 left its helpers in the file; they would only clutter these views.
    d.command("view.guides.clear")
    d.command("view.selection")
    d.pose((60, 30, 52), (17, 4, 12))


# Panel controls ------------------------------------------------------------

def controls(d):
    return d.rpc.status()["workspace"]["controls"]


def press(d, point, count=1):
    """A real click on a control, with the click ring; panels and editors use real X input."""
    d.glide(*point)
    time.sleep(0.3)
    d._ring()
    d._xdo("click", "--repeat", count, "--delay", 110, 1)
    # Real input keeps human delays even in a dry run, so the window takes each click in.
    time.sleep(0.6)


def reveal(d, key):
    """Scroll the materials grid with the real wheel until the control is in view; return its centre."""
    previous = None
    for _ in range(60):
        # Positions come from the last painted frame, so wait for the window to settle on one.
        time.sleep(0.35)
        found = controls(d)
        d.require(key in found, f"{key} is in the Materials panel")
        box, view = found[key], found["panel/materials/scroll"]
        if box != previous:
            previous = box
            continue
        top, bottom = box["y"], box["y"] + box["height"]
        if top >= view["y"] - 1 and bottom <= view["y"] + view["height"] + 1:
            return box["x"] + box["width"] / 2, box["y"] + box["height"] / 2
        centre = (view["x"] + view["width"] / 2, view["y"] + view["height"] / 2)
        d.glide(*centre)
        d._xdo("click", "--repeat", 2, "--delay", 60, 5 if top > view["y"] else 4)
        previous = None
    raise RuntimeError(f"Could not scroll {key} into view")


def control(d, key, count=1):
    """Travel to a measured panel control and click it for real."""
    if key.startswith("panel/materials/") and not key.startswith("panel/materials/tab/"):
        point = reveal(d, key)
    else:
        point = d.control(key)
    press(d, point, count)


def browse(d, key):
    """Wheel the grid down to a section, as a person skims the library."""
    reveal(d, key)
    d.wait(0.8)


def type_into(d, point, text):
    """Type into an editor field for real. Each frame is slow under software rendering, so the
    characters go in at a pace the window can show."""
    press(d, point)
    d._xdo("key", "ctrl+a")
    time.sleep(0.3)
    for character in text:
        d._xdo("type", character)
        time.sleep(0.32)
    time.sleep(0.6)


def material(d, name):
    found = [m for m in d.scene()["materials"] if m["name"] == name]
    d.require(len(found) == 1, f"one material named {name}: {found}")
    return found[0]


def selection_materials(d):
    return set(d.rpc.status()["workspace"]["selectionMaterials"])


def semantic(d, kind):
    page = d.rpc.call("meshcraft_geometry", dict(path=d.root()["path"], limit=1000))
    found = {}
    for primitive in page["primitives"]:
        target = primitive["target"]
        if target.get("semantic_kind") == kind:
            found[target["id"]] = target
    return list(found.values())


def wait_for(d, predicate, message, seconds=20):
    deadline = time.monotonic() + seconds
    while not predicate():
        d.require(time.monotonic() < deadline, message)
        time.sleep(0.1)


def editor_open(d):
    return d.rpc.status()["workspace"]["titleToolsBlocked"]


# Texture edit ---------------------------------------------------------------

def texture_drag(d, start, end):
    """A real-handler drag of the texture tile: press on the handle, hover along, release."""
    d.glide(*d.project(start))
    d._native("hover")
    d.wait(0.4)
    d._ring()
    d._native("down")
    d.wait(0.2)
    d.glide(*d.project(end), seconds=1.6)
    d.wait(0.3)
    d._native("up")
    d.wait(0.8)


def tile_corners(d):
    tile = d.rpc.status()["textureEdit"]["tile"]
    return [[c[k] for k in "xyz"] for c in (tile or {}).get("corners", [])]


def record(d):
    d.title("Materials: previews, library and shaders", "Meshcraft tutorials · 8")
    d.say("This continues the cabin from tutorial 2, so the units are set. Let's dress it up.")
    d.hold()

    # 0.60: the Material Manager -------------------------------------------------
    d.say("On the Materials tab, open the Material Manager.")
    d.tool("view.panel.materials.toggle", tab="Materials")
    wait_for(d, lambda: "panel/materials/tab/model" in controls(d), "the Materials panel opened")
    d.say("Each material is now a little sphere wearing it, rendered with the real shaders.")
    d.glide(*d.control("panel/materials/material/ansi-red"))
    d.hold()

    d.say("The Model tab holds this model's materials. The Library tab holds ready-made ones.")
    control(d, "panel/materials/tab/library")
    wait_for(d, lambda: "panel/materials/library/library.color.red-50" in controls(d), "library shown")
    d.hold()
    d.say("Plain colors first, then vegetation, then construction: brick, tiles, stone, glass, metal.")
    browse(d, "panel/materials/library/library.grass.normal")
    browse(d, "panel/materials/library/library.brick.red")
    d.hold()

    # Library brick on the walls --------------------------------------------------
    d.say("Select the four walls.")
    walls = semantic(d, "wall")
    d.require(len(walls) == 4, f"four walls: {walls}")
    d.select(walls, around=FRONT_WALL)
    d.select(walls, around=SIDE_WALL)
    d.say("Pick Red brick, then Apply to Selection. That adds it to the model and paints the walls.")
    control(d, "panel/materials/library/library.brick.red")
    control(d, "panel/button/material.apply_from_library")
    ids = {m["id"] for m in d.scene()["materials"]}
    d.require("library.brick.red" in ids, "red brick added to the model")
    d.require(selection_materials(d) == {"library.brick.red"}, f"walls are brick: {selection_materials(d)}")
    d.hold()
    d.say("Ooh, real bricks. The tick marks library materials already in the model.")
    d.glide(*reveal(d, "panel/materials/library/library.brick.red"))
    d.rpc.select([])
    d.hold()

    # 0.55: a textured material ----------------------------------------------------
    d.say("Now a texture for the roof. Back on the Model tab, choose New Material.")
    control(d, "panel/materials/tab/model")
    before = {m["id"] for m in d.scene()["materials"]}
    control(d, "panel/button/material.new")
    wait_for(d, lambda: editor_open(d), "the Material Editor opened")
    d.say("Name it, then give it an image. Browse opens a file dialog; typing the path works too.")
    type_into(d, EDITOR_NAME, "Decking")
    d.glide(*EDITOR_BROWSE)
    d.wait(0.5)
    type_into(d, EDITOR_TEXTURE, "textures/decking.png")
    d.say("The image covers 4 by 4 units. That's 1.2 metres in this model.")
    type_into(d, EDITOR_WIDTH, "4")
    type_into(d, EDITOR_HEIGHT, "4")
    d._xdo("key", "Tab")
    d.wait(1.5)
    d.hold()
    press(d, EDITOR_SAVE)
    wait_for(d, lambda: not editor_open(d), "the Material Editor closed")
    deck = material(d, "Decking")
    d.require(deck["id"] not in before and deck.get("texturePath") == "textures/decking.png"
              and deck["textureScaleU"] == 4 and deck["textureScaleV"] == 4, f"decking saved: {deck}")

    d.say("Select the roof, then Apply to Selection.")
    roof = [s for s in semantic(d, "slab") if s["id"].endswith(":1")]
    d.require(len(roof) == 1, f"the roof slab: {semantic(d, 'slab')}")
    d.click(ROOF)
    d.require([t["id"] for t in d.selected()] == [roof[0]["id"]], f"roof selected: {d.selected()}")
    control(d, f"panel/materials/material/{deck['id']}")
    control(d, "panel/button/material.apply")
    d.require(selection_materials(d) == {deck["id"]}, f"roof is decking: {selection_materials(d)}")
    d.rpc.select([])
    d.fly((40, 46, 46), (15, 8, 10))
    d.hold()

    d.say("Hmm, the boards are narrow. Choose Edit Texture Position, then click the roof.")
    control(d, "panel/button/material.edit_texture")
    d.require(d.rpc.status()["textureEdit"]["active"], "texture edit mode on")
    d.glide(*d.project(ROOF))
    d._native("hover")
    d.wait(0.3)
    d._ring()
    d._native("down")
    d._native("up")
    d.wait(0.8)
    corners = tile_corners(d)
    d.require(len(corners) == 4, f"the roof shows its tile: {d.rpc.status()['textureEdit']}")
    d.hold()

    d.say("Drag a corner to size the image. The opposite corner stays put.")
    # Drag whichever corner can double the tile and still land on the roof.
    target = None
    for moving in range(4):
        fixed = corners[(moving + 2) % 4]
        end = [f + 2 * (m - f) for m, f in zip(corners[moving], fixed)]
        if 1 <= end[0] <= 29 and 1 <= end[2] <= 19:
            target = (corners[moving], end)
            break
    d.require(target is not None, f"a corner that doubles the tile on the roof: {corners}")
    texture_drag(d, *target)
    deck = material(d, "Decking")
    d.require(abs(deck["textureScaleU"] - 8) < 0.05 and abs(deck["textureScaleV"] - 8) < 0.05,
              f"tile doubled: {deck}")
    d.say("Nice, wider boards. The drag is saved to the material, and Undo takes it back.")
    d.hold()
    d.escape()
    d.escape()
    d.require(not d.rpc.status()["textureEdit"]["active"], "texture edit mode off")

    # 0.59: New Material with a built-in shader ------------------------------------
    d.say("Next, something shiny on the lawn. Press T on the grid for a helper.")
    d.fly((64, 30, 62), (22, 3, 16))
    d.helper(BALL, GRID)
    d.say("Sphere: pick a centre up the helper, then a point above it for the radius.")
    d.tool("tool.builtin.primitive_sphere", tab="Solids")
    before = d.root()["counts"]["triangles"]
    d.pick((BALL[0], BALL_RADIUS, BALL[2]), ON_HELPER)
    d.pick((BALL[0], 2 * BALL_RADIUS, BALL[2]), ON_HELPER)
    d.escape()
    d.require(d.root()["counts"]["triangles"] > before, "a sphere")
    ball = d.selected()
    d.require(ball, "the sphere stays selected")
    d.tool("view.guides.clear")
    d.rpc.select(ball)
    d.hold()

    d.say("New Material now lists every shader in one place. Open the Program list.")
    d.tab("Materials")
    control(d, "panel/button/material.new")
    wait_for(d, lambda: editor_open(d), "the Material Editor opened")
    press(d, EDITOR_PROGRAM)
    d.wait(0.8)
    d.hold()
    d.say("Emboss tiling, rough stone, metal, glass and Perlin noise. Pick Polished metal.")
    press(d, PROGRAM_POLISHED_METAL)
    d.wait(2.5)
    d.hold()
    press(d, EDITOR_SAVE)
    wait_for(d, lambda: not editor_open(d), "the Material Editor closed")
    metal = material(d, "Polished metal")
    d.require(metal.get("procedural", {}).get("program") == "metal.polished", f"metal saved: {metal}")
    d.rpc.select(ball)
    d.say("The sphere is still selected, so Apply to Selection.")
    control(d, f"panel/materials/material/{metal['id']}")
    control(d, "panel/button/material.apply")
    d.require(selection_materials(d) == {metal["id"]}, f"ball is metal: {selection_materials(d)}")
    d.rpc.select([])
    d.fly((56, 14, 52), (34, 3, 24))
    d.say("Look, a mirror. Metal reflects a studio sky with hard highlights.")
    d.hold()

    # 0.58 and 0.59: shader programs --------------------------------------------------
    d.say("Shader Programs lists every program and what uses it.")
    d.tool("view.panel.programs.toggle", tab="Materials")
    wait_for(d, lambda: "panel/programs/metal.polished" in controls(d), "Shader Programs opened")
    d.glide(*d.control("panel/programs/metal.polished"))
    d.hold()
    d.say("Deleting a custom program turns its materials back into plain ones.")
    d.glide(*d.control("panel/button/program.delete"))
    d.hold()
    d.say("New Shader opens the shader editor. Full Source shows exactly what gets compiled.")
    control(d, "panel/button/material.new_shader")
    wait_for(d, lambda: editor_open(d), "the shader editor opened")
    d.wait(1.0)
    press(d, SHADER_FULL_SOURCE)
    d.wait(1.5)
    d.hold()
    d.say("No hidden layers. We won't write one today, so Cancel.")
    press(d, SHADER_CANCEL)
    wait_for(d, lambda: not editor_open(d), "the shader editor closed")

    # Library grass for the lawn ------------------------------------------------------
    d.say("Last, the lawn. Rectangle, and click two grid points around the cabin.")
    d.fly((70, 44, 70), (16, 0, 14))
    d.tool("tool.builtin.rectangle")
    d.pick(LAWN[0], GRID)
    d.pick(LAWN[1], GRID)
    d.escape()
    lawn = d.selected()
    d.require(lawn, "the lawn stays selected")
    d.say("In the Library, double-click Grass. It joins the model with a tick.")
    d.tool("view.panel.materials.toggle", tab="Materials")
    wait_for(d, lambda: "panel/materials/tab/library" in controls(d), "the Materials panel opened")
    control(d, "panel/materials/tab/library")
    control(d, "panel/materials/library/library.grass.normal", count=2)
    d.wait(1.0)
    d.require("library.grass.normal" in {m["id"] for m in d.scene()["materials"]}, "grass added")
    d.rpc.select(lawn)
    d.hold()
    d.say("Then Apply to Selection.")
    control(d, "panel/button/material.apply_from_library")
    d.require(selection_materials(d) == {"library.grass.normal"}, f"lawn is grass: {selection_materials(d)}")
    d.rpc.select([])
    d.fly((58, 26, 56), (17, 3, 13))
    d.say("Wooow. Every material is saved with the project, and stays editable.")
    d.orbit(-110, 12)
    d.hold()
    d.fly((62, 24, 54), (18, 3, 14))
    control(d, "panel/materials/tab/model")
    d.say("And the Model tab now shows them all: brick, decking, metal and grass.")
    d.glide(*reveal(d, "panel/materials/material/library.grass.normal"))
    d.hold()
    d.finish("Thanks for watching", "Meshcraft tutorials · 8")
