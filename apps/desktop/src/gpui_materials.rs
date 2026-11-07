//! Draft material editor and project material list, rendered entirely in GPUI.
use super::*;
#[path = "gpui_object_preview.rs"]
mod object_preview;
#[path = "gpui_shader_editor.rs"]
mod shader_editor;
use gpui_component::checkbox::Checkbox;
use gpui_component::{
    Colorize,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use meshcraft_core::document::{
    DefinitionId, OccurrenceRef,
    materials::{Material, ShadingType},
    procedural::{self, Parameter, ProceduralMaterial, Program},
};

/// The two tabs of the Materials panel.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    /// Materials in the model.
    #[default]
    Model,
    /// The material library that ships with Meshcraft.
    Library,
}

#[derive(Default)]
pub(super) struct MaterialsUi {
    pub selected: Option<String>,
    pub tab: Tab,
    /// The library material selected in the Library tab.
    pub library: Option<String>,
    /// The program selected in the Shader Programs panel.
    pub program: Option<String>,
    pub editor: Option<Editor>,
    program_editor: Option<shader_editor::Editor>,
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
struct ParameterField {
    parameter: Parameter,
    input: Entity<InputState>,
    choices: Option<Entity<SelectState<Vec<ParameterChoice>>>>,
    color: Option<Entity<ColorPickerState>>,
    _color_subscription: Option<gpui::Subscription>,
}
#[derive(Clone)]
struct ObjectEdit {
    definition: Option<DefinitionId>,
    instance: Option<OccurrenceRef>,
}
pub(super) struct Editor {
    object: Option<ObjectEdit>,
    preview: Option<object_preview::Preview>,
    material: Material,
    name: Entity<InputState>,
    color: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    texture: Entity<InputState>,
    u: Entity<InputState>,
    v: Entity<InputState>,
    tint: Entity<InputState>,
    shading: Entity<SelectState<Vec<String>>>,
    roughness: Entity<InputState>,
    metallic: Entity<InputState>,
    program: Entity<SelectState<Vec<ParameterChoice>>>,
    /// The name the draft opened with, while it is a new, unsaved material.
    auto_name: Option<String>,
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
    object_preview::detach(shell);
    let is_object = material.procedural.as_ref().is_some_and(|c| {
        shell
            .cached_document
            .procedural_programs()
            .iter()
            .any(|p| p.id == c.program && p.is_object_generator())
    });
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
    let tint =
        cx.new(|cx| InputState::new(window, cx).default_value(material.texture_tint.to_string()));
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
    let ids = program_choices(&shell.cached_document, is_object);
    let selected = material
        .procedural
        .as_ref()
        .and_then(|p| ids.iter().position(|c| c.key == p.program))
        .unwrap_or(0);
    let auto_name = (!shell
        .cached_document
        .materials()
        .iter()
        .any(|m| m.id == material.id))
    .then(|| material.name.clone());
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
            apply_program_look(this, &id, window, cx);
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
            // Every material keeps its preview, including Conventional. The
            // normal debounced draft observer updates its private scene.
            if this
                .materials
                .editor
                .as_ref()
                .is_some_and(|e| e.preview.is_none())
            {
                object_preview::open(this, cx);
            }
            cx.notify();
        },
    );
    shell.open_menu = None;
    shell.command_palette_open = false;
    shell.materials.editor = Some(Editor {
        object: None,
        preview: None,
        material,
        name: name.clone(),
        color,
        picker,
        texture,
        u,
        v,
        tint,
        shading,
        roughness,
        metallic,
        program,
        auto_name,
        parameters,
        base,
        lod,
        budget,
        _program_subscription: program_subscription,
        error: None,
        _subscription: subscription,
        _color_subscription: color_subscription,
    });
    object_preview::open(shell, cx);
    name.update(cx, |state, cx| state.focus(window, cx));
    cx.notify();
}

pub(super) fn selected_material(
    document: &meshcraft_core::document::Document,
    selected: Option<&str>,
) -> Material {
    let materials = document.materials();
    materials
        .iter()
        .find(|material| Some(material.id.as_str()) == selected)
        .or_else(|| materials.first())
        .cloned()
        .unwrap_or_default()
}

