#!/usr/bin/env python3
import argparse
import gzip
import hashlib
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path


def git(source, *arguments):
    return subprocess.check_output(["git", "-C", str(source), *arguments], text=True).strip()


def location(path, line):
    return {"path": path, "line": line}


def describe_tool(source, relative, class_name):
    lines = (source / relative).read_text().splitlines()
    declaration = re.compile(r"^(?:(?:open|abstract|sealed|data|enum) )?class (\w+)")
    start = next(index for index, line in enumerate(lines) if (match := declaration.match(line)) and match.group(1) == class_name)
    end = next((index for index in range(start + 1, len(lines)) if declaration.match(lines[index])), len(lines))
    result = {"class": class_name, **location(relative, start + 1), "state_fields": [], "messages": [], "keys": [], "numeric_sites": []}
    seen_messages = set()
    seen_keys = set()
    for index in range(start, end):
        line = lines[index]
        state = re.match(r"    private (?:var|val) (\w+)", line)
        if state:
            result["state_fields"].append({"name": state.group(1), "line": index + 1})
        message = re.search(r'(?:status\.message|override val message[^=]*)\s*=\s*"([^"\n]*)"', line)
        if message and message.group(1) not in seen_messages:
            result["messages"].append({"text": message.group(1), "line": index + 1})
            seen_messages.add(message.group(1))
        for key in re.findall(r"Input\.Keys\.(\w+)", line):
            if key not in seen_keys:
                result["keys"].append({"key": key, "line": index + 1})
                seen_keys.add(key)
        if re.search(r"to(?:Float|Double|Int)OrNull|parseNumeric|parseInput", line):
            result["numeric_sites"].append(index + 1)
    return result


