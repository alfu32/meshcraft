# ADR-05: egui application shell with a Bevy model viewport

- Status: Superseded by [ADR-06](ADR-06-gpui-raylib.md) and high-priority [issue #21](https://github.com/alfu32/meshcraft/issues/21). Retained as a record of the egui/Bevy migration attempt; issue #20 is closed as wont-fix after the architecture changed.
- Date: 2026-10-19
- Supersedes: The Bevy UI direction recorded in [ADR-04](ADR-04-ui-stack.md). ADR-04 remains historical.

## Context

Bevy 0.20 now renders the 3D model with GPU batches and provides working camera, selection, picking, snapping, guides, tool previews and model interaction. Its UI widgets produced poor visual quality and unacceptable UX for a CAD application. The CPU renderer and egui shell that preceded the Bevy viewport are not the target either: the model must remain rendered and interacted with by the custom Bevy viewport.

The user requested a mature, component-rich UI toolkit while retaining the custom Bevy viewport and its interaction with the normalized object model. CADCraft and SolidCraft both provide useful Rust/egui prior art. Their relevant UI crates are Apache-2.0 licensed: [CADCraft `crates/ui-egui`](https://github.com/storytold/cadcraft/tree/main/crates/ui-egui) and [SolidCraft `crates/ui-egui`](https://github.com/tangency-labs/solidcraft/tree/main/crates/ui-egui).

## Decision

Use egui for the native desktop application shell, hosted in the Bevy 0.20 window through [`bevy_egui`](https://github.com/vladbat00/bevy_egui). Pin a Bevy-compatible release; `bevy_egui` 0.43.1 currently lists Bevy 0.20 support in its [release notes](https://github.com/vladbat00/bevy_egui/releases) and [crate dependencies](https://docs.rs/crate/bevy_egui/0.43.1). The compatibility spike in T58 must verify the actual renderer, event ordering and platform features before implementation is migrated.

Use one native window, one winit event loop and Bevy's WGPU device. Bevy renders the custom 3D scene into the center viewport. egui owns the surrounding menus, toolbars, panels and dialogs and renders above the scene through the integration plugin. The egui layout's central content rectangle is the contract for the Bevy camera viewport: convert its logical points to physical pixels using the active scale factor and update camera viewport/projection on layout, resize and DPI changes. If plugin ordering makes the central scissor path impractical, the spike may prove a Bevy render-to-texture surface displayed in egui; do not introduce a second independent renderer/window without recording a new decision.

Visual direction: use SolidCraft's application organization as the primary reference: a compact textual menu row, a tabbed/grouped command ribbon below it, a central Bevy viewport, and a persistent right inspector for selection and settings. Use CADCraft's searchable command completion treatment for Ctrl+Shift+P. Build these patterns on Meshcraft's existing command catalog and settings parameter schemas rather than copying either application's model or command implementation. A new document remains an empty model, while the shell, tools, settings and explicit empty-state labels remain visible and useful.

Compatibility spike status (2026-10-19): the desktop pins `bevy_egui 0.43.1` with Bevy 0.20 and renders the menu/ribbon/inspector shell plus a searchable command window. The first inspector form reads the `model.update_settings` parameter schema and dispatches edits through that shared command. The camera viewport is mapped below the top chrome and to the left of the right inspector in physical pixels. The plugin uses egui 0.36, while eframe remains on egui 0.37 for the existing shared/WASM shell; this duplicate-version arrangement is temporary and must be removed as components migrate. Compile, tests and native startup are verified; manual visual and click review remains open, and the legacy Bevy UI is still present during this staged migration.

egui owns application presentation only:

- Menus, grouped toolbars, searchable command palette, dockable/scrollable inspectors, settings forms, object list, dialogs, status and feedback.
- Reusable labeled text/numeric inputs, dropdowns, color controls, checkboxes, sliders, validation feedback, tooltips and keyboard focus.
- A consistent Meshcraft theme with readable type, spacing, meaningful labels, clear active/disabled states and visible focus.

Bevy owns the viewport and all model-space interaction:

- GPU scene batches/materials, camera and projection, lighting/shadows, grid, guides, cursor and tool feedback.
- Selection and marquee overlays, ray picking, snapping, viewport input, and interaction with the shared Meshcraft object model.
- Geometry and transform cache synchronization from document revisions; UI activity must not trigger scene rebuilds unless the authoritative document changed.

The normalized `Document`, command registry/IDs, selection, tool state, undo, persistence, preferences and status remain authoritative and UI-framework-neutral. egui actions query shared command state and dispatch shared commands; widgets never write model internals. egui pointer/keyboard capture and the reported layout rectangle determine which input system owns each event. All existing click, double/triple-click, marquee direction, snap, Escape, nested-object edit and command-palette semantics remain intact.

## Component reuse

T58 audits both Apache-2.0 UI crates before building equivalents. Candidate areas include CADCraft's modular front-end boundary, menus/chrome, docking, palettes, dialogs, themes and interaction tests, and SolidCraft's CAD panels, forms, dialogs, controls, accessibility and widget tests. Copy or adapt focused components/assets when their dependencies and APIs fit Meshcraft. Do not import either application's renderer, document model, command engine or entire UI crate. Preserve license notices, copyright headers and required source attribution for copied/adapted material; record candidates, disposition and required notices in the T58 handoff.

## Consequences and acceptance

- The Bevy scene renderer stays on the same GPU/window path; egui does not project or redraw model geometry.
- egui immediate-mode layout runs during UI updates, so keep large panels virtualized or bounded and measure its cost separately from viewport rendering. Preserve event-driven idle updates and request redraws when UI/model state changes.
- Theme/component ecosystem and integration versions are explicit, pinned dependencies. UI integration churn is isolated from core, IO, model semantics and shared commands.
- C40 covers actual control editing/focus, menu and command parity, UI/viewport input ownership, center-rect camera mapping, resize/DPI, viewport workflows, visual review and performance. C17/C23/C26/C39 remain applicable. Do not claim the #18 performance issue resolved until the reproducible large-scene measurements are recorded.

## Upstream evidence

- egui describes itself as a GUI library that integrates with game engines, lists text editing, sliders, color controls, panels, scroll regions, windows and tooltips, and names `bevy_egui` as an integration option: [official egui repository](https://github.com/emilk/egui).
- `bevy_egui` provides Bevy integration and has a Bevy 0.20-compatible 0.43.x release: [compatibility table](https://github.com/vladbat00/bevy_egui#bevy-support-table), [0.43.1 release](https://github.com/vladbat00/bevy_egui/releases/tag/v0.43.1).
- [CADCraft UI crate](https://github.com/storytold/cadcraft/tree/main/crates/ui-egui) and [SolidCraft UI crate](https://github.com/tangency-labs/solidcraft/tree/main/crates/ui-egui) report Apache-2.0 project licenses; verify file-level headers and bundled asset licenses before copying individual files.
