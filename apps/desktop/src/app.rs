use crate::assets::{self, IconRegion};
use crate::viewport::{CuboidPreview, GridStyle, ToolPreview, Viewport, ViewportScene};
use eframe::egui::{self, Align, Align2, Color32, Layout, Vec2};
use meshcraft_core::commands::{
    CommandCapability, CommandDescriptor, CommandKind, CommandPayload, CommandPresentation,
    CommandRegistry, CommandResult, CommandState, CommandStateQuery,
};
use meshcraft_core::document::{DefinitionId, Document, OccurrenceRef};
use meshcraft_core::geometry::Vec3;
use meshcraft_core::selection::{SelectedEntity, SelectionSession, SelectionTarget};
use meshcraft_core::transactions::{
    HistoryEntryOrigin, OpaqueLegacyHistory, SystemHistoryClock, TransactionHistory,
};
use meshcraft_io::project::{ImportedProject, LegacySnapshotAdapter};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::sync::OnceLock;

const PREFERENCES_KEY: &str = "meshcraft.preferences";
const ARCHITECTURE_PANEL_IDS: &[&str] = &[
    "architecture_walls",
    "architecture_slabs",
    "architecture_stairs",
    "architecture_frames",
];
const LEGACY_BUILTIN_TOOLS: &[(&str, &str)] = &[
    ("select", "Select"),
    ("axial_grid", "Axial Grid"),
    ("planar_grid", "Planar Grid"),
    ("line", "Line"),
    ("construction_line", "Construction Line"),
    ("polyline", "Polyline"),
    ("double_line", "Double Line"),
    ("ribbon_3d", "Ribbon 3D"),
    ("voxel", "Voxel"),
    ("voxel_volume", "Voxel Volume"),
    ("voxel_frame", "Voxel Frame"),
    ("arch_wall", "Wall"),
    ("arch_slab", "Slab"),
    ("arch_stair", "Stair"),
    ("arch_add_hole", "Add Hole"),
    ("arch_window_frame", "Window Frame"),
    ("arch_door_frame", "Door Frame"),
    ("hvac_plumbing", "HVAC Plumbing"),
    ("hvac_ventilation", "HVAC Ventilation"),
    ("primitive_sphere", "Sphere"),
    ("primitive_cylinder", "Cylinder"),
    ("primitive_cone", "Cone"),
    ("primitive_pill", "Pill"),
    ("mech_screw", "Screw"),
    ("mech_circular_hole", "Circular Hole"),
    ("mech_round_washer", "Round Washer"),
    ("mech_cog_wheel", "Cog Wheel"),
    ("face_outline", "Face Outline"),
    ("polyline_mesh", "Polyline Mesh"),
    ("object_cut", "Cut Objects"),
    ("solid_union", "Solid Union"),
    ("solid_intersection", "Solid Intersection"),
    ("solid_subtraction", "Solid Subtraction"),
    ("mesh", "Mesh"),
    ("line_offset", "Line Offset"),
    ("cut_holes", "Cut Holes"),
    ("cut_holes_2", "Cut Holes 2"),
    ("cut_out_3", "Cutout"),
    ("extrude_swipe", "Extrude Swipe"),
    ("extrude_swipe_3d", "Extrude Swipe 3D"),
    ("plane_section", "Plane Section"),
    ("mesh_intersection", "Mesh Intersection"),
    ("cut_with_plane", "Cut With Plane"),
    ("revolve", "Revolve"),
    ("rectangle", "Rectangle"),
    ("surface_rectangle", "Surface Rect"),
    ("quad", "Quad"),
    ("circle", "Circle"),
    ("linear_dimension", "Linear Dimension"),
    ("text", "Text"),
    ("vector_text", "Vector Text"),
    ("push_pull", "Push/Pull"),
    ("move", "Move"),
    ("rotate", "Rotate"),
    ("rotate_2", "Rotate 2"),
    ("scale", "Scale"),
    ("stretch", "Stretch"),
    ("stretch_scale", "Stretch Scale"),
    ("random_offset", "Random Offset"),
    ("random_surface_array", "Random Surface Array"),
    ("mesh_regularize", "Planar Regularize"),
    ("surface_remesh", "Surface Remesh"),
    ("rotate_stretch", "Rotate Stretch"),
    ("copy_multiple", "Copy Multiple"),
    ("planar_translate_multiple", "Planar Translate Multiple"),
    (
        "volumetric_translate_multiple",
        "Volumetric Translate Multiple",
    ),
    ("planar_rotate_multiple", "Planar Rotate Multiple"),
    ("helicoidal_rotate_multiple", "Helicoidal Rotate Multiple"),
    ("rotational_array", "Rotational Array"),
    ("helical_array", "Helical Array"),
    ("rotational_array_equalized", "Rotational Array Even"),
    ("helical_array_equalized", "Helical Array Even"),
    ("paint", "Paint"),
    ("object_place", "Object"),
];
const LEGACY_TOOLBARS: &[(&str, &str)] = &[
    ("builtin_toolbar_construction_points", "Point Construction"),
    (
        "builtin_toolbar_construction_entities",
        "Entity Construction",
    ),
    ("builtin_toolbar_modification", "Modification"),
    ("builtin_toolbar_modification_2", "Modification 2"),
    ("builtin_toolbar_architecture", "Architecture"),
    ("builtin_toolbar_hvac", "HVAC"),
    ("builtin_toolbar_primitives", "Primitives"),
    ("builtin_toolbar_mech", "Mech"),
    ("builtin_toolbar_voxel", "Voxel"),
    ("builtin_toolbar_actions", "Actions"),
    ("builtin_toolbar_camera", "Camera"),
    ("builtin_toolbar_rendering", "Rendering"),
];
const LEGACY_COMMANDS: &[(&str, &str, &str, &str, Option<&str>)] = &[
    (
        "edit.new_architecture_group",
        "Edit> Architecture: Wall Tool",
        "Edit",
        "architecture wall slab stair",
        Some("edit"),
    ),
    (
        "edit.new_voxel_group",
        "Edit> New Voxel Group",
        "Edit",
        "voxel group minecraft",
        Some("edit"),
    ),
    (
        "edit.voxelize_faces",
        "Edit> Voxelize Faces",
        "Edit",
        "voxel faces convert",
        Some("edit"),
    ),
    (
        "edit.group",
        "Edit> Group",
        "Edit",
        "group object prototype selection",
        Some("edit"),
    ),
    (
        "edit.ungroup",
        "Edit> Ungroup",
        "Edit",
        "ungroup object",
        Some("edit"),
    ),
    (
        "view.axis_guide",
        "View> Add Axis Guide",
        "View",
        "axis guide",
        Some("view"),
    ),
    (
        "view.grid_guide",
        "View> Add Grid Guide",
        "View",
        "grid guide",
        Some("view"),
    ),
    (
        "view.camera.orbit",
        "View> Camera Orbit",
        "View",
        "camera orbit perspective",
        Some("view"),
    ),
    (
        "view.camera.walkthrough",
        "View> Camera Walkthrough",
        "View",
        "camera walkthrough perspective",
        Some("view"),
    ),
    (
        "view.camera.orthographic",
        "View> Camera Orthographic",
        "View",
        "camera orthographic",
        Some("view"),
    ),
    (
        "view.capture_ui_minimal",
        "View> Capture UI Minimal",
        "View",
        "capture screenshot ui panels",
        Some("view"),
    ),
    (
        "view.capture_ui_restore",
        "View> Capture UI Restore",
        "View",
        "capture screenshot ui panels restore",
        Some("view"),
    ),
    (
        "export.svg_view",
        "Export> SVG (View)",
        "Export",
        "export svg view",
        Some("export"),
    ),
    (
        "export.screenshot",
        "Export> Screenshot",
        "Export",
        "export screenshot printscreen png",
        Some("export"),
    ),
    (
        "export.ifc_model",
        "Export> IFC (Model)",
        "Export",
        "export ifc model",
        Some("export"),
    ),
    (
        "import.mesh",
        "Import> Mesh (OBJ/STL/FBX/glTF/DAE/DXF/3MF/AMF/IFC)",
        "Import",
        "import mesh obj stl fbx gltf glb dae dxf 3mf amf ifc",
        Some("import"),
    ),
    (
        "export.mesh",
        "Export> Mesh (OBJ/STL/FBX/glTF/DAE/DXF/3MF/AMF/IFC)",
        "Export",
        "export mesh obj stl fbx gltf glb dae dxf 3mf amf ifc",
        Some("export"),
    ),
    (
        "edit.cut_rect_hole",
        "Edit> Cut Rect Hole",
        "Edit",
        "cut hole rect polyline faces",
        Some("edit"),
    ),
    (
        "edit.reset_scene_for_capture",
        "Edit> Reset Scene For Capture",
        "Edit",
        "reset scene capture screenshot perspective",
        Some("edit"),
    ),
];

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
    SelectToolAlias(&'static str),
    AddCuboid,
    CopySelection,
    CreateObject,
    ArraySelection,
    Undo,
    Redo,
    NewDocument,
    SetProfile(UiProfile),
    ToggleToolbars,
    TogglePanels,
    ToggleGrid,
    OpenPanel(&'static str),
    OpenPanelGroup(&'static [&'static str]),
    Unsupported,
    About,
}

