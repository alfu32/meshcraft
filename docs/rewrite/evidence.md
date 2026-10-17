# Evidence and source map

## Baseline and method

Legacy root `/home/alfu64/Development/k3d`; HEAD `acd84f4c67509e10363c33850501eb8f4fc3ffa7`, tag `5.7.2`, dated 2026-09-20. Audit date 2026-10-15. Reachable history: 559 commits, beginning 2026-01-14. Destination had no application files when inspected.

Inspected current implementation entry points, tool registration and representative tool state machines, model stores/DTOs, input/snapping, scene structure, render controller, import/export dispatch, console/plugins, MCP transport, web bridge/component, tutorial schemas, manuals/specs/roadmap, release/build definitions, and history. Large modules were inspected by symbol and bounded excerpts. This is a feature and architecture audit, not a line-by-line correctness proof of 73,751 lines.

`source-inventory.json` was generated from tracked working-tree files and records the source HEAD, status, content hashes, declarations, symbol locations, model fixture hashes, and commit metadata. Source was clean. Generated `web/build` copies, marketing images, and vendored/generated output are not separate features.

Reproduce the census from the destination root:

```sh
python3 docs/rewrite/scripts/source_inventory.py --source /home/alfu64/Development/k3d --output /tmp/k3d-inventory.json
python3 docs/rewrite/scripts/verify_plan.py
```

Updating the checked-in inventory to a newer legacy HEAD requires a reviewed specification delta. Handoff scripts reject a changed HEAD or dirty source checkout by default.

## Primary code locations

All paths below are relative to the legacy root. `K` means `core/src/main/kotlin/com/github/alfu32/sketch/`. Use inventory lookup for exact current declaration lines; the inventory pins the commit and content hashes.

| Area | Source / entry points | Why it matters |
| --- | --- | --- |
| Application wiring | `K/Main.kt`: `create`, `registerBuiltInToolCommands`, `registerToolbarVisibilityCommands`, `onModelChanged` | Actual registration, platform gates, callbacks, settings, saves |
| Tool census | `K/ui/ToolId.kt:3`; `K/Main.kt:813` tool construction | Enum declarations plus actual runtime wiring |
| Tool protocol | `K/ui/Tool.kt`, `ToolController.kt`, `ToolInputProcessor.kt` | Input, cancel, copy, numeric fields, in-tool buttons |
| Geometry | `K/model/DraftLineStore.kt`, `DraftFaceStore.kt`, `ModelCleanup.kt` | Independent segments and colored triangles; JTS cuts/cleanup |
| Scene | `K/model/GroupScene.kt:28` | Definitions, instances, transforms, overrides, domains, hotspot evaluation |
| Persistence | `K/model/ModelPersistence.kt:16`, `ModelSnapshot:849` | Actual schema 17, defaults, raw load, staged application |
| Undo | `K/model/UndoRedoManager.kt:3`; `K/Main.kt:7758` | Debounced encoded snapshots; application cap 20 |
| Selection | `K/tools/SelectTool.kt` | Multi-click, 2D/volume selection, domain handles, filters |
| Snapping | `K/input/Snapper.kt`, `SnapType.kt`, `GuideManager.kt` | Screen-space radius, transformed groups, grid/guide inference |
| Domain entities | `K/model/{Voxel,Architecture,Hvac,Hotspot}Store.kt` | Editable parameters distinct from generated triangles |
| Rendering | `K/render/*`, `K/Main.kt` mesh/shadow/capture paths, `assets/shaders/*` | Real-time versus CPU offline rendering |
| Interchange | `K/export/MeshIo.kt:24`, `IfcExporter.kt` | Actual format subsets, units, validation |
| UI | `K/ui/SketchUiOverlay.kt`, `CommandPaletteUI.kt` | Twelve floating toolbars, panels, prefs, touch operators |
| Palette | `K/plugin/CommandPalette.kt`; `K/Main.kt:1175` | Static/generated commands and metadata |
| Automation | `K/mcp/McpHttpServer.kt:51`, `McpModels.kt`; MCP methods in `Main.kt` | Routes, six tools, five resources, dispatch/thread rules |
| Scripting | `K/console/*`, `K/tui/*` | Bindings, REPL metacommands, application-thread bridge |
| Plugins | `K/plugin/*`, `assets/plugins/*`, `scripts/*.groovy` | Lifecycle, catalog, registration capabilities versus implemented loading |
| Tutorials | `K/tutorial/TutorialManager.kt:11`, `assets/tutorials`, `docs/tutorial` | Action-matching recorder/player and embedded previews |
| Desktop | `lwjgl3/src/main/kotlin/com/github/alfu32/sketch/lwjgl3/Lwjgl3Launcher.kt` | CLI, file arguments, console and update behavior |
| Browser | `web/component/octodraw-element.js`, `web/src/main/java/...`, `K/WebRuntimeBridge.kt` | Promise/event API, storage, dialogs, singleton runtime |
| Existing Android | `android/src/main/...`, `K/AndroidSafBridge.kt` | Evidence of legacy scope only; deferred from Rust delivery |
| Docs site | `docs-site/src/routes`, `src/lib`, `scripts`, `static/tutorials` | Documentation intent, embed tests, separate tutorial schema |
| Release | `.github/workflows/release.yml`, `build.gradle`, `lwjgl3/build.gradle`, `snapcraft.yaml` | Platform packages, release channels, version generation |

## Documentation priority

