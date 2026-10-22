//! Shell layout copied from tag 0.2.0 (081d012), with live command/session bindings.
//! Reference appearance: docs/full_ui.png. The viewport is owned by gpui_app's offscreen renderer.

use super::{MeshcraftShell, command_enabled, ribbon_command_for_key};

pub(super) const LEFT_PANEL_WIDTH: f32 = 230.0;
pub(super) const RIGHT_PANEL_WIDTH: f32 = 330.0;
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
            if !occurrence.path.is_empty()
                && document.scene_container_occurrence().as_ref() != Some(&occurrence)
            {
                counts.objects += 1;
            }
            if let Some(geometry) = document.effective_geometry(&occurrence) {
                counts.edges += geometry
                    .segments
                    .values()
                    .filter(|e| !meshcraft_core::document::architecture::generated(&e.extensions))
                    .count();
                counts.faces += geometry
                    .triangles
                    .values()
                    .filter(|e| !meshcraft_core::document::architecture::generated(&e.extensions))
                    .count();
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
        let icon_size = (self.shell.session.native_ui_preferences().toolbar_icon_size as f32
            * 0.75)
            .clamp(18., if ribbon { 22. } else { 32. });
        let button = Button::new(SharedString::from(format!("{namespace}/{key}")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .w(px(80.))
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
                            .truncate()
                            .child(label),
                    ),
            )
            .small()
            .px(px(2.))
            .h(px(if ribbon {
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
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            Button::new("scripts/new")
                                .label("New Scripted Object")
                                .small()
                                .on_click(self.cx.listener(|this, _, window, cx| {
                                    super::scripts::edit(this, true, window, cx);
                                })),
                        )
                        .child(
                            Button::new("scripts/edit")
                                .label("Edit Script")
                                .small()
                                .on_click(self.cx.listener(|this, _, window, cx| {
                                    super::scripts::edit(this, false, window, cx);
                                })),
                        ),
                )
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
                                        } else {
                                            "internal"
                                        },
                                        count,
                                        id.0
                                    ))),
                        )
                        .w_full()
                        .h(px(46.))
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
            .child(div().w(px(32.)).text_right().child("T"))
            .child(div().w(px(32.)).text_right().child("S"));
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
            let mut row = div()
                .flex()
                .items_center()
                .text_xs()
                .text_color(rgb(0xc0c6cf))
                .child(div().flex_1().min_w(px(0.)).child(label))
                .child(div().w(px(32.)).text_right().child(total.to_string()))
                .child(div().w(px(32.)).text_right().child(selected.to_string()));
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

pub(super) fn menu_command_key(label: &str) -> String {
    match label {
        "Planar Mesh" => "closed-mesh",
        "2-Line" => "double-line",
        "Mesh 2-Line" => "meshed-double-line",
        "Object" => "create-object",
        "Linear Dimension" => "dimension",
        "Place Instance" => "place-object",
        "Real-Time Render" => "realtime-render",
        "Save Image" => "save-render",
        _ => return label.trim_end_matches('…').to_lowercase().replace(' ', "-"),
    }
    .to_owned()
}

pub(super) fn menu_items(menu: &str) -> &'static [&'static str] {
    match menu {
        "File" => &[
            "New",
            "Open…",
            "Save",
            "Save As…",
            "Import Objects…",
            "Import Mesh…",
            "Export Mesh…",
            "Export IFC…",
            "Recent Files",
            "Exit",
        ],
        "Edit" => &[
            "Undo",
            "Redo",
            "Cut",
            "Copy",
            "Paste",
            "Paste in Place",
            "Delete",
            "Select All",
        ],
        "View" => &[
            "Orbit",
            "Pan",
            "Zoom",
            "Fit Model",
            "Projection",
            "Permanent Grid",
            "Guides",
        ],
        "Insert" => &[
            "Point",
            "Line",
            "Polyline",
            "Planar Mesh",
            "Rectangle",
            "Circle",
            "Object",
            "Text",
        ],
        "Format" => &[
            "Model Settings",
            "UI Settings",
            "Material Color",
            "Dimension Style",
        ],
        "Draw" => &[
            "Line",
            "Construction Polyline",
            "2-Line",
            "Mesh 2-Line",
            "Rectangle",
            "Circle",
            "Arc",
            "Hotspot",
        ],
        "Dimension" => &[
            "Linear Dimension",
            "Aligned Dimension",
            "Angular Dimension",
            "Radius",
            "Diameter",
        ],
        "Modify" => &[
            "Move",
            "Copy",
            "Rotate 3D",
            "Scale",
            "Squash",
            "Stretch",
            "Extrude",
            "Mirror",
            "Offset",
            "Sweep",
        ],
        "Objects" => &[
            "Make Object",
            "Edit Definition",
            "Place Instance",
            "Explode Object",
        ],
        "Tools" => &[
            "Select",
            "Window Select",
            "Snap Settings",
            "Linear Array",
            "Planar Array",
            "Spatial Array",
            "Distance",
        ],
        "Render" => &[
            "Real-Time Render",
            "Global Illumination",
            "Stop Render",
            "Save Image",
        ],
        "Window" => &[
            "Selection Panel",
            "Object Panel",
            "Settings Panel",
            "Reset Layout",
        ],
        "Help" => &["Tutorials", "Keyboard Shortcuts", "About Meshcraft"],
        _ => &[],
    }
}

