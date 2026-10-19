# Rewrite guide

Prepared 2026-10-15. Source: `/home/alfu64/Development/k3d`, commit `acd84f4c67509e10363c33850501eb8f4fc3ffa7`, tag `5.7.2`. The source checkout was clean at inspection. `meshcraft` is the destination workspace name; final product name, file association, and app identifier remain product decisions.

## Read according to your job

| Job | Read |
| --- | --- |
| Review scope and tradeoffs | This file, `history.md`, `architecture.md`, `plan.md`, `object-model.md`, `plugins.md` |
| Implement one task | Run `python3 docs/rewrite/scripts/handoff.py Txx`; then read only its named source symbols |
| Check a behavior | Relevant `spec/*.md` requirement and `acceptance.md` scenario |
| Implement legacy IO | `compatibility.md`, inventory `dto_classes`, fixtures, T05/T09 |
| Find exact legacy code | `python3 docs/rewrite/scripts/handoff.py --find SYMBOL` |
| Inspect one tool economically | `python3 docs/rewrite/scripts/handoff.py --tool HELICAL_ARRAY` gives its owner, class, state fields, input keys and source-message hints |
| Coordinate agents | `plan.md`, `tasks.json`; task dependencies are authoritative |
| Review evidence or gaps | `evidence.md`, `source-inventory.json` |

## Scope and evidence policy

The rewrite preserves observable current functionality, with explicit intentional changes recorded here. Source implementation outranks documentation for current behavior. Documentation records intent and helps identify incomplete features. Git history explains dependencies and regression risks; commit subjects alone do not prove completion.

Requirement sections carry one of these labels:

- **Observed**: backed by inspected source. Static inspection does not imply live validation.
- **Intent**: documented aspiration without an established complete implementation; outside parity unless separately accepted.
- **New**: explicitly requested in this conversation: a conventional desktop menu interface, coexisting with the button interface.
- **Proposed change**: recommended Rust behavior where preserving an implementation detail would add cost or reproduce a defect. Review the decision register before depending on it.

Initial desktop scope includes drafting, selection/snapping, reusable objects, dynamic hotspots, voxel/architecture/HVAC/mechanical tools, advanced mesh operations, interchange, viewport and offline rendering, structured automation, tutorials, and packaging. A smaller usable desktop milestone comes first; it is not the completion criterion for this scope. Plugins are required in the final product, using WebKit JavaScriptCore and a versioned application context in a later delivery. Groovy/JVM plugin execution and the CLI/TUI developer console remain excluded from the first Rust delivery. A normal executable with file-opening arguments remains in scope.

Browser runtime and `<octodraw-editor>` compatibility follow desktop. Android has no initial application, packaging, SAF bridge, or store task. Core boundaries leave room for deferred capabilities without making them initial gates. Structured MCP command/pointer/status support remains proposed initial automation scope; executing arbitrary Groovy over MCP is deferred with the console.

The object model is now examined separately in [object-model.md](object-model.md): eliminate redundant ownership/copies before freezing core contracts. A measured census found 2,899 of 2,939 serialized geometry override bundles equal to their definitions across the 60 fixtures. These are normalization candidates, not permission to discard authored instance differences. T46 gates the core model; T47–T50 deliver JavaScriptCore plugins after initial desktop qualification.

The implementation has since grown substantially: the Rust core contains the normalized document, transactions, selection, tool state machines, shared command catalog, IO and many geometry/array operations. The current desktop executable is an interim Bevy/egui stack and is not the selected product architecture. The accepted direction is GPUI + raylib-rs; see [ADR-06](../decisions/ADR-06-gpui-raylib.md) and [UI-3D_renderer.md](../UI-3D_renderer.md). T59 is the first gate: qualify raylib offscreen rendering presented as a normal GPUI image, preserving UI stacking and meeting the 25 FPS minimum / 30–60 FPS target. T60 removes the old framework-specific desktop code after replacement. Preserve commands and model behavior while separating the current desktop session from GPUI and raylib. The supplied, locally ignored recordings inform C41's dense-array interaction regression.