fn shell_commands() -> &'static CommandRegistry<ShellAction> {
    static REGISTRY: OnceLock<CommandRegistry<ShellAction>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::default();
        let commands = [
            (
                "tool.select",
                "Tool> Select",
                "Tools",
                "selection pointer",
                Some("select"),
                ShellAction::SelectTool("Select"),
            ),
            (
                "tool.cuboid",
                "Tool> Cuboid",
                "Tools",
                "box primitive draw solid",
                None,
                ShellAction::SelectTool("Cuboid"),
            ),
            (
                "geometry.add_cuboid",
                "Draw> Add Cuboid at Origin",
                "Draw",
                "box primitive create solid",
                None,
                ShellAction::AddCuboid,
            ),
            (
                "edit.undo",
                "Edit> Undo",
                "Edit",
                "history revert previous",
                None,
                ShellAction::Undo,
            ),
            (
                "edit.redo",
                "Edit> Redo",
                "Edit",
                "history repeat restore",
                None,
                ShellAction::Redo,
            ),
            (
                "edit.copy_selection",
                "Edit> Copy Selection",
                "Edit",
                "duplicate clone",
                None,
                ShellAction::CopySelection,
            ),
            (
                "object.create_from_selection",
                "Objects> Create Object from Selection",
                "Objects",
                "group prototype",
                None,
                ShellAction::CreateObject,
            ),
            (
                "object.array_selection",
                "Objects> Array Selected Object",
                "Objects",
                "repeat copies distribute",
                None,
                ShellAction::ArraySelection,
            ),
            (
                "file.new",
                "File> New Document",
                "File",
                "clear reset",
                None,
                ShellAction::NewDocument,
            ),
            (
                "tool.line",
                "Tool> Line",
                "Tools",
                "draw segment",
                Some("construction_line"),
                ShellAction::SelectTool("Line"),
            ),
            (
                "tool.rectangle",
                "Tool> Rectangle",
                "Tools",
                "draw shape",
                Some("rectangle"),
                ShellAction::SelectTool("Rectangle"),
            ),
            (
                "view.orbit",
                "View> Orbit",
                "View",
                "camera rotate",
                Some("camera_rotating_operation"),
                ShellAction::SelectTool("Orbit"),
            ),
            (
                "view.profile.menus",
                "View> Desktop Menus Profile",
                "View",
                "workspace interface",
                None,
                ShellAction::SetProfile(UiProfile::DesktopMenus),
            ),
            (
                "view.profile.buttons",
                "View> Buttons Profile",
                "View",
                "workspace interface toolbar",
                None,
                ShellAction::SetProfile(UiProfile::Buttons),
            ),
            (
                "view.profile.hybrid",
                "View> Hybrid Profile",
                "View",
                "workspace interface toolbar",
                None,
                ShellAction::SetProfile(UiProfile::Hybrid),
            ),
            (
                "view.toolbars.toggle",
                "View> Toggle Toolbars",
                "View",
                "workspace buttons toolbar",
                None,
                ShellAction::ToggleToolbars,
            ),
            (
                "view.panels.toggle",
                "View> Toggle Panels",
                "View",
                "workspace inspector visibility",
                None,
                ShellAction::TogglePanels,
            ),
            (
                "view.grid.toggle",
                "View> Toggle Permanent Grid",
                "View",
                "grid visibility display",
                None,
                ShellAction::ToggleGrid,
            ),
            (
                "help.about",
                "Help> About Meshcraft",
                "Help",
                "version information",
                None,
                ShellAction::About,
            ),
            (
                "view.objects",
                "View> Objects",
                "View",
                "objects panel prototypes",
                Some("view"),
                ShellAction::OpenPanel("objects"),
            ),
            (
                "view.selection",
                "View> Selection",
                "View",
                "selection panel",
                Some("view"),
                ShellAction::OpenPanel("selection"),
            ),
            (
                "view.object_info",
                "View> Object Info",
                "View",
                "object info panel",
                Some("view"),
                ShellAction::OpenPanel("object"),
            ),
            (
                "view.lighting",
                "View> Lighting",
                "View",
                "lighting shader panel",
                Some("view"),
                ShellAction::OpenPanel("shaders"),
            ),
            (
                "view.model_settings",
                "View> Model Settings",
                "View",
                "model settings",
                Some("view"),
                ShellAction::OpenPanel("model_settings"),
            ),
            (
                "view.polyline_settings",
                "View> Polyline Settings",
                "View",
                "polyline settings",
                Some("view"),
                ShellAction::OpenPanel("polyline"),
            ),
            (
                "view.vector_text_settings",
                "View> Vector Text Settings",
                "View",
                "vector text settings glyph",
                Some("view"),
                ShellAction::OpenPanel("vector_text"),
            ),
            (
                "view.architecture_settings",
                "View> Architecture Settings",
                "View",
                "architecture walls slabs stairs frames settings",
                Some("view"),
                ShellAction::OpenPanelGroup(ARCHITECTURE_PANEL_IDS),
            ),
            (
                "view.hvac_settings",
                "View> HVAC Settings",
                "View",
                "hvac settings ventilation plumbing",
                Some("view"),
                ShellAction::OpenPanel("hvac"),
            ),
            (
                "view.hotspot_settings",
                "View> Hotspot Settings",
                "View",
                "hotspot settings operation",
                Some("view"),
                ShellAction::OpenPanel("hotspots"),
            ),
            (
                "view.random_tool_settings",
                "View> Random Tool Settings",
                "View",
                "random fuzzy scatter remesh settings",
                Some("view"),
                ShellAction::OpenPanel("random"),
            ),
            (
                "view.help_about",
                "View> Help / About",
                "View",
                "help about platform input",
                Some("view"),
                ShellAction::OpenPanel("help"),
            ),
            (
                "view.plugin_manager",
                "View> Plugin Manager",
                "View",
                "plugins manager",
                Some("view"),
                ShellAction::Unsupported,
            ),
        ];
        for (id, name, category, keywords, icon, action) in commands {
            register_shell_command(&mut registry, id, name, category, keywords, icon, action);
        }
        for (tool_id, label) in LEGACY_BUILTIN_TOOLS {
            let id = format!("tool.builtin.{tool_id}");
            let name = format!("Tool> {label}");
            let keywords = format!("{} tool", label.to_lowercase());
            let action = match *tool_id {
                "select" => ShellAction::SelectToolAlias("Select"),
                "line" => ShellAction::SelectToolAlias("Line"),
                "rectangle" => ShellAction::SelectToolAlias("Rectangle"),
                _ => ShellAction::Unsupported,
            };
            register_shell_command(
                &mut registry,
                &id,
                &name,
                "Tools",
                &keywords,
                Some("tool"),
                action,
            );
        }
        for (id, name, category, keywords, icon) in LEGACY_COMMANDS {
            let action = if *id == "edit.group" {
                ShellAction::CreateObject
            } else {
                ShellAction::Unsupported
            };
            register_shell_command(&mut registry, id, name, category, keywords, *icon, action);
        }
        for (view_id, label) in [
            ("top", "Ortho Top"),
            ("bottom", "Ortho Bottom"),
            ("left", "Ortho Left"),
            ("right", "Ortho Right"),
            ("front", "Ortho Front"),
            ("back", "Ortho Back"),
        ] {
            let id = format!("view.ortho.{view_id}");
            let name = format!("View> {label}");
            let keywords = format!("camera orthographic {view_id}");
            register_shell_command(
                &mut registry,
                &id,
                &name,
                "View",
                &keywords,
                Some("view"),
                ShellAction::Unsupported,
            );
        }
        for (toolbar_id, label) in LEGACY_TOOLBARS {
            let Some(stem) = toolbar_id.strip_prefix("builtin_toolbar_") else {
                continue;
            };
            for verb in ["show", "hide"] {
                let title = if verb == "show" { "Show" } else { "Hide" };
                let id = format!("view.toolbar.{stem}.{verb}");
                let name = format!("View> Toolbars> {title} {label}");
                let keywords = format!("toolbar {verb} {}", label.to_lowercase());
                register_shell_command(
                    &mut registry,
                    &id,
                    &name,
                    "View",
                    &keywords,
                    Some("view"),
                    ShellAction::Unsupported,
                );
            }
        }
        registry
    })
}

