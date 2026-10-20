# Meshcraft

Meshcraft is a **direct mesh modeler** for building and editing 3D geometry. Draw edges and faces, shape them with modeling tools, and assemble reusable objects whose instances can adapt independently through interactive hotspots.

![img.png](docs/img.png)

The desktop workspace combines a 3D viewport with menus, labeled tool buttons, a searchable command palette and settings panels. Meshcraft is written in Rust, with a GPUI interface and an offscreen raylib renderer integrated into the layout.

## Modeling tools

- **Draw geometry:** lines, polylines with straight and curved segments, rectangles, circles, planar meshes and 3D primitives.
- **Shape meshes:** extrusion and Push/Pull, sweeps, ribbons, revolve, offsets and outlines.
- **Transform and repeat:** move, rotate, scale, stretch, move a copy, rotate a copy and arrays.
- **Work precisely:** geometry and grid snapping, construction guides, workplanes, live geometry previews, and relative distance and coordinate feedback.
- **Control selection and display:** select individual elements, connected geometry or regions; inspect totals and selection counts; set visibility, editability and wireframe display by entity type.
- **Inspect your model:** orbit, pan and zoom in a shaded viewport with configurable lighting, shadows, antialiasing and model line widths.
- **Materials:** create and edit named RGBA materials, assign them to selections, and see their colors and transparency in the viewport. Optional texture paths, U/V sizes and shading types are retained for future rendering work.
- **Keep working:** save and load projects, open legacy `.octd` and `.k3d` designs, and undo or redo edits.

## Dynamic objects: prototypes and instances

An **object prototype** is a reusable subscene containing edges, faces, hotspots and other object instances. An **instance** places that prototype in your design. Multiple instances share the same definition while retaining their own placement and hotspot values.

Double-click a prototype in the object list to place an instance, with a wireframe preview before insertion. Double-click an instance to edit its prototype in place. During editing, the surrounding model stays visible and available for snapping, while selection and edits remain isolated to the prototype. Changes to the shared definition update its instances.

**Hotspots make instances interactive.** Inside a prototype, attach geometry to a hotspot and configure its operation, reference point, shape and color. After leaving prototype editing, drag the hotspot on an instance to move, stretch, scale, rotate or repeat its attached geometry. Each instance keeps its own values, so instances can take different forms while sharing the same base topology. Copying an instance preserves its hotspot values.

External scenes can also serve as referenced prototypes, allowing reusable designs to live in separate project files.

## Scripted prototypes

Prototypes can also define JavaScript `onCreate` and `onAnimationFrame` hooks while retaining graphically authored geometry. The object script editor provides a single monospace code editor, Ctrl+Space API help/completion, and an orbitable preview. Geometry helpers create lines, faces and primitives; generated hotspots change each instance independently. Copies retain instance properties, and Explode freezes the current generated geometry.

See the [SDK, example and runtime requirements](docs/rewrite/spec/scripted-objects.md). Native script execution currently requires JavaScriptCore; Linux is locally qualified, with Windows runtime packaging and macOS live qualification still outstanding.

## Planned features

Meshcraft is under active development. The following capabilities are planned; they are not yet complete in the current application:

- **MCP integration:** structured access to modeling commands and application state through the Model Context Protocol, for automation and assistant workflows.
- **JavaScript plugins:** extend the application through a JavaScriptCore runtime and a versioned application API.
- **Built-in architecture tools:** walls, slabs, openings, stairs and frames, alongside plumbing and ventilation tools.
- **Photorealistic rendering:** offline rendering for presentation images, complementing the interactive shaded viewport.
- **Annotations:** dimensions, leaders and text for documenting designs.

The [development dashboard](DASHBOARD.md) tracks implemented behavior, validation and remaining work. The [rewrite guide](docs/rewrite/README.md) links the detailed specifications, architecture and implementation plan.

## Run from source

With Rust and the native build dependencies installed:

```sh
env -u WAYLAND_DISPLAY cargo run -- --open examples/dynamic-object.octd
cargo test --workspace --locked
```

The command above starts the main GPUI application with an example dynamic object. On Linux, unsetting `WAYLAND_DISPLAY` selects the X11 path used for desktop validation.

## Builds and releases

The manual [release workflow](.github/workflows/distribution.yaml) targets Linux, Windows and macOS on x86_64 and ARM64. Select an existing version tag to run it; the tag must match the desktop app version. See [packaging instructions](packaging/README.md) for portable archives, installers, native dependencies and platform qualification status. MSIX signing is planned for a later stage.
