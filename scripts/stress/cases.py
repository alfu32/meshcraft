"""Operation recipes, shared by local and CI execution. Never time tool activation alone."""

from dataclasses import dataclass
import time
import subprocess
from runtime import Failure

BULK_COMMANDS = {"tool.select", "tool.builtin.move", "tool.builtin.rotate"}

O = (0, 0, 0)
X = (1, 0, 0)
Y = (0, 1, 0)
Z = (0, 0, 1)
RECT = [(-3, 0, -3), (-1, 0, -1)]
PATH = [(-3, 0, -3), (-1, 0, -3), (-1, 0, -1)]
TRANSFORMS = {
    "move": [O, (3, 0, 0)],
    "stretch": [O, (3, 0, 0)],
    "scale": [O, X, (2, 0, 0)],
    "stretch_scale": [O, X, (2, 0, 0)],
    "squash": [O, X, (2, 0, 0)],
    "rotate": [O, Y, X, Z],
    "rotate_stretch": [O, Y, X, Z],
    "rotate_2": [O, X, Z],
    "move_copy": [O, (3, 0, 0)],
    "rotate_copy": [O, X, Z],
    "mirror": [O, Y],
}
ARRAYS = {
    "copy_multiple": [O, X, (3, 0, 0)],
    "planar_translate_multiple": [O, (2, 0, 2), (4, 0, 4)],
    "volumetric_translate_multiple": [O, (2, 2, 2), (4, 4, 4)],
    "planar_rotate_multiple": [O, X, Z, (-3, 0, 0)],
    "helicoidal_rotate_multiple": [O, X, (0, 2, 1), (-3, 4, 0)],
    "rotational_array": [O, X, Z, (-3, 0, 0)],
    "helical_array": [O, X, (0, 2, 1), (-3, 4, 0)],
    "rotational_array_equalized": [O, X, Z, (-3, 0, 0)],
    "helical_array_equalized": [O, X, (0, 2, 1), (-3, 4, 0)],
}
DRAW = {
    "line": [(-3, 0, -3), (-1, 0, -3)],
    "construction_line": [(-3, 0, -3), (-1, 0, -3)],
    "rectangle": RECT,
    "surface_rectangle": RECT,
    "circle": [(-3, 0, -3), (-2, 0, -3)],
    "quad": [(-3, 0, -3), (-1, 0, -3), (-1, 0, -1)],
    "cuboid": [(-3, 0, -3), (-1, 2, -1)],
    "primitive_sphere": [(-3, 0, -3), (-2, 0, -3)],
    "primitive_cylinder": [(-3, 0, -3), (-2, 0, -3), (-3, 2, -3)],
    "primitive_cone": [(-3, 0, -3), (-2, 0, -3), (-3, 2, -3)],
    "primitive_pill": [(-3, 0, -3), (-2, 0, -3), (-3, 2, -3)],
    "text": [(-3, 0, -3)],
    "linear_dimension": [(-3, 0, -3), (-1, 0, -3), (-1, 0, -2)],
    "leader": [(-3, 0, -3), (-2, 0, -2), (-1, 0, -2)],
    "arch_wall": [(-3, 0, -3), (-1, 0, -3)],
    "arch_slab": RECT,
    "arch_window_frame": RECT,
    "arch_door_frame": RECT,
}
PATHS = {
    "polyline",
    "mesh",
    "double_line",
    "ribbon_3d",
    "arch_slab_freeform",
    "extrude_swipe",
    "extrude_swipe_3d",
}
SCENE_COMMANDS = {
    "geometry.add_cuboid",
    "paint.apply_selection",
    "geometry.flip_selection",
    "object.array_selection",
    "edit.duplicate_selection",
    "edit.delete_selection",
    "object.create_from_selection",
    "edit.group",
    "object.explode_selection",
    "edit.ungroup",
    "geometry.fill_loops",
    "tool.builtin.polyline_mesh",
    "tool.builtin.face_outline",
    "tool.builtin.solid_union",
    "tool.builtin.solid_intersection",
    "tool.builtin.solid_subtraction",
    "tool.builtin.mesh_intersection",
    "tool.builtin.cut_holes",
    "tool.builtin.cut_holes_2",
    "tool.builtin.cut_out_3",
}
# Commands whose intended action is a UI/session transition, not a geometry operation.
SESSION = {
    "file.open_recent",
    "help.about",
    "file.cancel_open",
    "interchange.cancel",
    "interchange.formats",
    "ui.update_preferences",
    "model.update_settings",
    "selection.update_display",
    "paint.set_color",
    "document.snapshot",
    "edit.reset_scene_for_capture",
    "file.new",
    "app.quit",
    "annotation.update",
}


