//! Retained GPUI component state; settings commits go through the headless command API.
use super::*;
use gpui_component::{
    Colorize,
    input::{Input, InputEvent, InputState, NumberInput, NumberInputEvent, StepAction},
    select::{Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
};

#[derive(Default)]
pub(super) struct Editors {
    inputs: BTreeMap<&'static str, Entity<InputState>>,
    selects: BTreeMap<&'static str, Entity<SelectState<Vec<String>>>>,
    sliders: BTreeMap<&'static str, Entity<SliderState>>,
    errors: BTreeMap<&'static str, String>,
    subscriptions: Vec<gpui::Subscription>,
    pub active: Option<&'static str>,
    hotspot_color: Option<Entity<gpui_component::color_picker::ColorPickerState>>,
}
#[derive(Clone, Copy)]
struct Field {
    label: &'static str,
    group: &'static str,
    key: &'static str,
    step: f64,
    min: f64,
    max: f64,
    integer: bool,
}
macro_rules! field {
    ($label:literal,$group:literal,$key:literal,$step:expr,$min:expr,$max:expr) => {
        Field {
            label: $label,
            group: $group,
            key: $key,
            step: $step,
            min: $min,
            max: $max,
            integer: false,
        }
    };
}
const FIELDS: &[Field] = &[
    field!("Architecture name", "arch", "architecture_name", 0., 0., 0.),
    field!(
        "Wall thickness",
        "arch",
        "wall_thickness",
        0.05,
        0.000001,
        1e6
    ),
    field!("Wall height", "arch", "wall_height", 0.1, 0.000001, 1e6),
    field!(
        "Wall inclination",
        "arch",
        "wall_inclination",
        1.,
        -84.9,
        84.9
    ),
    field!(
        "Slab thickness",
        "arch",
        "slab_thickness",
        0.05,
        0.000001,
        1e6
    ),
    field!("Frame depth", "arch", "frame_depth", 0.01, 0.000001, 1e6),
    field!("Frame width", "arch", "frame_width", 0.01, 0.000001, 1e6),
    field!("Unit name", "model", "unit_name", 0., 0., 0.),
    field!("Unit size", "model", "unit_size", 0.1, 0.000001, 1.0e12),
    field!("Grid size", "model", "grid_size", 0.1, 0.000001, 1.0e9),
    Field {
        integer: true,
        ..field!("Circle segments", "model", "circle_segments", 1., 3., 256.)
    },
    field!("Snap radius", "model", "snap_radius", 1., 0., 128.),
    field!(
        "Arc max length",
        "model",
        "arc_max_length",
        0.05,
        0.000001,
        1.0e9
    ),
    field!(
        "Double line size",
        "model",
        "double_line_size",
        0.1,
        0.000001,
        1.0e9
    ),
    field!(
        "Double line offset",
        "model",
        "double_line_offset",
        0.1,
        -1.0e9,
        1.0e9
    ),
    field!("Walk speed", "ui", "walk_move_speed", 1., 0.1, 1000.),
    field!("Jump velocity", "ui", "walk_jump_velocity", 1., 0.1, 1000.),
    field!("Gravity", "ui", "walk_gravity", 1., 0.1, 10000.),
    field!(
        "Height adjust speed",
        "ui",
        "walk_height_adjust_speed",
        1.,
        0.1,
        1000.
    ),
    field!("UI text size", "ui", "ui_text_scale", 0.1, 0.5, 2.),
    Field {
        integer: true,
        ..field!("Grid extent", "ui", "grid_extent", 1., 1., 1000.)
    },
    field!(
        "Normal line width",
        "ui",
        "normal_grid_width",
        0.25,
        0.25,
        4.
    ),
    field!("Thick line width", "ui", "thick_grid_width", 0.25, 0.5, 8.),
    field!("Ambient light", "light", "ambient", 0.05, 0., 1.),
    field!("Directional light", "light", "directional", 0.05, 0., 1.),
    field!("Shadow alpha", "light", "shadow_strength", 0.05, 0., 1.),
    field!("Specular value", "light", "specular", 0.05, 0., 1.),
    field!("Shadow bias", "light", "depth_bias", 0.00005, 0., 0.01),
    field!("Normal bias", "light", "normal_bias", 0.001, 0., 0.1),
    field!("Light azimuth", "light", "azimuth", 5., -180., 180.),
    field!("Light elevation", "light", "elevation", 5., 5., 85.),
];
fn current(shell: &MeshcraftShell, field: Field) -> String {
    let payload = match field.group {
        "arch" => shell.session.native_architecture_payload(),
        "model" => model_settings_payload(shell.cached_document.model_settings()),
        "ui" => shell.session.native_ui_preferences_payload(),
        _ => {
            let light = shell.lighting;
            let value = match field.key {
                "ambient" => light.ambient,
                "directional" => light.directional,
                "shadow_strength" => light.shadow_strength,
                "specular" => light.specular,
                "depth_bias" => light.depth_bias,
                "normal_bias" => light.normal_bias,
                "azimuth" => light.azimuth,
                "elevation" => light.elevation,
                _ => 0.,
            };
            return format_number(f64::from(value));
        }
    };
    let value = payload
        .values
        .get(if field.key == "architecture_name" {
            "name"
        } else {
            field.key
        })
        .cloned()
        .unwrap_or(Value::Null);
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.as_f64().map(format_number).unwrap_or_default())
}
fn format_number(value: f64) -> String {
    format!("{value:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
fn parsed(field: Field, text: &str) -> Result<Value, String> {
    if matches!(field.key, "unit_name" | "architecture_name") {
        return if text.trim().is_empty() {
            Err("Unit name is required".into())
        } else {
            Ok(json!(text.trim()))
        };
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| "Enter a valid number".to_owned())?;
    if !value.is_finite()
        || value < field.min
        || value > field.max
        || (field.integer && value.fract() != 0.)
    {
        return Err(format!(
            "Use {} to {}{}",
            field.min,
            field.max,
            if field.integer {
                " (whole numbers)"
            } else {
                ""
            }
        ));
    }
    Ok(if field.integer {
        json!(value as i64)
    } else {
        json!(value)
    })
}
fn commit(shell: &mut MeshcraftShell, field: Field, text: &str) {
    let value = match parsed(field, text) {
        Ok(v) => v,
        Err(e) => {
            shell.settings.errors.insert(field.key, e);
            return;
        }
    };
    shell.settings.errors.remove(field.key);
    if text == current(shell, field) {
        return;
    }
    if field.group == "arch" {
        let mut payload = CommandPayload::default();
        payload.values.insert(
            if field.key == "architecture_name" {
                "name"
            } else {
                field.key
            }
            .into(),
            value,
        );
        let selected = shell.session.active_tool_label() == "Select"
            && shell.session.native_selected_architecture().is_some();
        shell.dispatch_payload(
            if selected {
                "architecture.update"
            } else {
                "architecture.settings"
            },
            &payload,
        );
    } else if field.group == "light" {
        let v = value.as_f64().unwrap() as f32;
        match field.key {
            "ambient" => shell.lighting.ambient = v,
            "directional" => shell.lighting.directional = v,
            "shadow_strength" => shell.lighting.shadow_strength = v,
            "specular" => shell.lighting.specular = v,
            "depth_bias" => shell.lighting.depth_bias = v,
            "normal_bias" => shell.lighting.normal_bias = v,
            "azimuth" => shell.lighting.azimuth = v,
            "elevation" => shell.lighting.elevation = v,
            _ => {}
        }
        shell.refresh_scene();
    } else {
        let mut payload = if field.group == "model" {
            model_settings_payload(shell.cached_document.model_settings())
        } else {
            shell.session.native_ui_preferences_payload()
        };
        payload.values.insert(field.key.into(), value);
        shell.dispatch_payload(
            if field.group == "model" {
                "model.update_settings"
            } else {
                "ui.update_preferences"
            },
            &payload,
        );
    }
}
pub(super) fn apply_all(
    shell: &mut MeshcraftShell,
    _: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let mut payload = CommandPayload::default();
    let mut valid = true;
    for &field in FIELDS.iter().filter(|f| f.group == "model") {
        let Some(input) = shell.settings.inputs.get(field.key) else {
            continue;
        };
        match parsed(field, input.read(cx).value().as_ref()) {
            Ok(value) => {
                payload.values.insert(field.key.to_owned(), value);
                shell.settings.errors.remove(field.key);
            }
            Err(error) => {
                shell.settings.errors.insert(field.key, error);
                valid = false;
            }
        }
    }
    if valid {
        shell.dispatch_payload("model.update_settings", &payload);
    }
}
pub(super) fn reset_model(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    shell.dispatch_payload(
        "model.update_settings",
        &model_settings_payload(&Default::default()),
    );
    shell.settings.active = None;
    for &field in FIELDS.iter().filter(|f| f.group == "model") {
        shell.settings.errors.remove(field.key);
        if let Some(input) = shell.settings.inputs.get(field.key).cloned() {
            let value = current(shell, field);
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }
    window.focus(&shell.focus);
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    prepare_objects(shell, window, cx);
    for &field in FIELDS {
        let value = current(shell, field);
        if let Some(input) = shell.settings.inputs.get(field.key).cloned() {
            if shell.settings.active != Some(field.key)
                && !shell.settings.errors.contains_key(field.key)
                && input.read(cx).value().as_ref() != value
            {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            continue;
        }
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
        shell.settings.subscriptions.push(cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Focus => this.settings.active = Some(field.key),
                    InputEvent::Change => {
                        this.settings.errors.remove(field.key);
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        let text = input.read(cx).value().to_string();
                        commit(this, field, &text);
                        this.settings.active = None;
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            window.focus(&this.focus);
                        }
                    }
                }
                cx.notify();
            },
        ));
        if !matches!(field.key, "unit_name" | "architecture_name") {
            shell.settings.subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, input, event: &NumberInputEvent, window, cx| {
                    let NumberInputEvent::Step(action) = event;
                    let old = input.read(cx).value().parse::<f64>().unwrap_or(field.min);
                    let next = (old
                        + if *action == StepAction::Increment {
                            field.step
                        } else {
                            -field.step
                        })
                    .clamp(field.min, field.max);
                    let text = format_number(next);
                    commit(this, field, &text);
                    input.update(cx, |input, cx| input.set_value(text, window, cx));
                    cx.notify();
                },
            ));
        }
        shell.settings.inputs.insert(field.key, input);
    }
    let preferences = shell.session.native_ui_preferences();
    let axis = match preferences.grid_normal_axis {
        GridNormalAxis::X => "X (right)",
        GridNormalAxis::Y => "Y (up, default)",
        GridNormalAxis::Z => "Z (back)",
    };
    for (key, items, selected) in [
        (
            "grid_normal_axis",
            vec!["Y (up, default)", "X (right)", "Z (back)"],
            axis.to_owned(),
        ),
        (
            "toolbar_icon_size",
            vec!["24 × 24", "32 × 32", "40 × 40", "48 × 48"],
            format!(
                "{} × {}",
                preferences.toolbar_icon_size, preferences.toolbar_icon_size
            ),
        ),
        (
            "antialiasing",
            vec!["Off", "Low", "Medium", "High"],
            super::postprocess::Antialiasing::from_level(preferences.antialiasing_quality)
                .label()
                .to_owned(),
        ),
    ] {
        if let Some(select) = shell.settings.selects.get(key).cloned() {
            if select.read(cx).selected_value() != Some(&selected) {
                select.update(cx, |select, cx| {
                    select.set_selected_value(&selected, window, cx)
                });
            }
            continue;
        }
        let index = items.iter().position(|s| *s == selected).unwrap_or(0);
        let select = cx.new(|cx| {
            SelectState::new(
                items.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                Some(gpui_component::IndexPath::default().row(index)),
                window,
                cx,
            )
        });
        shell.settings.subscriptions.push(cx.subscribe_in(
            &select,
            window,
            move |this, _, event: &SelectEvent<Vec<String>>, _, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                match key {
                    "grid_normal_axis" => {
                        this.update_ui_preference(key, json!(&value[..1].to_ascii_lowercase()))
                    }
                    "toolbar_icon_size" => this.update_ui_preference(
                        key,
                        json!(
                            value
                                .split_whitespace()
                                .next()
                                .unwrap()
                                .parse::<u32>()
                                .unwrap()
                        ),
                    ),
                    "antialiasing" => {
                        this.update_ui_preference(
                            "antialiasing_quality",
                            json!(super::postprocess::Antialiasing::from_label(value).level()),
                        );
                    }
                    _ => {}
                }
                cx.notify();
            },
        ));
        shell.settings.selects.insert(key, select);
    }
    for (key, value, min, max) in [
        ("normal_grid_width", preferences.normal_grid_width, 0.25, 4.),
        ("thick_grid_width", preferences.thick_grid_width, 0.5, 8.),
    ] {
        if let Some(slider) = shell.settings.sliders.get(key).cloned() {
            if slider.read(cx).value() != SliderValue::Single(value) {
                slider.update(cx, |state, cx| state.set_value(value, window, cx));
            }
            continue;
        }
        let slider = cx.new(|_| {
            SliderState::new()
                .min(min)
                .max(max)
                .step(0.25)
                .default_value(value)
        });
        shell.settings.subscriptions.push(cx.subscribe(
            &slider,
            move |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(SliderValue::Single(value)) = event else {
                    return;
                };
                this.update_ui_preference(key, json!(value));
                cx.notify();
            },
        ));
        shell.settings.sliders.insert(key, slider);
    }
}
pub(super) fn render_field(
    shell: &MeshcraftShell,
    label: &'static str,
) -> Option<gpui::AnyElement> {
    if let Some(editor) = render_object_field(shell, label) {
        return Some(editor);
    }
    let key = match label {
        "Permanent grid normal" => Some("grid_normal_axis"),
        "Toolbar size" => Some("toolbar_icon_size"),
        "Antialiasing" => Some("antialiasing"),
        _ => None,
    };
    let body = if let Some(key) = key {
        Select::new(shell.settings.selects.get(key)?)
            .small()
            .w_full()
            .into_any_element()
    } else {
        let field = FIELDS.iter().find(|f| f.label == label)?;
        let input = shell.settings.inputs.get(field.key)?;
        let editor = if matches!(field.key, "unit_name" | "architecture_name") {
            Input::new(input).small().w_full().into_any_element()
        } else {
            NumberInput::new(input).small().w_full().into_any_element()
        };
        let mut row = div().flex().items_center().gap(px(6.)).min_w(px(0.));
        if let Some(slider) = shell.settings.sliders.get(field.key) {
            row = row
                .child(Slider::new(slider).w_full())
                .child(div().w(px(84.)).flex_none().child(editor));
        } else {
            row = row.child(editor);
        }
        let mut body = div().child(row);
        if let Some(error) = shell.settings.errors.get(field.key) {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(rgb(0xf28b82))
                    .child(error.clone()),
            );
        }
        body.into_any_element()
    };
    Some(
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .gap(px(3.))
            .child(div().text_xs().text_color(rgb(0xc2c8d0)).child(label))
            .child(body)
            .into_any_element(),
    )
}

