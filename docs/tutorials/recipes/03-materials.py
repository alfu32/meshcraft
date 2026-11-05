"""Tutorial 3: materials on the side table from tutorial 1. New Wood, Apply, New Grass."""

from pathlib import Path
import shutil
import tempfile
import time

from director import ROOT

TAB = "Materials"
GRID = ("Grid", "Grid Line")
SOURCE = ROOT / "docs/tutorials/projects/01-side-table.octd"
# The title bar shows the open path, so a neutral copy keeps home directories out of the video.
PROJECT = Path(tempfile.gettempdir()) / "meshcraft-tutorials" / "side-table.octd"
SAVE_BUTTON = (1658, 991)


def setup(d):
    d.command("ui.update_preferences", accordion_settings_panels=True)
    PROJECT.parent.mkdir(exist_ok=True)
    shutil.copyfile(SOURCE, PROJECT)
    d.command("file.open", path=str(PROJECT))
    deadline = time.monotonic() + 60
    while d.rpc.status()["projectPath"] != str(PROJECT):
        d.require(time.monotonic() < deadline, "project opened")
        time.sleep(0.05)
    d.command("view.selection")
    d.pose((205, 95, 115), (0, 14, 0))


def new_material(d, command, record, hint):
    """The button opens the editor panel; saving there creates the catalog entry."""
    before = {m["id"] for m in d.scene()["materials"]}
    d.tool(command, tab=TAB)
    d.require(d.rpc.status()["workspace"]["titleToolsBlocked"], "material editor open")
    d.say(hint)
    d.hold()
    d.click_mark(*SAVE_BUTTON)
    d.command("material.save", material=record)
    d.wait(0.5)
    d.close_modal()
    d.require(not d.rpc.status()["workspace"]["titleToolsBlocked"], "material editor closed")
    created = [m for m in d.scene()["materials"] if m["id"] not in before]
    d.require(len(created) == 1, f"{command} created one material")
    return created[0]


def material_ids(d, path):
    page = d.rpc.call("meshcraft_geometry", dict(path=path, limit=1000))
    # A closed object reports its parts as occurrence targets; faces have three vertices.
    return {p["record"].get("extensions", {}).get("k3d.meshcraftMaterialId")
            for p in page["primitives"] if len(p["record"].get("vertices", [])) == 3}


def record(d):
    d.title("Materials: wood and grass", "Meshcraft tutorials · 3")
    d.say("Here's our table, units already set. Let's give it real materials.")
    d.hold()

    d.say("On the Materials tab, New Wood opens a procedural wood material.")
    wood = new_material(d, "material.new_wood", dict(
        id="table-oak", name="Table oak", color=dict(red=168, green=118, blue=68, alpha=255),
        shadingType="phong", textureScaleU=1, textureScaleV=1,
        procedural=dict(program="meshcraft.wood.v1", parameters={}, maxInstances=20000,
                        lodDistance=250, showBounds=False)),
        "The editor previews the material as you adjust it. Save adds it to the project.")
    d.hold()

    d.say("Select the table top, then choose Apply Material.")
    objects = d.scene()["objects"]
    top = next(o for o in objects if o["name"] == "Table top")
    d.click((0, 45, 0))
    d.tool("material.apply", tab=TAB, id=wood["id"])
    d.require(material_ids(d, top["path"]) == {wood["id"]}, f"wood on top: {material_ids(d, top['path'])}")
    d.rpc.select([])
    d.hold()

    d.say("Ooh, look at that grain! Materials belong to each instance, so select all four legs.")
    legs = [o for o in objects if o["name"] == "Leg"]
    d.require(len(legs) == 4, "four legs")
    d.click((-25, 15, 27.5))
    for spot in ((30, 15, 27.5), (-25, 15, -27.5), (30, 15, -27.5)):
        d.click(spot, add=True)
    d.require(len(d.selected()) == 4, f"four legs selected: {d.selected()}")
    d.tool("material.apply", tab=TAB, id=wood["id"])
    for leg in legs:
        d.require(material_ids(d, leg["path"]) == {wood["id"]}, "wood on every leg")
    d.rpc.select([])
    d.hold()

    d.say("Nice. For the ground, snap a large rectangle around the table.")
    d.tool("tool.builtin.rectangle")
    d.pick((-60, 0, 60), GRID)
    d.pick((60, 0, -60), GRID)
    d.escape()
    d.say("New Grass grows real blades on whatever surface it's applied to.")
    grass = new_material(d, "material.new_grass", dict(
        id="lawn", name="Lawn", color=dict(red=78, green=119, blue=45, alpha=255),
        shadingType="phong", textureScaleU=1, textureScaleV=1,
        procedural=dict(program="meshcraft.grass.v1",
                        parameters=dict(density=0.45, height=7, width=1.25, wind=0),
                        maxInstances=20000, lodDistance=500, showBounds=False)),
        "Set the blade height and density here, then Save.")
    d.hold()
    root = d.root()["path"]
    faces = d.primitives(root, kinds=("triangle",))
    d.require(len(faces) == 2, f"ground face, got {len(faces)}")
    # A drawing tool leaves its result selected, and the editor keeps that selection.
    d.require(len(d.selected()) >= 2, f"ground still selected: {d.selected()}")
    d.glide(*d.project((50, 0, -50)))
    d.tool("material.apply", tab=TAB, id=grass["id"])
    d.require(material_ids(d, root) == {grass["id"]}, "grass on the ground")
    d.rpc.select([])
    d.say("Wooow, real grass! Materials are saved with the project, editable at any time.")
    d.orbit(-180, 25)
    d.hold()
    d.finish("Next: a parametric bookcase", "Meshcraft tutorials · 4")
