use crate::assets::{self, IconRegion};
use crate::viewport::{CuboidPreview, GridStyle, ToolPreview, Viewport, ViewportScene};
use eframe::egui::{self, Align, Align2, Color32, Layout, Vec2};
use meshcraft_core::document::{Document, OccurrenceRef};
use meshcraft_core::geometry::Vec3;
use meshcraft_core::selection::{SelectedEntity, SelectionSession, SelectionTarget};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

const PREFERENCES_KEY: &str = "meshcraft.preferences";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum UiProfile {
    #[default]
    DesktopMenus,
    Buttons,
    Hybrid,
}

impl UiProfile {
    fn label(self) -> &'static str {
        match self {
            Self::DesktopMenus => "Desktop Menus",
            Self::Buttons => "Buttons",
            Self::Hybrid => "Hybrid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShellAction {
    SelectTool(&'static str),
    AddCuboid,
    CopySelection,
    CreateObject,
    ArraySelection,
    NewDocument,
    SetProfile(UiProfile),
    ToggleToolbars,
    TogglePanels,
    ToggleGrid,
    About,
}

#[derive(Clone, Copy, Debug)]
struct ShellCommand {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    keywords: &'static str,
    icon: Option<&'static str>,
    action: ShellAction,
}

const SHELL_COMMANDS: &[ShellCommand] = &[
    ShellCommand {
        id: "tool.select",
        name: "Tool> Select",
        category: "Tools",
        keywords: "selection pointer",
        icon: Some("select"),
        action: ShellAction::SelectTool("Select"),
    },
    ShellCommand {
        id: "tool.cuboid",
        name: "Tool> Cuboid",
        category: "Tools",
        keywords: "box primitive draw solid",
        icon: None,
        action: ShellAction::SelectTool("Cuboid"),
    },
    ShellCommand {
        id: "geometry.add_cuboid",
        name: "Draw> Add Cuboid at Origin",
        category: "Draw",
        keywords: "box primitive create solid",
        icon: None,
        action: ShellAction::AddCuboid,
    },
    ShellCommand {
        id: "edit.copy_selection",
        name: "Edit> Copy Selection",
        category: "Edit",
        keywords: "duplicate clone",
        icon: None,
        action: ShellAction::CopySelection,
    },
    ShellCommand {
        id: "object.create_from_selection",
        name: "Objects> Create Object from Selection",
        category: "Objects",
        keywords: "group prototype",
        icon: None,
        action: ShellAction::CreateObject,
    },
    ShellCommand {
        id: "object.array_selection",
        name: "Objects> Array Selected Object",
        category: "Objects",
        keywords: "repeat copies distribute",
        icon: None,
        action: ShellAction::ArraySelection,
    },
    ShellCommand {
        id: "file.new",
        name: "File> New Document",
        category: "File",
        keywords: "clear reset",
        icon: None,
        action: ShellAction::NewDocument,
    },
    ShellCommand {
        id: "tool.line",
        name: "Tool> Line",
        category: "Tools",
        keywords: "draw segment",
        icon: Some("construction_line"),
        action: ShellAction::SelectTool("Line"),
    },
    ShellCommand {
        id: "tool.rectangle",
        name: "Tool> Rectangle",
        category: "Tools",
        keywords: "draw shape",
        icon: Some("rectangle"),
        action: ShellAction::SelectTool("Rectangle"),
    },
    ShellCommand {
        id: "view.orbit",
        name: "View> Orbit",
        category: "View",
        keywords: "camera rotate",
        icon: Some("camera_rotating_operation"),
        action: ShellAction::SelectTool("Orbit"),
    },
    ShellCommand {
        id: "view.profile.menus",
        name: "View> Desktop Menus Profile",
        category: "View",
        keywords: "workspace interface",
        icon: None,
        action: ShellAction::SetProfile(UiProfile::DesktopMenus),
    },
    ShellCommand {
        id: "view.profile.buttons",
        name: "View> Buttons Profile",
        category: "View",
        keywords: "workspace interface toolbar",
        icon: None,
        action: ShellAction::SetProfile(UiProfile::Buttons),
    },
    ShellCommand {
        id: "view.profile.hybrid",
        name: "View> Hybrid Profile",
        category: "View",
        keywords: "workspace interface toolbar",
        icon: None,
        action: ShellAction::SetProfile(UiProfile::Hybrid),
    },
    ShellCommand {
        id: "view.toolbars.toggle",
        name: "View> Toggle Toolbars",
        category: "View",
        keywords: "workspace buttons toolbar",
        icon: None,
        action: ShellAction::ToggleToolbars,
    },
    ShellCommand {
        id: "view.panels.toggle",
        name: "View> Toggle Panels",
        category: "View",
        keywords: "workspace inspector visibility",
        icon: None,
        action: ShellAction::TogglePanels,
    },
    ShellCommand {
        id: "view.grid.toggle",
        name: "View> Toggle Permanent Grid",
        category: "View",
        keywords: "grid visibility display",
        icon: None,
        action: ShellAction::ToggleGrid,
    },
    ShellCommand {
        id: "help.about",
        name: "Help> About Meshcraft",
        category: "Help",
        keywords: "version information",
        icon: None,
        action: ShellAction::About,
    },
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    profile: UiProfile,
    toolbars_visible: bool,
    auto_collapse_toolbars: bool,
    ui_text_scale: f32,
    toolbar_icon_size: u32,
    show_permanent_grid: bool,
    grid_extent: u32,
    normal_grid_width: f32,
    thick_grid_width: f32,
    toolbar_flow_top_down: bool,
    panels_visible: bool,
    panel_open: BTreeMap<String, bool>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            profile: UiProfile::DesktopMenus,
            toolbars_visible: true,
            auto_collapse_toolbars: false,
            ui_text_scale: 1.0,
            toolbar_icon_size: 32,
            show_permanent_grid: true,
            grid_extent: 20,
            normal_grid_width: 1.0,
            thick_grid_width: 3.0,
            toolbar_flow_top_down: false,
            panels_visible: true,
            panel_open: BTreeMap::new(),
        }
    }
}

