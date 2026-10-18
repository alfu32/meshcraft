use crate::assets::{self, IconRegion};
use crate::viewport::{CuboidPreview, ToolPreview, Viewport};
use eframe::egui::{self, Align, Align2, Layout, Vec2};
use meshcraft_core::document::{Document, OccurrenceRef};
use meshcraft_core::geometry::Vec3;
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
        name: "View> Toggle Hybrid Toolbars",
        category: "View",
        keywords: "workspace buttons",
        icon: None,
        action: ShellAction::ToggleToolbars,
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

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    profile: UiProfile,
    toolbars_visible: bool,
}

pub struct MeshcraftApp {
    preferences: Preferences,
    icon_atlas: Option<egui::TextureHandle>,
    icon_regions: BTreeMap<&'static str, IconRegion>,
    active_tool: String,
    input_probe: String,
    status: String,
    pointer_in_viewport: Option<egui::Vec2>,
    viewport_size: Vec2,
    open_file_menu: bool,
    command_palette_open: bool,
    command_search: String,
    command_selection: usize,
    focus_command_search: bool,
    document: Document,
    selection: Vec<OccurrenceRef>,
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
            input_probe: String::new(),
            status,
            pointer_in_viewport: None,
            viewport_size: Vec2::ZERO,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            document,
            selection: Vec::new(),
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