/// Width of one material tile; three fit across the settings panel.
const TILE_WIDTH: f32 = 122.;
const TILE_IMAGE: f32 = 96.;

/// One material: its sphere preview (a colour swatch until the preview is
/// rendered) with its name underneath.
fn tile(
    shell: &MeshcraftShell,
    id: SharedString,
    material: &Material,
    active: bool,
    in_model: bool,
) -> gpui::Stateful<gpui::Div> {
    let color = material.color;
    let swatch = gpui::rgba(u32::from_be_bytes([
        color.red,
        color.green,
        color.blue,
        color.alpha,
    ]));
    let picture = match super::material_thumbnails::image(shell, material) {
        Some(image) => img(ImageSource::Render(image))
            .size(px(TILE_IMAGE))
            .into_any_element(),
        None => div()
            .size(px(TILE_IMAGE))
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(TILE_IMAGE * 0.72)).rounded_full().bg(swatch))
            .into_any_element(),
    };
    let backdrop = super::material_thumbnails::BACKDROP;
    let probe = super::workspace_controls::bounds_probe(shell, format!("panel/materials/{id}"));
    div()
        .id(id)
        .relative()
        .child(probe)
        .w(px(TILE_WIDTH))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(2.))
        .pt(px(4.))
        .pb(px(5.))
        .px(px(4.))
        .rounded(px(4.))
        .border_2()
        .bg(rgb(u32::from_be_bytes([
            0,
            backdrop.red,
            backdrop.green,
            backdrop.blue,
        ])))
        .border_color(if active { rgb(0x5aa7ec) } else { rgb(0x2b323b) })
        .when(!active, |tile| {
            tile.hover(|s| s.border_color(rgb(0x6b7c8e)))
        })
        .cursor_pointer()
        .child(picture)
        .child(
            div()
                .w_full()
                .text_sm()
                .text_center()
                .truncate()
                .child(material.name.clone()),
        )
        .when(in_model, |tile| {
            tile.child(
                div()
                    .absolute()
                    .top(px(4.))
                    .right(px(6.))
                    .text_xs()
                    .text_color(rgb(0x8fd18b))
                    .child("✓"),
            )
        })
}

fn grid(id: &'static str, tiles: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(6.))
        .children(tiles)
        .into_any_element()
}

fn tab_button(
    shell: &MeshcraftShell,
    label: &'static str,
    tab: Tab,
    current: Tab,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let active = tab == current;
    div()
        .id(SharedString::from(format!(
            "materials/tab/{}",
            label.to_lowercase()
        )))
        .relative()
        .child(super::workspace_controls::bounds_probe(
            shell,
            format!("panel/materials/tab/{}", label.to_lowercase()),
        ))
        .px(px(14.))
        .py(px(4.))
        .border_1()
        .border_b_0()
        .rounded_t(px(4.))
        .border_color(if active { rgb(0xb7c4d2) } else { rgb(0x48525d) })
        .bg(if active { rgb(0x2b323b) } else { rgb(0x1e242c) })
        .text_color(if active { rgb(0xe8eef5) } else { rgb(0x8d99a6) })
        .cursor_pointer()
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.materials.tab = tab;
            cx.notify();
        }))
        .into_any_element()
}

