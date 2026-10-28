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
Tags are important in this process so each time you commit dont forget to tag according to semver rules and increase the harcoded app version in the app toml (versionise before committing). Major version incrementation is my responsibility.
When the user requests a completed phase to be published, commit, create its matching annotated version tag, and push both the branch and tag. Do not stop after a local commit/tag.

## Tool inventory, performance, and menu review

Keep `docs/tools/tools.csv` aligned with the live command registry using `scripts/export_tool_inventory.py`. Preserve user review columns by command ID; menu restructuring consumes those assignments only after review. README screenshots use descriptive stable paths in `docs/screenshots/manifest.json` and must be refreshed after the menu/ribbon redesign.

The performance suite requires every tool to be exercised on small, medium, high and heavy model tiers, locally and by a dedicated manual `stress-test.yaml` workflow. That workflow must consume the release binary at the selected existing tag and fail if it is absent. Publish measured results and coverage as release assets, including Markdown for a stable latest-release README link. Do not claim generic command-activation timing covers an actual tool operation. Use `scripts/stress/run.py` and `.github/workflows/stress-test.yaml`; preserve complete coverage and publication guards when adding commands. Record unsupported operations separately from failures. Do not shrink fixtures, raise app limits or relabel failed registered tools to make reports green. The manual stress workflow runs only existing same-tag release binaries; current app failures stay visible in its published reports. Measurement outcomes are non-gating: a completed stress run succeeds irrespective of performance or tool failures. Always publish detailed canonical Markdown/CSV/JSON reports and a diagnostic ZIP to the same release. Incomplete or rejected worker data must be clearly labeled diagnostic/unvalidated rather than presented as complete measurements. Missing binaries and infrastructure/publication errors remain failures.

## Manual desktop releases

Use `.github/workflows/distribution.yaml` with `workflow_dispatch` at an existing version tag; do not add a redundant tag/source input or create/move tags in the workflow. The tag must match the hardcoded desktop app version. Preserve the six-target native matrix and portable/AppImage/Snap/DEB/RPM/MSI/MSIX/DMG artifacts documented in `packaging/README.md`. Linux compilation and native dependency collection use the old-glibc container, with an ELF version ceiling; a newer hosted runner is only the orchestrator. DEB/RPM packages wrap that audited payload for amd64/ARM64 and must pass installed-payload verification plus native installation/CLI/removal checks. Repository hosting/signing is separate from publishing package downloads. Snap packages use `grade: stable` so revisions are eligible for every risk channel; the chosen Store release channel remains a separate explicit publication action. MSIX uses the reserved identity in `packaging/windows/store-identity.json` and requires `MESHCRAFT_PFX_B64` / `MESHCRAFT_PFX_PASSWORD`. Preserve positive-major Store version mapping, public CER downloads, publisher/signature/payload validation, and native Windows installation/removal checks. Private signing material stays in ignored local storage and repository secrets. Packaging helpers use the standard library and have checks via `python3 -m unittest discover -s scripts/release -p 'test_*.py' -v` (install `rpm`, `dpkg` and `openssl` to run native package checks).

## User requests check

you will periodically check for issues opened on the github issues list using gh, bugs will be treated with priority. features will be either integrated into the existing plan or you will integrate another feature into the features list. rules types of issues apply as a general rule and should be integrated in agents.md and/or the specification.

bugs will be solved with a commit that includes the # issue number ( github will construct a link between the issue and the commit message ).
features will be integrated containing the issue number in the title of the local description so that upon resolution and commit(s) it will relate to the issue in question.

bugs and features will be closed on resolution
rules will be closed upon integration as an acknowledgement.

issues labeled as high-priority will be solved with high priority