    fn run_command(&mut self, id: &str) {
        let Some(command) = SHELL_COMMANDS.iter().find(|command| command.id == id) else {
            self.status = format!("Unknown command: {id}");
            return;
        };

        match command.action {
            ShellAction::SelectTool(tool) => {
                self.active_tool = tool.to_owned();
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
                    self.selection = vec![occurrence];
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
            ShellAction::About => {
                self.status = "Meshcraft · Rust rewrite · development build".to_owned();
            }
        }
    }

    fn copy_selection(&mut self) {
        if self.selection.len() != 1 || self.selection[0].path.len() != 1 {
            self.status = "Copy requires one selected root object.".to_owned();
            return;
        }
        let source = self.selection[0].clone();
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
        self.selection = vec![copy];
        self.status = "Copied the selected object; the copy shares its definition.".to_owned();
    }

    fn create_object_from_selection(&mut self) {
        let Some(object) = self
            .document
            .create_object_from_root_occurrences(&self.selection, "Object")
        else {
            self.status = "Create Object requires one or more selected root objects.".to_owned();
            return;
        };
        self.selection = vec![object];
        self.status = "Created an object definition from the selected geometry.".to_owned();
    }

    fn array_selection(&mut self) {
        if self.selection.len() != 1 || self.selection[0].path.len() != 1 {
            self.status = "Array requires one selected root object.".to_owned();
            return;
        }
        let source = self.selection[0].clone();
        let Some(copies) = self.document.array_root_occurrence(
            &source,
            self.array_count,
            Vec3::new(self.array_spacing, 0.0, 0.0),
        ) else {
            self.status = "Array could not create the requested object instances.".to_owned();
            return;
        };
        self.selection = copies;
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
                    self.selection = vec![occurrence];
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
                    self.selection = vec![occurrence];
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
                self.selection = vec![occurrence];
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
            self.selection = self
                .viewport
                .pick(rect, &self.document, position)
                .into_iter()
                .collect();
            self.status = if self.selection.is_empty() {
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

    fn show_tool_button(&self, ui: &mut egui::Ui, tool: &str) -> bool {
        let command_id = tool_command_id(tool);
        let icon_name = SHELL_COMMANDS
            .iter()
            .find(|command| command.id == command_id)
            .and_then(|command| command.icon);
        let selected = self.active_tool == tool;
        let button = match self.icon_image(icon_name, Vec2::splat(16.0)) {
            Some(icon) => egui::Button::image_and_text(icon, tool),
            None => egui::Button::new(tool),
        }
        .selected(selected);
        ui.add(button).clicked()
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
                    .checkbox(&mut toolbars_visible, "Toolbars in Hybrid profile")
                    .changed()
                {
                    self.run_command("view.toolbars.toggle");
                }
                ui.separator();
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
                ui.add_enabled(false, egui::Button::new("Inspector"));
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
        ui.horizontal(|ui| {
            ui.strong("Tools");
            ui.separator();
            for tool in ["Select", "Cuboid", "Line", "Rectangle", "Orbit"] {
                if self.show_tool_button(ui, tool) {
                    self.run_command(tool_command_id(tool));
                }
            }
        });
    }

    fn show_tool_panel(&mut self, ui: &mut egui::Ui, height: f32) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(148.0);
            ui.set_min_height(height);
            ui.heading("Tools");
            ui.label("Modeling tools");
            ui.separator();
            for tool in ["Select", "Cuboid", "Line", "Rectangle", "Orbit"] {
                if self.show_tool_button(ui, tool) {
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
        let input = self.viewport.show(
            ui,
            size,
            &self.document,
            &self.selection,
            &self.active_tool,
            preview,
        );
        self.viewport_size = input.rect.size();
        self.pointer_in_viewport = input.pointer_local;
        if let Some(position) = input.click {
            self.handle_viewport_click(input.rect, position);
        }
    }

    fn show_inspector(&mut self, ui: &mut egui::Ui, height: f32) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(228.0);
            ui.set_min_height(height);
            ui.heading("Selection");
            let root_objects = self.top_level_occurrences();
            ui.label(format!("Objects: {}", root_objects.len()));
            ui.label(format!("Selected: {}", self.selection.len()));
            ui.separator();
            ui.label("Objects");
            egui::ScrollArea::vertical()
                .id_salt("document-object-list")
                .max_height(120.0)
                .show(ui, |ui| {
                    for occurrence in &root_objects {
                        let label = self
                            .document
                            .resolve_occurrence(occurrence)
                            .map(|resolved| resolved.definition.name.as_str())
                            .unwrap_or("Missing object");
                        let selected = self.selection.contains(occurrence);
                        if ui.selectable_label(selected, label).clicked() {
                            self.selection = vec![occurrence.clone()];
                        }
                    }
                });
            ui.separator();
            ui.label("Model Settings");
            ui.label("Unit size: 1.0000 · Grid: 1.0000");
            ui.label("Circle segments: 24");
            ui.separator();
            ui.label("Array");
            ui.add(
                egui::DragValue::new(&mut self.array_count)
                    .range(1..=10_000)
                    .prefix("Copies: "),
            );
            ui.add(
                egui::DragValue::new(&mut self.array_spacing)
                    .speed(0.1)
                    .prefix("Spacing: "),
            );
            if ui.button("Copy Selection").clicked() {
                self.run_command("edit.copy_selection");
            }
            if ui.button("Create Object from Selection").clicked() {
                self.run_command("object.create_from_selection");
            }
            if ui.button("Array Selected Object").clicked() {
                self.run_command("object.array_selection");
            }
            ui.separator();
            ui.label("Workspace profile");
            ui.label("UI profile");
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
            ui.separator();
            ui.label("Keyboard focus / IME probe");
            let input_response = ui.add(
                egui::TextEdit::singleline(&mut self.input_probe)
                    .hint_text("Type or compose text…")
                    .desired_width(210.0),
            );
            if input_response.changed() {
                self.status = format!(
                    "Text input received: {} characters",
                    self.input_probe.chars().count()
                );
            }
            ui.add_space(12.0);
            ui.label(format!("Active tool: {}", self.active_tool));
            ui.label(format!(
                "Viewport area: {:.0} × {:.0} px",
                self.viewport_size.x, self.viewport_size.y
            ));
            if let Some(pointer) = self.pointer_in_viewport {
                ui.label(format!(
                    "Pointer in viewport: {:.0}, {:.0}",
                    pointer.x, pointer.y
                ));
            } else {
                ui.label("Pointer outside viewport");
            }
        });
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui) {
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
        let toolbar_visible = self.preferences.profile == UiProfile::Buttons
            || (self.preferences.profile == UiProfile::Hybrid && self.preferences.toolbars_visible);

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
        let content_height = (ui.available_height() - 28.0).max(160.0);
        let content_width = ui.available_width();
        let show_tools = content_width >= 680.0;
        let show_inspector = content_width >= 560.0;
        let side_width =
            if show_tools { 166.0 } else { 0.0 } + if show_inspector { 246.0 } else { 0.0 };
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
                    Vec2::new(246.0, content_height),
                    Layout::top_down(Align::Min),
                    |ui| self.show_inspector(ui, content_height),
                );
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(&self.status);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let object_count = self.top_level_occurrences().len();
                ui.weak(format!(
                    "{} root objects · revision {}",
                    object_count, self.document.revision
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
    use super::{SHELL_COMMANDS, command_matches, registry_has_unique_ids, tool_command_id};

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
}
