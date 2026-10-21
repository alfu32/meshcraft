//! Native architecture properties and color drafts; profile controls are in the canvas.
use super::*;
use gpui_component::Colorize;
use gpui_component::{
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
};
use meshcraft_core::document::architecture::{Color as SurfaceColor, Element};

#[derive(Default)]
pub(super) struct Ui {
    key: Option<(u64, String)>,
    colors: BTreeMap<&'static str, Entity<ColorPickerState>>,
    dirty: bool,
    subscriptions: Vec<gpui::Subscription>,
}
const COLORS: &[(&str, &str)] = &[
    ("wall_exterior_color", "Exterior"),
    ("wall_interior_color", "Interior"),
    ("slab_top_color", "Top"),
    ("slab_bottom_color", "Bottom"),
    ("slab_side_color", "Sides"),
    ("frame_color", "Frame"),
    ("glazing_color", "Glazing"),
];
fn inspected(
    shell: &MeshcraftShell,
) -> Option<meshcraft_core::document::architecture::PlacedElement> {
    shell
        .session
        .native_selected_architecture()
        .filter(|_| shell.session.active_tool_label() == "Select")
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let selected = inspected(shell);
    let key = (
        shell.session.document_generation(),
        selected
            .as_ref()
            .map(|e| format!("{:?}", e.target))
            .unwrap_or_else(|| format!("defaults/{}", shell.session.active_tool_label())),
    );
    if shell.architecture.key.as_ref() != Some(&key) {
        shell.architecture.key = Some(key);
        shell.architecture.dirty = false;
    }
    let values = shell.session.native_architecture_payload();
    for &(field, _) in COLORS {
        let Some(color) = values
            .values
            .get(field)
            .and_then(|v| serde_json::from_value::<SurfaceColor>(v.clone()).ok())
        else {
            continue;
        };
        let c = color.rgba();
        let hex = format!("#{:02x}{:02x}{:02x}{:02x}", c.red, c.green, c.blue, c.alpha);
        let hsla = gpui::Hsla::parse_hex(&hex).unwrap_or_default();
        if let Some(picker) = shell.architecture.colors.get(field).cloned() {
            if !shell.architecture.dirty && picker.read(cx).value() != Some(hsla) {
                picker.update(cx, |p, cx| p.set_value(hsla, window, cx));
            }
        } else {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla));
            shell.architecture.subscriptions.push(cx.subscribe(
                &picker,
                |this, _, event: &ColorPickerEvent, cx| {
                    if matches!(event, ColorPickerEvent::Change(_)) {
                        this.architecture.dirty = true;
                        cx.notify();
                    }
                },
            ));
            shell.architecture.colors.insert(field, picker);
        }
    }
}
fn apply_colors(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    let selected = inspected(shell);
    let prefix = selected.as_ref().map(|e| match e.element {
        Element::Wall(_) => "wall_",
        Element::Slab(_) => "slab_",
        Element::Frame(_) => "frame_",
    });
    let mut payload = CommandPayload::default();
    for &(key, _) in COLORS {
        if prefix.is_some_and(|p| !key.starts_with(p) && !(p == "frame_" && key == "glazing_color"))
        {
            continue;
        }
        if let Some(color) = shell
            .architecture
            .colors
            .get(key)
            .and_then(|p| p.read(cx).value())
        {
            let c: gpui::Rgba = color.into();
            payload
                .values
                .insert(key.into(), json!({"r":c.r,"g":c.g,"b":c.b,"a":c.a}));
        }
    }
    shell.dispatch_payload(
        if selected.is_some() {
            "architecture.update"
        } else {
            "architecture.settings"
        },
        &payload,
    );
    shell.architecture.dirty = false;
}
pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected = inspected(shell);
    let mut body = div().flex().flex_col().gap(px(7.));
    if let Some(e) = &selected {
        body = body.child(format!("Selected {}", e.element.name()));
        if !shell.session.native_architecture_editable(&e.target) {
            return body.child("Open the object's prototype to edit these properties, or unlock its type in Selection.").into_any_element();
        }
        if let Some(field) = settings::render_field(shell, "Architecture name") {
            body = body.child(field);
        }
    } else {
        body = body.child("Defaults for new architecture");
    }
    let wall = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Wall(_)));
    let slab = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Slab(_)));
    let frame = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Frame(_)));
    for (show, labels) in [
        (
            wall,
            &["Wall thickness", "Wall height", "Wall inclination"][..],
        ),
        (slab, &["Slab thickness"][..]),
        (frame, &["Frame depth", "Frame width"][..]),
    ] {
        if show {
            for label in labels {
                if let Some(field) = settings::render_field(shell, label) {
                    body = body.child(field);
                }
            }
        }
    }
    for &(key, label) in COLORS {
        let show = if key.starts_with("wall_") {
            wall
        } else if key.starts_with("slab_") {
            slab
        } else {
            frame
        };
        if show && let Some(picker) = shell.architecture.colors.get(key) {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label)
                    .child(ColorPicker::new(picker).small()),
            );
        }
    }
    if frame {
        let enabled = shell
            .session
            .native_architecture_payload()
            .values
            .get("glazing_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        body = body.child(
            Checkbox::new("architecture/glazing")
                .label("Glazing")
                .checked(enabled)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                    let mut payload = CommandPayload::default();
                    payload
                        .values
                        .insert("glazing_enabled".into(), json!(*enabled));
                    this.dispatch_payload(
                        if inspected(this).is_some() {
                            "architecture.update"
                        } else {
                            "architecture.settings"
                        },
                        &payload,
                    );
                    cx.notify();
                })),
        );
    }
    body = body.child(
        Button::new("architecture/apply-colors")
            .small()
            .label("Apply Colors")
            .disabled(!shell.architecture.dirty)
            .on_click(cx.listener(|this, _, _, cx| {
                apply_colors(this, cx);
                cx.notify();
            })),
    );
    if let Some(e) = &selected {
        for hole in e.element.holes() {
            let target = e.target.clone();
            let id = hole.id.clone();
            let name = if hole.name.is_empty() {
                "Opening"
            } else {
                &hole.name
            };
            body = body.child(
                Button::new(SharedString::from(format!("architecture/hole/{id}")))
                    .small()
                    .label(format!("Delete {name}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let mut payload = CommandPayload::default();
                        if let SelectedEntity::SemanticHandle { id, .. } = &target.entity {
                            payload.values.insert("id".into(), json!(id));
                        }
                        payload
                            .values
                            .insert("occurrence".into(), json!(target.occurrence));
                        payload.values.insert("hole_id".into(), json!(id));
                        this.dispatch_payload("architecture.remove_hole", &payload);
                        cx.notify();
                    })),
            );
        }
        if matches!(e.element, Element::Slab(_)) {
            body=body.child(div().text_xs().child("Drag the slab's blue profile controls in the canvas. Holes remain owned by this slab."));
        }
    }
    body.child(div().text_xs().text_color(rgb(0xa5adb7)).child("Add Hole cuts only its picked wall or slab. Windows and doors are independent objects and never cut an opening automatically.")).into_any_element()
}
