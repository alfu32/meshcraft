//! Offline IFC property-set browser and transaction-backed prototype/instance editors.
use super::*;
use gpui_component::select::{Select, SelectEvent, SelectState};
use meshcraft_core::document::{
    DefinitionId, OccurrenceRef,
    ifc::{self, Kind, Property},
};
#[derive(Default)]
pub(super) struct Ui {
    pub editor: Option<Editor>,
}
pub(super) struct Editor {
    generation: u64,
    definition: DefinitionId,
    paths: Vec<OccurrenceRef>,
    old: Option<(String, String)>,
    set: Entity<InputState>,
    name: Entity<InputState>,
    data_type: Entity<InputState>,
    value: Entity<InputState>,
    unit: Entity<InputState>,
    description: Entity<InputState>,
    extra: Entity<InputState>,
    kind: Entity<SelectState<Vec<String>>>,
    catalog: Entity<SelectState<Vec<String>>>,
    error: Option<String>,
    _subscription: gpui::Subscription,
}
fn open(
    shell: &mut MeshcraftShell,
    prototype: bool,
    old: Option<(String, String, Property)>,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let Some((definition, paths, _)) = shell.session.native_ifc_selection(prototype) else {
        return;
    };
    let (set, name, p) = old.clone().unwrap_or_else(|| {
        (
            "Meshcraft_Custom".into(),
            "Reference".into(),
            Property::default(),
        )
    });
    let mut options = serde_json::to_value(&p).unwrap();
    let options = options.as_object_mut().unwrap();
    for key in ["kind", "dataType", "value", "unit", "description"] {
        options.remove(key);
    }
    let options = serde_json::to_string_pretty(&options).unwrap();
    let mut input = |text: String, cx: &mut Context<MeshcraftShell>| {
        cx.new(|cx| InputState::new(window, cx).default_value(text))
    };
    let set = input(set, cx);
    let name = input(name, cx);
    let data_type = input(p.data_type, cx);
    let unit = input(p.unit.unwrap_or_default(), cx);
    let description = input(p.description, cx);
    let extra = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .rows(2)
            .default_value(options)
    });

    let value = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .rows(3)
            .default_value(if p.value.is_null() {
                String::new()
            } else if let Some(v) = p.value.as_str() {
                v.into()
            } else {
                serde_json::to_string_pretty(&p.value).unwrap()
            })
    });
    let kinds = Kind::ALL
        .iter()
        .map(|k| {
            serde_json::to_value(k)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let index = Kind::ALL.iter().position(|k| *k == p.kind).unwrap();
    let kind = cx.new(|cx| {
        SelectState::new(
            kinds,
            Some(gpui_component::IndexPath::default().row(index)),
            window,
            cx,
        )
    });
    let catalog = cx.new(|cx| {
        SelectState::new(
            ifc::catalog()
                .properties
                .iter()
                .map(|p| format!("{} / {}", p.set, p.name))
                .collect::<Vec<_>>(),
            None,
            window,
            cx,
        )
        .searchable(true)
    });
    let subscription = cx.subscribe_in(
        &catalog,
        window,
        |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
            if let SelectEvent::Confirm(Some(selected)) = event
                && let Some(template) = ifc::catalog()
                    .properties
                    .iter()
                    .find(|p| format!("{} / {}", p.set, p.name) == *selected)
                && let Some(e) = this.ifc.editor.as_mut()
            {
                e.set
                    .update(cx, |i, cx| i.set_value(template.set.clone(), window, cx));
                e.name
                    .update(cx, |i, cx| i.set_value(template.name.clone(), window, cx));
                e.data_type.update(cx, |i, cx| {
                    i.set_value(template.data_type.clone(), window, cx)
                });
                let k = serde_json::to_value(template.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned();
                e.kind
                    .update(cx, |s, cx| s.set_selected_value(&k, window, cx));
                e.value.update(cx, |i, cx| i.set_value("", window, cx));
                let mut options = serde_json::Map::new();
                if let Some(t) = &template.secondary_data_type {
                    options.insert("secondaryDataType".into(), json!(t));
                }
                if !template.choices.is_empty() {
                    options.insert("enumeration".into(), json!(template.choices));
                }
                e.extra.update(cx, |i, cx| {
                    i.set_value(serde_json::to_string_pretty(&options).unwrap(), window, cx)
                });
                e.unit.update(cx, |i, cx| i.set_value("", window, cx));
                cx.notify();
            }
        },
    );
    shell.ifc.editor = Some(Editor {
        generation: shell.session.document_generation(),
        definition,
        paths,
        old: old.map(|(s, n, _)| (s, n)),
        set,
        name,
        data_type,
        value,
        unit,
        description,
        extra,
        kind,
        catalog,
        error: None,
        _subscription: subscription,
    });
    cx.notify();
}
pub(super) fn panel(
    shell: &MeshcraftShell,
    prototype: bool,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let Some((definition, paths, props)) = shell.session.native_ifc_selection(prototype) else {
        return div()
            .text_xs()
            .child(if prototype {
                "Select a prototype to edit its IFC defaults."
            } else {
                "Select an object instance to edit IFC properties."
            })
            .into_any_element();
    };
    let generation = shell.session.document_generation();
    let mut list = div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().mt(px(6.)).text_sm().child(if prototype {
            "IFC properties · defaults for new instances"
        } else {
            "IFC properties · instance"
        }));
    for (set, properties) in props {
        list = list.child(div().mt(px(4.)).text_xs().child(set.clone()));
        for (name, property) in properties {
            let old = (set.clone(), name.clone(), property.clone());
            let delete = (set.clone(), name.clone());
            let paths = paths.clone();
            let summary = if property.value.is_null() {
                "Unset".into()
            } else {
                property
                    .value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| property.value.to_string())
            };
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        Button::new(SharedString::from(format!("ifc/{prototype}/{set}/{name}")))
                            .ghost()
                            .small()
                            .justify_start()
                            .tooltip(format!("{set}.{name}: {summary}"))
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .w(px(layout::RIGHT_PANEL_WIDTH - 100.))
                                    .max_w_full()
                                    .min_w(px(0.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .text_xs()
                                            .truncate()
                                            .child(name.clone()),
                                    )
                                    .child(
                                        div()
                                            .w(px(100.))
                                            .flex_shrink_0()
                                            .text_xs()
                                            .text_right()
                                            .truncate()
                                            .child(summary),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                open(this, prototype, Some(old.clone()), window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "ifc/delete/{prototype}/{set}/{name}"
                        )))
                        .small()
                        .label("×")
                        .tooltip("Delete IFC property")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.session.document_generation() != generation {
                                return;
                            }
                            if let Err(error) = this.session.edit_native_ifc(
                                definition,
                                paths.clone(),
                                Some(delete.clone()),
                                delete.0.clone(),
                                delete.1.clone(),
                                None,
                            ) {
                                this.session.set_native_status(error);
                            }
                            this.refresh_scene();
                            cx.notify();
                        })),
                    ),
            );
        }
    }
    list = list.child(
        Button::new(if prototype {
            "ifc/add-prototype"
        } else {
            "ifc/add-instance"
        })
        .small()
        .label("Add IFC property")
        .on_click(cx.listener(move |this, _, window, cx| open(this, prototype, None, window, cx))),
    );
    let mut presets = div().flex().flex_wrap().gap(px(4.));
    for preset in ifc::presets::ALL {
        let paths = paths.clone();
        presets = presets.child(
            Button::new(SharedString::from(format!(
                "ifc/preset/{prototype}/{}",
                preset.id
            )))
            .small()
            .label(format!("{} · {}", preset.label, preset.fields.len()))
            .tooltip(format!(
                "Add {} fields from {}. Existing properties are preserved.",
                preset.fields.len(),
                preset.set
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.session.document_generation() != generation {
                    return;
                }
                if let Err(error) =
                    this.session
                        .apply_native_ifc_preset(definition, paths.clone(), preset.id)
                {
                    this.session.set_native_status(error);
                }
                this.refresh_scene();
                cx.notify();
            })),
        );
    }
    list.child(
        div()
            .mt(px(4.))
            .text_xs()
            .child("Add common properties · values stay unset"),
    )
    .child(presets)
    .into_any_element()
}
fn field(label: &'static str, input: &Entity<InputState>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(label)
        .child(Input::new(input).small())
        .into_any_element()
}
pub(super) fn overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let e = shell.ifc.editor.as_ref()?;
    let form = div()
        .w(px(640.))
        .max_h(px(900.))
        .id("ifc/dialog")
        .overflow_y_scroll()
        .p(px(18.))
        .bg(rgb(0x202832))
        .flex()
        .flex_col()
        .gap(px(9.))
        .child(div().text_lg().child("IFC property · IFC4X3_ADD2"))
        .child(div().text_xs().child(
            "Search 3,988 standard properties and quantities, or enter a custom set and name.",
        ))
        .child(Select::new(&e.catalog).placeholder("Search standard property…"))
        .child(field("Property set", &e.set))
        .child(field("Property name", &e.name))
        .child(Select::new(&e.kind))
        .child(field("IFC value type", &e.data_type))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child("Value · text, number, true/false or structured JSON")
                .child(Input::new(&e.value).h(px(68.))),
        )
        .child(field("Unit (optional)", &e.unit))
        .child(field("Description", &e.description))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child("IFC options · JSON for table axes, units, interpolation or allowed values")
                .child(Input::new(&e.extra).h(px(60.))),
        )
        .when_some(e.error.clone(), |v, error| {
            v.child(div().text_color(rgb(0xff9999)).child(error))
        })
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    Button::new("ifc/cancel")
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.ifc.editor = None;
                            window.focus(&this.focus);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("ifc/save")
                        .label("Save")
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            match save(this, cx) {
                                Ok(()) => {
                                    this.ifc.editor = None;
                                    this.refresh_scene();
                                    window.focus(&this.focus);
                                }
                                Err(error) => {
                                    if let Some(e) = this.ifc.editor.as_mut() {
                                        e.error = Some(error);
                                    }
                                }
                            }
                            cx.notify();
                        })),
                ),
        );
    Some(
        div()
            .absolute()
            .inset_0()
            .top(px(layout::TITLE_BAR_HEIGHT))
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000080))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(form)
            .into_any_element(),
    )
}

