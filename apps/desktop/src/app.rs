use crate::assets::{self, IconRegion};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Sense, Stroke, Vec2};
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
}

impl MeshcraftApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
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
            status: "Workspace shell ready · no document loaded".to_owned(),
            pointer_in_viewport: None,
            viewport_size: Vec2::ZERO,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
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
                self.status = format!("{tool} selected in the workspace shell.");
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
                ui.add_enabled(false, egui::Button::new("New Document"));
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
                    ui.label("Actions will be connected to the shared command registry.");
                    ui.add_enabled(false, egui::Button::new("No commands are enabled yet"));
                });
            }

            ui.menu_button("Window", |ui| {
                ui.label("Workspace panels");
                ui.add_enabled(false, egui::Button::new("Inspector"));
                ui.add_enabled(false, egui::Button::new("Reset Workspace Layout"));
            });

            ui.menu_button("Help", |ui| {
                ui.label("Meshcraft development shell");
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
            for tool in ["Select", "Line", "Rectangle", "Orbit"] {
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
            ui.label("Shell controls");
            ui.separator();
            for tool in ["Select", "Line", "Rectangle", "Orbit"] {
                if self.show_tool_button(ui, tool) {
                    self.run_command(tool_command_id(tool));
                }
            }
            ui.add_space(12.0);
            ui.label("Modeling actions are added after the core model gate.");
        });
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui, size: Vec2) {
        let (response, painter) = ui.allocate_painter(size, Sense::hover());
        let rect = response.rect;
        self.viewport_size = rect.size();
        painter.rect_filled(rect, 2.0, Color32::from_rgb(26, 31, 39));

        let step = 32.0;
        let first_x = (rect.left() / step).floor() * step;
        let mut x = first_x;
        while x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_rgb(43, 50, 60)),
            );
            x += step;
        }
        let first_y = (rect.top() / step).floor() * step;
        let mut y = first_y;
        while y <= rect.bottom() {
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(1.0, Color32::from_rgb(43, 50, 60)),
            );
            y += step;
        }

        let center = rect.center();
        painter.line_segment(
            [
                egui::pos2(rect.left(), center.y),
                egui::pos2(rect.right(), center.y),
            ],
            Stroke::new(1.5, Color32::from_rgb(184, 89, 87)),
        );
        painter.line_segment(
            [
                egui::pos2(center.x, rect.top()),
                egui::pos2(center.x, rect.bottom()),
            ],
            Stroke::new(1.5, Color32::from_rgb(101, 161, 114)),
        );
        painter.text(
            rect.left_top() + egui::vec2(14.0, 14.0),
            Align2::LEFT_TOP,
            "Viewport work area",
            FontId::proportional(15.0),
            Color32::from_rgb(222, 227, 234),
        );

        self.pointer_in_viewport = response.hover_pos().map(|position| position - rect.min);
    }

    fn show_inspector(&mut self, ui: &mut egui::Ui, height: f32) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(228.0);
            ui.set_min_height(height);
            ui.heading("Inspector");
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
            ui.label(format!("Value: {}", self.input_probe));
            ui.add_space(12.0);
            ui.label(format!("Active shell tool: {}", self.active_tool));
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
                ui.weak("Drafting shell · no document model loaded");
            });
        });

        self.show_command_palette(ui.ctx());
    }
}

fn tool_command_id(tool: &str) -> &'static str {
    match tool {
        "Select" => "tool.select",
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
        for tool in ["Select", "Line", "Rectangle", "Orbit"] {
            let id = tool_command_id(tool);
            assert!(SHELL_COMMANDS.iter().any(|command| command.id == id));
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
