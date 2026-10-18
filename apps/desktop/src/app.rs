use crate::assets::{self, IconRegion};
use crate::viewport::{
    ConstructionGuide, ConstructionGuideKind, CuboidPreview, GridNormalAxis, GridStyle, OrthoView,
    SnapOptions, SnapResult, SnapScope, SnapVisibility, ToolPreview, Viewport, ViewportScene,
    WalkthroughTuning,
};
use eframe::egui::{self, Align, Align2, Color32, Layout, Vec2};
use meshcraft_core::commands::HostEffect;
use meshcraft_core::commands::{
    CommandCapability, CommandCompletion, CommandDescriptor, CommandDispatch, CommandKind,
    CommandParameter, CommandPayload, CommandPresentation, CommandRegistry, CommandResult,
    CommandState, CommandStateQuery, CommandValueType,
};
use meshcraft_core::document::{DefinitionId, Document, LayoutEntryId, OccurrenceRef};
use meshcraft_core::geometry::Vec3;
use meshcraft_core::selection::{EditContext, SelectedEntity, SelectionSession, SelectionTarget};
use meshcraft_core::transactions::{
    HistoryEntryOrigin, OpaqueLegacyHistory, SystemHistoryClock, TransactionHistory,
};
#[cfg(not(target_arch = "wasm32"))]
use meshcraft_io::project::{
    DecodeProgress, encode_native_history, read_project_with, write_project,
};
use meshcraft_io::project::{ImportedProject, LegacySnapshotAdapter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::fs::{self, OpenOptions};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{self, Receiver, TryRecvError};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

const PREFERENCES_KEY: &str = "meshcraft.preferences";
#[cfg(not(target_arch = "wasm32"))]
const MAX_PROJECT_BYTES: u64 = 512 * 1024 * 1024;
const ARCHITECTURE_PANEL_IDS: &[&str] = &[
    "architecture_walls",
    "architecture_slabs",
    "architecture_stairs",
    "architecture_frames",
];
const INSPECTOR_PANELS: &[(&str, &str)] = &[
    ("selection", "Selection"),
    ("object", "Object"),
    ("objects", "Objects"),
    ("model_settings", "Model Settings"),
    ("profile", "Workspace Profile"),
    ("ui_settings", "UI Settings"),
    ("help", "Help / About"),
    ("polyline", "Polyline Settings"),
    ("vector_text", "Vector Text"),
    ("architecture_walls", "Architecture - Walls"),
    ("architecture_slabs", "Architecture - Slabs"),
    ("architecture_stairs", "Architecture - Stairs"),
    ("architecture_frames", "Architecture - Frames"),
    ("hvac", "HVAC Settings"),
    ("hotspots", "Hotspot Settings"),
    ("random", "Random Tools"),
    ("tutorials", "Tutorials"),
    ("shaders", "Shader Settings"),
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
    ("builtin_toolbar_actions", "Actions"),
    ("builtin_toolbar_modification", "Modification"),
    (
        "builtin_toolbar_construction_entities",
        "Entity Construction",
    ),
    ("builtin_toolbar_rendering", "Rendering"),
    ("builtin_toolbar_architecture", "Architecture"),
    ("builtin_toolbar_camera", "Camera"),
    ("builtin_toolbar_modification_2", "Modification 2"),
    ("builtin_toolbar_mech", "Mech"),
    ("builtin_toolbar_primitives", "Primitives"),
    ("builtin_toolbar_voxel", "Voxel"),
    ("builtin_toolbar_in_tool_operators", "In-tool Operators"),
    ("builtin_toolbar_hvac", "HVAC"),
];
const LEGACY_TOOLBAR_TOOL_IDS: &[(&str, &[&str])] = &[
    (
        "construction_points",
        &[
            "axial_grid",
            "planar_grid",
            "line",
            "construction_line",
            "polyline",
            "double_line",
            "ribbon_3d",
            "rectangle",
            "surface_rectangle",
            "face_outline",
            "polyline_mesh",
            "object_cut",
            "solid_union",
            "solid_intersection",
            "solid_subtraction",
            "mesh",
            "quad",
            "circle",
        ],
    ),
    (
        "construction_entities",
        &[
            "linear_dimension",
            "text",
            "vector_text",
            "line_offset",
            "cut_out_3",
            "push_pull",
            "extrude_swipe",
            "extrude_swipe_3d",
            "plane_section",
            "mesh_intersection",
            "cut_with_plane",
            "revolve",
            "mesh_regularize",
            "surface_remesh",
        ],
    ),
    (
        "modification",
        &[
            "select",
            "move",
            "rotate",
            "scale",
            "stretch",
            "stretch_scale",
            "random_offset",
            "random_surface_array",
            "rotate_stretch",
            "copy_multiple",
            "planar_translate_multiple",
            "volumetric_translate_multiple",
            "planar_rotate_multiple",
            "helicoidal_rotate_multiple",
            "paint",
        ],
    ),
    (
        "modification_2",
        &[
            "rotate_2",
            "rotational_array",
            "helical_array",
            "rotational_array_equalized",
            "helical_array_equalized",
        ],
    ),
    ("voxel", &["voxel", "voxel_volume", "voxel_frame"]),
    (
        "architecture",
        &[
            "arch_wall",
            "arch_slab",
            "arch_stair",
            "arch_add_hole",
            "arch_window_frame",
            "arch_door_frame",
        ],
    ),
    ("hvac", &["hvac_plumbing", "hvac_ventilation"]),
    (
        "primitives",
        &[
            "primitive_sphere",
            "primitive_cylinder",
            "primitive_cone",
            "primitive_pill",
        ],
    ),
    (
        "mech",
        &[
            "mech_screw",
            "mech_circular_hole",
            "mech_round_washer",
            "mech_cog_wheel",
        ],
    ),
];
const LEGACY_TOOLBAR_COMMAND_IDS: &[(&str, &[&str])] = &[
    (
        "actions",
        &[
            "file.open",
            "file.save_as",
            "edit.undo",
            "edit.redo",
            "edit.copy_selection",
            "edit.delete_selection",
            "object.create_from_selection",
            "object.array_selection",
        ],
    ),
    (
        "camera",
        &[
            "view.camera.reset",
            "view.frame_document",
            "view.camera.walkthrough",
            "view.camera.orthographic",
            "view.ortho.top",
            "view.orbit",
        ],
    ),
    (
        "rendering",
        &[
            "render.raytrace",
            "render.global_illumination",
            "render.stop",
            "render.save",
        ],
    ),
];
const LEGACY_TOOL_ICON_KEYS: &[(&str, &str)] = &[
    ("axial_grid", "grid_helper_axial"),
    ("planar_grid", "grid_helper_planar"),
    ("ribbon_3d", "double_line"),
    ("arch_add_hole", "arch_hole"),
    ("arch_window_frame", "arch_window"),
    ("arch_door_frame", "arch_door"),
    ("primitive_sphere", "sphere"),
    ("primitive_cylinder", "cylinder"),
    ("primitive_cone", "cone"),
    ("primitive_pill", "pill"),
    ("mech_screw", "mech_screw_surface"),
    ("mech_circular_hole", "mech_round_hole"),
    ("mech_round_washer", "mech_washer_round"),
    ("object_cut", "mesh_intersection"),
    ("solid_intersection", "solid_intersect"),
    ("solid_subtraction", "solid_subtract"),
    ("cut_holes", "cleanup"),
    ("cut_holes_2", "cleanup"),
    ("line_offset", "offset"),
    ("cut_out_3", "cleanup"),
    ("extrude_swipe", "swipe_surface"),
    ("extrude_swipe_3d", "swipe_surface"),
    ("cut_with_plane", "plane_section"),
    ("surface_rectangle", "surface_rect"),
    ("linear_dimension", "dimension"),
    ("vector_text", "vectorial-text"),
    ("rotate_2", "rotate"),
    ("random_offset", "fuzzy_offset"),
    ("random_surface_array", "fuzzy_cover"),
    ("rotate_stretch", "stretch-rotate"),
    ("copy_multiple", "multiple-copy-translate"),
    (
        "planar_translate_multiple",
        "multiple-copy-translate-planar",
    ),
    (
        "volumetric_translate_multiple",
        "multiple-copy-translate-volumetric",
    ),
    ("planar_rotate_multiple", "multiple-copy-planar-rotate"),
    (
        "helicoidal_rotate_multiple",
        "multiple-copy-helicoidal-rotate",
    ),
    ("rotational_array", "multiple-copy-planar-rotate"),
    ("helical_array", "multiple-copy-helicoidal-rotate"),
    ("rotational_array_equalized", "multiple-copy-planar-rotate"),
    ("helical_array_equalized", "multiple-copy-helicoidal-rotate"),
    ("surface_remesh", "mesh_regularize"),
    ("object_place", "select"),
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
    (
        "render.raytrace",
        "Rendering> RT",
        "Rendering",
        "raytrace real-time rendering start",
        Some("rendering_raytrace"),
    ),
    (
        "render.global_illumination",
        "Rendering> GI",
        "Rendering",
        "global illumination path tracing rendering start",
        Some("rendering_global_illumination"),
    ),
    (
        "render.stop",
        "Rendering> Stop",
        "Rendering",
        "stop cancel rendering",
        None,
    ),
    (
        "render.save",
        "Rendering> Save",
        "Rendering",
        "save rendered image output",
        None,
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
    PlaceGuide(ConstructionGuideKind),
    RemoveGuide,
    AddCuboid,
    CopySelection,
    DeleteSelection,
    ClearSelection,
    ResetSceneForCapture,
    SnapshotDocument,
    SetSelection,
    CreateObject,
    ArraySelection,
    UpdateModelSettings,
    UpdateUiPreferences,
    Undo,
    Redo,
    NewDocument,
    OpenProject,
    CancelProjectLoad,
    SaveProject,
    SaveProjectAs,
    ResetWorkspaceLayout,
    Quit,
    SetProfile(UiProfile),
    SetOrthographicView(OrthoView),
    SetWalkthroughView,
    FrameDocument,
    ResetCamera,
    SetCameraPose,
    TogglePanelSection(&'static str),
    ToggleToolbars,
    SetToolbarVisibility(&'static str, bool),
    TogglePanels,
    ToggleGrid,
    ToggleGuides,
    ClearGuides,
    OpenPanel(&'static str),
    OpenPanelGroup(&'static [&'static str]),
    Unsupported,
    About,
}

#[derive(Clone, Debug)]
enum ReplacementTarget {
    NewDocument,
    #[cfg(not(target_arch = "wasm32"))]
    OpenProject(PathBuf),
}

#[derive(Clone, Debug)]
struct PendingReplacement {
    job_id: String,
    target: ReplacementTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplacementDecision {
    #[cfg(not(target_arch = "wasm32"))]
    Save,
    Discard,
    Cancel,
}

#[cfg(not(target_arch = "wasm32"))]
enum ProjectLoadEvent {
    ReadProgress { read_bytes: usize, total_bytes: u64 },
    Progress(DecodeProgress),
    Finished(Box<Result<ImportedProject, String>>),
}

#[cfg(not(target_arch = "wasm32"))]
struct PendingProjectLoad {
    job_id: String,
    path: PathBuf,
    input_revision: u64,
    cancellation: Arc<AtomicBool>,
    receiver: Receiver<ProjectLoadEvent>,
}

#[cfg(not(target_arch = "wasm32"))]
struct PendingProjectSave {
    job_id: String,
    path: PathBuf,
    input_revision: u64,
    dropped_legacy_history: bool,
    receiver: Receiver<Result<(), String>>,
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
                "model.update_settings",
                "Model> Settings",
                "Model",
                "unit size grid circle segments snap radius",
                None,
                ShellAction::UpdateModelSettings,
            ),
            (
                "ui.update_preferences",
                "View> UI Settings",
                "View",
                "interface scale toolbar grid layout preferences",
                None,
                ShellAction::UpdateUiPreferences,
            ),
            (
                "document.snapshot",
                "Model> Snapshot Document",
                "Model",
                "snapshot scene capture automation regression state",
                None,
                ShellAction::SnapshotDocument,
            ),
            (
                "selection.set",
                "Selection> Set Occurrences",
                "Selection",
                "select set occurrence paths automation setup",
                None,
                ShellAction::SetSelection,
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
                "edit.delete_selection",
                "Edit> Delete Selection",
                "Edit",
                "remove erase objects",
                None,
                ShellAction::DeleteSelection,
            ),
            (
                "edit.clear_selection",
                "Edit> Clear Selection",
                "Edit",
                "deselect unselect selection",
                None,
                ShellAction::ClearSelection,
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
                "file.open",
                "File> Open Project…",
                "File",
                "load project octd legacy file",
                None,
                ShellAction::OpenProject,
            ),
            (
                "file.cancel_open",
                "File> Cancel Open",
                "File",
                "cancel stop loading project",
                None,
                ShellAction::CancelProjectLoad,
            ),
            (
                "file.save",
                "File> Save",
                "File",
                "write save project octd k3d",
                None,
                ShellAction::SaveProject,
            ),
            (
                "file.save_as",
                "File> Save As…",
                "File",
                "write export project choose path",
                None,
                ShellAction::SaveProjectAs,
            ),
            (
                "app.quit",
                "File> Exit",
                "File",
                "quit close exit application",
                None,
                ShellAction::Quit,
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
                "view.frame_document",
                "View> Frame Document",
                "View",
                "camera zoom fit all",
                None,
                ShellAction::FrameDocument,
            ),
            (
                "view.camera.reset",
                "View> Reset Camera",
                "View",
                "camera default reset isometric",
                None,
                ShellAction::ResetCamera,
            ),
            (
                "view.camera.set_pose",
                "View> Set Camera Pose",
                "View",
                "camera position target field of view automation",
                None,
                ShellAction::SetCameraPose,
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
                "view.guides.toggle",
                "View> Toggle Construction Guides",
                "View",
                "construction guide visibility show hide display",
                None,
                ShellAction::ToggleGuides,
            ),
            (
                "view.guides.clear",
                "View> Clear Construction Guides",
                "View",
                "construction guide remove delete clear all",
                None,
                ShellAction::ClearGuides,
            ),
            (
                "view.guide.remove",
                "View> Remove Guide",
                "View",
                "construction guide remove delete pick click",
                None,
                ShellAction::RemoveGuide,
            ),
            (
                "view.workspace.reset_layout",
                "View> Reset Workspace Layout",
                "View",
                "workspace layout reset default panels toolbars",
                None,
                ShellAction::ResetWorkspaceLayout,
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
        for (panel_id, label) in INSPECTOR_PANELS {
            let id = format!("view.panel.{panel_id}.toggle");
            let name = format!("View> Panels> Toggle {label}");
            let keywords = format!("panel inspector toggle {}", label.to_lowercase());
            register_shell_command(
                &mut registry,
                &id,
                &name,
                "View",
                &keywords,
                Some("view"),
                ShellAction::TogglePanelSection(panel_id),
            );
        }
        for (tool_id, label) in LEGACY_BUILTIN_TOOLS {
            let id = format!("tool.builtin.{tool_id}");
            let name = format!("Tool> {label}");
            let keywords = format!("{} tool", label.to_lowercase());
            let icon = LEGACY_TOOL_ICON_KEYS
                .iter()
                .find(|(id, _)| id == tool_id)
                .map_or(*tool_id, |(_, icon)| *icon);
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
                Some(icon),
                action,
            );
        }
        for (id, name, category, keywords, icon) in LEGACY_COMMANDS {
            let action = match *id {
                "edit.group" => ShellAction::CreateObject,
                "edit.reset_scene_for_capture" => ShellAction::ResetSceneForCapture,
                "view.axis_guide" => ShellAction::PlaceGuide(ConstructionGuideKind::Axis),
                "view.grid_guide" => ShellAction::PlaceGuide(ConstructionGuideKind::Grid),
                "view.camera.orbit" => ShellAction::SelectToolAlias("Orbit"),
                "view.camera.walkthrough" => ShellAction::SetWalkthroughView,
                "view.camera.orthographic" => ShellAction::SetOrthographicView(OrthoView::Top),
                _ => ShellAction::Unsupported,
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
                ShellAction::SetOrthographicView(
                    OrthoView::from_id(view_id).expect("registered orthographic view is valid"),
                ),
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
                let action =
                    ShellAction::SetToolbarVisibility(toolbar_visibility_key(stem), verb == "show");
                register_shell_command(
                    &mut registry,
                    &id,
                    &name,
                    "View",
                    &keywords,
                    Some("view"),
                    action,
                );
            }
        }
        registry
    })
}

fn action_kind(action: ShellAction) -> CommandKind {
    match action {
        ShellAction::SelectTool(_)
        | ShellAction::SelectToolAlias(_)
        | ShellAction::PlaceGuide(_)
        | ShellAction::RemoveGuide => CommandKind::ToolActivation,
        ShellAction::AddCuboid
        | ShellAction::CopySelection
        | ShellAction::DeleteSelection
        | ShellAction::ResetSceneForCapture
        | ShellAction::CreateObject
        | ShellAction::ArraySelection
        | ShellAction::UpdateModelSettings
        | ShellAction::Undo
        | ShellAction::Redo
        | ShellAction::NewDocument => CommandKind::DocumentMutation,
        ShellAction::OpenProject
        | ShellAction::CancelProjectLoad
        | ShellAction::SaveProject
        | ShellAction::SaveProjectAs
        | ShellAction::Quit
        | ShellAction::Unsupported => CommandKind::HostRequest,
        ShellAction::SetProfile(_)
        | ShellAction::SetOrthographicView(_)
        | ShellAction::SetWalkthroughView
        | ShellAction::UpdateUiPreferences
        | ShellAction::FrameDocument
        | ShellAction::ResetCamera
        | ShellAction::SetCameraPose
        | ShellAction::ClearSelection
        | ShellAction::TogglePanelSection(_)
        | ShellAction::ToggleToolbars
        | ShellAction::SetToolbarVisibility(_, _)
        | ShellAction::TogglePanels
        | ShellAction::ToggleGrid
        | ShellAction::ToggleGuides
        | ShellAction::ClearGuides
        | ShellAction::ResetWorkspaceLayout
        | ShellAction::OpenPanel(_)
        | ShellAction::OpenPanelGroup(_) => CommandKind::SessionMutation,
        ShellAction::SetSelection => CommandKind::SessionMutation,
        ShellAction::SnapshotDocument | ShellAction::About => CommandKind::ReadOnly,
    }
}

fn action_state_query(action: ShellAction) -> CommandStateQuery {
    match action {
        ShellAction::Undo => CommandStateQuery::UndoAvailable,
        ShellAction::Redo => CommandStateQuery::RedoAvailable,
        ShellAction::CopySelection => CommandStateQuery::SingleRootSelection,
        ShellAction::DeleteSelection => CommandStateQuery::DeleteSelectionAvailable,
        ShellAction::ClearSelection => CommandStateQuery::SelectionAvailable,
        ShellAction::CreateObject => CommandStateQuery::RootSelectionAvailable,
        ShellAction::ArraySelection => CommandStateQuery::SingleRootSelection,
        ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
            CommandStateQuery::ActiveTool(tool.to_owned())
        }
        ShellAction::PlaceGuide(ConstructionGuideKind::Axis) => {
            CommandStateQuery::ActiveTool("Axis Guide".to_owned())
        }
        ShellAction::PlaceGuide(ConstructionGuideKind::Grid) => {
            CommandStateQuery::ActiveTool("Grid Guide".to_owned())
        }
        ShellAction::RemoveGuide => CommandStateQuery::ActiveTool("Remove Guide".to_owned()),
        ShellAction::SetProfile(profile) => {
            CommandStateQuery::ActiveProfile(profile.label().to_owned())
        }
        ShellAction::ToggleToolbars => CommandStateQuery::PreferenceEnabled("toolbars".to_owned()),
        ShellAction::SetToolbarVisibility(stem, shown) => CommandStateQuery::PreferenceEnabled(
            format!("toolbar:{stem}:{}", if shown { "shown" } else { "hidden" }),
        ),
        ShellAction::TogglePanels => CommandStateQuery::PreferenceEnabled("panels".to_owned()),
        ShellAction::ToggleGrid => CommandStateQuery::PreferenceEnabled("grid".to_owned()),
        ShellAction::ToggleGuides => CommandStateQuery::PreferenceEnabled("guides".to_owned()),
        ShellAction::TogglePanelSection(panel) => {
            CommandStateQuery::PreferenceEnabled(format!("panel:{panel}"))
        }
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
    descriptor.parameters = command_parameters(id);
    if id == "file.open" {
        descriptor.shortcut = Some("Ctrl+O".to_owned());
    }
    if id == "file.save" {
        descriptor.shortcut = Some("Ctrl+S".to_owned());
    }
    if id == "app.quit" {
        descriptor.shortcut = Some("Ctrl+Q".to_owned());
    }
    if id == "edit.delete_selection" {
        descriptor.shortcut = Some("Delete".to_owned());
    }
    descriptor.capability = match action {
        ShellAction::SelectTool(_)
        | ShellAction::SelectToolAlias(_)
        | ShellAction::PlaceGuide(_)
        | ShellAction::RemoveGuide
        | ShellAction::SetProfile(_)
        | ShellAction::SetOrthographicView(_)
        | ShellAction::SetWalkthroughView
        | ShellAction::ToggleToolbars
        | ShellAction::SetToolbarVisibility(_, _)
        | ShellAction::TogglePanels
        | ShellAction::ToggleGrid
        | ShellAction::ToggleGuides
        | ShellAction::ClearGuides
        | ShellAction::OpenPanel(_)
        | ShellAction::OpenPanelGroup(_)
        | ShellAction::UpdateUiPreferences
        | ShellAction::FrameDocument
        | ShellAction::ResetCamera
        | ShellAction::SetCameraPose
        | ShellAction::TogglePanelSection(_)
        | ShellAction::OpenProject
        | ShellAction::CancelProjectLoad
        | ShellAction::SaveProject
        | ShellAction::SaveProjectAs
        | ShellAction::ResetWorkspaceLayout
        | ShellAction::Quit
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

fn command_parameters(id: &str) -> Vec<CommandParameter> {
    let parameters: &[(&str, CommandValueType)] = match id {
        "geometry.add_cuboid" => &[
            ("dimensions", CommandValueType::Array),
            ("origin", CommandValueType::Array),
        ],
        "selection.set" => &[("paths", CommandValueType::Array)],
        "object.array_selection" => &[
            ("count", CommandValueType::Integer),
            ("spacing", CommandValueType::Number),
        ],
        "model.update_settings" => &[
            ("unit_name", CommandValueType::String),
            ("unit_size", CommandValueType::Number),
            ("grid_size", CommandValueType::Number),
            ("circle_segments", CommandValueType::Integer),
            ("snap_radius", CommandValueType::Number),
        ],
        "ui.update_preferences" => &[
            ("ui_text_scale", CommandValueType::Number),
            ("auto_collapse_toolbars", CommandValueType::Boolean),
            ("toolbar_icon_size", CommandValueType::Integer),
            ("show_permanent_grid", CommandValueType::Boolean),
            ("show_construction_guides", CommandValueType::Boolean),
            ("grid_normal_axis", CommandValueType::String),
            ("grid_extent", CommandValueType::Integer),
            ("normal_grid_width", CommandValueType::Number),
            ("thick_grid_width", CommandValueType::Number),
            ("toolbar_flow_top_down", CommandValueType::Boolean),
            ("toolbars_visible", CommandValueType::Boolean),
            ("toolbar_visibility", CommandValueType::Object),
            ("walk_move_speed", CommandValueType::Number),
            ("walk_jump_velocity", CommandValueType::Number),
            ("walk_gravity", CommandValueType::Number),
            ("walk_height_adjust_speed", CommandValueType::Number),
        ],
        "view.camera.set_pose" => &[
            ("position", CommandValueType::Array),
            ("target", CommandValueType::Array),
            ("field_of_view", CommandValueType::Number),
            ("near", CommandValueType::Number),
            ("far", CommandValueType::Number),
        ],
        "file.open" | "file.save" | "file.save_as" => &[("path", CommandValueType::String)],
        _ => &[],
    };
    parameters
        .iter()
        .map(|(name, value_type)| CommandParameter {
            name: (*name).to_owned(),
            value_type: *value_type,
            required: (id == "selection.set" && *name == "paths")
                || (id == "view.camera.set_pose" && matches!(*name, "position" | "target")),
        })
        .collect()
}

fn toolbar_visibility_key(stem: &str) -> &'static str {
    LEGACY_TOOLBARS
        .iter()
        .find_map(|(id, _)| {
            id.strip_prefix("builtin_toolbar_")
                .filter(|candidate| *candidate == stem)
        })
        .expect("toolbar stems come from the legacy toolbar catalog")
}

fn legacy_toolbar_command_ids(stem: &str) -> Vec<String> {
    let mut ids = LEGACY_TOOLBAR_TOOL_IDS
        .iter()
        .find(|(toolbar, _)| *toolbar == stem)
        .map(|(_, tool_ids)| {
            tool_ids
                .iter()
                .map(|tool_id| format!("tool.builtin.{tool_id}"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ids.extend(
        LEGACY_TOOLBAR_COMMAND_IDS
            .iter()
            .find(|(toolbar, _)| *toolbar == stem)
            .into_iter()
            .flat_map(|(_, command_ids)| command_ids.iter().map(|id| (*id).to_owned())),
    );
    if stem == "construction_points" {
        ids.insert(0, "tool.cuboid".to_owned());
    }
    if stem == "voxel" {
        ids.push("edit.voxelize_faces".to_owned());
    }
    ids
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
struct Preferences {
    profile: UiProfile,
    toolbars_visible: bool,
    toolbar_visibility: BTreeMap<String, bool>,
    auto_collapse_toolbars: bool,
    ui_text_scale: f32,
    toolbar_icon_size: u32,
    show_permanent_grid: bool,
    show_construction_guides: bool,
    #[serde(deserialize_with = "deserialize_grid_normal_axis")]
    grid_normal_axis: GridNormalAxis,
    grid_extent: u32,
    normal_grid_width: f32,
    thick_grid_width: f32,
    toolbar_flow_top_down: bool,
    walk_move_speed: f32,
    walk_jump_velocity: f32,
    walk_gravity: f32,
    walk_height_adjust_speed: f32,
    panels_visible: bool,
    panel_open: BTreeMap<String, bool>,
}

fn deserialize_grid_normal_axis<'de, D>(deserializer: D) -> Result<GridNormalAxis, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(match value.as_str() {
        "x" | "x_right" => GridNormalAxis::X,
        "z" | "z_back" => GridNormalAxis::Z,
        "y" | "y_up" => GridNormalAxis::Y,
        _ => GridNormalAxis::Y,
    })
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            profile: UiProfile::DesktopMenus,
            toolbars_visible: true,
            toolbar_visibility: LEGACY_TOOLBARS
                .iter()
                .filter_map(|(id, _)| {
                    id.strip_prefix("builtin_toolbar_")
                        .map(|stem| (stem.to_owned(), true))
                })
                .collect(),
            auto_collapse_toolbars: false,
            ui_text_scale: 1.0,
            toolbar_icon_size: 32,
            show_permanent_grid: true,
            show_construction_guides: true,
            grid_normal_axis: GridNormalAxis::Y,
            grid_extent: 20,
            normal_grid_width: 1.0,
            thick_grid_width: 3.0,
            toolbar_flow_top_down: false,
            walk_move_speed: 24.0,
            walk_jump_velocity: 16.0,
            walk_gravity: 48.0,
            walk_height_adjust_speed: 8.0,
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
    viewport_rect: egui::Rect,
    open_file_menu: bool,
    command_palette_open: bool,
    command_search: String,
    command_selection: usize,
    focus_command_search: bool,
    quit_requested: bool,
    panel_focus: Option<String>,
    document: Document,
    saved_revision: u64,
    history: TransactionHistory<SystemHistoryClock>,
    legacy_history: Option<OpaqueLegacyHistory>,
    legacy_active_index: Option<usize>,
    #[cfg(not(target_arch = "wasm32"))]
    current_project_path: Option<PathBuf>,
    next_job_id: u64,
    job_results: BTreeMap<String, CommandResult>,
    pending_replacement: Option<PendingReplacement>,
    #[cfg(not(target_arch = "wasm32"))]
    replacement_after_save: Option<PendingReplacement>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_project_load: Option<PendingProjectLoad>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_project_save: Option<PendingProjectSave>,
    selection: SelectionSession,
    object_name_edit: Option<(DefinitionId, String)>,
    viewport: Viewport,
    guides: Vec<ConstructionGuide>,
    cuboid_first: Option<Vec3>,
    cuboid_base: Option<(Vec3, Vec3)>,
    line_first: Option<Vec3>,
    rectangle_first: Option<Vec3>,
    array_count: usize,
    array_spacing: f64,
}

impl MeshcraftApp {
    pub fn command_catalog() -> Vec<CommandDescriptor> {
        shell_commands()
            .entries()
            .map(|command| command.descriptor.clone())
            .collect()
    }

    pub fn command_job_result(&self, job_id: &str) -> Option<&CommandResult> {
        self.job_results.get(job_id)
    }

    pub fn current_project_path(&self) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.current_project_path
                .as_ref()
                .map(|path| path.display().to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    fn document_is_dirty(&self) -> bool {
        self.document.revision() != self.saved_revision
    }

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
        let saved_revision = document.revision();
        let mut viewport = Viewport::default();
        let restored_camera = meshcraft_core::legacy::legacy_camera_state(&document)
            .is_some_and(|state| viewport.restore_legacy_camera_state(state));
        if frame_document && !restored_camera {
            viewport.request_frame();
        }

        Self {
            preferences,
            icon_atlas: assets::load_icon_atlas(&creation_context.egui_ctx),
            icon_regions: assets::icon_regions(),
            active_tool: "Select".to_owned(),
            status,
            pointer_in_viewport: None,
            viewport_rect: egui::Rect::NOTHING,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            quit_requested: false,
            panel_focus: None,
            document,
            saved_revision,
            history: TransactionHistory::default(),
            legacy_history,
            legacy_active_index,
            #[cfg(not(target_arch = "wasm32"))]
            current_project_path: None,
            next_job_id: 1,
            job_results: BTreeMap::new(),
            pending_replacement: None,
            #[cfg(not(target_arch = "wasm32"))]
            replacement_after_save: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_project_load: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_project_save: None,
            selection: SelectionSession::default(),
            object_name_edit: None,
            viewport,
            guides: Vec::new(),
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

    pub fn dispatch_command(&mut self, id: &str, payload: &CommandPayload) -> CommandDispatch {
        let state = self.query_command_state(id);
        let before_revision = self.document.revision();
        let outcome = shell_commands().dispatch(id, payload, &state, |action, payload| {
            let mut result = self.execute_command(*action, payload);
            if result.document_revision.is_none() && before_revision != self.document.revision() {
                result.document_revision = Some(self.document.revision());
            }
            result
        });
        if !outcome.accepted || outcome.result.completion == CommandCompletion::Failed {
            if let Some(message) = outcome.result.diagnostics.first() {
                self.status = message.clone();
            } else if !outcome.accepted {
                self.status = format!("Command `{id}` was rejected.");
            }
        }
        outcome
    }

    fn open_project(&mut self, payload: &CommandPayload) -> CommandResult {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = payload;
            CommandResult::failed("Opening project files is available in the desktop app.")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.open_project_native(payload)
        }
    }

    fn save_project(&mut self, as_new_path: bool, payload: &CommandPayload) -> CommandResult {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (as_new_path, payload);
            CommandResult::failed("Saving project files is available in the desktop app.")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.save_project_native(as_new_path, payload)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_project_native(
        &mut self,
        as_new_path: bool,
        payload: &CommandPayload,
    ) -> CommandResult {
        if self.pending_project_save.is_some() || self.pending_project_load.is_some() {
            return CommandResult::failed("Another project operation is already running.");
        }
        let explicit_path = payload.values.get("path").and_then(|value| value.as_str());
        let path = if let Some(path) = explicit_path {
            if path.trim().is_empty() {
                return CommandResult::failed("path must be a non-empty string");
            }
            PathBuf::from(path)
        } else if !as_new_path {
            if let Some(path) = &self.current_project_path {
                path.clone()
            } else {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Octodraw projects", &["octd", "k3d"])
                    .set_file_name("Untitled.octd")
                    .save_file()
                else {
                    self.status = "Save cancelled.".to_owned();
                    let mut result = CommandResult::completed();
                    result.outputs.insert("cancelled".to_owned(), true.into());
                    return result;
                };
                path
            }
        } else {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Octodraw projects", &["octd", "k3d"])
                .set_file_name("Untitled.octd")
                .save_file()
            else {
                self.status = "Save cancelled.".to_owned();
                let mut result = CommandResult::completed();
                result.outputs.insert("cancelled".to_owned(), true.into());
                return result;
            };
            path
        };
        let path = normalize_project_path(path);
        let job_id = format!("document-save-{}", self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);
        let input_revision = self.document.revision();
        let document = match meshcraft_core::legacy::snapshot_with_legacy_camera_state(
            &self.document,
            self.viewport.legacy_camera_state(),
        ) {
            Ok(document) => document,
            Err(error) => return CommandResult::failed(error),
        };
        let document_is_dirty = self.document_is_dirty();
        let dropped_legacy_history = document_is_dirty && self.legacy_history.is_some();
        let imported_history = if document_is_dirty {
            None
        } else {
            self.legacy_history.clone()
        };
        let native_history = (document_is_dirty && !self.history.entries().is_empty()).then(|| {
            (
                self.history.snapshot_timeline(),
                self.history.cursor(),
                self.viewport.legacy_camera_state(),
            )
        });
        let worker_path = path.clone();
        let (sender, receiver) = mpsc::channel();
        let worker_name = job_id.clone();
        let spawn_result = thread::Builder::new().name(worker_name).spawn(move || {
            let result = (|| {
                let history = match native_history {
                    Some((timeline, cursor, camera_state)) => Some(
                        encode_native_history(&timeline, cursor, &camera_state)
                            .map_err(|error| error.to_string())?,
                    ),
                    None => imported_history,
                };
                write_project_file(&worker_path, &document, history.as_ref())
            })();
            let _ = sender.send(result);
        });
        if let Err(error) = spawn_result {
            return CommandResult::failed(format!("could not start project writer: {error}"));
        }
        self.status = format!("Saving {}…", path.display());
        self.pending_project_save = Some(PendingProjectSave {
            job_id: job_id.clone(),
            path,
            input_revision,
            dropped_legacy_history,
            receiver,
        });
        let result = CommandResult::pending(
            job_id.clone(),
            HostEffect::StartJob {
                job_id: job_id.clone(),
                kind: "document.save".to_owned(),
                input_revision: Some(input_revision),
            },
        );
        self.job_results.insert(job_id, result.clone());
        result
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_project_save(&mut self) {
        let Some(pending) = self.pending_project_save.take() else {
            return;
        };
        match pending.receiver.try_recv() {
            Ok(Ok(())) => {
                self.saved_revision = pending.input_revision;
                self.current_project_path = Some(pending.path.clone());
                let replacement = self.replacement_after_save.take();
                self.status = if self.document_is_dirty() {
                    format!(
                        "Saved {} at revision {}; newer edits remain unsaved.",
                        pending.path.display(),
                        pending.input_revision
                    )
                } else {
                    format!("Saved {}.", pending.path.display())
                };
                if pending.dropped_legacy_history {
                    self.status.push_str(
                        " Imported undo snapshots were discarded because they predate these edits; current session history was saved when available.",
                    );
                }
                self.job_results
                    .insert(pending.job_id, CommandResult::completed());
                if let Some(replacement) = replacement {
                    if self.document_is_dirty() {
                        self.status.push_str(
                            " Save the newer edits, discard them, or cancel the replacement.",
                        );
                        self.pending_replacement = Some(replacement);
                    } else {
                        self.continue_document_replacement(replacement);
                    }
                }
            }
            Ok(Err(error)) => {
                self.status = format!("Could not save {}: {error}", pending.path.display());
                self.job_results
                    .insert(pending.job_id, CommandResult::failed(error));
                if let Some(replacement) = self.replacement_after_save.take() {
                    self.pending_replacement = Some(replacement);
                    self.status
                        .push_str(" Choose Save to retry, Discard, or Cancel.");
                }
            }
            Err(TryRecvError::Empty) => {
                self.status = format!("Saving {}…", pending.path.display());
                self.pending_project_save = Some(pending);
            }
            Err(TryRecvError::Disconnected) => {
                let error = "project writer stopped before returning a result".to_owned();
                self.status = format!("Could not save {}: {error}", pending.path.display());
                self.job_results
                    .insert(pending.job_id, CommandResult::failed(error));
                if let Some(replacement) = self.replacement_after_save.take() {
                    self.pending_replacement = Some(replacement);
                    self.status
                        .push_str(" Choose Save to retry, Discard, or Cancel.");
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open_project_native(&mut self, payload: &CommandPayload) -> CommandResult {
        if self.pending_project_load.is_some() {
            return CommandResult::failed("A project is already being opened.");
        }
        if self.pending_project_save.is_some() {
            return CommandResult::failed("Wait for the current project save to finish first.");
        }
        if self.pending_replacement.is_some() {
            return CommandResult::failed("A document replacement is already awaiting a decision.");
        }

        let path = match payload.values.get("path") {
            Some(value) => {
                let Some(path) = value.as_str().filter(|path| !path.trim().is_empty()) else {
                    return CommandResult::failed("path must be a non-empty string");
                };
                PathBuf::from(path)
            }
            None => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Octodraw projects", &["octd", "k3d"])
                    .pick_file()
                else {
                    self.status = "Open cancelled.".to_owned();
                    let mut result = CommandResult::completed();
                    result.outputs.insert("cancelled".to_owned(), true.into());
                    return result;
                };
                path
            }
        };

        if self.document_is_dirty() {
            return self.request_document_replacement(ReplacementTarget::OpenProject(path));
        }
        self.start_project_load(path, None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_project_load(&mut self, path: PathBuf, job_id: Option<String>) -> CommandResult {
        if self.pending_project_load.is_some() {
            return CommandResult::failed("A project is already being opened.");
        }
        if self.pending_project_save.is_some() {
            return CommandResult::failed("Wait for the current project save to finish first.");
        }
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return CommandResult::failed("the selected path is not a file"),
            Err(error) => {
                return CommandResult::failed(format!(
                    "could not inspect {}: {error}",
                    path.display()
                ));
            }
        };
        if metadata.len() > MAX_PROJECT_BYTES {
            return CommandResult::failed(format!(
                "project file exceeds the {} MiB limit",
                MAX_PROJECT_BYTES / (1024 * 1024)
            ));
        }

        let job_id = job_id.unwrap_or_else(|| {
            let job_id = format!("document-load-{}", self.next_job_id);
            self.next_job_id = self.next_job_id.saturating_add(1);
            job_id
        });
        let input_revision = self.document.revision();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let worker_path = path.clone();
        let (sender, receiver) = mpsc::channel();
        let worker_sender = sender;
        let total_bytes = metadata.len();
        let spawn_result = thread::Builder::new().name(job_id.clone()).spawn(move || {
            let loaded = (|| {
                if worker_cancellation.load(Ordering::Acquire) {
                    return Err("project opening was cancelled".to_owned());
                }
                let mut file = fs::File::open(&worker_path).map_err(|error| {
                    format!("could not read {}: {error}", worker_path.display())
                })?;
                let mut bytes = Vec::with_capacity(total_bytes.min(16 * 1024 * 1024) as usize);
                let mut buffer = [0; 64 * 1024];
                loop {
                    if worker_cancellation.load(Ordering::Acquire) {
                        return Err("project opening was cancelled".to_owned());
                    }
                    let read = file.read(&mut buffer).map_err(|error| {
                        format!("could not read {}: {error}", worker_path.display())
                    })?;
                    if read == 0 {
                        break;
                    }
                    if bytes.len().saturating_add(read) > MAX_PROJECT_BYTES as usize {
                        return Err(format!(
                            "project file exceeds the {} MiB limit",
                            MAX_PROJECT_BYTES / (1024 * 1024)
                        ));
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    let _ = worker_sender.send(ProjectLoadEvent::ReadProgress {
                        read_bytes: bytes.len(),
                        total_bytes,
                    });
                }
                let project = read_project_with(
                    &bytes,
                    MAX_PROJECT_BYTES as usize,
                    |progress| {
                        let _ = worker_sender.send(ProjectLoadEvent::Progress(progress));
                    },
                    || worker_cancellation.load(Ordering::Acquire),
                )
                .map_err(|error| error.to_string())?;
                Ok(project)
            })();
            let _ = worker_sender.send(ProjectLoadEvent::Finished(Box::new(loaded)));
        });
        if let Err(error) = spawn_result {
            return CommandResult::failed(format!("could not start project loader: {error}"));
        }

        let path_label = path.display().to_string();
        self.status = format!("Opening {path_label}…");
        self.pending_project_load = Some(PendingProjectLoad {
            job_id: job_id.clone(),
            path,
            input_revision,
            cancellation,
            receiver,
        });
        let result = CommandResult::pending(
            job_id.clone(),
            HostEffect::StartJob {
                job_id: job_id.clone(),
                kind: "document.load".to_owned(),
                input_revision: Some(input_revision),
            },
        );
        self.job_results.insert(job_id, result.clone());
        result
    }

    fn cancel_project_load(&mut self) -> CommandResult {
        #[cfg(target_arch = "wasm32")]
        {
            CommandResult::failed("Project loading is available in the desktop app.")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.cancel_project_load_native()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn cancel_project_load_native(&mut self) -> CommandResult {
        let Some(pending) = &self.pending_project_load else {
            return CommandResult::failed("no project is currently being opened");
        };
        pending.cancellation.store(true, Ordering::Release);
        self.status = format!("Cancelling {}…", pending.path.display());
        let mut result = CommandResult::completed();
        result.host_effects.push(HostEffect::CancelJob {
            job_id: pending.job_id.clone(),
        });
        result
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_project_load(&mut self) {
        let Some(pending) = self.pending_project_load.take() else {
            return;
        };
        let mut latest_progress = None;
        let mut finished = None;
        let mut disconnected = false;
        loop {
            match pending.receiver.try_recv() {
                Ok(ProjectLoadEvent::ReadProgress {
                    read_bytes,
                    total_bytes,
                }) => {
                    self.status = format!(
                        "Reading {} · {read_bytes} of {total_bytes} bytes",
                        pending.path.display()
                    );
                }
                Ok(ProjectLoadEvent::Progress(progress)) => latest_progress = Some(progress),
                Ok(ProjectLoadEvent::Finished(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if let Some(result) = finished {
            let final_result = if pending.cancellation.load(Ordering::Acquire) {
                self.status =
                    "Project opening cancelled; the current document was kept.".to_owned();
                CommandResult::failed("project opening was cancelled")
            } else if self.document.revision() != pending.input_revision {
                self.status =
                    "Project was not opened because the document changed while it loaded."
                        .to_owned();
                CommandResult::failed(
                    "document revision changed during load; the candidate was not installed",
                )
            } else {
                match *result {
                    Ok(project) => self.install_loaded_project(&pending.path, project),
                    Err(error) => {
                        self.status = format!("Could not open {}: {error}", pending.path.display());
                        CommandResult::failed(error)
                    }
                }
            };
            self.job_results.insert(pending.job_id, final_result);
            return;
        }

        if disconnected {
            self.status = format!(
                "Project loader stopped while opening {}.",
                pending.path.display()
            );
            self.job_results.insert(
                pending.job_id,
                CommandResult::failed("project loader stopped before returning a result"),
            );
            return;
        }

        if let Some(progress) = latest_progress {
            let compression = if progress.compressed { " (gzip)" } else { "" };
            self.status = format!(
                "Opening {} · decoded {} of {} bytes{compression}",
                pending.path.display(),
                progress.decoded_bytes,
                progress.encoded_bytes,
            );
        }
        self.pending_project_load = Some(pending);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn install_loaded_project(
        &mut self,
        path: &std::path::Path,
        project: ImportedProject,
    ) -> CommandResult {
        self.document = project.document;
        self.saved_revision = self.document.revision();
        self.history.clear();
        self.legacy_active_index = project
            .history
            .as_ref()
            .and_then(OpaqueLegacyHistory::active_entry_index);
        let history_entries = project
            .history
            .as_ref()
            .map_or(0, |history| history.entries.len());
        self.legacy_history = project.history;
        self.current_project_path = Some(path.to_path_buf());
        self.selection.clear();
        self.object_name_edit = None;
        self.active_tool = "Select".to_owned();
        self.guides.clear();
        self.cuboid_first = None;
        self.cuboid_base = None;
        self.line_first = None;
        self.rectangle_first = None;
        self.viewport.reset_camera();
        let restored_camera = meshcraft_core::legacy::legacy_camera_state(&self.document)
            .is_some_and(|state| self.viewport.restore_legacy_camera_state(state));
        if !restored_camera {
            self.viewport.request_frame();
        }

        self.status = format!("Opened {}.", path.display());
        if history_entries > 0 {
            self.status.push_str(&format!(
                " {history_entries} legacy history entries retained."
            ));
        }
        if !project.warnings.is_empty() {
            self.status.push_str(&format!(
                " Import repairs: {}.",
                project.warnings.join("; ")
            ));
        }

        let mut result = CommandResult::completed();
        result.document_revision = Some(self.document.revision());
        result
            .outputs
            .insert("path".to_owned(), path.display().to_string().into());
        result.outputs.insert(
            "source_schema_version".to_owned(),
            project.source_schema_version.into(),
        );
        result.outputs.insert(
            "warnings".to_owned(),
            serde_json::to_value(&project.warnings).unwrap_or_default(),
        );
        result
    }

    fn run_command(&mut self, id: &str) {
        self.dispatch_command(id, &CommandPayload::default());
    }

    fn request_document_replacement(&mut self, target: ReplacementTarget) -> CommandResult {
        if self.pending_replacement.is_some() {
            return CommandResult::failed("A document replacement is already awaiting a decision.");
        }
        self.command_palette_open = false;
        self.focus_command_search = false;
        let job_id = format!("document-replace-{}", self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);
        let result = CommandResult::pending(
            job_id.clone(),
            HostEffect::StartJob {
                job_id: job_id.clone(),
                kind: "document.replace_confirmation".to_owned(),
                input_revision: Some(self.document.revision()),
            },
        );
        self.pending_replacement = Some(PendingReplacement {
            job_id: job_id.clone(),
            target,
        });
        self.job_results.insert(job_id, result.clone());
        self.status = "Save changes before replacing this document?".to_owned();
        result
    }

    fn resolve_document_replacement(&mut self, decision: ReplacementDecision) {
        let Some(pending) = self.pending_replacement.take() else {
            return;
        };
        match decision {
            ReplacementDecision::Cancel => {
                self.status = "Document replacement cancelled.".to_owned();
                let mut result = CommandResult::completed();
                result.document_revision = Some(self.document.revision());
                result.outputs.insert("cancelled".to_owned(), true.into());
                self.job_results.insert(pending.job_id, result);
            }
            ReplacementDecision::Discard => match pending.target.clone() {
                ReplacementTarget::NewDocument => {
                    let result = self.start_new_document();
                    self.job_results.insert(pending.job_id, result);
                }
                #[cfg(not(target_arch = "wasm32"))]
                ReplacementTarget::OpenProject(path) => {
                    let result = self.start_project_load(path, Some(pending.job_id.clone()));
                    if result.completion == CommandCompletion::Failed {
                        self.job_results.insert(pending.job_id, result);
                    }
                }
            },
            #[cfg(not(target_arch = "wasm32"))]
            ReplacementDecision::Save => {
                let save_as = self.current_project_path.is_none();
                self.replacement_after_save = Some(pending.clone());
                let result = self.save_project_native(save_as, &CommandPayload::default());
                if !matches!(result.completion, CommandCompletion::Pending { .. }) {
                    self.replacement_after_save = None;
                    self.pending_replacement = Some(pending);
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn continue_document_replacement(&mut self, pending: PendingReplacement) {
        match pending.target {
            ReplacementTarget::NewDocument => {
                let result = self.start_new_document();
                self.job_results.insert(pending.job_id, result);
            }
            #[cfg(not(target_arch = "wasm32"))]
            ReplacementTarget::OpenProject(path) => {
                let result = self.start_project_load(path, Some(pending.job_id.clone()));
                if result.completion == CommandCompletion::Failed {
                    self.job_results.insert(pending.job_id, result);
                }
            }
        }
    }

    fn start_new_document(&mut self) -> CommandResult {
        if let Err(error) = self.commit_document_edit("New document", |document| {
            *document = Document::new();
            Ok(())
        }) {
            return CommandResult::failed(error);
        }
        self.selection.clear();
        self.object_name_edit = None;
        self.cuboid_first = None;
        self.cuboid_base = None;
        self.line_first = None;
        self.rectangle_first = None;
        self.guides.clear();
        self.status = "New empty document.".to_owned();
        let mut result = CommandResult::completed();
        result.document_revision = Some(self.document.revision());
        result
    }

    fn show_document_replacement_dialog(&mut self, context: &egui::Context) {
        let Some(pending) = self.pending_replacement.as_ref() else {
            return;
        };
        let target_label = match &pending.target {
            ReplacementTarget::NewDocument => "create a new document",
            #[cfg(not(target_arch = "wasm32"))]
            ReplacementTarget::OpenProject(_) => "open another project",
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("document-replacement-confirmation")).show(
            context,
            |ui| {
                ui.set_min_width(360.0);
                ui.heading("Unsaved changes");
                ui.label(format!("Save changes before you {target_label}?"));
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Save").clicked() {
                        decision = Some(ReplacementDecision::Save);
                    }
                    #[cfg(target_arch = "wasm32")]
                    ui.add_enabled(false, egui::Button::new("Save"));
                    if ui.button("Discard").clicked() {
                        decision = Some(ReplacementDecision::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(ReplacementDecision::Cancel);
                    }
                });
            },
        );
        if let Some(decision) = decision {
            self.resolve_document_replacement(decision);
        }
    }

    fn execute_command(&mut self, action: ShellAction, payload: &CommandPayload) -> CommandResult {
        match action {
            ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
                self.active_tool = tool.to_owned();
                if tool == "Orbit" {
                    self.viewport.set_perspective_view();
                }
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
                    "Line" => "Line: click a start and end point in the active construction plane."
                        .to_owned(),
                    "Rectangle" => {
                        "Rectangle: click two opposite corners in the active construction plane."
                            .to_owned()
                    }
                    _ => format!("{tool} tool selected."),
                };
            }
            ShellAction::PlaceGuide(kind) => {
                self.active_tool = match kind {
                    ConstructionGuideKind::Axis => "Axis Guide",
                    ConstructionGuideKind::Grid => "Grid Guide",
                }
                .to_owned();
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.line_first = None;
                self.rectangle_first = None;
                self.status = match kind {
                    ConstructionGuideKind::Axis => {
                        "Axis Guide: click a point to place a temporary construction guide."
                    }
                    ConstructionGuideKind::Grid => {
                        "Grid Guide: click a point to place a temporary construction grid."
                    }
                }
                .to_owned();
            }
            ShellAction::RemoveGuide => {
                self.active_tool = "Remove Guide".to_owned();
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.line_first = None;
                self.rectangle_first = None;
                self.status = "Remove Guide: click a visible construction guide.".to_owned();
            }
            ShellAction::AddCuboid => {
                let dimensions = match payload_vec3(payload, "dimensions", Vec3::new(2.0, 2.0, 2.0))
                {
                    Ok(value) => value,
                    Err(error) => return CommandResult::failed(error),
                };
                let origin = match payload_vec3(payload, "origin", Vec3::new(-1.0, 0.0, -1.0)) {
                    Ok(value) => value,
                    Err(error) => return CommandResult::failed(error),
                };
                match self.commit_document_edit("Add cuboid", |document| {
                    document
                        .add_cuboid("cuboid", dimensions, origin)
                        .ok_or_else(|| "could not create a cuboid".to_owned())
                }) {
                    Ok(occurrence) => {
                        self.set_selection([occurrence.clone()]);
                        self.status = format!(
                            "Created a {:.2} × {:.2} × {:.2} cuboid.",
                            dimensions.x, dimensions.y, dimensions.z
                        );
                        return command_result_with_occurrences(
                            CommandResult::completed(),
                            std::slice::from_ref(&occurrence),
                        );
                    }
                    Err(error) => return CommandResult::failed(error),
                }
            }
            ShellAction::CopySelection => return self.copy_selection(),
            ShellAction::DeleteSelection => {
                let selected = self.selected_occurrences();
                let removed = match self
                    .commit_document_edit("Delete selected objects", |document| {
                        document.remove_selected_occurrences(&selected)
                    }) {
                    Ok(removed) => removed,
                    Err(error) => return CommandResult::failed(error),
                };
                self.selection.clear();
                self.object_name_edit = None;
                self.status = if removed == 1 {
                    "Deleted 1 selected object.".to_owned()
                } else {
                    format!("Deleted {removed} selected objects.")
                };
                let mut result = CommandResult::completed();
                result
                    .outputs
                    .insert("deleted_count".to_owned(), removed.into());
                return result;
            }
            ShellAction::ClearSelection => {
                self.selection.clear();
                self.object_name_edit = None;
                self.status = "Selection cleared.".to_owned();
                return CommandResult::completed();
            }
            ShellAction::ResetSceneForCapture => {
                match self.commit_document_edit("Reset scene for capture", |document| {
                    *document = Document::new();
                    Ok(())
                }) {
                    Ok(()) => {}
                    Err(error) => return CommandResult::failed(error),
                }
                self.selection.clear();
                self.object_name_edit = None;
                self.active_tool = "Select".to_owned();
                self.cuboid_first = None;
                self.cuboid_base = None;
                self.line_first = None;
                self.rectangle_first = None;
                self.guides.clear();
                self.viewport.reset_camera();
                let camera_state = serde_json::json!({
                    "position": {"x": 18.0, "y": 14.0, "z": 18.0},
                    "target": {"x": 0.0, "y": 0.0, "z": 0.0}
                });
                self.viewport.restore_legacy_camera_state(&camera_state);
                self.preferences.panels_visible = false;
                self.status = "Scene reset for capture (perspective, Y-up).".to_owned();
                return CommandResult::completed();
            }
            ShellAction::SnapshotDocument => {
                let document = match serde_json::to_value(&self.document) {
                    Ok(value) => value,
                    Err(error) => {
                        return CommandResult::failed(format!(
                            "could not serialize document snapshot: {error}"
                        ));
                    }
                };
                let selection = match serde_json::to_value(self.selected_occurrences()) {
                    Ok(value) => value,
                    Err(error) => {
                        return CommandResult::failed(format!(
                            "could not serialize selection snapshot: {error}"
                        ));
                    }
                };
                let mut result = CommandResult::completed();
                result.document_revision = Some(self.document.revision());
                result.outputs.insert("document".to_owned(), document);
                result.outputs.insert("selection".to_owned(), selection);
                result
                    .outputs
                    .insert("camera".to_owned(), self.viewport.legacy_camera_state());
                return result;
            }
            ShellAction::SetSelection => {
                let Some(paths) = payload
                    .values
                    .get("paths")
                    .and_then(|value| value.as_array())
                else {
                    return CommandResult::failed("paths must be an array of occurrence paths");
                };
                let mut seen = BTreeSet::new();
                let mut selection = Vec::with_capacity(paths.len());
                for path_value in paths {
                    let Some(path_values) = path_value.as_array() else {
                        return CommandResult::failed(
                            "each occurrence path must be an array of layout-entry IDs",
                        );
                    };
                    if path_values.is_empty() {
                        return CommandResult::failed("occurrence paths cannot be empty");
                    }
                    let mut path = Vec::with_capacity(path_values.len());
                    for entry_value in path_values {
                        let Some(entry_id) = entry_value.as_u64() else {
                            return CommandResult::failed(
                                "layout-entry IDs must be unsigned integers",
                            );
                        };
                        path.push(LayoutEntryId(entry_id));
                    }
                    let occurrence = OccurrenceRef { path };
                    if !seen.insert(occurrence.clone()) {
                        return CommandResult::failed("occurrence paths must be distinct");
                    }
                    if self.document.resolve_occurrence(&occurrence).is_none() {
                        return CommandResult::failed(
                            "an occurrence path does not resolve in the current document",
                        );
                    }
                    selection.push(occurrence);
                }
                selection.sort();
                let selection_value = match serde_json::to_value(&selection) {
                    Ok(value) => value,
                    Err(error) => {
                        return CommandResult::failed(format!(
                            "could not serialize selection result: {error}"
                        ));
                    }
                };
                let selected_count = selection.len();
                self.set_selection(selection);
                self.object_name_edit = None;
                self.status = "Selection updated.".to_owned();
                let mut result = CommandResult::completed();
                result.document_revision = Some(self.document.revision());
                result
                    .outputs
                    .insert("selected_count".to_owned(), selected_count.into());
                result
                    .outputs
                    .insert("selection".to_owned(), selection_value);
                return result;
            }
            ShellAction::CreateObject => return self.create_object_from_selection(),
            ShellAction::ArraySelection => {
                let count = match payload.values.get("count") {
                    Some(value) => {
                        match value.as_u64().and_then(|value| usize::try_from(value).ok()) {
                            Some(value) if value > 0 => value,
                            _ => return CommandResult::failed("count must be a positive integer"),
                        }
                    }
                    None => self.array_count,
                };
                let spacing = match payload.values.get("spacing") {
                    Some(value) => match value.as_f64() {
                        Some(value) if value.is_finite() => value,
                        _ => return CommandResult::failed("spacing must be a finite number"),
                    },
                    None => self.array_spacing,
                };
                return self.array_selection(count, spacing);
            }
            ShellAction::UpdateModelSettings => {
                if payload.values.is_empty() {
                    self.open_panels(&["model_settings"]);
                    return CommandResult::completed();
                }
                let mut settings = self.document.model_settings().clone();
                if let Some(value) = payload.values.get("unit_name") {
                    let Some(value) = value.as_str() else {
                        return CommandResult::failed("unit_name must be a string");
                    };
                    settings.unit_name = value.to_owned();
                }
                for (name, target) in [
                    ("unit_size", &mut settings.unit_size),
                    ("grid_size", &mut settings.grid_size),
                    ("snap_radius", &mut settings.snap_radius),
                ] {
                    if let Some(value) = payload.values.get(name) {
                        let Some(value) = value.as_f64().filter(|value| value.is_finite()) else {
                            return CommandResult::failed(format!(
                                "{name} must be a finite number"
                            ));
                        };
                        *target = value;
                    }
                }
                if let Some(value) = payload.values.get("circle_segments") {
                    let Some(value) = value.as_u64().and_then(|value| u32::try_from(value).ok())
                    else {
                        return CommandResult::failed("circle_segments must be an integer");
                    };
                    settings.circle_segments = value;
                }
                match self.commit_document_edit("Change model settings", |document| {
                    document.update_model_settings(settings).map(|_| ())
                }) {
                    Ok(()) => self.status = "Model settings updated.".to_owned(),
                    Err(error) => return CommandResult::failed(error),
                }
            }
            ShellAction::UpdateUiPreferences => {
                if payload.values.is_empty() {
                    self.open_panels(&["ui_settings"]);
                    return CommandResult::completed();
                }
                return self.update_ui_preferences(payload);
            }
            ShellAction::Undo => self.undo_document(),
            ShellAction::Redo => self.redo_document(),
            ShellAction::NewDocument if self.document_is_dirty() => {
                return self.request_document_replacement(ReplacementTarget::NewDocument);
            }
            ShellAction::NewDocument => return self.start_new_document(),
            ShellAction::OpenProject => return self.open_project(payload),
            ShellAction::CancelProjectLoad => return self.cancel_project_load(),
            ShellAction::SaveProject => return self.save_project(false, payload),
            ShellAction::SaveProjectAs => return self.save_project(true, payload),
            ShellAction::ResetWorkspaceLayout => {
                self.preferences = Preferences::default();
                self.panel_focus = None;
                self.status = "Workspace layout reset to the default profile.".to_owned();
            }
            ShellAction::Quit => {
                self.quit_requested = true;
                self.status = "Closing Meshcraft.".to_owned();
            }
            ShellAction::SetProfile(profile) => {
                self.preferences.profile = profile;
                self.status = format!("{} workspace profile selected.", profile.label());
            }
            ShellAction::SetOrthographicView(view) => {
                self.viewport.set_orthographic_view(view);
                self.status = format!("{} orthographic view selected.", view.id());
            }
            ShellAction::SetWalkthroughView => {
                self.viewport.set_walkthrough_view();
                self.status =
                    "Walkthrough camera selected. Move with WASD and look with right-drag."
                        .to_owned();
            }
            ShellAction::FrameDocument => {
                self.viewport.request_frame();
                self.status = "Framed the document in the viewport.".to_owned();
            }
            ShellAction::ResetCamera => {
                self.viewport.reset_camera();
                self.status = "Camera reset to the default perspective view.".to_owned();
            }
            ShellAction::SetCameraPose => {
                let Some(position_value) = payload.values.get("position") else {
                    return CommandResult::failed("position is required");
                };
                let Some(target_value) = payload.values.get("target") else {
                    return CommandResult::failed("target is required");
                };
                let position = match payload_vec3_value(position_value, "position") {
                    Ok(position) => position,
                    Err(error) => return CommandResult::failed(error),
                };
                let target = match payload_vec3_value(target_value, "target") {
                    Ok(target) => target,
                    Err(error) => return CommandResult::failed(error),
                };
                let mut camera_state = serde_json::json!({
                    "position": {"x": position.x, "y": position.y, "z": position.z},
                    "target": {"x": target.x, "y": target.y, "z": target.z},
                });
                for (payload_name, state_name, default) in [
                    ("field_of_view", "fieldOfView", 67.0),
                    ("near", "near", 0.1),
                    ("far", "far", 500.0),
                ] {
                    let value = match payload.values.get(payload_name) {
                        Some(value) => match value.as_f64().filter(|value| value.is_finite()) {
                            Some(value) => value,
                            None => {
                                return CommandResult::failed(format!(
                                    "{payload_name} must be a finite number"
                                ));
                            }
                        },
                        None => default,
                    };
                    camera_state[state_name] = value.into();
                }
                let field_of_view = camera_state["fieldOfView"].as_f64().unwrap_or_default();
                let near = camera_state["near"].as_f64().unwrap_or_default();
                let far = camera_state["far"].as_f64().unwrap_or_default();
                if !(1.0..179.0).contains(&field_of_view) {
                    return CommandResult::failed(
                        "field_of_view must be greater than 1 and less than 179 degrees",
                    );
                }
                if near <= 0.0 || far <= near {
                    return CommandResult::failed("camera clipping planes require 0 < near < far");
                }
                if !self.viewport.restore_legacy_camera_state(&camera_state) {
                    return CommandResult::failed(
                        "camera position and target must define a finite, nonzero view direction",
                    );
                }
                self.status = "Camera pose updated.".to_owned();
                let mut result = CommandResult::completed();
                result
                    .outputs
                    .insert("camera".to_owned(), self.viewport.legacy_camera_state());
                return result;
            }
            ShellAction::TogglePanelSection(panel) => self.toggle_panel_section(panel),
            ShellAction::ToggleToolbars => {
                self.preferences.toolbars_visible = !self.preferences.toolbars_visible;
                self.status = if self.preferences.toolbars_visible {
                    "Hybrid toolbars shown.".to_owned()
                } else {
                    "Hybrid toolbars hidden.".to_owned()
                };
            }
            ShellAction::SetToolbarVisibility(stem, shown) => {
                self.preferences
                    .toolbar_visibility
                    .insert(stem.to_owned(), shown);
                let label = LEGACY_TOOLBARS
                    .iter()
                    .find(|(id, _)| id.strip_prefix("builtin_toolbar_") == Some(stem))
                    .map(|(_, label)| *label)
                    .unwrap_or(stem);
                self.status = format!(
                    "{label} toolbar {}.",
                    if shown { "shown" } else { "hidden" }
                );
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
            ShellAction::ToggleGuides => {
                self.preferences.show_construction_guides =
                    !self.preferences.show_construction_guides;
                self.status = if self.preferences.show_construction_guides {
                    "Construction guides shown.".to_owned()
                } else {
                    "Construction guides hidden.".to_owned()
                };
            }
            ShellAction::ClearGuides => {
                let count = self.guides.len();
                self.guides.clear();
                self.status = format!("Cleared {count} construction guide(s).");
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

    fn toggle_panel_section(&mut self, panel: &str) {
        self.preferences.panels_visible = true;
        let is_open = self
            .preferences
            .panel_open
            .get(panel)
            .copied()
            .unwrap_or(true);
        self.preferences
            .panel_open
            .insert(panel.to_owned(), !is_open);
        self.panel_focus = Some(panel.to_owned());
        let label = INSPECTOR_PANELS
            .iter()
            .find(|(id, _)| *id == panel)
            .map(|(_, label)| *label)
            .unwrap_or(panel);
        self.status = format!(
            "{label} panel {}.",
            if is_open { "closed" } else { "opened" }
        );
    }

    fn update_ui_preferences(&mut self, payload: &CommandPayload) -> CommandResult {
        let mut preferences = self.preferences.clone();
        let numeric_preferences = [
            ("ui_text_scale", &mut preferences.ui_text_scale, 0.5, 2.0),
            (
                "normal_grid_width",
                &mut preferences.normal_grid_width,
                0.25,
                4.0,
            ),
            (
                "thick_grid_width",
                &mut preferences.thick_grid_width,
                0.5,
                8.0,
            ),
            (
                "walk_move_speed",
                &mut preferences.walk_move_speed,
                0.1,
                1000.0,
            ),
            (
                "walk_jump_velocity",
                &mut preferences.walk_jump_velocity,
                0.1,
                1000.0,
            ),
            ("walk_gravity", &mut preferences.walk_gravity, 0.1, 10000.0),
            (
                "walk_height_adjust_speed",
                &mut preferences.walk_height_adjust_speed,
                0.1,
                1000.0,
            ),
        ];
        for (name, target, minimum, maximum) in numeric_preferences {
            if payload.values.contains_key(name) {
                let value = match payload_f64(payload, name, 0.0) {
                    Ok(value) if (minimum..=maximum).contains(&value) => value as f32,
                    Ok(_) => {
                        return CommandResult::failed(format!(
                            "{name} must be between {minimum} and {maximum}"
                        ));
                    }
                    Err(error) => return CommandResult::failed(error),
                };
                *target = value;
            }
        }
        for (name, target) in [
            (
                "auto_collapse_toolbars",
                &mut preferences.auto_collapse_toolbars,
            ),
            ("show_permanent_grid", &mut preferences.show_permanent_grid),
            (
                "show_construction_guides",
                &mut preferences.show_construction_guides,
            ),
            (
                "toolbar_flow_top_down",
                &mut preferences.toolbar_flow_top_down,
            ),
            ("toolbars_visible", &mut preferences.toolbars_visible),
        ] {
            if payload.values.contains_key(name) {
                match payload_bool(payload, name, false) {
                    Ok(value) => *target = value,
                    Err(error) => return CommandResult::failed(error),
                }
            }
        }
        if let Some(value) = payload.values.get("grid_normal_axis") {
            preferences.grid_normal_axis = match value.as_str() {
                Some("x") => GridNormalAxis::X,
                Some("y") => GridNormalAxis::Y,
                Some("z") => GridNormalAxis::Z,
                _ => {
                    return CommandResult::failed(
                        "grid_normal_axis must be one of `x`, `y`, or `z`",
                    );
                }
            };
        }
        if payload.values.contains_key("toolbar_icon_size") {
            let size = match payload_u32(payload, "toolbar_icon_size", 0) {
                Ok(size) if [24, 32, 40, 48].contains(&size) => size,
                Ok(_) => {
                    return CommandResult::failed("toolbar_icon_size must be 24, 32, 40, or 48");
                }
                Err(error) => return CommandResult::failed(error),
            };
            preferences.toolbar_icon_size = size;
        }
        if payload.values.contains_key("grid_extent") {
            preferences.grid_extent = match payload_u32(payload, "grid_extent", 1) {
                Ok(extent) if (1..=1000).contains(&extent) => extent,
                Ok(_) => return CommandResult::failed("grid_extent must be between 1 and 1000"),
                Err(error) => return CommandResult::failed(error),
            };
        }
        if let Some(visibility) = payload.values.get("toolbar_visibility") {
            let Some(visibility) = visibility.as_object() else {
                return CommandResult::failed("toolbar_visibility must be an object of booleans");
            };
            for (stem, value) in visibility {
                if !LEGACY_TOOLBARS
                    .iter()
                    .any(|(id, _)| id.strip_prefix("builtin_toolbar_") == Some(stem.as_str()))
                {
                    return CommandResult::failed(format!(
                        "toolbar_visibility contains unknown toolbar {stem:?}"
                    ));
                }
                let Some(shown) = value.as_bool() else {
                    return CommandResult::failed(format!(
                        "toolbar_visibility.{stem} must be a boolean"
                    ));
                };
                preferences.toolbar_visibility.insert(stem.clone(), shown);
            }
        }
        self.preferences = preferences;
        self.status = "UI preferences updated.".to_owned();
        CommandResult::completed()
    }

    pub fn query_command_state(&self, id: &str) -> CommandState {
        let Some(command) = shell_commands().get(id) else {
            return CommandState::disabled(format!("unknown command `{id}`"));
        };
        if command.descriptor.capability == CommandCapability::Deferred {
            return CommandState::disabled(format!(
                "{} is not implemented in this build",
                command.descriptor.label
            ));
        }
        if id == "app.quit" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("The browser tab cannot be closed by the app");
        }
        if let Some(view) = id.strip_prefix("view.ortho.").and_then(OrthoView::from_id) {
            return CommandState {
                checked: Some(self.viewport.orthographic_view() == Some(view)),
                ..CommandState::default()
            };
        }
        if id == "view.camera.walkthrough" {
            return CommandState {
                checked: Some(self.viewport.walkthrough_active()),
                ..CommandState::default()
            };
        }
        if id == "file.open" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Open Project is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            {
                if self.pending_replacement.is_some() {
                    return CommandState::disabled("a document replacement is awaiting a decision");
                }
                if self.pending_project_load.is_some() {
                    return CommandState::disabled("another project is still loading");
                }
                if self.pending_project_save.is_some() {
                    return CommandState::disabled("a project save is still running");
                }
            }
        }
        if id == "file.new" && self.pending_replacement.is_some() {
            return CommandState::disabled("a document replacement is awaiting a decision");
        }
        if id == "file.save" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Saving project files is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            if self.pending_replacement.is_some()
                || self.pending_project_save.is_some()
                || self.pending_project_load.is_some()
            {
                return CommandState::disabled("another project operation is still running");
            }
        }
        if id == "file.save_as" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Saving project files is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            if self.pending_replacement.is_some()
                || self.pending_project_save.is_some()
                || self.pending_project_load.is_some()
            {
                return CommandState::disabled("another project operation is still running");
            }
        }
        if id == "file.cancel_open" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Project loading is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            if self.pending_project_load.is_none() {
                return CommandState::disabled("no project is currently being opened");
            }
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
            CommandStateQuery::DeleteSelectionAvailable => {
                let selection = self.selected_occurrences();
                let mut paths = BTreeSet::new();
                let existing = !selection.is_empty()
                    && selection.iter().all(|item| {
                        !item.path.is_empty()
                            && self.document.resolve_occurrence(item).is_some()
                            && paths.insert(item.path.clone())
                    });
                let overlaps = paths.iter().any(|path| {
                    paths
                        .iter()
                        .any(|other| path.len() < other.len() && other.starts_with(path))
                });
                if existing && !overlaps {
                    CommandState::default()
                } else {
                    CommandState::disabled(
                        "select existing objects without selecting both a parent and its child",
                    )
                }
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
                    "guides" => self.preferences.show_construction_guides,
                    key if key.starts_with("toolbar:") => {
                        let mut parts = key.split(':');
                        let _ = parts.next();
                        let stem = parts.next().unwrap_or_default();
                        let desired = parts.next().unwrap_or_default();
                        let shown = self
                            .preferences
                            .toolbar_visibility
                            .get(stem)
                            .copied()
                            .unwrap_or(true);
                        if desired == "hidden" { !shown } else { shown }
                    }
                    key if key.starts_with("panel:") => self
                        .preferences
                        .panel_open
                        .get(key.trim_start_matches("panel:"))
                        .copied()
                        .unwrap_or(true),
                    _ => false,
                }),
                ..CommandState::default()
            },
            CommandStateQuery::UndoAvailable
            | CommandStateQuery::RedoAvailable
            | CommandStateQuery::SelectionAvailable => CommandState::default(),
        }
    }

    fn copy_selection(&mut self) -> CommandResult {
        let selection = self.selected_occurrences();
        if selection.len() != 1 || selection[0].path.len() != 1 {
            return CommandResult::failed("Copy requires one selected root object.");
        }
        let source = selection[0].clone();
        let Some(resolved) = self.document.resolve_occurrence(&source) else {
            return CommandResult::failed("The selected object no longer exists.");
        };
        let Some(geometry) = self.document.effective_geometry(&source) else {
            return CommandResult::failed("The selected object has no resolved geometry.");
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
                self.set_selection([copy.clone()]);
                self.status =
                    "Copied the selected object; the copy shares its definition.".to_owned();
                command_result_with_occurrences(
                    CommandResult::completed(),
                    std::slice::from_ref(&copy),
                )
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn create_object_from_selection(&mut self) -> CommandResult {
        let selection = self.selected_occurrences();
        match self.commit_document_edit("Create object", |document| {
            document
                .create_object_from_root_occurrences(&selection, "Object")
                .ok_or_else(|| {
                    "Create Object requires one or more selected root objects.".to_owned()
                })
        }) {
            Ok(object) => {
                self.set_selection([object.clone()]);
                self.status = "Created an object definition from the selected geometry.".to_owned();
                command_result_with_occurrences(
                    CommandResult::completed(),
                    std::slice::from_ref(&object),
                )
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn array_selection(&mut self, count: usize, spacing: f64) -> CommandResult {
        let selection = self.selected_occurrences();
        if selection.len() != 1 || selection[0].path.len() != 1 {
            return CommandResult::failed("Array requires one selected root object.");
        }
        let source = selection[0].clone();
        match self.commit_document_edit("Array object", |document| {
            document
                .array_root_occurrence(&source, count, Vec3::new(spacing, 0.0, 0.0))
                .ok_or_else(|| "Array could not create the requested object instances.".to_owned())
        }) {
            Ok(copies) => {
                self.set_selection(copies.clone());
                self.status = format!("Created {} array copies.", count);
                command_result_with_occurrences(CommandResult::completed(), &copies)
            }
            Err(error) => CommandResult::failed(error),
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

    fn handle_viewport_click(
        &mut self,
        rect: egui::Rect,
        position: egui::Pos2,
        snap: Option<SnapResult>,
    ) {
        if self.active_tool == "Remove Guide" {
            let guides = if self.preferences.show_construction_guides {
                self.guides.as_slice()
            } else {
                &[]
            };
            let guide_index = self.viewport.guide_index_at(
                rect,
                position,
                guides,
                self.document.model_settings().grid_size,
                10.0,
            );
            if let Some(index) = guide_index {
                self.guides.remove(index);
                self.status = "Construction guide removed.".to_owned();
            } else {
                self.status = "No visible construction guide near the pointer.".to_owned();
            }
            self.active_tool = "Select".to_owned();
            return;
        }
        if matches!(self.active_tool.as_str(), "Axis Guide" | "Grid Guide") {
            let point = snap.map(|snap| (snap.point, snap.normal)).or_else(|| {
                self.viewport
                    .screen_to_construction_plane(rect, position)
                    .map(|point| (point, Vec3::Y))
            });
            let Some((point, normal)) = point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
            let kind = if self.active_tool == "Axis Guide" {
                ConstructionGuideKind::Axis
            } else {
                ConstructionGuideKind::Grid
            };
            if let Some(guide) = ConstructionGuide::at_point(kind, point, normal) {
                self.guides.push(guide);
                self.active_tool = "Select".to_owned();
                self.status = match kind {
                    ConstructionGuideKind::Axis => "Axis guide placed.".to_owned(),
                    ConstructionGuideKind::Grid => "Grid guide placed.".to_owned(),
                };
            } else {
                self.status = "Could not construct a stable guide orientation.".to_owned();
            }
            return;
        }

        if self.active_tool == "Line" {
            let point = snap
                .map(|snap| snap.point)
                .or_else(|| self.viewport.screen_to_construction_plane(rect, position));
            let Some(point) = point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
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
                        self.status = format!(
                            "Created line {:.2} units long{}.",
                            start.distance(point),
                            snap.map_or(String::new(), |snap| format!(
                                " · {} snap",
                                snap.kind.label()
                            ))
                        );
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
            let point = snap
                .map(|snap| snap.point)
                .or_else(|| self.viewport.screen_to_construction_plane(rect, position));
            let Some(point) = point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
            if let Some(first) = self.rectangle_first {
                let corners = self.viewport.construction_rectangle_corners(first, point);
                match self.commit_document_edit("Create rectangle", |document| {
                    document
                        .add_planar_rectangle("rectangle", corners)
                        .ok_or_else(|| "Rectangle sides must be distinct finite points.".to_owned())
                }) {
                    Ok(occurrence) => {
                        self.set_selection([occurrence]);
                        self.active_tool = "Select".to_owned();
                        self.rectangle_first = None;
                        self.status = format!(
                            "Created rectangle {:.2} × {:.2}.",
                            corners[0].distance(corners[1]),
                            corners[1].distance(corners[2])
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
            let Some(ground) = self.viewport.screen_to_ground(rect, position) else {
                self.status =
                    "Cuboid construction needs the perspective or top/bottom view.".to_owned();
                return;
            };
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
            let Some(height) = self
                .viewport
                .height_from_screen_position(rect, center, position)
            else {
                self.status = "Cuboid height cannot be set from this orthographic view.".to_owned();
                return;
            };
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

    fn tool_preview(
        &self,
        rect: egui::Rect,
        pointer: Option<egui::Pos2>,
        snap: Option<SnapResult>,
    ) -> Option<ToolPreview> {
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
                    .height_from_screen_position(rect, center, position)?
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
                    end: snap
                        .map(|snap| snap.point)
                        .or_else(|| self.viewport.screen_to_construction_plane(rect, position))?,
                })
            }
            "Rectangle" => {
                let first = self.rectangle_first?;
                let opposite = snap
                    .map(|snap| snap.point)
                    .or_else(|| self.viewport.screen_to_construction_plane(rect, position))?;
                Some(ToolPreview::Rectangle {
                    corners: self
                        .viewport
                        .construction_rectangle_corners(first, opposite),
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
        let tool = match command.handler {
            ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => Some(tool),
            _ => None,
        };
        let icon_name = command.descriptor.presentation.icon_key.as_deref();
        let fallback_label = tool
            .and_then(|tool| tool.chars().next())
            .or_else(|| command.descriptor.label.chars().next())
            .unwrap_or('?');
        let state = self.query_command_state(command.descriptor.id.as_str());
        let selected = state
            .checked
            .or_else(|| tool.map(|tool| self.active_tool == tool))
            .unwrap_or(false);
        let icon_size = (size * 0.8).max(12.0);
        let button = match (
            self.icon_image(icon_name, Vec2::splat(icon_size)),
            icon_only,
        ) {
            (Some(icon), true) => egui::Button::image(icon),
            (Some(icon), false) => egui::Button::image_and_text(icon, &command.descriptor.label),
            (None, true) => egui::Button::new(fallback_label.to_string()),
            (None, false) => egui::Button::new(&command.descriptor.label),
        }
        .selected(selected);
        let button_size = if icon_only {
            Vec2::splat(size)
        } else {
            Vec2::new(ui.available_width(), size)
        };
        let response = ui
            .add_enabled_ui(state.enabled, |ui| ui.add_sized(button_size, button))
            .inner;
        if !state.enabled {
            response.clone().on_disabled_hover_text(
                state
                    .disabled_reason
                    .as_deref()
                    .unwrap_or("Unavailable in this build"),
            );
        }
        response.on_hover_text(&command.descriptor.label).clicked()
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
            .map(|command| self.query_command_state(command.descriptor.id.as_str()))
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
        let state = self.query_command_state(id);
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
        let state = self.query_command_state(id);
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
                self.show_command_menu_item(ui, "file.open");
                self.show_command_menu_item(ui, "file.cancel_open");
                self.show_command_menu_item(ui, "file.save");
                self.show_command_menu_item(ui, "file.save_as");
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Import Geometry…"));
                ui.add_enabled(false, egui::Button::new("Export Geometry…"));
                self.show_command_menu_item(ui, "app.quit");
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
                self.show_command_menu_item(ui, "edit.delete_selection");
                self.show_command_menu_item(ui, "edit.clear_selection");
                self.show_command_menu_item(ui, "edit.reset_scene_for_capture");
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
                self.show_command_menu_item(ui, "view.workspace.reset_layout");
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
            ui.set_width(312.0);
            ui.set_min_height(height);
            ui.heading("Toolbars");
            let size = self.preferences.toolbar_icon_size as f32;
            let mut selected_command = None;
            egui::ScrollArea::vertical()
                .id_salt("legacy-toolbar-list")
                .show(ui, |ui| {
                    for (toolbar_id, label) in LEGACY_TOOLBARS {
                        let Some(stem) = toolbar_id.strip_prefix("builtin_toolbar_") else {
                            continue;
                        };
                        let shown = self
                            .preferences
                            .toolbar_visibility
                            .get(stem)
                            .copied()
                            .unwrap_or(true);
                        let mut next_shown = shown;
                        if ui.checkbox(&mut next_shown, *label).changed() {
                            let verb = if next_shown { "show" } else { "hide" };
                            self.run_command(&format!("view.toolbar.{stem}.{verb}"));
                        }
                        if shown {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                if stem == "in_tool_operators" {
                                    ui.weak(format!("Active tool: {}", self.active_tool));
                                }
                                ui.horizontal_wrapped(|ui| {
                                    for id in legacy_toolbar_command_ids(stem) {
                                        if let Some(command) = shell_commands().get(&id) {
                                            let clicked = if stem == "rendering" {
                                                let state = self.query_command_state(
                                                    command.descriptor.id.as_str(),
                                                );
                                                let response = ui
                                                    .add_enabled_ui(state.enabled, |ui| {
                                                        ui.add_sized(
                                                            [size * 1.7, size],
                                                            egui::Button::new(
                                                                &command.descriptor.label,
                                                            ),
                                                        )
                                                    })
                                                    .inner;
                                                if !state.enabled {
                                                    response.clone().on_disabled_hover_text(
                                                        state
                                                            .disabled_reason
                                                            .as_deref()
                                                            .unwrap_or("Unavailable in this build"),
                                                    );
                                                }
                                                response
                                                    .on_hover_text(&command.descriptor.label)
                                                    .clicked()
                                            } else {
                                                self.show_tool_button(ui, command, size, true)
                                            };
                                            if clicked {
                                                selected_command = Some(id);
                                            }
                                        }
                                    }
                                });
                            });
                        }
                        ui.add_space(3.0);
                    }
                });
            if let Some(id) = selected_command {
                self.run_command(&id);
            }
        });
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui, size: Vec2) {
        self.viewport
            .set_grid_normal_axis(self.preferences.grid_normal_axis);
        self.viewport.set_walkthrough_tuning(WalkthroughTuning {
            move_speed: f64::from(self.preferences.walk_move_speed),
            jump_velocity: f64::from(self.preferences.walk_jump_velocity),
            gravity: f64::from(self.preferences.walk_gravity),
            height_adjust_speed: f64::from(self.preferences.walk_height_adjust_speed),
        });
        let predicted_rect = egui::Rect::from_min_size(ui.cursor().min, size);
        let pointer = ui
            .ctx()
            .pointer_interact_pos()
            .filter(|position| predicted_rect.contains(*position));
        let snap_radius_points = (self.document.model_settings().snap_radius
            / f64::from(ui.ctx().pixels_per_point())) as f32;
        let snap_scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        let snap = if matches!(
            self.active_tool.as_str(),
            "Line" | "Rectangle" | "Axis Guide" | "Grid Guide"
        ) {
            let visible_guides = if self.preferences.show_construction_guides {
                self.guides.as_slice()
            } else {
                &[]
            };
            pointer.and_then(|position| {
                self.viewport.snap_point_with_options(
                    predicted_rect,
                    &self.document,
                    position,
                    SnapOptions {
                        snap_radius: snap_radius_points,
                        grid_spacing: self.document.model_settings().grid_size,
                        guides: visible_guides,
                        scope: snap_scope,
                        visibility: SnapVisibility::default(),
                    },
                )
            })
        } else {
            None
        };
        let visible_guides = if self.preferences.show_construction_guides {
            self.guides.as_slice()
        } else {
            &[]
        };
        let preview = self.tool_preview(predicted_rect, pointer, snap);
        let guide_preview = if matches!(self.active_tool.as_str(), "Axis Guide" | "Grid Guide") {
            snap.and_then(|snap| {
                let kind = if self.active_tool == "Axis Guide" {
                    ConstructionGuideKind::Axis
                } else {
                    ConstructionGuideKind::Grid
                };
                ConstructionGuide::at_point(kind, snap.point, snap.normal)
            })
        } else {
            None
        };
        let selection = self.selected_occurrences();
        let input = self.viewport.show(
            ui,
            &ViewportScene {
                size,
                document: &self.document,
                selection: &selection,
                active_tool: &self.active_tool,
                preview,
                snap,
                guides: visible_guides,
                guide_preview,
                grid: GridStyle {
                    visible: self.preferences.show_permanent_grid,
                    spacing: self.document.model_settings().grid_size,
                    extent: self.preferences.grid_extent,
                    normal_axis: self.preferences.grid_normal_axis,
                    normal_width: self.preferences.normal_grid_width,
                    thick_width: self.preferences.thick_grid_width,
                },
            },
        );
        self.viewport_rect = input.rect;
        self.pointer_in_viewport = input.pointer_local;
        if let Some(position) = input.click {
            self.handle_viewport_click(input.rect, position, snap);
        }
    }

    fn show_panel_header(&mut self, ui: &mut egui::Ui, id: &str, title: &str) -> bool {
        let is_open = *self.preferences.panel_open.get(id).unwrap_or(&true);
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
            self.run_command(&format!("view.panel.{id}.toggle"));
            self.preferences
                .panel_open
                .get(id)
                .copied()
                .unwrap_or(is_open)
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
        let mut preferences = self.preferences.clone();
        let mut changed = false;
        let mut preferences_changed = false;
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
        ui.separator();
        ui.label("Walk move speed");
        preferences_changed |= ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::DragValue::new(&mut preferences.walk_move_speed)
                    .range(0.1..=1000.0)
                    .speed(0.5),
            )
            .changed();
        ui.label("Walk jump velocity");
        preferences_changed |= ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::DragValue::new(&mut preferences.walk_jump_velocity)
                    .range(0.1..=1000.0)
                    .speed(0.5),
            )
            .changed();
        ui.label("Walk gravity");
        preferences_changed |= ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::DragValue::new(&mut preferences.walk_gravity)
                    .range(0.1..=10000.0)
                    .speed(1.0),
            )
            .changed();
        ui.label("Walk height adjust speed");
        preferences_changed |= ui
            .add_sized(
                [ui.available_width(), 24.0],
                egui::DragValue::new(&mut preferences.walk_height_adjust_speed)
                    .range(0.1..=1000.0)
                    .speed(0.5),
            )
            .changed();
        if changed {
            let mut payload = CommandPayload::default();
            payload.values.insert(
                "unit_name".to_owned(),
                serde_json::json!(settings.unit_name),
            );
            payload.values.insert(
                "unit_size".to_owned(),
                serde_json::json!(settings.unit_size),
            );
            payload.values.insert(
                "grid_size".to_owned(),
                serde_json::json!(settings.grid_size),
            );
            payload.values.insert(
                "circle_segments".to_owned(),
                serde_json::json!(settings.circle_segments),
            );
            payload.values.insert(
                "snap_radius".to_owned(),
                serde_json::json!(settings.snap_radius),
            );
            let result = self.dispatch_command("model.update_settings", &payload);
            if result.result.completion == CommandCompletion::Failed {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    result
                        .result
                        .diagnostics
                        .first()
                        .map(String::as_str)
                        .unwrap_or("Model settings could not be updated."),
                );
            }
        }
        if preferences_changed {
            self.dispatch_ui_preferences(&preferences);
        }
    }

    fn show_ui_settings(&mut self, ui: &mut egui::Ui) {
        let mut preferences = self.preferences.clone();
        ui.label("UI text size");
        egui::ComboBox::from_id_salt("ui-text-scale")
            .selected_text(format!("{}x", preferences.ui_text_scale))
            .show_ui(ui, |ui| {
                for scale in [0.75_f32, 1.0, 1.25, 1.5, 2.0] {
                    ui.selectable_value(&mut preferences.ui_text_scale, scale, format!("{scale}x"));
                }
            });
        ui.checkbox(
            &mut preferences.auto_collapse_toolbars,
            "Auto-collapse after tool selection",
        );
        ui.label("Toolbar icon/button size");
        egui::ComboBox::from_id_salt("toolbar-icon-size")
            .selected_text(format!(
                "{} × {} px",
                preferences.toolbar_icon_size, preferences.toolbar_icon_size
            ))
            .show_ui(ui, |ui| {
                for size in [24, 32, 40, 48] {
                    ui.selectable_value(
                        &mut preferences.toolbar_icon_size,
                        size,
                        format!("{size} × {size} px"),
                    );
                }
            });
        ui.checkbox(&mut preferences.show_permanent_grid, "Show permanent grid");
        ui.checkbox(
            &mut preferences.show_construction_guides,
            "Show construction guides",
        );
        ui.label("Permanent grid normal");
        egui::ComboBox::from_id_salt("permanent-grid-normal")
            .selected_text(preferences.grid_normal_axis.label())
            .show_ui(ui, |ui| {
                for axis in [GridNormalAxis::Y, GridNormalAxis::X, GridNormalAxis::Z] {
                    ui.selectable_value(&mut preferences.grid_normal_axis, axis, axis.label());
                }
            });
        ui.label("Permanent grid extent (steps from origin)");
        ui.add(egui::DragValue::new(&mut preferences.grid_extent).range(1..=1000));
        ui.label("Normal line width");
        ui.add(egui::Slider::new(&mut preferences.normal_grid_width, 0.25..=4.0).suffix(" px"));
        ui.label("Thick line width");
        ui.add(egui::Slider::new(&mut preferences.thick_grid_width, 0.5..=8.0).suffix(" px"));
        ui.label("Arrange toolbars");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!preferences.toolbar_flow_top_down, "Flow L→R")
                .clicked()
            {
                preferences.toolbar_flow_top_down = false;
            }
            if ui
                .selectable_label(preferences.toolbar_flow_top_down, "Flow T→D")
                .clicked()
            {
                preferences.toolbar_flow_top_down = true;
            }
        });
        ui.collapsing("Visible toolbars", |ui| {
            for (toolbar_id, label) in LEGACY_TOOLBARS {
                let Some(stem) = toolbar_id.strip_prefix("builtin_toolbar_") else {
                    continue;
                };
                let mut shown = preferences
                    .toolbar_visibility
                    .get(stem)
                    .copied()
                    .unwrap_or(true);
                if ui.checkbox(&mut shown, *label).changed() {
                    preferences
                        .toolbar_visibility
                        .insert(stem.to_owned(), shown);
                }
            }
        });
        ui.weak("Toolbar layout and grid display are saved as application preferences.");
        if preferences != self.preferences {
            self.dispatch_ui_preferences(&preferences);
        }
    }

    fn dispatch_ui_preferences(&mut self, preferences: &Preferences) {
        let mut payload = CommandPayload::default();
        payload.values.insert(
            "ui_text_scale".to_owned(),
            serde_json::json!(preferences.ui_text_scale),
        );
        payload.values.insert(
            "auto_collapse_toolbars".to_owned(),
            serde_json::json!(preferences.auto_collapse_toolbars),
        );
        payload.values.insert(
            "toolbar_icon_size".to_owned(),
            serde_json::json!(preferences.toolbar_icon_size),
        );
        payload.values.insert(
            "show_permanent_grid".to_owned(),
            serde_json::json!(preferences.show_permanent_grid),
        );
        payload.values.insert(
            "show_construction_guides".to_owned(),
            serde_json::json!(preferences.show_construction_guides),
        );
        payload.values.insert(
            "grid_normal_axis".to_owned(),
            serde_json::json!(preferences.grid_normal_axis.id()),
        );
        payload.values.insert(
            "grid_extent".to_owned(),
            serde_json::json!(preferences.grid_extent),
        );
        payload.values.insert(
            "normal_grid_width".to_owned(),
            serde_json::json!(preferences.normal_grid_width),
        );
        payload.values.insert(
            "thick_grid_width".to_owned(),
            serde_json::json!(preferences.thick_grid_width),
        );
        payload.values.insert(
            "toolbar_flow_top_down".to_owned(),
            serde_json::json!(preferences.toolbar_flow_top_down),
        );
        payload.values.insert(
            "toolbars_visible".to_owned(),
            serde_json::json!(preferences.toolbars_visible),
        );
        payload.values.insert(
            "toolbar_visibility".to_owned(),
            serde_json::to_value(&preferences.toolbar_visibility).unwrap_or_default(),
        );
        payload.values.insert(
            "walk_move_speed".to_owned(),
            serde_json::json!(preferences.walk_move_speed),
        );
        payload.values.insert(
            "walk_jump_velocity".to_owned(),
            serde_json::json!(preferences.walk_jump_velocity),
        );
        payload.values.insert(
            "walk_gravity".to_owned(),
            serde_json::json!(preferences.walk_gravity),
        );
        payload.values.insert(
            "walk_height_adjust_speed".to_owned(),
            serde_json::json!(preferences.walk_height_adjust_speed),
        );
        self.dispatch_command("ui.update_preferences", &payload);
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
        let mut toolbars_visible = self.preferences.toolbars_visible;
        if ui
            .checkbox(&mut toolbars_visible, "Show toolbars")
            .changed()
        {
            let mut preferences = self.preferences.clone();
            preferences.toolbars_visible = toolbars_visible;
            self.dispatch_ui_preferences(&preferences);
        }
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
                            ui.label("G places a grid guide; T places an axis guide.");
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.poll_project_load();
            self.poll_project_save();
            if self.pending_project_load.is_some() || self.pending_project_save.is_some() {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        }
        ui.ctx()
            .set_zoom_factor(self.preferences.ui_text_scale.clamp(0.5, 2.0));
        let open_project_shortcut = !self.command_palette_open
            && self.pending_replacement.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::O));
        if open_project_shortcut {
            self.run_command("file.open");
        }
        let save_shortcut = !self.command_palette_open
            && self.pending_replacement.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| {
                input.modifiers.ctrl && input.key_pressed(egui::Key::S) && !input.modifiers.shift
            });
        if save_shortcut {
            self.run_command("file.save");
        }
        let save_as_shortcut = !self.command_palette_open
            && self.pending_replacement.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| {
                input.modifiers.ctrl && input.modifiers.shift && input.key_pressed(egui::Key::S)
            });
        if save_as_shortcut {
            self.run_command("file.save_as");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.command_palette_open
            && self.pending_replacement.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Q))
        {
            self.run_command("app.quit");
        }
        let delete_shortcut = !self.command_palette_open
            && self.pending_replacement.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| input.key_pressed(egui::Key::Delete));
        if delete_shortcut {
            self.run_command("edit.delete_selection");
        }
        let toggle_command_palette = self.pending_replacement.is_none()
            && ui.input(|input| {
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
        } else if !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::G)
            })
        {
            self.run_command("view.grid_guide");
        } else if !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::T)
            })
        {
            self.run_command("view.axis_guide");
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
        let show_tools = toolbar_visible && content_width >= 980.0;
        let show_inspector = self.preferences.panels_visible && content_width >= 650.0;
        let side_width =
            if show_tools { 324.0 } else { 0.0 } + if show_inspector { 420.0 } else { 0.0 };
        let viewport_width = (content_width - side_width - 16.0).max(120.0);

        ui.horizontal(|ui| {
            if show_tools {
                ui.allocate_ui_with_layout(
                    Vec2::new(324.0, content_height),
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
                    self.viewport_rect.width(),
                    self.viewport_rect.height()
                ));
            });
        });

        self.show_command_palette(ui.ctx());
        self.show_document_replacement_dialog(ui.ctx());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_project_path(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("octd");
    }
    path
}

#[cfg(not(target_arch = "wasm32"))]
fn write_project_file(
    path: &Path,
    document: &Document,
    history: Option<&OpaqueLegacyHistory>,
) -> Result<(), String> {
    let bytes = write_project(document, history).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(format!(
            "encoded project exceeds the {} MiB limit",
            MAX_PROJECT_BYTES / (1024 * 1024)
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "project path has no valid file name".to_owned())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut temporary_path = None;
    let mut temporary_file = None;
    for attempt in 0..32 {
        let candidate = parent.join(format!(
            ".{file_name}.meshcraft-{}-{nonce}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary_path = Some(candidate);
                temporary_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "could not create a temporary project file: {error}"
                ));
            }
        }
    }
    let Some(temporary_path) = temporary_path else {
        return Err("could not reserve a temporary project file".to_owned());
    };
    let mut temporary_file = temporary_file.expect("temporary path and file are created together");
    let write_result = temporary_file
        .write_all(&bytes)
        .and_then(|()| temporary_file.sync_all());
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("could not write temporary project file: {error}"));
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("could not replace {}: {error}", path.display()));
    }
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
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

fn payload_f64(payload: &CommandPayload, name: &str, default: f64) -> Result<f64, String> {
    match payload.values.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("{name} must be a finite number")),
    }
}

fn payload_bool(payload: &CommandPayload, name: &str, default: bool) -> Result<bool, String> {
    match payload.values.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("{name} must be a boolean")),
    }
}

fn payload_u32(payload: &CommandPayload, name: &str, default: u32) -> Result<u32, String> {
    match payload.values.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .ok_or_else(|| format!("{name} must be an unsigned 32-bit integer")),
    }
}

fn payload_vec3(payload: &CommandPayload, name: &str, default: Vec3) -> Result<Vec3, String> {
    let Some(value) = payload.values.get(name) else {
        return Ok(default);
    };
    payload_vec3_value(value, name)
}

fn payload_vec3_value(value: &serde_json::Value, name: &str) -> Result<Vec3, String> {
    let Some(values) = value.as_array() else {
        return Err(format!("`{name}` must be an array of three numbers"));
    };
    if values.len() != 3 {
        return Err(format!("`{name}` must contain exactly three numbers"));
    }
    let mut coordinates = [0.0; 3];
    for (index, value) in values.iter().enumerate() {
        let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
            return Err(format!("`{name}` must contain finite numbers"));
        };
        coordinates[index] = number;
    }
    Ok(Vec3::new(coordinates[0], coordinates[1], coordinates[2]))
}

