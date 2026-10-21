# Interfaces, rendering, and integrations

### U01 Button workspace

**Observed.** The pinned source defines twelve movable floating toolbars: Point Construction, Entity Construction, Modification, Modification 2, Architecture, HVAC, Primitives, Mech, Voxel, Actions, Camera, and Rendering. The supplied running-app captures show thirteen labeled groups because In-tool Operators is presented as its own group there. Preserve the source/app distinction when comparing screenshots: the contextual operator group is shown separately in the live reference. Labels, icons, active state, delayed hints, adjustable icon/line sizes, docking/collapse behavior and persisted layout are part of workflow intent. In Buttons and Hybrid profiles, each visible group is an independently movable floating toolbar over the viewport; the workspace must not reserve a fixed tool column that substantially reduces the work surface (#16).

Preserve access to the same modeling operations in a button-oriented workspace. Hidden toolbars must remain recoverable through View/Window and the palette. Avoid copying the existing 7,468-line UI class; render toolbar definitions from command/tool metadata. Main toolbar categories may be reorganized if every legacy operation remains discoverable and mappings are documented.

Source: `K/ui/SketchUiOverlay.kt`: toolbar definitions, `builtInToolbarCommandTargets`, layout prefs. Acceptance: C17, C23.

### U02 Conventional desktop menus

**New: explicitly requested.** Provide three switchable profiles: **Desktop Menus** (desktop default), **Buttons**, and **Hybrid** (menus plus toolbars). All profiles retain viewport, status/measurement feedback, inspector access, shortcuts, and the command palette. `Ctrl+Shift+P` opens the complete palette of available commands. Every enabled menu, toolbar, palette, automation and tutorial action routes through the same command registry; a toolbar's visibility never determines command availability. Switching profiles preserves the document, active context, selection, camera and current tool draft, and persists as an application preference. Right-clicking the viewport opens the same Tools command tree at the pointer without changing the active tool or accepted picks; a secondary-button drag remains available for camera control (#17). Middle-button drag also pans, matching Shift+right-button drag; ordinary right-button drag orbits. Both pan gestures use half the prior GPUI pan multiplier (`distance × 0.00125` per pointer pixel), as requested on 2026-10-20.

Proposed top-level organization:

| Menu | Required contents |
| --- | --- |
| File | New document, Open, Save, Save As, Import geometry, Export geometry, Export IFC, Export view SVG/PNG, Exit |
| Edit | Undo/Redo, Delete, Clear selection, selection operations, Cleanup, Flip faces, color/paint entry |
| View | Orbit/Walkthrough/Orthographic and six standard views, grid/guides, per-type visibility/lock/wireframe controls, UI profile, toolbar visibility |
| Draw | Lines/paths, planar shapes/mesh, dimensions/text, primitives, voxel tools |
| Modify | Move/rotate/scale/stretch variants, arrays/copy variants, push/pull/sweeps/revolve, cuts/sections/intersections, booleans, fuzzy/remesh |
| Objects | Create object/group, place instance, enter/exit definition edit, explode, prototype management, xrefs, hotspots |
| Tools | Architecture, HVAC, Mechanical; model/tool settings; structured automation status when enabled |
| Render | Lighting/shadows, raytrace, pathtrace, stop, resolution/workers, image save |
| Window | Every inspector/settings panel, toolbar restoration, reset workspace layout |
| Help | Manual, tutorials/player/recording, keyboard reference, about/build information |

This is a product menu proposal, not a claim that these legacy command IDs already exist. T06 creates canonical IDs for direct callbacks; T08 maps all UI actions and T17/T44 check complete coverage. Parameter dialogs open from commands. Menu tool entries activate the same state machine as toolbar/palette actions. Every built-in tool receives exactly one primary menu location, with optional aliases. Deferred Groovy/plugin/developer-console actions do not appear as available first-phase features.

Menus display shortcuts, disabled reasons where helpful, active tool and checked visibility/camera/profile state. Support keyboard opening (F10/Alt where appropriate), arrows, Enter, Escape, focus restoration, accessible names, high-DPI scaling, and platform shortcut conventions. Mac menu-bar integration is evaluated in T01/T08; an in-window menu must still feel conventional and work from keyboard. OS-native rendering is a design choice to verify, not assumed by selecting a Rust widget library.

While a menu is open, tool-cycle letters, Delete, numeric typing and pointer clicks cannot leak into modeling. Dismissal restores prior viewport/tool focus without committing a point. Menu-bar height changes must update the viewport rectangle used by camera projection/picking/capture/automation. Test menu/button/palette/HTTP invocation produces identical geometry, selected results, dirty state and undo count.

Acceptance: C17, C18, C23. This requirement is an initial desktop release gate.

### U03 Inspectors and settings

**Observed.** The legacy source defines Selection, Object Info, Objects/prototypes, Model Settings, Polyline Settings, Vector Text Settings, Architecture Settings, HVAC Settings, Hotspot Settings, Random Tools Settings, Lighting, and Help/About panels. Plugin Manager is legacy/deferred. Rendering/tutorial controls also exist as UI groups. In the supplied running-app walkthrough, the right accordion is labeled “Panels” and shows Selection, Object, Objects, Model Settings, UI Settings, Help/About, Polyline Settings, Vector Text, Architecture - Walls/Slabs/Stairs/Frames, HVAC Settings, Hotspot Settings, Random Tools, Tutorials, and Shader Settings. The walkthrough is indexed by screenshot and visible crop in the [live reference](../live-reference.md#right-hand-panels-screenshots); it records sample state rather than factory defaults. The legacy `Lighting` panel and screenshot `Shader Settings` label are recorded separately until their equivalence is verified.

Selection exposes counts and appropriate entity controls: show/hide, lock, wireframe, color, text metadata, semantic architecture/HVAC fields. Object info exposes name/glue/edit state; objects expose placement/import/delete/xref management. Model settings include unit size/name, grid spacing, circle segments, snap radius and walkthrough tuning. Preserve defaults and allowed ranges from source; do not infer one shared panel model for all entity kinds. Field edits validate and commit transactions rather than causing a save on every invalid keystroke.

**Selection table clarification (2026-10-20).** The columns are Type, T (total), S (selected), V (visible), E (editable), W (wireframe), replacing D/M with V/E. Totals include hidden/read-only entities. V/E/W are live checkboxes per row and in the header; each header checkbox sets the entire column. A partially enabled column is unchecked; clicking it enables the column. Hidden types cannot be edited or selected, and their E controls are disabled until visible. Locking an entity type retains visible geometry as a snap reference while excluding it from edits/selection. Changing visibility/editability removes affected selections and cancels pending tool drafts so they cannot modify newly locked content. Object controls apply to placed subscenes, not the imported root scene wrapper. Dimension/Text and architecture/HVAC rows apply to their grouped types; line-only geometry naturally remains line-only when W is enabled. These are session display controls and do not modify project geometry or create undo entries.

The requested table and shader preset are retained as [table reference](../references/selection-controls/requested-table.png) and [shader reference](../references/selection-controls/requested-shader-defaults.png). Shader defaults are azimuth −35°, elevation 55°, directional 0.73, ambient 0.64, shadow alpha 0.5, specular 0.15, shadow bias 0.00025, normal bias 0.037, shadows enabled and Medium antialiasing. Existing explicitly chosen antialiasing preferences remain respected.

**Point-array keyboard input (2026-10-20).** Enter accepts the current multipoint path through the shared finish operation; Escape cancels an uncommitted draft. Mesh chains use Enter to end the chain without undoing already committed segments/faces. Polyline, Double Line and planar/spatial sweep/ribbon paths reserve A for an arc through a control point, C for a circular arc using a center, and L for a straight segment. These keys must not also cycle tools. Shift+L explicitly cycles planar construction tools; plain L remains a compatibility shortcut outside path input. Mode switching preserves accepted path points and discards only an incomplete arc control. Menus, palette and focused text editors consume their own keyboard input.

**Current GPUI behavior requested by the user (#21).** Settings section headers collapse/expand their own content. The optional accordion preference keeps only one settings section open; it does not collapse toolbars. Keep toolbar labels alongside transparent, legible icons. Model/UI fields use editable controls with validated command-backed changes. UI Settings includes the permanent grid normal (X/Y/Z), grid extent, normal/thick model-segment pixel widths, text scale and toolbar size. Obsolete Flow L→R/T→D controls are omitted. Shader Settings provides GPU antialiasing at Low/Medium/High quality, plus Off. The compact command palette has a search/filter field, left-aligned results and keyboard activation. Dropdown menu items are left-aligned. Tool measurement text is a GPUI overlay, while preview geometry stays in raylib. Preview edges use the configured thick model-line width (at least the normal width), with faces drawn before outlines. GPUI pointer motion updates all tool-specific draft states, including Push/Pull, before calculating feedback; it does not accept another point or edit the document. Command palette pointer/scroll input must not reach the viewport; Delete acts on the current selection outside focused text editors.

Source: `K/ui/SketchUiOverlay.kt`, callback wiring `K/Main.kt:1038`, settings classes. Acceptance: C17, C22, C23.

**GPUI layout visibility (2026-10-20).** UI Settings provides saved visibility
controls for the entire menu bar, each dropdown (File, Edit, View, Insert, Format,
Draw, Dimension, Modify, Objects, Tools, Render, Window, Help), the ribbon including
its tabs, and the left tool panel. All are visible by default, including when
loading older preferences. Individual dropdown choices survive hiding/revealing
the menu bar. Hiding a section removes its reserved layout space and updates
viewport resolution and pointer/picking offsets at every DPI. Any hidden menu
popup closes. UI Settings on the right and Ctrl+Shift+P remain available to
restore the layout. Preferences do not modify the document or its undo history.

### U04 Preferences, status, and feedback

**Observed + new profile preference.** Persist application workspace profile, panel/toolbar layout/visibility, UI scale/line widths, grid presentation and navigation tuning separately from model settings. Reset-layout must recover panels placed outside a changed monitor configuration. Status shows active tool, cursor screen/world coordinates, snap label, numeric input, operation messages, load/save progress and edit context.

The construction cursor uses only red, green and blue axis segments without arrowheads, with a small center bullet drawn last in the viewport overlay. On a segment snap, blue follows the segment, red remains parallel to the ground and green is the remaining orthogonal axis (#14, #15). Selected vertex handles are small red bullets drawn in screen space (#15).

Keep a persistent recent-project list of at most ten paths. Successful opens move the project to the front and remove an older duplicate; failed or cancelled loads do not add an entry. Expose entries under File and through the command palette, using the normal load/replacement flow.

Preview overlays show selections, hover, guides, measurement labels, tool geometry and handles in distinct readable styles. Model captures can omit UI while retaining appropriate world overlays. Preference changes must not mutate model geometry or create arbitrary geometry undo entries. Menus and panels stay synchronized with actual state after undo/load/context changes.

Source: `K/ui/{StatusModel,SketchUiOverlay}.kt`, feedback classes, runtime prefs in `K/Main.kt`. Acceptance: C17, C23.

### U05 Geometric tool feedback

**User-confirmed rule (GitHub issue #1).** Every enabled tool that constructs or changes geometry shows live feedback while points or numeric input are pending; selection is the exception. Prefer a preview of the generated geometry, rendered as lines/faces where possible. Otherwise draw a vector from the previous accepted point to the current snapped pointer. Vector feedback displays its length and signed relative ΔX/ΔY/ΔZ in the model's named units, rounded to at most three decimals. Preview geometry, measurements and the eventual commit must use the same snapped points, transform and numeric values. Cancellation clears the preview without changing authored geometry or history.

Acceptance: C34.

### R01 Interactive viewport

**Observed.** Faces, independent edges, RGBA transparency, ground/grid, semantic objects, bitmap/vector annotations and feedback overlays render with depth-aware composition. Front-facing triangles are brighter; back-facing triangles are darker and slightly bluer. Coplanar triangles on one surface must share a rendered orientation even when adjacent triangles use distinct vertex IDs at coincident positions. Real-time directional/ambient/specular lighting and per-type display controls exist. Face winding and selected/hover appearance matter. Shader output is not a requirement for a PBR material system.

Render both sides of each ideal mesh triangle. Keep the back face as a render-only layer offset a small scale-relative distance opposite the normal to avoid shearing. In filled display, omit coplanar shared triangle edges so triangulation seams do not appear; retain true boundaries, creases and separately authored lines (#13).

Use incremental revisioned geometry buffers and per-instance transforms; avoid full-document snapshots and mesh rebuilds every frame. Keep display/picking/selection policies consistent and define transparent sorting/two-sided behavior using source scenarios. Basic flat/shaded rendering ships with the first viewport; later lighting/shadow work completes fidelity. Device loss/resize must not corrupt documents.

Source: real-time render/chunk paths in `K/Main.kt`, `K/render/SketchDefaultShader.kt`, shader assets. Acceptance: C09, C24, C26.

Authored unselected line segments render black in the light canvas palette; selection and helper colors remain distinguishable. Command buttons use the original k3d SVG grid and its named CSV tiles. Generate 32/48/64-pixel icon atlases during the Cargo build and cache cropped GPUI images outside redraws. Preserve command IDs, tooltips and disabled availability.

### R02 Lighting and shadows

**Observed.** Model settings serialize shadow/directional/ambient/specular values/alphas and shadow bias, normal bias, PCF mode, dither, CSM flag. Controls update real-time lighting. Shadow maps cast entity shadows onto both the ground and other entities. Initial defaults include directional 0.73, ambient 0.59, specular 0.2; legacy shadow numeric scales are unusual (2500/5620) and cannot be assigned directly to a different GPU pipeline.

Persist legacy fields and define an explicit conversion to the new renderer. Match intent and representative image outcomes, not shader-internal units. Check acne, peter-panning, orbit stability, ground extent, transformed instances, alpha, distant geometry, toggles and quality changes. Do not claim “CSM flag serialized” alone proves all cascade quality behavior.

Source: `K/ui/{LightingSettings,ShadowSettings}.kt`, persistence DTOs, `SketchShaderProvider.kt`, GLSL assets. Acceptance: C24.

### R03 Offline raytrace and pathtrace

**Observed.** Two CPU render modes (`RAYTRACE`, `PATHTRACE`) operate on immutable scene/camera/light snapshots, use progressive tiles and worker threads, show progress/preview and save PNG. Controls include resolution divisor (1/2/4/8/16), worker count, pruning, overlay blend/blur, glass transmission, light level and camera light. Preview and final buffer are separate.

Port snapshot extraction and progressive scheduler before shading details. Jobs are cancellable and cannot publish results for a stale/replaced document unnoticed. UI remains responsive while rendering; resizing and worker completion are safe. Browser gets cooperative/incremental work unless threading support is explicitly established. Seedable stochastic sampling permits numeric/image-quality comparisons without demanding identical pixels.

Source: `K/render/{SceneSnapshot,RenderController,CpuRenderers,ProgressiveTileScheduler,RenderBuffer}.kt`, render actions in Main. Acceptance: C25.

### R04 View capture and SVG/PNG

**Observed.** Export screenshot and SVG-view commands exist; PNG capture can include model overlays and special minimal/restored UI state. SVG is a projection of the current view, not full semantic SVG model persistence. Offline render buffers also export PNG. Captures are asynchronous and must signal completion/file freshness rather than returning a successful dispatch as proof of a finished image.

Keep capture inclusion rules, camera/view rectangle and output size explicit; menu profile must not alter the intended model-only composition. Do not use OS screenshots as the canonical renderer regression method. Record fallback capture methods when used.

Source: `K/Main.kt`: screenshot/SVG/export/render-buffer functions, `docs/automation/*`, docs-site capture manifest/scripts. Acceptance: C24, C25.

### X01 Mesh interchange

**Observed.** Import: OBJ, STL auto/ASCII/binary, FBX, glTF, GLB, DAE, DXF, 3MF, AMF, IFC. Export through `MeshIo`: OBJ, STL ASCII/binary, FBX ASCII, glTF, GLB, DAE, DXF, 3MF, AMF. IFC export uses a separate exporter. DXF includes line geometry as well as triangles. Interchange does not generally preserve editable prototype/domain/hotspot structure.

Each parser is a supported subset, not a promise of format-complete DCC/CAD interoperability. Preserve current working subsets and document their boundaries. T36/T37 inventory exact entity/primitive/transform/compression/material support and add reference fixtures from examples; use upstream libraries only if their conversions satisfy those fixtures. OBJ includes normals in current output; 3MF has unit handling, instancing/component import work, indexed-mesh validation and manifold/edge checks.

Unit conversion must distinguish model display units, output physical units, extra coordinate scale and 3MF model-units-per-mm. Test no double-scaling, nested transforms, winding, empty models, malformed files, binary data and cancellation. Export selection-versus-whole-scene choices follow actual GUI export callbacks. Unsupported textures/materials/animation or lost parametric semantics are reported, not silently advertised as retained.

Source: `K/export/MeshIo.kt`, import/export UI callbacks in `K/Main.kt`. Acceptance: C27.

### X02 IFC interchange

**Observed.** Separate IFC4 exporter builds project/site/building/storey structures and emits mesh/line chunks as `IFCBUILDINGELEMENTPROXY` products, including resolved architecture/HVAC geometry. It does not emit semantic `IfcWall`/`IfcSlab` authoring entities merely because those source objects exist. Dimensions are also collected as line geometry. IFC import in `MeshIo` is a distinct subset.

Preserve the actual proxy/tessellation representation, spatial containment, names, units and resolved geometry; compare entities/geometry with source fixtures. Test mixed architecture/HVAC/mesh scenes, selected export, units and reopened geometry. Semantic BIM classes and property sets are future enhancements, not parity requirements.

Source: `K/export/IfcExporter.kt`, `MeshIo.parseIfc`, IFC selection/export callbacks in Main. Acceptance: C27.

### A02 Structured HTTP and MCP automation

**Observed + scoped change.** Legacy HTTP binds loopback, default 8765; exposes JSON-RPC at `/` and `/mcp`, status/contract, command discovery/execution and pointer routes. It declares six tools and five resources; routes/schemas are indexed. Calls marshal model work onto the application thread, with timeouts and logs. Pointer actions accept world or screen coordinates; use world coordinates for deterministic modeling tests.

First Rust scope retains command discovery/execution, pointer input, status/contract and scene/selection resources. Groovy execution endpoints/tools are explicitly unavailable until the deferred console work. Report capabilities in the contract; do not claim full old transport compatibility or advertise `k3d_execute_console` as working. Preserve existing supported tool/resource names and route aliases. Adapt documented status/error shapes from actual source tests, including JSON-RPC versus REST logical-error conventions.

New typed commands supply deterministic model reset, geometry setup, selection changes, camera setting, save and snapshots for regression tests. These are new command registrations, not invented legacy IDs. Separate accepted, completed and failed operations; expose completion/scene revision so clients do not rely on sleeps. Bound work at the dispatcher and cancel pending jobs on close. UI and remote operations share transactions and capability checks.

Source: `K/mcp/{McpHttpServer,McpModels,StdoutTap}.kt`, MCP dispatch functions in Main, `docs/MCP_CONTRACT_RELIABLE_AGENTS.md`. Acceptance: C18, C28.

The native Rust server adds an MCP Server accordion with persisted port (default 8765),
Start, Stop and Restart. It starts stopped, binds only IPv4 loopback, validates Host/Origin,
and bounds request bytes/connections/queue/deadlines. Stop, disconnect and expired requests
prevent queued application work from executing; the server closes with the shell. MCP discovery
publishes protocol versions, onboarding guidance, tools, resources and current typed command
schemas/states. Application-owned command execution shares UI transactions and camera effects;
world picks and exact-ID selection drive tools without screen-coordinate guesses. Prototype,
instance, geometry/material/model-setting, script and job queries expose current state. Generated
primitive IDs are decimal strings where needed to avoid JavaScript integer precision loss.
The scripting API catalog is shared with editor completion and contains each public signature,
description and example plus model/this accessors and the exact SDK source. The modern stateless
JSON transport and legacy MCP initialization are implemented; full old REST-route equivalence
remains tracked in T41. See [agent guide](../../mcp.md).

### A03 Developer console and CLI/TUI

**Observed, deferred by user.** Legacy desktop launches a persistent Groovy REPL with `app`, `scene`, selection, camera, lighting, unit/save/mcp/version bindings; terminal UI has input/history/output, meta commands and introspection. `app.run` marshals to the render thread and can defer nested operations. Desktop `groovy <script>` and developer performance/introspection tools are part of that legacy surface.

Initial Rust delivery does not implement the REPL, TUI, arbitrary Groovy evaluation, Java object bindings, or script command. Retain this feature description and source map for future compatibility work. Do not add another scripting language or JVM sidecar speculatively to replace a deliberately deferred feature. Normal executable file-opening arguments are O01.

Source: `K/console/*`, `K/tui/*`, desktop terminal implementations and launcher. Future acceptance: captured script corpus and explicit API migration policy.

### A04 JavaScriptCore plugins

**Observed legacy behavior + new final-product direction.** Current plugins use JVM classes/JARs or Groovy scripts, catalog/enable state and lifecycle callbacks. Capability interfaces declare commands, tools, UI, entities, importers/exporters; plugin results can replace model, add geometry, show status/panels. Runtime support must be checked per hook rather than inferred from interfaces. The final Rust product must have plugins, with WebKit JavaScriptCore as the native execution engine and an application context handed to each plugin.

First desktop delivery still has no plugin runtime or manager UI. T47–T50 implement the required later JSC runtime, manifest/lifecycle, context/SDK, command/tool/declarative-panel/interchange contributions, state handling, manager UI and native packaging. Built-in domain systems remain Rust modules. Groovy/JAR compatibility is a documented port, not a JVM bridge. Preserve unknown model data and report phase-specific capabilities honestly. Browser plugin execution requires a separate adapter decision; native JSC is not assumed to run in WASM.

Source: `K/plugin/*`, `K/ui/{PluginManagerPanel,PluginToolAdapter}.kt`, sample plugin directories and `docs/PLUGIN_DEVELOPMENT.md`; new runtime/context contract in `../plugins.md`. Acceptance: C33 for later plugin delivery; C18/C28 capability honesty in first delivery.

### A05 Tutorials and documentation workflows

**Observed.** Desktop recorder/player observes actions and matches expected actions, with next/previous/pause/stop and before/after PNG previews; it does not automatically replay every user's pointer gesture. Desktop schema is name/createdAt/steps(action,message,previews). Browser docs use a separate id/title/level/initialScene/steps/actions/success schema with some manual or pending checks.

Keep two explicit schema adapters and version them. Menu actions emit the same tutorial action IDs as their button counterparts. Preserve manual completion where automation is absent and distinguish it in reports. Browser tutorials use the component bridge, never require a running desktop MCP service. Keep the existing docs site/tool-reference generation approach unless a separate docs rewrite is requested.

Source: `K/tutorial/TutorialManager.kt`, Main action observation, `docs-site/static/tutorials`, validation/player code. Acceptance: C29.

### A06 Embedded browser contract

**Observed, later delivery.** `<octodraw-editor>` supports promise-based model get/set, selection get/clear, camera get/set, tool select/cancel/pointer, `exec`, `play`, UI visibility, a `value` property, and ready/change/selectionchange/toolchange/error events. One runtime/editor per page is currently supported. Host tutorial config and focus/scroll integration matter. Sparse version-only model seeds can produce invalid zero root axes in the current loader.

Preserve supported method/event names and documented command payloads through a thin JS wrapper around Rust/WASM. New loader validates transforms and supplies explicitly defined safe-new-document defaults; it cannot silently collapse the model. Host commands are correlated, ordered and completed asynchronously. Runtime accepts embed data without writing desktop paths. Changes carry a revision and are debounced coherently. Supporting multiple independent editors is future enhancement, not a parity prerequisite.

Source: `web/component/octodraw-element.js`, `web/component/README.md`, `K/WebRuntimeBridge.kt`, Main embed dispatch. Acceptance: C30.

### O01 Desktop application and launch arguments

**Observed, scoped.** Deliver a native executable on Linux/Windows/macOS with normal window lifecycle, file argument/open behavior, size argument where retained, build/version/about info, file dialogs, working-file association and error reporting. Keep `.octd`/`.k3d` opening; final product name/executable alias is undecided. New/Open must not overwrite an existing named document accidentally.

Developer `groovy`, REPL/TUI and updater-JAR replacement workflows are deferred. A new native self-updater is not implicit scope; release/download channels and packaged updates suffice initially. No Android application or store package is required.

Source: desktop launcher, file paths/dialogs in Main, release definitions. Acceptance: C05, C17, C31.

### O02 Standalone browser application

**Observed, after initial desktop.** Browser mode supplies local model recovery/storage, open/import and download/save via browser APIs, camera/modeling UI and embed-compatible runtime. Browser cannot assume local HTTP server, filesystem xrefs, worker threads, JVM/Groovy, or native dialogs. Use capability-specific disabled states with explanations rather than dead buttons. Preserve existing storage/export behavior through the platform adapter where useful.

Source: `K/WebRuntimeBridge.kt`, browser bridge Java code, web build/component tasks. Acceptance: C30.

### O03 Release, assets, and documentation

**Observed.** Desktop distributions target Windows, Linux and macOS with platform packaging; docs site and webcomponent bundles are separate artifacts. Preserve required source/asset provenance, icon mappings, glyph catalog, shaders/converted shaders, tutorial assets, manual links and version metadata. Generate command/tool references from the live registry. Do not copy generated web build trees as editable Rust source.

Rust release gates include reproducible locked toolchain/dependencies, packaged launch/open/save smoke checks and platform-specific dialog/shortcut/menu verification. Store submission, Android distribution, and changing licensing terms are separate decisions. Retain existing tutorials/docs content and adapt runtime integration.

**User-requested release workflow (2026-10-20).** Use the manual build/release workflow and six-target matrix from the Rust Kode project: Linux x86_64/aarch64, Windows x64/ARM64 and macOS Intel/Apple Silicon. Every target produces ZIP and tar.gz; add AppImage/Snap on Linux, MSI/MSIX on Windows and DMG on macOS. Dispatch at the existing tag reference, without a second tag parameter. Require tag, literal desktop app version, source revision and packaged file manifests to agree before publication. Commit with an incremented literal app version, then create the matching SemVer tag; the user controls major-version increments. Compile native Linux code against the oldest verified practical sysroot and audit the resulting glibc symbol floor; a current orchestration runner must not set the binary ABI baseline. MSIX signing is deferred pending repository secrets. See [packaging instructions](../../../packaging/README.md).

Source: build/release files, `assets`, `docs-site`. Acceptance: C31.

### O04 Responsiveness and performance

**Observed intent + measured target.** Original SPEC targets 60 FPS and typical simple actions below 50 ms; these are goals, not measured audit findings. History repeatedly fixes full scans, full mesh rebuilding, raw-load replay, orthographic indexing, stale chunks, plugin snapshots and excessive feedback. Initial design must avoid those failure modes.

T00 records fixed test hardware, fixture sizes, cold/warm load, picking percentiles, simple edit latency, frame time, memory and save/undo size. T44 compares against that baseline and documented targets with justified thresholds; no invented benchmark claims. Use revisioned indexes, changed-store/chunk updates, frustum culling, bounded previews, cancellable jobs and cooperative browser work. Correct fallback queries are required while an index is rebuilding. A large undo history must not trigger a snapshot copy every frame.

Source: `K/model/{AsyncSpatialIndexing,SpatialHash3D}.kt`, `K/perf/PerfStats.kt`, render/chunk paths and May 4 history. Acceptance: C26, C31.

### A07 Versioned plugin application context

**New: explicitly requested.** JavaScriptCore plugins receive a negotiated, plugin-scoped `ctx` providing targeted immutable document/effective-content queries, occurrence-aware selection, registered commands/tools/declarative panels/interchange adapters, typed transactional edits, events, state and diagnostics. Its public values use stable IDs, occurrence references and explicit document revisions; no mutable Rust stores, raw pointers, legacy GroupNode wrappers or full-model copies on every update.

Commands/menu actions share the host registry, enabled/checked state and undo service. A validated operation batch commits once or leaves the model unchanged; stale revisions return an explicit conflict. Plugin tool previews remain transient. Plugin context and retained callback values have explicit executor/GC/lifecycle ownership. Unload/reload clears registrations, previews and pending replies coherently. Plugin document state is namespaced/versioned and round-trips when the plugin is absent; app settings and runtime JS heap remain separate.

Freeze identity/query/operation boundaries with F10, then implement the engine adapter and context in T47–T50. Engine binding, promise integration, interruption semantics and Linux/Windows/macOS packaging are proof tasks, not assumptions. Full custom plugin entity schemas are a subsequent extension; declarative plugin data must not become a second uncontrolled object model.

Design: `../plugins.md`, `../object-model.md`. Acceptance: C18, C32, C33.

The native window has a permanent title bar with close, minimize and
maximize/restore controls, independent of menu/ribbon visibility. The script and
material editors leave these controls accessible. The canvas and picking offset
include the title-bar height. Arrange Toolbars is omitted from the fixed GPUI
ribbon.

### Native camera and compact ribbon (0.10)

The GPUI offscreen viewport supports perspective orbit, six axis-aligned orthographic views,
and walkthrough. Orthographic rays are parallel; projection, model picking, helpers and pixel-width
strokes use the same camera basis, including top/bottom views. Right-drag orbits; middle-drag or
Shift+right-drag pans. Walkthrough uses right-drag to look, WASD/arrows to move, Q/E or PageDown/
PageUp to adjust eye height, and Space to jump with the existing surface-support and gravity rules.
Movement stops while the canvas lacks focus, the window is inactive, or a modal/menu is open.
Perspective and Frame Document leave walkthrough. UI Settings supplies walking speed/jump/gravity.
The ribbon content is 82 px (previously 138); its buttons use compact 24 px rows and retain labels.
The ground receiver has only an upper face; authored open meshes still render from both sides.

### Native workflow follow-up (0.11)

The ribbon content height is 97 px, adding 15 px to the compact 0.10 layout while retaining 24 px
button rows and labels. Canvas bounds and high-DPI picking follow that height. Native snapping
includes segment and triangle-edge midpoints, prioritized with endpoints ahead of nearest-edge
and face hits, and preserved through the grid/helper layer. Occluded features remain excluded.
The Annotations settings accordion supplies label and text-size inputs. Edit contains clipboard
commands and Copy as Script; Modify contains Flip. Selected segment arrows are viewport feedback.


Normal Paste (Ctrl/Cmd+V) starts a snapped insertion-point pick with floating wireframe feedback.
Paste in Place (Ctrl/Cmd+Shift+V) inserts immediately at the copied coordinates. Both commands
remain available in Edit and the Actions panel. Escape cancels only the floating paste when active.
