# Historical layers and recommended order

The full chronological commit list is in `source-inventory.json.history`. Below, dates and hashes refer to inspected local history, not a published release promise. Subjects are terse and sometimes misleading; source and selected diffs determine dependencies. Many commits include generated binary/assets files, which should not be replayed as implementation work.

| Historical layer | Representative evidence | Dependency lesson |
| --- | --- | --- |
| Jan 14–15: render shell, drafting, extrusion, concave faces, selection | `34fe8ee`, `39eb48c`, `b444a72`, `e6c45b7`, `1ba4680` | Small end-to-end sketch/extrude slice validates geometry and interaction early |
| Jan 16–18: color, transforms, persistence, copies, guides, CLI, crossing selection, custom shaders | `ce32f7e`, `694b6be`, `9e0025d`, `0297f98`, `3722b96`, `0c5ca8c`, `db59e37`, `2438532` | Shared transforms, local planes, settings and render state underpin later tools |
| Jan 19–22: scene graph/prototype separation, plugins, panels and palette commands | `d334551`, `d28e17e`, `5e17300`, `bbd2546`, `c85b8b9`, `c9871f4`, `1335f58` | Definition/instance/origin fixes arrived immediately after groups; design these contracts together |
| Jan 24–Feb 2: typed numbers, dimensions/text, packaging, stretch, cleanup, console and undo | `a5c3470`, `89ac53d`, `9353441`, `a94708f`, `5de624b`, `ea0c1f9` | Explicit transactions and common numeric handling should precede many tools, even though undo came later historically |
| Feb 3–10: polyline/double-line, hole cutting, scaling fixes and voxel objects | `0898c50`, `dca0b87`, `cf3ff0a`, `b14b23f`, `8481b9a`, `9b7d36c` | Path primitives, planar splitting and special prototype kinds enable domain generation |
| Feb 11–15: architecture, stairs/walls, section/intersection math, cameras, MCP, swipe extrusion | `8010723`, `31a8c20`, `6f763d8`, `91dc052`, `07a745b`, `c424e43`, `8324f74`, `9987283` | Shared selection/path/mesh contracts support domain controls and automation |
| Feb 17–21: HVAC then dynamic hotspots | `560bd17`, `fec930c`, `a27de15`, `7772394`, `92df998`, `eae3851` | Hotspot definitions and runtime overrides are a separate layer over reusable objects, not simple UI handles |
| Feb 22–Mar 11: Android, UI docking, interchange and browser/component | `dee3ca0`, `1a38ef6`, `9b6fb91`, `fd21d49`, `268a796` | Platform IO and focus need adapters; Android is now explicitly deferred |
| Mar 14–24: tutorial recorder/player, spatial indexes, compression, arrays, 3MF, UI/async indexes | `dca228a`, `ed3f6c6`, `a506cc3`, `4ea63f5`, `40836c5`, `38f401a`, `66989f0` | Large models expose loading, preview, history and stale-index costs; establish measurements early |
| Mar 26–Apr 2: staged/cancellable load, object pick fix, threaded CPU rendering, DXF | `ce93f1e`, `c100c94`, `453880e`, `16f60d4`, `0b5b3b5`, `8a897d4` | Correct picking, job lifetime and coordinate conventions must survive performance changes |
| Apr 9–20: grid/line UI, in-tool operators, camera touch/ortho fixes, exports and xrefs | `9fd1ba2`, `1f15974`, `5621b2a`, `778761a`, `5abb269` | Prefer shared action metadata and platform-neutral IO over feature-specific callbacks |
| May 4: sustained geometry/render performance changes | `e801796`, `f499919`, `f77131f`, `ae8f822`, `7085cc8`, `813459c`, `4ebb2c8` | Bulk loading must bypass edit-time cuts; cache revisions/frustum/ortho bounds and incremental chunks are design constraints |
| May 5–8: docs overhaul, browser tutorials, click/touch interface refinement | `1d5859b`, `87528c6`, `f770155`, `b53cb4d` | Documentation/tutorial automation should be backed by live command metadata; menus extend the shared UI model |
| May 13–27: mechanical tools, solid booleans/object cutting, gears, revolve/stretch-scale and 3D sweeps | `ae5ef89`, `ed691e7`, `35c70f3`, `981d3ea`, `aa06780`, `cbf3db6`, `02b7536`, `0674721` | Build booleans after shared mesh intersection/cutting; build mechanical generation on path/sweep infrastructure |
| July 1–7: primitives, randomness, patch refinement, scatter; OBJ normals | `4c923a2`, `6327127`, `94ae742`, `bc73d71`, `8a45d17` | Primitive generators can move earlier; fuzzy tools belong after payload/copy/selection and surface queries |
| Sep 20: store translations, encoding and release fixes | `a99f626`, `e852b9a`, `acd84f4` | Distribution metadata is separate from modeling semantics |

## Deliberate deviations

1. **Characterize files before translating tools.** Sparse defaults, prototype axes and generated domain geometry can make a successful JSON parse unusable. Add raw DTO loading and fixtures immediately, then complete editing semantics by feature family.
2. **Centralize transactions and commands before most interaction.** Legacy undo arrived after many tools and the palette remained partly tied to plugin/UI infrastructure. New menus, buttons, typed automation and tools share application commands from the beginning.
3. **Ship a conventional desktop menu shell with the first editing slice.** This is the user's explicit addition. It changes viewport bounds, focus, command coverage and discoverability; postponing it would force input/UI rework.
4. **Normalize prototype/instance/override data before grouping UI.** History's early group/origin fixes and later hotspot overrides show the cost of treating these as one mutable mesh. The expanded object-model audit finds no-op geometry snapshots and cloned child layouts; T46 settles ownership and migration before T02/T03 contracts harden.
5. **Build performance boundaries early; tune late with measurements.** Raw loading, versioned caches, immutable worker inputs and correct stale-index fallbacks are foundational. Specialized chunk/index tuning follows measured scenarios, not speculative work.
6. **Move primitives earlier, keep booleans after cuts.** Primitives provide useful deterministic test solids; object booleans reuse triangle intersection/splitting and parent-context semantics. The OpenSCAD-first roadmap is not the current implementation's dependency graph.
7. **Keep domain built-ins, defer plugin implementation from the first desktop delivery.** The final product now explicitly requires JavaScriptCore plugins with a versioned host context. Design the model/query/transaction boundary early and implement the engine/SDK in later T47–T50. Groovy/JVM execution and the developer console/TUI remain deferred; native modeling has no interpreter prerequisite.
8. **Deliver desktop first; browser integration later; Android later still.** Compile/capability boundaries are tested early so browser work remains practical, but no Android implementation or package gates desktop. Existing browser docs/component contracts remain a later tracked delivery.

## How to use history economically

Read a relevant current class first. For an ambiguous contract, use `git log --follow -- path`, then `git show HASH -- exact/path`. For a known regression, inspect the named fix and reproduce its scenario. Do not read all 559 commits into an agent prompt. The symbol index and task cards name bounded starting points; update a task's evidence note after resolving ambiguity so the next agent does not repeat it.