fn command_result_with_occurrences(
    mut result: CommandResult,
    occurrences: &[OccurrenceRef],
) -> CommandResult {
    if let Ok(value) = serde_json::to_value(occurrences) {
        result.outputs.insert("occurrences".to_owned(), value);
    }
    result
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
        if std::mem::take(&mut self.quit_requested) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFERENCES_KEY, &self.preferences);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for MeshcraftApp {
    fn drop(&mut self) {
        if let Some(pending) = &self.pending_project_load {
            pending.cancellation.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandPayload, MeshcraftApp, OrthoView, Preferences, ReplacementDecision,
        SelectionSession, TransactionHistory, UiProfile, command_matches, registry_has_unique_ids,
        shell_commands, tool_command_id,
    };
    use crate::viewport::{
        ConstructionGuide, ConstructionGuideKind, GridNormalAxis, SnapKind, Viewport,
    };
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use eframe::egui::{self, Event, ImeEvent, Key, Modifiers, Pos2, RawInput, Rect, Vec2};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use meshcraft_core::commands::{
        CommandCapability, CommandCompletion, CommandKind, CommandValueType,
    };
    use meshcraft_core::document::Document;
    use meshcraft_core::geometry::Vec3;
    use meshcraft_core::transactions::{OpaqueLegacyHistory, OpaqueLegacyUndoEntry};
    use meshcraft_io::project::read_project;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::Write;
    #[cfg(not(target_arch = "wasm32"))]
    use std::path::{Path, PathBuf};
    #[cfg(not(target_arch = "wasm32"))]
    use std::thread;
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::Duration;

    const LEGACY_MODEL: &str =
        include_str!("../../../fixtures/legacy/samples/examples/arch_frame.octd");

    fn snapshot_json(root_only: bool) -> String {
        let mut value: Value = serde_json::from_str(LEGACY_MODEL).unwrap();
        if root_only {
            value["rootInstance"]["children"] = json!([]);
        }
        value.to_string()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn project_fixture_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/legacy/samples/examples/arch_frame.octd")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_project_load(app: &mut MeshcraftApp, path: &Path) -> String {
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("path".to_owned(), path.display().to_string().into());
        let dispatch = app.dispatch_command("file.open", &payload);
        assert!(dispatch.accepted);
        let CommandCompletion::Pending { job_id } = dispatch.result.completion else {
            panic!(
                "Open Project did not start a load job: {:?}",
                dispatch.result
            );
        };
        job_id
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait_for_project_load(app: &mut MeshcraftApp, job_id: &str) {
        for _ in 0..2500 {
            app.poll_project_load();
            let finished = app.command_job_result(job_id).is_some_and(|result| {
                !matches!(result.completion, CommandCompletion::Pending { .. })
            });
            if finished {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("project loading did not finish");
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait_for_project_save(app: &mut MeshcraftApp, job_id: &str) {
        for _ in 0..2500 {
            app.poll_project_save();
            let finished = app.command_job_result(job_id).is_some_and(|result| {
                !matches!(result.completion, CommandCompletion::Pending { .. })
            });
            if finished {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("project saving did not finish");
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
        app.saved_revision = app.document.revision();
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
            viewport_rect: egui::Rect::NOTHING,
            open_file_menu: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            quit_requested: false,
            panel_focus: None,
            document: Document::new(),
            saved_revision: 0,
            history: TransactionHistory::default(),
            legacy_history: None,
            legacy_active_index: None,
            #[cfg(not(target_arch = "wasm32"))]
            current_project_path: None,
            next_job_id: 1,
            job_results: BTreeMap::new(),
            pending_replacement: None,
            #[cfg(not(target_arch = "wasm32"))]
            replacement_after_save: None,
            pending_project_load: None,
            pending_project_save: None,
            selection: SelectionSession::default(),
            object_name_edit: None,
            viewport: Default::default(),
            guides: Vec::new(),
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

    #[test]
    fn viewport_world_mapping_uses_the_real_menu_and_button_layout_rects() {
        let expected_world = Vec3::ZERO;
        for profile in [UiProfile::DesktopMenus, UiProfile::Buttons] {
            let mut app = test_app();
            app.preferences.profile = profile;
            let context = egui::Context::default();
            run_workspace_frame(&mut app, &context, Vec::new());
            let rect = app.viewport_rect;
            let expected_screen = app.viewport.world_to_screen(rect, expected_world);
            run_workspace_frame(
                &mut app,
                &context,
                vec![Event::PointerMoved(expected_screen)],
            );
            let local = app
                .pointer_in_viewport
                .expect("world point remains inside the viewport");
            assert!((local - (expected_screen - rect.min)).length() < 1.0e-4);
            let recovered = app
                .viewport
                .screen_to_ground(rect, rect.min + local)
                .expect("viewport pointer maps onto the ground construction plane");
            assert!(
                recovered.distance(expected_world) < 1.0e-5,
                "profile={profile:?}"
            );
        }
    }

    fn text_center_in_row(output: &egui::FullOutput, text: &str, max_y: f32) -> Pos2 {
        fn collect(
            shape: &egui::epaint::Shape,
            text: &str,
            centers: &mut Vec<Pos2>,
            labels: &mut Vec<String>,
        ) {
            match shape {
                egui::epaint::Shape::Text(text_shape) => {
                    let label = text_shape.galley.job.text.clone();
                    labels.push(label.clone());
                    if label == text {
                        centers.push(text_shape.visual_bounding_rect().center());
                    }
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text, centers, labels);
                    }
                }
                _ => {}
            }
        }

        let mut centers = Vec::new();
        let mut labels = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, text, &mut centers, &mut labels);
        }
        centers
            .into_iter()
            .filter(|position| position.y <= max_y)
            .max_by(|left, right| left.x.total_cmp(&right.x))
            .unwrap_or_else(|| {
                panic!("could not find `{text}` in the top {max_y}px row; labels: {labels:?}")
            })
    }

    fn click_workspace_position(
        app: &mut MeshcraftApp,
        context: &egui::Context,
        position: Pos2,
    ) -> egui::FullOutput {
        run_workspace_frame(
            app,
            context,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_workspace_frame(
            app,
            context,
            vec![Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        )
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
        assert_eq!(197, entries.len());
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
            26,
            entries
                .iter()
                .filter(|command| command.descriptor.id.as_str().starts_with("view.toolbar."))
                .count()
        );
        assert!(shell_commands().get("edit.group").is_some());
        assert!(shell_commands().get("edit.delete_selection").is_some());
        assert!(shell_commands().get("edit.clear_selection").is_some());
        assert!(
            shell_commands()
                .get("edit.reset_scene_for_capture")
                .is_some()
        );
        assert!(shell_commands().get("document.snapshot").is_some());
        assert!(shell_commands().get("selection.set").is_some());
        assert!(
            shell_commands()
                .get("view.workspace.reset_layout")
                .is_some()
        );
        assert!(shell_commands().get("app.quit").is_some());
        assert!(shell_commands().get("export.screenshot").is_some());
        assert!(shell_commands().get("import.mesh").is_some());
        assert!(entries.iter().any(|command| {
            command.descriptor.id.as_str() == "tool.builtin.axial_grid"
                && command.descriptor.capability
                    == meshcraft_core::commands::CommandCapability::Deferred
        }));
    }

    #[test]
    fn legacy_toolbar_visibility_commands_update_and_persist_preferences() {
        let mut app = test_app();
        let document_revision = app.document.revision();

        for (toolbar_id, _) in super::LEGACY_TOOLBARS {
            let stem = toolbar_id.strip_prefix("builtin_toolbar_").unwrap();
            let show_id = format!("view.toolbar.{stem}.show");
            let hide_id = format!("view.toolbar.{stem}.hide");
            for id in [&show_id, &hide_id] {
                let command = shell_commands().get(id).expect("toolbar command exists");
                assert_eq!(CommandCapability::Desktop, command.descriptor.capability);
                assert_eq!(CommandKind::SessionMutation, command.descriptor.kind);
            }

            app.run_command(&hide_id);
            assert_eq!(Some(&false), app.preferences.toolbar_visibility.get(stem));
            assert_eq!(Some(true), app.query_command_state(&hide_id).checked);
            assert_eq!(Some(false), app.query_command_state(&show_id).checked);

            app.run_command(&show_id);
            assert_eq!(Some(&true), app.preferences.toolbar_visibility.get(stem));
            assert_eq!(Some(true), app.query_command_state(&show_id).checked);
            assert_eq!(Some(false), app.query_command_state(&hide_id).checked);
        }

        assert_eq!(document_revision, app.document.revision());
        assert!(!app.document_is_dirty());

        let mut payload = CommandPayload::default();
        payload.values.insert(
            "toolbar_visibility".to_owned(),
            json!({"actions": false, "camera": true}),
        );
        assert!(
            app.dispatch_command("ui.update_preferences", &payload)
                .accepted
        );
        assert_eq!(
            Some(&false),
            app.preferences.toolbar_visibility.get("actions")
        );
        assert_eq!(
            Some(&true),
            app.preferences.toolbar_visibility.get("camera")
        );

        let serialized = serde_json::to_string(&app.preferences).unwrap();
        let restored: Preferences = serde_json::from_str(&serialized).unwrap();
        assert_eq!(app.preferences, restored);
    }

    #[test]
    fn workspace_reset_is_command_driven_and_keeps_document_unchanged() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let document_revision = app.document.revision();
        app.preferences.profile = super::UiProfile::Hybrid;
        app.preferences.toolbars_visible = false;
        app.preferences.panels_visible = false;
        app.preferences.toolbar_icon_size = 48;
        app.preferences
            .panel_open
            .insert("objects".to_owned(), false);
        app.panel_focus = Some("objects".to_owned());

        let outcome =
            app.dispatch_command("view.workspace.reset_layout", &CommandPayload::default());

        assert!(outcome.accepted);
        assert_eq!(Preferences::default(), app.preferences);
        assert_eq!(None, app.panel_focus);
        assert_eq!(document_revision, app.document.revision());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn quit_is_a_palette_command_and_requests_native_close_without_document_mutation() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let document_revision = app.document.revision();
        let command = shell_commands().get("app.quit").unwrap();

        assert_eq!(Some("Ctrl+Q"), command.descriptor.shortcut.as_deref());
        assert!(app.query_command_state("app.quit").enabled);
        let outcome = app.dispatch_command("app.quit", &CommandPayload::default());

        assert!(outcome.accepted);
        assert!(app.quit_requested);
        assert_eq!(document_revision, app.document.revision());
    }

    #[test]
    fn legacy_toolbar_buttons_use_known_legacy_icon_regions_and_commands() {
        let regions = crate::assets::icon_regions();
        for command in shell_commands()
            .entries()
            .filter(|command| command.descriptor.id.as_str().starts_with("tool.builtin."))
        {
            let icon = command
                .descriptor
                .presentation
                .icon_key
                .as_deref()
                .unwrap_or_else(|| panic!("{} has no icon", command.descriptor.id));
            assert!(
                regions.contains_key(icon),
                "{} references missing icon {icon}",
                command.descriptor.id
            );
        }

        for (toolbar_id, _) in super::LEGACY_TOOLBARS {
            let stem = toolbar_id.strip_prefix("builtin_toolbar_").unwrap();
            for id in super::legacy_toolbar_command_ids(stem) {
                assert!(
                    shell_commands().get(&id).is_some(),
                    "missing toolbar command {id}"
                );
            }
        }
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
        assert!(!app.query_command_state("edit.undo").enabled);
        assert!(!app.query_command_state("edit.copy_selection").enabled);
        assert!(!app.query_command_state("edit.delete_selection").enabled);

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
    fn cuboid_tool_activation_is_shared_by_menu_toolbar_palette_and_structured_dispatch() {
        let context = egui::Context::default();
        let mut menu_path = test_app();
        let menu_bar = run_workspace_frame(&mut menu_path, &context, vec![]);
        let tools_menu = text_center_in_row(&menu_bar, "Tools", 48.0);
        click_workspace_position(&mut menu_path, &context, tools_menu);
        let open_menu = run_workspace_frame(&mut menu_path, &context, vec![]);
        let cuboid_item = text_center_in_row(&open_menu, "Cuboid", 500.0);
        click_workspace_position(&mut menu_path, &context, cuboid_item);
        assert_eq!("Cuboid", menu_path.active_tool);

        let toolbar_command_ids = super::legacy_toolbar_command_ids("construction_points");
        assert!(toolbar_command_ids.iter().any(|id| id == "tool.cuboid"));
        let mut toolbar_path = test_app();
        toolbar_path.preferences.profile = super::UiProfile::Buttons;
        let toolbar = run_workspace_frame(&mut toolbar_path, &context, vec![]);
        let cuboid_button = text_center_in_row(&toolbar, "C", 48.0);
        click_workspace_position(&mut toolbar_path, &context, cuboid_button);
        assert_eq!("Cuboid", toolbar_path.active_tool);

        let mut structured_path = test_app();
        let structured =
            structured_path.dispatch_command("tool.cuboid", &CommandPayload::default());
        assert!(structured.accepted);
        assert_eq!(CommandCompletion::Completed, structured.result.completion);

        let mut palette_path = test_app();
        open_palette(&mut palette_path, &context);
        palette_path.command_search = "tool.cuboid".to_owned();
        assert!(
            palette_path
                .matching_commands()
                .iter()
                .any(|command| command.descriptor.id.as_str() == "tool.cuboid")
        );
        run_workspace_frame(
            &mut palette_path,
            &context,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );

        for app in [&menu_path, &toolbar_path, &structured_path, &palette_path] {
            assert_eq!("Cuboid", app.active_tool);
            assert_eq!(menu_path.document, app.document);
            assert_eq!(menu_path.selected_occurrences(), app.selected_occurrences());
            assert_eq!(menu_path.document.revision(), app.document.revision());
            assert_eq!(0, app.history.entries().len());
        }
        assert!(!palette_path.command_palette_open);
    }

    #[test]
    fn camera_commands_share_registry_and_do_not_edit_the_document() {
        let mut app = test_app();
        let initial_revision = app.document.revision();

        app.run_command("view.camera.orbit");

        assert_eq!("Orbit", app.active_tool);
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(
            meshcraft_core::commands::CommandKind::ToolActivation,
            shell_commands()
                .get("view.camera.orbit")
                .unwrap()
                .descriptor
                .kind
        );

        app.run_command("view.frame_document");
        assert!(app.status.contains("Framed the document"));
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(
            meshcraft_core::commands::CommandKind::SessionMutation,
            shell_commands()
                .get("view.frame_document")
                .unwrap()
                .descriptor
                .kind
        );

        for (id, view) in [
            ("view.ortho.top", OrthoView::Top),
            ("view.ortho.bottom", OrthoView::Bottom),
            ("view.ortho.left", OrthoView::Left),
            ("view.ortho.right", OrthoView::Right),
            ("view.ortho.front", OrthoView::Front),
            ("view.ortho.back", OrthoView::Back),
        ] {
            assert!(app.query_command_state(id).enabled);
            app.run_command(id);
            assert_eq!(Some(view), app.viewport.orthographic_view());
            assert_eq!(Some(true), app.query_command_state(id).checked);
            assert_eq!(initial_revision, app.document.revision());
        }

        app.run_command("view.camera.reset");
        assert_eq!(None, app.viewport.orthographic_view());
        app.run_command("view.camera.walkthrough");
        assert!(app.viewport.walkthrough_active());
        assert_eq!(
            Some(true),
            app.query_command_state("view.camera.walkthrough").checked
        );
        assert_eq!(
            meshcraft_core::commands::CommandKind::SessionMutation,
            shell_commands()
                .get("view.camera.walkthrough")
                .unwrap()
                .descriptor
                .kind
        );
        app.run_command("view.camera.orthographic");
        assert_eq!(Some(OrthoView::Top), app.viewport.orthographic_view());
        assert!(!app.viewport.walkthrough_active());
        app.run_command("view.camera.orbit");
        assert_eq!(None, app.viewport.orthographic_view());
        assert!(!app.viewport.walkthrough_active());
    }

    #[test]
    fn guide_commands_activate_placement_and_keep_guides_out_of_document_geometry() {
        let mut app = test_app();
        let initial_revision = app.document.revision();
        let commands = MeshcraftApp::command_catalog();
        let guide_commands: &[(&str, &str, ConstructionGuideKind)] = &[
            ("view.grid_guide", "Grid Guide", ConstructionGuideKind::Grid),
            ("view.axis_guide", "Axis Guide", ConstructionGuideKind::Axis),
        ];
        for (index, &(id, tool_name, kind)) in guide_commands.iter().enumerate() {
            let descriptor = commands
                .iter()
                .find(|command| command.id.as_str() == id)
                .expect("guide command is registered in the palette");
            assert_eq!(CommandKind::ToolActivation, descriptor.kind);
            app.run_command(id);
            assert_eq!(tool_name, app.active_tool);

            let position = Pos2::new(440.0, 330.0);
            app.handle_viewport_click(
                Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0)),
                position,
                None,
            );
            assert_eq!(index + 1, app.guides.len());
            assert_eq!(kind, app.guides[index].kind);
            assert_eq!("Select", app.active_tool);
            assert_eq!(initial_revision, app.document.revision());
        }
    }

    #[test]
    fn guide_visibility_and_clear_commands_control_session_guides_from_the_palette() {
        let mut app = test_app();
        let initial_revision = app.document.revision();
        app.guides.push(
            ConstructionGuide::at_point(ConstructionGuideKind::Grid, Vec3::ZERO, Vec3::Y)
                .expect("grid guide basis"),
        );
        app.guides.push(
            ConstructionGuide::at_point(ConstructionGuideKind::Axis, Vec3::X, Vec3::Z)
                .expect("axis guide basis"),
        );

        let commands = MeshcraftApp::command_catalog();
        for id in ["view.guides.toggle", "view.guides.clear"] {
            let command = commands
                .iter()
                .find(|command| command.id.as_str() == id)
                .expect("guide control is available in the command palette");
            assert_eq!(CommandKind::SessionMutation, command.kind);
        }
        let remove_command = commands
            .iter()
            .find(|command| command.id.as_str() == "view.guide.remove")
            .expect("guide removal is available in the command palette");
        assert_eq!(CommandKind::ToolActivation, remove_command.kind);
        assert_eq!(
            Some(true),
            app.query_command_state("view.guides.toggle").checked
        );
        app.run_command("view.guides.toggle");
        assert!(!app.preferences.show_construction_guides);
        assert_eq!(
            Some(false),
            app.query_command_state("view.guides.toggle").checked
        );

        app.run_command("view.guides.clear");
        assert!(app.guides.is_empty());
        assert_eq!(initial_revision, app.document.revision());
        app.run_command("view.guides.toggle");
        assert!(app.preferences.show_construction_guides);

        app.guides.push(
            ConstructionGuide::at_point(ConstructionGuideKind::Axis, Vec3::ZERO, Vec3::Y)
                .expect("axis guide basis"),
        );
        app.run_command("view.guide.remove");
        let viewport_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let guide_origin = app.viewport.world_to_screen(viewport_rect, Vec3::ZERO);
        app.handle_viewport_click(viewport_rect, guide_origin, None);
        assert!(app.guides.is_empty());
        assert_eq!("Select", app.active_tool);
        assert_eq!(initial_revision, app.document.revision());
    }

    #[test]
    fn camera_pose_changes_remain_outside_document_dirty_tracking() {
        let mut app = test_app();
        let original_revision = app.document.revision();
        let camera_state = json!({
            "position": {"x": 13.0, "y": 8.0, "z": 2.0},
            "target": {"x": 1.0, "y": 2.0, "z": 3.0},
            "fieldOfView": 58.0
        });

        assert!(app.viewport.restore_legacy_camera_state(&camera_state));
        app.run_command("view.ortho.front");
        app.run_command("view.camera.orbit");

        assert_eq!(original_revision, app.document.revision());
        assert!(!app.document_is_dirty());
    }

    #[test]
    fn camera_pose_command_sets_typed_pose_without_mutating_the_document() {
        let mut app = test_app();
        let revision = app.document.revision();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("position".to_owned(), json!([13.0, 8.0, 2.0]));
        payload
            .values
            .insert("target".to_owned(), json!([1.0, 2.0, 3.0]));
        payload
            .values
            .insert("field_of_view".to_owned(), json!(58.0));
        payload.values.insert("near".to_owned(), json!(0.25));
        payload.values.insert("far".to_owned(), json!(900.0));

        let dispatch = app.dispatch_command("view.camera.set_pose", &payload);

        assert!(dispatch.accepted);
        assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
        assert_eq!(revision, app.document.revision());
        assert_eq!(None, dispatch.result.document_revision);
        let camera = dispatch.result.outputs.get("camera").unwrap();
        for (axis, expected) in [("x", 13.0), ("y", 8.0), ("z", 2.0)] {
            let actual = camera["position"][axis].as_f64().unwrap();
            assert!((actual - expected).abs() < 1.0e-10);
        }
        assert_eq!(json!({"x": 1.0, "y": 2.0, "z": 3.0}), camera["target"]);
        assert_eq!(json!(58.0), camera["fieldOfView"]);
        assert_eq!(json!(0.25), camera["near"]);
        assert_eq!(json!(900.0), camera["far"]);
        assert_eq!(
            "view.camera.set_pose",
            dispatch.command_id.unwrap().as_str()
        );
    }

    #[test]
    fn camera_pose_command_rejects_missing_or_invalid_inputs_without_changing_pose() {
        let mut app = test_app();
        let original = app.viewport.legacy_camera_state();
        let missing = app.dispatch_command("view.camera.set_pose", &CommandPayload::default());
        assert!(!missing.accepted);
        assert_eq!(original, app.viewport.legacy_camera_state());

        let mut invalid = CommandPayload::default();
        invalid
            .values
            .insert("position".to_owned(), json!([1.0, 1.0, 1.0]));
        invalid
            .values
            .insert("target".to_owned(), json!([1.0, 1.0, 1.0]));
        let rejected = app.dispatch_command("view.camera.set_pose", &invalid);
        assert!(rejected.accepted);
        assert_eq!(CommandCompletion::Failed, rejected.result.completion);
        assert_eq!(original, app.viewport.legacy_camera_state());

        invalid
            .values
            .insert("target".to_owned(), json!([0.0, 0.0, 0.0]));
        invalid
            .values
            .insert("field_of_view".to_owned(), json!(179.0));
        let rejected = app.dispatch_command("view.camera.set_pose", &invalid);
        assert!(rejected.accepted);
        assert_eq!(CommandCompletion::Failed, rejected.result.completion);
        assert_eq!(original, app.viewport.legacy_camera_state());
    }

    #[test]
    fn rectangle_tool_creates_a_face_in_the_active_orthographic_plane() {
        let mut app = test_app();
        app.active_tool = "Rectangle".to_owned();
        app.viewport.set_orthographic_view(OrthoView::Front);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

        app.handle_viewport_click(rect, rect.center(), None);
        app.handle_viewport_click(rect, Pos2::new(456.0, 244.0), None);

        assert_eq!("Select", app.active_tool);
        let occurrence = app.selected_occurrences().pop().unwrap();
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(2, geometry.triangles.len());
        assert!(
            geometry
                .vertices
                .values()
                .all(|vertex| vertex.position.z == 0.0)
        );
        assert!(
            geometry
                .vertices
                .values()
                .any(|vertex| vertex.position.x == 2.0)
        );
        assert!(
            geometry
                .vertices
                .values()
                .any(|vertex| vertex.position.y == 2.0)
        );
    }

    #[test]
    fn line_tool_commits_scene_snaps_as_one_undoable_edit() {
        let mut app = test_app();
        let source = app
            .document
            .add_line_segment(
                "reference",
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
            )
            .unwrap();
        app.active_tool = "Line".to_owned();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let start = Vec3::new(2.0, 0.0, 0.0);
        let start_screen = app.viewport.world_to_screen(rect, start);
        let start_snap = app
            .viewport
            .snap_point(rect, &app.document, start_screen, 12.0, 1.0)
            .unwrap();
        assert_eq!(SnapKind::Endpoint, start_snap.kind);

        let raw_end = Vec3::new(-3.02, 0.0, 4.02);
        let end_screen = app.viewport.world_to_screen(rect, raw_end);
        let end_snap = app
            .viewport
            .snap_point(rect, &app.document, end_screen, 12.0, 1.0)
            .unwrap();
        assert_eq!(SnapKind::Grid, end_snap.kind);

        app.handle_viewport_click(rect, start_screen, Some(start_snap));
        app.handle_viewport_click(rect, end_screen, Some(end_snap));

        assert_eq!("Select", app.active_tool);
        assert_eq!(1, app.history.entries().len());
        assert!(app.status.contains("Grid snap"));
        let created = app.selected_occurrences().pop().unwrap();
        assert_ne!(source, created);
        let geometry = app.document.effective_geometry(&created).unwrap();
        let positions = geometry
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        assert!(positions.contains(&start_snap.point));
        assert!(positions.contains(&end_snap.point));
    }

    #[test]
    fn inspector_section_toggles_are_palette_commands() {
        let mut app = test_app();
        let initial_revision = app.document.revision();

        for (panel, _) in super::INSPECTOR_PANELS {
            let id = format!("view.panel.{panel}.toggle");
            assert!(shell_commands().get(&id).is_some(), "missing {id}");
        }
        assert_eq!(
            Some(true),
            app.query_command_state("view.panel.ui_settings.toggle")
                .checked
        );
        app.run_command("view.panel.ui_settings.toggle");
        assert_eq!(Some(&false), app.preferences.panel_open.get("ui_settings"));
        assert_eq!(
            Some(false),
            app.query_command_state("view.panel.ui_settings.toggle")
                .checked
        );
        app.run_command("view.panel.ui_settings.toggle");
        assert_eq!(Some(&true), app.preferences.panel_open.get("ui_settings"));
        assert!(app.preferences.panels_visible);
        assert_eq!(initial_revision, app.document.revision());
    }

    #[test]
    fn delete_selection_command_is_undoable_and_clears_selection() {
        let mut app = test_app();
        let first = app
            .document
            .add_cuboid(
                "first",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::ZERO,
            )
            .unwrap();
        let second = app
            .document
            .add_cuboid(
                "second",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::new(3.0, 0.0, 0.0),
            )
            .unwrap();
        app.set_selection([first, second]);
        assert!(app.query_command_state("edit.delete_selection").enabled);

        let deleted = app.dispatch_command("edit.delete_selection", &CommandPayload::default());

        assert!(deleted.accepted);
        assert_eq!(2, deleted.result.outputs["deleted_count"]);
        assert!(app.document.occurrence_refs().unwrap().is_empty());
        assert!(app.selected_occurrences().is_empty());
        assert!(app.can_undo_document());
        app.run_command("edit.undo");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.redo");
        assert!(app.document.occurrence_refs().unwrap().is_empty());
    }

    #[test]
    fn delete_command_handles_selection_across_separate_groups() {
        let mut app = test_app();
        let make_pair = |app: &mut MeshcraftApp, prefix: &str, offset: f64| {
            let first = app
                .document
                .add_cuboid(
                    format!("{prefix}-first"),
                    meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                    meshcraft_core::geometry::Vec3::new(offset, 0.0, 0.0),
                )
                .unwrap();
            let second = app
                .document
                .add_cuboid(
                    format!("{prefix}-second"),
                    meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                    meshcraft_core::geometry::Vec3::new(offset + 2.0, 0.0, 0.0),
                )
                .unwrap();
            app.document
                .create_object_from_root_occurrences(&[first, second], format!("{prefix}-pair"))
                .unwrap()
        };
        let first_group = make_pair(&mut app, "first", 0.0);
        let second_group = make_pair(&mut app, "second", 8.0);
        let selected = [first_group.clone(), second_group.clone()]
            .into_iter()
            .map(|group| {
                app.document
                    .occurrence_refs()
                    .unwrap()
                    .into_iter()
                    .find(|item| item.path.len() == 2 && item.path[0] == group.path[0])
                    .unwrap()
            })
            .collect::<Vec<_>>();
        app.set_selection(selected);

        assert!(app.query_command_state("edit.delete_selection").enabled);
        let dispatch = app.dispatch_command("edit.delete_selection", &CommandPayload::default());

        assert!(dispatch.accepted);
        assert_eq!(json!(2), dispatch.result.outputs["deleted_count"]);
        for group in [first_group, second_group] {
            assert_eq!(
                1,
                app.document
                    .occurrence_refs()
                    .unwrap()
                    .iter()
                    .filter(|item| item.path.len() == 2 && item.path[0] == group.path[0])
                    .count()
            );
        }
        assert!(app.document.validate().is_ok());
    }

    #[test]
    fn delete_key_uses_the_shared_command_when_viewport_owns_focus() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "selected",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::ZERO,
            )
            .unwrap();
        app.set_selection([occurrence]);
        let context = egui::Context::default();
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(app.document.occurrence_refs().unwrap().is_empty());
    }

    #[test]
    fn delete_command_is_searchable_from_the_palette() {
        let command = shell_commands()
            .get("edit.delete_selection")
            .expect("delete command is registered");
        assert!(command_matches(&command.descriptor, "delete selection"));
        assert_eq!(Some("Delete"), command.descriptor.shortcut.as_deref());
        assert_eq!(CommandKind::DocumentMutation, command.descriptor.kind);
    }

    #[test]
    fn clear_selection_command_is_searchable_and_does_not_edit_the_document() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let revision = app.document.revision();
        let can_undo = app.can_undo_document();
        assert!(!app.selected_occurrences().is_empty());
        assert!(app.query_command_state("edit.clear_selection").enabled);

        let command = shell_commands()
            .get("edit.clear_selection")
            .expect("clear selection command is registered");
        assert!(command_matches(&command.descriptor, "clear selection"));
        assert_eq!(CommandKind::SessionMutation, command.descriptor.kind);
        let dispatch = app.dispatch_command("edit.clear_selection", &CommandPayload::default());

        assert!(dispatch.accepted);
        assert!(app.selected_occurrences().is_empty());
        assert_eq!(revision, app.document.revision());
        assert_eq!(can_undo, app.can_undo_document());
        assert!(!app.query_command_state("edit.clear_selection").enabled);
    }

    #[test]
    fn reset_scene_for_capture_clears_model_and_restores_the_reference_camera() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.active_tool = "Rectangle".to_owned();
        app.viewport.set_orthographic_view(OrthoView::Front);
        app.preferences.panels_visible = true;

        let dispatch =
            app.dispatch_command("edit.reset_scene_for_capture", &CommandPayload::default());

        assert!(dispatch.accepted);
        assert!(app.document.occurrence_refs().unwrap().is_empty());
        assert!(app.selected_occurrences().is_empty());
        assert_eq!("Select", app.active_tool);
        assert_eq!(None, app.viewport.orthographic_view());
        assert!(!app.viewport.walkthrough_active());
        assert!(!app.preferences.panels_visible);
        assert!(app.status.contains("perspective, Y-up"));
        let camera = app.viewport.legacy_camera_state();
        for (axis, expected) in [("x", 18.0), ("y", 14.0), ("z", 18.0)] {
            let actual = camera["position"][axis].as_f64().unwrap();
            assert!((actual - expected).abs() < 1.0e-9, "{axis}: {actual}");
        }
    }

    #[test]
    fn document_snapshot_command_returns_read_only_scene_state() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let revision = app.document.revision();
        let can_undo = app.can_undo_document();
        let expected_document = serde_json::to_value(&app.document).unwrap();
        let expected_selection = serde_json::to_value(app.selected_occurrences()).unwrap();
        let expected_camera = app.viewport.legacy_camera_state();

        let command = shell_commands()
            .get("document.snapshot")
            .expect("snapshot command is registered");
        assert!(command_matches(&command.descriptor, "snapshot document"));
        assert_eq!(CommandKind::ReadOnly, command.descriptor.kind);
        let dispatch = app.dispatch_command("document.snapshot", &CommandPayload::default());

        assert!(dispatch.accepted);
        assert_eq!(Some(revision), dispatch.result.document_revision);
        assert_eq!(expected_document, dispatch.result.outputs["document"]);
        assert_eq!(expected_selection, dispatch.result.outputs["selection"]);
        assert_eq!(expected_camera, dispatch.result.outputs["camera"]);
        assert_eq!(revision, app.document.revision());
        assert_eq!(can_undo, app.can_undo_document());
    }

    #[test]
    fn set_selection_command_validates_paths_before_changing_session_state() {
        let mut app = test_app();
        let first = app
            .document
            .add_cuboid(
                "first",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::ZERO,
            )
            .unwrap();
        let second = app
            .document
            .add_cuboid(
                "second",
                meshcraft_core::geometry::Vec3::new(1.0, 1.0, 1.0),
                meshcraft_core::geometry::Vec3::new(3.0, 0.0, 0.0),
            )
            .unwrap();
        let revision = app.document.revision();
        let can_undo = app.can_undo_document();
        let command = shell_commands()
            .get("selection.set")
            .expect("selection command is registered");
        assert!(command_matches(&command.descriptor, "set occurrences"));
        assert_eq!(CommandKind::SessionMutation, command.descriptor.kind);
        assert!(command.descriptor.parameters[0].required);

        let mut payload = CommandPayload::default();
        payload.values.insert(
            "paths".to_owned(),
            json!([[second.path[0].0], [first.path[0].0]]),
        );
        let dispatch = app.dispatch_command("selection.set", &payload);

        assert!(dispatch.accepted);
        assert_eq!(json!(2), dispatch.result.outputs["selected_count"]);
        assert_eq!(
            json!(app.selected_occurrences()),
            dispatch.result.outputs["selection"]
        );
        assert_eq!(Some(revision), dispatch.result.document_revision);
        assert_eq!(revision, app.document.revision());
        assert_eq!(can_undo, app.can_undo_document());
        let previous_selection = app.selected_occurrences();

        payload
            .values
            .insert("paths".to_owned(), json!([[first.path[0].0], [u64::MAX]]));
        let rejected = app.dispatch_command("selection.set", &payload);

        assert!(rejected.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            rejected.result.completion
        );
        assert_eq!(previous_selection, app.selected_occurrences());
        assert_eq!(revision, app.document.revision());
    }

    #[test]
    fn structured_dispatch_uses_the_palette_registry_and_typed_cuboid_parameters() {
        let catalog = MeshcraftApp::command_catalog();
        let add_cuboid = catalog
            .iter()
            .find(|command| command.id.as_str() == "geometry.add_cuboid")
            .unwrap();
        assert_eq!(1, add_cuboid.schema_version);
        assert_eq!(2, add_cuboid.parameters.len());

        let mut menu_path = test_app();
        menu_path.run_command("geometry.add_cuboid");

        let mut structured_path = test_app();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("dimensions".to_owned(), json!([2.0, 2.0, 2.0]));
        payload
            .values
            .insert("origin".to_owned(), json!([-1.0, 0.0, -1.0]));
        let result = structured_path.dispatch_command("geometry.add_cuboid", &payload);

        assert!(result.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Completed,
            result.result.completion
        );
        assert_eq!(
            Some(structured_path.document.revision()),
            result.result.document_revision
        );
        assert_eq!(
            1,
            result.result.outputs["occurrences"]
                .as_array()
                .unwrap()
                .len()
        );
        assert_eq!(menu_path.document, structured_path.document);
        assert_eq!(1, structured_path.history.entries().len());
    }

    #[test]
    fn structured_dispatch_rejects_invalid_geometry_values_without_publishing() {
        let mut app = test_app();
        let before = app.document.clone();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("dimensions".to_owned(), json!([1.0, 2.0]));

        let result = app.dispatch_command("geometry.add_cuboid", &payload);

        assert!(result.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            result.result.completion
        );
        assert_eq!(before, app.document);
        assert!(app.status.contains("exactly three numbers"));
        assert!(app.history.entries().is_empty());
    }

    #[test]
    fn structured_array_dispatch_uses_the_supplied_count_and_spacing() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let mut payload = CommandPayload::default();
        payload.values.insert("count".to_owned(), json!(2));
        payload.values.insert("spacing".to_owned(), json!(4.0));

        let result = app.dispatch_command("object.array_selection", &payload);

        assert!(result.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Completed,
            result.result.completion
        );
        assert_eq!(
            Some(app.document.revision()),
            result.result.document_revision
        );
        assert_eq!(
            2,
            result.result.outputs["occurrences"]
                .as_array()
                .unwrap()
                .len()
        );
        let copies = app.selected_occurrences();
        assert_eq!(2, copies.len());
        let x_positions = copies
            .iter()
            .map(|copy| {
                app.document
                    .resolve_occurrence(copy)
                    .unwrap()
                    .world_transform
                    .translation
                    .x
            })
            .collect::<Vec<_>>();
        assert_eq!(vec![3.0, 7.0], x_positions);
    }

    #[test]
    fn model_settings_inspector_and_structured_command_share_transaction_validation() {
        let mut app = test_app();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("unit_name".to_owned(), json!("meter"));
        payload.values.insert("unit_size".to_owned(), json!(0.5));
        payload.values.insert("grid_size".to_owned(), json!(0.25));
        payload
            .values
            .insert("circle_segments".to_owned(), json!(32));
        payload.values.insert("snap_radius".to_owned(), json!(8.0));

        let result = app.dispatch_command("model.update_settings", &payload);

        assert!(result.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Completed,
            result.result.completion
        );
        assert_eq!(
            Some(app.document.revision()),
            result.result.document_revision
        );
        assert_eq!("meter", app.document.model_settings().unit_name);
        assert_eq!(0.5, app.document.model_settings().unit_size);
        assert_eq!(0.25, app.document.model_settings().grid_size);
        assert_eq!(32, app.document.model_settings().circle_segments);
        assert_eq!(8.0, app.document.model_settings().snap_radius);
        assert_eq!(1, app.history.entries().len());

        let before = app.document.clone();
        let mut invalid = CommandPayload::default();
        invalid.values.insert("grid_size".to_owned(), json!(0.0));
        let rejected = app.dispatch_command("model.update_settings", &invalid);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            rejected.result.completion
        );
        assert_eq!(before, app.document);
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn ui_settings_command_applies_preferences_without_document_history() {
        let mut app = test_app();
        let initial_revision = app.document.revision();
        app.run_command("ui.update_preferences");
        assert!(app.preferences.panels_visible);
        assert_eq!(Some(&true), app.preferences.panel_open.get("ui_settings"));

        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("ui_text_scale".to_owned(), json!(1.25));
        payload
            .values
            .insert("auto_collapse_toolbars".to_owned(), json!(true));
        payload
            .values
            .insert("toolbar_icon_size".to_owned(), json!(40));
        payload
            .values
            .insert("show_permanent_grid".to_owned(), json!(false));
        payload
            .values
            .insert("grid_normal_axis".to_owned(), json!("z"));
        payload.values.insert("grid_extent".to_owned(), json!(64));
        payload
            .values
            .insert("normal_grid_width".to_owned(), json!(1.5));
        payload
            .values
            .insert("thick_grid_width".to_owned(), json!(4.0));
        payload
            .values
            .insert("toolbar_flow_top_down".to_owned(), json!(true));
        payload
            .values
            .insert("toolbars_visible".to_owned(), json!(false));
        payload
            .values
            .insert("walk_move_speed".to_owned(), json!(30.0));
        payload
            .values
            .insert("walk_jump_velocity".to_owned(), json!(18.0));
        payload
            .values
            .insert("walk_gravity".to_owned(), json!(52.0));
        payload
            .values
            .insert("walk_height_adjust_speed".to_owned(), json!(9.0));

        let result = app.dispatch_command("ui.update_preferences", &payload);

        assert!(result.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Completed,
            result.result.completion
        );
        assert_eq!(1.25, app.preferences.ui_text_scale);
        assert!(app.preferences.auto_collapse_toolbars);
        assert_eq!(40, app.preferences.toolbar_icon_size);
        assert!(!app.preferences.show_permanent_grid);
        assert_eq!(GridNormalAxis::Z, app.preferences.grid_normal_axis);
        assert_eq!(64, app.preferences.grid_extent);
        assert_eq!(1.5, app.preferences.normal_grid_width);
        assert_eq!(4.0, app.preferences.thick_grid_width);
        assert!(app.preferences.toolbar_flow_top_down);
        assert!(!app.preferences.toolbars_visible);
        assert_eq!(30.0, app.preferences.walk_move_speed);
        assert_eq!(18.0, app.preferences.walk_jump_velocity);
        assert_eq!(52.0, app.preferences.walk_gravity);
        assert_eq!(9.0, app.preferences.walk_height_adjust_speed);
        assert_eq!(initial_revision, app.document.revision());
        assert!(app.history.entries().is_empty());

        let before = app.preferences.clone();
        let mut invalid = CommandPayload::default();
        invalid.values.insert("grid_extent".to_owned(), json!(1001));
        let rejected = app.dispatch_command("ui.update_preferences", &invalid);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            rejected.result.completion
        );
        assert_eq!(before.grid_extent, app.preferences.grid_extent);

        let mut invalid_grid_axis = CommandPayload::default();
        invalid_grid_axis
            .values
            .insert("grid_normal_axis".to_owned(), json!("diagonal"));
        let rejected = app.dispatch_command("ui.update_preferences", &invalid_grid_axis);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            rejected.result.completion
        );
        assert_eq!(before.grid_normal_axis, app.preferences.grid_normal_axis);

        let mut invalid_tuning = CommandPayload::default();
        invalid_tuning
            .values
            .insert("walk_move_speed".to_owned(), json!(0.0));
        let rejected = app.dispatch_command("ui.update_preferences", &invalid_tuning);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Failed,
            rejected.result.completion
        );
        assert_eq!(before.walk_move_speed, app.preferences.walk_move_speed);
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
        for id in [
            "view.plugin_manager",
            "render.raytrace",
            "render.global_illumination",
            "render.stop",
            "render.save",
        ] {
            assert!(shell_commands().get(id).is_some(), "missing {id}");
            assert!(!app.query_command_state(id).enabled, "enabled {id}");
            assert!(
                app.query_command_state(id)
                    .disabled_reason
                    .is_some_and(|reason| reason.contains("not implemented")),
                "missing deferred reason for {id}"
            );
        }
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
        assert_eq!(GridNormalAxis::Y, preferences.grid_normal_axis);
        assert_eq!(20, preferences.grid_extent);
        assert_eq!(1.0, preferences.normal_grid_width);
        assert_eq!(3.0, preferences.thick_grid_width);
        assert_eq!(24.0, preferences.walk_move_speed);
        assert_eq!(16.0, preferences.walk_jump_velocity);
        assert_eq!(48.0, preferences.walk_gravity);
        assert_eq!(8.0, preferences.walk_height_adjust_speed);
        assert!(preferences.panels_visible);

        let legacy_axis: Preferences =
            serde_json::from_str(r#"{"grid_normal_axis":"x_right"}"#).unwrap();
        assert_eq!(GridNormalAxis::X, legacy_axis.grid_normal_axis);
        let unknown_axis: Preferences =
            serde_json::from_str(r#"{"grid_normal_axis":"unknown"}"#).unwrap();
        assert_eq!(GridNormalAxis::Y, unknown_axis.grid_normal_axis);
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

    #[test]
    fn open_project_is_a_desktop_host_command_with_an_optional_path() {
        let command = MeshcraftApp::command_catalog()
            .into_iter()
            .find(|command| command.id.as_str() == "file.open")
            .unwrap();

        assert_eq!(CommandKind::HostRequest, command.kind);
        assert_eq!(CommandCapability::Desktop, command.capability);
        assert_eq!(Some("Ctrl+O"), command.shortcut.as_deref());
        assert_eq!(1, command.parameters.len());
        assert_eq!("path", command.parameters[0].name);
        assert_eq!(CommandValueType::String, command.parameters[0].value_type);
        assert!(!command.parameters[0].required);
        assert!(command_matches(&command, "open project"));

        for id in ["file.save", "file.save_as"] {
            let save = MeshcraftApp::command_catalog()
                .into_iter()
                .find(|command| command.id.as_str() == id)
                .unwrap();
            assert_eq!(CommandKind::HostRequest, save.kind);
            assert_eq!(CommandCapability::Desktop, save.capability);
            assert_eq!("path", save.parameters[0].name);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn dirty_open_requests_a_decision_and_cancel_preserves_the_document() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let before = app.document.clone();
        let mut payload = CommandPayload::default();
        payload.values.insert(
            "path".to_owned(),
            project_fixture_path().display().to_string().into(),
        );

        assert!(app.query_command_state("file.open").enabled);
        let result = app.dispatch_command("file.open", &payload);

        assert!(result.accepted);
        assert!(matches!(
            &result.result.completion,
            CommandCompletion::Pending { .. }
        ));
        assert!(app.pending_replacement.is_some());
        assert_eq!(before, app.document);

        app.resolve_document_replacement(ReplacementDecision::Cancel);

        assert!(app.pending_replacement.is_none());
        assert_eq!(before, app.document);
        assert_eq!("Document replacement cancelled.", app.status);
        let job_id = match result.result.completion {
            CommandCompletion::Pending { job_id, .. } => job_id,
            _ => unreachable!(),
        };
        let completed = app.job_results.get(&job_id).unwrap();
        assert_eq!(CommandCompletion::Completed, completed.completion);
        assert_eq!(Some(&true.into()), completed.outputs.get("cancelled"));
    }

    #[test]
    fn dirty_new_can_discard_and_replace_the_document() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let result = app.dispatch_command("file.new", &CommandPayload::default());

        assert!(result.accepted);
        assert_eq!(1, app.document.definitions().count());
        app.resolve_document_replacement(ReplacementDecision::Discard);

        assert!(app.pending_replacement.is_none());
        assert_eq!(0, app.document.definitions().count());
        assert_eq!("New empty document.", app.status);
        let job_id = match result.result.completion {
            CommandCompletion::Pending { job_id, .. } => job_id,
            _ => unreachable!(),
        };
        assert_eq!(
            CommandCompletion::Completed,
            app.job_results.get(&job_id).unwrap().completion
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn saving_before_new_continues_after_the_save_finishes() {
        let path = std::env::temp_dir().join(format!(
            "meshcraft-replace-save-{}-{}.octd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.current_project_path = Some(path.clone());
        let replacement = app.dispatch_command("file.new", &CommandPayload::default());
        let replacement_job = match &replacement.result.completion {
            CommandCompletion::Pending { job_id } => job_id.clone(),
            _ => panic!("dirty New should wait for a decision"),
        };

        app.resolve_document_replacement(ReplacementDecision::Save);
        assert!(app.pending_project_save.is_some());
        for _ in 0..100 {
            app.poll_project_save();
            if app.pending_project_save.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(app.pending_project_save.is_none());
        assert!(app.document.definitions().next().is_none());
        assert_eq!(
            CommandCompletion::Completed,
            app.job_results.get(&replacement_job).unwrap().completion
        );
        let saved = read_project(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(1, saved.document.definitions().count());
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn newer_edits_after_save_prompt_again_before_replacement() {
        let path = std::env::temp_dir().join(format!(
            "meshcraft-replace-newer-{}-{}.octd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.current_project_path = Some(path.clone());
        app.dispatch_command("file.new", &CommandPayload::default());
        app.resolve_document_replacement(ReplacementDecision::Save);
        app.run_command("geometry.add_cuboid");

        for _ in 0..100 {
            app.poll_project_save();
            if app.pending_project_save.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(app.pending_project_save.is_none());
        assert!(app.document_is_dirty());
        assert!(app.pending_replacement.is_some());
        assert_eq!(2, app.document.definitions().count());
        app.resolve_document_replacement(ReplacementDecision::Cancel);
        assert_eq!(2, app.document.definitions().count());
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn save_and_save_as_write_legacy_projects_and_track_the_saved_revision() {
        let path = std::env::temp_dir().join(format!(
            "meshcraft-save-{}-{}.octd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        assert!(app.document_is_dirty());
        let camera_input = json!({
            "position": {"x": 9.0, "y": 4.0, "z": -6.0},
            "target": {"x": 1.0, "y": 2.0, "z": 3.0},
            "near": 0.2,
            "far": 320.0,
            "fieldOfView": 54.0
        });
        assert!(app.viewport.restore_legacy_camera_state(&camera_input));
        let model_revision_before_save = app.document.revision();
        let expected_camera = app.viewport.legacy_camera_state();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("path".to_owned(), path.display().to_string().into());
        let dispatch = app.dispatch_command("file.save_as", &payload);
        assert!(dispatch.accepted);
        let CommandCompletion::Pending { job_id } = dispatch.result.completion else {
            panic!("Save As did not start a save job");
        };
        wait_for_project_save(&mut app, &job_id);
        assert_eq!(
            CommandCompletion::Completed,
            app.command_job_result(&job_id).unwrap().completion
        );
        assert!(!app.document_is_dirty());
        assert_eq!(Some(path.display().to_string()), app.current_project_path());
        let reopened = read_project(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(1, reopened.document.occurrence_refs().unwrap().len());
        assert_eq!(model_revision_before_save, app.document.revision());
        let saved_camera = meshcraft_core::legacy::legacy_camera_state(&reopened.document).unwrap();
        assert_eq!(expected_camera["position"], saved_camera["position"]);
        assert_eq!(expected_camera["target"], saved_camera["target"]);
        assert_eq!(expected_camera["fieldOfView"], saved_camera["fieldOfView"]);
        let mut reopened_viewport = Viewport::default();
        assert!(reopened_viewport.restore_legacy_camera_state(saved_camera));
        let restored_camera = reopened_viewport.legacy_camera_state();
        let camera_vector = |value: &Value| {
            serde_json::from_value::<meshcraft_core::geometry::Vec3>(value.clone()).unwrap()
        };
        assert!(
            camera_vector(&restored_camera["position"])
                .distance(camera_vector(&saved_camera["position"]))
                < 1.0e-10
        );
        assert_eq!(saved_camera["target"], restored_camera["target"]);
        assert_eq!(saved_camera["fieldOfView"], restored_camera["fieldOfView"]);
        let saved_history = reopened.history.as_ref().unwrap();
        assert_eq!(2, saved_history.entries.len());
        assert_eq!(1, saved_history.index);

        let mut reopened_app = test_app();
        let load_job = start_project_load(&mut reopened_app, &path);
        wait_for_project_load(&mut reopened_app, &load_job);
        assert_eq!(1, reopened_app.document.occurrence_refs().unwrap().len());
        reopened_app.run_command("edit.undo");
        assert!(
            reopened_app
                .document
                .definitions()
                .all(|definition| definition.geometry.triangles.is_empty())
        );
        reopened_app.run_command("edit.redo");
        assert!(
            reopened_app
                .document
                .definitions()
                .any(|definition| !definition.geometry.triangles.is_empty())
        );

        app.legacy_history = Some(OpaqueLegacyHistory {
            max_entries: 4,
            index: 0,
            entries: vec![OpaqueLegacyUndoEntry {
                data: "stale-snapshot".to_owned(),
                label: Some("Old state".to_owned()),
                timestamp_millis: 1,
                diagnostic: None,
                extensions: BTreeMap::new(),
            }],
            extensions: BTreeMap::new(),
        });
        let mut settings_payload = CommandPayload::default();
        settings_payload
            .values
            .insert("grid_size".to_owned(), json!(2.0));
        assert!(
            app.dispatch_command("model.update_settings", &settings_payload)
                .accepted
        );
        let dirty_revision = app.document.revision();
        assert!(app.document_is_dirty());
        let dispatch = app.dispatch_command("file.save", &CommandPayload::default());
        assert!(dispatch.accepted);
        let CommandCompletion::Pending { job_id } = dispatch.result.completion else {
            panic!("Save did not start a save job");
        };
        wait_for_project_save(&mut app, &job_id);
        assert_eq!(dirty_revision, app.saved_revision);
        assert!(!app.document_is_dirty());
        let reopened = read_project(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(1, reopened.document.occurrence_refs().unwrap().len());
        assert_eq!(2.0, reopened.document.model_settings().grid_size);
        let saved_history = reopened.history.as_ref().unwrap();
        assert_eq!(3, saved_history.entries.len());
        assert_eq!(2, saved_history.index);
        assert!(
            app.status
                .contains("Imported undo snapshots were discarded")
        );

        let job_id = start_project_load(&mut app, &path);
        wait_for_project_load(&mut app, &job_id);
        assert_eq!(
            CommandCompletion::Completed,
            app.command_job_result(&job_id).unwrap().completion
        );
        let reloaded_camera = app.viewport.legacy_camera_state();
        assert_eq!(expected_camera["target"], reloaded_camera["target"]);
        assert_eq!(
            expected_camera["fieldOfView"],
            reloaded_camera["fieldOfView"]
        );
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_project_save_keeps_the_document_dirty_and_does_not_set_its_path() {
        let path = std::env::temp_dir()
            .join(format!("meshcraft-missing-parent-{}", std::process::id()))
            .join("project.octd");
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("path".to_owned(), path.display().to_string().into());
        let dispatch = app.dispatch_command("file.save_as", &payload);
        let CommandCompletion::Pending { job_id } = dispatch.result.completion else {
            panic!("Save As did not start a save job");
        };
        wait_for_project_save(&mut app, &job_id);
        assert_eq!(
            CommandCompletion::Failed,
            app.command_job_result(&job_id).unwrap().completion
        );
        assert!(app.document_is_dirty());
        assert_eq!(None, app.current_project_path());
        assert!(app.status.contains("Could not save"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn open_project_loads_a_valid_legacy_candidate() {
        let mut app = test_app();
        let job_id = start_project_load(&mut app, &project_fixture_path());
        assert!(matches!(
            app.command_job_result(&job_id).unwrap().completion,
            CommandCompletion::Pending { .. }
        ));

        wait_for_project_load(&mut app, &job_id);

        let result = app.command_job_result(&job_id).unwrap();
        assert_eq!(CommandCompletion::Completed, result.completion);
        assert_eq!(Some(app.document.revision()), result.document_revision);
        assert!(result.outputs["source_schema_version"].is_number());
        assert_eq!(
            Some(project_fixture_path().display().to_string()),
            app.current_project_path()
        );
        assert!(!app.document.occurrence_refs().unwrap().is_empty());
        assert_eq!("Select", app.active_tool);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_project_decode_keeps_the_active_document_unchanged() {
        let path = std::env::temp_dir().join(format!(
            "meshcraft-invalid-project-{}.octd",
            std::process::id()
        ));
        std::fs::write(&path, b"{").unwrap();
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.history.clear();
        app.saved_revision = app.document.revision();
        let before = app.document.clone();

        let job_id = start_project_load(&mut app, &path);
        wait_for_project_load(&mut app, &job_id);

        let result = app.command_job_result(&job_id).unwrap();
        assert_eq!(CommandCompletion::Failed, result.completion);
        assert_eq!(before, app.document);
        assert_eq!(None, app.current_project_path());
        assert!(app.status.contains("Could not open"));
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn load_candidate_is_not_published_after_a_newer_document_edit() {
        let mut app = test_app();
        let job_id = start_project_load(&mut app, &project_fixture_path());
        app.run_command("geometry.add_cuboid");
        let edited_document = app.document.clone();

        wait_for_project_load(&mut app, &job_id);

        let result = app.command_job_result(&job_id).unwrap();
        assert_eq!(CommandCompletion::Failed, result.completion);
        assert!(result.diagnostics[0].contains("revision changed"));
        assert_eq!(edited_document, app.document);
        assert_eq!(None, app.current_project_path());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cancelling_project_load_keeps_the_active_document_unchanged() {
        let mut app = test_app();
        let job_id = start_project_load(&mut app, &project_fixture_path());
        let cancellation = app.dispatch_command("file.cancel_open", &CommandPayload::default());
        assert!(cancellation.accepted);
        assert!(matches!(
            cancellation.result.host_effects.as_slice(),
            [meshcraft_core::commands::HostEffect::CancelJob { job_id: cancelled }]
                if cancelled == &job_id
        ));

        wait_for_project_load(&mut app, &job_id);

        assert_eq!(
            CommandCompletion::Failed,
            app.command_job_result(&job_id).unwrap().completion
        );
        assert!(app.document.occurrence_refs().unwrap().is_empty());
        assert_eq!(None, app.current_project_path());
        assert!(app.status.contains("cancelled"));
    }
}
