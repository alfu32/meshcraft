//! Shell layout copied from tag 0.2.0 (081d012), with live command/session bindings.
//! Reference appearance: docs/full_ui.png. The viewport is owned by gpui_app's offscreen renderer.

use super::{MeshcraftShell, command_enabled, ribbon_command_for_key};

pub(super) const RIGHT_PANEL_WIDTH: f32 = 420.0;
const SELECTION_COUNT_WIDTH: f32 = 68.0;
pub(super) const TITLE_BAR_HEIGHT: f32 = 34.0;
pub(super) const MENU_HEIGHT: f32 = 34.0;
pub(super) const TAB_HEIGHT: f32 = 28.0;
pub(super) const RIBBON_HEIGHT: f32 = 97.0;
pub(super) const HEADER_HEIGHT: f32 = MENU_HEIGHT + TAB_HEIGHT + RIBBON_HEIGHT;
pub(super) const STATUS_HEIGHT: f32 = 26.0;
use gpui::{
    Context, ImageSource, IntoElement, SharedString, Styled, div, img, prelude::*, px, rgb,
};
use gpui_component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
};
use meshcraft_core::{commands::CommandDescriptor, document::Document, selection::SelectionKind};

#[derive(Clone, Default)]
pub(super) struct SceneCounts {
    edges: usize,
    faces: usize,
    objects: usize,
    hotspots: usize,
    semantic: std::collections::BTreeMap<SelectionKind, usize>,
    prototypes: std::collections::BTreeMap<meshcraft_core::document::DefinitionId, usize>,
}

impl SceneCounts {
    pub(super) fn from_document(document: &Document) -> Self {
        let mut counts = Self::default();
        for occurrence in document.scene_geometry_refs().unwrap_or_default() {
            if let Some(r) = document.resolve_occurrence(&occurrence) {
                *counts.prototypes.entry(r.definition.id).or_default() += 1;
                counts.hotspots += document
                    .instance_hotspots(&occurrence)
                    .map_or(0, |h| h.len());
                for (field, kind) in [
                    ("voxels", SelectionKind::Voxel),
                    ("dimensions", SelectionKind::Dimension),
                    ("texts", SelectionKind::Text),
                    ("architectureWalls", SelectionKind::Wall),
                    ("architectureSlabs", SelectionKind::Slab),
                    ("architectureStairs", SelectionKind::Stair),
                    ("architectureFrames", SelectionKind::Frame),
                    ("hvac", SelectionKind::Hvac),
                ] {
                    let count = r
                        .definition
                        .domain_entity_ids
                        .get(field)
                        .map(|ids| ids.len())
                        .or_else(|| {
                            r.definition
                                .domain_data
                                .get(field)
                                .and_then(|v| v.as_array())
                                .map(|v| v.len())
                        })
                        .unwrap_or(0);
                    *counts.semantic.entry(kind).or_default() += count;
                }
            }
            if document.is_object_occurrence(&occurrence) {
                counts.objects += 1;
            }
            if let Some(geometry) = document.effective_geometry(&occurrence) {
                counts.edges += geometry.segments.len();
                counts.faces += geometry.triangles.len();
            }
        }
        counts
    }
}

pub(super) struct LayoutBindings<'view, 'cx, 'app> {
    shell: &'view MeshcraftShell,
    cx: &'cx mut Context<'app, MeshcraftShell>,
    catalog: Vec<CommandDescriptor>,
}

impl<'view, 'cx, 'app> LayoutBindings<'view, 'cx, 'app> {
    pub(super) fn new(
        shell: &'view MeshcraftShell,
        cx: &'cx mut Context<'app, MeshcraftShell>,
    ) -> Self {
        Self {
            shell,
            cx,
            catalog: crate::MeshcraftApp::command_catalog(),
        }
    }