pub struct MeshcraftApp {
    preferences: Preferences,
    icon_atlas: Option<egui::TextureHandle>,
    icon_regions: BTreeMap<&'static str, IconRegion>,
    active_tool: String,
    status: String,
    pointer_in_viewport: Option<egui::Vec2>,
    viewport_size: Vec2,
    open_file_menu: bool,
    command_palette_open: bool,
    command_search: String,
    command_selection: usize,
    focus_command_search: bool,
    document: Document,
    selection: SelectionSession,
    viewport: Viewport,
    cuboid_first: Option<Vec3>,
    cuboid_base: Option<(Vec3, Vec3)>,
    line_first: Option<Vec3>,
    rectangle_first: Option<Vec3>,
    array_count: usize,
    array_spacing: f64,
}

impl MeshcraftApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        Self::from_document(
            creation_context,
            Document::new(),
            "Empty document ready · choose Cuboid or Add Cuboid to begin.".to_owned(),
            false,
        )
    }

    pub fn new_with_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
    ) -> Self {
        Self::from_document(creation_context, document, status, true)
    }

    pub fn new_with_error(creation_context: &eframe::CreationContext<'_>, error: String) -> Self {
        Self::from_document(creation_context, Document::new(), error, false)
    }

    fn from_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
        frame_document: bool,
    ) -> Self {
        let preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, PREFERENCES_KEY))
            .unwrap_or_default();

        Self {
            preferences,
            icon_atlas: assets::load_icon_atlas(&creation_context.egui_ctx),
            icon_regions: assets::icon_regions(),
            active_tool: "Select".to_owned(),
            status,
            pointer_in_viewport: None,
            viewport_size: Vec2::ZERO,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            document,
            selection: SelectionSession::default(),
            viewport: {
                let mut viewport = Viewport::default();
                if frame_document {
                    viewport.request_frame();
                }
                viewport
            },
            cuboid_first: None,
            cuboid_base: None,
            line_first: None,
            rectangle_first: None,
            array_count: 12,
            array_spacing: 3.0,
        }
    }

    fn top_level_occurrences(&self) -> Vec<OccurrenceRef> {
        let occurrences = self.document.occurrence_refs().unwrap_or_default();
        let root = occurrences
            .iter()
            .find(|occurrence| occurrence.path.len() == 1);
        if let Some(root) = root
            && self
                .document
                .resolve_occurrence(root)
                .is_some_and(|resolved| resolved.definition.name == "Root")
        {
            let children = occurrences
                .iter()
                .filter(|occurrence| occurrence.path.len() == 2)
                .cloned()
                .collect::<Vec<_>>();
            if !children.is_empty() {
                return children;
            }
        }
        occurrences
            .into_iter()
            .filter(|occurrence| occurrence.path.len() == 1)
            .collect()
    }

    fn selected_occurrences(&self) -> Vec<OccurrenceRef> {
        self.selection
            .selected()
            .iter()
            .filter(|target| target.entity == SelectedEntity::Occurrence)
            .map(|target| target.occurrence.clone())
            .collect()
    }

    fn set_selection(&mut self, occurrences: impl IntoIterator<Item = OccurrenceRef>) {
        self.selection.clear();
        for occurrence in occurrences {
            self.selection.select(SelectionTarget {
                occurrence,
                entity: SelectedEntity::Occurrence,
            });
        }
    }

    fn run_command(&mut self, id: &str) {
        let Some(command) = SHELL_COMMANDS.iter().find(|command| command.id == id) else {
            self.status = format!("Unknown command: {id}");
            return;
        };

        match command.action {
            ShellAction::SelectTool(tool) => {
                self.active_tool = tool.to_owned();
                if self.preferences.auto_collapse_toolbars {
                    self.preferences.toolbars_visible = false;
                }
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.line_first = None;
                self.rectangle_first = None;
                self.status = match tool {
                    "Cuboid" => {
                        "Cuboid: click two ground-plane corners, then click the height.".to_owned()
                    }
                    "Line" => {
                        "Line: click a start point and an end point on the ground plane.".to_owned()
                    }
                    "Rectangle" => "Rectangle: click two opposite ground-plane corners.".to_owned(),
                    _ => format!("{tool} tool selected."),
                };
            }
            ShellAction::AddCuboid => {
                if let Some(occurrence) = self.document.add_cuboid(
                    "cuboid",
                    Vec3::new(2.0, 2.0, 2.0),
                    Vec3::new(-1.0, 0.0, -1.0),
                ) {
                    self.set_selection([occurrence]);
                    self.status = "Created a 2 × 2 × 2 cuboid.".to_owned();
                }
            }
            ShellAction::CopySelection => self.copy_selection(),
            ShellAction::CreateObject => self.create_object_from_selection(),
            ShellAction::ArraySelection => self.array_selection(),
            ShellAction::NewDocument => {
                self.document = Document::new();
                self.selection.clear();
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.line_first = None;
                self.rectangle_first = None;
                self.status = "New empty document.".to_owned();
            }
            ShellAction::SetProfile(profile) => {
                self.preferences.profile = profile;
                self.status = format!("{} workspace profile selected.", profile.label());
            }
            ShellAction::ToggleToolbars => {
                self.preferences.toolbars_visible = !self.preferences.toolbars_visible;
                self.status = if self.preferences.toolbars_visible {
                    "Hybrid toolbars shown.".to_owned()
                } else {
                    "Hybrid toolbars hidden.".to_owned()
                };
            }
            ShellAction::TogglePanels => {
                self.preferences.panels_visible = !self.preferences.panels_visible;
                self.status = if self.preferences.panels_visible {
                    "Panels shown.".to_owned()
                } else {
                    "Panels hidden.".to_owned()
                };
            }
            ShellAction::ToggleGrid => {
                self.preferences.show_permanent_grid = !self.preferences.show_permanent_grid;
                self.status = if self.preferences.show_permanent_grid {
                    "Permanent grid shown.".to_owned()
                } else {
                    "Permanent grid hidden.".to_owned()
                };
            }
            ShellAction::About => {
                self.status = "Meshcraft · Rust rewrite · development build".to_owned();
            }
        }
    }

    fn copy_selection(&mut self) {
        let selection = self.selected_occurrences();
        if selection.len() != 1 || selection[0].path.len() != 1 {
            self.status = "Copy requires one selected root object.".to_owned();
            return;
        }
        let source = selection[0].clone();
        let Some(resolved) = self.document.resolve_occurrence(&source) else {
            self.status = "The selected object no longer exists.".to_owned();
            return;
        };
        let Some(geometry) = self.document.effective_geometry(&source) else {
            self.status = "The selected object has no resolved geometry.".to_owned();
            return;
        };
        let span_x = geometry
            .vertices
            .values()
            .map(|vertex| resolved.world_transform.transform_point(vertex.position).x)
            .fold(None, |span: Option<(f64, f64)>, x| {
                Some(span.map_or((x, x), |(min, max)| (min.min(x), max.max(x))))
            })
            .map_or(1.0, |(min, max)| max - min);
        let Some(copy) = self
            .document
            .copy_root_occurrence(&source, Vec3::new(span_x + 0.5, 0.0, 0.0))
        else {
            self.status = "Copy could not create a root object instance.".to_owned();
            return;
        };
        self.set_selection([copy]);
        self.status = "Copied the selected object; the copy shares its definition.".to_owned();
    }

    fn create_object_from_selection(&mut self) {
        let selection = self.selected_occurrences();
        let Some(object) = self
            .document
            .create_object_from_root_occurrences(&selection, "Object")
        else {
            self.status = "Create Object requires one or more selected root objects.".to_owned();
            return;
        };
        self.set_selection([object]);
        self.status = "Created an object definition from the selected geometry.".to_owned();
    }

    fn array_selection(&mut self) {
        let selection = self.selected_occurrences();
        if selection.len() != 1 || selection[0].path.len() != 1 {
            self.status = "Array requires one selected root object.".to_owned();
            return;
        }
        let source = selection[0].clone();
        let Some(copies) = self.document.array_root_occurrence(
            &source,
            self.array_count,
            Vec3::new(self.array_spacing, 0.0, 0.0),
        ) else {
            self.status = "Array could not create the requested object instances.".to_owned();
            return;
        };
        self.set_selection(copies);
        self.status = format!("Created {} array copies.", self.array_count);
    }

    fn cancel_active_tool(&mut self) {
        self.active_tool = "Select".to_owned();
        self.cuboid_first = None;
        self.cuboid_base = None;
        self.line_first = None;
        self.rectangle_first = None;
        self.status = "Current tool operation canceled.".to_owned();
    }

    fn handle_viewport_click(&mut self, rect: egui::Rect, position: egui::Pos2) {
        if self.active_tool == "Line" {
            let point = self.viewport.screen_to_ground(rect, position);
            if let Some(start) = self.line_first {
                if let Some(occurrence) = self.document.add_line_segment("line", start, point) {
                    self.set_selection([occurrence]);
                    self.active_tool = "Select".to_owned();
                    self.line_first = None;
                    self.status = format!("Created line {:.2} units long.", start.distance(point));
                } else {
                    self.status = "Line endpoints must be distinct finite points.".to_owned();
                }
            } else {
                self.line_first = Some(point);
                self.status = "Line: choose the end point.".to_owned();
            }
            return;
        }

        if self.active_tool == "Rectangle" {
            let point = self.viewport.screen_to_ground(rect, position);
            if let Some(first) = self.rectangle_first {
                if let Some(occurrence) =
                    self.document
                        .add_ground_rectangle("rectangle", first, point)
                {
                    self.set_selection([occurrence]);
                    self.active_tool = "Select".to_owned();
                    self.rectangle_first = None;
                    self.status = format!(
                        "Created ground rectangle {:.2} × {:.2}.",
                        (point.x - first.x).abs(),
                        (point.z - first.z).abs()
                    );
                } else {
                    self.status = "Rectangle sides must be distinct finite points.".to_owned();
                }
            } else {
                self.rectangle_first = Some(point);
                self.status = "Rectangle: choose the opposite corner.".to_owned();
            }
            return;
        }

        if self.active_tool == "Cuboid" {
            let ground = self.viewport.screen_to_ground(rect, position);
            if self.cuboid_first.is_none() {
                self.cuboid_first = Some(ground);
                self.status = "Cuboid: choose the opposite corner of its base.".to_owned();
                return;
            }
            if self.cuboid_base.is_none() {
                let first = self.cuboid_first.unwrap();
                let width = (ground.x - first.x).abs();
                let depth = (ground.z - first.z).abs();
                if width < 0.05 || depth < 0.05 {
                    self.status =
                        "Cuboid base must be at least 0.05 units wide and deep.".to_owned();
                    return;
                }
                self.cuboid_base = Some((first, ground));
                self.status = "Cuboid: click above the base to set its height.".to_owned();
                return;
            }

            let (first, opposite) = self.cuboid_base.unwrap();
            let center = Vec3::new(
                (first.x + opposite.x) * 0.5,
                0.0,
                (first.z + opposite.z) * 0.5,
            );
            let height = self
                .viewport
                .height_from_screen_position(rect, center, position);
            if height < 0.05 {
                self.status = "Cuboid height must be at least 0.05 units.".to_owned();
                return;
            }
            let origin = Vec3::new(first.x.min(opposite.x), 0.0, first.z.min(opposite.z));
            let dimensions = Vec3::new(
                (opposite.x - first.x).abs(),
                height,
                (opposite.z - first.z).abs(),
            );
            if let Some(occurrence) = self.document.add_cuboid("cuboid", dimensions, origin) {
                self.set_selection([occurrence]);
                self.active_tool = "Select".to_owned();
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.status = format!(
                    "Created cuboid {:.2} × {:.2} × {:.2}.",
                    dimensions.x, dimensions.y, dimensions.z
                );
            }
            return;
        }

        if self.active_tool == "Select" {
            let selection: Vec<OccurrenceRef> = self
                .viewport
                .pick(rect, &self.document, position)
                .into_iter()
                .collect();
            self.set_selection(selection);
            self.status = if self.selection.selected().is_empty() {
                "Selection cleared.".to_owned()
            } else {
                "Object selected.".to_owned()
            };
        }
    }

    fn tool_preview(&self, rect: egui::Rect, pointer: Option<egui::Pos2>) -> Option<ToolPreview> {
        let position = pointer?;
        match self.active_tool.as_str() {
            "Cuboid" => {
                let (first, opposite) = self.cuboid_base?;
                let center = Vec3::new(
                    (first.x + opposite.x) * 0.5,
                    0.0,
                    (first.z + opposite.z) * 0.5,
                );
                let height = self
                    .viewport
                    .height_from_screen_position(rect, center, position)
                    .max(0.05);
                Some(ToolPreview::Cuboid(CuboidPreview {
                    origin: Vec3::new(first.x.min(opposite.x), 0.0, first.z.min(opposite.z)),
                    dimensions: Vec3::new(
                        (opposite.x - first.x).abs(),
                        height,
                        (opposite.z - first.z).abs(),
                    ),
                }))
            }
            "Line" => {
                let start = self.line_first?;
                Some(ToolPreview::Line {
                    start,
                    end: self.viewport.screen_to_ground(rect, position),
                })
            }
            "Rectangle" => {
                let first = self.rectangle_first?;
                Some(ToolPreview::GroundRectangle {
                    first,
                    opposite: self.viewport.screen_to_ground(rect, position),
                })
            }
            _ => None,
        }
    }

    fn matching_commands(&self) -> Vec<&'static ShellCommand> {
        SHELL_COMMANDS
            .iter()
            .filter(|command| command_matches(command, &self.command_search))
            .collect()
    }

    fn icon_image(&self, name: Option<&str>, size: Vec2) -> Option<egui::Image<'static>> {
        let region = self.icon_regions.get(name?)?;
        let texture = self.icon_atlas.as_ref()?;
        let atlas_size = 320.0;
        let uv = egui::Rect::from_min_max(
            egui::pos2(region.x as f32 / atlas_size, region.y as f32 / atlas_size),
            egui::pos2(
                (region.x + region.width) as f32 / atlas_size,
                (region.y + region.height) as f32 / atlas_size,
            ),
        );
        Some(egui::Image::new((texture.id(), size)).uv(uv))
    }

    fn show_tool_button(&self, ui: &mut egui::Ui, tool: &str, size: f32, icon_only: bool) -> bool {
        let command_id = tool_command_id(tool);
        let icon_name = SHELL_COMMANDS
            .iter()
            .find(|command| command.id == command_id)
            .and_then(|command| command.icon);
        let selected = self.active_tool == tool;
        let icon_size = (size * 0.55).max(12.0);
        let button = match (
            self.icon_image(icon_name, Vec2::splat(icon_size)),
            icon_only,
        ) {
            (Some(icon), true) => egui::Button::image(icon),
            (Some(icon), false) => egui::Button::image_and_text(icon, tool),
            (None, true) => egui::Button::new(tool.chars().next().unwrap_or('?').to_string()),
            (None, false) => egui::Button::new(tool),
        }
        .selected(selected);
        let button_size = if icon_only {
            Vec2::splat(size)
        } else {
            Vec2::new(ui.available_width(), size)
        };
        ui.add_sized(button_size, button).clicked()
    }

    fn show_command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_palette_open {
            return;
        }

        let commands = self.matching_commands();
        if !commands.is_empty() {
            self.command_selection = self.command_selection.min(commands.len() - 1);
        } else {
            self.command_selection = 0;
        }

        let mut command_icons = commands
            .iter()
            .map(|command| self.icon_image(command.icon, Vec2::splat(16.0)))
            .collect::<Vec<_>>()
            .into_iter();
        let mut selected_command = None;
        let focus_search = std::mem::take(&mut self.focus_command_search);
        egui::Window::new("Command Palette")
            .id(egui::Id::unique("command-palette"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .open(&mut self.command_palette_open)
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.command_search)
                        .id_salt("command-palette-search")
                        .hint_text("Search commands…")
                        .desired_width(f32::INFINITY),
                );
                if focus_search {
                    search.request_focus();
                }

                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for (index, command) in commands.iter().enumerate() {
                            let label = format!("[{}] {}", command.category, command.name);
                            let icon = command_icons.next().flatten();
                            ui.horizontal(|ui| {
                                if let Some(icon) = icon {
                                    ui.add(icon);
                                }
                                if ui
                                    .selectable_label(index == self.command_selection, label)
                                    .clicked()
                                {
                                    selected_command = Some(command.id);
                                }
                            });
                        }
                    });
                ui.separator();
                ui.weak(format!(
                    "{} commands available · Enter to run · Esc to close",
                    commands.len()
                ));
            });

        if let Some(id) = selected_command {
            self.run_command(id);
            self.command_palette_open = false;
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        let request_file_menu = std::mem::take(&mut self.open_file_menu);

        egui::MenuBar::new().ui(ui, |ui| {
            let file_menu = ui.menu_button("File", |ui| {
                if ui.button("New Document").clicked() {
                    self.run_command("file.new");
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Open…"));
                ui.add_enabled(false, egui::Button::new("Save"));
                ui.add_enabled(false, egui::Button::new("Save As…"));
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Import Geometry…"));
                ui.add_enabled(false, egui::Button::new("Export Geometry…"));
                ui.add_enabled(false, egui::Button::new("Exit"));
            });
            if request_file_menu {
                let popup_id = egui::Popup::default_response_id(&file_menu.response);
                ui.memory_mut(|memory| memory.request_focus(file_menu.response.id));
                egui::Popup::open_id(ui.ctx(), popup_id);
                ui.ctx().request_repaint();
            }

            ui.menu_button("Edit", |ui| {
                ui.add_enabled(false, egui::Button::new("Undo"));
                ui.add_enabled(false, egui::Button::new("Redo"));
                ui.add_enabled(false, egui::Button::new("Delete"));
                ui.add_enabled(false, egui::Button::new("Clear Selection"));
                ui.separator();
                if ui.button("Copy Selection").clicked() {
                    self.run_command("edit.copy_selection");
                }
            });

            ui.menu_button("View", |ui| {
                ui.label("Workspace profile");
                for (id, profile) in [
                    ("view.profile.menus", UiProfile::DesktopMenus),
                    ("view.profile.buttons", UiProfile::Buttons),
                    ("view.profile.hybrid", UiProfile::Hybrid),
                ] {
                    if ui
                        .radio(self.preferences.profile == profile, profile.label())
                        .clicked()
                    {
                        self.run_command(id);
                    }
                }
                ui.separator();
                let mut toolbars_visible = self.preferences.toolbars_visible;
                if ui
                    .checkbox(&mut toolbars_visible, "Show toolbars")
                    .changed()
                {
                    self.run_command("view.toolbars.toggle");
                }
                ui.separator();
                let mut grid_visible = self.preferences.show_permanent_grid;
                if ui
                    .checkbox(&mut grid_visible, "Show permanent grid")
                    .changed()
                {
                    self.run_command("view.grid.toggle");
                }
                ui.add_enabled(false, egui::Button::new("Camera and visibility controls"));
            });

            for menu in ["Draw", "Modify", "Objects", "Tools", "Render"] {
                ui.menu_button(menu, |ui| {
                    let commands = SHELL_COMMANDS
                        .iter()
                        .filter(|command| command.category == menu)
                        .collect::<Vec<_>>();
                    if commands.is_empty() {
                        ui.label("No commands are registered in this menu yet.");
                    }
                    for command in commands {
                        let label = command
                            .name
                            .split_once("> ")
                            .map_or(command.name, |(_, label)| label);
                        if ui.button(label).clicked() {
                            self.run_command(command.id);
                        }
                    }
                });
            }

            ui.menu_button("Window", |ui| {
                ui.label("Workspace panels");
                if ui
                    .button(if self.preferences.panels_visible {
                        "Hide Panels"
                    } else {
                        "Show Panels"
                    })
                    .clicked()
                {
                    self.run_command("view.panels.toggle");
                }
                ui.add_enabled(false, egui::Button::new("Reset Workspace Layout"));
            });

            ui.menu_button("Help", |ui| {
                ui.label("Meshcraft development build");
                ui.label("F10 or Alt+F opens the File menu.");
                ui.label("Ctrl+Shift+P opens the Command Palette.");
                if ui.button("About Meshcraft").clicked() {
                    self.run_command("help.about");
                }
            });
        });
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let tools = ["Select", "Cuboid", "Line", "Rectangle", "Orbit"];
        let size = self.preferences.toolbar_icon_size as f32;
        if self.preferences.toolbar_flow_top_down {
            ui.vertical(|ui| {
                ui.strong("Tools");
                for tool in tools {
                    if self.show_tool_button(ui, tool, size, true) {
                        self.run_command(tool_command_id(tool));
                    }
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.strong("Tools");
                ui.separator();
                for tool in tools {
                    if self.show_tool_button(ui, tool, size, true) {
                        self.run_command(tool_command_id(tool));
                    }
                }
            });
        }
    }

    fn show_tool_panel(&mut self, ui: &mut egui::Ui, height: f32) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(148.0);
            ui.set_min_height(height);
            ui.heading("Tools");
            ui.label("Modeling tools");
            ui.separator();
            let size = self.preferences.toolbar_icon_size as f32;
            for tool in ["Select", "Cuboid", "Line", "Rectangle", "Orbit"] {
                if self.show_tool_button(ui, tool, size, false) {
                    self.run_command(tool_command_id(tool));
                }
            }
            ui.add_space(12.0);
            if ui.button("Add Cuboid").clicked() {
                self.run_command("geometry.add_cuboid");
            }
            if ui.button("Copy Selection").clicked() {
                self.run_command("edit.copy_selection");
            }
            if ui.button("Create Object").clicked() {
                self.run_command("object.create_from_selection");
            }
            if ui.button("Array Object").clicked() {
                self.run_command("object.array_selection");
            }
        });
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui, size: Vec2) {
        let predicted_rect = egui::Rect::from_min_size(ui.cursor().min, size);
        let pointer = ui
            .ctx()
            .pointer_interact_pos()
            .filter(|position| predicted_rect.contains(*position));
        let preview = self.tool_preview(predicted_rect, pointer);
        let selection = self.selected_occurrences();
        let input = self.viewport.show(
            ui,
            &ViewportScene {
                size,
                document: &self.document,
                selection: &selection,
                active_tool: &self.active_tool,
                preview,
                grid: GridStyle {
                    visible: self.preferences.show_permanent_grid,
                    spacing: self.document.model_settings().grid_size,
                    extent: self.preferences.grid_extent,
                    normal_width: self.preferences.normal_grid_width,
                    thick_width: self.preferences.thick_grid_width,
                },
            },
        );
        self.viewport_size = input.rect.size();
        self.pointer_in_viewport = input.pointer_local;
        if let Some(position) = input.click {
            self.handle_viewport_click(input.rect, position);
        }
    }

    fn show_panel_header(&mut self, ui: &mut egui::Ui, id: &str, title: &str) -> bool {
        let is_open = *self
            .preferences
            .panel_open
            .entry(id.to_owned())
            .or_insert(true);
        let marker = if is_open { "-" } else { "+" };
        let width = ui.available_width();
        let response = ui.add_sized(
            [width, 28.0],
            egui::Button::new(
                egui::RichText::new(format!("{marker}  {title}")).color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(54, 163, 244))
            .corner_radius(0.0),
        );
        if response.clicked() {
            self.preferences.panel_open.insert(id.to_owned(), !is_open);
            !is_open
        } else {
            is_open
        }
    }

    fn show_panel_body(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(Color32::from_rgb(84, 84, 84))
            .inner_margin(egui::Margin::same(7))
            .show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                add_contents(ui);
            });
        ui.add_space(5.0);
    }

    fn show_selection_panel(&mut self, ui: &mut egui::Ui) {
        let root_objects = self.top_level_occurrences();
        let mut all_edges = 0;
        let mut all_faces = 0;
        for occurrence in self.document.occurrence_refs().unwrap_or_default() {
            if let Some(geometry) = self.document.effective_geometry(&occurrence) {
                all_edges += geometry.segments.len();
                all_faces += geometry.triangles.len();
            }
        }
        let selection = self.selected_occurrences();
        let (selected_edges, selected_faces) = selection.iter().fold((0, 0), |sum, item| {
            self.document
                .effective_geometry(item)
                .map_or(sum, |geometry| {
                    (
                        sum.0 + geometry.segments.len(),
                        sum.1 + geometry.triangles.len(),
                    )
                })
        });
        ui.horizontal(|ui| {
            ui.weak("Type");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.weak("Total   Selected");
            });
        });
        for (label, total, selected) in [
            ("Edges", all_edges, selected_edges),
            ("Faces", all_faces, selected_faces),
            ("Voxels", 0, 0),
            ("Hotspots", 0, 0),
            ("Objects", root_objects.len(), selection.len()),
            ("Dimensions", 0, 0),
            ("Texts", 0, 0),
            ("Walls", 0, 0),
            ("Slabs", 0, 0),
            ("Stairs", 0, 0),
            ("Frames", 0, 0),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.monospace(format!("{total:>6}   {selected:>4}"));
                });
            });
        }
    }

    fn show_object_panel(&mut self, ui: &mut egui::Ui) {
        let selection = self.selected_occurrences();
        let Some(selected) = selection.first() else {
            ui.label("No object selected");
            return;
        };
        let Some(resolved) = self.document.resolve_occurrence(selected) else {
            ui.label("The selected object no longer exists");
            return;
        };
        ui.label(format!("{} selected", selection.len()));
        ui.label("Name");
        let mut name = resolved.definition.name.clone();
        if ui
            .add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY))
            .changed()
        {
            match self
                .document
                .rename_definition(resolved.definition.id, &name)
            {
                Ok(_) => {}
                Err(error) => self.status = error,
            }
        }
    }

    fn show_objects_panel(&mut self, ui: &mut egui::Ui) {
        let root_objects = self.top_level_occurrences();
        egui::ScrollArea::vertical()
            .id_salt("document-object-list")
            .max_height(180.0)
            .show(ui, |ui| {
                for occurrence in &root_objects {
                    let label = self
                        .document
                        .resolve_occurrence(occurrence)
                        .map(|resolved| resolved.definition.name.as_str())
                        .unwrap_or("Missing object");
                    let selected = self.selection.selected().contains(&SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Occurrence,
                    });
                    if ui.selectable_label(selected, label).clicked() {
                        self.set_selection([occurrence.clone()]);
                    }
                }
            });
    }

    fn show_model_settings(&mut self, ui: &mut egui::Ui) {
        let mut settings = self.document.model_settings().clone();
        let mut changed = false;
        ui.label("Unit name");
        changed |= ui
            .add(egui::TextEdit::singleline(&mut settings.unit_name).desired_width(f32::INFINITY))
            .changed();
        for (label, value, range) in [
            ("Unit size", &mut settings.unit_size, 0.0001..=1_000_000.0),
            ("Grid size", &mut settings.grid_size, 0.0001..=1_000_000.0),
            ("Snap radius", &mut settings.snap_radius, 0.0..=128.0),
        ] {
            ui.label(label);
            changed |= ui
                .add_sized(
                    [ui.available_width(), 24.0],
                    egui::DragValue::new(value)
                        .range(range)
                        .max_decimals(4)
                        .speed(0.05),
                )
                .changed();
        }
        ui.label("Circle segments");
        changed |= ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::DragValue::new(&mut settings.circle_segments).range(3..=256),
            )
            .changed();
        if changed {
            match self.document.update_model_settings(settings) {
                Ok(_) => {}
                Err(error) => {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            }
        }
    }

    fn show_ui_settings(&mut self, ui: &mut egui::Ui) {
        ui.label("UI text size");
        egui::ComboBox::from_id_salt("ui-text-scale")
            .selected_text(format!("{}x", self.preferences.ui_text_scale))
            .show_ui(ui, |ui| {
                for scale in [0.75_f32, 1.0, 1.25, 1.5, 2.0] {
                    ui.selectable_value(
                        &mut self.preferences.ui_text_scale,
                        scale,
                        format!("{scale}x"),
                    );
                }
            });
        ui.checkbox(
            &mut self.preferences.auto_collapse_toolbars,
            "Auto-collapse after tool selection",
        );
        ui.label("Toolbar icon/button size");
        egui::ComboBox::from_id_salt("toolbar-icon-size")
            .selected_text(format!(
                "{} × {} px",
                self.preferences.toolbar_icon_size, self.preferences.toolbar_icon_size
            ))
            .show_ui(ui, |ui| {
                for size in [24, 32, 40, 48] {
                    ui.selectable_value(
                        &mut self.preferences.toolbar_icon_size,
                        size,
                        format!("{size} × {size} px"),
                    );
                }
            });
        ui.checkbox(
            &mut self.preferences.show_permanent_grid,
            "Show permanent grid",
        );
        ui.label("Permanent grid extent (steps from origin)");
        ui.add(egui::DragValue::new(&mut self.preferences.grid_extent).range(1..=256));
        ui.label("Normal line width");
        ui.add(
            egui::Slider::new(&mut self.preferences.normal_grid_width, 0.25..=4.0).suffix(" px"),
        );
        ui.label("Thick line width");
        ui.add(egui::Slider::new(&mut self.preferences.thick_grid_width, 0.5..=8.0).suffix(" px"));
        ui.label("Arrange toolbars");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!self.preferences.toolbar_flow_top_down, "Flow L→R")
                .clicked()
            {
                self.preferences.toolbar_flow_top_down = false;
            }
            if ui
                .selectable_label(self.preferences.toolbar_flow_top_down, "Flow T→D")
                .clicked()
            {
                self.preferences.toolbar_flow_top_down = true;
            }
        });
        ui.weak("Toolbar layout and grid display are saved as application preferences.");
    }

    fn show_profile_settings(&mut self, ui: &mut egui::Ui) {
        let selected_profile = self.preferences.profile;
        egui::ComboBox::from_id_salt("ui-profile")
            .selected_text(selected_profile.label())
            .show_ui(ui, |ui| {
                for (id, profile) in [
                    ("view.profile.menus", UiProfile::DesktopMenus),
                    ("view.profile.buttons", UiProfile::Buttons),
                    ("view.profile.hybrid", UiProfile::Hybrid),
                ] {
                    if ui
                        .selectable_label(selected_profile == profile, profile.label())
                        .clicked()
                    {
                        self.run_command(id);
                    }
                }
            });
        ui.checkbox(&mut self.preferences.toolbars_visible, "Show toolbars");
    }

    fn show_inspector(&mut self, ui: &mut egui::Ui, height: f32) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(402.0);
            ui.set_min_height(height);
            ui.heading("Panels");
            egui::ScrollArea::vertical()
                .id_salt("panels-scroll")
                .max_height((height - 34.0).max(100.0))
                .show(ui, |ui| {
                    if self.show_panel_header(ui, "selection", "Selection") {
                        Self::show_panel_body(ui, |ui| self.show_selection_panel(ui));
                    }
                    if self.show_panel_header(ui, "object", "Object") {
                        Self::show_panel_body(ui, |ui| self.show_object_panel(ui));
                    }
                    if self.show_panel_header(ui, "objects", "Objects") {
                        Self::show_panel_body(ui, |ui| {
                            self.show_objects_panel(ui);
                            ui.separator();
                            ui.add(
                                egui::DragValue::new(&mut self.array_count)
                                    .range(1..=10_000)
                                    .prefix("Copy count: "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.array_spacing)
                                    .speed(0.1)
                                    .prefix("Spacing: "),
                            );
                            ui.horizontal(|ui| {
                                if ui.button("Copy").clicked() {
                                    self.run_command("edit.copy_selection");
                                }
                                if ui.button("Create Object").clicked() {
                                    self.run_command("object.create_from_selection");
                                }
                                if ui.button("Array").clicked() {
                                    self.run_command("object.array_selection");
                                }
                            });
                        });
                    }
                    if self.show_panel_header(ui, "model_settings", "Model Settings") {
                        Self::show_panel_body(ui, |ui| self.show_model_settings(ui));
                    }
                    if self.show_panel_header(ui, "profile", "Workspace Profile") {
                        Self::show_panel_body(ui, |ui| self.show_profile_settings(ui));
                    }
                    if self.show_panel_header(ui, "ui_settings", "UI Settings") {
                        Self::show_panel_body(ui, |ui| self.show_ui_settings(ui));
                    }
                    if self.show_panel_header(ui, "help", "Help / About") {
                        Self::show_panel_body(ui, |ui| {
                            ui.label("Meshcraft");
                            ui.label("Platform: Desktop");
                            ui.label("Rust rewrite · development build");
                            ui.label("Ctrl+Shift+P opens the command palette.");
                        });
                    }
                    if self.show_panel_header(ui, "polyline", "Polyline Settings") {
                        Self::show_panel_body(ui, |ui| {
                            ui.label("Arc max length · Double line size · Double line offset");
                            ui.weak("Polyline construction settings are not implemented yet.");
                        });
                    }
                    if self.show_panel_header(ui, "vector_text", "Vector Text") {
                        Self::show_panel_body(ui, |ui| {
                            ui.label("Text · Size · Tracking · Line spacing · Glyph source");
                            ui.weak("Vector text settings are not implemented yet.");
                        });
                    }
                    for (id, title) in [
                        ("architecture_walls", "Architecture - Walls"),
                        ("architecture_slabs", "Architecture - Slabs"),
                        ("architecture_stairs", "Architecture - Stairs"),
                        ("architecture_frames", "Architecture - Frames"),
                        ("hvac", "HVAC Settings"),
                        ("hotspots", "Hotspot Settings"),
                        ("random", "Random Tools"),
                        ("tutorials", "Tutorials"),
                        ("shaders", "Shader Settings"),
                    ] {
                        if self.show_panel_header(ui, id, title) {
                            Self::show_panel_body(ui, |ui| {
                                ui.weak(format!("{title} are not implemented yet."));
                            });
                        }
                    }
                });
        });
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui) {
        ui.ctx()
            .set_zoom_factor(self.preferences.ui_text_scale.clamp(0.5, 2.0));
        let toggle_command_palette = ui.input(|input| {
            input.modifiers.ctrl && input.modifiers.shift && input.key_pressed(egui::Key::P)
        });
        if toggle_command_palette {
            self.command_palette_open = !self.command_palette_open;
            self.command_search.clear();
            self.command_selection = 0;
            self.focus_command_search = self.command_palette_open;
        }

        if self.command_palette_open {
            let close_palette = ui.input(|input| input.key_pressed(egui::Key::Escape));
            if close_palette {
                self.command_palette_open = false;
            } else {
                if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                    self.command_selection = self.command_selection.saturating_add(1);
                }
                if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                    self.command_selection = self.command_selection.saturating_sub(1);
                }
                if ui.input(|input| input.key_pressed(egui::Key::Enter))
                    && let Some(command) = self.matching_commands().get(self.command_selection)
                {
                    self.run_command(command.id);
                    self.command_palette_open = false;
                }
            }
        } else if ui.input(|input| input.key_pressed(egui::Key::Escape))
            && self.active_tool != "Select"
        {
            self.cancel_active_tool();
        }

        let menus_visible = self.preferences.profile != UiProfile::Buttons;
        let toolbar_visible = self.preferences.toolbars_visible
            && matches!(
                self.preferences.profile,
                UiProfile::Buttons | UiProfile::Hybrid
            );

        if menus_visible {
            let open_file_menu = ui.input(|input| {
                input.key_pressed(egui::Key::F10)
                    || (input.modifiers.alt && input.key_pressed(egui::Key::F))
            });
            if open_file_menu {
                self.status = "Keyboard menu shortcut received.".to_owned();
            }
            self.open_file_menu |= open_file_menu;
            self.show_menu_bar(ui);
        }
        if toolbar_visible {
            self.show_toolbar(ui);
        }

        ui.separator();
        let content_height = (ui.available_height() - 48.0).max(160.0);
        let content_width = ui.available_width();
        let show_tools = content_width >= 820.0;
        let show_inspector = self.preferences.panels_visible && content_width >= 650.0;
        let side_width =
            if show_tools { 166.0 } else { 0.0 } + if show_inspector { 420.0 } else { 0.0 };
        let viewport_width = (content_width - side_width - 16.0).max(120.0);

        ui.horizontal(|ui| {
            if show_tools {
                ui.allocate_ui_with_layout(
                    Vec2::new(166.0, content_height),
                    Layout::top_down(Align::Min),
                    |ui| self.show_tool_panel(ui, content_height),
                );
            }
            self.show_viewport(ui, Vec2::new(viewport_width, content_height));
            if show_inspector {
                ui.allocate_ui_with_layout(
                    Vec2::new(420.0, content_height),
                    Layout::top_down(Align::Min),
                    |ui| self.show_inspector(ui, content_height),
                );
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.weak(format!("Tool: {}", self.active_tool));
            ui.separator();
            ui.label(&self.status);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let object_count = self.top_level_occurrences().len();
                ui.weak(format!(
                    "{object_count} objects · revision {}",
                    self.document.revision()
                ));
                if let Some(pointer) = self.pointer_in_viewport {
                    ui.weak(format!("Pointer: {:.0}, {:.0}", pointer.x, pointer.y));
                }
                ui.separator();
                ui.weak(format!(
                    "Viewport {:.0} × {:.0}",
                    self.viewport_size.x, self.viewport_size.y
                ));
            });
        });

        self.show_command_palette(ui.ctx());
    }
}

