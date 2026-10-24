//! Draft material editor and project material list, rendered entirely in GPUI.
use super::*;
use gpui_component::checkbox::Checkbox;
use gpui_component::{
    Colorize,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use meshcraft_core::document::{
    materials::{Material, ShadingType},
    procedural::{GRASS, Parameter, ProceduralMaterial, Program},
};

#[derive(Default)]
pub(super) struct MaterialsUi {
    pub selected: Option<String>,
    pub editor: Option<Editor>,
    program_editor: Option<ProgramEditor>,
}

#[derive(Clone)]
struct ParameterChoice {
    key: String,
    label: String,
}
impl SelectItem for ParameterChoice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.key
    }
}
type ParameterField = (
    Parameter,
    Entity<InputState>,
    Option<Entity<SelectState<Vec<ParameterChoice>>>>,
);
pub(super) struct Editor {
    material: Material,
    name: Entity<InputState>,
    color: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    texture: Entity<InputState>,
    u: Entity<InputState>,
    v: Entity<InputState>,
    shading: Entity<SelectState<Vec<String>>>,
    roughness: Entity<InputState>,
    metallic: Entity<InputState>,
    program: Entity<SelectState<Vec<String>>>,
    parameters: Vec<ParameterField>,
    base: Entity<SelectState<Vec<String>>>,
    lod: Entity<InputState>,
    budget: Entity<InputState>,
    _program_subscription: gpui::Subscription,
    error: Option<String>,
    _subscription: gpui::Subscription,
    _color_subscription: gpui::Subscription,
}

fn hex(color: Rgba8) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.red, color.green, color.blue, color.alpha
    )
}