fn action_kind(action: ShellAction) -> CommandKind {
    match action {
        ShellAction::SelectTool(_) | ShellAction::SelectToolAlias(_) => CommandKind::ToolActivation,
        ShellAction::AddCuboid
        | ShellAction::CopySelection
        | ShellAction::CreateObject
        | ShellAction::ArraySelection
        | ShellAction::Undo
        | ShellAction::Redo
        | ShellAction::NewDocument => CommandKind::DocumentMutation,
        ShellAction::SetProfile(_)
        | ShellAction::ToggleToolbars
        | ShellAction::TogglePanels
        | ShellAction::ToggleGrid
        | ShellAction::OpenPanel(_)
        | ShellAction::OpenPanelGroup(_) => CommandKind::SessionMutation,
        ShellAction::Unsupported => CommandKind::HostRequest,
        ShellAction::About => CommandKind::ReadOnly,
    }
}

fn action_state_query(action: ShellAction) -> CommandStateQuery {
    match action {
        ShellAction::Undo => CommandStateQuery::UndoAvailable,
        ShellAction::Redo => CommandStateQuery::RedoAvailable,
        ShellAction::CopySelection => CommandStateQuery::SingleRootSelection,
        ShellAction::CreateObject => CommandStateQuery::RootSelectionAvailable,
        ShellAction::ArraySelection => CommandStateQuery::SingleRootSelection,
        ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
            CommandStateQuery::ActiveTool(tool.to_owned())
        }
        ShellAction::SetProfile(profile) => {
            CommandStateQuery::ActiveProfile(profile.label().to_owned())
        }
        ShellAction::ToggleToolbars => CommandStateQuery::PreferenceEnabled("toolbars".to_owned()),
        ShellAction::TogglePanels => CommandStateQuery::PreferenceEnabled("panels".to_owned()),
        ShellAction::ToggleGrid => CommandStateQuery::PreferenceEnabled("grid".to_owned()),
        _ => CommandStateQuery::Always,
    }
}

