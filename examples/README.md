# Dynamic object workflow

`dynamic-object.octd` is an authored Meshcraft example, not legacy fixture evidence. It contains two instances of **Dynamic Part**, a shared prototype with a square Stretch handle and a green Move handle.

```sh
env -u WAYLAND_DISPLAY cargo run -- --open examples/dynamic-object.octd
```

- Drag either handle with prototype editing closed. Only that instance changes; Undo restores the previous value.
- Double-click the prototype in **Objects** to place another instance with a wireframe preview.
- Double-click an instance to edit the shared prototype. Surroundings stay visible and supply snap references; selection is isolated.
- Inside prototype editing, dragging a hotspot moves its shared marker without operating on geometry. Select a hotspot to edit its operation, shape, RGBA color, attachments and reference in **Hotspot Settings**. Select geometry, then **Attach**; **Select Attached** recalls it.
- **Finish Editing** or Escape returns to instance operations. Copying an instance preserves its current handle values.

**Objects → Import Scene as Prototype** creates an internal snapshot. **Reference External Scene** stores the source path and cached subscene; **Refresh External Prototype** reloads it deliberately. Missing files leave the cached scene intact.