/// The Materials panel: a Model tab with the model's materials and a Library tab
/// with the materials that ship with Meshcraft, each shown as a sphere preview.
pub(super) fn panel(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
    model_actions: gpui::AnyElement,
    library_actions: gpui::AnyElement,
) -> gpui::AnyElement {
    let tab = shell.materials.tab;
    let materials = shell.cached_document.materials();
    let tabs = div()
        .flex()
        .items_end()
        .gap(px(2.))
        .border_b_1()
        .border_color(rgb(0x48525d))
        .child(tab_button(shell, "Model", Tab::Model, tab, cx))
        .child(tab_button(shell, "Library", Tab::Library, tab, cx));
    let (content, actions, hint) = match tab {
        Tab::Model => {
            let selected =
                selected_material(&shell.cached_document, shell.materials.selected.as_deref()).id;
            let tiles = materials
                .iter()
                .map(|material| {
                    let id = material.id.clone();
                    let edited = material.clone();
                    tile(
                        shell,
                        SharedString::from(format!("material/{}", material.id)),
                        material,
                        material.id == selected,
                        false,
                    )
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            this.materials.selected = Some(id.clone());
                            let _ = this.session.choose_native_material(&id);
                            if event.click_count() == 2 {
                                edit(this, edited.clone(), window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element()
                })
                .collect();
            (
                grid("materials/list", tiles),
                model_actions,
                "Double-click to edit. Procedural details are visual; select and snap to the source surface.",
            )
        }
        Tab::Library => {
            let in_model = materials
                .iter()
                .map(|m| m.id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let selected = shell.materials.library.as_deref();
            let sections = meshcraft_core::document::material_library::material_library()
                .into_iter()
                .map(|section| {
                    let tiles = section
                        .materials
                        .iter()
                        .map(|material| {
                            let id = material.id.clone();
                            tile(
                                shell,
                                SharedString::from(format!("library/{}", material.id)),
                                material,
                                selected == Some(material.id.as_str()),
                                in_model.contains(material.id.as_str()),
                            )
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                this.materials.library = Some(id.clone());
                                if event.click_count() == 2 {
                                    let mut payload = CommandPayload::default();
                                    payload.values.insert("id".into(), json!(id.clone()));
                                    this.dispatch_payload("material.add_from_library", &payload);
                                    this.materials.selected = Some(id.clone());
                                }
                                cx.notify();
                            }))
                            .into_any_element()
                        })
                        .collect();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(0xc9d6e3))
                                .child(section.title),
                        )
                        .child(grid(section.id, tiles))
                })
                .collect::<Vec<_>>();
            (
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(sections)
                    .into_any_element(),
                library_actions,
                "Double-click or Add to Model to copy a material into the model, where it can be edited. Apply to Selection adds it too. ✓ marks materials already in the model.",
            )
        }
    };
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(tabs)
        .child(
            // The probe sits beside the scrolling list, so it measures the visible window.
            div()
                .relative()
                .child(
                    div()
                        .id(SharedString::from(match tab {
                            Tab::Model => "materials/model-scroll",
                            Tab::Library => "materials/library-scroll",
                        }))
                        .max_h(px(470.))
                        .overflow_y_scroll()
                        .child(content),
                )
                .child(super::workspace_controls::bounds_probe(
                    shell,
                    "panel/materials/scroll".into(),
                )),
        )
        .child(actions)
        .children(
            shell
                .bridge
                .latest_frame()
                .into_iter()
                .filter(|f| f.procedural_candidates > 0)
                .map(|f| {
                    div().text_xs().text_color(rgb(0xa9b7c5)).child(format!(
                        "Procedural GPU: {} candidates · {} draws",
                        f.procedural_candidates, f.procedural_draw_calls
                    ))
                }),
        )
        .children(
            shell
                .bridge
                .latest_frame()
                .into_iter()
                .flat_map(|f| f.procedural_diagnostics)
                .map(|d| div().text_xs().text_color(rgb(0xffaa80)).child(d)),
        )
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(hint))
        .into_any_element()
}

