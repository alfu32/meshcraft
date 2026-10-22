# Object prototypes, instances and dynamic hotspots

This is the user-confirmed object workflow, supplied with the 19 captures below on 2026-10-20. It refines F04/F09, I06/I07 and M26/M27; acceptance remains C07/C08/C15/C16. Source behavior is implemented in k3d's `GroupScene.kt`, `HotspotStore.kt`, `ObjectPlaceTool.kt` and the object/hotspot panel callbacks. Preserve source characterization separately from implementation verification.

## Prototypes and instances

A prototype is a reusable subscene: it owns lines, faces, hotspots, semantic entities and a hierarchy of other object instances, using the same scene capabilities as the root. An instance references that prototype and owns its placement plus per-instance hotspot values/overrides. Copying an instance preserves its current hotspot values; it does not turn the copy into a separate prototype. All instances continue to share definition edits while retaining their independent parameter values.

The **Objects** browser lists prototypes, once each, with name, internal/external status, occurrence count and identity. It must not list one row per instance. The separate **Object** inspector describes the selected instance or the prototype being edited in place. Screenshot 01 shows the current instance-list deficiency; screenshots 02–19 are legacy workflow evidence.

Double-clicking a prototype row activates insertion of that definition, even when no existing instance is selected. The cursor shows the instance's wireframe placement feedback and uses ordinary point-input snapping and surface-glue rules. Accepted placement creates an instance in the active scene/subscene. Double-clicking a placed instance enters its shared prototype for editing in place.

Import/prototype insertion prepares its wireframe once per source revision/context. Pointer
motion only updates the placement transform; it must not clone, insert or validate a temporary
scene on every move. The native renderer retains the uploaded wireframe between moves, including
when surface alignment changes. Clicking commits the complete geometry once; Escape releases
the preview without creating an instance.

## Editing isolation and navigation

Within prototype editing, selection and modification are limited to the active subscene's own geometry and immediate nested instances. Outside geometry stays visible, optionally faded, and remains a snapping/constraint reference. It is not selectable or modifiable. A nested instance can be entered to edit its definition in turn. Construction writes directly into the active prototype.

Pointer motion, snapping and individual picks must reuse the prepared prototype editing view,
rather than cloning the scene for every query. Rendering and spatial queries share that immutable
view until authored/evaluated geometry, document generation or the edit context changes.

Escape exits the active object-edit context and cancels its current draft; a dedicated Close Object/Exit Editing button provides the same one-handed action. Outside object editing, retain the established staged Escape behavior. Exit clears edit-context selection and restores instance-level interaction. The inspector must clearly distinguish selected instance, edited prototype and root scene.

## Hotspot authoring and instance behavior

Hotspots are scene elements and may be authored either in a prototype or directly at the root. A definition hotspot owns a stable ID, name, local anchor, operation, shape, RGBA color, optional reference point, attached geometry and optional attached hotspots. Supported operations are MOVE, SCALE, STRETCH, ROTATE, MULTIPLY_LINEAR, MULTIPLY_VOLUMETRIC, MULTIPLY_ROTATE_2D and MULTIPLY_ROTATE_3D. Shapes are TRIANGLE_UP, SQUARE, CIRCLE, DIAMOND and TRIANGLE_DOWN.

The author can place/select a hotspot, edit its properties, attach a mesh/entity selection, select attached geometry, pick or clear its reference, and delete it. Attach Selection uses the current supported geometry selection without requiring an object-only selection. Vertex/edge/face identity and connected geometry matter for stretch behavior. The reference is expressed in the owner's local frame and transformed for world-space snapping/input. Screenshot 10 displays STRETCH with a reference set; the user's description also cites move-type point input. Do not infer an operation from the screenshot caption alone.

At instance level, prototype hotspots appear as handles with their configured shapes/colors. Dragging one invokes its configured operator on its attached content and previews both geometry and measurements. Each instance owns its hotspot targets independently. Recompute from immutable prototype content and that instance's values; repeated motion must not accumulate deformation. A geometry drag commits one undoable change; cancellation restores the initial values/geometry. Prototype geometry/bindings must remain unchanged by a runtime instance drag. Copy/array operations preserve the source instance's values. Editing the shared prototype propagates to all users without flattening their parameter state.

## External prototypes

A prototype may reference an external scene. That scene's root is used as a subscene, including its root hotspots and nested instances. Placement, instance-local values, copy and editing/isolation semantics remain explicit. Preserve source path/identity, missing-file diagnostics and a deliberate refresh workflow; loading or refreshing an external scene must not silently discard local instance values or recurse indefinitely through cyclic references.

Instance hotspot operations run with prototype editing **closed**. Editing mode authors the shared handle definitions: dragging repositions the marker without applying its operator; dragging a closed-instance handle changes only that instance’s values.

