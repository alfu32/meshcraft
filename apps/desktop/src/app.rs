use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    profile: UiProfile,
    toolbars_visible: bool,
}

pub struct MeshcraftApp {
    preferences: Preferences,
    active_tool: String,
    input_probe: String,
    status: String,
    pointer_in_viewport: Option<egui::Vec2>,
    viewport_size: Vec2,
    open_file_menu: bool,
}

impl MeshcraftApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        let preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, PREFERENCES_KEY))
            .unwrap_or_default();

        Self {
            preferences,
            active_tool: "Select".to_owned(),
            input_probe: String::new(),
            status: "Workspace shell ready · no document loaded".to_owned(),
            pointer_in_viewport: None,
            viewport_size: Vec2::ZERO,
            open_file_menu: false,
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        let request_file_menu = std::mem::take(&mut self.open_file_menu);

        egui::MenuBar::new().ui(ui, |ui| {
            let file_menu = ui.menu_button("File", |ui| {
                if ui.button("New Document").clicked() {
                    self.status =
                        "New document is waiting for the normalized document core.".to_owned();
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
            });

            ui.menu_button("View", |ui| {
                ui.label("Workspace profile");
                ui.radio_value(
                    &mut self.preferences.profile,
                    UiProfile::DesktopMenus,
                    "Desktop Menus",
                );
                ui.radio_value(&mut self.preferences.profile, UiProfile::Buttons, "Buttons");
                ui.radio_value(&mut self.preferences.profile, UiProfile::Hybrid, "Hybrid");
                ui.separator();
                ui.checkbox(
                    &mut self.preferences.toolbars_visible,
                    "Toolbars in Hybrid profile",
                );
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
                if ui.button("About Meshcraft").clicked() {
                    self.status = "Meshcraft · Rust rewrite · development build".to_owned();
                }
            });
        });
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Tools");
            ui.separator();
            for tool in ["Select", "Line", "Rectangle", "Orbit"] {
                if ui
                    .selectable_label(self.active_tool == tool, tool)
                    .clicked()
                {
                    self.active_tool = tool.to_owned();
                    self.status = format!("{tool} selected in the workspace shell.");
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
                if ui
                    .selectable_label(self.active_tool == tool, tool)
                    .clicked()
                {
                    self.active_tool = tool.to_owned();
                    self.status = format!("{tool} selected in the workspace shell.");
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
            egui::ComboBox::from_id_salt("ui-profile")
                .selected_text(self.preferences.profile.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.preferences.profile,
                        UiProfile::DesktopMenus,
                        "Desktop Menus",
                    );
                    ui.selectable_value(
                        &mut self.preferences.profile,
                        UiProfile::Buttons,
                        "Buttons",
                    );
                    ui.selectable_value(&mut self.preferences.profile, UiProfile::Hybrid, "Hybrid");
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
    }
}

impl eframe::App for MeshcraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| self.show_workspace(ui));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFERENCES_KEY, &self.preferences);
    }
}