#[derive(Clone, Copy)]
pub(super) struct RibbonGroup {
    pub(super) title: &'static str,
    pub(super) namespace: &'static str,
    pub(super) commands: &'static [(&'static str, &'static str)],
}

const HOME_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Construction",
        namespace: "ribbon/home/construction",
        commands: &[
            ("point", "Point"),
            ("line", "Line"),
            ("polyline", "Polyline"),
            ("double-line", "2-Line"),
            ("meshed-double-line", "Mesh 2-Line"),
            ("closed-mesh", "Planar Mesh"),
            ("rectangle", "Rectangle"),
            ("circle", "Circle"),
        ],
    },
    RibbonGroup {
        title: "Modify Selection",
        namespace: "ribbon/home/modify",
        commands: &[
            ("select", "Select"),
            ("window-select", "Window"),
            ("move", "Move"),
            ("move-copy", "Move Copy"),
            ("rotate-3d", "Rotate 3D"),
            ("scale", "Scale"),
            ("squash", "Squash"),
            ("stretch", "Stretch"),
            ("extrude", "Extrude"),
        ],
    },
    RibbonGroup {
        title: "Arrays & Objects",
        namespace: "ribbon/home/arrays",
        commands: &[
            ("make-object", "Make Object"),
            ("edit-object", "Edit Object"),
            ("linear-array", "Linear"),
            ("planar-array", "Planar"),
            ("spatial-array", "Spatial"),
            ("rotational-array", "Rotate"),
            ("helical-array", "Helical"),
        ],
    },
    RibbonGroup {
        title: "Entities & View",
        namespace: "ribbon/home/entities",
        commands: &[
            ("dimension", "Dimension"),
            ("wall", "Wall"),
            ("slab", "Rectangular Slab"),
            ("freeform-slab", "Freeform Slab"),
            ("add-hole", "Add Hole"),
            ("stair", "Stair"),
            ("frame", "Frame"),
            ("orbit", "Orbit"),
            ("zoom", "Zoom"),
        ],
    },
];

const CONSTRUCTION_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Point Construction",
        namespace: "ribbon/construction/points",
        commands: &[
            ("point", "Point"),
            ("line", "Line"),
            ("construction-polyline", "Polyline"),
            ("double-line", "2-Line"),
            ("meshed-double-line", "Mesh 2-Line"),
            ("closed-mesh", "Closed Mesh"),
            ("rectangle", "Rectangle"),
            ("circle", "Circle"),
            ("arc", "Arc"),
            ("hotspot", "Hotspot"),
        ],
    },
    RibbonGroup {
        title: "Entity Construction",
        namespace: "ribbon/construction/entities",
        commands: &[
            ("dimension", "Dimension"),
            ("text", "Text"),
            ("leader", "Leader"),
            ("wall", "Wall"),
            ("slab", "Rectangular Slab"),
            ("freeform-slab", "Freeform Slab"),
            ("add-hole", "Add Hole"),
            ("stair", "Stair"),
            ("frame", "Frame"),
        ],
    },
    RibbonGroup {
        title: "Primitives & Services",
        namespace: "ribbon/construction/primitives",
        commands: &[
            ("cuboid", "Cuboid"),
            ("sphere", "Sphere"),
            ("cylinder", "Cylinder"),
            ("cone", "Cone"),
            ("voxel", "Voxel"),
            ("pipe", "Pipe"),
            ("duct", "Duct"),
        ],
    },
];