/// A path box with a Browse button that opens the system file dialog, the same one Open and Save
/// use, filtered to images. Files beside the project are stored relative to it.
fn file_input(
    id: &str,
    input: &Entity<InputState>,
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let root = shell.texture_root();
    let browse = input.clone();
    let clear = input.clone();
    div()
        .flex()
        .flex_1()
        .items_center()
        .gap(px(6.))
        .child(div().flex_1().child(Input::new(input)))
        .child(
            Button::new(SharedString::from(format!("{id}/browse")))
                .small()
                .label("Browse…")
                .on_click(cx.listener(move |_, _, window, cx| {
                    let mut dialog = rfd::FileDialog::new()
                        .set_title("Choose a texture image")
                        .add_filter("Images", textures::EXTENSIONS);
                    let current = browse.read(cx).value().to_string();
                    let start = if current.trim().is_empty() {
                        root.clone()
                    } else {
                        textures::resolve(&current, root.as_deref())
                            .parent()
                            .map(Into::into)
                    };
                    if let Some(dir) = start.filter(|d| d.is_dir()) {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(file) = dialog.pick_file() {
                        let value = textures::stored_path(&file, root.as_deref());
                        browse.update(cx, |state, cx| state.set_value(value, window, cx));
                        cx.notify();
                    }
                })),
        )
        .child(
            Button::new(SharedString::from(format!("{id}/clear")))
                .small()
                .ghost()
                .label("Clear")
                .on_click(cx.listener(move |_, _, window, cx| {
                    clear.update(cx, |state, cx| state.set_value("", window, cx));
                    cx.notify();
                })),
        )
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
    let fields = div()
        .id("material-editor-fields")
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(field("Name", &editor.name))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child("Program")
                .child(div().flex_1().child(Select::new(&editor.program).w_full()))
                .when(editor.object.is_none(), |row| {
                    row.child(
                        Button::new("material/new-shader")
                            .small()
                            .label("New Shader…")
                            .tooltip("Write a new shader program; saving it selects it for this material")
                            .on_click(cx.listener(|this, _, window, cx| {
                                shader_editor::open_new_for_material(this, window, cx);
                                cx.notify();
                            })),
                    )
                }),
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
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(div().text_sm().child("Texture image (optional)"))
                .child(file_input("material/texture", &editor.texture, shell, cx)),
        )
        .child(
            div()
                .flex()
                .gap(px(10.))
                .items_end()
                .child(div().flex_1().child(field("Image width (model units)", &editor.u)))
                .child(div().pb(px(6.)).child("×"))
                .child(div().flex_1().child(field("Image height (model units)", &editor.v))),
        )
        .child(field(
            "Colorization · 0 shows the image as it is, 1 multiplies it by the color",
            &editor.tint,
        ))
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(
            "To place the image on a face, use Materials › Edit Texture Position and drag its corners in the viewport.",
        ));
    let footer =
        div()
            .flex()
            .flex_col()
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
                    .child(
                        Button::new("material/cancel")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                close(this, window, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("material/save")
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                let result = save(this, cx);
                                match result {
                                    Ok(()) => {
                                        close(this, window, cx);
                                        this.refresh_scene();
                                    }
                                    Err(error) => {
                                        if let Some(editor) = this.materials.editor.as_mut() {
                                            editor.error = Some(error);
                                        }
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            );
    let wide = editor.preview.is_some();
    let content = if wide {
        div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .gap(px(16.))
            .child(
                div()
                    .id("object-settings-scroll")
                    .w(px(530.))
                    .max_w(gpui::relative(0.5))
                    .min_w(px(0.))
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .child(fields),
            )
            .child(object_preview::panel(shell, cx))
            .into_any_element()
    } else {
        fields.into_any_element()
    };
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
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .bg(rgb(0x202832))
                    .border_1()
                    .border_color(rgb(0x60768a))
                    .rounded_lg()
                    .when(wide, |row| {
                        row.w(gpui::relative(0.96))
                            .max_w(px(1500.))
                            .h(gpui::relative(0.9))
                    })
                    .when(!wide, |row| {
                        row.w(px(570.))
                            .max_h(gpui::relative(0.85))
                            .overflow_y_scroll()
                    })
                    .child(div().text_lg().child(if editor.object.is_some() {
                        "Procedural Object Editor"
                    } else {
                        "Material Editor"
                    }))
                    .child(content)
                    .child(footer),
            )
            .into_any_element(),
    )
}

pub(super) fn close(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    object_preview::detach(shell);
    shell.materials.editor = None;
    window.focus(&shell.focus);
    cx.notify();
}