## Screenshot evidence

All original captures are retained. Captures 15/16 and 17/18/19 show repeated states and are not independent validation runs.

### Capture 01 — Current Meshcraft Objects panel (instance list to replace with prototype browser)

![Current Meshcraft Objects panel (instance list to replace with prototype browser)](../references/objects/01-current-list.png)

### Capture 02 — Legacy selected-instance inspector and prototype browser

![Legacy selected-instance inspector and prototype browser](../references/objects/02-selected-instance.png)

### Capture 03 — Legacy in-place prototype edit context

![Legacy in-place prototype edit context](../references/objects/03-editing-prototype.png)

### Capture 04 — Selection confined to prototype geometry

![Selection confined to prototype geometry](../references/objects/04-prototype-selection.png)

### Capture 05 — Hotspot authored at a prototype corner

![Hotspot authored at a prototype corner](../references/objects/05-added-hotspot.png)

### Capture 06 — Window selection of the hotspot

![Window selection of the hotspot](../references/objects/06-hotspot-window-selection.png)

### Capture 07 — Hotspot operation choices

![Hotspot operation choices](../references/objects/07-hotspot-operation.png)

### Capture 08 — Hotspot shape choices

![Hotspot shape choices](../references/objects/08-hotspot-shape.png)

### Capture 09 — RGBA hotspot color editor

![RGBA hotspot color editor](../references/objects/09-hotspot-color.png)

### Capture 10 — Hotspot reference set (STRETCH shown)

![Hotspot reference set (STRETCH shown)](../references/objects/10-hotspot-reference.png)

### Capture 11 — Instance-level square and circular handles after leaving edit mode

![Instance-level square and circular handles after leaving edit mode](../references/objects/11-instance-handles.png)

### Capture 12 — Wireframe array preview carrying instance handles

![Wireframe array preview carrying instance handles](../references/objects/12-instance-array-preview.png)

### Capture 13 — Copied/arrayed instances with hotspot values

![Copied/arrayed instances with hotspot values](../references/objects/13-copied-instance-values.png)

### Capture 14 — Attaching the selected face region to a hotspot

![Attaching the selected face region to a hotspot](../references/objects/14-attach-face-selection.png)

### Capture 15 — Runtime hotspot drag preview and attached-content count

![Runtime hotspot drag preview and attached-content count](../references/objects/15-hotspot-drag-preview.png)

### Capture 16 — Repeated capture of runtime hotspot drag preview

![Repeated capture of runtime hotspot drag preview](../references/objects/16-hotspot-drag-preview-repeat.png)

### Capture 17 — Independent deformed instance shapes sharing one prototype

![Independent deformed instance shapes sharing one prototype](../references/objects/17-independent-instance-results.png)

### Capture 18 — Repeated independent-instance result capture

![Repeated independent-instance result capture](../references/objects/18-independent-instance-results-repeat.png)

### Capture 19 — Repeated independent-instance result capture

![Repeated independent-instance result capture](../references/objects/19-independent-instance-results-repeat-2.png)


## Meshcraft implementation evidence (2026-10-20)

The following are live Meshcraft GPUI captures, separate from the user-provided legacy evidence above. The offscreen raylib viewport remains an ordinary GPUI image. These checks used Linux X11/Xvfb with software Vulkan; they are interaction checks, not hardware frame-rate qualification. See [the validation record](../handoffs/object-workflow-validation.md) for exact automated commands and remaining acceptance scope.

![Closed-instance drag: relative feedback and unchanged second instance](../references/objects/20-gpui-instance-drag.png)

![Prototype editing: shared shape and color controls, faded surroundings and Finish Editing](../references/objects/21-gpui-hotspot-authoring.png)

![Prototype browser double-click: wireframe insertion with no selected source occurrence](../references/objects/22-gpui-prototype-insertion.png)

![Saved and reopened independent shapes, after one-click Finish Editing](../references/objects/23-gpui-save-reopen-finish.png)

Scripted configuration points augment this workflow; see [scripted objects](scripted-objects.md). Materials use the [project catalog](materials.md).

### Batch inspector properties — 2026-10-22

Hotspot metadata fields (name, operation, shape and color) apply to all selected
hotspot definitions that can be authored in the active context. Preserve every
hotspot's own position, attached geometry and reference point. Validate and undo
multi-field/multi-hotspot edits as one transaction. This does not change the
per-instance operator values or the rule that instance operations happen outside
prototype editing.

The object-name inspector renames each distinct selected object prototype once,
in one undoable edit. While editing a prototype it continues to name the active
prototype. Unselected prototypes retain their names; shared definitions retain
shared naming semantics.