def collect(source):
    tracked = git(source, "ls-files").splitlines()
    files = {}
    symbols = []
    implementations = {}
    kotlin_root = "core/src/main/kotlin/com/github/alfu32/sketch/"
    for relative in tracked:
        if not (relative.startswith(("core/src/", "lwjgl3/src/", "android/src/", "web/src/", "web/component/")) and relative.endswith((".kt", ".java", ".js"))):
            continue
        path = source / relative
        if not path.is_file():
            continue
        content = path.read_text()
        lines = content.splitlines()
        files[relative] = {"lines": len(lines), "sha256": hashlib.sha256(content.encode()).hexdigest()}
        for number, line in enumerate(lines, 1):
            declaration = re.search(r"\b(?:fun|class|interface|enum class|data class)\s+([A-Za-z_][A-Za-z_0-9]*)", line)
            if declaration and not line.lstrip().startswith(("//", "*")):
                symbols.append({**location(relative, number), "symbol": declaration.group(1), "declaration": line.strip()})
            tool = re.search(r"override val id:\s*ToolId\s*=\s*ToolId\.([A-Z_0-9]+)", line)
            if tool:
                implementations.setdefault(tool.group(1), []).append(location(relative, number))
    tools_path = kotlin_root + "ui/ToolId.kt"
    tools = []
    for number, line in enumerate((source / tools_path).read_text().splitlines(), 1):
        match = re.match(r'\s+([A-Z_0-9]+)\("([^"]+)"\)', line)
        if match:
            identifier, label = match.groups()
            tools.append({"id": identifier, "label": label, "command": None if identifier == "PLUGIN" else "tool.builtin." + identifier.lower(), "declaration": location(tools_path, number), "implementations": implementations.get(identifier, [])})
    parameterized_tools = {
        "AXIAL_GRID": ("tools/GuidePlacementTool.kt", "GuidePlacementTool"),
        "PLANAR_GRID": ("tools/GuidePlacementTool.kt", "GuidePlacementTool"),
        "ARCH_DOOR_FRAME": ("tools/ArchitectureTools.kt", "ArchitectureDoorFrameTool"),
        "SOLID_UNION": ("tools/VolumeBooleanTool.kt", "VolumeBooleanTool"),
        "SOLID_INTERSECTION": ("tools/VolumeBooleanTool.kt", "VolumeBooleanTool"),
        "SOLID_SUBTRACTION": ("tools/VolumeBooleanTool.kt", "VolumeBooleanTool"),
        "MESH_REGULARIZE": ("tools/RandomTools.kt", "MeshRegularizeTool"),
        "SURFACE_REMESH": ("tools/RandomTools.kt", "MeshRegularizeTool"),
    }
    for tool in tools:
        if tool["id"] in parameterized_tools:
            suffix, class_name = parameterized_tools[tool["id"]]
            relative = kotlin_root + suffix
        else:
            implementation = tool["implementations"][0]
            relative = implementation["path"]
            prior_classes = [match.group(1) for line in (source / relative).read_text().splitlines()[:implementation["line"]] if (match := re.match(r"^(?:(?:open|abstract) )?class (\w+)", line))]
            class_name = prior_classes[-1]
        tool["source_hints"] = describe_tool(source, relative, class_name)
    main_path = kotlin_root + "Main.kt"
    main_lines = (source / main_path).read_text().splitlines()
    commands = []
    for number, line in enumerate(main_lines, 1):
        match = re.search(r'id = "((?:view|edit|export|import)\.[^"$]+)"', line)
        if match and any("PaletteCommand(" in previous for previous in main_lines[max(0, number - 5):number]):
            commands.append({"id": match.group(1), **location(main_path, number)})
    orthographic = []
    for number, line in enumerate(main_lines, 1):
        match = re.search(r'"(view\.ortho\.[a-z]+)"', line)
        if match:
            orthographic.append({"id": match.group(1), **location(main_path, number)})
    overlay_path = kotlin_root + "ui/SketchUiOverlay.kt"
    toolbars = []
    for number, line in enumerate((source / overlay_path).read_text().splitlines(), 1):
        match = re.search(r'"(builtin_toolbar_[^"]+)" to "([^"]+)"', line)
        if match:
            identifier, label = match.groups()
            stem = identifier.removeprefix("builtin_toolbar_")
            toolbars.append({"id": identifier, "label": label, "commands": ["view.toolbar." + stem + "." + verb for verb in ("show", "hide")], **location(overlay_path, number)})
    persistence_path = kotlin_root + "model/ModelPersistence.kt"
    dto_classes = []
    current = None
    for number, line in enumerate((source / persistence_path).read_text().splitlines(), 1):
        declaration = re.match(r"    class (\w+)", line)
        if declaration:
            current = None
            if declaration.group(1).endswith("Dto") or declaration.group(1) in ("ModelSnapshot", "UndoHistoryDto", "UndoEntryDto"):
                current = {"name": declaration.group(1), **location(persistence_path, number), "fields": []}
                dto_classes.append(current)
        field = re.match(r"        var (\w+):\s*(.+?)\s*=\s*(.*)", line)
        if current is not None and field:
            current["fields"].append({"name": field.group(1), "type": field.group(2), "default": field.group(3), "line": number})
    mcp_path = kotlin_root + "mcp/McpHttpServer.kt"
    mcp = {"routes": [], "tools": [], "resources": []}
    for number, line in enumerate((source / mcp_path).read_text().splitlines(), 1):
        for section, pattern in (("routes", r'createContext\("([^"]+)"'), ("tools", r'name = "(k3d_[^"]+)"'), ("resources", r'uri = "(k3d://[^"]+)"')):
            match = re.search(pattern, line)
            if match:
                mcp[section].append({"id": match.group(1), **location(mcp_path, number)})
    fixtures = []
    for relative in tracked:
        if not relative.endswith((".octd", ".k3d", ".octo")) or relative.startswith("web/build/"):
            continue
        path = source / relative
        if not path.is_file():
            continue
        payload = path.read_bytes()
        compressed = payload.startswith(b"\x1f\x8b")
        entry = {"path": relative, "bytes": len(payload), "gzip": compressed, "sha256": hashlib.sha256(payload).hexdigest()}
        try:
            model = json.loads(gzip.decompress(payload) if compressed else payload)
            entry.update({"version": model.get("version"), "version_field_present": "version" in model, "root_keys": sorted(model), "prototypes": len(model.get("prototypes", []))})
        except (ValueError, OSError, UnicodeError) as error:
            entry["decode_error"] = type(error).__name__
        fixtures.append(entry)
    history = []
    for line in git(source, "log", "--reverse", "--format=%H%x09%cs%x09%s").splitlines():
        commit, date, subject = line.split("\t", 2)
        history.append({"commit": commit, "date": date, "subject": subject})
    return {
        "source": str(source),
        "head": git(source, "rev-parse", "HEAD"),
        "describe": git(source, "describe", "--tags", "--always"),
        "status": git(source, "status", "--porcelain"),
        "method": "Static declaration census; does not prove runtime registration, reachability, parser coverage, or behavior.",
        "files": files,
        "tools": tools,
        "commands": commands,
        "orthographic_commands": orthographic,
        "toolbars": toolbars,
        "dto_classes": dto_classes,
        "mcp": mcp,
        "fixtures": fixtures,
        "history": history,
        "symbols": symbols,
    }


def main():
    parser = argparse.ArgumentParser(description="Index the legacy checkout without modifying it.")
    parser.add_argument("--source", type=Path, default=Path("/home/alfu64/Development/k3d"))
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    inventory = collect(arguments.source.resolve())
    output = json.dumps(inventory, indent=2, ensure_ascii=False) + "\n"
    if arguments.output:
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(output)
    else:
        print(output, end="")
    print(json.dumps({"head": inventory["head"], "describe": inventory["describe"], "source_files": len(inventory["files"]), "source_lines": sum(entry["lines"] for entry in inventory["files"].values()), "tools": len(inventory["tools"]), "static_commands": len(inventory["commands"]), "toolbars": len(inventory["toolbars"]), "dto_classes": len(inventory["dto_classes"]), "commits": len(inventory["history"]), "fixture_versions": dict(Counter(str(entry.get("version", "unparsed")) for entry in inventory["fixtures"]))}), file=sys.stderr)


if __name__ == "__main__":
    main()