const MODIFY_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Selection",
        namespace: "ribbon/modify/selection",
        commands: &[
            ("select", "Select"),
            ("window-select", "Window"),
            ("select-connected", "Connected"),
            ("select-coplanar", "Coplanar"),
            ("select-all", "Select All"),
        ],
    },
    RibbonGroup {
        title: "Transforms",
        namespace: "ribbon/modify/transforms",
        commands: &[
            ("move", "Move"),
            ("move-copy", "Move Copy"),
            ("rotate-3d", "Rotate 3D"),
            ("rotate-copy-3d", "Rotate Copy"),
            ("scale", "Scale"),
            ("squash", "Squash"),
            ("stretch", "Stretch"),
            ("extrude", "Extrude"),
            ("mirror", "Mirror"),
            ("flip", "Flip"),
        ],
    },
    RibbonGroup {
        title: "Surface & Path",
        namespace: "ribbon/modify/surface",
        commands: &[
            ("offset", "Offset"),
            ("sweep", "Sweep"),
            ("fillet", "Fillet"),
            ("chamfer", "Chamfer"),
            ("planar-regularize", "Regularize"),
            ("surface-remesh", "Remesh"),
        ],
    },
];

const ARRAY_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Selection Arrays",
        namespace: "ribbon/arrays/spread",
        commands: &[
            ("linear-array", "Linear Spread"),
            ("planar-array", "Planar Spread"),
            ("spatial-array", "Spatial Spread"),
        ],
    },
    RibbonGroup {
        title: "Rotational Arrays",
        namespace: "ribbon/arrays/rotational",
        commands: &[
            ("rotational-array", "Rotational"),
            ("helical-array", "Helical"),
        ],
    },
    RibbonGroup {
        title: "Objects",
        namespace: "ribbon/arrays/objects",
        commands: &[
            ("make-object", "Make Object"),
            ("edit-object", "Edit Object"),
            ("place-object", "Place Instance"),
            ("explode-object", "Explode"),
        ],
    },
];

const ARCHITECTURE_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Architecture",
        namespace: "ribbon/architecture/building",
        commands: &[
            ("wall", "Wall"),
            ("slab", "Rectangular Slab"),
            ("freeform-slab", "Freeform Slab"),
            ("add-hole", "Add Hole"),
            ("stair", "Stair"),
            ("frame", "Frame"),
            ("door", "Door"),
            ("window", "Window"),
        ],
    },
    RibbonGroup {
        title: "Building Services",
        namespace: "ribbon/architecture/services",
        commands: &[
            ("pipe", "Pipe"),
            ("duct", "Duct"),
            ("voxel", "Voxel"),
            ("dimension", "Dimension"),
            ("text", "Text"),
        ],
    },
];

const INSPECT_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Inspect",
        namespace: "ribbon/inspect/measure",
        commands: &[
            ("distance", "Distance"),
            ("dimension", "Dimension"),
            ("selection-info", "Selection Info"),
            ("object-info", "Object Info"),
        ],
    },
    RibbonGroup {
        title: "Selection",
        namespace: "ribbon/inspect/selection",
        commands: &[
            ("select", "Select"),
            ("window-select", "Window"),
            ("select-connected", "Connected"),
            ("select-coplanar", "Coplanar"),
        ],
    },
    RibbonGroup {
        title: "Model",
        namespace: "ribbon/inspect/model",
        commands: &[
            ("objects", "Objects"),
            ("hotspots", "Hotspots"),
            ("walls", "Walls"),
            ("dimensions", "Dimensions"),
        ],
    },
];

const VIEW_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Camera",
        namespace: "ribbon/view/camera",
        commands: &[
            ("orbit", "Orbit"),
            ("pan", "Pan"),
            ("zoom", "Zoom"),
            ("fit-model", "Fit Model"),
            ("reset-camera", "Reset"),
        ],
    },
    RibbonGroup {
        title: "Projection & Grid",
        namespace: "ribbon/view/grid",
        commands: &[
            ("perspective", "Perspective"),
            ("orthographic", "Orthographic"),
            ("walkthrough", "Walkthrough"),
            ("permanent-grid", "Grid"),
            ("guides", "Guides"),
            ("grid-guide", "3D Grid"),
            ("axis-guide", "3D Guide"),
            ("snap-settings", "Snapping"),
        ],
    },
    RibbonGroup {
        title: "Rendering",
        namespace: "ribbon/view/render",
        commands: &[
            ("realtime-render", "RT"),
            ("global-illumination", "GI"),
            ("stop-render", "Stop"),
            ("save-render", "Save Image"),
        ],
    },
];

