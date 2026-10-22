//! Object parameter forms are projections of typed model declarations.
use super::*;
use crate::app::parameter_workflow::ParameterRow;
use gpui_component::{
    checkbox::Checkbox,
    input::InputEvent,
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use meshcraft_core::document::scripts::parameters::{Kind, Scope};

#[derive(Clone)]
struct Choice {
    key: String,
    label: String,
}
impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.key
    }
}
struct Field {
    row: ParameterRow,
    input: Option<Entity<InputState>>,
    select: Option<Entity<SelectState<Vec<Choice>>>>,
    error: Option<String>,
    active: bool,
    _subscription: Option<gpui::Subscription>,
}
#[derive(Default)]
pub(super) struct Ui {
    key: String,
    fields: Vec<Field>,
}
fn display(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}
fn commit(shell: &mut MeshcraftShell, index: usize, value: Value) {
    let row = shell.parameters.fields[index].row.clone();
    let result = shell.session.set_native_script_parameter(
        row.definition,
        row.paths,
        &row.declaration.id,
        value,
    );
    shell.parameters.fields[index].error = result.err();
    shell.refresh_scene();
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let rows = shell.session.native_script_parameters();
    let key = format!(
        "{}:{:?}",
        shell.session.document_generation(),
        rows.iter()
            .map(|r| (&r.definition, &r.paths, &r.declaration))
            .collect::<Vec<_>>()
    );
    if shell.parameters.key != key {
        shell.parameters = Ui {
            key,
            fields: vec![],
        };
        for (index, row) in rows.iter().cloned().enumerate() {
            let mut field = Field {
                row: row.clone(),
                input: None,
                select: None,
                error: None,
                active: false,
                _subscription: None,
            };
            match row.declaration.kind {
                Kind::Text | Kind::Number => {
                    let binding_key = shell.parameters.key.clone();
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .default_value(display(row.value.as_ref()))
                            .placeholder("Mixed")
                    });
                    field._subscription = Some(cx.subscribe_in(
                        &input,
                        window,
                        move |this, input, event: &InputEvent, window, cx| {
                            if this.parameters.key != binding_key {
                                return;
                            }
                            match event {
                                InputEvent::Focus => {
                                    this.parameters.fields[index].active = true;
                                    this.settings.active = Some("script-parameter");
                                }
                                InputEvent::Change => this.parameters.fields[index].error = None,
                                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                                    let text = input.read(cx).value().to_string();
                                    let kind = this.parameters.fields[index].row.declaration.kind;
                                    let value = if kind == Kind::Number {
                                        text.parse::<f64>()
                                            .ok()
                                            .filter(|v| v.is_finite())
                                            .map(|v| json!(v))
                                    } else {
                                        Some(json!(text))
                                    };
                                    if let Some(value) = value {
                                        commit(this, index, value);
                                    } else {
                                        this.parameters.fields[index].error =
                                            Some("Enter a finite number".into());
                                    }
                                    this.parameters.fields[index].active = false;
                                    this.settings.active = None;
                                    if matches!(event, InputEvent::PressEnter { .. }) {
                                        window.focus(&this.focus);
                                    }
                                }
                            }
                            cx.notify();
                        },
                    ));
                    field.input = Some(input);
                }
                Kind::List => {
                    let binding_key = shell.parameters.key.clone();
                    let choices = row
                        .declaration
                        .options
                        .iter()
                        .map(|(key, label)| Choice {
                            key: key.clone(),
                            label: label.clone(),
                        })
                        .collect::<Vec<_>>();
                    let selected = row
                        .value
                        .as_ref()
                        .and_then(Value::as_str)
                        .and_then(|v| choices.iter().position(|c| c.key == v))
                        .map(|r| gpui_component::IndexPath::default().row(r));
                    let select = cx.new(|cx| SelectState::new(choices, selected, window, cx));
                    field._subscription = Some(cx.subscribe_in(
                        &select,
                        window,
                        move |this, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                            if this.parameters.key != binding_key {
                                return;
                            }
                            if let SelectEvent::Confirm(Some(value)) = event {
                                commit(this, index, json!(value));
                                cx.notify();
                            }
                        },
                    ));
                    field.select = Some(select);
                }
                _ => {}
            }
            shell.parameters.fields.push(field);
        }
    }
    for (field, row) in shell.parameters.fields.iter_mut().zip(rows) {
        if !field.active && field.error.is_none() {
            if let Some(input) = &field.input {
                let text = display(row.value.as_ref());
                if input.read(cx).value().as_ref() != text {
                    input.update(cx, |input, cx| input.set_value(text, window, cx));
                }
            }
            if let Some(select) = &field.select {
                let value = row
                    .value
                    .as_ref()
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if select.read(cx).selected_value() != value.as_ref() {
                    select.update(cx, |s, cx| {
                        s.set_selected_value(&value.unwrap_or_default(), window, cx)
                    });
                }
            }
        }
        field.row = row;
    }
}
pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let mut form = div().flex().flex_col().gap(px(5.));
    for scope in [Scope::Instance, Scope::Prototype] {
        if !shell
            .parameters
            .fields
            .iter()
            .any(|f| f.row.declaration.scope == scope)
        {
            continue;
        }
        form = form.child(div().mt(px(6.)).text_sm().text_color(rgb(0xef7edb)).child(
            if scope == Scope::Instance {
                "Instance parameters"
            } else {
                "Prototype parameters · shared"
            },
        ));
        for (index, field) in shell
            .parameters
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.row.declaration.scope == scope)
        {
            let d = &field.row.declaration;
            let binding_key = shell.parameters.key.clone();
            let value = if let Some(input) = &field.input {
                Input::new(input).small().into_any_element()
            } else if let Some(select) = &field.select {
                Select::new(select).small().into_any_element()
            } else if d.kind == Kind::Checkbox {
                Checkbox::new(SharedString::from(format!("script-param/{index}")))
                    .checked(
                        field
                            .row
                            .value
                            .as_ref()
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        if this.parameters.key != binding_key {
                            return;
                        }
                        commit(this, index, json!(checked));
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div()
                    .text_xs()
                    .child("Drag hotspot handle")
                    .into_any_element()
            };
            form = form.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(130.))
                            .flex_none()
                            .text_sm()
                            .child(d.label.clone()),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(value)),
            );
            if let Some(error) = &field.error {
                form = form.child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xff9999))
                        .child(error.clone()),
                );
            }
        }
    }
    form.into_any_element()
}