fn save(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> Result<(), String> {
    let editor = shell
        .materials
        .editor
        .as_ref()
        .ok_or("material editor is closed")?;
    let material = draft_material(editor, cx)?;
    if let Some(object) = editor.object.clone() {
        let id = shell.session.save_native_procedural_object(
            object.definition,
            object.instance,
            material,
        )?;
        if let Some(e) = shell.materials.editor.as_mut() {
            e.object.as_mut().unwrap().definition = Some(id);
        }
        return Ok(());
    }
    let id = material.id.clone();
    shell.session.save_native_material(material)?;
    shell.session.choose_native_material(&id)?;
    shell.materials.selected = Some(id);
    Ok(())
}

fn draft_material(editor: &Editor, cx: &Context<MeshcraftShell>) -> Result<Material, String> {
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
    material.texture_tint = editor
        .tint
        .read(cx)
        .value()
        .parse()
        .map_err(|_| "colorization must be a number from 0 to 1")?;
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
    material.validate()?;
    Ok(material)
}

fn display_value(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
fn parameter_color(value: &Value) -> Option<gpui::Hsla> {
    let a = value.as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some(
        gpui::Rgba {
            r: a[0].as_f64()? as f32,
            g: a[1].as_f64()? as f32,
            b: a[2].as_f64()? as f32,
            a: a[3].as_f64()? as f32,
        }
        .into(),
    )
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
            let (color, subscription) = if p.kind == "color" {
                let picker = cx.new(|cx| {
                    ColorPickerState::new(window, cx)
                        .default_value(parameter_color(value).unwrap_or_default())
                });
                let raw = input.clone();
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    move |_, _, event: &ColorPickerEvent, window, cx| {
                        if let ColorPickerEvent::Change(Some(value)) = event {
                            let rgba: gpui::Rgba = (*value).into();
                            let value = json!([
                                rgba.r.clamp(0., 1.),
                                rgba.g.clamp(0., 1.),
                                rgba.b.clamp(0., 1.),
                                rgba.a.clamp(0., 1.)
                            ]);
                            raw.update(cx, |input, cx| {
                                input.set_value(value.to_string(), window, cx)
                            });
                            cx.notify();
                        }
                    },
                );
                (Some(picker), Some(subscription))
            } else {
                (None, None)
            };
            ParameterField {
                parameter: p,
                input,
                choices,
                color,
                _color_subscription: subscription,
            }
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
        .map(|field| {
            let ParameterField {
                parameter: p,
                input,
                choices,
                ..
            } = field;
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
        .children(editor.parameters.iter().map(|field| {
            let ParameterField {
                parameter: p,
                input,
                choices,
                color,
                ..
            } = field;
            let input = input.clone();
            let input_bool = input.clone();
            let choices_reset = choices.clone();
            let color_reset = color.clone();
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
                        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(
                            if p.kind == "color" {
                                "Color and opacity".into()
                            } else {
                                format!("{} · {} {}", p.description, p.kind, p.unit)
                            },
                        )),
                )
                .child(if let Some(picker) = color {
                    div()
                        .flex_1()
                        .child(ColorPicker::new(picker).w_full())
                        .into_any_element()
                } else if p.kind == "boolean" {
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
                } else if p.kind == "texture" {
                    file_input(&format!("procedural/{}", p.id), &input, shell, cx)
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
                                if let Some(picker) = &color_reset
                                    && let Some(value) = parameter_color(&value)
                                {
                                    picker.update(cx, |picker, cx| {
                                        picker.set_value(value, window, cx)
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
                        if procedural::is_builtin(&p.id) {
                            p.id = format!("custom.{}", p.name.to_lowercase());
                            p.name = format!("Custom {}", p.name);
                        }
                        open_program(this, p, window, cx);
                    })),
            )
        })
        .when(editor.object.is_none(), |row| {
            row.child(
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
                                    this.session.assign_native_material_parameters(
                                        &e.material.id,
                                        Some(values),
                                    )
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
                                let id =
                                    this.materials.editor.as_ref().unwrap().material.id.clone();
                                let result = this.session.assign_native_material(&id);
                                if let Some(e) = this.materials.editor.as_mut() {
                                    e.error = result.err();
                                }
                                this.refresh_scene();
                                cx.notify();
                            })),
                    ),
            )
        })
        .children(
            shell
                .bridge
                .latest_frame()
                .into_iter()
                .filter(|_| editor.preview.is_none())
                .flat_map(|f| f.procedural_diagnostics)
                .map(|d| div().text_xs().text_color(rgb(0xffaa80)).child(d)),
        )
        .into_any_element()
}
/// Program picker entries: "Conventional" (no shader) for materials, then every
/// program of the matching kind, labelled by name with the ID for reference.
fn program_choices(
    document: &meshcraft_core::document::Document,
    is_object: bool,
) -> Vec<ParameterChoice> {
    (!is_object)
        .then(|| ParameterChoice {
            key: "Conventional".into(),
            label: "Conventional · no shader".into(),
        })
        .into_iter()
        .chain(
            document
                .procedural_programs()
                .into_iter()
                .filter(|p| p.is_object_generator() == is_object)
                .map(|p| ParameterChoice {
                    label: format!("{} · {}", p.name, p.id),
                    key: p.id,
                }),
        )
        .collect()
}