const SETTINGS_GROUPS: &[RibbonGroup] = &[
    RibbonGroup {
        title: "Model Settings",
        namespace: "ribbon/settings/model",
        commands: &[
            ("unit-name", "Unit Name"),
            ("unit-size", "Unit Size"),
            ("grid-size", "Grid Size"),
            ("circle-segments", "Circle Segments"),
            ("snap-radius", "Snap Radius"),
        ],
    },
    RibbonGroup {
        title: "Interface",
        namespace: "ribbon/settings/ui",
        commands: &[
            ("ui-text-size", "Text Size"),
            ("toolbar-size", "Toolbar Size"),
            ("permanent-grid", "Permanent Grid"),
            ("mcp-settings", "MCP Server"),
        ],
    },
    RibbonGroup {
        title: "Domain Settings",
        namespace: "ribbon/settings/domain",
        commands: &[
            ("architecture-settings", "Architecture"),
            ("hvac-settings", "HVAC"),
            ("hotspot-settings", "Hotspots"),
            ("shader-settings", "Shaders"),
        ],
    },
];

pub(super) fn ribbon_groups(tab: &str) -> &'static [RibbonGroup] {
    match tab {
        "Construction" => CONSTRUCTION_GROUPS,
        "Modify" => MODIFY_GROUPS,
        "Arrays" => ARRAY_GROUPS,
        "Architecture" => ARCHITECTURE_GROUPS,
        "Inspect" => INSPECT_GROUPS,
        "View" => VIEW_GROUPS,
        "Settings" => SETTINGS_GROUPS,
        _ => HOME_GROUPS,
    }
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

fn palette(
    title: &'static str,
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .p(px(7.))
        .border_1()
        .border_color(rgb(0x3b4654))
        .bg(rgb(0x242b34))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgb(0xc4d6eb))
                .child(title),
        )
        .child(
            div().flex().flex_wrap().gap(px(3.)).children(
                commands
                    .iter()
                    .map(|(command, label)| bindings.button(namespace, command, label).w(px(96.))),
            ),
        )
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
        .w(px((commands.len().div_ceil(2) as f32 * 99.) + 14.))
        .gap(px(4.))
        .px(px(7.))
        .py(px(4.))
        .border_r_1()
        .border_color(rgb(0x3b4654))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0x9caec3))
                .child(title),
        )
        .child(
            div().flex().flex_wrap().gap(px(3.)).children(
                commands
                    .iter()
                    .map(|(command, label)| bindings.button(namespace, command, label).w(px(96.))),
            ),
        ))
    .into_any_element()
}