fn tool_command_id(tool: &str) -> &'static str {
    match tool {
        "Select" => "tool.select",
        "Cuboid" => "tool.cuboid",
        "Line" => "tool.line",
        "Rectangle" => "tool.rectangle",
        "Orbit" => "view.orbit",
        _ => "tool.select",
    }
}

fn command_matches(command: &ShellCommand, raw_query: &str) -> bool {
    let query = raw_query.trim().to_lowercase();
    query.split_whitespace().all(|term| {
        command.name.to_lowercase().contains(term)
            || command.id.to_lowercase().contains(term)
            || command.category.to_lowercase().contains(term)
            || command.keywords.to_lowercase().contains(term)
    })
}

#[cfg(test)]
fn registry_has_unique_ids() -> bool {
    let mut ids = BTreeSet::new();
    SHELL_COMMANDS.iter().all(|command| ids.insert(command.id))
}

impl eframe::App for MeshcraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| self.show_workspace(ui));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFERENCES_KEY, &self.preferences);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MeshcraftApp, Preferences, SHELL_COMMANDS, SelectionSession, command_matches,
        registry_has_unique_ids, tool_command_id,
    };
    use eframe::egui::{self, Event, ImeEvent, Key, Modifiers, Pos2, RawInput, Rect, Vec2};
    use meshcraft_core::document::Document;
    use std::collections::BTreeMap;

    fn test_app() -> MeshcraftApp {
        MeshcraftApp {
            preferences: Preferences::default(),
            icon_atlas: None,
            icon_regions: BTreeMap::new(),
            active_tool: "Select".to_owned(),
            status: String::new(),
            pointer_in_viewport: None,
            viewport_size: Vec2::ZERO,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            document: Document::new(),
            selection: SelectionSession::default(),
            viewport: Default::default(),
            cuboid_first: None,
            cuboid_base: None,
            line_first: None,
            rectangle_first: None,
            array_count: 12,
            array_spacing: 3.0,
        }
    }

    fn run_workspace_frame(
        app: &mut MeshcraftApp,
        context: &egui::Context,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        let mut output = context.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| {
                egui::Frame::central_panel(ui.style()).show(ui, |ui| app.show_workspace(ui));
            },
        );
        output.textures_delta.clear();
        output
    }

    fn open_palette(app: &mut MeshcraftApp, context: &egui::Context) {
        run_workspace_frame(app, context, {
            let modifiers = Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            };
            vec![
                Event::ModifiersChanged(modifiers),
                Event::Key {
                    key: Key::P,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ]
        });
        assert!(app.command_palette_open);
    }

    #[test]
    fn every_shell_command_has_a_unique_stable_id() {
        assert!(registry_has_unique_ids());
    }

    #[test]
    fn every_visible_tool_has_a_registry_command() {
        for tool in ["Select", "Cuboid", "Line", "Rectangle", "Orbit"] {
            let id = tool_command_id(tool);
            assert!(SHELL_COMMANDS.iter().any(|command| command.id == id));
        }
    }

    #[test]
    fn modeling_workflow_commands_are_searchable_in_the_palette() {
        for (id, query) in [
            ("geometry.add_cuboid", "cuboid"),
            ("edit.copy_selection", "duplicate"),
            ("object.create_from_selection", "prototype"),
            ("object.array_selection", "array"),
        ] {
            let command = SHELL_COMMANDS
                .iter()
                .find(|command| command.id == id)
                .unwrap_or_else(|| panic!("missing command {id}"));
            assert!(
                command_matches(command, query),
                "command {id} misses {query}"
            );
        }
    }

    #[test]
    fn rectangle_search_metadata_is_present() {
        let rectangle = SHELL_COMMANDS
            .iter()
            .find(|command| command.id == "tool.rectangle")
            .unwrap();
        assert!(command_matches(rectangle, "Rectangle"));
        assert!(command_matches(rectangle, "tool shape"));
        assert!(!command_matches(rectangle, "camera"));
    }

    #[test]
    fn desktop_selection_is_owned_by_the_occurrence_session() {
        let mut app = test_app();
        let first = app
            .document
            .add_cuboid(
                "shared",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::ZERO,
            )
            .unwrap();
        let second = app
            .document
            .copy_root_occurrence(&first, meshcraft_core::geometry::Vec3::new(2.0, 0.0, 0.0))
            .unwrap();

        app.set_selection([first.clone()]);
        assert_eq!(app.selected_occurrences(), vec![first]);
        app.set_selection([second.clone()]);
        assert_eq!(app.selected_occurrences(), vec![second]);
    }

    #[test]
    fn panel_and_grid_visibility_have_palette_commands() {
        for (id, query) in [
            ("view.panels.toggle", "inspector"),
            ("view.grid.toggle", "permanent grid"),
        ] {
            let command = SHELL_COMMANDS
                .iter()
                .find(|command| command.id == id)
                .unwrap_or_else(|| panic!("missing command {id}"));
            assert!(command_matches(command, query));
        }
    }

    #[test]
    fn preferences_default_to_reference_ui_values_and_load_partial_old_state() {
        let preferences: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(1.0, preferences.ui_text_scale);
        assert_eq!(32, preferences.toolbar_icon_size);
        assert!(preferences.show_permanent_grid);
        assert_eq!(20, preferences.grid_extent);
        assert_eq!(1.0, preferences.normal_grid_width);
        assert_eq!(3.0, preferences.thick_grid_width);
        assert!(preferences.panels_visible);
    }

    #[test]
    fn command_palette_search_receives_text_and_ime_composition() {
        let mut app = test_app();
        let context = egui::Context::default();
        open_palette(&mut app, &context);

        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Text("rectangle".to_owned())],
        );
        assert_eq!(app.command_search, "rectangle");
        assert!(
            app.matching_commands()
                .iter()
                .any(|command| { command.id == "tool.rectangle" })
        );

        app.command_search.clear();
        let preedit = run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Ime(ImeEvent::Preedit {
                text: "かな".to_owned(),
                active_range_chars: Some(0..2),
            })],
        );
        assert_eq!(app.command_search, "かな");
        assert!(preedit.platform_output.ime.is_some());

        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Ime(ImeEvent::Commit("かな".to_owned()))],
        );
        assert_eq!(app.command_search, "かな");
    }
}