/// Choosing a built-in program for a new material also takes the built-in's
/// suggested colour and PBR settings, and its name while the name is untouched.
fn apply_program_look(
    shell: &mut MeshcraftShell,
    id: &str,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let Some(editor) = shell.materials.editor.as_mut() else {
        return;
    };
    let Some(auto_name) = editor.auto_name.clone() else {
        return;
    };
    if editor.object.is_some() {
        return;
    }
    let Some((name, color, roughness, metallic)) = procedural::preset_look(id) else {
        return;
    };
    if editor.name.read(cx).value().as_ref() == auto_name {
        editor
            .name
            .update(cx, |field, cx| field.set_value(name, window, cx));
        editor.auto_name = Some(name.into());
    }
    editor
        .color
        .update(cx, |field, cx| field.set_value(hex(color), window, cx));
    if let Ok(value) = gpui::Hsla::parse_hex(&hex(color)) {
        editor
            .picker
            .update(cx, |picker, cx| picker.set_value(value, window, cx));
    }
    editor.material.pbr = Some(meshcraft_core::document::materials::Pbr {
        roughness,
        metallic,
    });
    editor.roughness.update(cx, |field, cx| {
        field.set_value(roughness.to_string(), window, cx)
    });
    editor.metallic.update(cx, |field, cx| {
        field.set_value(metallic.to_string(), window, cx)
    });
}

/// Opens a stored program in the shader editor. Built-ins open unchanged; saving
/// one stores a custom copy, so built-ins never change.
pub(super) fn edit_program(
    shell: &mut MeshcraftShell,
    id: &str,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let Some(program) = shell
        .cached_document
        .procedural_programs()
        .into_iter()
        .find(|p| p.id == id)
    else {
        return;
    };
    object_preview::detach(shell);
    shell.materials.editor = None;
    open_program(shell, program, window, cx);
}

/// The Shader Programs panel: every program with its kind and users.
pub(super) fn programs_panel(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
    actions: gpui::AnyElement,
) -> gpui::AnyElement {
    let document = &shell.cached_document;
    let mut programs = document.procedural_programs();
    programs.sort_by_key(|p| procedural::is_builtin(&p.id));
    let selected = shell.materials.program.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("programs/list")
                .max_h(px(260.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(3.))
                .children(programs.into_iter().map(|program| {
                    let usage = document.procedural_program_usage(&program.id);
                    let builtin = procedural::is_builtin(&program.id);
                    let kind = if program.is_object_generator() {
                        "object"
                    } else {
                        "surface"
                    };
                    let mut users = Vec::new();
                    if !usage.materials.is_empty() {
                        users.push(format!(
                            "{} material{}",
                            usage.materials.len(),
                            if usage.materials.len() == 1 { "" } else { "s" }
                        ));
                    }
                    if usage.objects > 0 {
                        users.push(format!(
                            "{} object{}",
                            usage.objects,
                            if usage.objects == 1 { "" } else { "s" }
                        ));
                    }
                    let detail = format!(
                        "{} · {} {kind}{}",
                        program.id,
                        if builtin { "built-in" } else { "custom" },
                        if users.is_empty() {
                            " · unused".to_owned()
                        } else {
                            format!(" · {}", users.join(", "))
                        }
                    );
                    let active = selected.as_deref() == Some(program.id.as_str());
                    let id = program.id.clone();
                    let probe = super::workspace_controls::bounds_probe(
                        shell,
                        format!("panel/programs/{}", program.id),
                    );
                    Button::new(SharedString::from(format!("program/{}", program.id)))
                        .ghost()
                        .w_full()
                        .h(px(42.))
                        .flex_shrink_0()
                        .justify_start()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .items_start()
                                .child(program.name.clone())
                                .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(detail)),
                        )
                        .child(probe)
                        .when(active, |button| button.bg(rgb(0x35516a)))
                        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            this.materials.program = Some(id.clone());
                            let _ = this.session.choose_native_program(&id);
                            if event.click_count() == 2 {
                                edit_program(this, &id, window, cx);
                            }
                            cx.notify();
                        }))
                })),
        )
        .child(actions)
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(
            "Double-click to open in the shader editor. Built-ins are read-only: saving one stores a copy. Deleting a program turns its materials into conventional ones.",
        ))
        .into_any_element()
}