fn prepare_objects(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    use gpui_component::color_picker::{ColorPickerEvent, ColorPickerState};
    use meshcraft_core::document::hotspots::{Operation, Shape};
    let (_, h) = shell.session.native_hotspot_inspector();
    let color = h.color.rgba();
    let hex = format!(
        "{:02x}{:02x}{:02x}{:02x}",
        color.red, color.green, color.blue, color.alpha
    );
    let object_name = shell
        .session
        .native_active_object()
        .map(|(_, name, _)| name)
        .unwrap_or_default();
    for (key, value) in [
        ("object_name", object_name),
        ("hotspot_name", h.name.clone()),
        ("hotspot_color", hex.clone()),
    ] {
        if let Some(input) = shell.settings.inputs.get(key).cloned() {
            if shell.settings.active != Some(key) && input.read(cx).value().as_ref() != value {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            continue;
        }
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
        shell.settings.subscriptions.push(cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Focus => this.settings.active = Some(key),
                    InputEvent::PressEnter { .. } | InputEvent::Blur
                        if this.settings.active == Some(key) =>
                    {
                        let value = input.read(cx).value().to_string();
                        let result = if key == "object_name" {
                            this.session
                                .rename_native_selected_object(&value)
                                .map(|_| ())
                        } else {
                            this.session.update_native_hotspot_field(
                                if key == "hotspot_name" {
                                    "name"
                                } else {
                                    "color"
                                },
                                &value,
                            )
                        };
                        if let Err(error) = result {
                            this.session.set_native_status(error);
                        }
                        this.settings.active = None;
                        this.refresh_scene();
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            window.focus(&this.focus);
                        }
                    }
                    _ => {}
                }
                cx.notify();
            },
        ));
        shell.settings.inputs.insert(key, input);
    }
    for (key, options, selected) in [
        (
            "hotspot_operation",
            Operation::ALL
                .into_iter()
                .map(|x| x.name().to_owned())
                .collect::<Vec<_>>(),
            h.operation.name().to_owned(),
        ),
        (
            "hotspot_shape",
            Shape::ALL
                .into_iter()
                .map(|x| x.name().to_owned())
                .collect::<Vec<_>>(),
            h.shape.name().to_owned(),
        ),
    ] {
        if let Some(select) = shell.settings.selects.get(key).cloned() {
            if select.read(cx).selected_value() != Some(&selected) {
                select.update(cx, |state, cx| {
                    state.set_selected_value(&selected, window, cx)
                });
            }
            continue;
        }
        let index = options
            .iter()
            .position(|item| item == &selected)
            .unwrap_or(0);
        let select = cx.new(|cx| {
            SelectState::new(
                options,
                Some(gpui_component::IndexPath::default().row(index)),
                window,
                cx,
            )
        });
        shell.settings.subscriptions.push(cx.subscribe(
            &select,
            move |this, _, event: &SelectEvent<Vec<String>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    if let Err(error) = this.session.update_native_hotspot_field(
                        if key == "hotspot_operation" {
                            "operation"
                        } else {
                            "shape"
                        },
                        value,
                    ) {
                        this.session.set_native_status(error);
                    }
                    this.refresh_scene();
                    cx.notify();
                }
            },
        ));
        shell.settings.selects.insert(key, select);
    }
    let hsla = gpui::Hsla::parse_hex(&format!("#{hex}")).unwrap_or_else(|_| rgb(0x338cf2).into());
    if let Some(picker) = shell.settings.hotspot_color.clone() {
        if picker.read(cx).value() != Some(hsla) {
            picker.update(cx, |picker, cx| picker.set_value(hsla, window, cx));
        }
    } else {
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla));
        shell.settings.subscriptions.push(cx.subscribe(
            &picker,
            |this, _, event: &ColorPickerEvent, cx| {
                if let ColorPickerEvent::Change(Some(value)) = event {
                    let color: gpui::Rgba = (*value).into();
                    let hex = format!(
                        "{:02x}{:02x}{:02x}{:02x}",
                        (color.r * 255.).round() as u8,
                        (color.g * 255.).round() as u8,
                        (color.b * 255.).round() as u8,
                        (color.a * 255.).round() as u8
                    );
                    if let Err(error) = this.session.update_native_hotspot_field("color", &hex) {
                        this.session.set_native_status(error);
                    }
                    this.refresh_scene();
                    cx.notify();
                }
            },
        ));
        shell.settings.hotspot_color = Some(picker);
    }
}