pub(super) fn edit(
    shell: &mut MeshcraftShell,
    material: Material,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let name = cx.new(|cx| InputState::new(window, cx).default_value(material.name.clone()));
    let color = cx.new(|cx| InputState::new(window, cx).default_value(hex(material.color)));
    let picker = cx.new(|cx| {
        ColorPickerState::new(window, cx)
            .default_value(gpui::Hsla::parse_hex(&hex(material.color)).unwrap_or_default())
    });
    let color_field = color.clone();
    let subscription = cx.subscribe_in(
        &picker,
        window,
        move |_, _, event: &ColorPickerEvent, window, cx| {
            if let ColorPickerEvent::Change(Some(value)) = event {
                let rgba: gpui::Rgba = (*value).into();
                let rgba = Rgba8::new(
                    (rgba.r * 255.).round() as u8,
                    (rgba.g * 255.).round() as u8,
                    (rgba.b * 255.).round() as u8,
                    (rgba.a * 255.).round() as u8,
                );
                color_field.update(cx, |field, cx| field.set_value(hex(rgba), window, cx));
                cx.notify();
            }
        },
    );
    let color_picker = picker.clone();
    let color_subscription = cx.subscribe_in(
        &color,
        window,
        move |_, field, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Blur)
                && let Ok(value) = gpui::Hsla::parse_hex(field.read(cx).value().as_ref())
            {
                color_picker.update(cx, |picker, cx| picker.set_value(value, window, cx));
            }
        },
    );
    let texture = cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(material.texture_path.clone().unwrap_or_default())
            .placeholder("Optional texture file")
    });
    let u = cx
        .new(|cx| InputState::new(window, cx).default_value(material.texture_scale_u.to_string()));
    let v = cx
        .new(|cx| InputState::new(window, cx).default_value(material.texture_scale_v.to_string()));
    let index = match material.shading_type {
        ShadingType::Unlit => 0,
        ShadingType::Flat => 1,
        ShadingType::Gouraud => 2,
        ShadingType::Phong => 3,
    };
    let shading = cx.new(|cx| {
        SelectState::new(
            vec![
                "Unlit".into(),
                "Flat".into(),
                "Gouraud".into(),
                "Phong".into(),
            ],
            Some(gpui_component::IndexPath::default().row(index)),
            window,
            cx,
        )
    });
    let pbr = material.pbr.clone().unwrap_or_default();
    let roughness =
        cx.new(|cx| InputState::new(window, cx).default_value(pbr.roughness.to_string()));
    let metallic = cx.new(|cx| InputState::new(window, cx).default_value(pbr.metallic.to_string()));
    let ids = std::iter::once("Conventional".to_owned())
        .chain(
            shell
                .cached_document
                .procedural_programs()
                .into_iter()
                .map(|p| p.id),
        )
        .collect::<Vec<_>>();
    let selected = material
        .procedural
        .as_ref()
        .and_then(|p| ids.iter().position(|id| id == &p.program))
        .unwrap_or(0);
    let program = cx.new(|cx| {
        SelectState::new(
            ids,
            Some(gpui_component::IndexPath::default().row(selected)),
            window,
            cx,
        )
    });
    let parameters = parameter_fields(&material, &shell.cached_document, window, cx);
    let base_ids = std::iter::once("Material color".to_owned())
        .chain(
            shell
                .cached_document
                .materials()
                .into_iter()
                .filter(|m| m.procedural.is_none())
                .map(|m| m.id),
        )
        .collect::<Vec<_>>();
    let base_index = material
        .procedural
        .as_ref()
        .and_then(|p| p.base_material.as_ref())
        .and_then(|id| base_ids.iter().position(|v| v == id))
        .unwrap_or(0);
    let base = cx.new(|cx| {
        SelectState::new(
            base_ids,
            Some(gpui_component::IndexPath::default().row(base_index)),
            window,
            cx,
        )
    });
    let config = material.procedural.clone().unwrap_or_default();
    let lod =
        cx.new(|cx| InputState::new(window, cx).default_value(config.lod_distance.to_string()));
    let budget =
        cx.new(|cx| InputState::new(window, cx).default_value(config.max_instances.to_string()));
    let program_subscription = cx.subscribe_in(
        &program,
        window,
        |this, state, _: &SelectEvent<_>, window, cx| {
            let id = state.read(cx).selected_value().cloned().unwrap_or_default();
            if let Some(editor) = this.materials.editor.as_mut() {
                editor.material.procedural = if id == "Conventional" {
                    None
                } else {
                    Some(ProceduralMaterial {
                        program: id,
                        ..Default::default()
                    })
                };
                editor.parameters =
                    parameter_fields(&editor.material, &this.cached_document, window, cx);
            }
            cx.notify();
        },
    );
    shell.open_menu = None;
    shell.command_palette_open = false;
    shell.materials.editor = Some(Editor {
        material,
        name: name.clone(),
        color,
        picker,
        texture,
        u,
        v,
        shading,
        roughness,
        metallic,
        program,
        parameters,
        base,
        lod,
        budget,
        _program_subscription: program_subscription,
        error: None,
        _subscription: subscription,
        _color_subscription: color_subscription,
    });
    name.update(cx, |state, cx| state.focus(window, cx));
    cx.notify();
}

pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let materials = shell.cached_document.materials();
    let selected = shell
        .materials
        .selected
        .clone()
        .filter(|id| materials.iter().any(|m| &m.id == id))
        .unwrap_or_else(|| materials[0].id.clone());
    let selected_material = materials
        .iter()
        .find(|m| m.id == selected)
        .cloned()
        .unwrap_or_default();
    let delete_id = selected.clone();
    let used = shell.cached_document.material_is_used(&selected);
    div().flex().flex_col().gap(px(5.))
        .on_scroll_wheel(|_,_,cx| cx.stop_propagation())
        .child(div().id("materials/list").max_h(px(240.)).overflow_y_scroll().flex().flex_col().gap(px(3.))
        .children(materials.into_iter().map(|material| {
            let active = material.id == selected;
            Button::new(SharedString::from(format!("material/{}",material.id))).ghost().w_full().flex_shrink_0().justify_start()
                .child(div().w_full().flex().items_center().gap(px(8.))
                    .child(div().size(px(23.)).border_1().border_color(rgb(0x8e9eae)).bg(gpui::rgba(u32::from_be_bytes([material.color.red,material.color.green,material.color.blue,material.color.alpha]))))
                    .child(material.name.clone()))
                .when(active,|button| button.bg(rgb(0x35516a)))
                .on_click(cx.listener(move |this,event:&gpui::ClickEvent,window,cx| {
                    this.materials.selected=Some(material.id.clone());let _=this.session.choose_native_material(&material.id);
                    if event.click_count()==2 { edit(this,material.clone(),window,cx); }
                    cx.notify();
                }))
        })))
        .child(div().flex().flex_wrap().gap(px(4.))
            .child(Button::new("material/grass").small().label("New Grass").on_click(cx.listener(|this,_,window,cx| {
                let mut material=this.cached_document.next_material();material.name="Grass".into();material.color=Rgba8::new(65,130,35,255);material.procedural=Some(ProceduralMaterial::default());edit(this,material,window,cx);
            })))
            .child(Button::new("material/program").small().label("New Shader").on_click(cx.listener(|this,_,window,cx|open_program(this,Program {id:format!("custom.surface.{}",this.cached_document.procedural_programs().len()),name:"Custom surface".into(),..Default::default()},window,cx))))
            .child(Button::new("material/new").small().label("New Material").on_click(cx.listener(|this,_,window,cx| {let material=this.cached_document.next_material(); edit(this,material,window,cx);})))
            .child(Button::new("material/edit").small().label("Edit").on_click(cx.listener(move |this,_,window,cx| edit(this,selected_material.clone(),window,cx))))
            .child(Button::new("material/delete").small().label("Delete").disabled(used)
                .tooltip(if used { "Used by geometry or creation defaults" } else { "Delete unused material" })
                .on_click(cx.listener(move |this,_,_,cx| {
                    if let Err(error)=this.session.delete_native_material(&delete_id) {this.session.set_native_status(error);}
                    this.materials.selected=None;
                    this.refresh_scene(); cx.notify();
                })))
            .child(Button::new("material/apply").small().label("Apply to Selection").disabled(!shell.session.native_selection_targets().iter().any(|t|matches!(&t.entity,SelectedEntity::Segment(_)|SelectedEntity::Triangle(_)|SelectedEntity::Occurrence|SelectedEntity::SemanticHandle{kind:meshcraft_core::selection::SelectionKind::Wall|meshcraft_core::selection::SelectionKind::Slab|meshcraft_core::selection::SelectionKind::Frame,..}))).on_click(cx.listener(move |this,_,_,cx| {
                if let Err(error)=this.session.assign_native_material(&selected) {this.session.set_native_status(error);}
                this.refresh_scene(); cx.notify();
            }))))
        .children(shell.bridge.latest_frame().into_iter().filter(|f|f.procedural_candidates>0).map(|f|div().text_xs().text_color(rgb(0xa9b7c5)).child(format!("Procedural GPU: {} candidates · {} draws",f.procedural_candidates,f.procedural_draw_calls))))
        .children(shell.bridge.latest_frame().into_iter().flat_map(|f|f.procedural_diagnostics).map(|d|div().text_xs().text_color(rgb(0xffaa80)).child(d)))
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child("Double-click to edit. Procedural details are visual; select and snap to the source surface."))
        .into_any_element()
}

fn field(label: &'static str, input: &Entity<InputState>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(div().text_sm().child(label))
        .child(Input::new(input))
        .into_any_element()
}

