# Ely GPUI toolkit investigation (#26)

Reviewed 2026-10-20 for [issue #26](https://github.com/alfu32/meshcraft/issues/26). This is an evaluation, not approval or implementation of a production toolkit migration. Meshcraft's manifests, lockfile, application code and working layout are unchanged.

## Recommendation

Ely is a credible candidate for a **vendored, isolated integration proof**, particularly for menus, script editing, diagnostics, color controls, tours and consistent theming. It is not yet a qualified replacement for Meshcraft's working shell. Its broad catalog is backed by real source and tests, but the project is young, largely maintained by one author, and the reviewed head does not have an all-green upstream CI status.

Use an immutable local snapshot plus a reviewed dependency closure if the proof proceeds. Do not use the installation page's unpinned Git dependency in production. Preserve the raylib offscreen component, explicit GPU image eviction, headless commands, model, project storage and current layout throughout any migration.

## Provenance and availability

- Canonical source: [Gitea repository](https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components).
- Reviewed commit: [`7f7a715d797d8450eed4f17ae8002b5427d01d28`](https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components/src/commit/7f7a715d797d8450eed4f17ae8002b5427d01d28), dated 2026-10-19. All source observations below refer to this revision, not a moving branch.
- Tree: `6a9d32407494bebbbf1e156b1ab87e6fa3065145`.
- Repository clone and locked Linux dependency fetch succeeded. The site installation URL returned HTTP 200 via curl; the browser reader could not render it, so the installation text was also checked in the pinned `frontend/src/docs/installation.md`.
- Current GitHub API requests for the former repository return 404. Search still finds cached GitHub pages. Those cached pages are historical evidence, not a currently available origin.
- The commit carries a PGP signature, but its signer was not independently authenticated in this review. A source hash provides an immutable review identity, not proof of a trustworthy author.
- The [audit manifest](ely-gpui-audit.json) records hashes of the source archive, tree, manifest, lockfile and license. A review archive is retained locally at `/tmp/ely-gpui-component-7f7a715-reviewed.tar.gz`; its existence is not a committed vendor dependency.

### Maintainer and independent feedback

The Gitea account, [public portfolio](https://zacharyzhang.com/) and cached GitHub profile consistently identify the maintainer as Zachary Zhang (`ZacharyZhang-NY`, display name 雷电芽衣). The portfolio's employment, credentials, adoption and maintainer-role claims are self-descriptions; they were not independently verified and are not used as security evidence.

The public [r/gpui launch thread](https://www.reddit.com/r/gpui/comments/1ww60bc/i_create_a_gpui_library_that_contains_1272/) contains a positive independent response about the breadth of the components. The author's response calls it an initial release and invites bug reports. Cached [issue #6](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/issues/6) shows an external request for two/three-column layouts; the source has a corresponding column-shell implementation and later fixes. This is evidence of engagement and responsiveness, not a sustained production or security endorsement.

No substantial independent security review or detailed long-term production report was found in the searches performed. Social reposts, stars and automated repository summaries were not treated as audits. The cause of the maintainer's reported GitHub ban was not verified. No conclusion about maliciousness follows from the report or from a 404.

## Stability: observed evidence and limits

- Crate version `0.1.1`, `publish = false`; the README explicitly describes the project as early.
- The reviewed branch has 694 commits, beginning 2026-09-24. Its shortlog attributes 690 to the principal author, with four other single-commit authors. This is a concentrated maintenance dependency, despite frequent updates.
- There are 953 Rust files and 227,112 lines under `src/`, including tests; 2,152 `#[test]`/`#[gpui::test]` annotations were counted. Counts are inventory, not proof of widget quality or test effectiveness. The site's 1,279-component figure should not be read as 1,279 independently production-qualified subsystems.
- The source includes behavioral tests for menus, focus, scrolling, inputs, editor operations and tours. See `src/menus/tests`, `src/forms/tests`, `src/editor/tests`, and `src/overlays/tests/guides.rs` at the pinned revision.
- The Gitea status API for the exact reviewed commit reports overall **failure**: crate job failure, Windows and site success, Linux pending. Logs require authentication, so the failure's cause was not established. Local build/test results are recorded separately below.
- No semantic release series was found on Gitea: zero releases and one tag, `pre-coauthor-strip`, which is not a conventional release tag.
- All component modules are publicly mounted by `src/lib.rs`; the feature list does not provide per-chapter pruning. A consumer compiles a large library even if it initially uses a few widgets. We should measure compile/binary/runtime costs rather than assume unused UI is free.

## Coverage for Meshcraft

| Need | Pinned implementation | What Meshcraft still owns |
| --- | --- | --- |
| Dropdown/application menus | `menus::{MenuBar, DropdownMenu, ContextMenu, SearchableMenu, Menu, MenuItem}` | Command registry wiring, enabled state, mouse-down/keyboard semantics and focus/scroll tests |
| Script editor | `editor::{CodeEditor, EditorEvent, Diagnostic, CompletionMenu, SignatureHelp}`; syntax, selection, undo, folding and minimap code | JavaScriptCore execution, object-script lifecycle, cancellation, host APIs and compiler/runtime diagnostics. Visual editor/LSP controls do not implement a language server. |
| Long-operation feedback | `feedback::{Toaster, ToastViewport, NotificationCenter, AsyncView}` and motion/loading controls | Background job scheduling, progress events, errors, retry and cancellation |
| Errors/script errors | `editor::{ProblemsPanel, OutputPanel, DebugConsole}` and `feedback::ErrorView` | Feeding structured command/runtime errors and navigating to the actual script location |
| Colors/materials | `forms::{ColorPicker, ColorPalette, EyeDropper, GradientEditor}` | Material data, texture handling, model transactions and renderer/shader integration |
| Guided onboarding | `overlays::{Spotlight, Tour, TourStep}` and onboarding helpers | Workflow steps anchored to Meshcraft's stable command/control IDs and completion state |
| Inputs | Text/number/code/JSON inputs, selects, comboboxes, sliders, masks, validation and file controls | JSON schema adapters, units, validation and transaction boundaries |
| Theming | `Theme`, palettes, light/dark modes, high contrast, control/text tokens and reduced motion | Our light modeling canvas, command icons, user preference persistence and host theme policy |

These are source-backed capabilities, not native Meshcraft end-to-end acceptance. For example, Ely's dropdown trigger uses a click callback; menu mouse-down behavior must be checked against Meshcraft's explicit requirement, not silently changed during adoption.

## Telemetry, network and build review

**Finding: no automatic telemetry sender, crash uploader or hardcoded analytics collector was found in the reviewed first-party startup and network-capable paths. This is a bounded source review, not a guarantee that all 227k lines and transitive dependencies are free of malicious code.**

Evidence and qualifications:

- `src/lib.rs::init/setup` checks bundled assets, loads fonts, sets up the theme and binds keys. No startup registration, account or remote update call was found there.
- `src/settings/preferences.rs` has `Privacy.telemetry` and `crash_reports` fields. They are state passed to UI switches and callbacks; they do not themselves install a reporter or transmit data. A grep hit for “telemetry” is therefore not evidence of collection.
- `src/devtools/request.rs` and `src/devtools/socket.rs` are request/connection UIs with host callbacks. AI/chat components similarly expose UI/data callbacks; no automatic AI-service client or credential collection was found in the scanned paths.
- Actual resource-capable code exists: URL-backed images/documents can use GPUI's host HTTP client, external links call `open_url`, the macOS `wry` webview can load host-supplied URLs, file controls read chosen files, and the terminal can launch the host-supplied program or shell. These are intentional features, but should remain unavailable unless Meshcraft needs them. Untrusted document images should not trigger remote requests silently.
- Selected `gpui-pre` paths were reviewed: native `Application` starts with a `NullHttpClient`; a usable client must be supplied by the host. Browser platform constructors use browser fetch. This is not a transport-independent guarantee of no network behavior throughout the dependency graph.
- Many widgets emit local `log` messages. Some include user values, paths or text, e.g. `src/canvas/writing.rs`, `src/tables/gridkeys.rs` and `src/misc/scanner.rs`. These are not remote telemetry, but could expose private data through an application's logging setup. A vendor patch or strict default logging policy is warranted before use with scripts/material/project data.
- The optional GPUI tracing dependency can start Tracy profiling when explicitly built with its tracing configuration/environment. That is profiling capability, not a discovered automatic remote collector. Production builds should keep it disabled and verify their actual feature/environment configuration.
- No Ely root `build.rs` or Git submodule was found. The lockfile contains 814 packages, 813 registry checksums and no Git-sourced dependencies. The Linux resolution has 501 package nodes, 51 build-script targets and 48 proc-macro packages. Those executable build-time dependencies remain part of the trust surface.
- The website/gallery/CI is separate from a native Cargo dependency. Its maintenance scripts fetch showcase sources, npm assets and tool binaries, and `scripts/check.sh` intentionally resolves the newest GPUI Kit. None was executed for this review. The website also returns Cloudflare network-error-reporting headers; that is not embedded Rust-library telemetry.
- `cargo-audit` is not installed; a full vulnerability/advisory scan and exhaustive dependency source audit were not performed.

The sandbox build had no network interface, hid the real user home and desktop session, mounted the registry/toolchain read-only and restricted writes to isolated audit source/target/temp paths. Dependency fetch was a separate locked, non-compiling step. This limits the review's own exposure; it does not certify the resulting program.

## GPUI upgrade difficulty and the original version choice

### Versions are different package families

Live crates.io metadata checked on 2026-10-20:

| Package | Current latest | Meshcraft/Ely usage |
| --- | --- | --- |
| `gpui` | `0.2.2` | Meshcraft pins `=0.2.2`; it is the latest published version of this package, published 2025-10-22 |
| `gpui-pre` | `0.3.9` | Ely pins `=0.3.8`, a newer packaged Zed snapshot, not `gpui = 0.3.8` |
| `gpui-component` | `0.8.0` | Meshcraft pins `=0.5.1`; the component library has newer releases |
| `gpui-kit` | `0.8.0` | Ely's compatibility guide names Kit `0.7.1`; newest Kit pins `gpui-pre =0.3.9` |

Sources: crates.io APIs for [gpui](https://crates.io/api/v1/crates/gpui), [gpui-pre](https://crates.io/api/v1/crates/gpui-pre), [gpui-component](https://crates.io/api/v1/crates/gpui-component), [gpui-kit](https://crates.io/api/v1/crates/gpui-kit), and the downloaded published `0.8.0` manifests. `gpui-pre 0.3.8` records Zed revision `279fe070bb389b79652e52065b2f001edcc0b11b` and an upstream Zed crate version of `0.2.2`; comparing the package numbers as a simple upgrade distance is misleading. Its crates.io owner is `huacnlee`, while `gpui` lists Zed maintainers/team ownership. This is an additional upstream/package publisher to pin and review.

The history shows `gpui =0.2.2` entering the native-surface proof in commit `c147be5`, with matching GPUI Component `0.5.1` subsequently used and carried into production. ADR-06 deliberately pins the proven stack; the component crate's published `0.5.1` manifest depends on `gpui 0.2.2`. No recorded comprehensive comparison against `gpui-pre` justified rejecting it. The defensible explanation is a matching proof-of-concept dependency set, not that `0.2.2` is the newest Zed source snapshot or that newer code was tested and found unsuitable.

Why not simply take the newest today? Ely's reviewed manifest requires **exactly `gpui-pre =0.3.8`**; newest Kit/Component `0.8.0` require **exactly `=0.3.9`**. One GPUI type family is necessary for shared widgets. Mixing the old `gpui-component 0.5.1` with Ely produces incompatible `App`, `Window`, entity and element types. Patching Ely to `0.3.9` may be feasible, but it is a separate compatibility exercise, not a safe blind version bump. A separate Cargo metadata probe with pinned Ely plus `gpui-kit =0.8.0` reproduced the resolver failure (`failed to select a version for gpui-pre`); no code was compiled for that probe. The specific upstream CI failure must not be attributed to this conflict without its logs.

### Concrete work and risk

The framework-only move is **moderate**, while replacing the component library is **substantial**. About 4,746 lines currently sit in `gpui_app.rs`, `gpui_layout.rs` and `gpui_settings.rs`; this is the review/adapter surface, not a predicted rewrite-line count. `assets.rs` also uses GPUI images. Raylib shaders, rendering algorithms, offscreen readback, model/commands, persistence and most session tests are outside the widget migration.

Known changes from API/source comparison:

- Application creation changes to `gpui_platform::application()` and explicit platform backend configuration.
- Focus calls gain app context (`window.focus(&focus, cx)`), so focus and menu/editor ownership must be adapted and tested.
- Ely uses its own `FocusScope`, theme/asset initialization and button conventions instead of `gpui_component::Root` and the current component traits.
- Retained Input/NumberInput/Select/Slider state and subscriptions need adapters: e.g. Ely `TextInput` emits `Changed/Submit/Focus/Blur`; select/slider controls use different state/value APIs. Preserve draft validation and undo boundaries.
- New menu/palette models must bind our command catalog without losing disabled commands, labels, stable IDs, keyboard behavior or mouse-down activation.
- `RenderImage`, `ImageSource::Render` and `Window::drop_image` remain available in `gpui-pre 0.3.8`, so the offscreen canvas and explicit atlas-eviction fix have a viable API path. That is source compatibility evidence, not a measured GPU/runtime qualification.
- Ely's minimum Rust is 1.95; Meshcraft's declared minimum is already 1.97.1 and this host runs 1.99.1, so the current compiler is not the obstacle.

A sensible implementation sequence is several reviewable changesets: (1) freeze/vendor source and dependency closure; (2) port only the host/canvas and prove input, resize, frame lifetime and shutdown; (3) replace core controls/settings behind adapters; (4) replace menus/palette while preserving the full layout; (5) qualify the workflows and memory/latency before cutover. Add the script editor/tours/material UI in their own feature tasks afterward. A reliable calendar estimate requires the first running host/canvas proof; this should not be treated as a one-commit cosmetic swap.

## Vendoring policy if adopted

1. Copy the reviewed Ely commit to a local path dependency, preserving MIT, font OFL and icon ISC notices. The native source/assets/manifests/license subset is about 10.1 MiB; the entire tracked tree is about 16.4 MiB. Do not import the website, gallery photos, deployment/CI or scanner tooling into the production dependency.
2. Freeze the **application root lockfile**, not merely Ely's own lockfile: dependency library lockfiles do not dictate a consumer's resolution. Save the resolved feature tree and package checksums as an SBOM/review artifact.
3. Vendor the complete supported-platform dependency closure, including GPUI snapshot/platform crates, build dependencies and proc macros. The reviewed Linux source resolution occupies about 264 MiB; other platforms/features add dependencies. Vendoring Ely alone does not prevent transitive dependency substitution.
4. Configure Cargo source replacement and build/test with `--frozen --offline`. No branch-tracking Git dependencies, automatic dependency updates, remote fonts/assets or package-manager lifecycle scripts in the release path.
5. Keep vendor changes as small named patches: privacy-safe logging, any required menu activation adapter, and any tested GPUI snapshot compatibility fix. Disable optional network/process/browser capabilities unless explicitly needed.
6. Accept future updates only as reviewed diffs against this baseline. Exact pins and checksums prevent unintended drift; they do not remove vulnerabilities or malicious behavior already present in a frozen version.
7. Before production approval, run a dependency advisory/license review and an application-level outbound-network trace, then qualify the full native UI and long-running viewport memory. An offline library build alone does not establish these properties.

## Local validation

- `cargo metadata --offline --locked --no-deps --format-version 1`: passed.
- `cargo fetch --locked --target x86_64-unknown-linux-gnu`: passed; dependency download only.
- `cargo metadata --locked --offline --filter-platform x86_64-unknown-linux-gnu --format-version 1`: passed.
- Sandboxed `cargo check --offline --locked --lib`, Rust 1.99.1, two build jobs, debug symbols disabled: passed in 2m22s.
- Sandboxed `cargo test --offline --locked --lib --features test-support -- --test-threads=2`: **passed: 2,150 tests, 0 failed, 0 ignored**, after 3m48s test compilation and 103.42s execution. The candidate checkout and pinned manifest/lock/license hashes remained unchanged.
- No Meshcraft production dependency change, native Ely UI launch, dense-model GPU test or runtime network trace has been performed. The next step, if selected, is a separate proof using the pinned source and the existing raylib viewport.