@dataclass(frozen=True)
class Recipe:
    family: str
    operation: str


def recipe(command):
    key = command["id"]
    name = key.removeprefix("tool.builtin.").removeprefix("tool.")
    if command["capability"] == "deferred":
        return Recipe("unsupported", "Registry explicitly defers this command")
    if key in (
        "tool.select",
        "tool.builtin.select",
        "selection.set",
        "edit.clear_selection",
    ):
        return Recipe(
            "selection",
            "Select/clear exact instance targets and wait for a fresh viewport",
        )
    if name in TRANSFORMS:
        return Recipe(
            "transform",
            "Pick reference vectors, evaluate phantom preview, commit selected instance transform",
        )
    if name in ARRAYS:
        return Recipe(
            "array",
            "Pick array vectors and span; preview and commit multiple real copies",
        )
    if name in DRAW:
        return Recipe(
            "draw",
            "Complete point-driven construction with preview and verify a document edit",
        )
    if name in PATHS:
        return Recipe(
            "path",
            "Accept a multipoint path, evaluate feedback and finish its generated geometry",
        )
    if key in SCENE_COMMANDS:
        return Recipe(
            "mesh",
            "Execute the selected geometry operation and verify a committed edit",
        )
    if key.startswith("view."):
        return Recipe("view", "Change the real desktop camera, guide or panel state")
    if key.startswith("tool.cycle."):
        return Recipe("cycle", "Cycle tool then complete the resulting tool operation")
    if key.startswith("tool."):
        return Recipe(
            "special_tool",
            "Complete the specific active-tool workflow with its required input",
        )
    if key.startswith("hotspot."):
        return Recipe(
            "hotspot", "Author or manipulate a real hotspot and its attached geometry"
        )
    if key.startswith("architecture."):
        return Recipe(
            "architecture",
            "Create/modify a canonical architectural owner or owned opening",
        )
    if key.startswith("material."):
        return Recipe(
            "material",
            "Create/edit/apply the actual material or open its intended editor",
        )
    if key == "selection.update_display":
        return Recipe("session", "Update actual per-type selection display policy")
    if (
        key.startswith("object.")
        or key.startswith("selection.")
        or key.startswith("script.")
    ):
        return Recipe(
            "object", "Execute the intended prototype/instance/editor operation"
        )
    if key.startswith("file.") or key in (
        "import.mesh",
        "export.mesh",
        "export.ifc_model",
    ):
        return Recipe("file", "Complete the file job using temporary explicit paths")
    if key.startswith("edit."):
        return Recipe(
            "edit",
            "Exercise clipboard/history/document operation with valid prerequisites",
        )
    if key in SESSION:
        return Recipe(
            "session", "Execute the actual state/read operation with typed parameters"
        )
    if key.startswith("paint.") or key == "annotation.update":
        return Recipe(
            "session", "Update the configured property or selected annotation"
        )
    raise Failure(f"No performance recipe for registered command {key}")


def validate_observed(key, before, after):
    # World picks can report a rejected transaction in status rather than throwing
    # an RPC error. Never label these attempts successful solely on HTTP success.
    status = after.get("status", "")
    if "undo history needs " in status and "byte budget" in status:
        raise Failure(status)
    if (
        key
        in {"hotspot.begin_placement", "tool.copy_mode", "tool.distance", "tool.finish"}
        and before["revision"] == after["revision"]
    ):
        raise Failure(f"{key} did not commit its prepared point operation: {status}")


