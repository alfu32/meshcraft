#!/usr/bin/env python3
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parents[1]


def main():
    registry = json.loads((ROOT / "tasks.json").read_text())
    inventory = json.loads((ROOT / "source-inventory.json").read_text())
    errors = []
    tasks = {task["id"]: task for task in registry["tasks"]}
    if len(tasks) != len(registry["tasks"]):
        errors.append("Duplicate task IDs")
    if registry["baseline_commit"] != inventory["head"]:
        errors.append("Task and source baselines differ")
    requirements = {}
    for path in (ROOT / "spec").glob("*.md"):
        for identifier in re.findall(r"^### ([A-Z][0-9]{2})\b", path.read_text(), re.MULTILINE):
            if identifier in requirements:
                errors.append("Duplicate requirement " + identifier)
            requirements[identifier] = path
    scenarios = set(re.findall(r"^## (C[0-9]{2})\b", (ROOT / "acceptance.md").read_text(), re.MULTILINE))
    covered_requirements = set()
    covered_scenarios = set()
    covered_tools = []
    for task in tasks.values():
        for field in ("title", "phase", "requirements", "acceptance", "targets", "source_paths", "slices", "done", "status"):
            if not task.get(field):
                errors.append(task["id"] + " lacks " + field)
        for dependency in task["dependencies"]:
            if dependency not in tasks or dependency == task["id"]:
                errors.append(task["id"] + " invalid dependency " + dependency)
        for requirement in task["requirements"]:
            if requirement not in requirements:
                errors.append(task["id"] + " unknown requirement " + requirement)
            covered_requirements.add(requirement)
        for scenario in task["acceptance"]:
            if scenario not in scenarios:
                errors.append(task["id"] + " unknown scenario " + scenario)
            covered_scenarios.add(scenario)
        for path in task["source_paths"]:
            if not (Path(inventory["source"]) / path).is_file():
                errors.append(task["id"] + " missing source " + path)
        for path in task.get("design_docs", []):
            if not (WORKSPACE / path).is_file():
                errors.append(task["id"] + " missing design document " + path)
        if task["phase"] not in ("desktop", "browser", "plugins"):
            errors.append(task["id"] + " unknown phase " + task["phase"])
        if task["status"] not in ("planned", "in_progress", "complete", "blocked", "superseded"):
            errors.append(task["id"] + " unknown status " + task["status"])
        if task["phase"] == "desktop" and any(tasks.get(dependency, {}).get("phase") in ("browser", "plugins") for dependency in task["dependencies"]):
            errors.append(task["id"] + " initial desktop task depends on later delivery")
        if task["phase"] in ("browser", "plugins") and any(tasks.get(dependency, {}).get("phase") in ({"browser", "plugins"} - {task["phase"]}) for dependency in task["dependencies"]):
            errors.append(task["id"] + " independent browser/plugin delivery depends on the other")
        covered_tools.extend(task["tool_ids"])
    for identifier in ("T02", "T03", "T05", "T16"):
        if "T46" not in tasks.get(identifier, {}).get("dependencies", []):
            errors.append(identifier + " bypasses object-model gate")
    if "JavaScriptCore" not in registry["scope"].get("plugin_runtime", ""):
        errors.append("Required JavaScriptCore runtime direction missing")
    for identifier in ("T47", "T48", "T49", "T50"):
        if tasks.get(identifier, {}).get("phase") != "plugins":
            errors.append(identifier + " required later plugin task missing")
    for requirement in set(requirements) - covered_requirements:
        errors.append("Unowned requirement " + requirement)
    for scenario in scenarios - covered_scenarios:
        errors.append("Unused scenario " + scenario)
    expected_tools = {tool["id"] for tool in inventory["tools"] if tool["id"] != "PLUGIN"}
    if set(covered_tools) != expected_tools:
        errors.append("Tool coverage mismatch: " + str(set(covered_tools) ^ expected_tools))
    if len(covered_tools) != len(set(covered_tools)):
        errors.append("Tool has multiple primary task owners")
    specification = "\n".join(path.read_text() for path in (ROOT / "spec").glob("*.md"))
    for tool in expected_tools:
        if "`" + tool + "`" not in specification:
            errors.append("Tool missing from written specification " + tool)
    visiting = set()
    visited = set()

    def visit(identifier):
        if identifier in visiting:
            errors.append("Dependency cycle at " + identifier)
            return
        if identifier in visited or identifier not in tasks:
            return
        visiting.add(identifier)
        for dependency in tasks[identifier]["dependencies"]:
            visit(dependency)
        visiting.remove(identifier)
        visited.add(identifier)

    for identifier in tasks:
        visit(identifier)
    model_evidence = json.loads((ROOT / "object-model-evidence.json").read_text())
    if model_evidence["baseline_commit"] != inventory["head"]:
        errors.append("Object-model evidence baseline differs")
    expected_fixtures = {fixture["path"]: fixture["sha256"] for fixture in inventory["fixtures"]}
    actual_fixtures = {fixture["path"]: fixture["sha256"] for fixture in model_evidence["fixtures"]}
    if expected_fixtures != actual_fixtures or model_evidence["fixture_count"] != len(expected_fixtures):
        errors.append("Object-model census fixture manifest differs")
    for key, total in model_evidence["totals"].items():
        if sum(fixture["counts"].get(key, 0) for fixture in model_evidence["fixtures"]) != total:
            errors.append("Object-model census total mismatch: " + key)
    history_prefixes = {entry["commit"][:7] for entry in inventory["history"]}
    for path in [WORKSPACE / "README.md", *ROOT.rglob("*.md")]:
        content = path.read_text()
        for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", content):
            if target.startswith(("https://", "http://", "#")):
                continue
            relative = target.split("#", 1)[0]
            if relative and not (path.parent / relative).exists():
                errors.append(str(path.relative_to(WORKSPACE)) + " broken link " + target)
        if path.name == "history.md":
            for commit in re.findall(r"`([0-9a-f]{7})`", content):
                if commit not in history_prefixes:
                    errors.append("Unknown historical commit " + commit)
    if errors:
        raise SystemExit("\n".join(errors))
    print(f'Valid: {len(tasks)} tasks, {sum(len(task["slices"]) for task in tasks.values())} slices, {len(requirements)} requirements, {len(scenarios)} scenario suites, {len(expected_tools)} built-in tools; acyclic dependencies, source paths and local links checked.')


if __name__ == "__main__":
    main()
