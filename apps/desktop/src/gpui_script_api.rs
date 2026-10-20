//! Local API help and completion; no language server or network service is required.
use super::*;
use gpui::EntityInputHandler;

pub(super) struct Popup {
    pub selected: usize,
    x: f32,
    y: f32,
    scroll: gpui::ScrollHandle,
}
#[derive(Clone)]
struct Entry {
    name: String,
    signature: String,
    insert: String,
    help: String,
}
fn entry(name: &str, signature: &str, insert: &str, help: &str) -> Entry {
    Entry {
        name: name.into(),
        signature: signature.into(),
        insert: insert.into(),
        help: help.into(),
    }
}
fn catalog() -> Vec<Entry> {
    vec![
        entry(
            "Point",
            "new Point(x=0, y=0, z=0, id='')",
            "new Point(0, 0, 0)",
            "Local model-unit coordinates. An optional ID keeps the vertex identity stable as it moves.",
        ),
        entry(
            "Color",
            "new Color(red=255, green=255, blue=255, alpha=255)",
            "new Color(255, 255, 255, 255)",
            "RGBA channels from 0 to 255. Colors also accept #RRGGBB and #RRGGBBAA strings.",
        ),
        entry(
            "Material",
            "new Material(color, texturePath=null)",
            "new Material('#83bdeaff')",
            "Fields: id, name, color, shadingType, texturePath, textureScaleU, textureScaleV. Initial rendering uses color/alpha.",
        ),
        entry(
            "phong",
            "phong(color) → Material",
            "phong('#83bdeaff')",
            "Create a material with Phong shading metadata. Color and alpha are rendered in this phase.",
        ),
        entry(
            "gouraud",
            "gouraud(color) → Material",
            "gouraud('#83bdeaff')",
            "Create a material with Gouraud shading metadata. Optional texture settings remain metadata.",
        ),
        entry(
            "Segment",
            "new Segment(p0, p1, id, material?)",
            "new Segment(new Point(), new Point(1, 0, 0), this.id + ':edge')",
            "A line segment with a nonempty stable ID. An absent material uses a dark line color.",
        ),
        entry(
            "line",
            "line(p0, p1, id, material?) → Segment",
            "line(new Point(), new Point(1, 0, 0), this.id + ':edge')",
            "Same as new Segment. Point arguments have x/y/z coordinates. Each segment ID must be unique in the emitted array.",
        ),
        entry(
            "Face",
            "new Face(a, b, c, id, material?)",
            "new Face(new Point(), new Point(1, 0, 0), new Point(0, 1, 0), this.id + ':face')",
            "A triangle in local coordinates. Vertex winding controls its normal. Its ID must be unique.",
        ),
        entry(
            "face",
            "face(a, b, c, id, material?) → Face",
            "face(new Point(), new Point(1, 0, 0), new Point(0, 1, 0), this.id + ':face')",
            "Same as new Face. Return this drawable or a nested array containing it from onAnimationFrame.",
        ),
        entry(
            "hotspot",
            "hotspot(position, id) → Hotspot",
            "hotspot(new Point(3, 0, 0), 'width')",
            "Return the handle with your geometry. Its position contains this instance's dragged override; use position.x/y/z to parameterize geometry.",
        ),
        entry(
            "polyline",
            "polyline(points, id, material?, closed=false) → Segment[]",
            "polyline([new Point(), new Point(1, 0, 0)], 'path')",
            "Connect consecutive points; closed=true adds the final edge. Child IDs are prefixed by id.",
        ),
        entry(
            "circle",
            "circle(center, radius, id, material?, segments=32) → Segment[]",
            "circle(new Point(), 1, 'circle')",
            "A closed polygonal circle on the local XZ plane. Radius is in model units; segments are clamped to 3–512.",
        ),
        entry(
            "cube",
            "cube(origin, size, id, material?) → Drawable[]",
            "cube(new Point(), new Point(3, 2, 1), 'body', phong('#83bdeaff'))",
            "Create triangular faces and boundary edges. Size accepts a Point or one scalar for all axes.",
        ),
        entry(
            "sphere",
            "sphere(center, radius, id, material?, segments=24) → Face[]",
            "sphere(new Point(), 1, 'sphere')",
            "A triangulated sphere. Radius is in model units; segments are clamped to 4–128.",
        ),
        entry(
            "cone",
            "cone(origin, radius, height, id, material?, segments=32) → Face[]",
            "cone(new Point(), 1, 2, 'cone')",
            "A closed triangulated cone along local +Y. Radius/height are model-unit distances.",
        ),
        entry(
            "this",
            "this / instance → current object instance",
            "this.",
            "Both names refer to the same object. Assign your own JSON properties in onCreate; they persist per instance. No host mutation methods are exposed.",
        ),
        entry(
            "this.id",
            "this.id: string",
            "this.id",
            "The current occurrence ID, supplied by the host. Useful as a prefix for emitted entity IDs.",
        ),
        entry(
            "this.geometry",
            "this.geometry: Drawable[]",
            "this.geometry",
            "Graphically authored local geometry. Return it, spread it, or combine it with generated output. This does not overwrite the authored prototype.",
        ),
        entry(
            "this.hotspots",
            "this.hotspots: Record<string, Point>",
            "this.hotspots",
            "Per-instance dragged position overrides. hotspot(position, id) supplies defaults when the map has no value for id.",
        ),
        entry(
            "this.time",
            "this.time: number (seconds)",
            "this.time",
            "Elapsed script runtime time, also available as model.time. Use for animation.",
        ),
        entry(
            "this.deltaTime",
            "this.deltaTime: number (seconds)",
            "this.deltaTime",
            "Elapsed time since the previous frame request, also available as model.deltaTime.",
        ),
        entry(
            "instance",
            "instance: same object as this",
            "instance.",
            "The first hook argument aliases this. Custom JSON properties belong to one instance and are preserved by copy and project save.",
        ),
        entry(
            "model",
            "model: immutable query facade",
            "model.",
            "Fields: objects, materials, settings, time, deltaTime. Methods: getObject, findObjects, getMaterial, queryGeometry. No document writes, filesystem, networking, Node or DOM.",
        ),
        entry(
            "model.objects",
            "model.objects: readonly ObjectRecord[]",
            "model.objects",
            "Each record has id, name, definitionId, transform and geometry. Geometry is local; transform places it in the scene.",
        ),
        entry(
            "model.materials",
            "model.materials: readonly Material[]",
            "model.materials",
            "Named project materials: id, name, color (red/green/blue/alpha), shadingType, texturePath, textureScaleU/V.",
        ),
        entry(
            "model.settings",
            "model.settings: readonly ModelSettings",
            "model.settings",
            "Fields use snake_case: unit_name, unit_size, grid_size, circle_segments, snap_radius, arc_max_length, double_line_size and double_line_offset.",
        ),
        entry(
            "model.time",
            "model.time: number (seconds)",
            "model.time",
            "Elapsed runtime clock for this frame. It is not a model-unit distance.",
        ),
        entry(
            "model.deltaTime",
            "model.deltaTime: number (seconds)",
            "model.deltaTime",
            "Time since the previous frame request. Suitable for updating this instance's own animation properties.",
        ),
        entry(
            "model.getObject",
            "model.getObject(id) → ObjectRecord | undefined",
            "model.getObject(this.id)",
            "Read an occurrence by its ID. Record fields: id, name, definitionId, transform and geometry.",
        ),
        entry(
            "model.findObjects",
            "model.findObjects(name) → ObjectRecord[]",
            "model.findObjects('Object')",
            "Find occurrences by exact prototype name. Results are frozen model snapshots.",
        ),
        entry(
            "model.getMaterial",
            "model.getMaterial(id) → Material | undefined",
            "model.getMaterial('default')",
            "Read a catalog material by stable ID. Use its color or pass the value to a geometry helper.",
        ),
        entry(
            "model.queryGeometry",
            "model.queryGeometry(id) → readonly Drawable[]",
            "model.queryGeometry(this.id)",
            "Read an occurrence's current local drawable array. Apply its transform for world coordinates. Mutating query results is prohibited.",
        ),
        entry(
            "onCreate",
            "function onCreate(instance, model) → void",
            "function onCreate(instance, model) {\n  this.width = 3;\n}",
            "Initialize additional instance properties. It runs once for the current initializer, not on every animation frame.",
        ),
        entry(
            "onAnimationFrame",
            "function onAnimationFrame(instance, model) → Drawable[]",
            "function onAnimationFrame(instance, model) {\n  return this.geometry;\n}",
            "Return arrays or nested arrays of drawables. This hook is required. Geometry uses local coordinates; placement happens afterward.",
        ),
        entry(
            "Math.sin",
            "Math.sin(angleRadians) → number",
            "Math.sin(this.time)",
            "Standard JavaScript Math is available, together with ordinary ECMAScript arrays, objects, JSON and other built-ins.",
        ),
        entry(
            "Math.cos",
            "Math.cos(angleRadians) → number",
            "Math.cos(this.time)",
            "Standard JavaScript cosine; angles are in radians.",
        ),
        entry(
            "Math.PI",
            "Math.PI: number",
            "Math.PI",
            "Standard JavaScript constant π. Other ECMAScript Math functions are available.",
        ),
    ]
}
fn prefix(text: &str, cursor: usize) -> (usize, &str) {
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let start = text[..cursor]
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        .last()
        .map_or(cursor, |(i, _)| i);
    (start, &text[start..cursor])
}
fn entries(editor: &Editor, cx: &Context<MeshcraftShell>) -> Vec<Entry> {
    let mut entries = catalog();
    if let Some(frame) = editor
        .document
        .scripted_occurrences()
        .iter()
        .find_map(|p| editor.document.script_frame(p))
    {
        for (name, value) in &frame.state.properties {
            let access = if name.chars().enumerate().all(|(i, c)| {
                c == '_' || c == '$' || c.is_alphabetic() || (i > 0 && c.is_numeric())
            }) {
                format!("this.{name}")
            } else {
                format!("this[{}]", serde_json::to_string(name).unwrap_or_default())
            };
            if entries.iter().any(|e| e.name == access) {
                continue;
            }
            let kind = match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
            };
            let sample = value.to_string();
            entries.push(entry(
                &access,
                &format!("{access}: {kind}"),
                &access,
                &format!(
                    "Your persisted instance property. Preview value: {}",
                    sample.chars().take(120).collect::<String>()
                ),
            ));
        }
    }
    let input = editor.code.read(cx);
    let text = input.value();
    let (_, filter) = prefix(&text, input.cursor());
    if filter.starts_with("instance.") {
        for item in &mut entries {
            if item.name.starts_with("this.") {
                item.name = item.name.replacen("this.", "instance.", 1);
                item.signature = item.signature.replacen("this.", "instance.", 1);
                item.insert = item.insert.replacen("this.", "instance.", 1);
            }
        }
    }
    let filter = filter.to_lowercase();
    entries.retain(|e| e.name.to_lowercase().starts_with(&filter));
    entries
}
pub(super) fn open(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let Some(editor) = shell.scripts.editor.as_mut() else {
        return;
    };
    let mut bounds = editor.bounds.lock().map(|b| *b).unwrap_or_default();
    bounds.origin += gpui::point(px(12.), px(8.));
    let anchor = editor.code.update(cx, |input, cx| {
        let value = input.value();
        let cursor = input.cursor().min(value.len());
        let utf16 = value[..cursor].encode_utf16().count();
        input
            .bounds_for_range(utf16..utf16, bounds, window, cx)
            .unwrap_or(bounds)
    });
    let width = f32::from(window.viewport_size().width);
    let height = f32::from(window.viewport_size().height);
    let x = f32::from(anchor.origin.x)
        .min((width - 610.).max(16.))
        .max(16.);
    let below = f32::from(anchor.origin.y + anchor.size.height) + 6.;
    let y = if below + 340. < height {
        below
    } else {
        (f32::from(anchor.origin.y) - 340.).max(16.)
    };
    editor.completion = Some(Popup {
        selected: 0,
        x,
        y,
        scroll: gpui::ScrollHandle::new(),
    });
    editor.code.update(cx, |input, cx| input.focus(window, cx));
    cx.notify();
}
fn insert(
    shell: &mut MeshcraftShell,
    index: usize,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let Some(editor) = shell.scripts.editor.as_mut() else {
        return;
    };
    let Some(item) = entries(editor, cx).get(index).cloned() else {
        return;
    };
    editor.completion = None;
    editor.code.update(cx, |input, cx| {
        let value = input.value();
        let cursor = input.cursor().min(value.len());
        let (start, _) = prefix(&value, cursor);
        let text = if value[..start].trim_end().ends_with("new") {
            item.insert.strip_prefix("new ").unwrap_or(&item.insert)
        } else {
            &item.insert
        };
        let range = value[..start].encode_utf16().count()..value[..cursor].encode_utf16().count();
        input.replace_text_in_range(Some(range), text, window, cx);
        input.focus(window, cx);
    });
    cx.notify();
}
pub(super) fn capture_key(
    shell: &mut MeshcraftShell,
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    let Some(_) = shell.scripts.editor.as_ref() else {
        return false;
    };
    if event.keystroke.modifiers.control && matches!(event.keystroke.key.as_str(), "space" | " ") {
        open(shell, window, cx);
        return true;
    }
    navigate(shell, event.keystroke.key.as_str(), window, cx)
}
pub(super) fn navigate(
    shell: &mut MeshcraftShell,
    key: &str,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    let Some(editor) = shell.scripts.editor.as_ref() else {
        return false;
    };
    if editor.completion.is_none() {
        return false;
    }
    let count = entries(editor, cx).len();
    match key {
        "escape" => {
            if let Some(e) = shell.scripts.editor.as_mut() {
                e.completion = None;
            }
            cx.notify();
        }
        "enter" | "return" | "tab" => {
            let index = editor.completion.as_ref().map_or(0, |p| p.selected);
            insert(shell, index, window, cx);
        }
        "up" | "down" => {
            if let Some(popup) = shell
                .scripts
                .editor
                .as_mut()
                .and_then(|e| e.completion.as_mut())
            {
                popup.selected = if key == "up" {
                    popup.selected.saturating_sub(1)
                } else {
                    (popup.selected + 1).min(count.saturating_sub(1))
                };
                popup.scroll.scroll_to_top_of_item(popup.selected);
            }
            cx.notify();
        }
        _ => return false,
    }
    true
}
pub(super) fn render(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let editor = shell.scripts.editor.as_ref()?;
    let popup = editor.completion.as_ref()?;
    let entries = entries(editor, cx);
    let selected = popup.selected.min(entries.len().saturating_sub(1));
    let font = cx.global::<Theme>().mono_font_family.clone();
    Some(
        div()
            .id("script/api-popup")
            .absolute()
            .left(px(popup.x))
            .top(px(popup.y))
            .w(px(600.))
            .max_h(px(332.))
            .occlude()
            .p(px(8.))
            .bg(rgb(0x18212d))
            .border_1()
            .border_color(rgb(0x708da8))
            .rounded_md()
            .shadow_lg()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if let Some(e) = this.scripts.editor.as_mut() {
                    e.completion = None;
                }
                cx.notify();
            }))
            .child(
                div().text_xs().mb(px(6.)).child(
                    "Script API · type to filter · ↑/↓ select · Enter/Tab insert · Esc dismiss",
                ),
            )
            .child(
                div()
                    .id("script/api-items")
                    .max_h(px(286.))
                    .overflow_y_scroll()
                    .track_scroll(&popup.scroll)
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .when(entries.is_empty(), |list| {
                        list.child(div().p(px(8.)).child(
                            "No matching API. Ctrl+Space on an empty line shows the full list.",
                        ))
                    })
                    .children(entries.into_iter().enumerate().map(|(index, item)| {
                        div()
                            .id(gpui::ElementId::Name(format!("script/api/{index}").into()))
                            .p(px(8.))
                            .rounded_md()
                            .bg(rgb(if index == selected {
                                0x334e67
                            } else {
                                0x202c3b
                            }))
                            .cursor_pointer()
                            .child(
                                div()
                                    .font_family(font.clone())
                                    .text_sm()
                                    .child(item.signature),
                            )
                            .child(
                                div()
                                    .mt(px(4.))
                                    .text_xs()
                                    .text_color(rgb(0xc0cbd8))
                                    .child(item.help),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    insert(this, index, window, cx);
                                    cx.stop_propagation();
                                }),
                            )
                    })),
            )
            .into_any_element(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_prefixes_preserve_unicode_and_dotted_accessors() {
        assert_eq!((9, "model.ge"), prefix("const π=model.ge", 17));
        assert_eq!((7, "this."), prefix("return this.", 12));
        assert_eq!((0, ""), prefix("", 0));
        let apis = catalog();
        for name in [
            "this.id",
            "this.geometry",
            "this.hotspots",
            "model.objects",
            "model.settings",
            "model.queryGeometry",
            "Point",
            "cube",
            "hotspot",
        ] {
            assert!(apis.iter().any(|e| e.name == name), "{name}");
        }
    }
}