pub(super) fn ribbon_content(
    tab: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (div()
        .id("ribbon-strip")
        .overflow_x_scroll()
        .h(px(RIBBON_HEIGHT))
        .flex()
        .gap(px(3.))
        .px(px(5.))
        .bg(rgb(0x20262e))
        .children(
            ribbon_groups(tab)
                .iter()
                .map(|group| ribbon_group(group.title, group.namespace, group.commands, bindings)),
        ))
    .into_any_element()
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
    (div()
        .w(px(LEFT_PANEL_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .p(px(7.))
        .bg(rgb(0x1e242c))
        .id("toolbar/palettes-scroll")
        .overflow_y_scroll()
        .child(palette(
            "Point Construction",
            "toolbar/point-construction",
            &[
                ("point", "Point"),
                ("line", "Line"),
                ("polyline", "Pline"),
                ("double-line", "2-Line"),
                ("meshed-double-line", "Mesh 2L"),
                ("closed-mesh", "Planar Mesh"),
                ("rectangle", "Rect"),
                ("circle", "Circle"),
                ("arc", "Arc"),
                ("hotspot", "Hotspot"),
            ],
            bindings,
        ))
        .child(palette(
            "Actions",
            "toolbar/actions",
            &[
                ("open", "Open"),
                ("save", "Save"),
                ("undo", "Undo"),
                ("redo", "Redo"),
                ("copy", "Copy"),
                ("delete", "Delete"),
                ("create-object", "Object"),
                ("explode", "Explode"),
                ("paste", "Paste"),
                ("paste-in-place", "In Place"),
                ("cut", "Cut"),
                ("copy-as-script", "Copy Script"),
                ("array", "Array"),
            ],
            bindings,
        ))
        .child(palette(
            "Modification",
            "toolbar/modification",
            &[
                ("select", "Select"),
                ("move", "Move"),
                ("rotate-3d", "Rotate 3D"),
                ("rotate-copy", "Rotate Copy"),
                ("scale", "Scale"),
                ("squash", "Squash"),
                ("stretch", "Stretch"),
                ("extrude", "Extrude"),
                ("mirror", "Mirror"),
                ("offset", "Offset"),
                ("sweep", "Sweep"),
                ("fillet", "Fillet"),
                ("chamfer", "Chamfer"),
            ],
            bindings,
        ))
        .child(palette(
            "Entity Construction",
            "toolbar/entity-construction",
            &[
                ("dimension", "Dim"),
                ("text", "Text"),
                ("leader", "Leader"),
                ("wall", "Wall"),
                ("slab", "Rectangular Slab"),
                ("freeform-slab", "Freeform Slab"),
                ("add-hole", "Add Hole"),
                ("stair", "Stair"),
                ("frame", "Frame"),
                ("voxel", "Voxel"),
                ("pipe", "Pipe"),
                ("duct", "Duct"),
            ],
            bindings,
        ))
        .child(palette(
            "Rendering",
            "toolbar/rendering",
            &[
                ("ray-trace", "RT"),
                ("global-illumination", "GI"),
                ("stop-render", "Stop"),
                ("save-render", "Save"),
            ],
            bindings,
        ))
        .child(palette(
            "Architecture",
            "toolbar/architecture",
            &[
                ("wall", "Wall"),
                ("slab", "Rectangular Slab"),
                ("freeform-slab", "Freeform Slab"),
                ("add-hole", "Add Hole"),
                ("stair", "Stair"),
                ("frame", "Frame"),
                ("door", "Door"),
                ("window", "Window"),
            ],
            bindings,
        ))
        .child(palette(
            "Camera",
            "toolbar/camera",
            &[
                ("reset-camera", "Reset"),
                ("front-view", "Front"),
                ("top-view", "Top"),
                ("orbit", "Orbit"),
                ("pan", "Pan"),
                ("zoom", "Zoom"),
            ],
            bindings,
        ))
        .child(palette(
            "Modification 2",
            "toolbar/modification-2",
            &[
                ("planar-regularize", "Regularize"),
                ("surface-remesh", "Remesh"),
                ("random-offset", "Fuzzy Offset"),
                ("surface-array", "Surface Array"),
                ("mesh-intersection", "Intersect"),
            ],
            bindings,
        ))
        .child(palette(
            "Mech",
            "toolbar/mech",
            &[
                ("beam", "Beam"),
                ("circle-profile", "Circle"),
                ("ring-profile", "Ring"),
                ("spring", "Spring"),
            ],
            bindings,
        ))
        .child(palette(
            "Spatial Arrays",
            "toolbar/arrays",
            &[
                ("linear-array", "Linear"),
                ("planar-array", "Planar"),
                ("spatial-array", "Spatial"),
                ("rotational-array", "Rotate"),
                ("helical-array", "Helical"),
            ],
            bindings,
        ))
        .child(palette(
            "Primitives",
            "toolbar/primitives",
            &[
                ("sphere", "Sphere"),
                ("cylinder", "Cylinder"),
                ("cone", "Cone"),
                ("tube", "Tube"),
            ],
            bindings,
        ))
        .child(palette(
            "Voxel",
            "toolbar/voxel",
            &[
                ("voxel", "Voxel"),
                ("voxel-fill", "Fill"),
                ("voxel-edit", "Edit"),
            ],
            bindings,
        ))
        .child(palette(
            "In-tool Operators",
            "toolbar/in-tool-operators",
            &[
                ("select-operator", "Select"),
                ("distance-operator", "Distance"),
            ],
            bindings,
        ))
        .child(palette(
            "HVAC",
            "toolbar/hvac",
            &[("plumbing", "Plumbing"), ("ventilation", "Ventilation")],
            bindings,
        )))
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
            "Object",
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
                .child(command_row(
                    "panel/object",
                    &[
                        ("rename-object", "Rename"),
                        ("edit-definition", "Edit Definition"),
                        ("exit-object", "Finish Editing"),
                    ],
                    bindings,
                )),
            bindings,
        ))
        .child(setting_section(
            "Objects",
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
                .child(command_row(
                    "panel/objects",
                    &[
                        ("create-object", "Create Object"),
                        ("import-objects", "Import Objects"),
                        ("reference-object", "Reference Scene"),
                        ("refresh-object", "Refresh Reference"),
                        ("place-object", "Place Instance"),
                    ],
                    bindings,
                )),
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
                .child(bindings.hotspot_summary())
                .child(command_row(
                    "panel/hotspots",
                    &[
                        ("attach-selection", "Attach"),
                        ("select-attached", "Select Attached"),
                        ("pick-reference", "Reference"),
                        ("clear-reference", "Clear Reference"),
                        ("add-hotspot", "Add At Cursor"),
                        ("delete-hotspot", "Delete"),
                    ],
                    bindings,
                )),
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
        "object" => (1, "Object"),
        "objects" => (2, "Objects"),
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