    fn button(
        &mut self,
        namespace: &'static str,
        key: &'static str,
        label: &'static str,
    ) -> Button {
        let command = ribbon_command_for_key(key, &self.catalog);
        let checked = command.is_some_and(|command| {
            self.shell
                .session
                .query_command_state(command.id.as_str())
                .checked
                == Some(true)
        });
        let icon = crate::assets::gpui_icon(key)
            .or_else(|| {
                command
                    .and_then(|command| command.presentation.icon_key.as_deref())
                    .and_then(crate::assets::gpui_icon)
            })
            .or_else(|| command.and_then(|command| crate::assets::gpui_icon(command.id.as_str())));
        let ribbon = namespace.starts_with("ribbon/");
        let functional = namespace.ends_with("/functional");
        let icon_size = (self.shell.session.native_ui_preferences().toolbar_icon_size as f32
            * 0.75)
            .clamp(18., if ribbon { 22. } else { 32. });
        let button = Button::new(SharedString::from(format!("{namespace}/{key}")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .w(px(if functional { 92. } else { 80. }))
                    .min_w(px(0.))
                    .when_some(icon, |row, icon| {
                        let tile = div()
                            .relative()
                            .size(px(icon_size))
                            .flex_none()
                            .child(img(ImageSource::Render(icon)).size(px(icon_size)));
                        row.child(tile.when(checked, |tile| {
                            tile.child(
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .top(px(0.))
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(rgb(0xf45151)),
                            )
                        }))
                    })
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex_1()
                            .text_size(px(11.))
                            .when(!functional, |text| text.truncate())
                            .when(functional, |text| {
                                text.whitespace_normal().line_height(px(12.))
                            })
                            .child(label),
                    ),
            )
            .small()
            .px(px(2.))
            .h(px(if namespace.contains("/home/") {
                22.
            } else if functional {
                30.
            } else if ribbon {
                24.
            } else {
                self.shell.session.native_ui_preferences().toolbar_icon_size as f32
            }))
            .ghost()
            .compact()
            .tooltip(format!("{label} · {key}"));
        if key == "rename-object" {
            return button
                .disabled(self.shell.session.native_active_object().is_none())
                .on_click(self.cx.listener(|this, _, window, cx| {
                    super::settings::focus_object_name(this, window, cx);
                }));
        }
        if key == "apply-model-settings" || key == "reset-model-settings" {
            return button.on_click(self.cx.listener(move |this, _, window, cx| {
                if key == "reset-model-settings" {
                    super::settings::reset_model(this, window, cx);
                } else {
                    super::settings::apply_all(this, window, cx);
                }
                cx.notify();
            }));
        }
        if let Some(command) = ribbon_command_for_key(key, &self.catalog) {
            let id = command.id.as_str().to_owned();
            button
                .selected(
                    self.shell
                        .session
                        .query_command_state(&id)
                        .checked
                        .unwrap_or(false),
                )
                .disabled(!command_enabled(&self.shell.session, command))
                .on_click(self.cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus);
                    this.dispatch_with_clipboard(&id, cx);
                    cx.notify();
                }))
        } else {
            button.disabled(true)
        }
    }

    fn field(&mut self, label: &'static str, fallback: &'static str) -> gpui::AnyElement {
        if let Some(editor) = super::settings::render_field(self.shell, label) {
            return editor;
        }
        let settings = self.shell.cached_document.model_settings();
        let preferences = self.shell.session.native_ui_preferences();
        let model_number = match label {
            "Unit size" => Some(("unit_size", settings.unit_size, 0.1)),
            "Grid size" => Some(("grid_size", settings.grid_size, 0.1)),
            "Circle segments" => {
                Some(("circle_segments", f64::from(settings.circle_segments), 1.0))
            }
            "Snap radius" => Some(("snap_radius", settings.snap_radius, 1.0)),
            "Arc max length" => Some(("arc_max_length", settings.arc_max_length, 0.05)),
            "Double line size" => Some(("double_line_size", settings.double_line_size, 0.1)),
            "Double line offset" => Some(("double_line_offset", settings.double_line_offset, 0.1)),
            _ => None,
        };
        let light = self.shell.lighting;
        let lighting_number = match label {
            "Ambient light" => Some(("ambient", light.ambient, 0.05)),
            "Directional light" => Some(("directional", light.directional, 0.05)),
            "Shadow alpha" => Some(("shadow_strength", light.shadow_strength, 0.05)),
            "Specular value" => Some(("specular", light.specular, 0.05)),
            "Shadow bias" => Some(("depth_bias", light.depth_bias, 0.00005)),
            "Normal bias" => Some(("normal_bias", light.normal_bias, 0.001)),
            "Light azimuth" => Some(("azimuth", light.azimuth, 5.0)),
            "Light elevation" => Some(("elevation", light.elevation, 5.0)),
            _ => None,
        };
        let ui_number = match label {
            "UI text size" => Some((
                "ui_text_scale",
                format!("{:.1}x", preferences.ui_text_scale),
            )),
            "Toolbar size" => Some((
                "toolbar_icon_size",
                format!(
                    "{} × {}",
                    preferences.toolbar_icon_size, preferences.toolbar_icon_size
                ),
            )),
            "Grid normal / extent" => Some((
                "grid_extent",
                format!(
                    "{:?} / {} steps",
                    preferences.grid_normal_axis, preferences.grid_extent
                ),
            )),
            _ => None,
        };
        let value = if let Some((_, number, _)) = model_number {
            if label == "Circle segments" {
                format!("{number:.0}")
            } else {
                format!("{number:.4}")
            }
        } else if let Some((_, number, _)) = lighting_number {
            format!("{number:.5}")
        } else if let Some((_, value)) = &ui_number {
            value.clone()
        } else if label == "Unit name" {
            settings.unit_name.clone()
        } else {
            fallback.to_owned()
        };
        let mut cell = div()
            .h(px(25.))
            .flex()
            .items_center()
            .px(px(7.))
            .border_1()
            .border_color(rgb(0x454b53))
            .bg(rgb(0x292e34))
            .text_sm()
            .text_color(rgb(0x9299a2))
            .child(div().flex_1().child(value));
        if let Some((field, _, step)) = model_number {
            for (suffix, sign) in [("decrease", -1.0), ("increase", 1.0)] {
                cell = cell.child(
                    Button::new(SharedString::from(format!(
                        "settings/model/{field}/{suffix}"
                    )))
                    .label(if sign < 0.0 { "−" } else { "+" })
                    .xsmall()
                    .ghost()
                    .compact()
                    .on_click(self.cx.listener(move |this, _, _, cx| {
                        this.update_model_number(field, sign * step);
                        cx.notify();
                    })),
                );
            }
        } else if let Some((field, _, step)) = lighting_number {
            for (suffix, sign) in [("decrease", -1.0), ("increase", 1.0)] {
                cell = cell.child(
                    Button::new(SharedString::from(format!(
                        "settings/light/{field}/{suffix}"
                    )))
                    .label(if sign < 0.0 { "−" } else { "+" })
                    .xsmall()
                    .ghost()
                    .compact()
                    .on_click(self.cx.listener(move |this, _, _, cx| {
                        this.adjust_lighting(field, sign * step);
                        cx.notify();
                    })),
                );
            }
        } else if let Some((field, _)) = ui_number {
            for (suffix, sign) in [("decrease", -1), ("increase", 1)] {
                cell = cell.child(
                    Button::new(SharedString::from(format!("settings/ui/{field}/{suffix}")))
                        .label(if sign < 0 { "−" } else { "+" })
                        .xsmall()
                        .ghost()
                        .compact()
                        .on_click(self.cx.listener(move |this, _, _, cx| {
                            this.adjust_ui_preference(field, sign);
                            cx.notify();
                        })),
                );
            }
        }
        let body = if label == "Object list" {
            let document = &self.shell.cached_document;
            let prototypes = document
                .definitions()
                .filter(|definition| {
                    definition.id != Document::SCENE_ROOT_DEFINITION_ID
                        && !document.is_frame_definition(definition.id)
                        && !document
                            .scene_container_occurrence()
                            .and_then(|path| document.resolve_occurrence(&path))
                            .is_some_and(|root| root.definition.id == definition.id)
                })
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(prototypes.into_iter().map(|prototype| {
                    let id = prototype.id;
                    let count = self
                        .shell
                        .cached_scene_counts
                        .prototypes
                        .get(&id)
                        .copied()
                        .unwrap_or(0);
                    Button::new(SharedString::from(format!("objects/prototype/{}", id.0)))
                        .justify_start()
                        .when_some(super::thumbnails::image(self.shell, id), |button, image| {
                            button.child(img(image).w(px(78.)).h(px(58.)).flex_none())
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_start()
                                .w_full()
                                .min_w(px(0.))
                                .child(div().truncate().child(prototype.name.clone()))
                                .child(div().text_xs().text_color(rgb(0xa5adb7)).child(format!(
                                        "{} · {} instance(s) · #{}",
                                        if prototype
                                            .extensions
                                            .get("k3d.externalReferenceEnabled")
                                            .and_then(serde_json::Value::as_bool)
                                            == Some(true)
                                        {
                                            "external"
                                        } else if prototype
                                            .extensions
                                            .get(meshcraft_core::operations::objects::GPU_MESH_KEY)
                                            .and_then(serde_json::Value::as_bool)
                                            == Some(true)
                                        {
                                            "GPU mesh"
                                        } else {
                                            "internal"
                                        },
                                        count,
                                        id.0
                                    ))),
                        )
                        .w_full()
                        .h(px(64.))
                        .small()
                        .ghost()
                        .tooltip("Double-click to insert an instance of this prototype")
                        .on_click(
                            self.cx
                                .listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                    this.session.select_native_prototype(id);
                                    if event.click_count() == 2 {
                                        this.session.begin_placing_definition(id);
                                    }
                                    this.refresh_scene();
                                    cx.notify();
                                }),
                        )
                }))
                .into_any_element()
        } else {
            cell.into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .gap(px(3.))
            .child(div().text_xs().text_color(rgb(0xc2c8d0)).child(label))
            .child(body)
            .into_any_element()
    }

    fn check(&mut self, label: &'static str, fallback: bool) -> gpui::AnyElement {
        let preferences = self.shell.session.native_ui_preferences();
        let binding = match label {
            "Show menu bar" => Some(("menu_bar_visible", preferences.menu_bar_visible)),
            "Show ribbon" => Some(("ribbon_visible", preferences.ribbon_visible)),
            "Black model lines" => Some(("black_model_lines", preferences.black_model_lines)),
            "Show left panel" => Some(("left_panel_visible", preferences.left_panel_visible)),
            "Accordion settings panels" => Some((
                "accordion_settings_panels",
                preferences.accordion_settings_panels,
            )),
            "Show permanent grid" => Some(("show_permanent_grid", preferences.show_permanent_grid)),
            "Show construction guides" => Some((
                "show_construction_guides",
                preferences.show_construction_guides,
            )),
            _ => None,
        };
        let checked = if label == "Shadows" {
            self.shell.lighting.shadows
        } else {
            binding.map_or(fallback, |(_, checked)| checked)
        };
        let row = div()
            .id(SharedString::from(format!("settings/check/{label}")))
            .flex()
            .items_center()
            .gap(px(7.))
            .text_sm()
            .text_color(rgb(0xaab2bd))
            .child(
                div()
                    .w(px(14.))
                    .h(px(14.))
                    .border_1()
                    .border_color(rgb(0x646d79))
                    .bg(if checked {
                        rgb(0x3d6388)
                    } else {
                        rgb(0x30363e)
                    }),
            )
            .child(label);
        if label == "Shadows" {
            row.on_mouse_down(
                gpui::MouseButton::Left,
                self.cx.listener(|this, _, _, cx| {
                    this.lighting.shadows = !this.lighting.shadows;
                    this.refresh_scene();
                    cx.notify();
                }),
            )
            .into_any_element()
        } else if let Some((field, _)) = binding {
            row.on_mouse_down(
                gpui::MouseButton::Left,
                self.cx.listener(move |this, _, _, cx| {
                    this.toggle_ui_preference(field);
                    cx.notify();
                }),
            )
            .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    fn menu_check(&mut self, menu: &'static str) -> gpui::AnyElement {
        let visible = self
            .shell
            .session
            .native_ui_preferences()
            .menu_visibility
            .get(menu)
            .copied()
            .unwrap_or(true);
        div()
            .flex_1()
            .min_w(px(0.))
            .child(
                Checkbox::new(SharedString::from(format!("settings/menu/{menu}")))
                    .label(menu)
                    .checked(visible)
                    .on_click(self.cx.listener(move |this, visible: &bool, _, cx| {
                        let mut menus = this.session.native_ui_preferences().menu_visibility;
                        menus.insert(menu.to_owned(), *visible);
                        this.update_ui_preference("menu_visibility", serde_json::json!(menus));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn stat(&self, label: &'static str, fallback: &'static str) -> gpui::AnyElement {
        let value = fallback.to_owned();
        div()
            .flex()
            .justify_between()
            .gap(px(8.))
            .text_sm()
            .text_color(rgb(0xc0c6cf))
            .child(label)
            .child(div().text_color(rgb(0x8f98a4)).child(value))
            .into_any_element()
    }

    fn selection_checkbox(
        &mut self,
        key: &'static str,
        kinds: &[SelectionKind],
        property: &'static str,
    ) -> gpui::AnyElement {
        let policy = self.shell.session.native_selection_display_policy();
        let checked = kinds.iter().all(|kind| {
            let state = policy.state(*kind);
            match property {
                "visible" => state.visible,
                "editable" => state.unlocked,
                _ => state.wireframe,
            }
        });
        let disabled = property == "editable" && kinds.iter().all(|kind| !policy.can_draw(*kind));
        div()
            .w(px(26.))
            .h(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(
                Checkbox::new(SharedString::from(format!("selection/{key}/{property}")))
                    .small()
                    .checked(checked)
                    .disabled(disabled)
                    .on_click(self.cx.listener(move |this, enabled: &bool, _, cx| {
                        if this.architecture.drag.take().is_some()
                            || this.hotspot_drag.take().is_some()
                        {
                            this.cached_document_revision = None;
                        }
                        this.selection_gesture = None;
                        this.selection_cursor = None;
                        let mut payload = meshcraft_core::commands::CommandPayload::default();
                        payload.values.insert("kind".into(), key.into());
                        payload.values.insert("property".into(), property.into());
                        payload.values.insert("enabled".into(), (*enabled).into());
                        this.dispatch_payload("selection.update_display", &payload);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn selection_table(&mut self) -> gpui::AnyElement {
        const ROWS: &[(&str, &str, &[SelectionKind])] = &[
            ("Edges", "edge", &[SelectionKind::Edge]),
            ("Faces", "face", &[SelectionKind::Face]),
            ("Voxels", "voxel", &[SelectionKind::Voxel]),
            ("Hotspots", "hotspot", &[SelectionKind::Hotspot]),
            ("Objects", "object", &[SelectionKind::Object]),
            (
                "Dimensions / Texts",
                "dimensions_texts",
                &[SelectionKind::Dimension, SelectionKind::Text],
            ),
            (
                "Walls / Slabs / Stairs",
                "architecture",
                &[
                    SelectionKind::Wall,
                    SelectionKind::Slab,
                    SelectionKind::Stair,
                    SelectionKind::Frame,
                    SelectionKind::Hvac,
                ],
            ),
        ];
        let mut header = div()
            .flex()
            .items_center()
            .text_xs()
            .text_color(rgb(0xc0c6cf))
            .child(div().flex_1().min_w(px(0.)).child("Type"))
            .child(
                div()
                    .w(px(SELECTION_COUNT_WIDTH))
                    .flex_none()
                    .whitespace_nowrap()
                    .text_right()
                    .child("T"),
            )
            .child(
                div()
                    .w(px(SELECTION_COUNT_WIDTH))
                    .flex_none()
                    .whitespace_nowrap()
                    .text_right()
                    .child("S"),
            );
        for (label, property) in [("V", "visible"), ("E", "editable"), ("W", "wireframe")] {
            header = header.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(label)
                    .child(self.selection_checkbox("all", &SelectionKind::ALL, property)),
            );
        }
        let summary = self.shell.session.native_selection_summary();
        let totals = self.shell.cached_scene_counts.clone();
        let mut table = div().flex().flex_col().gap(px(2.)).child(header);
        for &(label, key, kinds) in ROWS {
            let total = match key {
                "edge" => totals.edges,
                "face" => totals.faces,
                "object" => totals.objects,
                "hotspot" => totals.hotspots,
                _ => kinds
                    .iter()
                    .map(|k| totals.semantic.get(k).copied().unwrap_or(0))
                    .sum(),
            };
            let selected: usize = kinds
                .iter()
                .map(|k| summary.counts_by_kind.get(k).copied().unwrap_or(0))
                .sum();
            let selected = match (key, self.shell.cached_selected_mesh.as_ref()) {
                ("edge", Some((_, _, mesh))) => mesh.edges,
                ("face", Some((_, _, mesh))) => mesh.faces,
                _ => selected,
            };
            let mut row = div()
                .flex()
                .items_center()
                .text_xs()
                .text_color(rgb(0xc0c6cf))
                .child(div().flex_1().min_w(px(0.)).child(label))
                .child(
                    div()
                        .w(px(SELECTION_COUNT_WIDTH))
                        .flex_none()
                        .whitespace_nowrap()
                        .text_right()
                        .child(total.to_string()),
                )
                .child(
                    div()
                        .w(px(SELECTION_COUNT_WIDTH))
                        .flex_none()
                        .whitespace_nowrap()
                        .text_right()
                        .child(selected.to_string()),
                );
            for property in ["visible", "editable", "wireframe"] {
                row = row.child(self.selection_checkbox(key, kinds, property));
            }
            table = table.child(row);
        }
        table.into_any_element()
    }

    fn object_selection(&self) -> String {
        self.shell.session.native_active_object().map_or_else(
            || "No object selected".to_owned(),
            |(_, name, editing)| {
                format!(
                    "{}: {name}",
                    if editing {
                        "Editing prototype"
                    } else if self.shell.session.native_selection_summary().objects > 0 {
                        "Selected instance"
                    } else {
                        "Prototype"
                    }
                )
            },
        )
    }

    fn hotspot_summary(&self) -> gpui::AnyElement {
        let (selected, h) = self.shell.session.native_hotspot_inspector();
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_xs()
            .text_color(rgb(0xa5adb7))
            .child(if selected.is_some() {
                format!("Selected hotspot: {}", h.name)
            } else {
                "Default hotspot settings".into()
            })
            .child(format!(
                "Attached: {} edges · {} faces · {} vertices · {} hotspots",
                h.attached_segments.len(),
                h.attached_triangles.len(),
                h.attached_vertex_ids.len(),
                h.attached_hotspot_ids.len()
            ))
            .child(format!(
                "Reference: {}",
                h.reference_position
                    .map(|p| format!("({:.3}, {:.3}, {:.3})", p.x, p.y, p.z))
                    .unwrap_or_else(|| "none".into())
            ))
            .child(
                if selected.as_ref().is_some_and(|(path, _)| {
                    !self.shell.session.native_hotspot_authoring_allowed(path)
                }) {
                    "Drag this instance's handle. Open its prototype to edit properties."
                } else {
                    "Author shared handles here; operate them after finishing prototype editing."
                },
            )
            .into_any_element()
    }

    fn object_summary(&self) -> String {
        format!(
            "{} object instances in the active scene",
            self.shell.cached_scene_counts.objects
        )
    }
}

pub(super) fn ribbon_groups(tab: &str) -> Vec<&'static crate::functional_ui::Section> {
    crate::functional_ui::ribbon_sections(tab)
}

fn command_row(
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(4.))
        .children(
            commands
                .iter()
                .map(|(command, label)| bindings.button(namespace, command, label)),
        ))
    .into_any_element()
}

fn ribbon_group(
    title: &'static str,
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px((commands
            .len()
            .div_ceil(if namespace.contains("/home/") { 3 } else { 2 })
            as f32
            * 111.)
            + 14.))
        .gap(px(4.))
        .px(px(7.))
        .py(px(4.))
        .border_r_1()
        .border_color(rgb(0x3b4654))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .line_height(px(12.))
                .text_color(rgb(0x9caec3))
                .child(title),
        )
        .child(
            div().flex().flex_wrap().gap(px(3.)).children(
                commands
                    .iter()
                    .map(|(command, label)| bindings.button(namespace, command, label).w(px(108.))),
            ),
        ))
    .into_any_element()
}

pub(super) fn ribbon_content(
    tab: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    let mut ribbon = div()
        .id("ribbon-strip")
        .overflow_x_scroll()
        .h(px(RIBBON_HEIGHT))
        .flex()
        .gap(px(3.))
        .px(px(5.))
        .bg(rgb(0x20262e));
    for (index, group) in ribbon_groups(tab).iter().enumerate() {
        ribbon = ribbon.child(ribbon_group(
            group.title,
            if tab == "Home" {
                "ribbon/home/functional"
            } else {
                "ribbon/functional"
            },
            group.commands,
            bindings,
        ));
        if tab == "Home" && index == 0 {
            ribbon = ribbon
                .child(super::workspace_controls::picker(
                    bindings.shell,
                    false,
                    "home/material",
                ))
                .child(super::workspace_controls::picker(
                    bindings.shell,
                    true,
                    "home/object",
                ));
        }
    }
    if tab == "Materials" {
        ribbon = ribbon.child(super::workspace_controls::picker(
            bindings.shell,
            false,
            "materials/material",
        ));
    }
    ribbon.into_any_element()
}

fn setting_field(
    label: &'static str,
    value: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (bindings.field(label, value)).into_any_element()
}

fn setting_pair(
    left_label: &'static str,
    left_value: &'static str,
    right_label: &'static str,
    right_value: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (div()
        .flex()
        .gap(px(8.))
        .child(setting_field(left_label, left_value, bindings))
        .child(setting_field(right_label, right_value, bindings)))
    .into_any_element()
}

fn setting_check(
    label: &'static str,
    checked: bool,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (bindings.check(label, checked)).into_any_element()
}

fn setting_section(
    title: &'static str,
    controls: impl IntoElement,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    let accordion = bindings
        .shell
        .session
        .native_ui_preferences()
        .accordion_settings_panels;
    let expanded = if accordion {
        bindings.shell.open_settings_panel == Some(title)
    } else {
        !bindings.shell.collapsed_settings_panels.contains(title)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .border_1()
        .border_color(rgb(0x414a55))
        .bg(rgb(0x343a42))
        .child(
            div()
                .id(SharedString::from(format!("settings/section/{title}")))
                .px(px(8.))
                .py(px(5.))
                .bg(rgb(0x24537a))
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0xe0ebf7))
                .cursor_pointer()
                .child(format!("{} {title}", if expanded { "▾" } else { "▸" }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    bindings.cx.listener(move |this, _, _, cx| {
                        if this
                            .session
                            .native_ui_preferences()
                            .accordion_settings_panels
                        {
                            this.open_settings_panel = if this.open_settings_panel == Some(title) {
                                None
                            } else {
                                Some(title)
                            };
                        } else if !this.collapsed_settings_panels.remove(title) {
                            this.collapsed_settings_panels.insert(title);
                        }
                        cx.notify();
                    }),
                ),
        )
        .when(expanded, |section| {
            section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .px(px(8.))
                    .pb(px(8.))
                    .child(controls),
            )
        })
        .into_any_element()
}

fn stat_row(
    label: &'static str,
    value: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (bindings.stat(label, value)).into_any_element()
}

pub(super) fn left_toolbars(bindings: &mut LayoutBindings<'_, '_, '_>) -> gpui::AnyElement {
    let query = bindings.shell.workspace_controls.query.clone();
    let mut content = div()
        .id("toolbar/function-sections")
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(7.));
    let mut count = 0;
    for root in crate::functional_ui::GROUPS {
        for section in crate::functional_ui::sections(root) {
            let commands = section
                .commands
                .iter()
                .filter(|(id, _)| {
                    bindings
                        .catalog
                        .iter()
                        .find(|c| c.id.as_str() == *id)
                        .is_some_and(|c| super::workspace_controls::matches(c, &query))
                })
                .copied()
                .collect::<Vec<_>>();
            if commands.is_empty() {
                continue;
            }
            count += commands.len();
            let heading = if *root == section.title {
                (*root).to_owned()
            } else {
                format!("{root} · {}", section.title)
            };
            content = content
                .child(
                    div()
                        .text_xs()
                        .py(px(4.))
                        .text_color(rgb(0xa5bbce))
                        .child(heading),
                )
                .child(
                    div().flex().flex_wrap().gap(px(4.)).children(
                        commands
                            .iter()
                            .map(|(id, label)| bindings.button("toolbar/functional", id, label)),
                    ),
                );
        }
    }
    if count == 0 {
        content = content.child(div().text_sm().child("No matching tools"));
    }
    div()
        .id("toolbar/left-panel")
        .relative()
        .w(px(bindings.shell.workspace_controls.width))
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(0x1e242c))
        .child(
            div()
                .px(px(7.))
                .pt(px(6.))
                .pr(px(12.))
                .child(super::workspace_controls::search(bindings.shell)),
        )
        .child(content)
        .child(
            div()
                .id("toolbar/resize")
                .absolute()
                .right_0()
                .top_0()
                .h_full()
                .w(px(6.))
                .cursor_col_resize()
                .child(super::workspace_controls::bounds_probe(
                    bindings.shell,
                    "left/resize".into(),
                ))
                .hover(|s| s.bg(rgb(0x58799a)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    bindings
                        .cx
                        .listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            window.focus(&this.focus);
                            super::workspace_controls::start_resize(
                                this,
                                f32::from(event.position.x),
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }),
                )
                .on_mouse_up_out(
                    gpui::MouseButton::Left,
                    bindings.cx.listener(|this, _, _, cx| {
                        super::workspace_controls::finish_resize(this, cx)
                    }),
                ),
        )
        .into_any_element()
}

pub(super) fn right_settings_panel(bindings: &mut LayoutBindings<'_, '_, '_>) -> gpui::AnyElement {
    (div()
        .w(px(RIGHT_PANEL_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(8.))
        .bg(rgb(0x20262e))
        .id("panel/settings-scroll")
        .track_scroll(&bindings.shell.settings_scroll)
        .overflow_y_scroll()
        .child(setting_section(
            "Selection",
            bindings.selection_table(),
            bindings,
        ))
        .child(setting_section(
            "Object Instance",
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xa5adb7))
                        .child(bindings.object_selection()),
                )
                .child(setting_field("Object name", "", bindings))
                .child(super::parameters::panel(bindings.shell, bindings.cx))
                .child(super::ifc::panel(bindings.shell, false, bindings.cx)),
            bindings,
        ))
        .child(setting_section(
            "Object Prototypes",
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xa5adb7))
                        .child(bindings.object_summary()),
                )
                .child(setting_field("Object list", "Empty scene", bindings))
                .child(super::ifc::panel(bindings.shell, true, bindings.cx)),
            bindings,
        ))
        .child(setting_section(
            "Materials",
            super::materials::panel(bindings.shell, bindings.cx),
            bindings,
        ))
        .child(setting_section(
            "Annotations",
            super::annotations::panel(bindings.shell, bindings.cx),
            bindings,
        ))
        .child(setting_section(
            "Model Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Unit name", "unit", bindings))
                .child(setting_pair(
                    "Unit size",
                    "1.0000",
                    "Grid size",
                    "1.0000",
                    bindings,
                ))
                .child(setting_pair(
                    "Circle segments",
                    "24",
                    "Snap radius",
                    "12.0",
                    bindings,
                ))
                .child(setting_pair(
                    "Walk speed",
                    "24.000",
                    "Jump velocity",
                    "16.000",
                    bindings,
                ))
                .child(setting_pair(
                    "Gravity",
                    "48.000",
                    "Height adjust speed",
                    "8.000",
                    bindings,
                ))
                .child(command_row(
                    "panel/model-settings",
                    &[
                        ("apply-model-settings", "Apply Settings"),
                        ("reset-model-settings", "Reset Defaults"),
                    ],
                    bindings,
                )),
            bindings,
        ))
        .child(setting_section(
            "UI Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_check("Black model lines", true, bindings))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xc2c8d0))
                        .child("Off: use line material colors"),
                )
                .child(setting_check("Show menu bar", true, bindings))
                .child(setting_check("Show ribbon", true, bindings))
                .child(setting_check("Show left panel", true, bindings))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xc2c8d0))
                        .child("Dropdown menus"),
                )
                .children(super::MENU_NAMES.chunks(2).map(|menus| {
                    div()
                        .flex()
                        .gap(px(8.))
                        .children(menus.iter().copied().map(|menu| bindings.menu_check(menu)))
                }))
                .child(setting_pair(
                    "UI text size",
                    "1x",
                    "Toolbar size",
                    "32 × 32",
                    bindings,
                ))
                .child(setting_check("Accordion settings panels", false, bindings))
                .child(setting_check("Show construction guides", true, bindings))
                .child(setting_check("Show permanent grid", true, bindings))
                .child(setting_field(
                    "Permanent grid normal",
                    "Y (up, default)",
                    bindings,
                ))
                .child(setting_field("Grid extent", "20", bindings))
                .child(setting_field("Normal line width", "1.0", bindings))
                .child(setting_field("Thick line width", "3.0", bindings)),
            bindings,
        ))
        .child(setting_section(
            "Polyline & Vector Text",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Arc max length",
                    "0.500",
                    "Double line size",
                    "1.000",
                    bindings,
                ))
                .child(setting_pair(
                    "Double line offset",
                    "0.500",
                    "Text size",
                    "1.000",
                    bindings,
                ))
                .child(setting_pair(
                    "Tracking",
                    "0.100",
                    "Line spacing",
                    "1.250",
                    bindings,
                ))
                .child(setting_field("Glyph source", "embedded:alphabet", bindings))
                .child(command_row(
                    "panel/vector-text",
                    &[
                        ("load-glyph-source", "Load Glyphs"),
                        ("browse-glyph-source", "Browse…"),
                    ],
                    bindings,
                )),
            bindings,
        ))
        .child(setting_section(
            "Architecture — Walls / Slabs / Stairs / Frames",
            super::architecture::panel(bindings.shell, bindings.cx),
            bindings,
        ))
        .child(setting_section(
            "HVAC Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Pipe diameter",
                    "0.200",
                    "Pipe sides",
                    "16",
                    bindings,
                ))
                .child(setting_pair(
                    "Duct width",
                    "0.500",
                    "Duct height",
                    "0.250",
                    bindings,
                ))
                .child(setting_pair(
                    "Hump span",
                    "0.625",
                    "Hump clearance",
                    "0.050",
                    bindings,
                ))
                .child(setting_check("Ventilation auto-join", true, bindings))
                .child(setting_field(
                    "Pipe / duct colors",
                    "Default material colors",
                    bindings,
                )),
            bindings,
        ))
        .child(setting_section(
            "Hotspot Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Hotspot name", "", bindings))
                .child(setting_pair(
                    "Operation",
                    "MOVE",
                    "Shape",
                    "CIRCLE",
                    bindings,
                ))
                .child(setting_field("Hotspot color", "338cf2ff", bindings))
                .child(bindings.hotspot_summary()),
            bindings,
        ))
        .child(setting_section(
            "Random Tools & Surface Remesh",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field(
                    "Fuzzy offset strength [0…100]",
                    "1",
                    bindings,
                ))
                .child(setting_pair(
                    "Copy count",
                    "12",
                    "Scale fuzz [0…100]",
                    "0",
                    bindings,
                ))
                .child(setting_pair(
                    "Rotation fuzz [0…100]",
                    "0",
                    "Subdivisions",
                    "12",
                    bindings,
                ))
                .child(setting_check("Align to surface normal", true, bindings))
                .child(setting_field("Projection tolerance", "0.05", bindings)),
            bindings,
        ))
        .child(setting_section(
            "Tutorials",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(stat_row("Mode / step", "Idle · 0 / 0", bindings))
                .child(setting_field(
                    "Tutorial",
                    "01A. Full House Exterior",
                    bindings,
                ))
                .child(setting_field(
                    "Lesson",
                    "01. Lines / Extrude-swipe",
                    bindings,
                ))
                .child(command_row(
                    "panel/tutorials",
                    &[
                        ("record", "Record"),
                        ("stop-recording", "Stop Rec"),
                        ("play", "Play"),
                        ("pause", "Pause"),
                        ("stop", "Stop"),
                    ],
                    bindings,
                )),
            bindings,
        ))
        .child(setting_section(
            "Shader Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Light azimuth",
                    "-35",
                    "Shadow alpha",
                    "0.50",
                    bindings,
                ))
                .child(setting_pair(
                    "Directional light",
                    "0.73",
                    "Ambient light",
                    "0.59",
                    bindings,
                ))
                .child(setting_pair(
                    "Specular value",
                    "0.20",
                    "Light elevation",
                    "55",
                    bindings,
                ))
                .child(setting_pair(
                    "Shadow bias",
                    "2500",
                    "Normal bias",
                    "5620",
                    bindings,
                ))
                .child(setting_field("Antialiasing", "Medium", bindings))
                .child(setting_check("Shadows", true, bindings))
                .child(div().text_xs().child("2048² light-camera map · 3 × 3 PCF")),
            bindings,
        ))
        .child(setting_section(
            "MCP Server",
            super::automation::panel(bindings.shell, bindings.cx),
            bindings,
        ))
        .child(setting_section(
            "Help / About",
            div()
                .flex()
                .flex_col()
                .gap(px(5.))
                .text_sm()
                .text_color(rgb(0xa5adb7))
                .child("Meshcraft")
                .child("Desktop · Commands are headless and UI-independent"),
            bindings,
        )))
    .into_any_element()
}

