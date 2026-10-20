//! Draft material editor and project material list, rendered entirely in GPUI.
use super::*;
use gpui_component::{
    Colorize,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    select::{Select, SelectState},
};
use meshcraft_core::document::materials::{Material, ShadingType};

#[derive(Default)]
pub(super) struct MaterialsUi {
    pub selected: Option<String>,
    pub editor: Option<Editor>,
}

pub(super) struct Editor {
    material: Material,
    name: Entity<InputState>,
    color: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    texture: Entity<InputState>,
    u: Entity<InputState>,
    v: Entity<InputState>,
    shading: Entity<SelectState<Vec<String>>>,
    error: Option<String>,
    _subscription: gpui::Subscription,
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
        error: None,
        _subscription: subscription,
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
    div().flex().flex_col().gap(px(5.))
        .children(materials.into_iter().map(|material| {
            let active = material.id == selected;
            Button::new(SharedString::from(format!("material/{}",material.id))).ghost().w_full()
                .child(div().w_full().flex().items_center().gap(px(8.))
                    .child(div().size(px(23.)).border_1().border_color(rgb(0x8e9eae)).bg(gpui::rgba(u32::from_be_bytes([material.color.red,material.color.green,material.color.blue,material.color.alpha]))))
                    .child(material.name.clone()))
                .when(active,|button| button.bg(rgb(0x35516a)))
                .on_click(cx.listener(move |this,event:&gpui::ClickEvent,window,cx| {
                    this.materials.selected=Some(material.id.clone());
                    if event.click_count()==2 { edit(this,material.clone(),window,cx); }
                    cx.notify();
                }))
        }))
        .child(div().flex().flex_wrap().gap(px(4.))
            .child(Button::new("material/new").small().label("New Material").on_click(cx.listener(|this,_,window,cx| {let material=this.cached_document.next_material(); edit(this,material,window,cx);})))
            .child(Button::new("material/edit").small().label("Edit").on_click(cx.listener(move |this,_,window,cx| edit(this,selected_material.clone(),window,cx))))
            .child(Button::new("material/apply").small().label("Apply to Selection").disabled(shell.session.native_selection_targets().is_empty()).on_click(cx.listener(move |this,_,_,cx| {
                if let Err(error)=this.session.assign_native_material(&selected) {this.session.set_native_status(error);}
                this.refresh_scene(); cx.notify();
            }))))
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child("Double-click a material to edit. Textures and shading styles are stored; this phase displays color."))
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
    let editor = shell.materials.editor.as_ref()?;
    Some(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000080))
            .occlude()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(
                div()
                    .w(px(510.))
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
    let id = material.id.clone();
    shell.session.save_native_material(material)?;
    shell.materials.selected = Some(id);
    Ok(())
}