class Exercise:
    def __init__(self, rpc, work, env):
        self.rpc = rpc
        self.work = work
        self.env = env
        self.timings = {}
        self.steps = []
        self.cache = {}
        self.variant = "standard"

    def timed(self, stage, function, *args, **kwargs):
        start = time.perf_counter_ns()
        owner = self.rpc.owner_ms_total
        seen = self.rpc.owner_count
        try:
            result = function(*args, **kwargs)
            if isinstance(result, dict) and "renderFrameMs" in result:
                self.timings["render_frame_ms"] = result["renderFrameMs"]
            return result
        finally:
            self.timings[stage] = (
                self.timings.get(stage, 0) + (time.perf_counter_ns() - start) / 1e6
            )
            if self.rpc.owner_count > seen:
                self.timings["mcp_owner_thread_ms"] = self.timings.get(
                    "mcp_owner_thread_ms", 0
                ) + (self.rpc.owner_ms_total - owner)

    def cmd(self, key, args=None, stage=None):
        self.steps.append(key)
        if stage:
            return self.timed(stage, self.rpc.command, key, args)
        return self.rpc.command(key, args)

    def setup(self, function, *args, **kwargs):
        """Reuse a validated recipe as a prerequisite without timing it as this tool."""
        saved = self.timings
        self.timings = {}
        try:
            return function(*args, **kwargs)
        finally:
            self.timings = saved

    def point(self, action, p, stage):
        self.steps.append(f"pointer.{action}({p})")
        return self.timed(stage, self.rpc.pointer, action, p)

    def scene(self):
        return self.rpc.call("meshcraft_scene", dict(limit=32))

    def obj(self, name="Benchmark Cube", index=0):
        if name not in self.cache:
            self.cache[name] = [o for o in self.scene()["objects"] if o["name"] == name]
        return self.cache[name][index]

    def geometry(self, name="Benchmark Cube"):
        offset = 0
        combined = None
        while True:
            value = self.rpc.call(
                "meshcraft_geometry",
                dict(path=self.obj(name)["path"], offset=offset, limit=1000),
            )
            if combined is None:
                combined = value
            else:
                combined["primitives"].extend(value["primitives"])
            offset = value["nextOffset"]
            if offset is None:
                return combined

    def sources(self, name="Benchmark Cube", kind="occurrence", both=False):
        if (
            self.variant == "fragment"
            and name == "Benchmark Cube"
            and kind == "occurrence"
        ):
            name = "Benchmark Fragment"
            kind = "all"
        if kind == "occurrence":
            objects = [self.obj(name, i) for i in range(2 if both else 1)]
            targets = [o["target"] for o in objects]
        else:
            obj = self.obj(name)
            if self.rpc.status()["editContext"].get("path") != obj["path"]:
                if self.rpc.status()["editContext"]["kind"] != "document":
                    self.cmd("selection.exit_context")
                self.rpc.select([obj["target"]])
                self.cmd("selection.edit_object")
            targets = [
                p["target"]
                for p in self.geometry(name)["primitives"]
                if kind == "all" or p["target"]["kind"] == kind
            ]
            if kind == "triangle":
                targets = targets[:2]
        self.rpc.select(targets)
        return targets

    def picks(self, key, points, finish=False):
        before = self.rpc.status()
        self.cmd(key, stage="start_ms")
        for i, p in enumerate(points):
            if i == len(points) - 1:
                preview_state = self.point("move", p, "preview_ms")
                self.timed("preview_ms", self.rpc.frame)
            picked = self.point(
                "click",
                p,
                "commit_ms"
                if i == len(points) - 1 and (not finish or key == "tool.builtin.mesh")
                else "input_ms",
            )
            if (
                i == len(points) - 1
                and (not finish or key == "tool.builtin.mesh")
                and picked["revision"] == preview_state["revision"]
            ):
                raise Failure(
                    "Final pick did not commit the operation: "
                    + picked.get("status", "")
                )
        if finish and self.rpc.status()["activeTool"] != "Select":
            self.timed("commit_ms", self.rpc.pointer, "finish")
        self.changed(before)

    def changed(self, before):
        after = self.rpc.status()
        if after["revision"] == before["revision"]:
            raise Failure(
                f"Tool did not commit geometry: {after['activeTool']}: {after['status']}"
            )
        return after

    def mutate(self, key, args=None):
        before = self.rpc.status()
        self.cmd(key, args, "commit_ms")
        return self.changed(before)

    def execute(self, command):
        key = command["id"]
        name = key.removeprefix("tool.builtin.").removeprefix("tool.")
        family = recipe(command).family
        if family == "unsupported":
            return "unsupported"
        if family == "selection" and self.variant == "fragment":
            targets = self.sources()
            self.rpc.select([])
            self.cmd(key, stage="start_ms")
            self.timed("commit_ms", self.rpc.select, targets)
            if len(self.rpc.status()["selection"]) != len(targets):
                raise Failure("Bulk selection count differs")
        elif family == "selection":
            self.cmd(
                key,
                {"paths": [self.obj()["path"]]} if key == "selection.set" else None,
                "start_ms",
            )
            if key == "edit.clear_selection":
                # Preselect is installed by prepare().
                if self.rpc.status()["selection"]:
                    raise Failure("Clear Selection left selected targets")
            else:
                self.timed("commit_ms", self.rpc.select, [self.obj()["target"]])
                if not self.rpc.status()["selection"]:
                    raise Failure("Selection stayed empty")
        elif family in ("transform", "array"):
            self.sources()
            before = self.rpc.status()
            self.picks(key, (TRANSFORMS if family == "transform" else ARRAYS)[name])
            self.changed(before)
        elif family in ("draw", "path"):
            if name.startswith("extrude_swipe"):
                self.sources("Benchmark Profile", "segment")
            before = self.rpc.status()
            self.picks(
                key,
                DRAW[name]
                if family == "draw"
                else [O, (6, 0, 0), (6, 3 if name.endswith("3d") else 0, 6)]
                if name.startswith("extrude_swipe")
                else PATH,
                finish=family == "path",
            )
            self.changed(before)
        elif family == "mesh":
            self.mesh(key)
        elif family == "special_tool":
            self.special(key, name)
        elif family == "architecture":
            self.architecture(key)
        elif family == "material":
            self.material(key)
        elif family == "hotspot":
            self.hotspot(key)
        elif family == "object":
            self.object(key)
        elif family == "edit":
            self.edit(key)
        elif family == "file":
            self.file(key)
        elif family == "view":
            self.view(key)
        elif family == "cycle":
            self.cmd(key, stage="start_ms")
            active = self.rpc.status()["activeTool"]
            # Initial defaults pick the first concrete tool in each cycle.
            aliases = {
                c["label"]: c["id"]
                for c in self.catalog
                if c["id"].startswith("tool.") and not c["id"].startswith("tool.cycle.")
            }
            aliases["Select"] = "tool.select"
            if active not in aliases:
                raise Failure(f"Unhandled cycle result: {active}")
            target = next(c for c in self.catalog if c["id"] == aliases[active])
            if self.execute(target) != "passed":
                raise Failure(f"Cycle selected an unsupported tool: {active}")
        elif family == "session":
            self.session(key)
        return "passed"

    def mesh(self, key):
        args = None
        if key == "geometry.add_cuboid":
            args = dict(dimensions=[2, 2, 2], origin=[-3, 0, -3])
        elif key in ("object.explode_selection", "edit.ungroup"):
            self.sources()
        elif "solid_" in key or "mesh_intersection" in key:
            self.sources(both=True)
        elif key in ("geometry.fill_loops", "tool.builtin.polyline_mesh"):
            self.sources("Benchmark Fill", "segment")
        elif key in ("object.create_from_selection", "edit.group"):
            self.sources(kind="triangle")
        else:
            self.sources()
        if key == "paint.apply_selection":
            self.sources(kind="triangle")
        if "face_outline" in key or key == "geometry.flip_selection":
            self.sources(kind="triangle")
        if "cut_holes" in key or "cut_out" in key:
            # A planar face and nested closed loop in the same prototype.
            self.sources("Benchmark Fill")
            self.cmd("selection.edit_object")
            self.setup(self.picks, "tool.rectangle", [(5, 0, 0), (8, 0, 3)])
            faces = self.rpc.status()["selection"]
            self.setup(
                self.picks,
                "tool.builtin.polyline",
                [
                    (5.5, 0, 0.5),
                    (6.5, 0, 0.5),
                    (6.5, 0, 1.5),
                    (5.5, 0, 1.5),
                    (5.5, 0, 0.5),
                ],
                False,
            )
            self.rpc.select(faces + self.rpc.status()["selection"])
        self.mutate(key, args)

    def special(self, key, name):
        if name in ("object_place",) or key == "object.place_selected_definition":
            self.sources()
            before = self.rpc.status()
            self.picks(key, [(-4, 0, -4)])
            self.changed(before)
        elif name == "push_pull":
            self.sources(kind="triangle")
            before = self.rpc.status()
            self.picks(key, [O, (0, 0, 3)])
            self.changed(before)
        elif name == "revolve":
            self.sources("Benchmark Revolve", "segment")
            before = self.rpc.status()
            self.picks(key, [O, Y], True)
            self.changed(before)
        elif name == "line_offset":
            self.sources("Benchmark Fill", "segment")
            before = self.rpc.status()
            self.picks(key, [(5, 0, 0), (5, 0, -1)])
            self.changed(before)
        elif name == "arch_stair":
            self.sources("Benchmark Contour")
            before = self.rpc.status()
            self.cmd(key, stage="start_ms")
            self.sources("Benchmark Walking")
            self.cmd("architecture.stair_path", stage="commit_ms")
            self.changed(before)
        elif name == "arch_add_hole":
            self.make_slab()
            before = self.rpc.status()
            self.picks(key, [(-2, 0, -2), (-1, 0, -1)])
            self.changed(before)
        elif name == "plane_section":
            self.sources()
            before = self.rpc.status()
            self.picks(key, [(1, 1, 1), (2, 1, 1)])
            self.changed(before)
        elif name == "paint":
            self.sources(kind="triangle")
            self.cmd("paint.set_color", dict(red=250, green=40, blue=50, alpha=255))
            self.cmd(
                "view.camera.set_pose", dict(position=[1, 1, 12], target=[1, 1, 1])
            )
            self.rpc.frame()
            before = self.rpc.status()
            self.cmd(key, stage="start_ms")
            self.timed(
                "commit_ms",
                subprocess.run,
                ["xdotool", "mousemove", "545", "533", "click", "1"],
                env=self.env,
                check=True,
            )
            self.changed(before)
        elif name in ("finish", "undo_point", "cancel", "distance"):
            self.cmd(
                "tool.builtin.polyline"
                if name in ("finish", "undo_point")
                else "tool.line"
            )
            self.rpc.pointer("click", (-3, 0, -3))
            if name in ("finish", "undo_point"):
                self.rpc.pointer("click", (-1, 0, -3))
            if name == "distance":
                self.cmd(key, stage="start_ms")
                self.point("move", (-1, 0, -1), "preview_ms")
                self.timed("preview_ms", self.rpc.frame)
                self.timed(
                    "input_ms",
                    self.rpc.call,
                    "k3d_pointer_event",
                    dict(action="text", text="2"),
                )
                self.timed("commit_ms", self.rpc.pointer, "finish")
            else:
                self.cmd(key, stage="commit_ms")
        elif name == "copy_mode":
            self.sources()
            self.cmd("tool.builtin.move")
            self.cmd(key, stage="start_ms")
            self.point("click", O, "input_ms")
            self.point("move", X, "preview_ms")
            self.timed("preview_ms", self.rpc.frame)
            self.point("click", X, "commit_ms")
        else:
            raise Failure(f"Missing concrete tool recipe: {key}")

    def make_slab(self):
        self.cmd(
            "architecture.create_rectangular_slab",
            dict(points=[[-3, 0, -3], [3, 0, 3]]),
        )
        return self.rpc.status()["selection"][0]

    def architecture(self, key):
        args = {}
        if key.endswith("create_wall"):
            args = dict(start=[-3, 0, -3], end=[3, 0, -3])
        elif key.endswith("create_rectangular_slab"):
            args = dict(points=[[-3, 0, -3], [3, 0, 3]])
        elif key.endswith("create_slab"):
            args = dict(points=[[-3, 0, -3], [3, 0, -3], [3, 0, 3], [-3, 0, 3]])
        elif key.endswith("create_frame"):
            args = dict(points=[[-3, 0, -3], [3, 0, 3]], normal=[0, 1, 0])
        elif key.endswith("create_stair"):
            args = dict(
                contour=[[-8, 0, -2], [-4, 0, -2], [-4, 0, 4], [-8, 0, 4]],
                walking_path=[[-6, 0, -2], [-6, 0, 4]],
            )
        elif key.endswith("settings"):
            args = dict(wall_thickness=1.1)
        elif key.endswith("stair_contour") or key.endswith("stair_path"):
            self.sources("Benchmark Contour")
            self.cmd("tool.builtin.arch_stair")
            if key.endswith("stair_path"):
                self.sources("Benchmark Walking")
            self.cmd(key, stage="commit_ms")
            return
        else:
            target = self.make_slab()
            args = dict(occurrence=dict(path=target["path"]), id=target["id"])
            if key.endswith("add_hole") or key.endswith("remove_hole"):
                args.update(start=[-2, 0, -2], end=[-1, 0, -1])
                if key.endswith("remove_hole"):
                    result = self.cmd("architecture.add_hole", args)
                    # Owner keeps its hole records; query selected record from document snapshot is unnecessary.
                    args["hole_id"] = result["dispatch"]["result"]["outputs"][
                        "architecture"
                    ]["hole_id"]
            elif key.endswith("move_slab_vertex"):
                args.update(index=0, position=[-4, 0, -4])
            elif key.endswith("move_control"):
                args.update(
                    control={"kind": "slab_vertex", "index": 0}, position=[-4, 0, -4]
                )
            else:
                args.update(slab_thickness=1.2)
        if key.endswith("settings"):
            self.cmd(key, args, "commit_ms")
        else:
            self.mutate(key, args)

    def edit(self, key):
        self.sources()
        if key in ("edit.copy_selection", "edit.cut_selection", "edit.copy_as_script"):
            result = self.cmd(key, stage="commit_ms")
            if not result.get("clipboard"):
                raise Failure("Clipboard command produced no payload")
        elif key in ("edit.paste", "edit.paste_in_place"):
            clipboard = self.cmd("edit.copy_selection")["clipboard"]
            before = self.rpc.status()
            self.cmd(key, dict(clipboard=clipboard), "start_ms")
            if key == "edit.paste":
                self.point("move", (-5, 0, -5), "preview_ms")
                self.timed("preview_ms", self.rpc.frame)
                self.point("click", (-5, 0, -5), "commit_ms")
            self.changed(before)
        elif key in ("edit.undo", "edit.redo"):
            self.cmd(
                "geometry.add_cuboid", dict(dimensions=[1, 1, 1], origin=[-3, 0, -3])
            )
            if key == "edit.redo":
                self.cmd("edit.undo")
            self.mutate(key)
        elif key == "edit.reset_scene_for_capture":
            self.cmd(key, stage="commit_ms")
        else:
            raise Failure(f"Missing edit recipe {key}")

    def cancel_job(self, key):
        project = key == "file.cancel_open"
        path = self.work / ("cancel-input.octd" if project else "cancel-input.obj")
        if not path.exists():
            with path.open("wb") as stream:
                stream.write(
                    self.fixture.read_bytes()
                    if project
                    else b"v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 2 3\n"
                )
                block = b" " * (1024 * 1024) if project else b"#\n" * (1024 * 1024 // 2)
                # Many short valid comments keep the parser pending while a large
                # resident scene's startup response is rendered. A few long lines
                # could finish before cancellation was delivered. Stay below the
                # real importer's 512 MiB input limit; this is setup, not timing.
                for _ in range(128 if project else 384):
                    stream.write(block)
        args = dict(path=str(path))
        if not project:
            args["position"] = [-5, 0, -5]
        begin = self.rpc.call(
            "k3d_execute_command",
            dict(commandId="file.open" if project else "import.mesh", arguments=args),
        )
        if begin.get("status", {}).get("pendingDialog"):
            self.rpc.call("meshcraft_resolve_document", dict(action="discard"))
        self.cmd(key, stage="commit_ms")
        completion = begin["dispatch"]["result"]["completion"]
        if completion["state"] == "pending":
            deadline = time.monotonic() + self.rpc.timeout
            while True:
                job = self.rpc.call(
                    "meshcraft_get_job", dict(job_id=completion["job_id"])
                )
                if job["result"]["completion"]["state"] != "pending":
                    break
                if time.monotonic() > deadline:
                    raise Failure("Cancellation did not complete")
                time.sleep(0.01)
            if job["result"]["completion"]["state"] != "failed":
                raise Failure("Job finished instead of cancelling")
            if "cancel" not in str(job["result"].get("diagnostics", [])).lower():
                raise Failure(
                    "Job failed without cancellation evidence: " + str(job["result"])
                )

    def file(self, key):
        if key == "file.cancel_open":
            self.cancel_job(key)
            return
        if key in ("file.open_recent",):
            self.cmd(key, stage="commit_ms")
            return
        if key == "file.new":
            self.cmd(key, stage="commit_ms")
            return
        if key in ("export.mesh", "export.ifc_model", "file.save", "file.save_as"):
            suffix = (
                ".obj"
                if key == "export.mesh"
                else ".ifc"
                if key == "export.ifc_model"
                else ".octd"
            )
            path = self.work / ("output" + suffix)
            self.cmd(
                key,
                dict(
                    path=str(path),
                    **({"selection_only": False} if key.startswith("export.") else {}),
                ),
                "commit_ms",
            )
            if not path.exists() or not path.stat().st_size:
                raise Failure("File job did not write output")
        elif key == "file.open":
            self.cmd(key, dict(path=str(self.fixture)), "commit_ms")
        elif key == "import.mesh":
            path = self.work / "import.obj"
            path.write_text("v -3 0 -3\nv -2 0 -3\nv -3 0 -2\nf 1 2 3\n")
            self.mutate(key, dict(path=str(path), position=[-5, 0, -5]))
        else:
            raise Failure(f"Missing file recipe {key}")

    def view(self, key):
        if key in ("view.orbit", "view.camera.orbit"):
            self.cmd(key, stage="start_ms")
            before = self.rpc.status()["camera"]

            def drag():
                subprocess.run(
                    [
                        "xdotool",
                        "mousemove",
                        "500",
                        "400",
                        "mousedown",
                        "3",
                        "mousemove",
                        "550",
                        "430",
                        "mouseup",
                        "3",
                    ],
                    env=self.env,
                    check=True,
                )

            self.timed("commit_ms", drag)
            if self.rpc.status()["camera"] == before:
                raise Failure("Orbit did not move camera")
        elif key == "view.camera.set_pose":
            self.cmd(key, dict(position=[15, 20, 15], target=[0, 0, 0]), "commit_ms")
        elif key in ("view.axis_guide", "view.grid_guide", "view.guide.remove"):
            self.rpc.pointer("move", O)
            if key == "view.guide.remove":
                self.cmd("view.axis_guide")
            self.cmd(key, stage="commit_ms")
            if key == "view.guide.remove":
                self.point("click", O, "commit_ms")
        else:
            self.cmd(key, stage="commit_ms")

    def session(self, key):
        args = {}
        if key == "interchange.cancel":
            self.cancel_job(key)
            return
        if key == "model.update_settings":
            args = dict(unit_size=2)
        elif key == "ui.update_preferences":
            args = dict(black_model_lines=False)
        elif key == "selection.update_display":
            args = dict(kind="face", property="wireframe", enabled=True)
        elif key == "paint.set_color":
            args = dict(red=200, green=20, blue=20, alpha=255)
        elif key == "annotation.update":
            self.setup(self.picks, "tool.builtin.text", [(-5, 0, -5)])
            args = dict(text="Benchmark label", size=0.3)
        elif key == "paint.apply_selection":
            self.sources(kind="triangle")
        elif key == "app.quit":
            # Actual shutdown is measured last in a tier, not replaced by a no-op.
            self.cmd(key, stage="commit_ms")
            return
        self.cmd(key, args, "commit_ms")

    def object(self, key):
        self.sources()
        if key in ("selection.edit_object", "selection.exit_context"):
            if key.endswith("exit_context"):
                self.cmd("selection.edit_object")
            self.cmd(key, stage="commit_ms")
        elif key in ("object.place_selected_definition", "object.toggle_place_glue"):
            if key.endswith("toggle_place_glue"):
                self.cmd("object.place_selected_definition")
            self.special(
                "object.place_selected_definition", "object_place"
            ) if key.endswith("definition") else self.cmd(key, stage="commit_ms")
        elif key.startswith("object.delete"):
            self.cmd("object.create_from_selection", dict(name="Temporary prototype"))
            definition = next(
                p["id"]
                for p in self.scene()["prototypes"]
                if p["name"] == "Temporary prototype"
            )
            if "unused" in key:
                self.cmd("edit.delete_selection")
            self.mutate(
                key,
                dict(
                    definition_id=definition,
                    **(
                        {"confirmed": True} if key == "object.delete_definition" else {}
                    ),
                ),
            )
        elif key in ("object.import_scene", "object.reference_scene"):
            self.cmd(key, dict(path=str(self.external)), "start_ms")
            self.picks("object.place_selected_definition", [(-5, 0, -5)])
        elif key == "object.refresh_reference":
            self.cmd("object.reference_scene", dict(path=str(self.external)))
            self.cmd(key, stage="commit_ms")
        elif key == "object.new_tree":
            self.cmd(key, stage="commit_ms")
        elif key == "object.configure_procedural":
            self.rpc.select([])
            self.mutate(key)
            self.picks("object.place_selected_definition", [(-5, 0, -5)])
        elif key in ("object.edit_tree", "script.new", "script.edit"):
            if key == "object.edit_tree":
                self.cmd("object.configure_procedural")
            self.cmd(key, stage="commit_ms")
            self.timed(
                "commit_ms",
                subprocess.run,
                ["xdotool", "key", "Escape"],
                env=self.env,
                check=True,
            )
        else:
            raise Failure(f"Missing object recipe {key}")

    def material(self, key):
        if key == "material.save":
            self.mutate(
                key,
                dict(
                    material=dict(
                        id="benchmark",
                        name="Benchmark",
                        color=dict(red=50, green=90, blue=180, alpha=255),
                    )
                ),
            )
        elif key in ("material.apply", "material.parameters"):
            self.sources()
            self.cmd(
                "material.save",
                dict(
                    material=dict(
                        id="benchmark-grass",
                        name="Benchmark Grass",
                        procedural=dict(program="meshcraft.grass.v1"),
                    )
                ),
            ) if key.endswith("parameters") else None
            self.mutate(
                key,
                dict(
                    id="benchmark-grass" if key.endswith("parameters") else "ansi-red",
                    **(
                        {"parameters": {"height": 1.2}}
                        if key.endswith("parameters")
                        else {}
                    ),
                ),
            )
        elif key == "material.delete":
            self.setup(self.material, "material.save")
            self.mutate(key, dict(id="benchmark"))
        elif key == "material.program":
            program = self.scene()["proceduralPrograms"][0]
            program["id"] = "benchmark-program"
            program["name"] = "Benchmark program"
            self.mutate(key, dict(program=program))
        else:
            self.cmd(key, stage="commit_ms")

    def hotspot(self, key):
        self.sources()
        self.cmd("selection.edit_object")
        if key in ("hotspot.begin_placement", "hotspot.cancel_placement"):
            if key.endswith("cancel_placement"):
                self.cmd("hotspot.begin_placement")
            self.cmd(key, stage="start_ms")
            if key.endswith("begin_placement"):
                self.point("click", (-3, 0, -3), "commit_ms")
            return
        self.cmd("hotspot.add", dict(position=dict(x=0, y=0, z=0)))
        target = self.rpc.status()["selection"][0]
        args = {}
        if key == "hotspot.add":
            args = dict(position=dict(x=1, y=0, z=0))
        elif key == "hotspot.update":
            args = dict(
                name="Benchmark handle",
                operation="MOVE",
                shape="CIRCLE",
                color="#00ffffff",
            )
        elif key == "hotspot.attach":
            self.sources(kind="triangle")
            self.rpc.select(self.rpc.status()["selection"] + [target])
        elif key in ("hotspot.target", "hotspot.select_attached"):
            self.sources(kind="triangle")
            self.rpc.select(self.rpc.status()["selection"] + [target])
            self.cmd("hotspot.attach")
            self.rpc.select([target])
            if key == "hotspot.target":
                self.cmd("selection.exit_context")
                self.rpc.select([target])
                args = dict(target=dict(x=1, y=0, z=0))
        self.cmd(key, args, "commit_ms")
        if key == "hotspot.reference":
            self.point("click", X, "commit_ms")

    def prepare(self, command):
        if not hasattr(self, "catalog"):
            self.catalog = [
                x["command"] for x in self.rpc.call("k3d_list_commands")["commands"]
            ]
        if command["id"] == "edit.clear_selection":
            self.sources()
        self.rpc.frame()