pub(super) fn overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    if shell.materials.program_editor.is_some() {
        return program_overlay(shell, cx);
    }
    let editor = shell.materials.editor.as_ref()?;
    Some(
        div()
            .absolute()
            .inset_0()
            .top(px(layout::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000080))
            .occlude()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(
                div()
                    .id("material-editor-body")
                    .max_h(gpui::relative(0.85))
                    .overflow_y_scroll()
                    .w(px(570.))
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .bg(rgb(0x202832))
                    .border_1()
                    .border_color(rgb(0x60768a))
                    .rounded_lg()
                    .child(div().text_lg().child("Material Editor"))
                    .child(field("Name", &editor.name))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child("Program")
                            .child(Select::new(&editor.program).w_full()),
                    )
                    .child(procedural_fields(shell, editor, cx))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .items_end()
                            .child(div().flex_1().child(field(
                                "Color · #RRGGBBAA (AA controls opacity)",
                                &editor.color,
                            )))
                            .child(ColorPicker::new(&editor.picker)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child("Shading type")
                            .child(Select::new(&editor.shading)),
                    )
                    .child(
                        Checkbox::new("material/pbr")
                            .label("PBR base shading for procedural surfaces")
                            .checked(editor.material.pbr.is_some())
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(e) = this.materials.editor.as_mut() {
                                    e.material.pbr = if *checked {
                                        Some(Default::default())
                                    } else {
                                        None
                                    };
                                }
                                cx.notify();
                            })),
                    )
                    .when(editor.material.pbr.is_some(), |row| {
                        row.child(
                            div()
                                .flex()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .flex_1()
                                        .child(field("Roughness · 0–1", &editor.roughness)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .child(field("Metallic · 0–1", &editor.metallic)),
                                ),
                        )
                    })
                    .child(field("Texture path (optional)", &editor.texture))
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .child(
                                div()
                                    .flex_1()
                                    .child(field("Texture size U (model units)", &editor.u)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(field("Texture size V (model units)", &editor.v)),
                            ),
                    )
                    .when_some(editor.error.clone(), |row, error| {
                        row.child(div().text_color(rgb(0xff9999)).child(error))
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(Button::new("material/apply-draft").label("Apply").on_click(
                                cx.listener(|this, _, _, cx| {
                                    if let Err(e) = save(this, cx)
                                        && let Some(editor) = this.materials.editor.as_mut()
                                    {
                                        editor.error = Some(e);
                                    }
                                    this.refresh_scene();
                                    cx.notify();
                                }),
                            ))
                            .child(Button::new("material/cancel").label("Cancel").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.materials.editor = None;
                                    window.focus(&this.focus);
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("material/save")
                                    .label("Save")
                                    .primary()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let result = save(this, cx);
                                        match result {
                                            Ok(()) => {
                                                this.materials.editor = None;
                                                this.refresh_scene();
                                                window.focus(&this.focus);
                                            }
                                            Err(error) => {
                                                if let Some(editor) = this.materials.editor.as_mut()
                                                {
                                                    editor.error = Some(error);
                                                }
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element(),
    )
}

fn save(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> Result<(), String> {
    let editor = shell
        .materials
        .editor
        .as_ref()
        .ok_or("material editor is closed")?;
    let mut material = editor.material.clone();
    material.name = editor.name.read(cx).value().to_string();
    let color: gpui::Rgba = gpui::Hsla::parse_hex(editor.color.read(cx).value().as_ref())
        .map_err(|_| "use a color such as #88aaccff (last two digits are opacity)")?
        .into();
    material.color = Rgba8::new(
        (color.r * 255.).round() as u8,
        (color.g * 255.).round() as u8,
        (color.b * 255.).round() as u8,
        (color.a * 255.).round() as u8,
    );
    material.texture_path = Some(editor.texture.read(cx).value().to_string());
    material.texture_scale_u = editor
        .u
        .read(cx)
        .value()
        .parse()
        .map_err(|_| "texture U size must be a number")?;
    material.texture_scale_v = editor
        .v
        .read(cx)
        .value()
        .parse()
        .map_err(|_| "texture V size must be a number")?;
    material.shading_type = match editor.shading.read(cx).selected_value().map(String::as_str) {
        Some("Unlit") => ShadingType::Unlit,
        Some("Flat") => ShadingType::Flat,
        Some("Gouraud") => ShadingType::Gouraud,
        _ => ShadingType::Phong,
    };
    if let Some(p) = material.procedural.as_mut() {
        p.parameters = read_parameter_fields(editor, cx)?;
        p.lod_distance = editor
            .lod
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "LOD distance must be positive")?;
        p.max_instances = editor
            .budget
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "Budget must be an integer")?;
        p.base_material = editor
            .base
            .read(cx)
            .selected_value()
            .filter(|v| v.as_str() != "Material color")
            .cloned();
    }
    if let Some(pbr) = material.pbr.as_mut() {
        pbr.roughness = editor
            .roughness
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "Roughness must be a number")?;
        pbr.metallic = editor
            .metallic
            .read(cx)
            .value()
            .parse()
            .map_err(|_| "Metallic must be a number")?;
    }
    let id = material.id.clone();
    shell.session.save_native_material(material)?;
    shell.session.choose_native_material(&id)?;
    shell.materials.selected = Some(id);
    Ok(())
}

fn display_value(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
fn parameter_fields(
    material: &Material,
    document: &Document,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> Vec<ParameterField> {
    let Some(config) = &material.procedural else {
        return vec![];
    };
    let Some(program) = document
        .procedural_programs()
        .into_iter()
        .find(|p| p.id == config.program)
    else {
        return vec![];
    };
    program
        .parameters
        .into_iter()
        .map(|p| {
            let value = config.parameters.get(&p.id).unwrap_or(&p.default);
            let input =
                cx.new(|cx| InputState::new(window, cx).default_value(display_value(value)));
            let choices = if p.kind == "enum" {
                let rows = p
                    .options
                    .iter()
                    .map(|(key, label)| ParameterChoice {
                        key: key.clone(),
                        label: label.clone(),
                    })
                    .collect::<Vec<_>>();
                let at = rows
                    .iter()
                    .position(|c| Some(c.key.as_str()) == value.as_str())
                    .unwrap_or(0);
                Some(cx.new(|cx| {
                    SelectState::new(
                        rows,
                        Some(gpui_component::IndexPath::default().row(at)),
                        window,
                        cx,
                    )
                }))
            } else {
                None
            };
            (p, input, choices)
        })
        .collect()
}
fn read_parameter_fields(
    editor: &Editor,
    cx: &Context<MeshcraftShell>,
) -> Result<BTreeMap<String, Value>, String> {
    editor
        .parameters
        .iter()
        .map(|(p, input, choices)| {
            let text = input.read(cx).value().to_string();
            let value = if let Some(choices) = choices {
                json!(
                    choices
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default()
                )
            } else if matches!(p.kind.as_str(), "texture" | "enum") {
                json!(text)
            } else {
                serde_json::from_str(&text)
                    .map_err(|_| format!("{}: enter a {} value", p.label, p.kind))?
            };
            p.validate_value(&value)?;
            Ok((p.id.clone(), value))
        })
        .collect()
}
fn procedural_fields(
    shell: &MeshcraftShell,
    editor: &Editor,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let Some(config) = editor.material.procedural.as_ref() else {
        return div().into_any_element();
    };
    let program_id = config.program.clone();
    let program = shell
        .cached_document
        .procedural_programs()
        .into_iter()
        .find(|p| p.id == program_id);
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(div().text_sm().child("Procedural parameters"))
        .children(editor.parameters.iter().map(|(p, input, choices)| {
            let input = input.clone();
            let input_bool = input.clone();
            let choices_reset = choices.clone();
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(170.))
                        .text_sm()
                        .child(if p.label.is_empty() {
                            p.id.clone()
                        } else {
                            p.label.clone()
                        })
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0xa9b7c5))
                                .child(format!("{} · {} {}", p.description, p.kind, p.unit)),
                        ),
                )
                .child(if p.kind == "boolean" {
                    Checkbox::new(SharedString::from(format!("procedural/{}", p.id)))
                        .checked(input.read(cx).value().as_ref() == "true")
                        .on_click(cx.listener(move |_, checked: &bool, window, cx| {
                            input_bool
                                .update(cx, |i, cx| i.set_value(checked.to_string(), window, cx));
                            cx.notify();
                        }))
                        .into_any_element()
                } else if let Some(choices) = choices {
                    div()
                        .flex_1()
                        .child(Select::new(choices))
                        .into_any_element()
                } else {
                    div().flex_1().child(Input::new(&input)).into_any_element()
                })
                .child(
                    Button::new(SharedString::from(format!("procedural/reset/{}", p.id)))
                        .small()
                        .label("Reset")
                        .on_click(cx.listener({
                            let value = p.default.clone();
                            move |_, _, window, cx| {
                                if let Some(choices) = &choices_reset {
                                    choices.update(cx, |state, cx| {
                                        state.set_selected_value(
                                            &value.as_str().unwrap_or_default().to_owned(),
                                            window,
                                            cx,
                                        )
                                    });
                                }
                                input.update(cx, |i, cx| {
                                    i.set_value(display_value(&value), window, cx)
                                });
                                cx.notify();
                            }
                        })),
                )
                .into_any_element()
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child("Base shading")
                .child(Select::new(&editor.base).w_full()),
        )
        .child(
            div()
                .flex()
                .gap(px(10.))
                .child(div().flex_1().child(field("LOD distance", &editor.lod)))
                .child(
                    div()
                        .flex_1()
                        .child(field("Candidate budget", &editor.budget)),
                ),
        )
        .child(
            Checkbox::new("procedural/bounds")
                .label("Show procedural bounds")
                .checked(config.show_bounds)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    if let Some(p) = this
                        .materials
                        .editor
                        .as_mut()
                        .and_then(|e| e.material.procedural.as_mut())
                    {
                        p.show_bounds = *checked;
                    }
                    cx.notify();
                })),
        )
        .when_some(program, |row, p| {
            row.child(
                Button::new("procedural/edit-program")
                    .small()
                    .label("Edit Program / Parameter Schema")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mut p = p.clone();
                        if p.id == GRASS {
                            p.id = "custom.grass".into();
                            p.name = "Custom grass".into();
                        }
                        open_program(this, p, window, cx);
                    })),
            )
        })
        .child(
            div()
                .flex()
                .gap(px(5.))
                .flex_wrap()
                .child(
                    Button::new("procedural/read-overrides")
                        .small()
                        .label("Read Selection Values")
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Err(error) = read_selection_values(this, window, cx)
                                && let Some(e) = this.materials.editor.as_mut()
                            {
                                e.error = Some(error);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("procedural/apply-overrides")
                        .small()
                        .label("Apply Values to Selection")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = (|| {
                                let e = this.materials.editor.as_ref().unwrap();
                                let values = read_parameter_fields(e, cx)?;
                                this.session
                                    .assign_native_material_parameters(&e.material.id, Some(values))
                            })();
                            if let Some(e) = this.materials.editor.as_mut() {
                                e.error = result.err();
                            }
                            this.refresh_scene();
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("procedural/reset-overrides")
                        .small()
                        .label("Reset Selection Overrides")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let id = this.materials.editor.as_ref().unwrap().material.id.clone();
                            let result = this.session.assign_native_material(&id);
                            if let Some(e) = this.materials.editor.as_mut() {
                                e.error = result.err();
                            }
                            this.refresh_scene();
                            cx.notify();
                        })),
                ),
        )
        .children(
            shell
                .bridge
                .latest_frame()
                .into_iter()
                .flat_map(|f| f.procedural_diagnostics)
                .map(|d| div().text_xs().text_color(rgb(0xffaa80)).child(d)),
        )
        .into_any_element()
}
struct ProgramEditor {
    code: Entity<InputState>,
    vertex: Entity<InputState>,
    fragment: Entity<InputState>,
    error: Option<String>,
}
fn open_program(
    shell: &mut MeshcraftShell,
    program: Program,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let mut metadata = serde_json::to_value(&program).unwrap();
    metadata.as_object_mut().unwrap().remove("vertex");
    metadata.as_object_mut().unwrap().remove("fragment");
    let vertex = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(program.vertex.clone())
    });
    let fragment = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(program.fragment.clone())
    });
    let code = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(serde_json::to_string_pretty(&metadata).unwrap())
    });
    shell.materials.program_editor = Some(ProgramEditor {
        code: code.clone(),
        vertex,
        fragment,
        error: None,
    });
    code.update(cx, |s, cx| s.focus(window, cx));
    cx.notify();
}
fn program_overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let editor = shell.materials.program_editor.as_ref()?;
    Some(div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000080)).occlude()
        .child(div().w(px(850.)).p(px(16.)).bg(rgb(0x202832)).flex().flex_col().gap(px(8.))
            .child("Procedural Program · JSON definition / GLSL functions")
            .child(div().text_sm().child("vertex: void proceduralVertex(inout vec3 position, inout vec3 normal, vec2 uv)\nfragment: vec4 proceduralColor(vec4 color, vec3 position, vec3 normal, vec2 uv)\nParameters are uniforms u_<id>; time is seconds. Sources use GLSL 330."))
            .child(div().h(px(180.)).font_family(cx.global::<Theme>().mono_font_family.clone()).child(Input::new(&editor.code).h_full()))
            .child(div().flex().gap(px(10.)).font_family(cx.global::<Theme>().mono_font_family.clone())
                .child(div().flex_1().child("Vertex function (GLSL)").child(div().h(px(210.)).child(Input::new(&editor.vertex).h_full())))
                .child(div().flex_1().child("Fragment function (GLSL)").child(div().h(px(210.)).child(Input::new(&editor.fragment).h_full()))))
            .when_some(editor.error.clone(),|row,e|row.child(div().text_color(rgb(0xff9999)).child(e)))
            .child(div().flex().gap(px(8.))
                .child(Button::new("program/cancel").label("Cancel").on_click(cx.listener(|this,_,_,cx|{this.materials.program_editor=None;cx.notify();})))
                .child(Button::new("program/save").label("Save Program").primary().on_click(cx.listener(|this,_,window,cx| {
                    let code=this.materials.program_editor.as_ref().unwrap().code.read(cx).value().to_string();
                    let result=(|| {let editor=this.materials.program_editor.as_ref().unwrap();let mut value=serde_json::from_str::<Value>(&code).map_err(|e|e.to_string())?;
                        let metadata=value.as_object_mut().ok_or("Program metadata must be an object")?;
                        metadata.insert("vertex".into(),json!(editor.vertex.read(cx).value().to_string()));metadata.insert("fragment".into(),json!(editor.fragment.read(cx).value().to_string()));
                        let p:Program=serde_json::from_value(value).map_err(|e|e.to_string())?;let id=p.id.clone();this.session.save_native_procedural_program(p)?;Ok::<_,String>(id)})();
                    match result {Ok(id)=>{this.materials.program_editor=None;this.refresh_scene();if let Some(e)=this.materials.editor.as_ref(){let mut m=e.material.clone();m.procedural=Some(ProceduralMaterial {program:id,..Default::default()});edit(this,m,window,cx);}},Err(e)=>{this.materials.program_editor.as_mut().unwrap().error=Some(e);}}
                    cx.notify();
                }))))) .into_any_element())
}