/// Materials › New Shader and Objects › New Shader Object. Both open the shader
/// editor on a blank program with a choice of starting point; only the object
/// command creates and places an object when an object program is saved.
pub(super) fn new_program(
    shell: &mut MeshcraftShell,
    object: bool,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    object_preview::detach(shell);
    shell.materials.editor = None;
    shader_editor::open_new(shell, object, window, cx);
}

fn open_program(
    shell: &mut MeshcraftShell,
    program: Program,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    shader_editor::open(
        shell,
        program,
        shader_editor::Mode::Edit,
        vec![],
        window,
        cx,
    );
}
fn program_overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    shader_editor::overlay(shell, cx)
}
pub(super) fn tick(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> bool {
    if shell.materials.program_editor.is_some() {
        shader_editor::tick(shell, cx)
    } else {
        object_preview::tick(shell, cx)
    }
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
    for field in &editor.parameters {
        let ParameterField {
            parameter: p,
            input,
            choices,
            color,
            ..
        } = field;
        if let Some(value) = values.get(&p.id) {
            input.update(cx, |i, cx| i.set_value(display_value(value), window, cx));
            if let Some(picker) = color
                && let Some(value) = parameter_color(value)
            {
                picker.update(cx, |picker, cx| picker.set_value(value, window, cx));
            }
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

pub(super) fn modal_open(shell: &MeshcraftShell) -> bool {
    shell.materials.editor.is_some() || shell.materials.program_editor.is_some()
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
        shader_editor::close(shell, window, cx);
        cx.stop_propagation();
    }
    true
}

/// Same typed editor for shared prototype defaults and private instance settings.
pub(super) fn edit_tree(
    shell: &mut MeshcraftShell,
    new: bool,
    instance: bool,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    if new {
        new_program(shell, true, window, cx);
        return;
    }
    let path = if instance {
        shell
            .session
            .native_selection_targets()
            .into_iter()
            .find(|t| matches!(t.entity, SelectedEntity::Occurrence))
            .map(|t| t.occurrence)
    } else {
        None
    };
    let definition = path
        .as_ref()
        .and_then(|p| {
            shell
                .cached_document
                .resolve_occurrence(p)
                .map(|r| r.definition.id)
        })
        .or_else(|| shell.session.native_selected_prototype());
    let settings = path
        .as_ref()
        .and_then(|p| shell.cached_document.procedural_object(p))
        .or_else(|| {
            definition.and_then(|id| shell.cached_document.procedural_object_definition(id))
        });
    if let Some(m) = settings {
        edit(shell, m, window, cx);
        shell.materials.editor.as_mut().unwrap().object = Some(ObjectEdit {
            definition,
            instance: path,
        });
        if shell
            .materials
            .editor
            .as_ref()
            .is_some_and(|e| e.preview.is_none())
        {
            object_preview::open(shell, cx);
        }
    } else {
        shell
            .session
            .set_native_status("Select a shader object or its prototype first".into());
    }
}