## What was found

- 169 tracked application/runtime source files, 73,751 lines in the census scope; 559 reachable commits.
- 75 `ToolId` declarations: 74 built-in tools and one plugin adapter ID. Hotspot placement is a separate action/mode, not an additional `ToolId`. The complete declaration list is indexed.
- 32 literal palette registrations, 6 orthographic view IDs, generated built-in tool commands, and 26 show/hide commands for 13 toolbars, including In-tool Operators. Availability depends on runtime capabilities; counts are not a promise of a fixed live catalog.
- 32 persistence DTO classes, model schema constant 17, JSON and gzip, legacy flat/group loaders, persisted undo, xrefs, prototype/instance overrides.
- 60 tracked model/glyph fixtures outside generated `web/build`. Every sampled/censused top-level model omits `version`; default omission is a real compatibility concern. An absent field does not identify a historical schema version.
- Local HTTP automation with six MCP tool declarations and five resources; desktop console/plugins are JVM/Groovy based.
- Existing integration scripts and docs-site validation, but no tracked application unit-test suite was found in `core/src`, `lwjgl3/src`, `android/src`, or `web/src`.

The inventory is navigation data, not a substitute for behavior specifications. In particular, an enum member, plugin interface hook, or parser branch can exist without complete semantics.

## Deliverables

| File | Purpose |
| --- | --- |
| `evidence.md` | Baseline, source map, divergences, confidence limits |
| `live-reference.md` | Supplemental screenshots and observed behavior from the running legacy UI/MCP build |
| `spec/foundation.md` | Document, persistence, input, selection, commands |
| `spec/modeling.md` | Every built-in tool family and domain system |
| `spec/interface.md` | Both desktop interfaces, panels, rendering, external surfaces |
| `compatibility.md` | Sparse JSON, transforms, history, formats, APIs, migration |
| `history.md` | Historical layers and deliberate sequencing changes |
| `architecture.md` | Rust boundaries, contracts, numerical and compatibility decisions |
| `../decisions/ADR-06-gpui-raylib.md` | Accepted GPUI shell/raylib viewport architecture and integration gates |
| `../UI-3D_renderer.md` | Desktop layout, renderer boundary, interaction and performance requirements |
| `object-model.md` | Source/fixture-backed redundancy analysis, normalized ownership proposal and migration gate |
| `object-model-evidence.json` | Reproducible counts for the 60 pinned top-level fixture models |
| `plugins.md` | Required later JavaScriptCore runtime, application context and SDK/lifecycle contracts |
| `plan.md` | Milestones, parallel lanes, gates, handoff protocol |
| `tasks.json` | Detailed task cards: inputs, outputs, steps, dependencies, checks |
| `acceptance.md` | Named scenario suites and release criteria |
| `source-inventory.json` | Reproducible source/command/schema/symbol/fixture/history index |
| `scripts/handoff.py` | Small task-specific context packets and symbol lookup |
| `scripts/verify_plan.py` | Cross-reference, dependency, catalog, link, and scope validation |

## Current handoff

T59 is the high-priority stack proof. Read `../decisions/ADR-06-gpui-raylib.md`, `../UI-3D_renderer.md`, and `handoffs/T59.md`; run `python3 docs/rewrite/scripts/handoff.py T59` for the implementation packet. T58 and issue #20 are superseded. Keep `meshcraft-core` and `meshcraft-io` headless, and extract the current desktop session/command handling before removing old UI and renderer modules. Use `python3 docs/rewrite/scripts/handoff.py --ready` to identify dependency-ready work.

The initial source audit did not launch the legacy application or run its source-repository build. A later live UI/MCP inspection is documented in `live-reference.md`; its screenshots are visual evidence, not a geometry parity oracle. The user-supplied `.octd` scene was read and imported without modifying or saving it.
