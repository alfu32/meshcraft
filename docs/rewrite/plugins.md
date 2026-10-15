# JavaScriptCore plugins and host context

## Delivery decision

Plugins are part of the intended final product. The user selected **WebKit JavaScriptCore** as the native plugin engine, with an application context handed to plugins. Plugin implementation remains after the initial desktop milestone. Groovy/JVM plugin execution, a developer REPL and the CLI/TUI are not reintroduced by this decision. Android remains deferred. Browser delivery and native plugin delivery are separate successors to the initial desktop release.

Design the context/identity/command boundaries now; implement and package the runtime in T47–T50. This avoids tying plugins to a redundant object model or forcing the first-phase app to depend on JavaScriptCore. The runtime engine direction is chosen; its Rust binding, supported versions, binaries and platform packaging still need proof.

## Runtime evidence and platform proof

The [official JavaScriptCore C context API](https://github.com/WebKit/WebKit/blob/main/Source/JavaScriptCore/API/JSContextRef.h) supplies global contexts and context groups. The [official object API](https://github.com/WebKit/WebKit/blob/main/Source/JavaScriptCore/API/JSObjectRef.h) provides native-backed classes/functions and deferred-promise creation. These support embedding a host context; the Meshcraft API below is our proposal, not an existing WebKit facility.

Native callback values retained beyond a call need lifetime management, including balanced protection/unprotection under the [JavaScriptCore value API](https://github.com/WebKit/WebKit/blob/main/Source/JavaScriptCore/API/JSValueRef.h). Keep all access to a plugin's engine state on its owning runtime executor; marshal model work to the application owner. Never send JS value handles or Rust model references into arbitrary worker threads.

[WebKit's port documentation](https://docs.webkit.org/Ports/Introduction.html) identifies Apple, Linux and JSCOnly ports and separate port releases. Its [Windows documentation](https://docs.webkit.org/Ports/WindowsPort.html) describes a distinct Windows build. T47 must prove a distributable JSC embedding on Linux/Windows/macOS and record linking, redistribution artifacts, minimum versions and update ownership. Do not assume an available Rust crate makes those packages equivalent or that a full WebKit browser is required. Sources reviewed for this revision; recheck versions during implementation.

First plugin delivery targets the native desktop runtime. A native JavaScriptCore library is not assumed to run inside the browser WASM application. Browser plugin execution/availability requires an explicit later adapter and capability policy; keep the SDK free of accidental DOM/Node dependencies to preserve that option. Do not silently substitute a different native engine when a platform proof is difficult.

## Plugin package and lifecycle

Use a local manifest with stable plugin ID, name/version, supported host API range, entry bundle and declared host capabilities. First delivery supports local installation; a remote marketplace/package manager is not a prerequisite. Source JS can be authored in any language that compiles to the supported bundle format, but the shipped host must not require Node or a TypeScript compiler.

Start with a prebundled script and an explicit activation entry receiving `ctx`. A module loader, Node `require`, package resolution, browser globals and network access do not arise automatically from embedding JSC. T47 selects/document the bundle format and promise/microtask integration using the actual supported C API and host scheduler.

Lifecycle: discover/validate manifest → create plugin-owned context → construct capability-scoped facade → evaluate entry → activate → register contributions → dispatch events/tool interactions → deactivate/dispose → unregister/cancel → release engine roots/context. Activation failure must roll back registrations. Reload must not duplicate commands/listeners or leave stale callbacks callable. Context separation is an execution boundary, not a claim of OS process isolation.

## Proposed `PluginContext` v1

The facade is versioned and plugin-scoped. Query results are immutable value DTOs tagged with document identity/revision. Stable IDs and `OccurrenceRef` identify content; JS never holds a raw pointer, Rust borrow, mutable store or legacy GroupNode wrapper. Definitions and placed occurrences remain distinct even when the UI uses the word “object.”

| Context member | Contract |
| --- | --- |
| `ctx.apiVersion`, `ctx.plugin`, `ctx.capabilities` | Negotiated API version, plugin metadata and available/declared host services |
| `ctx.document.read(query)` | Paged/targeted authored or effective-content queries with IDs, occurrence paths, source attribution and revision; no per-frame full-model copy |
| `ctx.selection.read()` | Session selection with explicit definition/entity/occurrence identity; not a shared mutable selection set on geometry |
| `ctx.commands.register(descriptor)` | Namespaced commands with parameter schema, label/menu/palette metadata, enabled/checked state and callback; returns a disposable registration |
| `ctx.commands.execute(id, args)` | Invokes the existing host dispatcher asynchronously; uses its completion, selection and undo policies |
| `ctx.document.apply(batch)` | Validated typed operation batch with expected document/revision and label; one atomic core transaction/undo entry on success |
| `ctx.tools.register(descriptor)` | Plugin-local tool state receives normalized events and submits preview DTOs/typed edits; shared focus/cancel/operator protocols apply |
| `ctx.ui.registerPanel(descriptor)` | Declarative supported widgets/actions backed by host UI; no native widget pointers or a second menu/undo implementation |
| `ctx.interchange.register(descriptor)` | Explicit import/export capability using byte/value DTOs and host operations; codecs do not mutate core stores directly |
| `ctx.events.subscribe(kind, callback)` | Revisioned/coalesced document/selection/tool notifications; disposable, ordered and bounded by host lifecycle |
| `ctx.state` | Plugin settings plus explicitly namespaced document state with plugin schema version; validated serializable values only |
| `ctx.log` | Plugin-tagged diagnostics visible in the host; no interactive developer console required |
| Optional host services | User-selected file reads/writes and other declared services through host effects; absent services return an explicit unsupported result |

This is an API contract proposal to ratify, not a promise of existing method names. Reserve a plugin namespace and prevent registration collisions. Built-in operations do not need to run through the JSC host; plugins call the same services those operations use.

## Mutation, identity and asynchronous behavior

An example transaction payload has `documentId`, `baseRevision`, `label`, and ordered typed `operations`; edits target stable canonical IDs or source semantic elements, with occurrence paths for per-instance edits. Batch contents are validated against the capability and current document before application. A stale revision yields a conflict/result the plugin can requery; it does not silently apply a selection from a different model or hold an edit lock across arbitrary awaited JS.

Read authored data when changing a definition; read resolved data when inspecting a placed instance. Resolved face/vertex handles are generation/revision-scoped and carry source attribution. Generated domain triangles cannot be passed off as authored entity IDs; the plugin edits domain parameters or explicitly explodes/materializes first. This prevents a second public object model from reproducing the legacy base/override ambiguity.

Queries/callbacks operate on bounded DTOs. Commands and batches use the existing dispatcher/transactions and return actual completion results. JS work runs away from the render/UI loop; asynchronous host replies are delivered on the runtime executor. Define event ordering and reentrancy so an event callback cannot recursively publish half a transaction. T47 proves promise scheduling and interruption/teardown behavior before claiming a blocking callback is cancellable. If reliable interruption cannot be provided, record/select a process boundary while retaining JavaScriptCore; do not treat “runs on a worker” as cancellation.

Enabled/checked UI state is supplied through host-readable state updates or bounded queries, not arbitrary unbounded JS execution every UI frame. Plugin-generated previews are transient and never create history by themselves. Clear them on cancel, document close and unload. Exceptions produce plugin-scoped diagnostics; failed commands leave the model unchanged. Lifecycle callbacks cannot release handles that remain in flight without host ownership checks.

## Persistence and unavailable plugins

Canonical document data has one extension owner per plugin ID plus plugin schema version. A plugin's private settings belong to application preferences, not model geometry. Persist only serializable authored extension values; JS functions/heap objects, ctx wrappers, promise state and caches are not saved.

Model data already created as native geometry remains editable without the plugin. Unknown plugin-owned values round-trip untouched when the plugin is absent. Fully custom plugin entity types/generators need a later explicit schema, source attribution and fallback-geometry protocol; do not invent it implicitly as an arbitrary JS object in core storage. Installing/enabling a plugin does not execute embedded model-file scripts automatically.

Groovy/JAR plugins need a documented source/API port to JavaScript. There is no direct binary, Java class identity or arbitrary Groovy binding compatibility. Preserve sample metadata for migration, but do not add a JVM bridge. Existing built-in voxel/architecture/HVAC/mechanical modules remain native and available independently.

## Agent work and qualification

T47 proves JSC binding/packaging/runtime scheduling. T48 builds lifecycle/context/GC ownership. T49 connects query/identity/command/tool/UI/state/interchange services. T50 qualifies sample plugins, manager UI, docs and the native desktop packages. These are required later-product tasks, not a vague optional backlog. They depend on T44 desktop qualification and the early T46 object-model decision, and do not block T45 browser delivery.

C33 covers: a command that edits selected geometry through one undoable transaction; a two-pick tool with a preview/cancel path; declarative panel/state persistence; an export adapter; namespaced registrations; API-range/capability errors; selection/definition/occurrence distinctions; stale-revision rejection; garbage collection while callbacks are retained; activation failure/unload/reload; pending async completion after close; large targeted queries; unknown plugin state with the plugin absent; packaged JSC loading on all supported desktop OSes. A full REPL/TUI is not part of this gate.
