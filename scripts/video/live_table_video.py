#!/usr/bin/env python3
"""Capture a side-table demonstration from an existing Meshcraft MCP session.

No application is launched. The recipe uses the interactive modeling tools,
records their requests and captures the live 3D viewport at each stage.
"""

import argparse
import base64
import json
import math
from pathlib import Path
import time
import urllib.request


class LiveTable:
    def __init__(self, endpoint, directory):
        self.endpoint = endpoint
        self.directory = directory.resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        (self.directory / "frames").mkdir(exist_ok=True)
        self.actions = []
        self.chapters = []
        self.frame_index = 0

    def call(self, name, arguments=None):
        payload = dict(jsonrpc="2.0", id=len(self.actions) + 1, method="tools/call",
                       params=dict(name=name, arguments=arguments or {}))
        request = urllib.request.Request(self.endpoint, json.dumps(payload).encode(),
                                        {"Content-Type": "application/json",
                                         "Accept": "application/json, text/event-stream"})
        with urllib.request.urlopen(request, timeout=60) as response:
            result = json.load(response)
        if "error" in result:
            raise RuntimeError(str(result["error"]))
        result = result["result"]
        if result.get("isError"):
            raise RuntimeError(str(result.get("content")))
        value = result.get("structuredContent")
        if value is None:
            blocks = [c["text"] for c in result.get("content", []) if c.get("type") == "text"]
            value = json.loads(blocks[0]) if blocks else {}
        self.actions.append(dict(tool=name, arguments=arguments or {},
                                 revision=value.get("revision"), generation=value.get("generation")))
        (self.directory / "mcp-actions.json").write_text(json.dumps(self.actions, indent=2) + "\n")
        return value, result

    def command(self, command, **arguments):
        value, _ = self.call("k3d_execute_command", dict(commandId=command, arguments=arguments))
        completion = value["dispatch"]["result"]["completion"]
        deadline = time.monotonic() + 60
        while completion["state"] == "pending":
            job, _ = self.call("meshcraft_get_job", dict(job_id=completion["job_id"]))
            completion = job["result"]["completion"]
            if time.monotonic() > deadline:
                raise RuntimeError(f"Command timed out: {command}")
            time.sleep(0.05)
        if completion["state"] != "completed":
            raise RuntimeError(f"Command did not complete: {command}: {completion}")

    def pointer(self, action, point=None, text=None):
        arguments = dict(action=action)
        if point is not None:
            arguments.update(zip(("worldX", "worldY", "worldZ"), point))
        if text is not None:
            arguments["text"] = text
        self.call("k3d_pointer_event", arguments)

    def scene(self):
        return self.call("meshcraft_scene")[0]

    def capture(self, preview=False):
        for attempt in range(20):
            try:
                # MCP preview means an object-script editor preview, not a
                # modeling tool's live wireframe. The normal canvas includes it.
                value, result = self.call("meshcraft_capture_view", dict(max_width=1280))
                if not value.get("fresh"):
                    raise RuntimeError("stale capture")
                break
            except RuntimeError as error:
                if "retry" not in str(error).lower() and "stale" not in str(error).lower():
                    raise
                if attempt == 19:
                    raise
                time.sleep(0.15)
        image = next(c for c in result["content"] if c["type"] == "image")
        path = Path("frames") / f"{self.frame_index:05}.png"
        (self.directory / path).write_bytes(base64.b64decode(image["data"]))
        self.frame_index += 1
        self.chapters[-1]["frames"].append(str(path))

    def chapter(self, title, text):
        print(title, flush=True)
        self.chapters.append(dict(title=title, text=text, frames=[]))

    def geometry(self, kinds):
        value, _ = self.call("meshcraft_geometry", dict(path=[], limit=1000))
        return [p["target"] for p in value["primitives"] if p["target"]["kind"] in kinds]

    def select(self, targets):
        self.call("meshcraft_select", dict(targets=targets))

    def sweep(self, start, end, steps=7):
        for i in range(1, steps + 1):
            f = i / steps
            self.pointer("move", [a + (b - a) * f for a, b in zip(start, end)])
            self.capture(preview=True)

    def pull(self, near, far, height):
        self.command("tool.builtin.rectangle")
        self.pointer("click", (near[0], 0, near[1]))
        self.sweep((near[0], 0, near[1]), (far[0], 0, far[1]))
        self.pointer("click", (far[0], 0, far[1]))
        self.pointer("cancel")
        self.capture()
        self.command("tool.builtin.push_pull")
        self.select(self.geometry(("triangle",)))
        middle = ((near[0] + far[0]) / 2, 0, (near[1] + far[1]) / 2)
        self.pointer("click", middle)
        self.sweep(middle, (middle[0], height, middle[2]))
        self.pointer("text", text=str(height))
        self.pointer("finish")
        self.pointer("cancel")
        self.capture()

    def make_object(self, name):
        self.select(self.geometry(("segment", "triangle")))
        self.command("object.create_from_selection", name=name)
        return next(o["target"] for o in self.scene()["objects"] if o["name"] == name)

    def run(self):
        self.call("k3d_contract")
        self.call("k3d_list_commands")
        status, _ = self.call("k3d_status")
        scene = self.scene()
        if status["unsavedChanges"] or any(o["counts"]["triangles"] or o["counts"]["segments"]
                                           for o in scene["objects"]):
            raise RuntimeError("This recipe requires the inspected saved, empty document; it will not replace existing work.")
        self.command("file.save_as", path=str(self.directory / "side-table.octd"))
        self.command("model.update_settings", unit_name="cm", unit_size=1, grid_size=5)
        self.command("view.camera.set_pose", position=[115, 90, 125], target=[0, 20, 0], field_of_view=36)

        self.chapter("A SIDE TABLE IN MESHCRAFT", "Build a simple side table with Rectangle, Push/Pull, Make Object, Move, and Move Copy.")
        self.capture()
        self.chapter("01  DRAW THE TABLETOP", "Choose Rectangle. Pick opposite corners to draw a tabletop sixty centimeters wide and deep.")
        self.command("tool.builtin.rectangle")
        self.pointer("click", (-30, 0, -30))
        self.sweep((-30, 0, -30), (30, 0, 30))
        self.pointer("click", (30, 0, 30))
        self.pointer("cancel")
        self.capture()

        self.chapter("02  ADD THICKNESS", "Choose Push/Pull, select the face, and pull it upward. Type three for the tabletop thickness.")
        self.command("tool.builtin.push_pull")
        self.select(self.geometry(("triangle",)))
        self.pointer("click", (0, 0, 0))
        self.sweep((0, 0, 0), (0, 3, 0))
        self.pointer("text", text="3")
        self.pointer("finish")
        self.pointer("cancel")
        self.capture()

        self.chapter("03  MAKE AN OBJECT", "Select the tabletop geometry and choose Make Object. This keeps it separate from the legs.")
        top = self.make_object("Table top")
        self.capture()
        self.chapter("04  RAISE THE TOP", "With Move, lift the tabletop forty-two centimeters. Its upper surface is now forty-five high.")
        self.select([top])
        self.command("tool.builtin.move")
        self.pointer("click", (0, 3, 0))
        self.sweep((0, 3, 0), (0, 45, 0))
        self.pointer("text", text="42")
        self.pointer("finish")
        self.pointer("cancel")
        self.capture()

        self.chapter("05  BUILD ONE LEG", "Draw a four-centimeter square below a corner. Pull it up forty-two centimeters, then make it an object.")
        self.pull((-27, -27), (-23, -23), 42)
        leg = self.make_object("Leg")
        self.capture()
        self.chapter("06  COPY THE LEGS", "Use Move Copy to make a second leg. Select both legs and copy them across to the far side.")
        self.select([leg])
        self.command("tool.move_copy")
        self.pointer("click", (-27, 0, -27))
        self.sweep((-27, 0, -27), (23, 0, -27))
        self.pointer("click", (23, 0, -27))
        self.pointer("cancel")
        legs = [o["target"] for o in self.scene()["objects"] if o["name"] == "Leg"]
        if len(legs) != 2:
            raise RuntimeError(f"Expected two legs, found {len(legs)}")
        self.select(legs)
        self.command("tool.move_copy")
        self.pointer("click", (-27, 0, -27))
        self.sweep((-27, 0, -27), (-27, 0, 23))
        self.pointer("click", (-27, 0, 23))
        self.pointer("cancel")
        self.select([])
        self.capture()

        scene = self.scene()
        legs = [o for o in scene["objects"] if o["name"] == "Leg"]
        if len(legs) != 4 or len({o["definition_id"] for o in legs}) != 1:
            raise RuntimeError("Four linked leg occurrences were not created")
        self.chapter("YOUR FIRST TABLE, COMPLETE", "The table is complete: one top and four linked legs. Save the project, then explore your own design.")
        for i in range(25):
            angle = math.radians(45 + i * 4)
            self.command("view.camera.set_pose", position=[165 * math.sin(angle), 90, 165 * math.cos(angle)],
                         target=[0, 22, 0], field_of_view=36)
            self.capture()
        self.command("file.save_as", path=str(self.directory / "side-table.octd"))
        (self.directory / "scene.json").write_text(json.dumps(scene, indent=2) + "\n")
        (self.directory / "chapters.json").write_text(json.dumps(dict(
            endpoint=self.endpoint, app_version=status["version"], capture="live MCP 3D viewport captures",
            chapters=self.chapters), indent=2) + "\n")
        print(f"Captured {self.frame_index} live frames and {len(self.chapters)} chapters", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="http://127.0.0.1:8766/mcp")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    LiveTable(args.endpoint, args.output).run()


if __name__ == "__main__":
    main()
