# Scripted prototype configuration points

### P08 Scripted object geometry and editor

**New user requirement (2026-10-20).** Scripting augments every object prototype, including prototypes retaining graphically authored geometry and nested instances. It does not introduce a separate incompatible object type. Instances share source hooks but retain their own serializable properties and hotspot values. Copying an instance preserves these values.

Expose two configuration points initially:

- `onCreate(instance, model)`: create additional instance properties; `this` is bound to the same instance. Initialize once per instance for the current initializer, including an existing instance newly given a script. An explicit initializer-source change reinitializes affected instance properties; ordinary frames do not rerun it.
- `onAnimationFrame(instance, model)`: return an array of drawables, allowing nested arrays from geometry helpers. `this.geometry` exposes the object's graphically authored geometry; the default hook returns it. Model access is query-only. Stable user IDs identify points, segments, faces and hotspots across frames.

The JavaScript functions may be entered as complete functions or function bodies. User pseudocode with typed arguments is a contract description; editor examples use valid JavaScript. The model facade supplies materials, object/geometry queries and frame timing without exposing mutable Rust/model pointers, filesystem, network, Node or DOM services. The existing engine direction remains WebKit JavaScriptCore; [runtime/plugin boundaries](../plugins.md) still apply. Object configuration is now desktop scope independently of the full plugin manager/API.

Provide `Point(x,y,z,id)`, `Material(color[,texturePath])`, `Color`, `Segment(p0,p1,id[,material])`, `Face(p0,p1,p2,id[,material])`, plus `line`, `face`, `hotspot`, `phong` and `gouraud`. Geometry helpers include `polyline`, `circle`, `cube`, `sphere` and `cone`, returning arrays of entities. Document exact helper signatures with runnable examples. Geometry is emitted in prototype-local coordinates; instance placement applies afterward. Script-created material values use the same color/texture/shading schema as the project catalog.

Scripted hotspots appear on closed instances and drive regenerated geometry through their per-instance values. Stable IDs retain their values when the hook emits a new frame. Prototype editing isolates authored geometry as before; external geometry remains visible for snapping. Script output is derived and is not destructively written over the prototype's authored geometry. Explode replaces an instance with its current issued geometry, transformed into the containing scene, with no script dependency. Per-frame output does not create an unbounded undo history.

The mini IDE has a vertical accordion of code editors on the left (one for each hook), and an independent preview on the right. Preview supports orbit, pan and zoom. Save, Preview and Cancel remain visible. Draft evaluation and camera navigation do not modify the source model; Save commits the prototype configuration; Cancel/Escape discards it. Exceptions, invalid output and execution limits produce visible diagnostics and leave the document usable. Run JavaScript away from the UI/render loop, with bounded output, interruption and clean disposal. Preserve native renderer performance and avoid leaked runtime processes, contexts or popup windows. The preview uses the existing offscreen raylib worker, not a second conflicting raylib global context.

Source: new user explanation. Acceptance: C43. Implementation: T62; shared JSC embedding evidence also informs T47. Further optimization is a later iteration.