fn register_shell_command(
    registry: &mut CommandRegistry<ShellAction>,
    id: &str,
    name: &str,
    category: &str,
    keywords: &str,
    icon: Option<&str>,
    action: ShellAction,
) {
    let (_, label) = name.split_once("> ").unwrap_or((category, name));
    let mut descriptor = CommandDescriptor::new(
        id,
        label,
        category,
        keywords.split_whitespace(),
        action_kind(action),
    )
    .expect("built-in command metadata is valid");
    descriptor.state_query = action_state_query(action);
    descriptor.capability = match action {
        ShellAction::SelectTool(_)
        | ShellAction::SelectToolAlias(_)
        | ShellAction::SetProfile(_)
        | ShellAction::ToggleToolbars
        | ShellAction::TogglePanels
        | ShellAction::ToggleGrid
        | ShellAction::OpenPanel(_)
        | ShellAction::OpenPanelGroup(_)
        | ShellAction::About => CommandCapability::Desktop,
        ShellAction::Unsupported => CommandCapability::Deferred,
        _ => CommandCapability::Core,
    };
    descriptor.presentation = CommandPresentation {
        palette_path: name.to_owned(),
        menu_path: vec![category.to_owned()],
        toolbar_group: match action {
            ShellAction::SelectTool(_) => Some("Tools".to_owned()),
            ShellAction::AddCuboid
            | ShellAction::CopySelection
            | ShellAction::CreateObject
            | ShellAction::ArraySelection => Some("Modeling".to_owned()),
            _ => None,
        },
        icon_key: icon.map(str::to_owned),
        tutorial_action_id: None,
    };
    registry
        .register(descriptor, action)
        .expect("built-in command ids are unique");
}

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
    panel_focus: Option<String>,
    document: Document,
    history: TransactionHistory<SystemHistoryClock>,
    legacy_history: Option<OpaqueLegacyHistory>,
    legacy_active_index: Option<usize>,
    selection: SelectionSession,
    object_name_edit: Option<(DefinitionId, String)>,
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
            None,
        )
    }

    pub fn new_with_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
    ) -> Self {
        Self::from_document(creation_context, document, status, true, None)
    }

    pub fn new_with_project(
        creation_context: &eframe::CreationContext<'_>,
        project: ImportedProject,
        status: String,
    ) -> Self {
        Self::from_document(
            creation_context,
            project.document,
            status,
            true,
            project.history,
        )
    }

    pub fn new_with_error(creation_context: &eframe::CreationContext<'_>, error: String) -> Self {
        Self::from_document(creation_context, Document::new(), error, false, None)
    }

    fn from_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
        frame_document: bool,
        legacy_history: Option<OpaqueLegacyHistory>,
    ) -> Self {
        let preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, PREFERENCES_KEY))
            .unwrap_or_default();

        let legacy_active_index = legacy_history
            .as_ref()
            .and_then(OpaqueLegacyHistory::active_entry_index);

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
            panel_focus: None,
            document,
            history: TransactionHistory::default(),
            legacy_history,
            legacy_active_index,
            selection: SelectionSession::default(),
            object_name_edit: None,
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

    fn commit_document_edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut pending = self.history.begin(&self.document, label);
        let result = pending
            .stage(operation)
            .map_err(|error| error.to_string())?;
        self.history
            .commit(&mut self.document, pending)
            .map_err(|error| error.to_string())?;
        self.selection.retain_valid(&self.document);
        Ok(result)
    }

    fn undo_document(&mut self) {
        if self.history.can_undo() {
            let entry = self.history.cursor() - 1;
            let origin = self.history.entries()[entry].origin.clone();
            match self.history.undo(&mut self.document) {
                Ok(Some(_)) => {
                    if let HistoryEntryOrigin::Legacy {
                        from_entry_index, ..
                    } = origin
                    {
                        self.legacy_active_index = Some(from_entry_index);
                    }
                    self.selection.retain_valid(&self.document);
                    self.status = "Undid the previous document operation.".to_owned();
                }
                Ok(None) => self.status = "Nothing to undo.".to_owned(),
                Err(error) => self.status = error.to_string(),
            }
            return;
        }

        let Some(current_index) = self.legacy_active_index else {
            self.status = "Nothing to undo.".to_owned();
            return;
        };
        let mut scan_index = current_index;
        let mut skipped_entries = Vec::new();
        let (previous_index, label, timestamp_millis, earlier) = loop {
            let Some(previous_index) = self
                .legacy_history
                .as_ref()
                .and_then(|history| history.previous_entry_index(scan_index))
            else {
                self.status = if skipped_entries.is_empty() {
                    "Nothing to undo.".to_owned()
                } else {
                    format!(
                        "No earlier legacy snapshot could be loaded; skipped corrupt entries {}.",
                        skipped_entries
                            .iter()
                            .map(|index: &usize| (index + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return;
            };
            let Some(entry) = self
                .legacy_history
                .as_ref()
                .and_then(|history| history.entries.get(previous_index))
            else {
                self.status = format!("Legacy history entry {} is missing.", previous_index + 1);
                return;
            };
            let label = entry
                .label
                .clone()
                .unwrap_or_else(|| format!("Legacy undo entry {}", previous_index + 1));
            let timestamp_millis = entry.timestamp_millis;
            let decoded = self
                .legacy_history
                .as_mut()
                .expect("legacy entry came from retained history")
                .decode_entry(previous_index, &LegacySnapshotAdapter::default());
            match decoded {
                Ok(document) => break (previous_index, label, timestamp_millis, document),
                Err(_) => {
                    skipped_entries.push(previous_index);
                    scan_index = previous_index;
                }
            }
        };
        if let Err(error) = self.history.prepend_legacy_state(
            &self.document,
            earlier,
            label.clone(),
            timestamp_millis,
            previous_index,
            current_index,
        ) {
            self.status = error.to_string();
            return;
        }

        match self.history.undo(&mut self.document) {
            Ok(Some(_)) => {
                self.legacy_active_index = Some(previous_index);
                self.selection.retain_valid(&self.document);
                self.status = legacy_replay_status("Undid", &label, &skipped_entries);
            }
            Ok(None) => self.status = "Nothing to undo.".to_owned(),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn redo_document(&mut self) {
        if self.history.can_redo() {
            let entry = self.history.cursor();
            let origin = self.history.entries()[entry].origin.clone();
            match self.history.redo(&mut self.document) {
                Ok(Some(_)) => {
                    if let HistoryEntryOrigin::Legacy { to_entry_index, .. } = origin {
                        self.legacy_active_index = Some(to_entry_index);
                    }
                    self.selection.retain_valid(&self.document);
                    self.status = "Redid the document operation.".to_owned();
                }
                Ok(None) => self.status = "Nothing to redo.".to_owned(),
                Err(error) => self.status = error.to_string(),
            }
            return;
        }

        let Some(current_index) = self.legacy_active_index else {
            self.status = "Nothing to redo.".to_owned();
            return;
        };
        let mut scan_index = current_index;
        let mut skipped_entries = Vec::new();
        let (next_index, label, timestamp_millis, later) = loop {
            let Some(next_index) = self
                .legacy_history
                .as_ref()
                .and_then(|history| history.next_entry_index(scan_index))
            else {
                self.status = if skipped_entries.is_empty() {
                    "Nothing to redo.".to_owned()
                } else {
                    format!(
                        "No later legacy snapshot could be loaded; skipped corrupt entries {}.",
                        skipped_entries
                            .iter()
                            .map(|index: &usize| (index + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return;
            };
            let Some(entry) = self
                .legacy_history
                .as_ref()
                .and_then(|history| history.entries.get(next_index))
            else {
                self.status = format!("Legacy history entry {} is missing.", next_index + 1);
                return;
            };
            let label = entry
                .label
                .clone()
                .unwrap_or_else(|| format!("Legacy undo entry {}", next_index + 1));
            let timestamp_millis = entry.timestamp_millis;
            let decoded = self
                .legacy_history
                .as_mut()
                .expect("legacy entry came from retained history")
                .decode_entry(next_index, &LegacySnapshotAdapter::default());
            match decoded {
                Ok(document) => break (next_index, label, timestamp_millis, document),
                Err(_) => {
                    skipped_entries.push(next_index);
                    scan_index = next_index;
                }
            }
        };
        if let Err(error) = self.history.append_legacy_state(
            &self.document,
            later,
            label.clone(),
            timestamp_millis,
            current_index,
            next_index,
        ) {
            self.status = error.to_string();
            return;
        }

        match self.history.redo(&mut self.document) {
            Ok(Some(_)) => {
                self.legacy_active_index = Some(next_index);
                self.selection.retain_valid(&self.document);
                self.status = legacy_replay_status("Redid", &label, &skipped_entries);
            }
            Ok(None) => self.status = "Nothing to redo.".to_owned(),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn can_undo_document(&self) -> bool {
        self.history.can_undo()
            || self
                .legacy_history
                .as_ref()
                .and_then(|history| history.previous_entry_index(self.legacy_active_index?))
                .is_some()
    }

    fn can_redo_document(&self) -> bool {
        self.history.can_redo()
            || self
                .legacy_history
                .as_ref()
                .and_then(|history| history.next_entry_index(self.legacy_active_index?))
                .is_some()
    }

    fn run_command(&mut self, id: &str) {
        let state = self.command_state(id);
        let outcome = shell_commands().dispatch(
            id,
            &CommandPayload::default(),
            &state,
            |action, _payload| self.execute_command(*action),
        );
        if !outcome.accepted {
            self.status = outcome
                .result
                .diagnostics
                .first()
                .cloned()
                .unwrap_or_else(|| format!("Command `{id}` was rejected."));
        }
    }

    fn execute_command(&mut self, action: ShellAction) -> CommandResult {
        match action {
            ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
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
                match self.commit_document_edit("Add cuboid", |document| {
                    document
                        .add_cuboid(
                            "cuboid",
                            Vec3::new(2.0, 2.0, 2.0),
                            Vec3::new(-1.0, 0.0, -1.0),
                        )
                        .ok_or_else(|| "could not create a cuboid".to_owned())
                }) {
                    Ok(occurrence) => {
                        self.set_selection([occurrence]);
                        self.status = "Created a 2 × 2 × 2 cuboid.".to_owned();
                    }
                    Err(error) => self.status = error,
                }
            }
            ShellAction::CopySelection => self.copy_selection(),
            ShellAction::CreateObject => self.create_object_from_selection(),
            ShellAction::ArraySelection => self.array_selection(),
            ShellAction::Undo => self.undo_document(),
            ShellAction::Redo => self.redo_document(),
            ShellAction::NewDocument => {
                match self.commit_document_edit("New document", |document| {
                    *document = Document::new();
                    Ok(())
                }) {
                    Ok(()) => {
                        self.selection.clear();
                        self.object_name_edit = None;
                        self.cuboid_first = None;
                        self.cuboid_base = None;
                        self.line_first = None;
                        self.rectangle_first = None;
                        self.status = "New empty document.".to_owned();
                    }
                    Err(error) => self.status = error,
                }
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
            ShellAction::OpenPanel(panel) => self.open_panels(&[panel]),
            ShellAction::OpenPanelGroup(panels) => self.open_panels(panels),
            ShellAction::Unsupported => {
                return CommandResult::failed("this command is not implemented in this build");
            }
            ShellAction::About => {
                self.status = "Meshcraft · Rust rewrite · development build".to_owned();
            }
        }
        CommandResult::completed()
    }

    fn open_panels(&mut self, panels: &[&str]) {
        self.preferences.panels_visible = true;
        for panel in panels {
            self.preferences
                .panel_open
                .insert((*panel).to_owned(), true);
        }
        self.panel_focus = panels.first().map(|panel| (*panel).to_owned());
        self.status = "Opened the requested inspector panel.".to_owned();
    }

    fn command_state(&self, id: &str) -> CommandState {
        let Some(command) = shell_commands().get(id) else {
            return CommandState::disabled(format!("unknown command `{id}`"));
        };
        if command.descriptor.capability == CommandCapability::Deferred {
            return CommandState::disabled(format!(
                "{} is not implemented in this build",
                command.descriptor.label
            ));
        }
        match &command.descriptor.state_query {
            CommandStateQuery::Always => CommandState::default(),
            CommandStateQuery::UndoAvailable if !self.can_undo_document() => {
                CommandState::disabled("there is no earlier document state")
            }
            CommandStateQuery::RedoAvailable if !self.can_redo_document() => {
                CommandState::disabled("there is no later document state")
            }
            CommandStateQuery::SelectionAvailable if self.selected_occurrences().is_empty() => {
                CommandState::disabled("select an object first")
            }
            CommandStateQuery::RootSelectionAvailable => {
                let selection = self.selected_occurrences();
                if selection.is_empty() || selection.iter().any(|item| item.path.len() != 1) {
                    CommandState::disabled("select one or more root objects first")
                } else {
                    CommandState::default()
                }
            }
            CommandStateQuery::SingleRootSelection => {
                let selection = self.selected_occurrences();
                if selection.len() != 1 || selection[0].path.len() != 1 {
                    CommandState::disabled("select one root object first")
                } else {
                    CommandState::default()
                }
            }
            CommandStateQuery::ActiveTool(tool) => CommandState {
                checked: Some(self.active_tool == *tool),
                ..CommandState::default()
            },
            CommandStateQuery::ActiveProfile(profile) => CommandState {
                checked: Some(self.preferences.profile.label() == profile),
                ..CommandState::default()
            },
            CommandStateQuery::PreferenceEnabled(key) => CommandState {
                checked: Some(match key.as_str() {
                    "toolbars" => self.preferences.toolbars_visible,
                    "panels" => self.preferences.panels_visible,
                    "grid" => self.preferences.show_permanent_grid,
                    _ => false,
                }),
                ..CommandState::default()
            },
            CommandStateQuery::UndoAvailable
            | CommandStateQuery::RedoAvailable
            | CommandStateQuery::SelectionAvailable => CommandState::default(),
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
        match self.commit_document_edit("Copy object", |document| {
            document
                .copy_root_occurrence(&source, Vec3::new(span_x + 0.5, 0.0, 0.0))
                .ok_or_else(|| "Copy could not create a root object instance.".to_owned())
        }) {
            Ok(copy) => {
                self.set_selection([copy]);
                self.status =
                    "Copied the selected object; the copy shares its definition.".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn create_object_from_selection(&mut self) {
        let selection = self.selected_occurrences();
        match self.commit_document_edit("Create object", |document| {
            document
                .create_object_from_root_occurrences(&selection, "Object")
                .ok_or_else(|| {
                    "Create Object requires one or more selected root objects.".to_owned()
                })
        }) {
            Ok(object) => {
                self.set_selection([object]);
                self.status = "Created an object definition from the selected geometry.".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn array_selection(&mut self) {
        let selection = self.selected_occurrences();
        if selection.len() != 1 || selection[0].path.len() != 1 {
            self.status = "Array requires one selected root object.".to_owned();
            return;
        }
        let source = selection[0].clone();
        let count = self.array_count;
        let spacing = self.array_spacing;
        match self.commit_document_edit("Array object", |document| {
            document
                .array_root_occurrence(&source, count, Vec3::new(spacing, 0.0, 0.0))
                .ok_or_else(|| "Array could not create the requested object instances.".to_owned())
        }) {
            Ok(copies) => {
                self.set_selection(copies);
                self.status = format!("Created {} array copies.", self.array_count);
            }
            Err(error) => self.status = error,
        }
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
                match self.commit_document_edit("Create line", |document| {
                    document
                        .add_line_segment("line", start, point)
                        .ok_or_else(|| "Line endpoints must be distinct finite points.".to_owned())
                }) {
                    Ok(occurrence) => {
                        self.set_selection([occurrence]);
                        self.active_tool = "Select".to_owned();
                        self.line_first = None;
                        self.status =
                            format!("Created line {:.2} units long.", start.distance(point));
                    }
                    Err(error) => self.status = error,
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
                match self.commit_document_edit("Create rectangle", |document| {
                    document
                        .add_ground_rectangle("rectangle", first, point)
                        .ok_or_else(|| "Rectangle sides must be distinct finite points.".to_owned())
                }) {
                    Ok(occurrence) => {
                        self.set_selection([occurrence]);
                        self.active_tool = "Select".to_owned();
                        self.rectangle_first = None;
                        self.status = format!(
                            "Created ground rectangle {:.2} × {:.2}.",
                            (point.x - first.x).abs(),
                            (point.z - first.z).abs()
                        );
                    }
                    Err(error) => self.status = error,
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
            match self.commit_document_edit("Create cuboid", |document| {
                document
                    .add_cuboid("cuboid", dimensions, origin)
                    .ok_or_else(|| "Cuboid could not be created.".to_owned())
            }) {
                Ok(occurrence) => {
                    self.set_selection([occurrence]);
                    self.active_tool = "Select".to_owned();
                    self.cuboid_first = None;
                    self.cuboid_base = None;
                    self.status = format!(
                        "Created cuboid {:.2} × {:.2} × {:.2}.",
                        dimensions.x, dimensions.y, dimensions.z
                    );
                }
                Err(error) => self.status = error,
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

    fn matching_commands(
        &self,
    ) -> Vec<&'static meshcraft_core::commands::CommandRegistration<ShellAction>> {
        shell_commands()
            .entries()
            .filter(|command| command_matches(&command.descriptor, &self.command_search))
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

    fn show_tool_button(
        &self,
        ui: &mut egui::Ui,
        command: &meshcraft_core::commands::CommandRegistration<ShellAction>,
        size: f32,
        icon_only: bool,
    ) -> bool {
        let ShellAction::SelectTool(tool) = command.handler else {
            return false;
        };
        let icon_name = command.descriptor.presentation.icon_key.as_deref();
        let selected = self.active_tool == tool;
        let icon_size = (size * 0.55).max(12.0);
        let button = match (
            self.icon_image(icon_name, Vec2::splat(icon_size)),
            icon_only,
        ) {
            (Some(icon), true) => egui::Button::image(icon),
            (Some(icon), false) => egui::Button::image_and_text(icon, &command.descriptor.label),
            (None, true) => egui::Button::new(
                command
                    .descriptor
                    .label
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_string(),
            ),
            (None, false) => egui::Button::new(&command.descriptor.label),
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
            .map(|command| {
                self.icon_image(
                    command.descriptor.presentation.icon_key.as_deref(),
                    Vec2::splat(16.0),
                )
            })
            .collect::<Vec<_>>()
            .into_iter();
        let command_states = commands
            .iter()
            .map(|command| self.command_state(command.descriptor.id.as_str()))
            .collect::<Vec<_>>();
        let available_count = command_states.iter().filter(|state| state.enabled).count();
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
                            let label = format!(
                                "[{}] {}",
                                command.descriptor.category,
                                command.descriptor.presentation.palette_path
                            );
                            let icon = command_icons.next().flatten();
                            let state = &command_states[index];
                            ui.horizontal(|ui| {
                                if let Some(icon) = icon {
                                    ui.add(icon);
                                }
                                let response = ui.add_enabled(
                                    state.enabled,
                                    egui::Button::selectable(
                                        index == self.command_selection,
                                        label,
                                    ),
                                );
                                if !state.enabled {
                                    response.clone().on_disabled_hover_text(
                                        state.disabled_reason.as_deref().unwrap_or("Unavailable"),
                                    );
                                }
                                if response.clicked() {
                                    selected_command =
                                        Some(command.descriptor.id.as_str().to_owned());
                                }
                            });
                        }
                    });
                ui.separator();
                ui.weak(format!(
                    "{available_count} available · {} cataloged · Enter to run · Esc to close",
                    commands.len(),
                ));
            });

        if let Some(id) = selected_command {
            self.run_command(&id);
            self.command_palette_open = false;
        }
    }

    fn show_command_menu_item(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(command) = shell_commands().get(id) else {
            return;
        };
        let label = command.descriptor.label.clone();
        let state = self.command_state(id);
        let response = ui.add_enabled(
            state.enabled,
            egui::Button::new(label).selected(state.checked.unwrap_or(false)),
        );
        if !state.enabled {
            response
                .clone()
                .on_disabled_hover_text(state.disabled_reason.as_deref().unwrap_or("Unavailable"));
        }
        if response.clicked() {
            self.run_command(id);
        }
    }

    fn show_command_toggle(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(command) = shell_commands().get(id) else {
            return;
        };
        let label = command.descriptor.label.clone();
        let state = self.command_state(id);
        let mut checked = state.checked.unwrap_or(false);
        if ui.checkbox(&mut checked, label).changed() {
            self.run_command(id);
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        let request_file_menu = std::mem::take(&mut self.open_file_menu);

        egui::MenuBar::new().ui(ui, |ui| {
            let file_menu = ui.menu_button("File", |ui| {
                self.show_command_menu_item(ui, "file.new");
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
                self.show_command_menu_item(ui, "edit.undo");
                self.show_command_menu_item(ui, "edit.redo");
                ui.add_enabled(false, egui::Button::new("Delete"));
                ui.add_enabled(false, egui::Button::new("Clear Selection"));
                ui.separator();
                self.show_command_menu_item(ui, "edit.copy_selection");
            });

            ui.menu_button("View", |ui| {
                let commands = shell_commands()
                    .entries()
                    .filter(|command| command.descriptor.category == "View")
                    .collect::<Vec<_>>();
                for command in commands {
                    let id = command.descriptor.id.as_str();
                    if matches!(
                        id,
                        "view.toolbars.toggle" | "view.panels.toggle" | "view.grid.toggle"
                    ) {
                        self.show_command_toggle(ui, id);
                    } else {
                        self.show_command_menu_item(ui, id);
                    }
                }
            });

            for menu in [
                "Draw", "Modify", "Objects", "Tools", "Render", "Import", "Export",
            ] {
                ui.menu_button(menu, |ui| {
                    let commands = shell_commands()
                        .entries()
                        .filter(|command| command.descriptor.category == menu)
                        .collect::<Vec<_>>();
                    if commands.is_empty() {
                        ui.label("No commands are registered in this menu yet.");
                    }
                    for command in commands {
                        self.show_command_menu_item(ui, command.descriptor.id.as_str());
                    }
                });
            }

            ui.menu_button("Window", |ui| {
                ui.label("Workspace panels");
                self.show_command_toggle(ui, "view.panels.toggle");
                ui.add_enabled(false, egui::Button::new("Reset Workspace Layout"));
            });

            ui.menu_button("Help", |ui| {
                ui.label("Meshcraft development build");
                ui.label("F10 or Alt+F opens the File menu.");
                ui.label("Ctrl+Shift+P opens the Command Palette.");
                self.show_command_menu_item(ui, "help.about");
            });
        });
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let tools = shell_commands()
            .entries()
            .filter(|command| {
                command.descriptor.presentation.toolbar_group.as_deref() == Some("Tools")
            })
            .map(|command| command.descriptor.id.as_str())
            .collect::<Vec<_>>();
        let size = self.preferences.toolbar_icon_size as f32;
        if self.preferences.toolbar_flow_top_down {
            ui.vertical(|ui| {
                ui.strong("Tools");
                for command_id in tools {
                    if let Some(command) = shell_commands().get(command_id)
                        && self.show_tool_button(ui, command, size, true)
                    {
                        self.run_command(command_id);
                    }
                }
            });
        } else {
            ui.horizontal(|ui| {
                ui.strong("Tools");
                ui.separator();
                for command_id in tools {
                    if let Some(command) = shell_commands().get(command_id)
                        && self.show_tool_button(ui, command, size, true)
                    {
                        self.run_command(command_id);
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
            for command in shell_commands().entries().filter(|command| {
                command.descriptor.presentation.toolbar_group.as_deref() == Some("Tools")
            }) {
                if self.show_tool_button(ui, command, size, false) {
                    self.run_command(command.descriptor.id.as_str());
                }
            }
            ui.add_space(12.0);
            for command in shell_commands().entries().filter(|command| {
                command.descriptor.presentation.toolbar_group.as_deref() == Some("Modeling")
            }) {
                let state = self.command_state(command.descriptor.id.as_str());
                let response =
                    ui.add_enabled(state.enabled, egui::Button::new(&command.descriptor.label));
                if !state.enabled {
                    response.clone().on_disabled_hover_text(
                        state.disabled_reason.as_deref().unwrap_or("Unavailable"),
                    );
                }
                if response.clicked() {
                    self.run_command(command.descriptor.id.as_str());
                }
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
        if self.panel_focus.as_deref() == Some(id) {
            ui.scroll_to_rect(response.rect, Some(Align::Min));
            self.panel_focus = None;
        }
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
            self.object_name_edit = None;
            ui.label("No object selected");
            return;
        };
        let Some(resolved) = self.document.resolve_occurrence(selected) else {
            ui.label("The selected object no longer exists");
            return;
        };
        ui.label(format!("{} selected", selection.len()));
        ui.label("Name");
        let definition_id = resolved.definition.id;
        let original_name = resolved.definition.name.clone();
        if self
            .object_name_edit
            .as_ref()
            .is_some_and(|(editing_id, _)| *editing_id != definition_id)
        {
            self.object_name_edit = None;
        }
        let mut name = self
            .object_name_edit
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or(original_name.clone());
        let response = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
        if response.changed() {
            self.object_name_edit = Some((definition_id, name.clone()));
        }
        let enter = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if (response.lost_focus() || enter)
            && let Some((editing_id, edited_name)) = self.object_name_edit.take()
            && editing_id == definition_id
            && edited_name != original_name
            && let Err(error) = self.commit_document_edit("Rename object", |document| {
                document
                    .rename_definition(definition_id, &edited_name)
                    .map(|_| ())
            })
        {
            self.status = error;
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
        if changed
            && let Err(error) = self.commit_document_edit("Change model settings", |document| {
                document.update_model_settings(settings).map(|_| ())
            })
        {
            ui.colored_label(Color32::LIGHT_RED, error);
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
                    self.run_command(command.descriptor.id.as_str());
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

#[cfg(test)]
fn tool_command_id(tool: &str) -> &'static str {
    shell_commands()
        .entries()
        .find_map(|command| match &command.handler {
            ShellAction::SelectTool(name) if *name == tool => Some(command.descriptor.id.as_str()),
            _ => None,
        })
        .unwrap_or("tool.select")
}

fn legacy_replay_status(action: &str, label: &str, skipped_entries: &[usize]) -> String {
    if skipped_entries.is_empty() {
        return format!("{action} legacy history: {label}.");
    }
    let skipped = skipped_entries
        .iter()
        .map(|index| (index + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("Skipped corrupt legacy entries {skipped}; {action} legacy history: {label}.")
}

fn command_matches(command: &CommandDescriptor, raw_query: &str) -> bool {
    let query = raw_query.trim().to_lowercase();
    query.split_whitespace().all(|term| {
        command
            .presentation
            .palette_path
            .to_lowercase()
            .contains(term)
            || command.id.as_str().to_lowercase().contains(term)
            || command.category.to_lowercase().contains(term)
            || command
                .keywords
                .iter()
                .any(|keyword| keyword.to_lowercase().contains(term))
    })
}

#[cfg(test)]
fn registry_has_unique_ids() -> bool {
    let mut ids = BTreeSet::new();
    shell_commands()
        .entries()
        .all(|command| ids.insert(command.descriptor.id.clone()))
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
        MeshcraftApp, Preferences, SelectionSession, TransactionHistory, command_matches,
        registry_has_unique_ids, shell_commands, tool_command_id,
    };
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use eframe::egui::{self, Event, ImeEvent, Key, Modifiers, Pos2, RawInput, Rect, Vec2};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use meshcraft_core::document::Document;
    use meshcraft_core::transactions::{OpaqueLegacyHistory, OpaqueLegacyUndoEntry};
    use meshcraft_io::project::read_project;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::Write;

    const LEGACY_MODEL: &str =
        include_str!("../../../fixtures/legacy/samples/examples/arch_frame.octd");

    fn snapshot_json(root_only: bool) -> String {
        let mut value: Value = serde_json::from_str(LEGACY_MODEL).unwrap();
        if root_only {
            value["rootInstance"]["children"] = json!([]);
        }
        value.to_string()
    }

    fn encode_history_snapshot(root_only: bool) -> String {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(snapshot_json(root_only).as_bytes())
            .unwrap();
        BASE64_STANDARD.encode(encoder.finish().unwrap())
    }

    fn app_with_legacy_history(corrupt_older_entry: bool) -> MeshcraftApp {
        let older_snapshot = if corrupt_older_entry {
            "%%% invalid base64 %%%".to_owned()
        } else {
            encode_history_snapshot(false)
        };
        let current_snapshot = encode_history_snapshot(true);
        let current_project = read_project(snapshot_json(true).as_bytes()).unwrap();
        let legacy_history = OpaqueLegacyHistory {
            max_entries: 10,
            index: 1,
            entries: vec![
                OpaqueLegacyUndoEntry {
                    data: older_snapshot,
                    label: Some("Before import".to_owned()),
                    timestamp_millis: 100,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
                OpaqueLegacyUndoEntry {
                    data: current_snapshot,
                    label: Some("Imported state".to_owned()),
                    timestamp_millis: 200,
                    diagnostic: None,
                    extensions: BTreeMap::new(),
                },
            ],
            extensions: BTreeMap::new(),
        };
        let active_index = legacy_history.active_entry_index();
        let mut app = test_app();
        app.document = current_project.document;
        app.legacy_active_index = active_index;
        app.legacy_history = Some(legacy_history);
        app
    }

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
            panel_focus: None,
            document: Document::new(),
            history: TransactionHistory::default(),
            legacy_history: None,
            legacy_active_index: None,
            selection: SelectionSession::default(),
            object_name_edit: None,
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
    fn pinned_legacy_command_ids_are_cataloged_with_explicit_availability() {
        let entries = shell_commands().entries().collect::<Vec<_>>();
        assert_eq!(155, entries.len());
        assert_eq!(
            74,
            entries
                .iter()
                .filter(|command| command.descriptor.id.as_str().starts_with("tool.builtin."))
                .count()
        );
        assert_eq!(
            6,
            entries
                .iter()
                .filter(|command| command.descriptor.id.as_str().starts_with("view.ortho."))
                .count()
        );
        assert_eq!(
            24,
            entries
                .iter()
                .filter(|command| command.descriptor.id.as_str().starts_with("view.toolbar."))
                .count()
        );
        assert!(shell_commands().get("edit.group").is_some());
        assert!(shell_commands().get("export.screenshot").is_some());
        assert!(shell_commands().get("import.mesh").is_some());
        assert!(entries.iter().any(|command| {
            command.descriptor.id.as_str() == "tool.builtin.axial_grid"
                && command.descriptor.capability
                    == meshcraft_core::commands::CommandCapability::Deferred
        }));
    }

    #[test]
    fn every_visible_tool_has_a_registry_command() {
        for tool in ["Select", "Cuboid", "Line", "Rectangle", "Orbit"] {
            let id = tool_command_id(tool);
            assert!(shell_commands().get(id).is_some());
        }
        assert_eq!(
            5,
            shell_commands()
                .entries()
                .filter(|command| {
                    command.descriptor.presentation.toolbar_group.as_deref() == Some("Tools")
                })
                .count()
        );
        assert!(shell_commands().entries().all(|command| {
            command.descriptor.presentation.menu_path.len() == 1
                && !command.descriptor.presentation.palette_path.is_empty()
        }));
    }

    #[test]
    fn command_state_blocks_wrong_context_and_tool_activation_has_no_document_edit() {
        let mut app = test_app();
        let initial_revision = app.document.revision();
        assert!(!app.command_state("edit.undo").enabled);
        assert!(!app.command_state("edit.copy_selection").enabled);

        app.run_command("edit.undo");
        assert!(app.status.contains("no earlier document state"));
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(0, app.history.entries().len());

        app.run_command("tool.cuboid");
        assert_eq!("Cuboid", app.active_tool);
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(0, app.history.entries().len());
        assert_eq!(
            meshcraft_core::commands::CommandKind::ToolActivation,
            shell_commands().get("tool.cuboid").unwrap().descriptor.kind
        );
    }

    #[test]
    fn legacy_inspector_commands_open_and_focus_the_matching_panel() {
        let mut app = test_app();
        app.preferences.panels_visible = false;
        app.preferences
            .panel_open
            .insert("model_settings".to_owned(), false);

        app.run_command("view.model_settings");

        assert!(app.preferences.panels_visible);
        assert_eq!(
            Some(&true),
            app.preferences.panel_open.get("model_settings")
        );
        assert_eq!(Some("model_settings"), app.panel_focus.as_deref());

        app.run_command("view.architecture_settings");
        for panel in [
            "architecture_walls",
            "architecture_slabs",
            "architecture_stairs",
            "architecture_frames",
        ] {
            assert_eq!(Some(&true), app.preferences.panel_open.get(panel));
        }
        assert_eq!(Some("architecture_walls"), app.panel_focus.as_deref());
    }

    #[test]
    fn deferred_commands_are_discoverable_but_report_their_unsupported_state() {
        let mut app = test_app();
        assert!(shell_commands().get("view.plugin_manager").is_some());
        assert!(!app.command_state("view.plugin_manager").enabled);
        assert!(
            app.command_state("view.plugin_manager")
                .disabled_reason
                .is_some_and(|reason| reason.contains("not implemented"))
        );
        app.run_command("view.plugin_manager");
        assert!(app.status.contains("not implemented"));
    }

    #[test]
    fn modeling_workflow_commands_are_searchable_in_the_palette() {
        for (id, query) in [
            ("edit.undo", "history revert"),
            ("edit.redo", "history restore"),
            ("geometry.add_cuboid", "cuboid"),
            ("edit.copy_selection", "duplicate"),
            ("object.create_from_selection", "prototype"),
            ("object.array_selection", "array"),
        ] {
            let command = shell_commands()
                .get(id)
                .unwrap_or_else(|| panic!("missing command {id}"));
            assert!(
                command_matches(&command.descriptor, query),
                "command {id} misses {query}"
            );
        }
    }

    #[test]
    fn add_undo_redo_commands_share_the_document_transaction_history() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert!(app.history.can_undo());
        assert_eq!(1, app.history.entries().len());

        app.run_command("edit.undo");
        assert!(app.document.occurrence_refs().unwrap().is_empty());
        assert!(app.history.can_redo());

        app.run_command("edit.redo");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert!(!app.history.can_redo());
        assert!(app.selection.selected().is_empty());
    }

    #[test]
    fn reopening_with_legacy_history_supports_edit_undo_and_lazy_legacy_replay() {
        let mut app = app_with_legacy_history(false);
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert!(app.can_undo_document());
        assert!(!app.can_redo_document());

        app.run_command("geometry.add_cuboid");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.undo");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert_eq!(0, app.history.cursor());

        app.run_command("edit.undo");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        assert_eq!(Some(0), app.legacy_active_index);
        assert!(app.status.contains("Before import"));
        assert!(!app.can_undo_document());
        assert!(app.can_redo_document());

        app.run_command("edit.redo");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert_eq!(Some(1), app.legacy_active_index);
        app.run_command("edit.redo");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        assert_eq!(2, app.history.entries().len());
    }

    #[test]
    fn corrupt_legacy_undo_entry_is_diagnosed_without_changing_the_document() {
        let mut app = app_with_legacy_history(true);
        let before = app.document.clone();
        let original = app.legacy_history.as_ref().unwrap().entries[0].data.clone();

        app.run_command("edit.undo");

        assert_eq!(before, app.document);
        assert_eq!(Some(1), app.legacy_active_index);
        assert!(app.history.entries().is_empty());
        let history = app.legacy_history.as_ref().unwrap();
        assert_eq!(original, history.entries[0].data);
        assert!(history.entries[0].diagnostic.is_some());
        assert!(
            app.status
                .contains("No earlier legacy snapshot could be loaded")
        );
    }

    #[test]
    fn legacy_undo_skips_a_corrupt_neighbor_and_keeps_its_diagnostic() {
        let mut app = app_with_legacy_history(false);
        let history = app.legacy_history.as_mut().unwrap();
        history.entries.insert(
            1,
            OpaqueLegacyUndoEntry {
                data: "%%% broken middle snapshot %%%".to_owned(),
                label: Some("Corrupt middle state".to_owned()),
                timestamp_millis: 150,
                diagnostic: None,
                extensions: BTreeMap::new(),
            },
        );
        history.index = 2;
        app.legacy_active_index = history.active_entry_index();

        app.run_command("edit.undo");

        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        assert_eq!(Some(0), app.legacy_active_index);
        assert!(
            app.legacy_history.as_ref().unwrap().entries[1]
                .diagnostic
                .is_some()
        );
        assert!(app.status.contains("Skipped corrupt legacy entries 2"));
    }

    #[test]
    fn rectangle_search_metadata_is_present() {
        let rectangle = &shell_commands().get("tool.rectangle").unwrap().descriptor;
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
            let command = shell_commands()
                .get(id)
                .unwrap_or_else(|| panic!("missing command {id}"));
            assert!(command_matches(&command.descriptor, query));
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
                .any(|command| { command.descriptor.id.as_str() == "tool.rectangle" })
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
