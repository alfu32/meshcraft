#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read_json(name):
    return json.loads((ROOT / name).read_text())


def sections(path, pattern):
    content = path.read_text()
    headings = list(re.finditer(pattern, content, re.MULTILINE))
    return {heading.group(1): content[heading.start():headings[index + 1].start() if index + 1 < len(headings) else len(content)].strip() for index, heading in enumerate(headings)}


def source_state(inventory, allow_drift):
    source = Path(inventory["source"])
    if not source.is_dir():
        raise SystemExit("Legacy checkout missing: " + str(source))
    head = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(source), "status", "--porcelain"], text=True).strip()
    if head != inventory["head"] or dirty:
        message = "Source drift detected; pinned " + inventory["head"] + ", current " + head + ("; working tree dirty" if dirty else "")
        if not allow_drift:
            raise SystemExit(message + ". Use the pinned clean checkout or --allow-source-drift for explicit investigation.")
        print("SOURCE WARNING: " + message)


def main():
    parser = argparse.ArgumentParser(description="Print a bounded task packet; never edits source or task state.")
    parser.add_argument("task", nargs="?")
    parser.add_argument("--slice", type=int)
    parser.add_argument("--find")
    parser.add_argument("--path", default="")
    parser.add_argument("--dto")
    parser.add_argument("--tool")
    parser.add_argument("--catalog", choices=("tools", "commands", "mcp", "fixtures"))
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--ready", action="store_true")
    parser.add_argument("--allow-source-drift", action="store_true")
    arguments = parser.parse_args()
    registry = read_json("tasks.json")
    inventory = read_json("source-inventory.json")
    tasks = {task["id"]: task for task in registry["tasks"]}
    if arguments.list or arguments.ready:
        for task in tasks.values():
            ready = all(tasks[dependency]["status"] == "complete" for dependency in task["dependencies"])
            if not arguments.ready or (ready and task["status"] != "complete"):
                print(f'{task["id"]} [{task["status"]}; {task["phase"]}] {task["title"]}; needs: {", ".join(task["dependencies"]) or "none"}; slices: {len(task["slices"])}')
        return
    source_state(inventory, arguments.allow_source_drift)
    if arguments.find:
        matches = [symbol for symbol in inventory["symbols"] if arguments.find.casefold() in symbol["symbol"].casefold() and arguments.path in symbol["path"]]
        for symbol in matches[:20]:
            print(f'{inventory["source"]}/{symbol["path"]}:{symbol["line"]} {symbol["declaration"]}')
        print(f"{len(matches)} matches; at most 20 shown. Narrow with --path.")
        return
    if arguments.dto:
        matches = [dto for dto in inventory["dto_classes"] if dto["name"] == arguments.dto]
        if not matches:
            raise SystemExit("Unknown DTO: " + arguments.dto)
        print(json.dumps(matches[0], indent=2))
        return
    if arguments.tool:
        matches = [tool for tool in inventory["tools"] if tool["id"] == arguments.tool.upper()]
        if not matches:
            raise SystemExit("Unknown ToolId: " + arguments.tool)
        tool = matches[0]
        owner = next((task["id"] for task in tasks.values() if tool["id"] in task["tool_ids"]), "later plugin delivery T48/T49")
        print(f'{tool["id"]}: {tool["label"]}; command={tool["command"]}; owner={owner}')
        hints = tool["source_hints"]
        print(f'{inventory["source"]}/{hints["path"]}:{hints["line"]} class {hints["class"]}')
        print("Static source hints only: messages are in source order, not a proven execution/state sequence; inherited helpers may live in another class.")
        print("State fields: " + ", ".join(f'{field["name"]}@{field["line"]}' for field in hints["state_fields"]))
        print("Keys: " + ", ".join(f'{key["key"]}@{key["line"]}' for key in hints["keys"]))
        print("Numeric parsing lines: " + ", ".join(str(line) for line in hints["numeric_sites"]))
        for message in hints["messages"][:24]:
            print(f'{message["line"]}: {message["text"]}')
        if len(hints["messages"]) > 24:
            print("More messages indexed; inspect the named handlers for the active state.")
        return
    if arguments.catalog:
        if arguments.catalog == "tools":
            owners = {tool: task["id"] for task in tasks.values() for tool in task["tool_ids"]}
            for tool in inventory["tools"]:
                print(f'{tool["id"]}: {tool["label"]}; owner={owners.get(tool["id"], "later plugin delivery T48/T49")}; command={tool["command"]}')
        elif arguments.catalog == "commands":
            for command in inventory["commands"] + inventory["orthographic_commands"]:
                print(f'{command["id"]} @ {command["path"]}:{command["line"]}')
            for toolbar in inventory["toolbars"]:
                print(" / ".join(toolbar["commands"]))
            print("Generated: tool.builtin.<lowercase ToolId>, excluding PLUGIN; runtime availability is capability-dependent.")
        elif arguments.catalog == "mcp":
            print(json.dumps(inventory["mcp"], indent=2))
        else:
            for fixture in inventory["fixtures"]:
                print(f'{fixture["path"]}: bytes={fixture["bytes"]}, gzip={fixture["gzip"]}, version_field={fixture.get("version_field_present", False)}, hash={fixture["sha256"]}')
        return
    if arguments.task not in tasks:
        raise SystemExit("Supply T00..T50, --list, --ready, --find SYMBOL, --dto NAME, --tool ID, or --catalog KIND.")
    task = tasks[arguments.task]
    if arguments.slice is not None and not 1 <= arguments.slice <= len(task["slices"]):
        raise SystemExit("Slice must be between 1 and " + str(len(task["slices"])))
    print(f'# {task["id"]}: {task["title"]}')
    print("Baseline: " + inventory["head"] + "; source: " + inventory["source"])
    print("Scope: desktop first; browser and required JavaScriptCore plugins later; Android, Groovy/JVM execution and developer CLI/TUI deferred.")
    print("Shared contracts: T46 normalization gate before core contracts harden; shared definitions/content/layouts, explicit authored occurrence edits, derived caches and session selection separated; f64 local geometry, raw-load versus edit paths, transactions and one command registry; later plugins get versioned value/ID ctx, not mutable stores.")
    print("Dependencies: " + (", ".join(task["dependencies"]) or "none"))
    for dependency in task["dependencies"]:
        print(f'- {dependency}: {tasks[dependency]["status"]}; read its completion capsule and exported contracts before editing.')
    print("Targets: " + ", ".join(task["targets"]))
    if task.get("design_docs"):
        print("Required design read set: " + ", ".join(task["design_docs"]))
    if task["id"] == "T46":
        print("Gate scope: schema/data probes and migration comparisons now; full C32 editor/resource assertions mature in downstream core/object/runtime tasks.")
    print("\nWork slices:")
    for index, instruction in enumerate(task["slices"], 1):
        if arguments.slice is None or arguments.slice == index:
            print(f"{index}. {instruction}")
    print("\nDone:")
    for criterion in task["done"]:
        print("- " + criterion)
    requirements = {}
    for path in sorted((ROOT / "spec").glob("*.md")):
        requirements.update(sections(path, r"^### ([A-Z][0-9]{2})\b.*$"))
    print("\nRequired specification:")
    for requirement in task["requirements"]:
        print("\n" + requirements[requirement])
    scenarios = sections(ROOT / "acceptance.md", r"^## (C[0-9]{2})\b.*$")
    print("\nAcceptance scenarios:")
    for scenario in task["acceptance"]:
        print("\n" + scenarios[scenario])
    print("\nLegacy read set (read named symbols and callers in bounded excerpts):")
    for source_path in task["source_paths"]:
        print("- " + inventory["source"] + "/" + source_path)
        matches = [symbol for symbol in inventory["symbols"] if symbol["path"] == source_path and symbol["symbol"] in task["source_symbols"]]
        for symbol in matches[:8]:
            print(f'  {symbol["line"]}: {symbol["declaration"]}')
    print("\nRecord unresolved behavior in the task capsule; do not invent an API. Add menu/command and applicable scenario coverage for each shipped action. Preserve unrelated source/workspace changes.")
    print(f'Finish with docs/rewrite/handoffs/{task["id"]}.md: completed slices, exported APIs, changed paths, checks/results, divergences and remaining work (target <=250 words). Mark complete only when all slices and acceptance criteria pass.')


if __name__ == "__main__":
    main()
