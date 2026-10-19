//! Shell layout copied from tag 0.2.0 (081d012), with live command/session bindings.
//! Reference appearance: docs/full_ui.png. The viewport is owned by gpui_app's offscreen renderer.

use super::{MeshcraftShell, command_enabled, ribbon_command_for_key};

pub(super) const LEFT_PANEL_WIDTH: f32 = 230.0;
pub(super) const RIGHT_PANEL_WIDTH: f32 = 330.0;
pub(super) const MENU_HEIGHT: f32 = 34.0;
pub(super) const TAB_HEIGHT: f32 = 28.0;
pub(super) const RIBBON_HEIGHT: f32 = 78.0;
pub(super) const HEADER_HEIGHT: f32 = MENU_HEIGHT + TAB_HEIGHT + RIBBON_HEIGHT;
pub(super) const STATUS_HEIGHT: f32 = 26.0;
use gpui::{Context, IntoElement, SharedString, Styled, div, prelude::*, px, rgb};
use gpui_component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use meshcraft_core::{commands::CommandDescriptor, document::Document};

#[derive(Clone, Copy, Default)]
pub(super) struct SceneCounts {
    edges: usize,
    faces: usize,
    objects: usize,
}

impl SceneCounts {
    pub(super) fn from_document(document: &Document) -> Self {
        let mut counts = Self::default();
        for occurrence in document.scene_geometry_refs().unwrap_or_default() {
            if !occurrence.path.is_empty() {
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
        let button = Button::new(SharedString::from(format!("{namespace}/{key}")))
            .label(label)
            .small()
            .ghost()
            .compact()
            .tooltip(key);
        match key {
            "flow-left-to-right" | "flow-top-to-down" => {
                return button.on_click(self.cx.listener(move |this, _, _, cx| {
                    this.update_ui_preference(
                        "toolbar_flow_top_down",
                        serde_json::json!(key == "flow-top-to-down"),
                    );
                    cx.notify();
                }));
            }
            _ => {}
        }
        if let Some(command) = ribbon_command_for_key(key, &self.catalog) {
            let id = command.id.as_str().to_owned();
            button
                .disabled(!command_enabled(&self.shell.session, command))
                .on_click(self.cx.listener(move |this, _, _, cx| {
                    this.dispatch(&id);
                    cx.notify();
                }))
        } else {
            button.disabled(true)
        }
    }

    fn field(&mut self, label: &'static str, fallback: &'static str) -> gpui::AnyElement {
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
            let occurrences = self.shell.session.native_top_level_occurrences();
            if occurrences.is_empty() {
                cell.into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(occurrences.into_iter().map(|occurrence| {
                        let name = self
                            .shell
                            .cached_document
                            .resolve_occurrence(&occurrence)
                            .map(|resolved| resolved.definition.name.clone())
                            .unwrap_or_else(|| "Object".to_owned());
                        Button::new(SharedString::from(format!("objects/{:?}", occurrence.path)))
                            .label(name)
                            .small()
                            .ghost()
                            .on_click(self.cx.listener(move |this, _, _, cx| {
                                let _ = this
                                    .session
                                    .select_native_object_from_inspector(&occurrence, false);
                                this.refresh_scene();
                                cx.notify();
                            }))
                    }))
                    .into_any_element()
            }
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
            "Auto-collapse toolbars" => {
                Some(("auto_collapse_toolbars", preferences.auto_collapse_toolbars))
            }
            "Show permanent grid" => Some(("show_permanent_grid", preferences.show_permanent_grid)),
            _ => None,
        };
        let checked = binding.map_or(fallback, |(_, checked)| checked);
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
        if let Some((field, _)) = binding {
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

    fn stat(&self, label: &'static str, fallback: &'static str) -> gpui::AnyElement {
        let selection = self.shell.session.native_selection_summary();
        let totals = self.shell.cached_scene_counts;
        let value = match label {
            "Edges" => format!("{}    {}    ✓    ✓    □", totals.edges, selection.edges),
            "Faces" => format!("{}    {}    ✓    ✓    □", totals.faces, selection.faces),
            "Objects" => format!("{}    {}    ✓    ✓    □", totals.objects, selection.objects),
            _ => fallback.to_owned(),
        };
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

    fn object_selection(&self) -> String {
        let selected = self.shell.session.native_selection_summary().objects;
        if selected == 0 {
            "No object selected".to_owned()
        } else {
            format!("{selected} object(s) selected")
        }
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
            "Export…",
            "Recent Projects",
            "Exit",
        ],
        "Edit" => &[
            "Undo",
            "Redo",
            "Cut",
            "Copy",
            "Paste",
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
            ("copy", "Copy"),
            ("rotate-3d", "Rotate 3D"),
            ("scale", "Scale"),
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
            ("slab", "Slab"),
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
            ("slab", "Slab"),
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
            ("copy", "Copy"),
            ("rotate-3d", "Rotate 3D"),
            ("rotate-copy-3d", "Rotate Copy"),
            ("scale", "Scale"),
            ("stretch", "Stretch"),
            ("extrude", "Extrude"),
            ("mirror", "Mirror"),
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
            ("slab", "Slab"),
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
            ("permanent-grid", "Grid"),
            ("guides", "Guides"),
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
            ("arrange-toolbars", "Arrange Toolbars"),
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
    (div()
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
                    .map(|(command, label)| bindings.button(namespace, command, label).w(px(66.))),
            ),
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
        .flex_1()
        .min_w(px(0.))
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
                    .map(|(command, label)| bindings.button(namespace, command, label).w(px(76.))),
            ),
        ))
    .into_any_element()
}

pub(super) fn ribbon_content(
    tab: &'static str,
    bindings: &mut LayoutBindings<'_, '_, '_>,
) -> gpui::AnyElement {
    (div()
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

fn setting_section(title: &'static str, controls: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .border_1()
        .border_color(rgb(0x414a55))
        .bg(rgb(0x343a42))
        .child(
            div()
                .px(px(8.))
                .py(px(5.))
                .bg(rgb(0x24537a))
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0xe0ebf7))
                .child(title),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .px(px(8.))
                .pb(px(8.))
                .child(controls),
        )
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
                ("slab", "Slab"),
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
                ("slab", "Slab"),
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
        .overflow_y_scroll()
        .child(setting_section(
            "Selection",
            div().flex().flex_col().gap(px(4.)).children([
                stat_row(
                    "Type                         T    S    D    M    W",
                    "",
                    bindings,
                ),
                stat_row("Edges", "0    0    ✓    ✓    □", bindings),
                stat_row("Faces", "0    0    ✓    ✓    □", bindings),
                stat_row("Voxels", "0    0    ✓    ✓    □", bindings),
                stat_row("Hotspots", "0    0    ✓    ✓    □", bindings),
                stat_row("Objects", "0    0    ✓    ✓    □", bindings),
                stat_row("Dimensions / Texts", "0    0    ✓    ✓    □", bindings),
                stat_row("Walls / Slabs / Stairs", "0    0    ✓    ✓    □", bindings),
            ]),
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
                .child(setting_field("Name", "", bindings))
                .child(command_row(
                    "panel/object",
                    &[
                        ("rename-object", "Rename"),
                        ("edit-definition", "Edit Definition"),
                    ],
                    bindings,
                )),
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
                    ],
                    bindings,
                )),
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
        ))
        .child(setting_section(
            "UI Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "UI text size",
                    "1x",
                    "Toolbar size",
                    "32 × 32",
                    bindings,
                ))
                .child(setting_check("Auto-collapse toolbars", false, bindings))
                .child(setting_check("Show permanent grid", true, bindings))
                .child(setting_field(
                    "Grid normal / extent",
                    "Y up / 20 steps",
                    bindings,
                ))
                .child(command_row(
                    "panel/ui-settings",
                    &[
                        ("flow-left-to-right", "Flow L→R"),
                        ("flow-top-to-down", "Flow T→D"),
                    ],
                    bindings,
                )),
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
        ))
        .child(setting_section(
            "Architecture — Walls / Slabs / Stairs / Frames",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Wall thickness",
                    "0.200",
                    "Wall height",
                    "2.700",
                    bindings,
                ))
                .child(setting_pair(
                    "Slab thickness",
                    "0.200",
                    "Stair height / steps",
                    "2.700 / 14",
                    bindings,
                ))
                .child(setting_pair(
                    "Frame depth",
                    "0.120",
                    "Frame width",
                    "0.060",
                    bindings,
                ))
                .child(setting_check(
                    "Stair left and right rail grids",
                    true,
                    bindings,
                ))
                .child(setting_field(
                    "Material colors",
                    "Exterior / interior / glazing",
                    bindings,
                )),
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
        ))
        .child(setting_section(
            "Hotspot Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Name", "", bindings))
                .child(setting_pair(
                    "Operation",
                    "MOVE",
                    "Shape",
                    "CIRCLE",
                    bindings,
                ))
                .child(setting_field("Color", "338cf2ff", bindings))
                .child(stat_row("Attached geometry", "0 edges · 0 faces", bindings))
                .child(stat_row("Reference", "none", bindings))
                .child(command_row(
                    "panel/hotspots",
                    &[
                        ("attach-selection", "Attach"),
                        ("select-attached", "Select Attached"),
                        ("pick-reference", "Reference"),
                        ("add-hotspot", "Add At Cursor"),
                        ("delete-hotspot", "Delete"),
                    ],
                    bindings,
                )),
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
        ))
        .child(setting_section(
            "Shader Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Shadow value",
                    "0.59",
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
                    "Specular alpha",
                    "0.95",
                    bindings,
                ))
                .child(setting_pair(
                    "Shadow bias",
                    "2500",
                    "Normal bias",
                    "5620",
                    bindings,
                ))
                .child(setting_check("Use CSM", true, bindings))
                .child(setting_check("Dither shadows", false, bindings)),
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
        )))
    .into_any_element()
}