fn save(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> Result<(), String> {
    let e = shell.ifc.editor.as_ref().ok_or("IFC editor closed")?;
    if e.generation != shell.session.document_generation() {
        return Err("The document changed; reopen this editor".into());
    }
    let read = |i: &Entity<InputState>| i.read(cx).value().to_string();
    let raw = read(&e.value);
    let value = if raw.trim().is_empty() {
        Value::Null
    } else if e.kind.read(cx).selected_value().map(String::as_str) == Some("single")
        && ifc::catalog()
            .type_kinds
            .get(&read(&e.data_type))
            .is_some_and(|k| k == "STRING")
    {
        Value::String(raw)
    } else {
        serde_json::from_str(&raw).unwrap_or(Value::String(raw))
    };
    let property = Property {
        kind: serde_json::from_value(json!(e.kind.read(cx).selected_value()))
            .map_err(|e| e.to_string())?,
        data_type: read(&e.data_type),
        value,
        unit: Some(read(&e.unit)).filter(|s| !s.is_empty()),
        description: read(&e.description),
        ..Default::default()
    };
    let options: serde_json::Map<String, Value> =
        serde_json::from_str(&read(&e.extra)).map_err(|e| format!("Invalid IFC options: {e}"))?;
    if options.keys().any(|k| {
        ![
            "secondaryDataType",
            "secondaryUnit",
            "expression",
            "interpolation",
            "usageName",
            "enumeration",
        ]
        .contains(&k.as_str())
    }) {
        return Err("Unknown IFC option".into());
    }
    let mut value = serde_json::to_value(property).unwrap();
    value.as_object_mut().unwrap().extend(options);
    let property = serde_json::from_value(value).map_err(|e| e.to_string())?;
    shell.session.edit_native_ifc(
        e.definition,
        e.paths.clone(),
        e.old.clone(),
        read(&e.set),
        read(&e.name),
        Some(property),
    )
}