fn read_selection_values(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> Result<(), String> {
    let editor = shell
        .materials
        .editor
        .as_ref()
        .ok_or("Material editor closed")?;
    let targets = shell.session.native_selection_targets();
    if targets.is_empty() {
        return Err("Select a surface or object first".into());
    }
    let mut result = None;
    for occurrence in shell.cached_document.scene_geometry_refs()? {
        let Some(g) = shell.cached_document.effective_geometry(&occurrence) else {
            continue;
        };
        for f in g.triangles.values() {
            if meshcraft_core::document::materials::material_id(&f.extensions)
                != Some(editor.material.id.as_str())
            {
                continue;
            }
            if !targets.iter().any(|t| match &t.entity {
                SelectedEntity::Occurrence => occurrence.path.starts_with(&t.occurrence.path),
                SelectedEntity::Triangle(id) => t.occurrence == occurrence && *id == f.id,
                _ => {
                    t.occurrence == occurrence
                        && meshcraft_core::document::architecture::semantic_entity(&f.extensions)
                            .as_ref()
                            == Some(&t.entity)
                }
            }) {
                continue;
            }
            let overrides = f
                .extensions
                .get(meshcraft_core::document::procedural::OVERRIDES_KEY)
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let config = editor
                .material
                .procedural
                .as_ref()
                .ok_or("Conventional material")?;
            let program = shell
                .cached_document
                .procedural_programs()
                .into_iter()
                .find(|p| p.id == config.program)
                .ok_or("Program missing")?;
            let values = program.resolve(&config.parameters, &overrides)?;
            if result.as_ref().is_some_and(|old| old != &values) {
                return Err(
                    "Selection has mixed values; select one assignment to read its parameters"
                        .into(),
                );
            }
            result = Some(values);
        }
    }
    let values = result.ok_or("The selection has no surfaces assigned to this material")?;
    for (p, input, choices) in &editor.parameters {
        if let Some(value) = values.get(&p.id) {
            input.update(cx, |i, cx| i.set_value(display_value(value), window, cx));
            if let Some(choices) = choices {
                choices.update(cx, |i, cx| {
                    i.set_selected_value(&value.as_str().unwrap_or_default().to_owned(), window, cx)
                });
            }
        }
    }
    shell.materials.editor.as_mut().unwrap().error = None;
    shell.session.set_native_status(
        "Loaded selected assignment values; Apply Values to Selection saves instance overrides."
            .into(),
    );
    Ok(())
}

pub(super) fn capture_key(
    shell: &mut MeshcraftShell,
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    if shell.materials.program_editor.is_none() {
        return false;
    }
    if event.keystroke.key == "escape" {
        shell.materials.program_editor = None;
        window.focus(&shell.focus);
        cx.stop_propagation();
        cx.notify();
    }
    true
}