fn render_object_field(shell: &MeshcraftShell, label: &'static str) -> Option<gpui::AnyElement> {
    use gpui_component::color_picker::ColorPicker;
    let key = match label {
        "Object name" => "object_name",
        "Hotspot name" => "hotspot_name",
        "Hotspot color" => "hotspot_color",
        "Operation" => "hotspot_operation",
        "Shape" => "hotspot_shape",
        _ => return None,
    };
    let (selected, _) = shell.session.native_hotspot_inspector();
    let allowed = selected
        .as_ref()
        .is_none_or(|(path, _)| shell.session.native_hotspot_authoring_allowed(path));
    let body = if key == "hotspot_operation" || key == "hotspot_shape" {
        Select::new(shell.settings.selects.get(key)?)
            .small()
            .w_full()
            .disabled(!allowed)
            .into_any_element()
    } else {
        let input = Input::new(shell.settings.inputs.get(key)?)
            .small()
            .w_full()
            .disabled(if key == "object_name" {
                shell.session.native_active_object().is_none()
            } else {
                !allowed
            });
        let mut row = div().flex().gap(px(5.)).child(input);
        if key == "hotspot_color"
            && allowed
            && let Some(picker) = shell.settings.hotspot_color.as_ref()
        {
            row = row.child(ColorPicker::new(picker).small());
        }
        row.into_any_element()
    };
    Some(
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .gap(px(3.))
            .child(div().text_xs().text_color(rgb(0xc2c8d0)).child(label))
            .child(body)
            .into_any_element(),
    )
}

pub(super) fn focus_object_name(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    if let Some(input) = shell.settings.inputs.get("object_name").cloned() {
        input.update(cx, |input, cx| input.focus(window, cx));
        shell.settings.active = Some("object_name");
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_edits_never_become_command_payloads() {
        for (key, text) in [
            ("unit_name", ""),
            ("grid_size", "0"),
            ("unit_size", "nan"),
            ("circle_segments", "5.5"),
            ("snap_radius", "129"),
            ("normal_grid_width", "9"),
        ] {
            let field = *FIELDS.iter().find(|f| f.key == key).unwrap();
            assert!(parsed(field, text).is_err(), "{key}");
        }
        let field = *FIELDS.iter().find(|f| f.key == "circle_segments").unwrap();
        assert_eq!(parsed(field, "48").unwrap(), json!(48));
    }
}
