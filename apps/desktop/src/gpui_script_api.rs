//! Local API help and completion; no language server or network service is required.
use super::*;
use gpui::EntityInputHandler;

pub(super) struct Popup {
    pub selected: usize,
    x: f32,
    y: f32,
    scroll: gpui::ScrollHandle,
}
use crate::script_api::{Entry, catalog, entry};
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
            .top(px(popup.y - layout::TITLE_BAR_HEIGHT))
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
