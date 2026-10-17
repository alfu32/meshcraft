# Repository Guidelines

## Project Structure & Module Organization

This repository currently contains the specification and implementation handoff for the Meshcraft Rust rewrite; product implementation has not started. `README.md` points to `docs/rewrite/README.md`, the project entry point. The rewrite documents live in `docs/rewrite/`: `spec/` holds behavior requirements, while `plan.md`, `tasks.json`, `acceptance.md`, `architecture.md`, and compatibility/evidence documents define delivery and review context. `scripts/` contains Python navigation and validation tools. Keep source inventory and fixture evidence aligned with their pinned legacy baseline.

## Build, Test, and Development Commands

There is no application build or runtime command yet. Use the existing planning tools:

- `python3 docs/rewrite/scripts/verify_plan.py` checks task references, dependencies, tool coverage, fixture evidence, and local Markdown links.
- `python3 docs/rewrite/scripts/handoff.py --ready` lists tasks whose dependencies are complete; `--list` shows all tasks.
- `python3 docs/rewrite/scripts/handoff.py T00` prints a task packet. Handoff source lookups require the pinned legacy checkout described in `docs/rewrite/source-inventory.json`.

## Coding, Documentation, and Data Conventions

Use Markdown headings and stable IDs already used by the docs: requirements such as `F01`, scenarios such as `C01`, and tasks such as `T00`. Keep task dependencies and acceptance links in `tasks.json` authoritative. Preserve JSON validity and the existing two-space indentation. Python scripts use the standard library and four-space indentation; keep utilities small and deterministic. Record observed behavior separately from proposed changes, and do not present static source inspection as live validation.

## Validation and Testing

Run `verify_plan.py` after changing requirements, tasks, acceptance suites, local links, or inventory evidence. The current acceptance suites describe planned product checks; they are not an executable application test suite. When implementation begins, add and run the task-specific checks defined by `plan.md` and record exact commands and results in the task handoff capsule.

## Commits and Pull Requests

The repository history uses Conventional Commit style (for example, `feat(planning): ...`). Keep commits focused and mention the affected task or document area. PRs should explain the behavior or planning change, link relevant requirement/task/scenario IDs, report validation commands and results, and include screenshots for visual changes. Call out unverified assumptions or blocked checks explicitly.