pub(super) fn panel_location(id: &str) -> Option<(usize, &'static str)> {
    Some(match id {
        "selection" => (0, "Selection"),
        "object" => (1, "Object Instance"),
        "objects" => (2, "Object Prototypes"),
        "materials" => (3, "Materials"),
        "annotations" => (4, "Annotations"),
        "model_settings" => (5, "Model Settings"),
        "ui_settings" | "profile" => (6, "UI Settings"),
        "polyline" | "vector_text" => (7, "Polyline & Vector Text"),
        "architecture_walls"
        | "architecture_slabs"
        | "architecture_stairs"
        | "architecture_frames" => (8, "Architecture — Walls / Slabs / Stairs / Frames"),
        "hvac" => (9, "HVAC Settings"),
        "hotspots" => (10, "Hotspot Settings"),
        "random" => (11, "Random Tools & Surface Remesh"),
        "tutorials" => (12, "Tutorials"),
        "shaders" => (13, "Shader Settings"),
        "mcp" => (14, "MCP Server"),
        "help" => (15, "Help / About"),
        _ => return None,
    })
}

#[cfg(test)]
mod count_tests {
    use super::*;
    use meshcraft_core::{
        document::architecture::{Element, Wall},
        geometry::Vec3,
    };
    #[test]
    fn totals_include_generated_architecture_and_each_placed_instance() {
        let mut d = Document::new();
        let def = d.ensure_scene_root();
        d.put_architecture(
            def,
            None,
            Element::Wall(Wall {
                start: Vec3::ZERO,
                end: Vec3::X * 5.,
                ..Default::default()
            }),
        )
        .unwrap();
        let cube = d
            .add_cuboid("Object", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        d.copy_root_occurrence(&cube, Vec3::Z * 3.).unwrap();
        let counts = SceneCounts::from_document(&d);
        assert_eq!(40, counts.edges);
        assert_eq!(44, counts.faces);
        assert_eq!(2, counts.objects);
        assert_eq!(Some(&1), counts.semantic.get(&SelectionKind::Wall));
    }
}