Use current docs-site references/tutorials and README for workflow intent; use `docs/SHORTCUTS.md` with actual key handlers for shortcut details. `docs/chapters` is broader but mixes February dates with later edits. `docs/specification/SPEC.md`, group/grid/lighting specs, roadmap, plugin and console specs are useful architectural history and aspirations. Their “Implemented” labels are not sufficient evidence.

The docs site is an existing SvelteKit application and can be retained when swapping its embedded runtime. Rewriting it in Rust is unnecessary for product parity. Browser tutorials intentionally avoid the desktop HTTP server.

## Divergences to preserve or resolve explicitly

| Topic | Evidence | Rewrite treatment |
| --- | --- | --- |
| “Face” as polygon with holes | Original SPEC describes polygon topology; `DraftFaceStore` stores colored triangles | Triangle model is parity baseline; region boundaries/holes are derived for operations |
| Per-instance geometry “overrides” | `recomputeInstanceGeometryFromPrototype` clones ordinary instances; 2,899 of 2,939 nonempty override bundles match their definitions in the 60-fixture census | Normalize proven no-op copies, while preserving real/ambiguous authored differences and legacy codec behavior |
| Reusable children | `GroupNode.children` plus clone/sync helpers store a tree per placed instance | Evaluate shared child layouts and occurrence IDs before freezing the Rust model; do not assume forwarding accessors are duplicate storage |
| Automatic faces/cuts while drawing | Roadmap describes automatic line-to-face cuts; current Line/Construction Line → `GroupNode.addSketchSegment` only inserts/cleans segments | Preserve current wiring; face cutters are used by explicit intersection/boolean/cut tools. Auto-face inference/cutting is optional documented intent |
| Cleanup command scope | Roadmap describes line and face union; current `ModelCleanup.run` only calls line-store cleanup per prototype | Do not advertise whole-model face union as current behavior |
| Quad picking | Old SPEC says four points; `QuadTool` takes three and constructs a parallelogram | Preserve three-pick workflow and resulting fourth corner |
| Modeling precision | SPEC asks for doubles; libGDX vectors/store fields are floats | Propose f64 kernel; compare geometrically with tolerance, never byte-for-byte float strings |
| Double click | Old SPEC says connected geometry; current `SelectTool`/shortcuts distinguish coplanar double click and connected triple click | Follow current behavior, with scenario coverage |
| Circle segments | Old SPEC says 24-gon; current circle/primitives use configurable `circleSegments` | Default 24; preserve configuration and clamping behavior after characterization |
| Scale | Early docs imply uniform scale; current README/tools include reference-axis squash/stretch | Keep axis constraints and later stretch variants |
| Eraser | README lists it; no eraser `ToolId` or registered implementation found | Delete selection is observed; dedicated eraser is intent only |
| Materials/tags/scenes | Old SPEC proposes textured front/back materials, UVs, tags, saved scenes | No corresponding complete persistent system found; do not inflate parity scope |
| OpenSCAD-first roadmap | Roadmap proposes OpenSCAD before domains; current tools directly implement domains and booleans | No OpenSCAD implementation found in inspected app sources; optional future work |
| Boolean backend | Earlier roadmap proposes shared external solid pipeline | Current `VolumeBooleanTool` cuts/classifies triangles locally; preserve actual restrictions and output |
| Schema number | `VERSION=17`, but all 60 fixture models omit a top-level version | libGDX prototype/default omission must be handled; missing version is not “v0” |
| Undo | SPEC asks for inverse commands; current manager snapshots with 3500 ms debounce, app cap 20 | Propose explicit operation transactions; import/preserve old history independently |
| Autosave and recovery | README says every change; `Main` schedules 1500 ms debounce; `.bak` copy created in load path | Match user intent, improve atomic save; never claim old saves were atomic |
| UI persistence | README “settings” is broad; schema stores a limited model subset, many controls use runtime prefs | Separate model settings, application prefs, session state |
| Plugin capabilities | Interfaces declare importers/entities/UI and lifecycle hooks | Test actual reachable hooks; declaration alone is not proof of complete entity serialization |
| Desktop and web | Many services gated by `BuildFlags.WEB_BUILD`/web-safe runtime | Capability registry; desktop-only services not silently required by browser |
| Tutorial formats | Desktop `TutorialScript` has action/message/previews; docs-site JSON has steps/actions/success rules | Two adapters/schemas, not one permissive schema |

## Confidence limits and baseline work

No live editor, renderer, MCP endpoint, Android device, browser component, or release artifact was exercised in this audit. Parser subsets, corner-case booleans, hotspot cascades, large-model performance, and exact UI focus behavior need characterization in T00 and feature-task scenarios. Do not label a generated screenshot or a manual tutorial checkpoint an automated assertion.

### Supplemental live inspection (2026-10-18)

The running desktop build was inspected separately after the static audit. It identifies itself as 5.7.1 (`5c97836026cf8274ad43d99542a3057b215231c4`), while the pinned source checkout remains 5.7.2 (`acd84f4c67509e10363c33850501eb8f4fc3ffa7`). Treat these as supplemental UI/runtime observations, not evidence about the pinned source or geometry parity. Screenshots, command-catalog counts and settings notes are recorded in [live-reference.md](live-reference.md). The editor MCP was reachable on port 8765; port 5678 had no listener during this inspection.

No fixed older schema version range is promised yet. Recover representative older flat/rootGroup documents from history in T00, and record what each migration actually supports. Preserve source fixture hashes and avoid copying large embedded undo/base64 blobs into prompts.

Licensing and naming metadata exist in `LICENSE.md`, `docs/CREDITS.md`, and store/build files. Carry their provenance into the release inventory; product-owner licensing and branding decisions are outside this technical audit. No license was changed here.
