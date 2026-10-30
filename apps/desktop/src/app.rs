#[path = "annotation_workflow.rs"]
mod annotation_workflow;
#[path = "architecture_workflow.rs"]
mod architecture_workflow;
#[path = "clipboard_workflow.rs"]
mod clipboard_workflow;
#[path = "ifc_workflow.rs"]
pub(crate) mod ifc_workflow;
#[path = "interchange_workflow.rs"]
mod interchange_workflow;
#[path = "materials_workflow.rs"]
mod materials_workflow;
#[cfg(not(target_arch = "wasm32"))]
#[path = "mcp_workflow.rs"]
mod mcp_workflow;
#[path = "mesh_workflow.rs"]
mod mesh_workflow;
#[path = "numeric_workflow.rs"]
mod numeric_workflow;
#[path = "object_workflow.rs"]
mod object_workflow;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use object_workflow::BakeCaptureRequest;
#[path = "parameter_workflow.rs"]
pub(crate) mod parameter_workflow;
#[path = "placement_preview.rs"]
mod placement_preview;
#[path = "script_workflow.rs"]
mod script_workflow;
use crate::assets::{self, IconRegion};
use crate::viewport::{
    ConstructionGuide, ConstructionGuideKind, CuboidPreview, GridNormalAxis, GridStyle, OrthoView,
    SelectionGesture, SnapKind, SnapOptions, SnapResult, SnapScope, SnapVisibility, ToolFeedback,
    ToolPreview, Viewport, ViewportClick, ViewportScene, WalkthroughTuning,
};
use egui::{Align, Align2, Color32, Layout, Vec2};
use meshcraft_core::commands::HostEffect;
use meshcraft_core::commands::{
    CommandCapability, CommandCompletion, CommandDescriptor, CommandDispatch, CommandKind,
    CommandParameter, CommandPayload, CommandPresentation, CommandRegistry, CommandResult,
    CommandState, CommandStateQuery, CommandValueType,
};
use meshcraft_core::document::{
    DefinitionId, Document, LayoutEntryId, ModelSettings, OccurrenceRef, Rgba8, TriangleId,
    TriangleReplacement, VertexId,
};
use meshcraft_core::geometry::{Affine3, GeometryTolerance, Vec3};
use meshcraft_core::operations::copy as modeling_copy;
use meshcraft_core::operations::cuts::{
    CutSegment3, CutSourceTriangle, CutTool, cut_selected_faces as cut_selected_face_mesh,
};
use meshcraft_core::operations::drawing::{self, LineOwner};
use meshcraft_core::operations::extrude::{ExtrudeSourceTriangle, extrude_region};
use meshcraft_core::operations::objects as modeling_objects;
use meshcraft_core::operations::primitives as modeling_primitives;
use meshcraft_core::operations::revolve as modeling_revolve;
use meshcraft_core::operations::sweep::{self as modeling_sweep, SweepMesh, SweepProfileSegment};
use meshcraft_core::operations::transforms as modeling_transforms;
use meshcraft_core::selection::{
    CONNECTION_DISTANCE, EditContext, GeometryConnections, SelectedEntity, SelectionClickTracker,
    SelectionDisplayPolicy, SelectionKind, SelectionMode, SelectionSession, SelectionTarget,
};
use meshcraft_core::tools::{
    DrawingTool, DrawingToolState, ToolCycle, ToolCycleEffect, ToolCycleState, ToolEffect,
    ToolEvent,
    arrays::{
        ArrayToolKind, ArrayToolPick, ArrayToolState, RotationalArrayKind, RotationalArrayPick,
        RotationalArrayToolState,
    },
    object_place::{ObjectPlaceEffect, ObjectPlaceToolState, PlacementHit},
    paths::{
        FACE_OUTLINE_MERGE_TOLERANCE, LINE_OFFSET_MERGE_TOLERANCE, LineOffsetPick,
        LineOffsetToolState, MeshToolState, POLYLINE_MESH_MERGE_TOLERANCE, PolylinePath,
        PolylinePickEffect, PolylineSegmentMode, PolylineToolState,
        build_double_line_mesh_in_plane, build_face_outline_mesh,
        build_line_offset_mesh_from_segments, build_polyline_mesh,
        build_polyline_mesh_from_segments,
    },
    primitives::{PrimitiveKind, PrimitivePick, PrimitiveToolState},
    revolve::{RevolveInput, RevolvePick, RevolveToolState},
    shapes::{PlanarMesh, PlanarShapeTool, ShapeToolState},
    transforms::{TransformPick, TransformTool, TransformToolState},
};
use meshcraft_core::transactions::{
    HistoryEntryOrigin, OpaqueLegacyHistory, SystemHistoryClock, TransactionHistory,
};
#[cfg(not(target_arch = "wasm32"))]
use meshcraft_io::project::{
    DecodeProgress, encode_native_history, read_project_with, write_project,
};
use meshcraft_io::project::{ImportedProject, LegacySnapshotAdapter};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
const PREFERENCES_KEY: &str = "meshcraft.preferences";
const MAX_RECENT_PROJECTS: usize = 10;
#[cfg(not(target_arch = "wasm32"))]
const MAX_PROJECT_BYTES: u64 = 512 * 1024 * 1024;

fn push_pull_feedback_text(displacement: Vec3, units: &ModelSettings) -> String {
    let displacement = displacement * units.unit_size;
    let unit_name = if units.unit_name.trim().is_empty() {
        "units"
    } else {
        units.unit_name.trim()
    };
    format!(
        "Push/Pull: {:.3} {unit_name} | ΔX {:+.3}, ΔY {:+.3}, ΔZ {:+.3} {unit_name}",
        displacement.length(),
        displacement.x,
        displacement.y,
        displacement.z,
    )
}

fn point_feedback_text(tool: &str, point: Vec3, units: &ModelSettings) -> String {
    let point = point * units.unit_size;
    let unit_name = units.unit_name.trim();
    format!(
        "{tool}: X {:.3}, Y {:.3}, Z {:.3} {unit_name}",
        point.x, point.y, point.z
    )
}

fn vector_feedback_text(tool: &str, displacement: Vec3, units: &ModelSettings) -> String {
    let displacement = displacement * units.unit_size;
    let unit_name = units.unit_name.trim();
    format!(
        "{tool}: {:.3} {unit_name} | ΔX {:+.3}, ΔY {:+.3}, ΔZ {:+.3} {unit_name}",
        displacement.length(),
        displacement.x,
        displacement.y,
        displacement.z,
    )
}

fn primitive_feedback_text(
    kind: PrimitiveKind,
    picks: &[Vec3],
    point: Vec3,
    units: &ModelSettings,
) -> String {
    if kind == PrimitiveKind::Sphere {
        return picks.first().map_or_else(
            || point_feedback_text(kind.label(), point, units),
            |center| vector_feedback_text("Sphere radius", point - *center, units),
        );
    }
    let Some(base) = picks.first().copied() else {
        return point_feedback_text(kind.label(), point, units);
    };
    if picks.len() < 2 {
        return vector_feedback_text("Radius point", point - base, units);
    }
    let height = point - base;
    let radius_vector = picks[1] - base;
    let radius = height
        .try_normalized(1.0e-12)
        .map(|axis| (radius_vector - axis * radius_vector.dot(axis)).length());
    match radius.filter(|radius| radius.is_finite()) {
        Some(radius) => format!(
            "{} | radius {}",
            vector_feedback_text(&format!("{} height", kind.label()), height, units),
            units.format_length(radius)
        ),
        None => vector_feedback_text(&format!("{} height", kind.label()), height, units),
    }
}
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
    ("materials", "Materials"),
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
    ("mcp", "MCP Server"),
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
    ("arch_slab", "Rectangular Slab"),
    ("arch_stair", "Stair"),
    ("arch_add_hole", "Add Hole"),
    ("arch_window_frame", "Window"),
    ("arch_door_frame", "Door"),
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
    ("builtin_toolbar_solids", "Solids"),
    ("builtin_toolbar_arrays", "Arrays"),
    ("builtin_toolbar_random", "Random Tools"),
    ("builtin_toolbar_advanced_mesh", "Advanced Mesh"),
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
            "cut_with_plane",
            "revolve",
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
            "rotate_stretch",
            "paint",
        ],
    ),
    ("modification_2", &["rotate_2"]),
    (
        "solids",
        &[
            "solid_union",
            "solid_intersection",
            "solid_subtraction",
            "plane_section",
            "mesh_intersection",
        ],
    ),
    (
        "arrays",
        &[
            "copy_multiple",
            "planar_translate_multiple",
            "volumetric_translate_multiple",
            "planar_rotate_multiple",
            "helicoidal_rotate_multiple",
            "rotational_array",
            "helical_array",
            "rotational_array_equalized",
            "helical_array_equalized",
            "random_surface_array",
        ],
    ),
    ("random", &["random_offset"]),
    ("advanced_mesh", &["mesh_regularize", "surface_remesh"]),
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
    ("modification", &["tool.mirror", "geometry.flip_selection"]),
    ("modification_2", &["geometry.fill_loops"]),
    ("construction_points", &["hotspot.begin_placement"]),
    ("in_tool_operators", &["tool.select", "tool.distance"]),
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
            "object.explode_selection",
            "object.place_selected_definition",
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
    ("primitives", &["tool.cuboid"]),
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
        "Export> Export IFC (Model)",
        "Export",
        "export ifc model",
        Some("export"),
    ),
    (
        "import.mesh",
        "Import> Import Mesh…",
        "Import",
        "import mesh obj stl fbx gltf glb dae dxf 3mf amf ifc",
        Some("import"),
    ),
    (
        "export.mesh",
        "Export> Export Mesh…",
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
    CycleTool(ToolCycle),
    BeginHotspotPlacement,
    CancelHotspotPlacement,
    HotspotAction(&'static str),
    ObjectFileAction(&'static str),
    InterchangeAction(&'static str),
    MaterialAction(&'static str),
    FocusToolDistance,
    FinishToolInput,
    UndoToolPoint,
    ToggleTransformCopyMode,
    BeginCopyTransform(bool),
    PaintSelection,
    FaceOutline,
    PolylineMesh,
    FillLoops,
    MeshIntersection,
    SolidBoolean(meshcraft_core::operations::solids::Boolean),
    BeginSection,
    CutSelectedFaces(CutTool),
    SetPaintColor,
    CancelActiveTool,
    PlaceGuide(ConstructionGuideKind),
    RemoveGuide,
    AddCuboid,
    AnnotationSettings,
    ArchitectureAction(&'static str),
    FlipSelection,
    CopySelection,
    ClipboardAction(&'static str),
    DeleteSelection,
    ClearSelection,
    EnterEditContext,
    ExitEditContext,
    ResetSceneForCapture,
    SnapshotDocument,
    SetSelection,
    UpdateSelectionDisplay,
    CreateObject,
    ExplodeSelection,
    BakeObjects(bool),
    BeginObjectPlacement,
    ToggleObjectPlaceGlue,
    DeleteUnusedDefinition,
    DeleteDefinition,
    ArraySelection,
    UpdateModelSettings,
    UpdateUiPreferences,
    Undo,
    Redo,
    NewDocument,
    OpenProject,
    ShowRecentProjects,
    CancelProjectLoad,
    SaveProject,
    SaveProjectAs,
    ResetWorkspaceLayout,
    Quit,
    SetProfile(UiProfile),
    SetOrthographicView(OrthoView),
    SetWalkthroughView,
    SetPerspectiveView,
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
pub(crate) enum ReplacementDecision {
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
                "selection.update_display",
                "Selection> Update Display",
                "Selection",
                "visibility modifiable unlocked wireframe filter",
                None,
                ShellAction::UpdateSelectionDisplay,
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
                "edit.duplicate_selection",
                "Edit> Duplicate Selection",
                "Edit",
                "duplicate clone",
                Some("copy"),
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
                "selection.exit_context",
                "Selection> Exit Object Edit",
                "Selection",
                "close object instance editing return to parent context",
                None,
                ShellAction::ExitEditContext,
            ),
            (
                "selection.edit_object",
                "Selection> Edit Selected Object",
                "Selection",
                "enter instance prototype edit in place double click object",
                None,
                ShellAction::EnterEditContext,
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
                "Objects> Array Selection",
                "Objects",
                "repeat copies distribute selected mesh objects linear array",
                None,
                ShellAction::ArraySelection,
            ),
            (
                "object.explode_selection",
                "Objects> Explode Selection",
                "Objects",
                "explode ungroup materialize mesh",
                Some("group"),
                ShellAction::ExplodeSelection,
            ),
            (
                "object.place_selected_definition",
                "Objects> Place Selected Definition",
                "Objects",
                "place insert instance object prototype",
                Some("object_place"),
                ShellAction::BeginObjectPlacement,
            ),
            (
                "object.toggle_place_glue",
                "Objects> Toggle Glue to Surface",
                "Objects",
                "object place align surface normal glue",
                None,
                ShellAction::ToggleObjectPlaceGlue,
            ),
            (
                "object.delete_definition",
                "Objects> Delete Prototype",
                "Objects",
                "prototype remove delete definition all instances confirmation",
                Some("edit.delete_selection"),
                ShellAction::DeleteDefinition,
            ),
            (
                "object.delete_unused_definition",
                "Objects> Delete Unused Definition",
                "Objects",
                "prototype remove purge definition id",
                None,
                ShellAction::DeleteUnusedDefinition,
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
                "file.open_recent",
                "File> Open Recent Project…",
                "File",
                "recent files projects reopen",
                None,
                ShellAction::ShowRecentProjects,
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
                "tool.cycle.planar_construction",
                "Tools> Next Planar Construction Tool",
                "Tools",
                "cycle line construction line polyline double line rectangle mesh quad circle hotspot L Shift+L",
                None,
                ShellAction::CycleTool(ToolCycle::PlanarConstruction),
            ),
            (
                "tool.cycle.annotation",
                "Tools> Next Annotation Tool",
                "Tools",
                "cycle dimension text vector text D",
                None,
                ShellAction::CycleTool(ToolCycle::Annotation),
            ),
            (
                "tool.cycle.operations",
                "Tools> Next Operation Tool",
                "Tools",
                "cycle offset push pull extrude section O",
                None,
                ShellAction::CycleTool(ToolCycle::Operations),
            ),
            (
                "tool.cycle.modification",
                "Tools> Next Modification Tool",
                "Tools",
                "cycle move rotate scale stretch copy array M",
                None,
                ShellAction::CycleTool(ToolCycle::Modification),
            ),
            (
                "hotspot.begin_placement",
                "Tools> Hotspot Add at Cursor",
                "Tools",
                "place add create hotspot operator attached geometry",
                None,
                ShellAction::BeginHotspotPlacement,
            ),
            (
                "hotspot.cancel_placement",
                "Tools> Cancel Hotspot Placement",
                "Tools",
                "cancel stop hotspot placement",
                None,
                ShellAction::CancelHotspotPlacement,
            ),
            (
                "tool.distance",
                "Tool> Distance Input",
                "Tools",
                "line construction line numeric length distance operator revolve angle",
                Some("distance"),
                ShellAction::FocusToolDistance,
            ),
            ("tool.squash", "Modify> Squash", "Modify", "squash axis compress stretch signed scale", Some("squash"), ShellAction::SelectToolAlias("Squash")),
            ("tool.move_copy", "Modify> Move Copy", "Modify", "copy move duplicate geometry", Some("copy"), ShellAction::BeginCopyTransform(false)),
            ("tool.rotate_copy", "Modify> Rotate Copy", "Modify", "copy rotate duplicate geometry", Some("rotate-copy"), ShellAction::BeginCopyTransform(true)),
            (
                "tool.copy_mode",
                "Tool> Toggle Copy Mode",
                "Tools",
                "move rotate copy duplicate transform mode",
                None,
                ShellAction::ToggleTransformCopyMode,
            ),
            (
                "paint.apply_selection",
                "Paint> Apply Current Material to Selection",
                "Paint",
                "paint color rgba selection faces edges",
                None,
                ShellAction::PaintSelection,
            ),
            (
                "paint.set_color",
                "Paint> Choose Material",
                "Paint",
                "paint color rgba current picker",
                None,
                ShellAction::SetPaintColor,
            ),
            (
                "tool.finish",
                "Tool> Finish Current Input",
                "Tools",
                "line construction line polyline revolve sweep angle finish enter commit chain",
                Some("select"),
                ShellAction::FinishToolInput,
            ),
            (
                "tool.undo_point",
                "Tool> Undo Last Point or Input",
                "Tools",
                "line construction line polyline revolve axis angle backspace undo point numeric input",
                Some("undo"),
                ShellAction::UndoToolPoint,
            ),
            (
                "tool.cancel",
                "Tool> Cancel Current Tool",
                "Tools",
                "cancel escape current drawing operation",
                Some("select"),
                ShellAction::CancelActiveTool,
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
        register_shell_command(&mut registry,"tool.arch_slab_freeform","Tool> Freeform Slab","Tool","architecture slab closed polyline profile arc",Some("arch_slab"),ShellAction::SelectToolAlias("Freeform Slab"));
        for (action,label) in [
            ("create_stair","Create Stair from Contour and Walking Path"),("stair_contour","Use Selection as Stair Contour C"),("stair_path","Use Selection as Stair Walking Path P"),
            ("settings","Architecture Defaults"),("update","Update Selected Architecture"),
            ("create_wall","Create Wall"),("create_rectangular_slab","Create Rectangular Slab"),
            ("create_slab","Create Freeform Slab"),("create_frame","Create Independent Window or Door"),
            ("add_hole","Add Owned Wall or Slab Opening"),("remove_hole","Remove Owned Opening"),
            ("move_slab_vertex","Move Slab Profile Vertex"),("move_control","Move Architecture Control"),
        ] {
            register_shell_command(&mut registry,&format!("architecture.{action}"),&format!("Architecture> {label}"),"Architecture","wall slab opening window door profile settings",Some("arch_wall"),ShellAction::ArchitectureAction(action));
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
                "solid_union" => ShellAction::SolidBoolean(meshcraft_core::operations::solids::Boolean::Union),
                "solid_subtraction" => ShellAction::SolidBoolean(meshcraft_core::operations::solids::Boolean::Difference),
                "solid_intersection" => ShellAction::SolidBoolean(meshcraft_core::operations::solids::Boolean::Intersection),
                "plane_section" => ShellAction::BeginSection,
                "mesh_intersection" => ShellAction::MeshIntersection,
                "arch_stair" => ShellAction::SelectToolAlias("Stair"),
                "arch_wall" => ShellAction::SelectToolAlias("Wall"),
                "arch_slab" => ShellAction::SelectToolAlias("Rectangular Slab"),
                "arch_add_hole" => ShellAction::SelectToolAlias("Add Hole"),
                "arch_window_frame" => ShellAction::SelectToolAlias("Window"),
                "arch_door_frame" => ShellAction::SelectToolAlias("Door"),
                "select" => ShellAction::SelectToolAlias("Select"),
                "line" => ShellAction::SelectToolAlias("Line"),
                "construction_line" => ShellAction::SelectToolAlias("Construction Line"),
                "polyline" => ShellAction::SelectToolAlias("Polyline"),
                "double_line" => ShellAction::SelectToolAlias("Double Line"),
                "face_outline" => ShellAction::FaceOutline,
                "polyline_mesh" => ShellAction::PolylineMesh,
                "cut_holes" => ShellAction::CutSelectedFaces(CutTool::Holes),
                "cut_holes_2" => ShellAction::CutSelectedFaces(CutTool::Holes2),
                "cut_out_3" => ShellAction::CutSelectedFaces(CutTool::Cutout),
                "line_offset" => ShellAction::SelectToolAlias("Line Offset"),
                "extrude_swipe" => ShellAction::SelectToolAlias("Extrude Swipe"),
                "extrude_swipe_3d" => ShellAction::SelectToolAlias("Extrude Swipe 3D"),
                "ribbon_3d" => ShellAction::SelectToolAlias("Ribbon 3D"),
                "revolve" => ShellAction::SelectToolAlias("Revolve"),
                "mesh" => ShellAction::SelectToolAlias("Mesh"),
                "push_pull" => ShellAction::SelectToolAlias("Push/Pull"),
                "rectangle" => ShellAction::SelectToolAlias("Rectangle"),
                "surface_rectangle" => ShellAction::SelectToolAlias("Surface Rect"),
                "quad" => ShellAction::SelectToolAlias("Quad"),
                "circle" => ShellAction::SelectToolAlias("Circle"),
                "move" => ShellAction::SelectToolAlias("Move"),
                "rotate" => ShellAction::SelectToolAlias("Rotate"),
                "rotate_2" => ShellAction::SelectToolAlias("Rotate 2"),
                "scale" => ShellAction::SelectToolAlias("Scale"),
                "stretch" => ShellAction::SelectToolAlias("Stretch"),
                "stretch_scale" => ShellAction::SelectToolAlias("Stretch Scale"),
                "rotate_stretch" => ShellAction::SelectToolAlias("Rotate Stretch"),
                "paint" => ShellAction::SelectToolAlias("Paint"),
                "copy_multiple" => ShellAction::SelectToolAlias("Copy Multiple"),
                "planar_rotate_multiple" => {
                    ShellAction::SelectToolAlias("Planar Rotate Multiple")
                }
                "helicoidal_rotate_multiple" => {
                    ShellAction::SelectToolAlias("Helicoidal Rotate Multiple")
                }
                "rotational_array" => ShellAction::SelectToolAlias("Rotational Array"),
                "helical_array" => ShellAction::SelectToolAlias("Helical Array"),
                "rotational_array_equalized" => {
                    ShellAction::SelectToolAlias("Rotational Array Even")
                }
                "helical_array_equalized" => ShellAction::SelectToolAlias("Helical Array Even"),
                "planar_translate_multiple" => {
                    ShellAction::SelectToolAlias("Planar Translate Multiple")
                }
                "volumetric_translate_multiple" => {
                    ShellAction::SelectToolAlias("Volumetric Translate Multiple")
                }
                "primitive_sphere" => ShellAction::SelectToolAlias("Sphere"),
                "primitive_cylinder" => ShellAction::SelectToolAlias("Cylinder"),
                "primitive_cone" => ShellAction::SelectToolAlias("Cone"),
                "primitive_pill" => ShellAction::SelectToolAlias("Pill"),
                "linear_dimension" => ShellAction::SelectToolAlias("Linear Dimension"),
                "text" => ShellAction::SelectToolAlias("Text"),
                "object_place" => ShellAction::BeginObjectPlacement,
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
                "edit.ungroup" => ShellAction::ExplodeSelection,
                "edit.reset_scene_for_capture" => ShellAction::ResetSceneForCapture,
                "view.axis_guide" => ShellAction::PlaceGuide(ConstructionGuideKind::Axis),
                "view.grid_guide" => ShellAction::PlaceGuide(ConstructionGuideKind::Grid),
                "view.camera.orbit" => ShellAction::SelectToolAlias("Orbit"),
                "view.camera.walkthrough" => ShellAction::SetWalkthroughView,
                "view.camera.orthographic" => ShellAction::SetOrthographicView(OrthoView::Top),
                "import.mesh" => ShellAction::InterchangeAction("import"),
                "export.mesh" => ShellAction::InterchangeAction("export"),
                "export.ifc_model" => ShellAction::InterchangeAction("ifc"),
                _ => ShellAction::Unsupported,
            };
            register_shell_command(&mut registry, id, name, category, keywords, *icon, action);
        }
        register_shell_command(&mut registry, "interchange.cancel", "File> Cancel Import / Export", "File", "cancel stop mesh import export", Some("stop"), ShellAction::InterchangeAction("cancel"));
        register_shell_command(&mut registry, "interchange.formats", "File> Supported Mesh Formats", "File", "mesh formats capabilities units limitations", Some("import.mesh"), ShellAction::InterchangeAction("formats"));
        register_shell_command(&mut registry, "view.camera.perspective", "View> Camera Perspective", "View", "camera perspective orbit", Some("view"), ShellAction::SetPerspectiveView);
        for (id, label, action) in [("edit.cut_selection", "Cut", "cut"), ("edit.paste", "Paste", "paste"), ("edit.paste_in_place", "Paste in Place", "paste_in_place"),
            ("edit.copy_as_script", "Copy as Script", "script"), ("edit.copy_selection", "Copy", "copy")] {
            register_shell_command(&mut registry, id, &format!("Edit> {label}"), "Edit", "clipboard cut copy paste script", Some(if action.starts_with("paste") { "paste" } else { "copy" }), ShellAction::ClipboardAction(action));
        }
        register_shell_command(&mut registry,"tool.mirror","Modify> Mirror Selection","Modify","reflect selection plane origin normal",Some("mirror"),ShellAction::SelectToolAlias("Mirror"));
        register_shell_command(&mut registry,"geometry.fill_loops","Modify> Fill Selected Loops","Modify","surface faces directed segments closed loops regularize",Some("polyline_mesh"),ShellAction::FillLoops);
        register_shell_command(&mut registry, "geometry.flip_selection", "Modify> Flip Direction / Normals", "Modify", "reverse face normal segment direction winding", Some("mirror"), ShellAction::FlipSelection);
        register_shell_command(&mut registry, "tool.leader", "Dimension> Leader", "Dimension", "annotation leader arrow label", Some("leader"), ShellAction::SelectToolAlias("Leader"));
        register_shell_command(&mut registry, "annotation.update", "Dimension> Apply Annotation Properties", "Dimension", "label text size dimension leader", Some("text"), ShellAction::AnnotationSettings);
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
        for (action, label) in [("import_scene", "Import Scene as Prototype"), ("reference_scene", "Reference External Scene"), ("refresh_reference", "Refresh External Prototype")] {
            register_shell_command(&mut registry, &format!("object.{action}"), &format!("Objects> {label}"), "Objects", "external scene prototype reference xref import refresh", Some("import-objects"), ShellAction::ObjectFileAction(action));
        }
        for (suffix, label) in [
            ("add", "Add at Cursor"), ("update", "Update Properties"), ("attach", "Attach Selection"),
            ("select_attached", "Select Attached"), ("reference", "Pick Reference"),
            ("clear_reference", "Clear Reference"), ("delete", "Delete Hotspot"), ("target", "Operate Instance Handle"),
        ] {
            register_shell_command(&mut registry, &format!("hotspot.{suffix}"),
                &format!("Hotspots> {label}"), "Hotspots", "object prototype instance hotspot", Some("hotspot"), ShellAction::HotspotAction(suffix));
        }
        for (id, label, action) in [
            ("new_foliage","New Procedural Foliage Material",ShellAction::OpenPanel("material_editor_foliage")),
            ("new_water","New Procedural Water Material",ShellAction::OpenPanel("material_editor_water")),
            ("new_brick","New Procedural Brick Material",ShellAction::OpenPanel("material_editor_brick")),
            ("new_wood","New Procedural Wood Material",ShellAction::OpenPanel("material_editor_wood")),
            ("new_grass", "New Procedural Grass Material", ShellAction::OpenPanel("material_editor_grass")),
            ("new", "New Material", ShellAction::OpenPanel("material_editor_new")),
            ("new_shader", "New Shader", ShellAction::OpenPanel("material_program_new")),
            ("edit", "Edit Material", ShellAction::OpenPanel("material_editor")),
            ("apply", "Apply Material to Selection", ShellAction::MaterialAction("apply")),
            ("save", "Save Material", ShellAction::MaterialAction("save")),
            ("program", "Save Procedural Program", ShellAction::MaterialAction("program")),
            ("parameters", "Apply Procedural Parameters", ShellAction::MaterialAction("parameters")),
            ("delete", "Delete Unused Material", ShellAction::MaterialAction("delete")),
        ] {
            register_shell_command(&mut registry, &format!("material.{id}"), &format!("Objects> Materials> {label}"), "Materials", "color transparency texture shading material", Some("paint"), action);
        }
        for (id, label, to_shader) in [("object.bake_to_shader", "Bake to Shader", true), ("object.bake_to_mesh", "Bake to Mesh", false)] {
            register_shell_command(&mut registry,id,&format!("Objects> {label}"),"Objects","bake freeze convert gpu shader editable mesh",Some("object"),ShellAction::BakeObjects(to_shader));
        }
        register_shell_command(&mut registry,"object.configure_procedural","Objects> Configure Procedural Object","Objects","tree gpu parameters prototype instance",Some("object"),ShellAction::MaterialAction("object"));
        for (id,name,panel) in [("object.new_tree","Objects> New Procedural Tree","tree_new"),("object.edit_tree","Objects> Edit Procedural Tree","tree_edit"),("script.new","Objects> New Scripted Object","script_new"),("script.edit","Objects> Edit Prototype Script","script_edit")] {
            register_shell_command(&mut registry,id,name,"Objects","javascript script prototype editor preview",Some("object"),ShellAction::OpenPanel(panel));
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
        | ShellAction::BeginCopyTransform(_)
        | ShellAction::CycleTool(_)
        | ShellAction::BeginObjectPlacement
        | ShellAction::BeginSection
        | ShellAction::RemoveGuide => CommandKind::ToolActivation,
        ShellAction::HotspotAction(_)
        | ShellAction::MaterialAction(_)
        | ShellAction::AddCuboid
        | ShellAction::AnnotationSettings
        | ShellAction::ArchitectureAction(_)
        | ShellAction::FlipSelection
        | ShellAction::CopySelection
        | ShellAction::ClipboardAction(_)
        | ShellAction::DeleteSelection
        | ShellAction::FaceOutline
        | ShellAction::PolylineMesh
        | ShellAction::FillLoops
        | ShellAction::MeshIntersection
        | ShellAction::SolidBoolean(_)
        | ShellAction::CutSelectedFaces(_)
        | ShellAction::PaintSelection
        | ShellAction::ResetSceneForCapture
        | ShellAction::CreateObject
        | ShellAction::ExplodeSelection
        | ShellAction::DeleteUnusedDefinition
        | ShellAction::DeleteDefinition
        | ShellAction::ArraySelection
        | ShellAction::UpdateModelSettings
        | ShellAction::Undo
        | ShellAction::Redo
        | ShellAction::NewDocument
        | ShellAction::FinishToolInput => CommandKind::DocumentMutation,
        ShellAction::InterchangeAction("formats") => CommandKind::ReadOnly,
        ShellAction::InterchangeAction(_)
        | ShellAction::ObjectFileAction(_)
        | ShellAction::OpenProject
        | ShellAction::CancelProjectLoad
        | ShellAction::SaveProject
        | ShellAction::SaveProjectAs
        | ShellAction::Quit
        | ShellAction::Unsupported => CommandKind::HostRequest,
        ShellAction::SetProfile(_)
        | ShellAction::SetOrthographicView(_)
        | ShellAction::SetPerspectiveView
        | ShellAction::SetWalkthroughView
        | ShellAction::UpdateUiPreferences
        | ShellAction::FrameDocument
        | ShellAction::ResetCamera
        | ShellAction::SetCameraPose
        | ShellAction::ClearSelection
        | ShellAction::EnterEditContext
        | ShellAction::ExitEditContext
        | ShellAction::TogglePanelSection(_)
        | ShellAction::ToggleToolbars
        | ShellAction::SetToolbarVisibility(_, _)
        | ShellAction::FocusToolDistance
        | ShellAction::ToggleTransformCopyMode
        | ShellAction::ToggleObjectPlaceGlue
        | ShellAction::SetPaintColor
        | ShellAction::UndoToolPoint
        | ShellAction::CancelActiveTool
        | ShellAction::BeginHotspotPlacement
        | ShellAction::CancelHotspotPlacement
        | ShellAction::TogglePanels
        | ShellAction::ToggleGrid
        | ShellAction::ToggleGuides
        | ShellAction::ClearGuides
        | ShellAction::PlaceGuide(_)
        | ShellAction::ShowRecentProjects
        | ShellAction::ResetWorkspaceLayout
        | ShellAction::OpenPanel(_)
        | ShellAction::OpenPanelGroup(_) => CommandKind::SessionMutation,
        ShellAction::SetSelection | ShellAction::UpdateSelectionDisplay => {
            CommandKind::SessionMutation
        }
        ShellAction::BakeObjects(_) => CommandKind::DocumentMutation,
        ShellAction::SnapshotDocument | ShellAction::About => CommandKind::ReadOnly,
    }
}

fn action_state_query(action: ShellAction) -> CommandStateQuery {
    match action {
        ShellAction::Undo => CommandStateQuery::UndoAvailable,
        ShellAction::Redo => CommandStateQuery::RedoAvailable,
        ShellAction::CopySelection | ShellAction::BeginCopyTransform(_) => {
            CommandStateQuery::SelectionAvailable
        }
        ShellAction::FaceOutline => CommandStateQuery::SelectionAvailable,
        ShellAction::PolylineMesh
        | ShellAction::FillLoops
        | ShellAction::SolidBoolean(_)
        | ShellAction::MeshIntersection
        | ShellAction::BeginSection => CommandStateQuery::SelectionAvailable,
        ShellAction::CutSelectedFaces(_) => CommandStateQuery::SelectionAvailable,
        ShellAction::DeleteSelection => CommandStateQuery::DeleteSelectionAvailable,
        ShellAction::ClearSelection => CommandStateQuery::SelectionAvailable,
        ShellAction::CreateObject
        | ShellAction::ExplodeSelection
        | ShellAction::BakeObjects(_)
        | ShellAction::BeginObjectPlacement => CommandStateQuery::SelectionAvailable,
        ShellAction::ToggleObjectPlaceGlue => {
            CommandStateQuery::ActiveTool("Object Place".to_owned())
        }
        ShellAction::ArraySelection => CommandStateQuery::SelectionAvailable,
        ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
            CommandStateQuery::ActiveTool(tool.to_owned())
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

fn command_menu_paths(command: &CommandDescriptor) -> impl Iterator<Item = &[String]> {
    std::iter::once(command.presentation.menu_path.as_slice())
        .chain(command.presentation.menu_aliases.iter().map(Vec::as_slice))
        .filter(|path| !path.is_empty())
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
    if id == "tool.distance" {
        descriptor.shortcut = Some("Ctrl+N".to_owned());
    }
    if let ShellAction::CycleTool(cycle) = action {
        descriptor.shortcut = Some(
            match cycle {
                ToolCycle::PlanarConstruction => "Shift+L",
                ToolCycle::Annotation => "D",
                ToolCycle::Operations => "O",
                ToolCycle::Modification => "M",
            }
            .to_owned(),
        );
    }
    if let Some(shortcut) = match id {
        "edit.copy_selection" => Some("Ctrl+C"),
        "edit.cut_selection" => Some("Ctrl+X"),
        "edit.paste" => Some("Ctrl+V"),
        "edit.paste_in_place" => Some("Ctrl+Shift+V"),
        "edit.copy_as_script" => Some("Ctrl+Shift+C"),
        _ => None,
    } {
        descriptor.shortcut = Some(shortcut.into());
    }
    if id == "edit.delete_selection" {
        descriptor.shortcut = Some("Delete".to_owned());
    }
    descriptor.capability = match action {
        ShellAction::SelectTool(_)
        | ShellAction::SelectToolAlias(_)
        | ShellAction::CycleTool(_)
        | ShellAction::BeginObjectPlacement
        | ShellAction::BeginSection
        | ShellAction::ToggleObjectPlaceGlue
        | ShellAction::BeginHotspotPlacement
        | ShellAction::CancelHotspotPlacement
        | ShellAction::ObjectFileAction(_)
        | ShellAction::InterchangeAction(_)
        | ShellAction::PlaceGuide(_)
        | ShellAction::RemoveGuide
        | ShellAction::SetProfile(_)
        | ShellAction::SetOrthographicView(_)
        | ShellAction::SetPerspectiveView
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
        | ShellAction::FocusToolDistance
        | ShellAction::ToggleTransformCopyMode
        | ShellAction::SetPaintColor
        | ShellAction::FinishToolInput
        | ShellAction::UndoToolPoint
        | ShellAction::CancelActiveTool
        | ShellAction::TogglePanelSection(_)
        | ShellAction::OpenProject
        | ShellAction::ShowRecentProjects
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
        menu_path: vec![],
        menu_aliases: vec![],
        toolbar_group: None,
        icon_key: icon.map(str::to_owned),
        tutorial_action_id: None,
    };
    crate::functional_ui::present(&mut descriptor);
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
        "selection.update_display" => &[
            ("kind", CommandValueType::String),
            ("property", CommandValueType::String),
            ("enabled", CommandValueType::Boolean),
        ],
        "paint.set_color" => &[
            ("red", CommandValueType::Integer),
            ("green", CommandValueType::Integer),
            ("blue", CommandValueType::Integer),
            ("alpha", CommandValueType::Integer),
        ],
        "object.array_selection" => &[
            ("count", CommandValueType::Integer),
            ("spacing", CommandValueType::Number),
        ],
        "material.apply" | "material.delete" => &[("id", CommandValueType::String)],
        "material.program" => &[("program", CommandValueType::Object)],
        "material.parameters" => &[
            ("id", CommandValueType::String),
            ("parameters", CommandValueType::Object),
        ],
        "object.configure_procedural" => &[
            ("definition_id", CommandValueType::Integer),
            ("instance_path", CommandValueType::Array),
            ("settings", CommandValueType::Object),
        ],
        "material.save" => &[("material", CommandValueType::Object)],
        "hotspot.add" => &[("position", CommandValueType::Object)],
        "hotspot.update" => &[
            ("name", CommandValueType::String),
            ("operation", CommandValueType::String),
            ("shape", CommandValueType::String),
            ("color", CommandValueType::String),
        ],
        "hotspot.target" => &[("target", CommandValueType::Object)],
        "object.import_scene" | "object.reference_scene" => &[("path", CommandValueType::String)],
        "object.place_selected_definition" => &[("definition_id", CommandValueType::Integer)],
        "object.create_from_selection" => &[("name", CommandValueType::String)],
        "object.delete_unused_definition" => &[("definition_id", CommandValueType::Integer)],
        "object.delete_definition" => &[
            ("definition_id", CommandValueType::Integer),
            ("confirmed", CommandValueType::Boolean),
        ],
        "model.update_settings" => &[
            ("unit_name", CommandValueType::String),
            ("unit_size", CommandValueType::Number),
            ("grid_size", CommandValueType::Number),
            ("circle_segments", CommandValueType::Integer),
            ("snap_radius", CommandValueType::Number),
            ("arc_max_length", CommandValueType::Number),
            ("double_line_size", CommandValueType::Number),
            ("double_line_offset", CommandValueType::Number),
        ],
        "ui.update_preferences" => &[
            ("menu_bar_visible", CommandValueType::Boolean),
            ("menu_visibility", CommandValueType::Object),
            ("ribbon_visible", CommandValueType::Boolean),
            ("left_panel_visible", CommandValueType::Boolean),
            ("left_panel_width", CommandValueType::Number),
            ("ui_text_scale", CommandValueType::Number),
            ("auto_collapse_toolbars", CommandValueType::Boolean),
            ("accordion_settings_panels", CommandValueType::Boolean),
            ("antialiasing_quality", CommandValueType::Integer),
            ("black_model_lines", CommandValueType::Boolean),
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
            ("toolbar_positions", CommandValueType::Object),
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
        id if id.starts_with("architecture.") => &[
            ("contour", CommandValueType::Array),
            ("walking_path", CommandValueType::Array),
            ("stair_height", CommandValueType::Number),
            ("stair_steps", CommandValueType::Integer),
            ("stair_support", CommandValueType::Number),
            ("stair_left", CommandValueType::Boolean),
            ("stair_right", CommandValueType::Boolean),
            ("stair_spacing", CommandValueType::Number),
            ("stair_levels", CommandValueType::Integer),
            ("stair_tread_material", CommandValueType::String),
            ("stair_support_material", CommandValueType::String),
            ("id", CommandValueType::String),
            ("occurrence", CommandValueType::Object),
            ("start", CommandValueType::Array),
            ("end", CommandValueType::Array),
            ("points", CommandValueType::Array),
            ("normal", CommandValueType::Array),
            ("kind", CommandValueType::String),
            ("name", CommandValueType::String),
            ("wall_thickness", CommandValueType::Number),
            ("wall_height", CommandValueType::Number),
            ("wall_end_height", CommandValueType::Number),
            ("wall_offset", CommandValueType::Number),
            ("wall_auto_join", CommandValueType::Boolean),
            ("control", CommandValueType::Object),
            ("wall_inclination", CommandValueType::Number),
            ("slab_thickness", CommandValueType::Number),
            ("frame_depth", CommandValueType::Number),
            ("frame_width", CommandValueType::Number),
            ("glazing_enabled", CommandValueType::Boolean),
            ("record", CommandValueType::Object),
            ("wall_exterior_material", CommandValueType::String),
            ("wall_interior_material", CommandValueType::String),
            ("slab_top_material", CommandValueType::String),
            ("slab_bottom_material", CommandValueType::String),
            ("slab_side_material", CommandValueType::String),
            ("frame_material", CommandValueType::String),
            ("glazing_material", CommandValueType::String),
            ("wall_exterior_color", CommandValueType::Object),
            ("wall_interior_color", CommandValueType::Object),
            ("slab_top_color", CommandValueType::Object),
            ("slab_bottom_color", CommandValueType::Object),
            ("slab_side_color", CommandValueType::Object),
            ("frame_color", CommandValueType::Object),
            ("glazing_color", CommandValueType::Object),
            ("hole_id", CommandValueType::String),
            ("index", CommandValueType::Integer),
            ("position", CommandValueType::Array),
        ],
        "annotation.update" => &[
            ("text", CommandValueType::String),
            ("size", CommandValueType::Number),
        ],
        "edit.paste" => &[("clipboard", CommandValueType::String)],
        "edit.paste_in_place" => &[
            ("clipboard", CommandValueType::String),
            ("offset", CommandValueType::Array),
        ],
        "import.mesh" => &[
            ("path", CommandValueType::String),
            ("unit", CommandValueType::String),
            ("scale", CommandValueType::Number),
            ("position", CommandValueType::Array),
        ],
        "export.mesh" | "export.ifc_model" => &[
            ("path", CommandValueType::String),
            ("unit", CommandValueType::String),
            ("scale", CommandValueType::Number),
            ("selection_only", CommandValueType::Boolean),
        ],
        "file.open" | "file.save" | "file.save_as" => &[("path", CommandValueType::String)],
        _ => &[],
    };
    parameters
        .iter()
        .map(|(name, value_type)| CommandParameter {
            name: (*name).to_owned(),
            value_type: *value_type,
            required: (id == "material.program" && *name == "program")
                || (id == "material.parameters" && matches!(*name, "id" | "parameters"))
                || (id == "selection.set" && *name == "paths")
                || (id == "selection.update_display"
                    && matches!(*name, "kind" | "property" | "enabled"))
                || (matches!(
                    id,
                    "object.delete_unused_definition" | "object.delete_definition"
                ) && *name == "definition_id")
                || (id == "paint.set_color" && matches!(*name, "red" | "green" | "blue" | "alpha"))
                || (id == "view.camera.set_pose" && matches!(*name, "position" | "target")),
        })
        .collect()
}

fn selection_kind_from_name(name: &str) -> Option<SelectionKind> {
    Some(match name {
        "vertex" => SelectionKind::Vertex,
        "edge" => SelectionKind::Edge,
        "face" => SelectionKind::Face,
        "object" => SelectionKind::Object,
        "voxel" => SelectionKind::Voxel,
        "hotspot" => SelectionKind::Hotspot,
        "dimension" => SelectionKind::Dimension,
        "text" => SelectionKind::Text,
        "wall" => SelectionKind::Wall,
        "slab" => SelectionKind::Slab,
        "stair" => SelectionKind::Stair,
        "frame" => SelectionKind::Frame,
        "hvac" => SelectionKind::Hvac,
        _ => return None,
    })
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
    if stem == "voxel" {
        ids.push("edit.voxelize_faces".to_owned());
    }
    ids
}

pub(crate) const DROPDOWN_MENU_NAMES: &[&str] = crate::functional_ui::GROUPS;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
struct Preferences {
    profile: UiProfile,
    menu_bar_visible: bool,
    menu_visibility: BTreeMap<String, bool>,
    ribbon_visible: bool,
    left_panel_visible: bool,
    left_panel_width: f32,
    toolbars_visible: bool,
    toolbar_visibility: BTreeMap<String, bool>,
    toolbar_positions: BTreeMap<String, [f32; 2]>,
    auto_collapse_toolbars: bool,
    accordion_settings_panels: bool,
    antialiasing_quality: u32,
    black_model_lines: bool,
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
    recent_projects: Vec<String>,
    mcp_port: u16,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeUiPreferences {
    pub menu_bar_visible: bool,
    pub menu_visibility: BTreeMap<String, bool>,
    pub ribbon_visible: bool,
    pub left_panel_visible: bool,
    pub left_panel_width: f32,
    pub ui_text_scale: f32,
    pub auto_collapse_toolbars: bool,
    pub accordion_settings_panels: bool,
    pub antialiasing_quality: u32,
    pub black_model_lines: bool,
    pub toolbar_icon_size: u32,
    pub show_permanent_grid: bool,
    pub show_construction_guides: bool,
    pub grid_normal_axis: GridNormalAxis,
    pub grid_extent: u32,
    pub normal_grid_width: f32,
    pub thick_grid_width: f32,
    pub toolbar_flow_top_down: bool,
    pub toolbar_positions: Vec<Option<[f32; 2]>>,
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
            menu_bar_visible: true,
            menu_visibility: BTreeMap::new(),
            ribbon_visible: true,
            left_panel_visible: true,
            left_panel_width: 230.,
            toolbars_visible: true,
            toolbar_visibility: LEGACY_TOOLBARS
                .iter()
                .filter_map(|(id, _)| {
                    id.strip_prefix("builtin_toolbar_")
                        .map(|stem| (stem.to_owned(), true))
                })
                .collect(),
            toolbar_positions: BTreeMap::new(),
            auto_collapse_toolbars: false,
            accordion_settings_panels: false,
            antialiasing_quality: 2,
            black_model_lines: true,
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
            recent_projects: Vec::new(),
            mcp_port: 8765,
        }
    }
}

impl Preferences {
    fn normalize_recent_projects(&mut self) {
        let mut seen = BTreeSet::new();
        self.recent_projects
            .retain(|path| !path.is_empty() && seen.insert(path.clone()));
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn record_recent_project(&mut self, path: String) {
        self.recent_projects.retain(|existing| existing != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SelectionQueryKey {
    generation: u64,
    visual_revision: u64,
    selection_revision: u64,
    context: EditContext,
}
struct CopyAvailability {
    key: SelectionQueryKey,
    available: bool,
}
struct TransformWireframe {
    key: SelectionQueryKey,
    edges: Vec<[Vec3; 2]>,
}

pub struct MeshcraftApp {
    preferences: Preferences,
    #[cfg(not(target_arch = "wasm32"))]
    persist_native_preferences: bool,
    icon_atlas: Option<egui::TextureHandle>,
    icon_regions: BTreeMap<&'static str, IconRegion>,
    active_tool: String,
    status: String,
    pointer_in_viewport: Option<egui::Vec2>,
    cursor_snap: Option<SnapResult>,
    viewport_rect: egui::Rect,
    open_file_menu: bool,
    recent_projects_dialog_open: bool,
    command_palette_open: bool,
    command_search: String,
    command_selection: usize,
    focus_command_search: bool,
    quit_requested: bool,
    panel_focus: Option<String>,
    document: Document,
    document_generation: u64,
    annotations: annotation_workflow::AnnotationState,
    architecture: architecture_workflow::State,
    section: mesh_workflow::SectionState,
    clipboard: clipboard_workflow::ClipboardState,
    #[cfg(not(target_arch = "wasm32"))]
    mcp_jobs: mcp_workflow::Jobs,
    saved_revision: u64,
    #[cfg(not(target_arch = "wasm32"))]
    autosave_observed_revision: u64,
    #[cfg(not(target_arch = "wasm32"))]
    autosave_deadline: Option<Instant>,
    history: TransactionHistory<SystemHistoryClock>,
    legacy_history: Option<OpaqueLegacyHistory>,
    legacy_active_index: Option<usize>,
    #[cfg(not(target_arch = "wasm32"))]
    current_project_path: Option<PathBuf>,
    next_job_id: u64,
    job_results: BTreeMap<String, CommandResult>,
    pending_replacement: Option<PendingReplacement>,
    pending_prototype_deletion: Option<object_workflow::PrototypeDeletionPrompt>,
    #[cfg(not(target_arch = "wasm32"))]
    replacement_after_save: Option<PendingReplacement>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_project_load: Option<PendingProjectLoad>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_project_save: Option<PendingProjectSave>,
    selection: SelectionSession,
    copy_availability: std::cell::RefCell<Option<CopyAvailability>>,
    transform_wireframe: Option<TransformWireframe>,
    selection_display_policy: SelectionDisplayPolicy,
    selection_click_tracker: SelectionClickTracker,
    #[cfg(not(target_arch = "wasm32"))]
    native_spatial_context: Option<(u64, EditContext)>,
    // Generation/context live in native_spatial_context; the tuple tracks evaluated geometry too.
    #[cfg(not(target_arch = "wasm32"))]
    native_edit_view: Option<(u64, Arc<Document>)>,
    object_name_edit: Option<(DefinitionId, String)>,
    object_browser_selection: Option<DefinitionId>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_object_import: Option<object_workflow::ObjectImportJob>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_object_bake: Option<object_workflow::ObjectBakeJob>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_interchange: Option<interchange_workflow::InterchangeJob>,
    hotspot_inspector: Option<(OccurrenceRef, String)>,
    hotspot_defaults: meshcraft_core::document::hotspots::Hotspot,
    hotspot_reference_pick: bool,
    viewport: Viewport,
    guides: Vec<ConstructionGuide>,
    cuboid_first: Option<Vec3>,
    drawing_tool: DrawingToolState,
    polyline_tool_state: PolylineToolState,
    mesh_tool_state: MeshToolState,
    line_offset_tool_state: LineOffsetToolState,
    push_pull_reference: Option<Vec3>,
    push_pull_pointer: Option<Vec3>,
    push_pull_faces: Vec<(TriangleId, ExtrudeSourceTriangle)>,
    push_pull_owner: Option<LineOwner>,
    push_pull_occurrence: Option<OccurrenceRef>,
    push_pull_distance_focused: bool,
    push_pull_numeric_input: String,
    numeric_entry: numeric_workflow::State,
    tool_cycle_state: ToolCycleState,
    primitive_tool_state: PrimitiveToolState,
    revolve_tool_state: RevolveToolState,
    shape_tool_state: ShapeToolState,
    transform_tool_state: TransformToolState,
    object_place_tool_state: ObjectPlaceToolState,
    placement_preview: placement_preview::State,
    array_tool_state: ArrayToolState,
    rotational_array_tool_state: RotationalArrayToolState,
    current_paint_color: Rgba8,
    current_paint_material: Option<String>,
    array_count: usize,
    array_spacing: f64,
    array_preview: bool,
    pending_selection_volume: Option<Vec3>,
}

impl MeshcraftApp {
    pub fn command_catalog() -> Vec<CommandDescriptor> {
        shell_commands()
            .entries()
            .map(|command| command.descriptor.clone())
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_toolbar_catalog() -> Vec<(String, Vec<CommandDescriptor>)> {
        let catalog = Self::command_catalog();
        crate::functional_ui::GROUPS
            .iter()
            .map(|root| {
                let commands = crate::functional_ui::sections(root)
                    .flat_map(|s| s.commands.iter())
                    .filter_map(|(id, _)| catalog.iter().find(|c| c.id.as_str() == *id).cloned())
                    .collect();
                ((*root).to_owned(), commands)
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_toolbar_visibility(&self) -> Vec<bool> {
        crate::functional_ui::GROUPS
            .iter()
            .map(|root| {
                self.preferences.toolbars_visible
                    && self
                        .preferences
                        .toolbar_visibility
                        .get(crate::functional_ui::toolbar_key(root))
                        .copied()
                        .unwrap_or(true)
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_ui_preferences(&self) -> NativeUiPreferences {
        let mut menu_visibility = self.preferences.menu_visibility.clone();
        if let Some(visible) = menu_visibility.remove("2D Construction") {
            menu_visibility
                .entry("Construction".into())
                .or_insert(visible);
        }
        NativeUiPreferences {
            menu_bar_visible: self.preferences.menu_bar_visible,
            menu_visibility,
            ribbon_visible: self.preferences.ribbon_visible,
            left_panel_visible: self.preferences.left_panel_visible,
            left_panel_width: self.preferences.left_panel_width,
            ui_text_scale: self.preferences.ui_text_scale,
            auto_collapse_toolbars: self.preferences.auto_collapse_toolbars,
            accordion_settings_panels: self.preferences.accordion_settings_panels,
            antialiasing_quality: self.preferences.antialiasing_quality,
            black_model_lines: self.preferences.black_model_lines,
            toolbar_icon_size: self.preferences.toolbar_icon_size,
            show_permanent_grid: self.preferences.show_permanent_grid,
            show_construction_guides: self.preferences.show_construction_guides,
            grid_normal_axis: self.preferences.grid_normal_axis,
            grid_extent: self.preferences.grid_extent,
            normal_grid_width: self.preferences.normal_grid_width,
            thick_grid_width: self.preferences.thick_grid_width,
            toolbar_flow_top_down: self.preferences.toolbar_flow_top_down,
            toolbar_positions: LEGACY_TOOLBARS
                .iter()
                .map(|(toolbar_id, _)| {
                    toolbar_id
                        .strip_prefix("builtin_toolbar_")
                        .and_then(|stem| self.preferences.toolbar_positions.get(stem))
                        .copied()
                })
                .collect(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_ui_preferences_payload(&self) -> CommandPayload {
        let preferences = &self.preferences;
        let mut payload = CommandPayload::default();
        payload.values.extend([
            (
                "menu_bar_visible".into(),
                json!(preferences.menu_bar_visible),
            ),
            ("menu_visibility".into(), json!(preferences.menu_visibility)),
            ("ribbon_visible".into(), json!(preferences.ribbon_visible)),
            (
                "black_model_lines".into(),
                json!(preferences.black_model_lines),
            ),
            (
                "left_panel_visible".into(),
                json!(preferences.left_panel_visible),
            ),
            (
                "left_panel_width".into(),
                json!(preferences.left_panel_width),
            ),
            ("ui_text_scale".to_owned(), json!(preferences.ui_text_scale)),
            (
                "accordion_settings_panels".to_owned(),
                json!(preferences.accordion_settings_panels),
            ),
            (
                "antialiasing_quality".to_owned(),
                json!(preferences.antialiasing_quality),
            ),
            (
                "auto_collapse_toolbars".to_owned(),
                json!(preferences.auto_collapse_toolbars),
            ),
            (
                "toolbar_icon_size".to_owned(),
                json!(preferences.toolbar_icon_size),
            ),
            (
                "show_permanent_grid".to_owned(),
                json!(preferences.show_permanent_grid),
            ),
            (
                "show_construction_guides".to_owned(),
                json!(preferences.show_construction_guides),
            ),
            (
                "grid_normal_axis".to_owned(),
                json!(match preferences.grid_normal_axis {
                    GridNormalAxis::X => "x",
                    GridNormalAxis::Y => "y",
                    GridNormalAxis::Z => "z",
                }),
            ),
            ("grid_extent".to_owned(), json!(preferences.grid_extent)),
            (
                "normal_grid_width".to_owned(),
                json!(preferences.normal_grid_width),
            ),
            (
                "thick_grid_width".to_owned(),
                json!(preferences.thick_grid_width),
            ),
            (
                "toolbar_flow_top_down".to_owned(),
                json!(preferences.toolbar_flow_top_down),
            ),
            (
                "toolbars_visible".to_owned(),
                json!(preferences.toolbars_visible),
            ),
            (
                "toolbar_visibility".to_owned(),
                json!(preferences.toolbar_visibility),
            ),
            (
                "toolbar_positions".to_owned(),
                json!(preferences.toolbar_positions),
            ),
            (
                "walk_move_speed".to_owned(),
                json!(preferences.walk_move_speed),
            ),
            (
                "walk_jump_velocity".to_owned(),
                json!(preferences.walk_jump_velocity),
            ),
            ("walk_gravity".to_owned(), json!(preferences.walk_gravity)),
            (
                "walk_height_adjust_speed".to_owned(),
                json!(preferences.walk_height_adjust_speed),
            ),
        ]);
        payload
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_panels_visible(&self) -> bool {
        self.preferences.panels_visible
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_command_state(&self, id: &str) -> CommandState {
        self.query_command_state(id)
    }

    pub fn command_job_result(&self, job_id: &str) -> Option<&CommandResult> {
        self.job_results.get(job_id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_native_session(
        document: Document,
        status: String,
        legacy_history: Option<OpaqueLegacyHistory>,
    ) -> Self {
        #[cfg(not(test))]
        let mut preferences = load_native_preferences();
        #[cfg(test)]
        let mut preferences = Preferences::default();
        preferences.normalize_recent_projects();
        let session = Self::from_parts(preferences, None, document, status, true, legacy_history);
        #[cfg(not(test))]
        {
            let mut session = session;
            session.persist_native_preferences = true;
            session
        }
        #[cfg(test)]
        {
            session
        }
    }

    pub fn document_snapshot(&self) -> Document {
        self.document.clone()
    }

    pub fn native_document_for_view(&self) -> Document {
        match &self.selection.context {
            EditContext::Occurrence(active) => self.document.prototype_edit_view(active),
            _ => self.document.clone(),
        }
    }

    /// Share the same prototype view between rendering and native spatial queries.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_view_snapshot(&mut self) -> Arc<Document> {
        self.native_prepare_spatial_view();
        self.native_edit_view.as_ref().map_or_else(
            || Arc::new(self.document.clone()),
            |(_, view)| Arc::clone(view),
        )
    }

    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }

    pub fn document_generation(&self) -> u64 {
        self.document_generation
    }

    /// Polls asynchronous project operations and autosave work for hosts that do not use egui.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn poll_native_host_work(&mut self) {
        self.poll_mcp_script();
        self.poll_project_load();
        self.poll_project_save();
        self.poll_object_import();
        self.poll_object_bake();
        self.poll_interchange();
        let _ = self.poll_autosave(Instant::now());
    }

    pub fn native_take_panel_focus(&mut self) -> Option<String> {
        self.panel_focus.take()
    }

    pub fn set_native_status(&mut self, text: String) {
        self.status = text;
    }

    pub fn status_text(&self) -> &str {
        &self.status
    }

    pub fn active_tool_label(&self) -> &str {
        &self.active_tool
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn handle_native_selection_click(
        &mut self,
        target: Option<SelectionTarget>,
        modifiers: egui::Modifiers,
        at_millis: u64,
        screen_position: meshcraft_core::geometry::Vec2,
    ) {
        if self.active_tool != "Select" {
            self.selection_click_tracker.reset();
            return;
        }
        let target = target
            .filter(|target| self.native_target_editable(target))
            .map(|target| self.native_normalize_selection_target(target));
        let click_count = self
            .selection_click_tracker
            .record(at_millis, screen_position);
        if click_count >= 3
            && let Some(target) = target.as_ref()
        {
            let connected = self
                .connected_selection_component(target)
                .into_iter()
                .filter(|target| self.native_target_editable(target))
                .collect::<Vec<_>>();
            self.selection.apply(connected, SelectionMode::Replace);
            self.status = format!(
                "{} Connected entity/entities selected.",
                self.selection.summary().total()
            );
            return;
        }
        if click_count == 2
            && !modifiers.shift
            && !modifiers.ctrl
            && !modifiers.mac_cmd
            && let Some(target) = target.as_ref()
        {
            let active_depth = match &self.selection.context {
                EditContext::Occurrence(active) => active.path.len(),
                _ if self
                    .document
                    .scene_container_occurrence()
                    .is_some_and(|root| target.occurrence.path.starts_with(&root.path)) =>
                {
                    1
                }
                _ => 0,
            };
            let child = (target.occurrence.path.len() > active_depth).then(|| OccurrenceRef {
                path: target.occurrence.path[..active_depth + 1].to_vec(),
            });
            if let Some(group) = child.filter(|p| self.document.is_object_occurrence(p)) {
                if self.block_gpu_object_edit(&group) {
                    return;
                }
                self.selection
                    .set_context(EditContext::Occurrence(group.clone()));
                self.selection_click_tracker.reset();
                self.status = format!("Editing object instance at path {:?}.", group.path);
                return;
            }
            match &target.entity {
                SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Frame,
                    ..
                } => {
                    self.selection
                        .apply([target.clone()], SelectionMode::Replace);
                    self.open_panels(ARCHITECTURE_PANEL_IDS);
                    self.selection_click_tracker.reset();
                    self.status="Frame selected: edit its parameters or handles; Explode exposes mesh primitives.".into();
                    return;
                }
                SelectedEntity::Occurrence => {
                    if self.block_gpu_object_edit(&target.occurrence) {
                        return;
                    }
                    self.selection
                        .set_context(EditContext::Occurrence(target.occurrence.clone()));
                    self.selection_click_tracker.reset();
                    self.status = format!(
                        "Editing object instance at path {:?}.",
                        target.occurrence.path
                    );
                    return;
                }
                SelectedEntity::Triangle(_) => {
                    let coplanar = self
                        .coplanar_faces(target)
                        .into_iter()
                        .filter(|target| self.native_target_editable(target))
                        .collect::<Vec<_>>();
                    self.selection.apply(coplanar, SelectionMode::Replace);
                    self.status = format!(
                        "{} Coplanar face(s) selected.",
                        self.selection.summary().faces
                    );
                    return;
                }
                SelectedEntity::Segment(_) => {
                    let connected = self.connected_line_segments(target);
                    self.selection.apply(connected, SelectionMode::Replace);
                    self.status = format!(
                        "{} Connected line segment(s) selected.",
                        self.selection.summary().edges
                    );
                    return;
                }
                _ => {}
            }
        }
        let mode = if modifiers.ctrl || modifiers.mac_cmd {
            SelectionMode::Remove
        } else if modifiers.shift {
            SelectionMode::Add
        } else if target
            .as_ref()
            .is_some_and(|target| self.selection.selected().contains(target))
        {
            SelectionMode::Remove
        } else {
            SelectionMode::Replace
        };
        if target.is_none() && mode == SelectionMode::Replace {
            self.hotspot_inspector = None;
        }
        self.selection.apply(target, mode);
        let summary = self.selection.summary();
        self.status = if summary.total() == 0 {
            "Selection cleared.".to_owned()
        } else {
            format!(
                "Selected {} object(s), {} face(s), {} edge(s), {} vertex/vertices.",
                summary.objects, summary.faces, summary.edges, summary.vertices
            )
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn apply_native_selection_region(
        &mut self,
        targets: Vec<SelectionTarget>,
        crossing: bool,
        mode: SelectionMode,
    ) {
        // Many generated facets resolve to the same architectural owner. Keep
        // their first occurrence and check editability only once per owner.
        let mut seen = BTreeSet::new();
        let targets = targets
            .into_iter()
            .map(|target| self.native_normalize_selection_target(target))
            .filter(|target| seen.insert(target.clone()))
            .filter(|target| self.native_target_editable(target))
            .collect::<Vec<_>>();
        self.selection.apply(targets, mode);
        let summary = self.selection.summary();
        self.status = format!(
            "{} selection: {} object(s), {} face(s), {} edge(s), {} vertex/vertices.",
            if crossing { "Crossing" } else { "Window" },
            summary.objects,
            summary.faces,
            summary.edges,
            summary.vertices,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn handle_native_world_pick(
        &mut self,
        point: Vec3,
        snap: Option<SnapResult>,
        target: Option<SelectionTarget>,
    ) {
        if !point.is_finite() {
            self.status = "World pointer needs finite coordinates.".to_owned();
            return;
        }
        if !self.numeric_entry.text.is_empty() {
            self.numeric_entry.pointer = Some(point);
            self.finish_numeric_entry();
            return;
        }
        if architecture_workflow::active(&self.active_tool) {
            self.arch_click(point, snap, target);
            return;
        }
        if self.active_tool == "Paint" {
            self.paint_native_target(target);
            return;
        }
        self.handle_viewport_click_at_point(
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::ZERO),
            egui::Pos2::ZERO,
            snap,
            Some(point),
        );
        self.prune_uneditable_selection();
    }

    fn paint_native_target(&mut self, target: Option<SelectionTarget>) {
        let Some(target) = target else {
            self.status = "Paint: click an object, edge, face or architectural element.".into();
            return;
        };
        let target = self.native_normalize_selection_target(target);
        if !self.native_target_editable(&target) {
            self.status = "Paint: this entity is hidden or read-only.".into();
            return;
        }
        match self.paint_material_targets(vec![target]) {
            Ok(()) => self.status = "Painted the picked element.".into(),
            Err(error) => self.status = error,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_begin_walkthrough(&mut self, eye: Vec3, forward: Vec3) {
        self.viewport.native_walkthrough_pose(eye, forward);
    }
    pub(crate) fn native_walkthrough_animating(&self) -> bool {
        self.viewport.native_walkthrough_animating()
    }
    pub(crate) fn native_walkthrough_scroll(&mut self, steps: f32) {
        self.viewport.native_walkthrough_scroll(steps);
    }
    pub(crate) fn native_travel_walkthrough(&mut self, delta: Vec3) {
        self.viewport.travel_walkthrough(&self.document, delta);
    }
    pub(crate) fn native_walkthrough_drag(&mut self, dx: f32, dy: f32, pan: bool) {
        self.viewport.native_walkthrough_drag(dx, dy, pan);
    }
    pub(crate) fn native_step_walkthrough(&mut self, dt: f64, keys: [bool; 7]) -> (Vec3, Vec3) {
        self.viewport.set_walkthrough_tuning(WalkthroughTuning {
            move_speed: f64::from(self.preferences.walk_move_speed),
            jump_velocity: f64::from(self.preferences.walk_jump_velocity),
            gravity: f64::from(self.preferences.walk_gravity),
            height_adjust_speed: f64::from(self.preferences.walk_height_adjust_speed),
        });
        self.viewport
            .native_walkthrough_step(&self.document, dt, keys)
    }

    pub fn handle_native_escape(&mut self) {
        self.handle_escape();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_edit_context(&self) -> EditContext {
        self.selection.context.clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_selection_display_policy(&self) -> SelectionDisplayPolicy {
        self.selection_display_policy.clone()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_selection_revision(&self) -> u64 {
        self.selection.revision()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_selection_summary(&self) -> meshcraft_core::selection::SelectionSummary {
        self.selection.summary()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_selection_mesh_counts(
        &self,
        view: &Document,
    ) -> meshcraft_core::selection::SelectionMeshCounts {
        self.selection.mesh_counts(view)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_selection_contains(&self, target: &SelectionTarget) -> bool {
        self.selection.selected().contains(target)
    }

    pub fn native_selection_targets(&self) -> Vec<SelectionTarget> {
        self.selection.selected().iter().cloned().collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_top_level_occurrences(&self) -> Vec<OccurrenceRef> {
        self.top_level_occurrences()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn select_native_object_from_inspector(
        &mut self,
        occurrence: &OccurrenceRef,
        edit: bool,
    ) -> Result<(), String> {
        if self.document.resolve_occurrence(occurrence).is_none() {
            return Err("the selected object no longer exists".to_owned());
        }
        if let Some(frame) = self.document.frame_target_for_occurrence(occurrence) {
            self.selection.apply([frame], SelectionMode::Replace);
            self.open_panels(ARCHITECTURE_PANEL_IDS);
            self.status = "Frame selected: edit its parameters or handles.".into();
            return Ok(());
        }
        self.set_selection([occurrence.clone()]);
        self.selection_click_tracker.reset();
        if edit && self.block_gpu_object_edit(occurrence) {
            return Ok(());
        }
        if edit {
            self.selection
                .set_context(EditContext::Occurrence(occurrence.clone()));
            self.status = format!("Editing object instance at path {:?}.", occurrence.path);
        } else {
            self.status = "Selected 1 object.".to_owned();
        }
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn rename_native_selected_object(&mut self, name: &str) -> Result<bool, String> {
        let paths = if matches!(self.selection.context, EditContext::Occurrence(_)) {
            vec![]
        } else {
            self.selected_occurrences()
        };
        let mut definitions = BTreeSet::new();
        for path in paths {
            if !self.native_target_editable(&SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Occurrence,
            }) {
                return Err("A selected object is read-only.".into());
            }
            definitions.insert(
                self.document
                    .resolve_occurrence(&path)
                    .ok_or("object no longer exists")?
                    .definition
                    .id,
            );
        }
        if definitions.is_empty() {
            definitions.insert(
                self.native_active_object()
                    .map(|(id, _, _)| id)
                    .ok_or("select an object instance or prototype before renaming it")?,
            );
        }
        match self.commit_document_edit("Rename object", |document| {
            let mut changed = false;
            for definition_id in definitions {
                changed |= document.rename_definition(definition_id, name)?;
            }
            Ok(changed)
        }) {
            Ok(changed) => {
                if changed {
                    self.status = format!("Renamed object to ‘{}’.", name.trim());
                }
                Ok(changed)
            }
            Err(error) => {
                self.status = error.clone();
                Err(error)
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_native_cursor_snap(&mut self, snap: Option<SnapResult>) {
        self.cursor_snap = snap;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_tool_feedback_at(
        &mut self,
        point: Vec3,
        screen_position: egui::Pos2,
    ) -> (Option<ToolPreview>, Option<ToolFeedback>) {
        let snap = SnapResult {
            point,
            normal: Vec3::Y,
            kind: SnapKind::Free,
            screen_distance: 0.0,
        };
        self.native_tool_feedback_for_snap(snap, screen_position)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_tool_feedback_for_snap(
        &mut self,
        mut snap: SnapResult,
        screen_position: egui::Pos2,
    ) -> (Option<ToolPreview>, Option<ToolFeedback>) {
        if self.active_tool == "Paint" {
            return (
                None,
                Some(ToolFeedback {
                    screen_position,
                    text: "Paint · click an object instance, architectural element, edge or face"
                        .into(),
                }),
            );
        }
        if self.object_place_tool_state.active_definition().is_some() {
            self.object_place_tool_state
                .update_pointer(Some(PlacementHit {
                    position: snap.point,
                    surface_normal: (snap.kind == SnapKind::Face).then_some(snap.normal),
                }));
        }
        if self.push_pull_reference.is_some() {
            self.push_pull_pointer = Some(snap.point);
        }
        self.transform_tool_state.set_pointer(Some(snap.point));
        self.mesh_tool_state.set_pointer(Some(snap.point));
        self.line_offset_tool_state.set_pointer(Some(snap.point));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::ONE);
        let preview = self.tool_preview(rect, Some(screen_position), Some(snap));
        snap.point = self.numeric_preview_point(snap.point);
        let mut feedback =
            self.tool_feedback(rect, Some(screen_position), Some(snap), preview.as_ref());
        if !self.numeric_entry.text.is_empty() {
            let note = format!(
                "Input: {} · Enter accept · Backspace edit",
                self.numeric_entry.text
            );
            if let Some(feedback) = &mut feedback {
                feedback.text = format!("{note} · {}", feedback.text);
            } else {
                feedback = Some(ToolFeedback {
                    screen_position,
                    text: note,
                });
            }
        }
        (preview, feedback)
    }

    fn rotational_reference_points(&self) -> &[Vec3] {
        if self.rotational_array_tool_state.active() {
            self.rotational_array_tool_state.points()
        } else if self.revolve_tool_state.active() {
            self.revolve_tool_state.axis_points()
        } else {
            &[]
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_rotational_reference_edges(&self, pointer: Vec3) -> Vec<[Vec3; 2]> {
        let points = self.rotational_reference_points();
        let Some(center) = points.first() else {
            return vec![];
        };
        points
            .iter()
            .skip(1)
            .copied()
            .chain([pointer])
            .flat_map(|p| [[*center, p], [p, p]])
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_construction_guides(&self) -> &[ConstructionGuide] {
        &self.guides
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_guides_visible(&self) -> bool {
        self.preferences.show_construction_guides
    }

    fn cursor_aligned_guide(
        &mut self,
        kind: ConstructionGuideKind,
        snap: SnapResult,
    ) -> Option<ConstructionGuide> {
        #[cfg(not(target_arch = "wasm32"))]
        let [axis_u, axis_v, axis_w] = self.native_cursor_axes(snap)?;
        #[cfg(target_arch = "wasm32")]
        let [axis_u, axis_v, axis_w] = {
            let scope = match &self.selection.context {
                EditContext::Document => SnapScope::Document,
                EditContext::Definition(id) => SnapScope::Definition(*id),
                EditContext::Occurrence(active) => SnapScope::Occurrence(active),
            };
            let line = matches!(snap.kind, SnapKind::Line | SnapKind::Midpoint)
                .then(|| crate::viewport::snapped_segment_axis(&self.document, scope, snap.point))
                .flatten();
            crate::viewport::cursor_axes(
                snap.normal,
                line,
                self.preferences.grid_normal_axis.vector(),
            )?
        };
        Some(ConstructionGuide {
            kind,
            origin: snap.point,
            axis_u,
            axis_v,
            axis_w,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn native_prepare_spatial_view(&mut self) {
        let context = (self.document_generation, self.selection.context.clone());
        if self.native_spatial_context.as_ref() != Some(&context) {
            self.native_spatial_context = Some(context);
            self.native_edit_view = None;
            self.viewport.invalidate_document_caches();
        }
        if let EditContext::Occurrence(active) = &self.selection.context {
            let revision = self.document.visual_revision();
            if self
                .native_edit_view
                .as_ref()
                .is_none_or(|(cached, _)| *cached != revision)
            {
                self.native_edit_view = Some((
                    revision,
                    Arc::new(self.document.prototype_edit_view(active)),
                ));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_cursor_axes(&mut self, snap: SnapResult) -> Option<[Vec3; 3]> {
        self.native_prepare_spatial_view();
        let document = self
            .native_edit_view
            .as_ref()
            .map_or(&self.document, |(_, view)| view.as_ref());
        let scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        self.viewport.native_cursor_axes(
            document,
            scope,
            &self.selection_display_policy,
            snap,
            self.preferences.grid_normal_axis.vector(),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn pick_native_target(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        radius: f64,
    ) -> Option<SelectionTarget> {
        self.native_prepare_spatial_view();
        let document = self
            .native_edit_view
            .as_ref()
            .map_or(&self.document, |(_, view)| view.as_ref());
        let scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        let target = self.viewport.pick_selection_target_on_ray(
            document,
            origin,
            direction,
            radius,
            scope,
            &self.selection_display_policy,
        )?;
        let target = if matches!(self.active_tool.as_str(), "Select" | "Paint") {
            self.native_normalize_selection_target(target)
        } else {
            target
        };
        self.native_target_editable(&target).then_some(target)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_snap_for_target(
        &self,
        target: &SelectionTarget,
        origin: Vec3,
        direction: Vec3,
    ) -> Option<SnapResult> {
        self.viewport
            .snap_selection_target_on_ray(&self.document, target, origin, direction)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_snap_point_on_ray(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        radius: f64,
    ) -> Option<SnapResult> {
        self.native_snap_with_tolerance(
            origin,
            direction,
            crate::native_snap::SnapTolerance::World(radius),
            None,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_snap_with_tolerance(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        tolerance: crate::native_snap::SnapTolerance,
        excluded: Option<&SelectionTarget>,
    ) -> Option<SnapResult> {
        // Editing isolates selection, not references: neighboring scene geometry still snaps.
        self.native_prepare_spatial_view();
        let document = self
            .native_edit_view
            .as_ref()
            .map_or(&self.document, |(_, view)| view.as_ref());
        let scope = SnapScope::Document;
        self.viewport.snap_geometry_on_ray(
            document,
            origin,
            direction,
            tolerance,
            scope,
            &self.selection_display_policy,
            excluded,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_recent_projects(&self) -> &[String] {
        &self.preferences.recent_projects
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_document_replacement_prompt(&self) -> Option<&'static str> {
        self.pending_replacement.as_ref().map(|p| match p.target {
            ReplacementTarget::NewDocument => "create a new document",
            ReplacementTarget::OpenProject(_) => "open another project",
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_resolve_document_replacement(&mut self, decision: ReplacementDecision) {
        self.resolve_document_replacement(decision);
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_native_project_path(&mut self, path: PathBuf) {
        if let Err(error) = self.remember_recent_project(&path) {
            self.status
                .push_str(&format!(" Recent files could not be saved: {error}"));
        }
        self.current_project_path = Some(path);
    }

    fn document_is_dirty(&self) -> bool {
        self.document.revision() != self.saved_revision
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        Self::from_document(
            creation_context,
            Document::new(),
            "Empty document ready · choose Cuboid or Add Cuboid to begin.".to_owned(),
            false,
            None,
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_with_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
    ) -> Self {
        Self::from_document(creation_context, document, status, true, None)
    }

    #[cfg(target_arch = "wasm32")]
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

    #[cfg(target_arch = "wasm32")]
    pub fn new_with_error(creation_context: &eframe::CreationContext<'_>, error: String) -> Self {
        Self::from_document(creation_context, Document::new(), error, false, None)
    }

    #[cfg(target_arch = "wasm32")]
    fn from_document(
        creation_context: &eframe::CreationContext<'_>,
        document: Document,
        status: String,
        frame_document: bool,
        legacy_history: Option<OpaqueLegacyHistory>,
    ) -> Self {
        let mut preferences: Preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, PREFERENCES_KEY))
            .unwrap_or_default();
        preferences.normalize_recent_projects();

        Self::from_parts(
            preferences,
            assets::load_icon_atlas(&creation_context.egui_ctx),
            document,
            status,
            frame_document,
            legacy_history,
        )
    }

    fn from_parts(
        preferences: Preferences,
        icon_atlas: Option<egui::TextureHandle>,
        document: Document,
        status: String,
        frame_document: bool,
        legacy_history: Option<OpaqueLegacyHistory>,
    ) -> Self {
        let mut preferences = preferences;
        preferences.normalize_recent_projects();
        let legacy_active_index = legacy_history
            .as_ref()
            .and_then(OpaqueLegacyHistory::active_entry_index);
        let mut document = document;
        let _ = document.normalize_material_bindings();
        let saved_revision = document.revision();
        let mut viewport = Viewport::default();
        let restored_camera = meshcraft_core::legacy::legacy_camera_state(&document)
            .is_some_and(|state| viewport.restore_legacy_camera_state(state));
        if frame_document && !restored_camera {
            viewport.request_frame();
        }

        Self {
            preferences,
            #[cfg(not(target_arch = "wasm32"))]
            persist_native_preferences: false,
            icon_atlas,
            icon_regions: assets::icon_regions(),
            active_tool: "Select".to_owned(),
            status,
            pointer_in_viewport: None,
            cursor_snap: None,
            viewport_rect: egui::Rect::NOTHING,
            open_file_menu: false,
            recent_projects_dialog_open: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            quit_requested: false,
            panel_focus: None,
            document,
            document_generation: 0,
            annotations: Default::default(),
            architecture: Default::default(),
            section: Default::default(),
            clipboard: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            mcp_jobs: Default::default(),
            saved_revision,
            #[cfg(not(target_arch = "wasm32"))]
            autosave_observed_revision: saved_revision,
            #[cfg(not(target_arch = "wasm32"))]
            autosave_deadline: None,
            history: TransactionHistory::default(),
            legacy_history,
            legacy_active_index,
            #[cfg(not(target_arch = "wasm32"))]
            current_project_path: None,
            next_job_id: 1,
            job_results: BTreeMap::new(),
            pending_replacement: None,
            pending_prototype_deletion: None,
            #[cfg(not(target_arch = "wasm32"))]
            replacement_after_save: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_project_load: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_project_save: None,
            selection: SelectionSession::default(),
            copy_availability: Default::default(),
            transform_wireframe: None,
            selection_display_policy: SelectionDisplayPolicy::default(),
            selection_click_tracker: SelectionClickTracker::default(),
            #[cfg(not(target_arch = "wasm32"))]
            native_spatial_context: None,
            #[cfg(not(target_arch = "wasm32"))]
            native_edit_view: None,
            object_name_edit: None,
            object_browser_selection: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_object_import: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_object_bake: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_interchange: None,
            hotspot_inspector: None,
            hotspot_defaults: Default::default(),
            hotspot_reference_pick: false,
            viewport,
            guides: Vec::new(),
            cuboid_first: None,
            drawing_tool: DrawingToolState::default(),
            polyline_tool_state: PolylineToolState::default(),
            mesh_tool_state: MeshToolState::default(),
            line_offset_tool_state: LineOffsetToolState::default(),
            push_pull_reference: None,
            push_pull_pointer: None,
            push_pull_faces: Vec::new(),
            push_pull_owner: None,
            push_pull_occurrence: None,
            push_pull_distance_focused: false,
            push_pull_numeric_input: String::new(),
            numeric_entry: Default::default(),
            tool_cycle_state: ToolCycleState::default(),
            primitive_tool_state: PrimitiveToolState::default(),
            revolve_tool_state: RevolveToolState::default(),
            shape_tool_state: ShapeToolState::default(),
            transform_tool_state: TransformToolState::default(),
            object_place_tool_state: ObjectPlaceToolState::default(),
            placement_preview: Default::default(),
            array_tool_state: ArrayToolState::default(),
            rotational_array_tool_state: RotationalArrayToolState::default(),
            current_paint_color: Rgba8::ANSI_GREY,
            current_paint_material: None,
            array_count: 12,
            array_spacing: 3.0,
            array_preview: false,
            pending_selection_volume: None,
        }
    }

    fn top_level_occurrences(&self) -> Vec<OccurrenceRef> {
        let container = self.document.scene_container_occurrence();
        self.document
            .occurrence_refs()
            .unwrap_or_default()
            .into_iter()
            .filter(|path| {
                if let Some(root) = container.as_ref() {
                    (path.path.len() == root.path.len() + 1 && path.path.starts_with(&root.path))
                        || (path.path.len() == 1 && path != root)
                } else {
                    path.path.len() == 1
                }
            })
            .filter(|path| self.document.is_object_occurrence(path))
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
        let targets = occurrences
            .into_iter()
            .map(|occurrence| SelectionTarget {
                occurrence,
                entity: SelectedEntity::Occurrence,
            })
            .filter(|target| {
                self.selection_display_policy
                    .can_edit_target(&self.document, target)
            })
            .collect::<Vec<_>>();
        self.selection.apply(targets, SelectionMode::Replace);
    }

    fn line_geometry_context(
        &self,
    ) -> (LineOwner, OccurrenceRef, meshcraft_core::geometry::Affine3) {
        let identity = meshcraft_core::geometry::Affine3::IDENTITY;
        match &self.selection.context {
            EditContext::Occurrence(occurrence) => {
                if let Some(resolved) = self.document.resolve_occurrence(occurrence) {
                    let inverse = resolved
                        .world_transform
                        .inverse(meshcraft_core::geometry::GeometryTolerance::default())
                        .unwrap_or(identity);
                    return (
                        LineOwner::Definition(resolved.definition.id),
                        occurrence.clone(),
                        inverse,
                    );
                }
            }
            EditContext::Definition(definition) => {
                if *definition == Document::SCENE_ROOT_DEFINITION_ID {
                    return (LineOwner::SceneRoot, OccurrenceRef::scene_root(), identity);
                }
                if let Some(occurrence) = self
                    .document
                    .occurrence_refs()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|occurrence| {
                        self.document
                            .resolve_occurrence(occurrence)
                            .is_some_and(|resolved| resolved.definition.id == *definition)
                    })
                {
                    let inverse = self
                        .document
                        .resolve_occurrence(&occurrence)
                        .and_then(|resolved| {
                            resolved
                                .world_transform
                                .inverse(meshcraft_core::geometry::GeometryTolerance::default())
                        })
                        .unwrap_or(identity);
                    return (LineOwner::Definition(*definition), occurrence, inverse);
                }
            }
            EditContext::Document => {}
        }

        if let Some(occurrence) = self.document.scene_container_occurrence()
            && let Some(r) = self.document.resolve_occurrence(&occurrence)
        {
            let inverse = r
                .world_transform
                .inverse(GeometryTolerance::default())
                .unwrap_or(identity);
            return (LineOwner::Definition(r.definition.id), occurrence, inverse);
        }
        (LineOwner::SceneRoot, OccurrenceRef::scene_root(), identity)
    }

    fn create_line_in_active_context(
        &mut self,
        start: Vec3,
        end: Vec3,
    ) -> Result<Vec<SelectionTarget>, String> {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let start = inverse.transform_point(start);
        let end = inverse.transform_point(end);
        let insertion = self.commit_document_edit("Create line", |document| {
            drawing::insert_line(document, owner, start, end)
        })?;
        let definition = match owner {
            LineOwner::SceneRoot => Document::SCENE_ROOT_DEFINITION_ID,
            LineOwner::Definition(definition) => definition,
        };
        self.selection
            .remap_segments(&self.document, definition, &insertion.remap.segments);
        Ok(insertion
            .affected_segments
            .into_iter()
            .map(|segment| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(segment),
            })
            .collect())
    }

    fn create_planar_shape_in_active_context(
        &mut self,
        mesh: PlanarMesh,
        label: &'static str,
    ) -> Result<Vec<SelectionTarget>, String> {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let vertices = mesh
            .vertices
            .into_iter()
            .map(|point| inverse.transform_point(point))
            .collect::<Vec<_>>();
        self.commit_local_planar_mesh(
            owner,
            occurrence,
            vertices,
            mesh.segments,
            mesh.triangles,
            label,
        )
    }

    fn create_primitive_in_active_context(
        &mut self,
        mesh: PlanarMesh,
        kind: PrimitiveKind,
    ) -> Result<Vec<SelectionTarget>, String> {
        let (owner, occurrence, _) = self.line_geometry_context();
        let color = self.current_face_color();
        let line_color = self.current_outline_color();
        let segments = mesh
            .segments
            .into_iter()
            .map(|segment| (segment, line_color))
            .collect::<Vec<_>>();
        let triangles = mesh
            .triangles
            .into_iter()
            .map(|triangle| (triangle, color))
            .collect::<Vec<_>>();
        let inserted = self.commit_document_edit(
            &format!("Create {}", kind.label()),
            |document| match owner {
                LineOwner::SceneRoot => document.append_colored_indexed_mesh_to_scene_root(
                    mesh.vertices,
                    segments,
                    triangles,
                ),
                LineOwner::Definition(definition) => document
                    .append_colored_indexed_mesh_to_definition(
                        definition,
                        mesh.vertices,
                        segments,
                        triangles,
                    ),
            },
        )?;
        let (segment_ids, triangle_ids) = inserted;
        Ok(segment_ids
            .into_iter()
            .map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(id),
            })
            .chain(triangle_ids.into_iter().map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(id),
            }))
            .collect())
    }

    fn create_cuboid_in_active_context(
        &mut self,
        preview: CuboidPreview,
    ) -> Result<Vec<SelectionTarget>, String> {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let mut mesh = cuboid_mesh(preview);
        for vertex in &mut mesh.vertices {
            *vertex = inverse.transform_point(*vertex);
        }
        let color = self.current_face_color();
        let line_color = self.current_outline_color();
        let segments = mesh
            .segments
            .into_iter()
            .map(|segment| (segment, line_color))
            .collect::<Vec<_>>();
        let triangles = mesh
            .triangles
            .into_iter()
            .map(|triangle| (triangle, color))
            .collect::<Vec<_>>();
        let (segment_ids, triangle_ids) =
            self.commit_document_edit("Create cuboid", |document| match owner {
                LineOwner::SceneRoot => document.append_colored_indexed_mesh_to_scene_root(
                    mesh.vertices,
                    segments,
                    triangles,
                ),
                LineOwner::Definition(definition) => document
                    .append_colored_indexed_mesh_to_definition(
                        definition,
                        mesh.vertices,
                        segments,
                        triangles,
                    ),
            })?;
        Ok(segment_ids
            .into_iter()
            .map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(id),
            })
            .chain(triangle_ids.into_iter().map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(id),
            }))
            .collect())
    }

    fn commit_local_planar_mesh(
        &mut self,
        owner: LineOwner,
        occurrence: OccurrenceRef,
        vertices: Vec<Vec3>,
        segments: Vec<[usize; 2]>,
        triangles: Vec<[usize; 3]>,
        label: &'static str,
    ) -> Result<Vec<SelectionTarget>, String> {
        let (segment_ids, triangle_ids) =
            self.commit_document_edit(&format!("Create {label}"), |document| match owner {
                LineOwner::SceneRoot => {
                    document.append_planar_mesh_to_scene_root(vertices, segments, triangles)
                }
                LineOwner::Definition(definition) => document
                    .append_planar_mesh_to_definition(definition, vertices, segments, triangles),
            })?;
        let mut targets = segment_ids
            .into_iter()
            .map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(id),
            })
            .collect::<Vec<_>>();
        targets.extend(triangle_ids.into_iter().map(|id| SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(id),
        }));
        Ok(targets)
    }

    fn pick_mesh_point(&mut self, point: Vec3, preferred_normal: Option<Vec3>) {
        let previous_state = self.mesh_tool_state.clone();
        let pick = match self.mesh_tool_state.pick(point) {
            Ok(pick) => pick,
            Err(error) => {
                self.status = error.to_owned();
                return;
            }
        };
        let Some([start, end]) = pick.segment else {
            self.status = "Pick another mesh point to create a connecting edge.".to_owned();
            return;
        };

        let (owner, occurrence, inverse) = self.line_geometry_context();
        let mut vertices = vec![inverse.transform_point(start), inverse.transform_point(end)];
        let mut triangles = Vec::new();
        if let Some(triangle) = pick.triangle {
            let mut local_triangle = triangle.map(|point| inverse.transform_point(point));
            if let Some(normal) = preferred_normal
                .and_then(|normal| inverse.transform_vector(normal).try_normalized(1.0e-12))
                && (local_triangle[1] - local_triangle[0])
                    .cross(local_triangle[2] - local_triangle[0])
                    .dot(normal)
                    < 0.0
            {
                local_triangle.swap(1, 2);
            }
            let first = vertices.len();
            vertices.extend(local_triangle);
            triangles.push([first, first + 1, first + 2]);
        }

        match self.commit_local_planar_mesh(
            owner,
            occurrence,
            vertices,
            vec![[0, 1]],
            triangles,
            "Mesh",
        ) {
            Ok(_) => {
                self.status = if pick.triangle.is_some() {
                    "Mesh point added. Triangle created.".to_owned()
                } else {
                    "Mesh point added.".to_owned()
                };
            }
            Err(error) => {
                self.mesh_tool_state = previous_state;
                self.status = error;
            }
        }
    }

    fn outline_selected_faces(&mut self) -> CommandResult {
        let mut world_triangles = Vec::new();
        for target in self.selection.selected() {
            let SelectedEntity::Triangle(triangle_id) = target.entity else {
                continue;
            };
            let Some(resolved) = self.document.resolve_occurrence(&target.occurrence) else {
                continue;
            };
            let Some(geometry) = self
                .document
                .geometry_in_context(&target.occurrence, &self.selection.context)
            else {
                continue;
            };
            let Some(triangle) = geometry.triangles.get(&triangle_id) else {
                continue;
            };
            let [Some(a), Some(b), Some(c)] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference))
            else {
                continue;
            };
            world_triangles
                .push([a, b, c].map(|point| resolved.world_transform.transform_point(point)));
        }
        if world_triangles.is_empty() {
            return CommandResult::failed("Select one or more faces to outline.");
        }
        let mesh = match build_face_outline_mesh(&world_triangles, FACE_OUTLINE_MERGE_TOLERANCE) {
            Ok(mesh) if !mesh.segments.is_empty() => mesh,
            Ok(_) => return CommandResult::failed("No outline found for the selected faces."),
            Err(error) => return CommandResult::failed(error),
        };
        let edge_count = mesh.segments.len();
        let face_count = world_triangles.len();
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let vertices = mesh
            .vertices
            .into_iter()
            .map(|point| inverse.transform_point(point))
            .collect();
        let targets = match self.commit_local_planar_mesh(
            owner,
            occurrence,
            vertices,
            mesh.segments,
            mesh.triangles,
            "Face Outline",
        ) {
            Ok(targets) => targets,
            Err(error) => return CommandResult::failed(error),
        };
        self.selection.apply(targets, SelectionMode::Replace);
        self.status =
            format!("Outlined {edge_count} boundary edge(s) from {face_count} selected face(s).");
        let mut result = CommandResult::completed();
        result.outputs.insert(
            "outlined".to_owned(),
            serde_json::json!({"edges": edge_count, "faces": face_count}),
        );
        result
    }

    fn selected_world_segments(&self) -> Vec<[Vec3; 2]> {
        let mut world_segments = Vec::new();
        for target in self.selection.selected() {
            let SelectedEntity::Segment(segment_id) = target.entity else {
                continue;
            };
            let Some(resolved) = self.document.resolve_occurrence(&target.occurrence) else {
                continue;
            };
            let Some(geometry) = self
                .document
                .geometry_in_context(&target.occurrence, &self.selection.context)
            else {
                continue;
            };
            let Some(segment) = geometry.segments.get(&segment_id) else {
                continue;
            };
            let [Some(start), Some(end)] = segment
                .endpoints
                .map(|reference| geometry.vertex_position(reference))
            else {
                continue;
            };
            world_segments.push([
                resolved.world_transform.transform_point(start),
                resolved.world_transform.transform_point(end),
            ]);
        }
        world_segments
    }

    fn mesh_selected_polylines(&mut self) -> CommandResult {
        let world_segments = self.selected_world_segments();
        if world_segments.is_empty() {
            return CommandResult::failed("Select connected line segments to fill with faces.");
        }
        let mesh =
            match build_polyline_mesh_from_segments(&world_segments, POLYLINE_MESH_MERGE_TOLERANCE)
            {
                Ok(mesh) => mesh,
                Err(error) => return CommandResult::failed(error),
            };
        let face_count = mesh.triangles.len();
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let vertices = mesh
            .vertices
            .into_iter()
            .map(|point| inverse.transform_point(point))
            .collect();
        let triangle_ids =
            match self.commit_document_edit("Polyline Mesh", |document| match owner {
                LineOwner::SceneRoot => {
                    document.append_triangles_to_scene_root(vertices, mesh.triangles)
                }
                LineOwner::Definition(definition) => {
                    document.append_triangles_to_definition(definition, vertices, mesh.triangles)
                }
            }) {
                Ok(triangle_ids) => triangle_ids,
                Err(error) => return CommandResult::failed(error),
            };
        self.selection.apply(
            triangle_ids.into_iter().map(|triangle| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(triangle),
            }),
            SelectionMode::Replace,
        );
        self.status = format!(
            "Created {face_count} face(s) from {} selected segment(s).",
            world_segments.len()
        );
        let mut result = CommandResult::completed();
        result.outputs.insert(
            "polyline_mesh".to_owned(),
            serde_json::json!({"faces": face_count, "segments": world_segments.len()}),
        );
        result
    }

    fn selected_cut_occurrence(&self) -> Option<OccurrenceRef> {
        let mut occurrence = None;
        let mut has_faces = false;
        let mut has_edges = false;
        for target in self.selection.selected() {
            match target.entity {
                SelectedEntity::Triangle(_) => has_faces = true,
                SelectedEntity::Segment(_) => has_edges = true,
                _ => continue,
            }
            if occurrence
                .as_ref()
                .is_some_and(|current| current != &target.occurrence)
            {
                return None;
            }
            occurrence = Some(target.occurrence.clone());
        }
        if !has_faces || !has_edges {
            return None;
        }
        let occurrence = occurrence?;
        let resolved = self.document.resolve_occurrence(&occurrence)?;
        match &self.selection.context {
            EditContext::Document if occurrence.is_scene_root() => Some(occurrence),
            EditContext::Definition(definition) if resolved.definition.id == *definition => {
                Some(occurrence)
            }
            EditContext::Occurrence(active) if active == &occurrence => Some(occurrence),
            _ => None,
        }
    }

    fn cut_selected_faces(&mut self, tool: CutTool) -> CommandResult {
        let Some(occurrence) = self.selected_cut_occurrence() else {
            return CommandResult::failed(
                "Select faces and edges in one active geometry graph before cutting.",
            );
        };
        let Some(resolved) = self.document.resolve_occurrence(&occurrence) else {
            return CommandResult::failed("The selected geometry graph is no longer available.");
        };
        let authored = matches!(&self.selection.context, EditContext::Occurrence(active) if active == &occurrence)
            || matches!(&self.selection.context, EditContext::Definition(id) if *id == resolved.definition.id)
            || occurrence.is_scene_root();
        let has_instance_geometry = !authored
            && resolved
                .edits
                .is_some_and(|edits| edits.geometry_replacement.is_some());
        let Some(inverse) = resolved
            .world_transform
            .inverse(GeometryTolerance::default())
        else {
            return CommandResult::failed("The selected geometry transform cannot be inverted.");
        };
        let definition = resolved.definition.id;
        let Some(geometry) = self
            .document
            .geometry_in_context(&occurrence, &self.selection.context)
        else {
            return CommandResult::failed("The selected geometry graph is no longer available.");
        };

        let mut source_ids = Vec::new();
        let mut source_faces = Vec::new();
        let mut selected_segments = Vec::new();
        for target in self.selection.selected() {
            if target.occurrence != occurrence {
                continue;
            }
            match target.entity {
                SelectedEntity::Triangle(id) => {
                    let Some(triangle) = geometry.triangles.get(&id) else {
                        continue;
                    };
                    let Some(points) = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                    else {
                        return CommandResult::failed("A selected face has invalid vertices.");
                    };
                    let source = source_ids.len();
                    source_ids.push(id);
                    source_faces.push(CutSourceTriangle {
                        source,
                        vertices: [points[0], points[1], points[2]]
                            .map(|point| resolved.world_transform.transform_point(point)),
                        color: triangle.color,
                    });
                }
                SelectedEntity::Segment(id) => {
                    let Some(segment) = geometry.segments.get(&id) else {
                        continue;
                    };
                    let [Some(start), Some(end)] = segment
                        .endpoints
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        return CommandResult::failed("A selected edge has invalid endpoints.");
                    };
                    selected_segments.push(CutSegment3 {
                        start: resolved.world_transform.transform_point(start),
                        end: resolved.world_transform.transform_point(end),
                    });
                }
                _ => {}
            }
        }
        let cut_tolerance =
            GeometryTolerance::new(1.0e-3, 1.0e-12).expect("the legacy cut tolerance is valid");
        let cut =
            match cut_selected_face_mesh(&source_faces, &selected_segments, tool, cut_tolerance) {
                Ok(cut) => cut,
                Err(error) => return CommandResult::failed(error),
            };

        let changed_source_ids = cut
            .replaced_sources
            .iter()
            .map(|source| source_ids[*source])
            .collect::<Vec<_>>();
        let replacements = cut
            .triangles
            .iter()
            .map(|triangle| TriangleReplacement {
                source: source_ids[triangle.source],
                vertices: triangle
                    .vertices
                    .map(|point| inverse.transform_point(point)),
                color: triangle.color,
            })
            .collect::<Vec<_>>();
        let label = match tool {
            CutTool::Holes => "Cut Holes",
            CutTool::Holes2 => "Cut Holes 2",
            CutTool::Cutout => "Cutout",
        };
        let remap = match self.commit_document_edit(label, |document| {
            if has_instance_geometry {
                document.replace_selected_occurrence_triangles(
                    &occurrence,
                    &changed_source_ids,
                    replacements,
                )
            } else {
                document.replace_selected_triangles(definition, &changed_source_ids, replacements)
            }
        }) {
            Ok(remap) => remap,
            Err(error) => return CommandResult::failed(error),
        };
        let mut selection = self
            .selection
            .selected()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for targets in remap.triangles.values() {
            selection.extend(targets.iter().copied().map(|id| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(id),
            }));
        }
        self.selection.apply(selection, SelectionMode::Replace);
        let created_faces = remap.triangles.values().map(Vec::len).sum::<usize>();
        let removed_faces = remap
            .triangles
            .values()
            .filter(|targets| targets.is_empty())
            .count();
        self.status = format!(
            "{label}: replaced {} selected face(s), created {created_faces}, removed {removed_faces}.",
            remap.triangles.len()
        );
        let mut result = CommandResult::completed();
        result.outputs.insert(
            "cut".to_owned(),
            serde_json::json!({
                "replaced_faces": remap.triangles.len(),
                "created_faces": created_faces,
                "removed_faces": removed_faces,
                "tool": match tool {
                    CutTool::Holes => "CUT_HOLES",
                    CutTool::Holes2 => "CUT_HOLES_2",
                    CutTool::Cutout => "CUT_OUT_3",
                },
            }),
        );
        result
    }

    fn commit_line_offset(&mut self, reference: Vec3, target: Vec3) -> CommandResult {
        let world_segments = self.selected_world_segments();
        if world_segments.is_empty() {
            return CommandResult::failed("No edges selected.");
        }
        let offset = match build_line_offset_mesh_from_segments(
            &world_segments,
            reference,
            target,
            LINE_OFFSET_MERGE_TOLERANCE,
        ) {
            Ok(offset) => offset,
            Err(error) => return CommandResult::failed(error),
        };
        let edge_count = offset.mesh.segments.len();
        let face_count = offset.mesh.triangles.len();
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let vertices = offset
            .mesh
            .vertices
            .into_iter()
            .map(|point| inverse.transform_point(point))
            .collect();
        let targets = match self.commit_local_planar_mesh(
            owner,
            occurrence,
            vertices,
            offset.mesh.segments,
            offset.mesh.triangles,
            "Line Offset",
        ) {
            Ok(targets) => targets,
            Err(error) => return CommandResult::failed(error),
        };
        self.selection.apply(targets, SelectionMode::Replace);
        self.status = format!("Offset copy | edges {edge_count} faces {face_count}");
        let mut result = CommandResult::completed();
        result.outputs.insert(
            "line_offset".to_owned(),
            serde_json::json!({"edges": edge_count, "faces": face_count, "offsets": offset.offsets}),
        );
        result
    }

    fn pick_line_offset_point(&mut self, point: Vec3) {
        if self.selected_world_segments().is_empty() {
            self.status = "No edges selected.".to_owned();
            return;
        }
        match self.line_offset_tool_state.pick(point) {
            Ok(LineOffsetPick::Reference(_)) => {
                self.status = "Line Offset: pick target point.".to_owned();
            }
            Ok(LineOffsetPick::Complete { reference, target }) => {
                let result = self.commit_line_offset(reference, target);
                if result.completion == CommandCompletion::Failed {
                    self.status = result
                        .diagnostics
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Line Offset failed.".to_owned());
                }
            }
            Err(error) => self.status = error.to_owned(),
        }
    }

    fn clear_push_pull_transient(&mut self) {
        self.push_pull_reference = None;
        self.push_pull_pointer = None;
        self.push_pull_faces.clear();
        self.push_pull_owner = None;
        self.push_pull_occurrence = None;
        self.push_pull_distance_focused = false;
        self.push_pull_numeric_input.clear();
    }

    fn push_pull_displacement(&self) -> Option<Vec3> {
        let reference = self.push_pull_reference?;
        let target = self.push_pull_pointer?;
        let mut displacement = target - reference;
        if self.push_pull_distance_focused
            && let Ok(distance) = self.push_pull_numeric_input.parse::<f64>()
            && let Some(direction) = displacement.try_normalized(1.0e-12)
        {
            displacement = direction * distance;
        }
        Some(displacement)
    }

    fn selected_push_pull_faces(
        &self,
        occurrence: &OccurrenceRef,
    ) -> Vec<(TriangleId, ExtrudeSourceTriangle)> {
        let Some(resolved) = self.document.resolve_occurrence(occurrence) else {
            return Vec::new();
        };
        let Some(geometry) = self
            .document
            .geometry_in_context(occurrence, &self.selection.context)
        else {
            return Vec::new();
        };
        self.selection
            .selected()
            .iter()
            .filter_map(|target| {
                if &target.occurrence != occurrence {
                    return None;
                }
                let SelectedEntity::Triangle(id) = target.entity else {
                    return None;
                };
                let triangle = geometry.triangles.get(&id)?;
                let points = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                    .into_iter()
                    .collect::<Option<Vec<_>>>()?;
                let [a, b, c] = [points[0], points[1], points[2]];
                Some((
                    id,
                    ExtrudeSourceTriangle {
                        vertices: [a, b, c]
                            .map(|point| resolved.world_transform.transform_point(point)),
                        color: triangle.color,
                    },
                ))
            })
            .collect()
    }

    fn pick_push_pull_point(&mut self, point: Vec3) {
        let Some(reference) = self.push_pull_reference else {
            let (owner, occurrence, _) = self.line_geometry_context();
            let faces = self.selected_push_pull_faces(&occurrence);
            if faces.is_empty() {
                self.status =
                    "Push/Pull requires selected faces in the active edit context.".to_owned();
                return;
            }
            self.push_pull_faces = faces;
            self.push_pull_owner = Some(owner);
            self.push_pull_occurrence = Some(occurrence);
            self.push_pull_reference = Some(point);
            self.push_pull_pointer = Some(point);
            self.status =
                "Push/Pull: pick extrusion point or Ctrl+N for a signed distance.".to_owned();
            return;
        };
        self.push_pull_pointer = Some(point);
        if let Err(error) = self.commit_push_pull(point - reference) {
            self.status = error;
        }
    }

    fn commit_push_pull(&mut self, displacement: Vec3) -> Result<(), String> {
        let source_faces = self
            .push_pull_faces
            .iter()
            .map(|(_, face)| *face)
            .collect::<Vec<_>>();
        let mesh = extrude_region(&source_faces, displacement, FACE_OUTLINE_MERGE_TOLERANCE)?;
        let owner = self
            .push_pull_owner
            .ok_or_else(|| "Push/Pull has no active geometry owner".to_owned())?;
        let occurrence = self
            .push_pull_occurrence
            .clone()
            .ok_or_else(|| "Push/Pull has no active occurrence".to_owned())?;
        let inverse = self
            .document
            .resolve_occurrence(&occurrence)
            .and_then(|resolved| {
                resolved
                    .world_transform
                    .inverse(GeometryTolerance::default())
            })
            .ok_or_else(|| "the active geometry transform cannot be inverted".to_owned())?;
        let vertices = mesh
            .vertices
            .iter()
            .map(|point| inverse.transform_point(*point))
            .collect::<Vec<_>>();
        let segments = mesh
            .segments
            .iter()
            .map(|segment| (segment.endpoints, segment.color))
            .collect::<Vec<_>>();
        let triangles = mesh
            .triangles
            .iter()
            .map(|triangle| (triangle.vertices, triangle.color))
            .collect::<Vec<_>>();
        let reverse_source_faces = self
            .push_pull_faces
            .iter()
            .filter_map(|(id, face)| {
                let [a, b, c] = face.vertices;
                ((b - a).cross(c - a).dot(displacement) > 0.0).then_some(*id)
            })
            .collect::<Vec<_>>();
        let edge_count = segments.len();
        let face_count = triangles.len();
        let inserted = self.commit_document_edit("Push/Pull", |document| match owner {
            LineOwner::SceneRoot => {
                document.reverse_triangle_winding_in_scene_root(&reverse_source_faces)?;
                document.append_colored_indexed_mesh_to_scene_root(vertices, segments, triangles)
            }
            LineOwner::Definition(definition) => {
                document
                    .reverse_triangle_winding_in_definition(definition, &reverse_source_faces)?;
                document.append_colored_indexed_mesh_to_definition(
                    definition, vertices, segments, triangles,
                )
            }
        })?;
        let (segment_ids, triangle_ids) = inserted;
        let targets = segment_ids
            .into_iter()
            .map(|segment| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(segment),
            })
            .chain(triangle_ids.into_iter().map(|triangle| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(triangle),
            }))
            .collect::<Vec<_>>();
        self.selection.apply(targets, SelectionMode::Replace);
        let distance = self
            .document
            .model_settings()
            .display_length(displacement.length());
        let unit_name = self.document.model_settings().unit_name.clone();
        self.clear_push_pull_transient();
        self.status = format!(
            "Push/Pull created {face_count} face(s) and {edge_count} edge(s) at {distance:.2} {unit_name}."
        );
        Ok(())
    }

    fn finish_push_pull_distance(&mut self) -> CommandResult {
        if !self.push_pull_distance_focused {
            return CommandResult::failed("Push/Pull numeric input is not active.");
        }
        let distance = match self.push_pull_numeric_input.parse::<f64>() {
            Ok(distance) if distance.is_finite() => distance,
            _ => return CommandResult::failed("Enter a finite signed Push/Pull distance."),
        };
        let Some(reference) = self.push_pull_reference else {
            return CommandResult::failed("Pick a Push/Pull reference point first.");
        };
        let Some(pointer) = self.push_pull_pointer else {
            return CommandResult::failed("Move the pointer to set Push/Pull direction first.");
        };
        let Some(direction) = (pointer - reference)
            .try_normalized(GeometryTolerance::default().linear_at(reference.length()))
        else {
            return CommandResult::failed(
                "Move the pointer away from the reference to set direction.",
            );
        };
        match self.commit_push_pull(direction * distance) {
            Ok(()) => CommandResult::completed(),
            Err(error) => CommandResult::failed(error),
        }
    }

    fn finish_polyline(&mut self, closed: bool) -> CommandResult {
        let closed = closed || self.active_tool == "Freeform Slab";
        let previous = self.polyline_tool_state.clone();
        let path = if closed {
            self.polyline_tool_state.close()
        } else {
            self.polyline_tool_state.finish()
        };
        let Some(path) = path else {
            return CommandResult::failed(if closed {
                "A closed polyline needs at least three points."
            } else {
                "Polyline needs at least two points to finish."
            });
        };
        let result = match self.active_tool.as_str() {
            "Freeform Slab" => self.commit_arch_slab_path(path),
            "Extrude Swipe" => self.commit_extrude_swipe_path(path),
            "Extrude Swipe 3D" | "Ribbon 3D" => self.commit_spatial_sweep_path(path),
            _ => self.commit_polyline_path(path),
        };
        if result.completion == CommandCompletion::Completed
            && matches!(
                self.active_tool.as_str(),
                "Extrude Swipe" | "Extrude Swipe 3D" | "Ribbon 3D" | "Freeform Slab"
            )
        {
            let status = self.status.clone();
            self.activate_tool("Select");
            self.status = status;
            return result;
        }
        if result.completion == CommandCompletion::Failed {
            self.polyline_tool_state = previous;
            let tool_name = match self.active_tool.as_str() {
                "Double Line" => "Double Line",
                "Extrude Swipe" => "Extrude Swipe",
                "Extrude Swipe 3D" => "Extrude Swipe 3D",
                "Ribbon 3D" => "Ribbon 3D",
                _ => "Polyline",
            };
            self.status = result
                .diagnostics
                .first()
                .cloned()
                .unwrap_or_else(|| format!("{tool_name} could not be created."));
        }
        result
    }

    fn commit_polyline_path(&mut self, path: PolylinePath) -> CommandResult {
        if self.active_tool == "Double Line" {
            return self.commit_double_line_path(path);
        }
        let mesh = match build_polyline_mesh(&path) {
            Ok(mesh) => mesh,
            Err(error) => return CommandResult::failed(error),
        };
        match self.create_planar_shape_in_active_context(mesh, "Polyline") {
            Ok(targets) => {
                self.selection.apply(targets, SelectionMode::Replace);
                self.status = format!(
                    "Created {} polyline point(s){} in the active geometry graph.",
                    path.points.len(),
                    if path.closed { " as a closed mesh" } else { "" }
                );
                CommandResult::completed()
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn selected_sweep_profile(
        &self,
    ) -> Result<(LineOwner, OccurrenceRef, Affine3, Vec<SweepProfileSegment>), String> {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let geometry = self
            .document
            .geometry_in_context(&occurrence, &self.selection.context)
            .ok_or_else(|| "Extrude Swipe needs an active geometry graph.".to_owned())?;
        let profile = self
            .selection
            .selected()
            .iter()
            .filter(|target| target.occurrence == occurrence)
            .filter_map(|target| {
                let SelectedEntity::Segment(id) = target.entity else {
                    return None;
                };
                let segment = geometry.segments.get(&id)?;
                let [Some(start), Some(end)] = segment
                    .endpoints
                    .map(|reference| geometry.vertex_position(reference))
                else {
                    return None;
                };
                Some(SweepProfileSegment {
                    endpoints: [start, end],
                })
            })
            .collect::<Vec<_>>();
        if profile.is_empty() {
            return Err(
                "Select one or more profile edges in the active geometry graph first.".to_owned(),
            );
        }
        Ok((owner, occurrence, inverse, profile))
    }

    fn build_extrude_swipe_mesh(&self, path: &PolylinePath) -> Result<SweepMesh, String> {
        let (_, _, inverse, profile) = self.selected_sweep_profile()?;
        let local_path = path
            .points
            .iter()
            .map(|point| inverse.transform_point(*point))
            .collect::<Vec<_>>();
        let scale = profile
            .iter()
            .flat_map(|segment| segment.endpoints)
            .chain(local_path.iter().copied())
            .map(Vec3::length)
            .fold(0.0, f64::max);
        let tolerance = GeometryTolerance::default().linear_at(scale);
        let local_up = inverse
            .transform_vector(Vec3::Y)
            .try_normalized(tolerance)
            .unwrap_or(Vec3::Y);
        modeling_sweep::extrude_swipe(&profile, &local_path, path.closed, local_up, tolerance)
            .map_err(str::to_owned)
    }

    fn build_spatial_sweep(
        &self,
        path: &PolylinePath,
    ) -> Result<(LineOwner, OccurrenceRef, Affine3, SweepMesh), String> {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let local_path = path
            .points
            .iter()
            .map(|point| inverse.transform_point(*point))
            .collect::<Vec<_>>();
        let settings = self.document.model_settings();
        let (profile, scale) = if self.active_tool == "Extrude Swipe 3D" {
            let (_, _, _, profile) = self.selected_sweep_profile()?;
            let scale = profile
                .iter()
                .flat_map(|segment| segment.endpoints)
                .chain(local_path.iter().copied())
                .map(Vec3::length)
                .fold(0.0, f64::max);
            (Some(profile), scale)
        } else {
            let scale = local_path
                .iter()
                .map(|point| point.length())
                .fold(0.0, f64::max);
            (None, scale)
        };
        let tolerance = GeometryTolerance::default().linear_at(scale);
        let local_up = inverse
            .transform_vector(Vec3::Y)
            .try_normalized(tolerance)
            .unwrap_or(Vec3::Y);
        let mesh = if let Some(profile) = profile {
            modeling_sweep::extrude_swipe_3d(
                &profile,
                &local_path,
                path.closed,
                local_up,
                tolerance,
            )
        } else {
            modeling_sweep::ribbon_3d(
                &local_path,
                path.closed,
                settings.double_line_size,
                settings.double_line_offset,
                local_up,
                tolerance,
            )
        }
        .map_err(str::to_owned)?;
        Ok((owner, occurrence, inverse, mesh))
    }

    fn build_revolve_mesh(
        &self,
        input: RevolveInput,
    ) -> Result<(LineOwner, OccurrenceRef, Affine3, SweepMesh), String> {
        let (owner, occurrence, inverse, profile) = self.selected_sweep_profile()?;
        let axis_origin = inverse.transform_point(input.axis_origin);
        let axis_end = inverse.transform_point(input.axis_origin + input.axis_direction);
        let axis_direction = axis_end - axis_origin;
        let scale = profile
            .iter()
            .flat_map(|segment| segment.endpoints)
            .chain([axis_origin, axis_end])
            .map(Vec3::length)
            .fold(0.0, f64::max);
        let tolerance = GeometryTolerance::default().linear_at(scale);
        let mesh = modeling_revolve::revolve(
            &profile,
            axis_origin,
            axis_direction,
            input.angle_radians,
            self.document
                .model_settings()
                .circle_segments
                .clamp(3, 4096) as usize,
            tolerance,
        )
        .map_err(str::to_owned)?;
        Ok((owner, occurrence, inverse, mesh))
    }

    fn revolve_preview(&self) -> Option<ToolPreview> {
        let input = self.revolve_tool_state.input().ok()?;
        let (_, occurrence, inverse, mesh) = self.build_revolve_mesh(input).ok()?;
        let world_transform = self
            .document
            .resolve_occurrence(&occurrence)
            .map(|resolved| resolved.world_transform)
            .or_else(|| inverse.inverse(GeometryTolerance::default()))?;
        let edges = mesh
            .segments
            .iter()
            .map(|[start, end]| {
                [
                    world_transform.transform_point(mesh.vertices[*start]),
                    world_transform.transform_point(mesh.vertices[*end]),
                ]
            })
            .collect();
        let triangles = mesh
            .triangles
            .iter()
            .map(|triangle| {
                (
                    triangle
                        .vertices
                        .map(|index| world_transform.transform_point(mesh.vertices[index])),
                    self.current_paint_color,
                )
            })
            .collect();
        Some(ToolPreview::Primitive { edges, triangles })
    }

    fn finish_revolve(&mut self) -> CommandResult {
        let input = match self.revolve_tool_state.input() {
            Ok(input) => input,
            Err(error) => return CommandResult::failed(error),
        };
        let (owner, occurrence, _, mesh) = match self.build_revolve_mesh(input) {
            Ok(result) => result,
            Err(error) => return CommandResult::failed(error),
        };
        let result = self.commit_sweep_mesh(owner, occurrence, mesh, "Revolve");
        if result.completion == CommandCompletion::Completed {
            let status = self.status.clone();
            self.activate_tool("Select");
            self.status = status;
        }
        result
    }

    fn spatial_sweep_preview(&self, path: Option<&PolylinePath>) -> Option<ToolPreview> {
        let (_, _, inverse) = self.line_geometry_context();
        let world_transform = inverse.inverse(GeometryTolerance::default())?;
        let color = self.current_face_color();
        if let Some(path) = path
            && let Ok((_, _, _, mesh)) = self.build_spatial_sweep(path)
        {
            let edges = mesh
                .segments
                .iter()
                .map(|[start, end]| {
                    [
                        world_transform.transform_point(mesh.vertices[*start]),
                        world_transform.transform_point(mesh.vertices[*end]),
                    ]
                })
                .collect();
            let triangles = mesh
                .triangles
                .iter()
                .map(|triangle| {
                    (
                        triangle
                            .vertices
                            .map(|index| world_transform.transform_point(mesh.vertices[index])),
                        color,
                    )
                })
                .collect();
            return Some(ToolPreview::Primitive { edges, triangles });
        }

        let mut edges = if self.active_tool == "Extrude Swipe 3D" {
            let (_, _, _, profile) = self.selected_sweep_profile().ok()?;
            profile
                .iter()
                .map(|segment| {
                    segment
                        .endpoints
                        .map(|point| world_transform.transform_point(point))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if let Some(path) = path {
            edges.extend(path.points.windows(2).map(|pair| [pair[0], pair[1]]));
            if path.closed && path.points.len() > 2 {
                edges.push([*path.points.last()?, path.points[0]]);
            }
        }
        if edges.is_empty() {
            return None;
        }
        Some(ToolPreview::Primitive {
            edges,
            triangles: Vec::new(),
        })
    }

    fn extrude_swipe_preview(&self, path: Option<&PolylinePath>) -> Option<ToolPreview> {
        let (_, _, inverse, profile) = self.selected_sweep_profile().ok()?;
        let world_transform = inverse.inverse(GeometryTolerance::default())?;
        let color = self.current_face_color();
        if let Some(path) = path
            && let Ok(mesh) = self.build_extrude_swipe_mesh(path)
        {
            let edges = mesh
                .segments
                .iter()
                .map(|[start, end]| {
                    [
                        world_transform.transform_point(mesh.vertices[*start]),
                        world_transform.transform_point(mesh.vertices[*end]),
                    ]
                })
                .collect();
            let triangles = mesh
                .triangles
                .iter()
                .map(|triangle| {
                    (
                        triangle
                            .vertices
                            .map(|index| world_transform.transform_point(mesh.vertices[index])),
                        color,
                    )
                })
                .collect();
            return Some(ToolPreview::Primitive { edges, triangles });
        }

        let mut edges = profile
            .iter()
            .map(|segment| {
                segment
                    .endpoints
                    .map(|point| world_transform.transform_point(point))
            })
            .collect::<Vec<_>>();
        if let Some(path) = path {
            edges.extend(path.points.windows(2).map(|pair| [pair[0], pair[1]]));
            if path.closed && path.points.len() > 2 {
                edges.push([*path.points.last()?, path.points[0]]);
            }
        }
        Some(ToolPreview::Primitive {
            edges,
            triangles: Vec::new(),
        })
    }

    fn commit_extrude_swipe_path(&mut self, path: PolylinePath) -> CommandResult {
        let (owner, occurrence, _, _) = match self.selected_sweep_profile() {
            Ok(input) => input,
            Err(error) => return CommandResult::failed(error),
        };
        let mesh = match self.build_extrude_swipe_mesh(&path) {
            Ok(mesh) => mesh,
            Err(error) => return CommandResult::failed(error),
        };
        self.commit_sweep_mesh(owner, occurrence, mesh, "Extrude Swipe")
    }

    fn commit_spatial_sweep_path(&mut self, path: PolylinePath) -> CommandResult {
        let label = self.active_tool.clone();
        let (owner, occurrence, _, mesh) = match self.build_spatial_sweep(&path) {
            Ok(result) => result,
            Err(error) => return CommandResult::failed(error),
        };
        self.commit_sweep_mesh(owner, occurrence, mesh, &label)
    }

    fn commit_sweep_mesh(
        &mut self,
        owner: LineOwner,
        occurrence: OccurrenceRef,
        mesh: SweepMesh,
        label: &str,
    ) -> CommandResult {
        let color = self.current_face_color();
        let line_color = self.current_outline_color();
        let inserted = self.commit_document_edit(label, |document| match owner {
            LineOwner::SceneRoot => document.append_colored_indexed_mesh_to_scene_root(
                mesh.vertices,
                mesh.segments
                    .into_iter()
                    .map(|segment| (segment, line_color))
                    .collect(),
                mesh.triangles
                    .into_iter()
                    .map(|triangle| (triangle.vertices, color))
                    .collect(),
            ),
            LineOwner::Definition(definition) => document
                .append_colored_indexed_mesh_to_definition(
                    definition,
                    mesh.vertices,
                    mesh.segments
                        .into_iter()
                        .map(|segment| (segment, line_color))
                        .collect(),
                    mesh.triangles
                        .into_iter()
                        .map(|triangle| (triangle.vertices, color))
                        .collect(),
                ),
        });
        match inserted {
            Ok((segments, triangles)) => {
                let targets = segments
                    .into_iter()
                    .map(|id| SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Segment(id),
                    })
                    .chain(triangles.into_iter().map(|id| SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Triangle(id),
                    }))
                    .collect::<Vec<_>>();
                self.selection.apply(targets, SelectionMode::Replace);
                self.status =
                    format!("{label} created a connected surface in the active geometry graph.");
                CommandResult::completed()
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn commit_double_line_path(&mut self, path: PolylinePath) -> CommandResult {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        let local_path = PolylinePath {
            points: path
                .points
                .iter()
                .map(|point| inverse.transform_point(*point))
                .collect(),
            closed: path.closed,
        };
        let settings = self.document.model_settings();
        let local_plane_normal = inverse
            .transform_vector(self.viewport.construction_plane_normal())
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::Y);
        let mesh = match build_double_line_mesh_in_plane(
            &local_path,
            settings.double_line_size,
            settings.double_line_offset,
            local_plane_normal,
        ) {
            Ok(mesh) => mesh,
            Err(error) => return CommandResult::failed(error),
        };
        match self.commit_local_planar_mesh(
            owner,
            occurrence.clone(),
            mesh.vertices,
            mesh.segments,
            mesh.triangles,
            "Double Line",
        ) {
            Ok(targets) => {
                self.selection.apply(targets, SelectionMode::Replace);
                self.status = format!(
                    "Created double-line ribbon from {} point(s) in the active geometry graph.",
                    path.points.len()
                );
                CommandResult::completed()
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    pub fn native_normalize_selection_target(
        &self,
        mut target: SelectionTarget,
    ) -> SelectionTarget {
        if target.entity == SelectedEntity::Occurrence {
            if let Some(frame) = self
                .document
                .frame_target_for_occurrence(&target.occurrence)
            {
                target = frame;
            }
        } else if let Some(g) = self.document.effective_geometry(&target.occurrence) {
            let ext = match target.entity {
                SelectedEntity::Segment(id) => g.segments.get(&id).map(|s| &s.extensions),
                SelectedEntity::Triangle(id) => g.triangles.get(&id).map(|f| &f.extensions),
                _ => None,
            };
            if let Some(entity) =
                ext.and_then(meshcraft_core::document::architecture::semantic_entity)
            {
                target.entity = entity;
            }
        }
        if let EditContext::Definition(id) = self.selection.context
            && self
                .document
                .resolve_occurrence(&target.occurrence)
                .is_some_and(|r| r.definition.id == id)
        {
            return target;
        }
        let depth = match &self.selection.context {
            EditContext::Occurrence(active) => active.path.len(),
            _ if self
                .document
                .scene_container_occurrence()
                .is_some_and(|p| target.occurrence.path.starts_with(&p.path)) =>
            {
                1
            }
            _ => 0,
        };
        for len in depth + 1..=target.occurrence.path.len() {
            let path = OccurrenceRef {
                path: target.occurrence.path[..len].to_vec(),
            };
            if self.document.is_object_occurrence(&path) {
                return SelectionTarget {
                    occurrence: path,
                    entity: SelectedEntity::Occurrence,
                };
            }
        }
        target
    }

    pub fn native_target_editable(&self, target: &SelectionTarget) -> bool {
        if self.document.is_baked_gpu_object(&target.occurrence)
            && target.entity != SelectedEntity::Occurrence
        {
            return false;
        }
        // Generated architecture facets are selected/edited through their canonical owner.
        if let Some(geometry) = self.document.effective_geometry(&target.occurrence) {
            let generated = match target.entity {
                SelectedEntity::Segment(id) => geometry.segments.get(&id).is_some_and(|e| {
                    meshcraft_core::document::architecture::generated(&e.extensions)
                }),
                SelectedEntity::Triangle(id) => geometry.triangles.get(&id).is_some_and(|e| {
                    meshcraft_core::document::architecture::generated(&e.extensions)
                }),
                SelectedEntity::Vertex(id) => geometry.vertices.get(&id).is_some_and(|v| {
                    v.legacy_aliases
                        .iter()
                        .any(|a| a.source == meshcraft_core::document::architecture::GENERATED)
                }),
                _ => false,
            };
            if generated {
                return false;
            }
        }
        if !self
            .selection_display_policy
            .can_edit_target(&self.document, target)
        {
            return false;
        }
        match &self.selection.context {
            EditContext::Document => true,
            EditContext::Occurrence(active) => target.occurrence.path.starts_with(&active.path),
            EditContext::Definition(id) => self
                .document
                .resolve_occurrence(&target.occurrence)
                .is_some_and(|resolved| resolved.definition.id == *id),
        }
    }

    fn exit_object_edit(&mut self) {
        let parent = match &self.selection.context {
            EditContext::Occurrence(active) if active.path.len() > 1 => {
                EditContext::Occurrence(OccurrenceRef {
                    path: active.path[..active.path.len() - 1].to_vec(),
                })
            }
            _ => EditContext::Document,
        };
        // Legacy files wrap the scene in a placement; it is not another object-edit level.
        let parent = if matches!(&parent, EditContext::Occurrence(path)
            if self.document.scene_container_occurrence().as_ref() == Some(path))
        {
            EditContext::Document
        } else {
            parent
        };
        self.selection.set_context(parent);
        self.selection_click_tracker.reset();
        self.status = "Finished editing the object prototype.".to_owned();
    }

    pub fn native_active_object(&self) -> Option<(DefinitionId, String, bool)> {
        if let EditContext::Occurrence(active) = &self.selection.context {
            return self
                .document
                .resolve_occurrence(active)
                .map(|r| (r.definition.id, r.definition.name.clone(), true));
        }
        if let Some(path) = self.selected_occurrences().first() {
            return self
                .document
                .resolve_occurrence(path)
                .map(|r| (r.definition.id, r.definition.name.clone(), false));
        }
        self.native_selected_prototype()
            .and_then(|id| self.document.definition(id))
            .map(|d| (d.id, d.name.clone(), false))
    }

    fn commit_document_edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        self.document.persist_script_states();
        let context = self.selection.context.clone();
        let (result, _) = self
            .history
            .transact(&mut self.document, label, |draft| {
                // Scene promotion only does work while imported root overrides remain.
                // The transaction already isolates the edit and validates the result.
                let promote_scene = matches!(context, EditContext::Document)
                    && draft
                        .scene_container_occurrence()
                        .and_then(|path| {
                            draft.resolve_occurrence(&path).map(|r| {
                                r.edits.is_some_and(|e| {
                                    e.geometry_replacement.is_some() || e.child_layout.is_some()
                                })
                            })
                        })
                        .unwrap_or(false);
                let result = match &context {
                    EditContext::Occurrence(active) => draft.edit_prototype(active, operation),
                    EditContext::Document if promote_scene => draft.edit_scene(operation),
                    _ => operation(draft),
                }?;
                draft.normalize_material_bindings()?;
                Ok(result)
            })
            .map_err(|error| error.to_string())?;
        if matches!(self.selection.context, EditContext::Occurrence(_)) {
            let view = self.native_document_for_view();
            self.selection.retain_valid(&view);
        } else {
            self.selection.retain_valid(&self.document);
        }
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

    fn prune_uneditable_selection(&mut self) {
        let retained = self
            .selection
            .ordered_selected()
            .iter()
            .map(|target| {
                if target.entity == SelectedEntity::Occurrence {
                    self.document
                        .frame_target_for_occurrence(&target.occurrence)
                        .unwrap_or_else(|| target.clone())
                } else {
                    target.clone()
                }
            })
            .filter(|target| {
                self.selection_display_policy
                    .can_edit_target(&self.document, target)
            })
            .collect::<Vec<_>>();
        self.selection.apply(retained, SelectionMode::Replace);
    }

    pub fn dispatch_command(&mut self, id: &str, payload: &CommandPayload) -> CommandDispatch {
        self.prune_uneditable_selection();
        let state = self.query_command_state(id);
        let before_revision = self.document.revision();
        let outcome = shell_commands().dispatch(id, payload, &state, |action, payload| {
            let mut result = self.execute_command(*action, payload);
            if result.document_revision.is_none() && before_revision != self.document.revision() {
                result.document_revision = Some(self.document.revision());
            }
            result
        });
        self.prune_uneditable_selection();
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
                self.reset_autosave_after_save(Instant::now());
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
    fn reset_autosave_after_save(&mut self, now: Instant) {
        self.autosave_observed_revision = self.document.revision();
        self.autosave_deadline = if self.document_is_dirty() && self.current_project_path.is_some()
        {
            Some(now + Duration::from_millis(1500))
        } else {
            None
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_autosave(&mut self, now: Instant) -> Option<Duration> {
        // Isolated performance runs exclude background serialization from tool timings.
        if std::env::var_os("MESHCRAFT_DISABLE_AUTOSAVE").is_some() {
            return None;
        }
        let revision = self.document.revision();
        if revision != self.autosave_observed_revision {
            self.autosave_observed_revision = revision;
            self.autosave_deadline =
                if self.document_is_dirty() && self.current_project_path.is_some() {
                    Some(now + Duration::from_millis(1500))
                } else {
                    None
                };
        }

        if !self.document_is_dirty() {
            self.autosave_deadline = None;
            return None;
        }

        let due = self
            .autosave_deadline
            .is_some_and(|deadline| now >= deadline);
        if due
            && self.current_project_path.is_some()
            && self.pending_project_load.is_none()
            && self.pending_project_save.is_none()
            && self.pending_replacement.is_none()
        {
            self.autosave_deadline = None;
            let _ = self.save_project_native(false, &CommandPayload::default());
        }

        self.autosave_deadline
            .map(|deadline| deadline.saturating_duration_since(now))
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
        let recent_error = self.remember_recent_project(path).err();
        self.document = project.document;
        let _ = self.document.normalize_material_bindings();
        self.current_paint_material = None;
        self.current_paint_color = Rgba8::ANSI_GREY;
        self.viewport.invalidate_document_caches();
        self.document_generation = self.document_generation.wrapping_add(1);
        self.clipboard.placement = None;
        self.numeric_entry = Default::default();
        self.saved_revision = self.document.revision();
        self.autosave_observed_revision = self.document.revision();
        self.autosave_deadline = None;
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
        self.tool_cycle_state.cancel_hotspot_placement();
        self.active_tool = "Select".to_owned();
        self.guides.clear();
        self.cursor_snap = None;
        self.cuboid_first = None;
        self.drawing_tool
            .handle(ToolEvent::Activate(DrawingTool::Select));
        self.shape_tool_state.clear_picks();
        self.primitive_tool_state.cancel();
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

        if let Some(error) = &recent_error {
            self.status
                .push_str(&format!(" Recent files could not be saved: {error}"));
        }
        let mut result = CommandResult::completed();
        result.diagnostics.extend(recent_error);
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

    #[cfg(not(target_arch = "wasm32"))]
    fn remember_recent_project(&mut self, path: &Path) -> Result<(), String> {
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.preferences
            .record_recent_project(path.display().to_string());
        if self.persist_native_preferences {
            persist_native_preferences(&self.preferences)?;
        }
        Ok(())
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
        self.cancel_active_tool();
        if let Err(error) = self.commit_document_edit("New document", |document| {
            *document = Document::new();
            Ok(())
        }) {
            return CommandResult::failed(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.current_project_path = None;
            self.autosave_deadline = None;
            self.autosave_observed_revision = self.document.revision();
        }
        self.current_paint_material = None;
        self.current_paint_color = Rgba8::ANSI_GREY;
        self.saved_revision = self.document.revision();
        self.document_generation = self.document_generation.wrapping_add(1);
        self.selection.clear();
        self.object_name_edit = None;
        self.tool_cycle_state.cancel_hotspot_placement();
        self.cuboid_first = None;
        self.drawing_tool
            .handle(ToolEvent::Activate(DrawingTool::Select));
        self.shape_tool_state.clear_picks();
        self.primitive_tool_state.cancel();
        self.guides.clear();
        self.cursor_snap = None;
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
        egui::Modal::new(egui::Id::new("document-replacement-confirmation")).show(context, |ui| {
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
        });
        if let Some(decision) = decision {
            self.resolve_document_replacement(decision);
        }
    }

    fn activate_tool(&mut self, tool: &str) {
        self.numeric_entry = Default::default();
        self.clipboard.placement = None;
        self.annotations.points.clear();
        let contour = self.architecture.stair_contour.take();
        self.architecture = Default::default();
        if matches!(tool, "Select" | "Stair") {
            self.architecture.stair_contour = contour;
        }
        self.section = Default::default();
        if matches!(tool, "Linear Dimension" | "Text" | "Leader") {
            self.panel_focus = Some("annotations".into());
        }
        self.tool_cycle_state.cancel_hotspot_placement();
        self.hotspot_reference_pick = false;
        self.object_place_tool_state.cancel();
        self.placement_preview = Default::default();
        self.array_tool_state
            .activate(ArrayToolKind::from_label(tool));
        self.rotational_array_tool_state
            .activate(RotationalArrayKind::from_label(tool));
        self.array_preview = false;
        self.clear_push_pull_transient();
        self.active_tool = tool.to_owned();
        if tool == "Orbit" {
            self.viewport.set_perspective_view();
        }
        if self.preferences.auto_collapse_toolbars {
            self.preferences.toolbars_visible = false;
        }
        self.cuboid_first = None;
        self.polyline_tool_state.activate(matches!(
            tool,
            "Polyline"
                | "Double Line"
                | "Extrude Swipe"
                | "Extrude Swipe 3D"
                | "Ribbon 3D"
                | "Freeform Slab"
        ));
        if matches!(tool, "Extrude Swipe 3D" | "Ribbon 3D") {
            self.polyline_tool_state.set_mode(PolylineSegmentMode::Line);
        }
        self.mesh_tool_state.activate(tool == "Mesh");
        self.line_offset_tool_state.activate(tool == "Line Offset");
        self.drawing_tool.handle(ToolEvent::Activate(match tool {
            "Line" => DrawingTool::Line,
            "Construction Line" => DrawingTool::ConstructionLine,
            _ => DrawingTool::Select,
        }));
        self.shape_tool_state
            .activate(PlanarShapeTool::from_label(tool));
        self.primitive_tool_state
            .activate(PrimitiveKind::from_label(tool));
        self.revolve_tool_state.activate(tool == "Revolve");
        self.transform_tool_state
            .activate(TransformTool::from_label(tool));
        self.status = match tool {
            "Cuboid" => "Cuboid: pick two opposite 3D corners.".to_owned(),
            "Line" => {
                "Line: click a start and end point in the active construction plane.".to_owned()
            }
            "Polyline" => {
                "Polyline: click to start; A selects three-point arc, C center arc, L line. Enter finishes.".to_owned()
            }
            "Double Line" => {
                "Double Line: click a center path; A/C/L switch arc modes, Enter finishes the ribbon.".to_owned()
            }
            "Extrude Swipe" => {
                "Extrude Swipe: select profile edges, then pick a path; A/C/L switch arc modes, Enter finishes.".to_owned()
            }
            "Extrude Swipe 3D" => {
                "Extrude Swipe 3D: select profile edges, pick spatial path points, then press Enter.".to_owned()
            }
            "Ribbon 3D" => {
                "Ribbon 3D: pick spatial path points; width and offset use Polyline Settings. Enter finishes.".to_owned()
            }
            "Revolve" => {
                "Revolve: select profile edges, pick axis origin and direction, then type an angle or press Enter for 360°. Resolution uses Circle Segments.".to_owned()
            }
            "Mesh" => "Mesh: pick points to add connecting edges and nearest-point triangles. Enter starts a new mesh chain; Escape exits.".to_owned(),
            "Line Offset" => "Line Offset: select edges, then pick a reference point and target point.".to_owned(),
            "Push/Pull" => "Push/Pull: select faces, pick a reference point and extrusion point; Ctrl+N accepts a signed distance.".to_owned(),
            "Construction Line" => {
                "Construction Line: click two points; the tool stays active for another segment."
                    .to_owned()
            }
            "Rectangle" => {
                "Rectangle: click two opposite corners in the active construction plane.".to_owned()
            }
            "Surface Rect" => {
                "Surface Rect: pick a surface point, then the opposite rectangle corner.".to_owned()
            }
            "Quad" => "Quad: pick the origin, second point, then third point.".to_owned(),
            "Circle" => "Circle: pick a center, then a radius point.".to_owned(),
            "Sphere" => "Sphere: pick a center, then a radius point.".to_owned(),
            "Cylinder" => "Cylinder: pick a base center, radius point, then height point.".to_owned(),
            "Cone" => "Cone: pick a base center, radius point, then apex point.".to_owned(),
            "Pill" => "Pill: pick a base center, radius point, then top hemisphere center.".to_owned(),
            "Move" => "Move: pick a reference point, then its destination.".to_owned(),
            "Rotate" => {
                "Rotate: pick center, axis point, reference point, then final point.".to_owned()
            }
            "Rotate 2" => {
                "Rotate 2: pick a center (C), first reference (A), then the end angle point (B). Toggle copy mode to preserve the source.".to_owned()
            }
            "Scale" => "Scale: pick center, reference point, then final point.".to_owned(),
            "Squash" => "Squash: pick center, reference axis, then target distance; enter a signed factor for numeric input.".to_owned(),
            "Stretch" => "Stretch: pick a reference point, then its destination.".to_owned(),
            "Stretch Scale" => {
                "Stretch Scale: pick center, reference point, then final point.".to_owned()
            }
            "Rotate Stretch" => {
                "Rotate Stretch: pick center, axis point, reference point, then final point."
                    .to_owned()
            }
            "Paint" => "Paint: click an edge or face, or apply the current color to a selection."
                .to_owned(),
            "Copy Multiple" => "Copy Multiple: pick reference point (U), size point (V), then span point (D). Copies advance along U→V and stop before passing D.".to_owned(),
            "Planar Rotate Multiple" => "Planar Rotate Multiple: pick center (C), reference (U), angular increment (V), then sweep end (W). Copies rotate the current selection by each fixed signed increment.".to_owned(),
            "Helicoidal Rotate Multiple" => "Helicoidal Rotate Multiple: pick center (C), reference (U), angular increment (V), then sweep/lift end (W). Copies follow the fixed rotation step and share the measured axial lift.".to_owned(),
            "Rotational Array" => "Rotational Array: pick center (C), first reference (A), increment (B), then sweep end (D). Copies rotate the current selection by each fixed signed increment.".to_owned(),
            "Helical Array" => "Helical Array: pick center (C), first reference (A), helical unit (B), then sweep/height end (D). Copies use a fixed Y-axis angle and lift step.".to_owned(),
            "Rotational Array Even" => "Rotational Array Even: pick center (C), reference (A), angular hint (B), then a full or partial sweep end (D). Full circles use selection span to equalize angular spacing.".to_owned(),
            "Helical Array Even" => "Helical Array Even: pick center (C), reference (A), helical unit (B), then sweep/height end (D). Full turns use selection span to equalize angular spacing.".to_owned(),
            "Planar Translate Multiple" => "Planar Translate Multiple: pick origin (O), planar XZ step (A), then extent (B). Copies fill the measured XZ grid; the source cell is omitted.".to_owned(),
            "Volumetric Translate Multiple" => "Volumetric Translate Multiple: pick origin (O), XYZ step (A), then extent (B). Copies fill the measured volume; the source cell is omitted.".to_owned(),
            _ => format!("{tool} tool selected."),
        };
        if architecture_workflow::active(tool) {
            self.arch_begin();
        }
    }

    fn rotational_array_selection_span(&self) -> f64 {
        let Some(kind) = self.rotational_array_tool_state.kind() else {
            return 0.0;
        };
        if !matches!(
            kind,
            RotationalArrayKind::RotationalArrayEven | RotationalArrayKind::HelicalArrayEven
        ) {
            return 0.0;
        }
        let points = self.rotational_array_tool_state.points();
        if points.len() != 3 {
            return 0.0;
        }
        let center = points[0];
        let reference = points[1] - center;
        let increment = points[2] - center;
        let axis = if kind == RotationalArrayKind::HelicalArrayEven {
            Vec3::Y
        } else {
            let Some(axis) = reference.cross(increment).try_normalized(1.0e-5) else {
                return 0.0;
            };
            axis
        };
        let Some(tangent) = axis.cross(reference).try_normalized(1.0e-5) else {
            return 0.0;
        };
        modeling_copy::projected_selection_span(&self.document, &self.selection, tangent)
            .unwrap_or(0.0)
    }

    fn set_path_segment_key(&mut self, key: &str) -> bool {
        if !self.polyline_tool_state.active() {
            return false;
        }
        let (mode, label) = match key {
            "a" => (
                PolylineSegmentMode::ArcThrough,
                "arc through a picked point",
            ),
            "c" => (
                PolylineSegmentMode::ArcCenter,
                "circular arc from a picked center",
            ),
            "l" => (PolylineSegmentMode::Line, "line segment"),
            _ => return false,
        };
        self.polyline_tool_state.set_mode(mode);
        self.status = format!(
            "{}: {label}. Enter finishes; Esc cancels.",
            self.active_tool
        );
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn handle_native_tool_key(&mut self, key: &str, shift: bool) -> bool {
        if !self.numeric_entry.text.is_empty()
            && matches!(
                key,
                "delete" | "left" | "right" | "up" | "down" | "home" | "end"
            )
        {
            return true;
        }
        if key == "backspace" && self.backspace_tool_numeric() {
            return true;
        }
        if key.chars().count() == 1 && self.append_tool_numeric_text(key) {
            return true;
        }
        if key == "backspace"
            && matches!(
                self.active_tool.as_str(),
                "Linear Dimension" | "Text" | "Leader"
            )
        {
            self.annotations.points.pop();
            return true;
        }
        if !shift && self.set_path_segment_key(key) {
            return true;
        }
        let command = match key {
            "enter" => "tool.finish",
            "backspace" => "tool.undo_point",
            "l" if shift || !self.polyline_tool_state.active() => "tool.cycle.planar_construction",
            "d" if !shift => "tool.cycle.annotation",
            "o" if !shift => "tool.cycle.operations",
            "m" if !shift => "tool.cycle.modification",
            _ => return false,
        };
        self.run_command(command);
        true
    }

    pub fn native_point_input_hint(&self) -> &'static str {
        if self.active_tool == "Planar Section" {
            return if self.section.origin.is_some() {
                "Pick a point defining the section plane normal"
            } else {
                "Pick the section plane origin"
            };
        }
        if architecture_workflow::active(&self.active_tool) {
            return self.arch_hint();
        }
        if self.active_tool == "Paste" {
            return "Pick insertion point · Esc cancel";
        }
        if self.active_tool == "Linear Dimension" {
            match self.annotations.points.len() {
                0 => "Dimension: pick first endpoint",
                1 => "Dimension: pick second endpoint",
                _ => "Dimension: pick offset position",
            }
        } else if self.active_tool == "Leader" {
            match self.annotations.points.len() {
                0 => "Leader: pick arrow anchor",
                1 => "Leader: pick elbow",
                _ => "Leader: pick label position",
            }
        } else if self.active_tool == "Text" {
            "Text: set label in Annotations, then pick its position"
        } else if self.polyline_tool_state.active() {
            "Enter finish · A arc · C circular · L line · Shift+L next tool · Esc cancel"
        } else if self.mesh_tool_state.active() {
            "Enter finish chain · Backspace undo point · Esc cancel"
        } else {
            ""
        }
    }

    fn advance_tool_cycle(&mut self, cycle: ToolCycle) {
        match self.tool_cycle_state.advance(cycle, &self.active_tool) {
            ToolCycleEffect::ActivateTool(tool) => self.activate_tool(tool),
            ToolCycleEffect::BeginHotspotPlacement => {
                self.status =
                    "Hotspot placement host action active; click a point or press Escape."
                        .to_owned();
            }
        }
    }

    fn execute_command(&mut self, action: ShellAction, payload: &CommandPayload) -> CommandResult {
        match action {
            ShellAction::MaterialAction(action) => {
                if action == "object" {
                    let result = (|| {
                        let definition = payload
                            .values
                            .get("definition_id")
                            .map(|v| {
                                v.as_u64()
                                    .map(DefinitionId)
                                    .ok_or("definition_id must be an unsigned integer")
                            })
                            .transpose()?;
                        let instance = payload
                            .values
                            .get("instance_path")
                            .map(|v| {
                                serde_json::from_value::<Vec<LayoutEntryId>>(v.clone())
                                    .map(|path| OccurrenceRef { path })
                                    .map_err(|_| "instance_path must be an array of numeric IDs")
                            })
                            .transpose()?;
                        let definition = definition.or_else(|| {
                            instance.as_ref().and_then(|p| {
                                self.document.resolve_occurrence(p).map(|r| r.definition.id)
                            })
                        });
                        let settings = if let Some(v) = payload.values.get("settings") {
                            serde_json::from_value(v.clone())
                                .map_err(|_| "Invalid procedural object settings")?
                        } else {
                            let mut m = meshcraft_core::document::materials::Material::default();
                            meshcraft_core::document::procedural::apply_preset(
                                &mut m,
                                meshcraft_core::document::procedural::BRANCHING,
                            );
                            m
                        };
                        self.save_native_procedural_object(definition, instance, settings)
                    })();
                    return match result {
                        Ok(id) => {
                            let mut r = CommandResult::completed();
                            r.outputs.insert("definition_id".into(), json!(id.0));
                            r
                        }
                        Err(e) => CommandResult::failed(e),
                    };
                }
                let result = match action {
                    "program" => serde_json::from_value(
                        payload
                            .values
                            .get("program")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|p| self.save_native_procedural_program(p)),
                    "parameters" => serde_json::from_value(
                        payload
                            .values
                            .get("parameters")
                            .cloned()
                            .unwrap_or_else(|| json!({})),
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|p| {
                        self.assign_native_material_parameters(
                            payload
                                .values
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or(""),
                            Some(p),
                        )
                    }),
                    "apply" => self.assign_native_material(
                        payload
                            .values
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("ansi-grey"),
                    ),
                    "delete" => payload
                        .values
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| "Material ID required".to_owned())
                        .and_then(|id| self.delete_native_material(id)),
                    "save" => match payload.values.get("material") {
                        Some(value) => serde_json::from_value(value.clone())
                            .map_err(|e| e.to_string())
                            .and_then(|material| self.save_native_material(material)),
                        None => {
                            self.open_panels(&["material_editor"]);
                            Ok(())
                        }
                    },
                    _ => Err("unknown material action".into()),
                };
                return result
                    .map(|_| CommandResult::completed())
                    .unwrap_or_else(CommandResult::failed);
            }
            ShellAction::InterchangeAction(action) => {
                return self.execute_interchange(action, payload);
            }
            ShellAction::ObjectFileAction(action) => {
                return self.execute_object_file_command(action, payload);
            }
            ShellAction::HotspotAction(action) => {
                return self.execute_hotspot_command(action, payload);
            }
            ShellAction::SelectTool(tool) | ShellAction::SelectToolAlias(tool) => {
                self.activate_tool(tool);
            }
            ShellAction::CycleTool(cycle) => self.advance_tool_cycle(cycle),
            ShellAction::BeginHotspotPlacement => {
                self.tool_cycle_state.begin_hotspot_placement();
                self.status =
                    "Hotspot placement host action active; click a point or press Escape."
                        .to_owned();
            }
            ShellAction::CancelHotspotPlacement => {
                if !self.tool_cycle_state.cancel_hotspot_placement() {
                    return CommandResult::failed("hotspot placement is not active");
                }
                self.status = "Hotspot placement canceled.".to_owned();
            }
            ShellAction::FocusToolDistance => {
                if self.active_tool == "Push/Pull" {
                    if self.push_pull_reference.is_none() {
                        self.status = "Pick the Push/Pull reference point before entering a value."
                            .to_owned();
                    } else {
                        self.push_pull_distance_focused = true;
                        self.push_pull_numeric_input.clear();
                        self.status = "Push/Pull input: move the pointer for direction, type a signed distance, then press Enter.".to_owned();
                    }
                    return CommandResult::completed();
                }
                if self.revolve_tool_state.active()
                    && self.revolve_tool_state.axis_points().len() == 2
                {
                    self.revolve_tool_state.clear_numeric_input();
                    self.status =
                        "Revolve input: type a signed angle from 0° to 360°, then press Enter."
                            .to_owned();
                    return CommandResult::completed();
                }
                if self.transform_tool_state.active().is_some() {
                    if self.transform_tool_state.points().is_empty() {
                        self.status = "Pick the transform reference point before entering a value."
                            .to_owned();
                    } else {
                        self.status = "Transform input: type a distance, factor, or angle, move the pointer, then press Enter.".to_owned();
                    }
                    return CommandResult::completed();
                }
                if self.drawing_tool.anchor().is_none() {
                    self.status =
                        "Choose a Line start point before entering a distance.".to_owned();
                } else {
                    self.drawing_tool.handle(ToolEvent::FocusDistance);
                    self.status = "Distance input: type a value, move the pointer for direction, then press Enter.".to_owned();
                }
            }
            ShellAction::FinishToolInput => {
                if !self.numeric_entry.text.is_empty() {
                    return self.finish_numeric_entry();
                }
                if self.active_tool == "Wall" {
                    self.activate_tool("Select");
                    self.status = "Wall chain finished.".into();
                    return CommandResult::completed();
                }
                if self.transform_tool_state.active().is_some() {
                    return self.finish_transform_input();
                }
                if self.active_tool == "Push/Pull" && self.push_pull_distance_focused {
                    return self.finish_push_pull_distance();
                }
                if self.revolve_tool_state.active() {
                    return self.finish_revolve();
                }
                if self.mesh_tool_state.active() {
                    self.mesh_tool_state.finish_chain();
                    self.status = "Mesh chain finished; pick a point to start another.".to_owned();
                    return CommandResult::completed();
                }
                if matches!(
                    self.active_tool.as_str(),
                    "Polyline"
                        | "Double Line"
                        | "Extrude Swipe"
                        | "Extrude Swipe 3D"
                        | "Ribbon 3D"
                        | "Freeform Slab"
                ) {
                    return self.finish_polyline(false);
                }
                self.sync_drawing_tool_state();
                let effect = self.drawing_tool.handle(ToolEvent::Enter);
                self.handle_drawing_tool_effect(effect, None);
                if effect == ToolEffect::InvalidInput {
                    return CommandResult::failed(self.status.clone());
                }
            }
            ShellAction::UndoToolPoint => {
                if self.backspace_tool_numeric() {
                    return CommandResult::completed();
                }
                if architecture_workflow::active(&self.active_tool)
                    && self.active_tool != "Freeform Slab"
                {
                    self.architecture.points.pop();
                    self.status = self.arch_hint().into();
                    return CommandResult::completed();
                }
                if self.rotational_array_tool_state.active() {
                    if !self.rotational_array_tool_state.backspace() {
                        return CommandResult::failed("there is no rotational array point to undo");
                    }
                    self.status = "Removed the last rotational array point.".to_owned();
                    return CommandResult::completed();
                }
                if self.array_tool_state.active() {
                    if !self.array_tool_state.backspace() {
                        return CommandResult::failed(
                            "there is no translation array point to undo",
                        );
                    }
                    self.status = "Removed the last array tool point.".to_owned();
                    return CommandResult::completed();
                }
                if self.transform_tool_state.active().is_some() {
                    if !self.transform_tool_state.backspace() {
                        return CommandResult::failed("there is no transform input to undo");
                    }
                    self.status = "Removed the last transform input or point.".to_owned();
                    return CommandResult::completed();
                }
                if self.revolve_tool_state.active() {
                    if !self.revolve_tool_state.backspace() {
                        return CommandResult::failed(
                            "there is no Revolve input or axis point to undo",
                        );
                    }
                    self.status =
                        "Removed the last Revolve input character or axis point.".to_owned();
                    return CommandResult::completed();
                }
                if self.active_tool == "Push/Pull" && self.push_pull_distance_focused {
                    if self.push_pull_numeric_input.pop().is_none() {
                        return CommandResult::failed(
                            "there is no Push/Pull numeric input to undo",
                        );
                    }
                    self.status = "Removed the last Push/Pull distance character.".to_owned();
                    return CommandResult::completed();
                }
                if self.mesh_tool_state.active() {
                    return CommandResult::failed("Mesh does not support undoing individual picks");
                }
                if matches!(
                    self.active_tool.as_str(),
                    "Polyline"
                        | "Double Line"
                        | "Extrude Swipe"
                        | "Extrude Swipe 3D"
                        | "Ribbon 3D"
                        | "Freeform Slab"
                ) {
                    if !self.polyline_tool_state.backspace() {
                        return CommandResult::failed("there is no path point to undo");
                    }
                    self.status = "Removed the last path point or arc control point.".to_owned();
                    return CommandResult::completed();
                }
                self.sync_drawing_tool_state();
                self.drawing_tool.handle(ToolEvent::Backspace);
                self.status = "Removed the last typed character or pending point.".to_owned();
            }
            ShellAction::BeginCopyTransform(rotate) => {
                self.activate_tool(if rotate { "Rotate 2" } else { "Move" });
                self.transform_tool_state.set_copy_mode(true);
                self.status = if rotate {
                    "Rotate Copy: pick center, reference, then final angle point. The source stays unchanged."
                } else {
                    "Move Copy: pick reference and destination. The source stays unchanged."
                }.into();
                return CommandResult::completed();
            }
            ShellAction::ToggleTransformCopyMode => {
                if !self.transform_tool_state.toggle_copy_mode() {
                    return CommandResult::failed(
                        "copy mode is available for Move, Rotate, and Rotate 2",
                    );
                }
                self.status = if self.transform_tool_state.copy_mode() {
                    "Transform copy mode enabled.".to_owned()
                } else {
                    "Transform copy mode disabled.".to_owned()
                };
            }
            ShellAction::PaintSelection => {
                if self.selection.selected().iter().any(|t| {
                    !matches!(
                        t.entity,
                        SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
                    )
                }) {
                    return match self
                        .paint_material_targets(self.selection.selected().iter().cloned().collect())
                    {
                        Ok(()) => CommandResult::completed(),
                        Err(error) => CommandResult::failed(error),
                    };
                }
                let selection = self.selection.clone();
                let color = self.current_face_color();
                let material = self.current_paint_material.clone();
                match self.commit_document_edit("Paint selection", |document| {
                    Self::paint_with_material(document, &selection, color, material.as_deref())
                }) {
                    Ok(report) if report.changed() => {
                        self.status = format!(
                            "Painted {} edge(s) and {} face(s).",
                            report.painted_segments, report.painted_triangles
                        );
                        let mut result = CommandResult::completed();
                        result.outputs.insert(
                            "painted".to_owned(),
                            serde_json::json!({
                                "edges": report.painted_segments,
                                "faces": report.painted_triangles,
                            }),
                        );
                        return result;
                    }
                    Ok(_) => {
                        self.status = "Selection already has the current color.".to_owned();
                    }
                    Err(error) => return CommandResult::failed(error),
                }
            }
            ShellAction::FaceOutline => return self.outline_selected_faces(),
            ShellAction::MeshIntersection => return self.intersect_mesh_selection(),
            ShellAction::FillLoops => return self.fill_selected_loops(),
            ShellAction::SolidBoolean(operation) => return self.solid_boolean(operation),
            ShellAction::BeginSection => return self.begin_section(),
            ShellAction::PolylineMesh => return self.mesh_selected_polylines(),
            ShellAction::CutSelectedFaces(tool) => return self.cut_selected_faces(tool),
            ShellAction::SetPaintColor => {
                let channels = ["red", "green", "blue", "alpha"]
                    .map(|name| payload.values.get(name).and_then(serde_json::Value::as_u64));
                if channels.iter().any(Option::is_none)
                    || channels.iter().flatten().any(|channel| *channel > 255)
                {
                    return CommandResult::failed(
                        "paint color channels must be integers between 0 and 255",
                    );
                }
                let [Some(red), Some(green), Some(blue), Some(alpha)] = channels else {
                    return CommandResult::failed("all four paint color channels are required");
                };
                self.current_paint_color =
                    Rgba8::new(red as u8, green as u8, blue as u8, alpha as u8);
                self.current_paint_material = None;
                self.status = format!(
                    "Current paint color set to #{red:02x}{green:02x}{blue:02x}{alpha:02x}."
                );
            }
            ShellAction::CancelActiveTool => self.cancel_active_tool(),
            ShellAction::PlaceGuide(kind) => {
                if let Some(cursor) = self.cursor_snap {
                    if let Some(guide) = self.cursor_aligned_guide(kind, cursor) {
                        self.guides.push(guide);
                        self.status = match kind {
                            ConstructionGuideKind::Axis => {
                                "Axis guide placed at the cursor.".to_owned()
                            }
                            ConstructionGuideKind::Grid => {
                                "Grid guide placed at the cursor.".to_owned()
                            }
                        };
                        return CommandResult::completed();
                    }
                    return CommandResult::failed(
                        "Could not construct a stable guide orientation.",
                    );
                }
                if self.active_tool != "Select" {
                    return CommandResult::failed(
                        "Move the pointer over the viewport to place a guide; the active tool and its picked points were kept.",
                    );
                }
                self.active_tool = match kind {
                    ConstructionGuideKind::Axis => "Axis Guide",
                    ConstructionGuideKind::Grid => "Grid Guide",
                }
                .to_owned();
                self.cuboid_first = None;
                self.drawing_tool
                    .handle(ToolEvent::Activate(DrawingTool::Select));
                self.shape_tool_state.activate(None);
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
                self.drawing_tool
                    .handle(ToolEvent::Activate(DrawingTool::Select));
                self.shape_tool_state.activate(None);
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
            ShellAction::AnnotationSettings => return self.annotation_settings(payload),
            ShellAction::ArchitectureAction(action) => {
                return self.architecture_action(action, payload);
            }
            ShellAction::FlipSelection => {
                let selection = self.selection.clone();
                return match self.commit_document_edit("Flip direction / normals", |doc| {
                    modeling_transforms::flip_selection(doc, &selection)
                }) {
                    Ok(count) => {
                        self.status = format!("Flipped {count} selected primitive(s).");
                        CommandResult::completed()
                    }
                    Err(e) => CommandResult::failed(e),
                };
            }
            ShellAction::CopySelection => return self.copy_selection(),
            ShellAction::ClipboardAction(action) => return self.clipboard_action(action, payload),
            ShellAction::DeleteSelection => {
                if self
                    .selection
                    .selected()
                    .iter()
                    .any(|t| self.document.architecture_element(t).is_some())
                {
                    return self.delete_architecture_selection();
                }
                if self.selection.selected().iter().any(|target| {
                    matches!(
                        target.entity,
                        SelectedEntity::SemanticHandle {
                            kind: SelectionKind::Hotspot,
                            ..
                        }
                    )
                }) {
                    return self.delete_selection_with_hotspots();
                }
                let selection = self.selection.clone();
                let deleted = match self.commit_document_edit("Delete selection", |document| {
                    modeling_objects::delete_selection(document, &selection)
                }) {
                    Ok(deleted) => deleted,
                    Err(error) => return CommandResult::failed(error),
                };
                self.selection.clear();
                self.object_name_edit = None;
                self.status = format!(
                    "Deleted {} object(s), {} vertex/vertices, {} edge(s), {} face(s), {} dimension(s) and {} text/leader(s).",
                    deleted.occurrences,
                    deleted.vertices,
                    deleted.segments,
                    deleted.triangles,
                    deleted.dimensions,
                    deleted.texts
                );
                let mut result = CommandResult::completed();
                result
                    .outputs
                    .insert("deleted_count".to_owned(), deleted.deleted_count().into());
                result.outputs.insert(
                    "deleted".to_owned(),
                    serde_json::json!({
                        "objects": deleted.occurrences,
                        "vertices": deleted.vertices,
                        "edges": deleted.segments,
                        "faces": deleted.triangles,
                    }),
                );
                return result;
            }
            ShellAction::ClearSelection => {
                self.selection.clear();
                self.object_name_edit = None;
                self.status = "Selection cleared.".to_owned();
                return CommandResult::completed();
            }
            ShellAction::BakeObjects(to_shader) => return self.bake_objects_command(to_shader),
            ShellAction::EnterEditContext => {
                let selected = self.selected_occurrences();
                if selected.len() != 1 {
                    return CommandResult::failed("select one object instance to edit it");
                }
                let occurrence = selected
                    .into_iter()
                    .next()
                    .expect("one selection was checked");
                if self.document.is_baked_gpu_object(&occurrence) {
                    return CommandResult::failed(
                        "Use Bake to Mesh before editing a frozen GPU object.",
                    );
                }
                if self.document.procedural_object(&occurrence).is_some() {
                    self.open_panels(&["tree_edit"]);
                    return CommandResult::completed();
                }
                if !self
                    .selection
                    .set_context(EditContext::Occurrence(occurrence.clone()))
                {
                    return CommandResult::failed("that object instance is already active");
                }
                self.selection_click_tracker.reset();
                self.status = format!("Editing object instance at path {:?}.", occurrence.path);
                return CommandResult::completed();
            }
            ShellAction::ExitEditContext => {
                self.cancel_active_tool();
                self.exit_object_edit();
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
                self.tool_cycle_state.cancel_hotspot_placement();
                self.active_tool = "Select".to_owned();
                self.cuboid_first = None;
                self.drawing_tool
                    .handle(ToolEvent::Activate(DrawingTool::Select));
                self.shape_tool_state.activate(None);
                self.guides.clear();
                self.cursor_snap = None;
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
                selection.retain(|path| {
                    self.selection_display_policy.can_edit_target(
                        &self.document,
                        &SelectionTarget {
                            occurrence: path.clone(),
                            entity: SelectedEntity::Occurrence,
                        },
                    )
                });
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
            ShellAction::UpdateSelectionDisplay => {
                let Some(kind_name) = payload.values.get("kind").and_then(|value| value.as_str())
                else {
                    return CommandResult::failed("kind must be a selection type name");
                };
                let kinds = match kind_name {
                    "all" => SelectionKind::ALL.to_vec(),
                    "dimensions_texts" => vec![SelectionKind::Dimension, SelectionKind::Text],
                    "architecture" => vec![
                        SelectionKind::Wall,
                        SelectionKind::Slab,
                        SelectionKind::Stair,
                        SelectionKind::Frame,
                        SelectionKind::Hvac,
                    ],
                    "edge" => vec![SelectionKind::Edge, SelectionKind::Vertex],
                    name => match selection_kind_from_name(name) {
                        Some(kind) => vec![kind],
                        None => {
                            return CommandResult::failed(format!(
                                "unknown selection type `{name}`"
                            ));
                        }
                    },
                };
                let Some(property) = payload
                    .values
                    .get("property")
                    .and_then(|value| value.as_str())
                else {
                    return CommandResult::failed(
                        "property must be `display`, `modifiable` or `wireframe`",
                    );
                };
                let Some(enabled) = payload
                    .values
                    .get("enabled")
                    .and_then(|value| value.as_bool())
                else {
                    return CommandResult::failed("enabled must be a boolean");
                };
                if !matches!(
                    property,
                    "display" | "visible" | "modifiable" | "editable" | "wireframe"
                ) {
                    return CommandResult::failed(format!(
                        "unknown selection display property `{property}`"
                    ));
                }
                let mut changed = false;
                for kind in kinds {
                    changed |= match property {
                        "display" | "visible" => {
                            self.selection_display_policy.set_visible(kind, enabled)
                        }
                        "modifiable" | "editable" => {
                            self.selection_display_policy.set_unlocked(kind, enabled)
                        }
                        _ => self.selection_display_policy.set_wireframe(kind, enabled),
                    };
                }
                if changed && property != "wireframe" {
                    self.cancel_active_tool();
                }
                if !self
                    .selection_display_policy
                    .can_select(SelectionKind::Object)
                    && matches!(self.selection.context, EditContext::Occurrence(_))
                {
                    self.selection.set_context(EditContext::Document);
                }
                let retained = self
                    .selection
                    .selected()
                    .iter()
                    .filter(|target| {
                        self.selection_display_policy
                            .can_edit_target(&self.document, target)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                self.selection.apply(retained, SelectionMode::Replace);
                self.hotspot_inspector = self.hotspot_inspector.take().filter(|(path, id)| {
                    self.selection_display_policy.can_edit_target(
                        &self.document,
                        &SelectionTarget {
                            occurrence: path.clone(),
                            entity: SelectedEntity::SemanticHandle {
                                kind: SelectionKind::Hotspot,
                                id: id.clone(),
                            },
                        },
                    )
                });
                self.status = format!(
                    "{kind_name} {property} {}.",
                    if enabled { "enabled" } else { "disabled" }
                );
                let mut result = CommandResult::completed();
                result.outputs.insert("kind".to_owned(), kind_name.into());
                result
                    .outputs
                    .insert("property".to_owned(), property.into());
                result.outputs.insert("enabled".to_owned(), enabled.into());
                result.outputs.insert("changed".to_owned(), changed.into());
                return result;
            }
            ShellAction::CreateObject => {
                let name = match payload.values.get("name") {
                    Some(value) => match value.as_str() {
                        Some(name) => name,
                        None => return CommandResult::failed("name must be a string"),
                    },
                    None => "Object",
                };
                return self.create_object_from_selection(name);
            }
            ShellAction::ExplodeSelection => return self.explode_selection(),
            ShellAction::BeginObjectPlacement => {
                if let Some(id) = payload
                    .values
                    .get("definition_id")
                    .and_then(serde_json::Value::as_u64)
                {
                    return self.begin_placing_definition(DefinitionId(id));
                }
                return self.begin_object_placement();
            }
            ShellAction::ToggleObjectPlaceGlue => {
                let enabled = !self.object_place_tool_state.glue_to_surface();
                if !self.object_place_tool_state.set_glue_to_surface(enabled) {
                    return CommandResult::failed("Object Place is not active");
                }
                self.status = if enabled {
                    "Object Place will align to picked surface normals.".to_owned()
                } else {
                    "Object Place will keep the definition orientation.".to_owned()
                };
                let mut result = CommandResult::completed();
                result.outputs.insert("enabled".to_owned(), enabled.into());
                return result;
            }
            ShellAction::DeleteDefinition => return self.delete_prototype_command(payload),
            ShellAction::DeleteUnusedDefinition => {
                let Some(definition_id) = payload
                    .values
                    .get("definition_id")
                    .and_then(serde_json::Value::as_u64)
                else {
                    return CommandResult::failed("definition_id must be an unsigned integer");
                };
                let definition_id = DefinitionId(definition_id);
                match self.commit_document_edit("Delete unused object definition", |document| {
                    modeling_objects::delete_unused_definition(document, definition_id)
                }) {
                    Ok(()) => {
                        self.status = "Deleted the unused object definition.".to_owned();
                        let mut result = CommandResult::completed();
                        result
                            .outputs
                            .insert("definition_id".to_owned(), definition_id.0.into());
                        return result;
                    }
                    Err(error) => return CommandResult::failed(error),
                }
            }
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
                for (name, target) in [
                    ("arc_max_length", &mut settings.arc_max_length),
                    ("double_line_size", &mut settings.double_line_size),
                    ("double_line_offset", &mut settings.double_line_offset),
                ] {
                    if let Some(value) = payload.values.get(name) {
                        let Some(value) = value.as_f64().filter(|value| value.is_finite()) else {
                            return CommandResult::failed(format!("{name} must be finite"));
                        };
                        *target = value;
                    }
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
            ShellAction::ShowRecentProjects => {
                if self.preferences.recent_projects.is_empty() {
                    return CommandResult::failed("there are no recently opened projects");
                }
                self.recent_projects_dialog_open = true;
                self.status = "Choose a recently opened project.".to_owned();
            }
            ShellAction::CancelProjectLoad => return self.cancel_project_load(),
            ShellAction::SaveProject => return self.save_project(false, payload),
            ShellAction::SaveProjectAs => return self.save_project(true, payload),
            ShellAction::ResetWorkspaceLayout => {
                let recent_projects = std::mem::take(&mut self.preferences.recent_projects);
                self.preferences = Preferences::default();
                self.preferences.recent_projects = recent_projects;
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
            ShellAction::SetPerspectiveView => {
                self.viewport.set_perspective_view();
                self.status = "Perspective camera selected.".into();
            }
            ShellAction::SetWalkthroughView => {
                self.viewport.set_walkthrough_view();
                self.status =
                    "Walkthrough camera selected. Move with WASD and look with right-drag."
                        .to_owned();
            }
            ShellAction::FrameDocument => {
                if self.viewport.walkthrough_active() {
                    self.viewport.set_perspective_view();
                }
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
                "left_panel_width",
                &mut preferences.left_panel_width,
                180.,
                600.,
            ),
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
            ("menu_bar_visible", &mut preferences.menu_bar_visible),
            ("ribbon_visible", &mut preferences.ribbon_visible),
            ("left_panel_visible", &mut preferences.left_panel_visible),
            (
                "auto_collapse_toolbars",
                &mut preferences.auto_collapse_toolbars,
            ),
            (
                "accordion_settings_panels",
                &mut preferences.accordion_settings_panels,
            ),
            ("show_permanent_grid", &mut preferences.show_permanent_grid),
            ("black_model_lines", &mut preferences.black_model_lines),
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
        if let Some(value) = payload.values.get("menu_visibility") {
            let Some(menus) = value.as_object() else {
                return CommandResult::failed("menu_visibility must be an object");
            };
            for (name, value) in menus {
                if !DROPDOWN_MENU_NAMES.contains(&name.as_str())
                    && ![
                        "2D Construction",
                        "File",
                        "Edit",
                        "View",
                        "Insert",
                        "Format",
                        "Draw",
                        "Dimension",
                        "Modify",
                        "Arrays",
                        "Tools",
                        "Render",
                        "Window",
                        "Help",
                    ]
                    .contains(&name.as_str())
                {
                    return CommandResult::failed(format!("unknown dropdown menu {name:?}"));
                }
                let Some(visible) = value.as_bool() else {
                    return CommandResult::failed(format!(
                        "menu_visibility.{name} must be boolean"
                    ));
                };
                preferences.menu_visibility.insert(name.clone(), visible);
            }
        }
        if payload.values.contains_key("antialiasing_quality") {
            preferences.antialiasing_quality = match payload_u32(payload, "antialiasing_quality", 2)
            {
                Ok(value) if value <= 3 => value,
                _ => {
                    return CommandResult::failed(
                        "antialiasing_quality must be 0 (Off), 1 (Low), 2 (Medium), or 3 (High)",
                    );
                }
            };
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
        if let Some(positions) = payload.values.get("toolbar_positions") {
            let Some(positions) = positions.as_object() else {
                return CommandResult::failed(
                    "toolbar_positions must be an object of [x, y] pairs",
                );
            };
            for (stem, value) in positions {
                if !LEGACY_TOOLBARS
                    .iter()
                    .any(|(id, _)| id.strip_prefix("builtin_toolbar_") == Some(stem.as_str()))
                {
                    return CommandResult::failed(format!(
                        "toolbar_positions contains unknown toolbar {stem:?}"
                    ));
                }
                let Some(pair) = value.as_array().filter(|pair| pair.len() == 2) else {
                    return CommandResult::failed(format!(
                        "toolbar_positions.{stem} must be a two-number [x, y] pair"
                    ));
                };
                let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) else {
                    return CommandResult::failed(format!(
                        "toolbar_positions.{stem} must contain numeric coordinates"
                    ));
                };
                if !(0.0..=100_000.0).contains(&x) || !(0.0..=100_000.0).contains(&y) {
                    return CommandResult::failed(format!(
                        "toolbar_positions.{stem} coordinates must be between 0 and 100000"
                    ));
                }
                preferences
                    .toolbar_positions
                    .insert(stem.clone(), [x as f32, y as f32]);
            }
        }
        self.preferences = preferences;
        self.status = "UI preferences updated.".to_owned();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut result = CommandResult::completed();
            if self.persist_native_preferences
                && let Err(error) = persist_native_preferences(&self.preferences)
            {
                let message = format!("UI preferences updated but could not be saved: {error}");
                self.status = message.clone();
                result.diagnostics.push(message);
            }
            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            CommandResult::completed()
        }
    }

    fn selection_can_array(&self) -> bool {
        let key = SelectionQueryKey {
            generation: self.document_generation,
            visual_revision: self.document.visual_revision(),
            selection_revision: self.selection.revision(),
            context: self.selection.context.clone(),
        };
        if let Some(cached) = self.copy_availability.borrow().as_ref()
            && cached.key == key
        {
            return cached.available;
        }
        let available = modeling_copy::can_linear_array_selection(&self.document, &self.selection);
        *self.copy_availability.borrow_mut() = Some(CopyAvailability { key, available });
        available
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
        if id == "tool.builtin.face_outline"
            && !self
                .selection
                .selected()
                .iter()
                .any(|target| matches!(target.entity, SelectedEntity::Triangle(_)))
        {
            return CommandState::disabled("select one or more faces first");
        }
        if id == "tool.builtin.polyline_mesh"
            && !self
                .selection
                .selected()
                .iter()
                .any(|target| matches!(target.entity, SelectedEntity::Segment(_)))
        {
            return CommandState::disabled("select connected line segments first");
        }
        if matches!(
            id,
            "tool.builtin.extrude_swipe" | "tool.builtin.extrude_swipe_3d"
        ) && self.selected_sweep_profile().is_err()
        {
            return CommandState::disabled(
                "select profile edges in the active geometry graph first",
            );
        }
        if id == "tool.builtin.revolve" && self.selected_sweep_profile().is_err() {
            return CommandState::disabled(
                "select profile edges in the active geometry graph first",
            );
        }
        if matches!(
            id,
            "tool.builtin.cut_holes" | "tool.builtin.cut_holes_2" | "tool.builtin.cut_out_3"
        ) && self.selected_cut_occurrence().is_none()
        {
            return CommandState::disabled(
                "select faces and edges in one active geometry graph first",
            );
        }
        if matches!(
            id,
            "tool.builtin.cut_holes" | "tool.builtin.cut_holes_2" | "tool.builtin.cut_out_3"
        ) && self.selected_cut_occurrence().is_none()
        {
            return CommandState::disabled(
                "select faces and edges in one active geometry graph first",
            );
        }
        if id == "tool.builtin.line_offset"
            && !self
                .selection
                .selected()
                .iter()
                .any(|target| matches!(target.entity, SelectedEntity::Segment(_)))
        {
            return CommandState::disabled("select edges before starting Line Offset");
        }
        if id == "tool.builtin.push_pull"
            && !self
                .selection
                .selected()
                .iter()
                .any(|target| matches!(target.entity, SelectedEntity::Triangle(_)))
        {
            return CommandState::disabled("select one or more faces before starting Push/Pull");
        }
        let push_pull_input_ready = self.active_tool == "Push/Pull"
            && self.push_pull_reference.is_some()
            && self.push_pull_distance_focused
            && !self.push_pull_numeric_input.is_empty();
        let line_distance_ready = matches!(self.active_tool.as_str(), "Line" | "Construction Line")
            && self.drawing_tool.anchor().is_some();
        let transform_input_ready = self.transform_tool_state.active().is_some()
            && !self.transform_tool_state.points().is_empty();
        let revolve_input_ready =
            self.revolve_tool_state.active() && self.revolve_tool_state.axis_points().len() == 2;
        let revolve_undo_ready = self.revolve_tool_state.active()
            && (!self.revolve_tool_state.numeric_input().is_empty()
                || !self.revolve_tool_state.axis_points().is_empty());
        let path_tool_active = matches!(
            self.active_tool.as_str(),
            "Polyline"
                | "Double Line"
                | "Extrude Swipe"
                | "Extrude Swipe 3D"
                | "Ribbon 3D"
                | "Freeform Slab"
        );
        let polyline_finish_ready =
            path_tool_active && self.polyline_tool_state.points().len() >= 2;
        let mesh_finish_ready =
            self.mesh_tool_state.active() && !self.mesh_tool_state.points().is_empty();
        let polyline_undo_ready = path_tool_active
            && (self.polyline_tool_state.arc_control().is_some()
                || !self.polyline_tool_state.points().is_empty());
        let array_undo_ready =
            self.array_tool_state.active() && !self.array_tool_state.points().is_empty();
        let rotational_array_undo_ready = self.rotational_array_tool_state.active()
            && !self.rotational_array_tool_state.points().is_empty();
        if id == "tool.distance"
            && !line_distance_ready
            && !transform_input_ready
            && !revolve_input_ready
            && !(self.active_tool == "Push/Pull" && self.push_pull_reference.is_some())
        {
            return CommandState::disabled(
                "place a line, transform, or Push/Pull reference point before entering a value",
            );
        }
        if id == "tool.finish"
            && self.numeric_entry.text.is_empty()
            && self.active_tool != "Wall"
            && !line_distance_ready
            && !transform_input_ready
            && !push_pull_input_ready
            && !polyline_finish_ready
            && !mesh_finish_ready
            && !revolve_input_ready
        {
            return CommandState::disabled(
                "start a Line, Construction Line, Polyline, Double Line, a sweep path, or a Mesh chain first",
            );
        }
        if id == "tool.undo_point"
            && self.numeric_entry.text.is_empty()
            && !(architecture_workflow::active(&self.active_tool)
                && !self.architecture.points.is_empty())
            && !line_distance_ready
            && !transform_input_ready
            && !polyline_undo_ready
            && !revolve_undo_ready
            && !array_undo_ready
            && !rotational_array_undo_ready
        {
            return CommandState::disabled("start a point-input tool before undoing a point");
        }
        if id == "tool.copy_mode"
            && !self
                .transform_tool_state
                .active()
                .is_some_and(TransformTool::supports_copy_mode)
        {
            return CommandState::disabled("copy mode is available for Move, Rotate, and Rotate 2");
        }
        if id == "paint.apply_selection"
            && !self.selection.selected().iter().any(|target| {
                matches!(
                    target.entity,
                    SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
                )
            })
        {
            return CommandState::disabled("select an edge or face to paint");
        }
        if matches!(id, "tool.move_copy" | "tool.rotate_copy")
            && !self.selection.selected().is_empty()
        {
            return CommandState {
                checked: Some(
                    self.transform_tool_state.copy_mode()
                        && self.active_tool
                            == if id == "tool.move_copy" {
                                "Move"
                            } else {
                                "Rotate 2"
                            },
                ),
                ..Default::default()
            };
        }
        if id == "tool.copy_mode" {
            return CommandState {
                checked: Some(self.transform_tool_state.copy_mode()),
                ..CommandState::default()
            };
        }
        if id == "object.toggle_place_glue" {
            return if self.object_place_tool_state.active_definition().is_some() {
                CommandState {
                    checked: Some(self.object_place_tool_state.glue_to_surface()),
                    ..CommandState::default()
                }
            } else {
                CommandState::disabled("start Object Place before changing glue behavior")
            };
        }
        if matches!(
            id,
            "edit.copy_selection" | "edit.cut_selection" | "edit.copy_as_script"
        ) {
            return if self.selection.selected().is_empty() {
                CommandState::disabled("select elements first")
            } else {
                CommandState::default()
            };
        }
        if matches!(id, "edit.paste" | "edit.paste_in_place") {
            return CommandState::default();
        }
        if matches!(id, "geometry.fill_loops" | "tool.mirror")
            && self.selection.selected().is_empty()
        {
            return CommandState::disabled("Select geometry first.");
        }
        if id == "geometry.fill_loops"
            && !self
                .selection
                .selected()
                .iter()
                .any(|t| matches!(t.entity, SelectedEntity::Segment(_)))
        {
            return CommandState::disabled("Select lines forming closed loops.");
        }
        if id == "geometry.flip_selection" {
            return if self.selection.selected().iter().any(|t| {
                matches!(
                    t.entity,
                    SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
                ) && self.native_target_editable(t)
            }) {
                CommandState::default()
            } else {
                CommandState::disabled("select editable segments or faces to flip")
            };
        }
        if id == "edit.duplicate_selection" {
            return if self.selection.selected().len() == 1
                && self
                    .selection
                    .selected()
                    .iter()
                    .all(|target| target.entity == SelectedEntity::Occurrence)
            {
                CommandState::default()
            } else {
                CommandState::disabled("select one object instance to copy")
            };
        }
        if id == "tool.cancel"
            && self.active_tool == "Select"
            && !self.tool_cycle_state.hotspot_placement_active()
        {
            return CommandState::disabled("there is no active tool to cancel");
        }
        if id == "hotspot.cancel_placement" && !self.tool_cycle_state.hotspot_placement_active() {
            return CommandState::disabled("hotspot placement is not active");
        }
        if id == "hotspot.begin_placement" {
            return CommandState {
                checked: Some(self.tool_cycle_state.hotspot_placement_active()),
                ..CommandState::default()
            };
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
        if id == "file.open_recent" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Opening project files is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            {
                if self.preferences.recent_projects.is_empty() {
                    return CommandState::disabled("there are no recently opened projects");
                }
                if self.pending_replacement.is_some()
                    || self.pending_project_load.is_some()
                    || self.pending_project_save.is_some()
                {
                    return CommandState::disabled("another project operation is already running");
                }
            }
        }
        if id == "edit.delete_selection"
            && self
                .selection
                .selected()
                .iter()
                .any(|t| self.document.architecture_element(t).is_some())
        {
            return if self.selection.selected().iter().all(|t| {
                self.document
                    .architecture_element(t)
                    .is_none_or(|e| self.arch_editable(&e))
            }) {
                CommandState::default()
            } else {
                CommandState::disabled("Architecture selection is read-only")
            };
        }
        if id == "edit.delete_selection"
            && self.selection.selected().iter().any(|target| {
                matches!(
                    target.entity,
                    SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Hotspot,
                        ..
                    }
                )
            })
        {
            return if self
                .selection
                .selected()
                .iter()
                .filter(|target| {
                    matches!(
                        target.entity,
                        SelectedEntity::SemanticHandle {
                            kind: SelectionKind::Hotspot,
                            ..
                        }
                    )
                })
                .all(|target| self.native_hotspot_authoring_allowed(&target.occurrence))
            {
                CommandState::default()
            } else {
                CommandState::disabled("open the prototype to delete a hotspot definition")
            };
        }
        if id.starts_with("hotspot.")
            && !matches!(
                id,
                "hotspot.begin_placement" | "hotspot.cancel_placement" | "hotspot.add"
            )
        {
            let (selected, _) = self.native_hotspot_inspector();
            if let Some((path, _)) = selected {
                let authoring = self.native_hotspot_authoring_allowed(&path);
                if id == "hotspot.target" && authoring {
                    return CommandState::disabled(
                        "finish prototype editing before operating an instance handle",
                    );
                }
                if id != "hotspot.target" && !authoring {
                    return CommandState::disabled(
                        "open the prototype to edit hotspot definitions",
                    );
                }
            } else if id != "hotspot.update" {
                return CommandState::disabled("select a hotspot first");
            }
        }
        if id == "object.place_selected_definition" && self.native_selected_prototype().is_some() {
            return CommandState::default();
        }
        if id == "object.refresh_reference"
            && !self
                .native_selected_prototype()
                .and_then(|id| self.document.definition(id))
                .is_some_and(|d| {
                    d.extensions
                        .get("k3d.externalReferenceEnabled")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true)
                })
        {
            return CommandState::disabled("select an external prototype in Objects first");
        }
        if id == "selection.exit_context" && self.selection.context == EditContext::Document {
            return CommandState::disabled("the document context is already active");
        }
        if id == "selection.edit_object" {
            let selected = self.selected_occurrences();
            if selected.len() != 1 || self.document.resolve_occurrence(&selected[0]).is_none() {
                return CommandState::disabled("select one object instance to edit it");
            }
        }
        if matches!(
            id,
            "tool.builtin.copy_multiple"
                | "tool.builtin.planar_translate_multiple"
                | "tool.builtin.volumetric_translate_multiple"
                | "tool.builtin.planar_rotate_multiple"
                | "tool.builtin.helicoidal_rotate_multiple"
                | "tool.builtin.rotational_array"
                | "tool.builtin.helical_array"
                | "tool.builtin.rotational_array_equalized"
                | "tool.builtin.helical_array_equalized"
        ) && !self.selection_can_array()
        {
            return CommandState::disabled(
                "select supported objects, mesh entities, dimensions, or text first",
            );
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
        if matches!(id, "import.mesh" | "export.mesh" | "export.ifc_model") {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Mesh file interchange is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            if self.pending_interchange.is_some()
                || self.pending_project_load.is_some()
                || self.pending_replacement.is_some()
            {
                return CommandState::disabled(
                    "another import/export or document replacement is still running",
                );
            }
        }
        if id == "interchange.cancel" {
            #[cfg(target_arch = "wasm32")]
            return CommandState::disabled("Mesh file interchange is available in the desktop app");
            #[cfg(not(target_arch = "wasm32"))]
            if self.pending_interchange.is_none() {
                return CommandState::disabled("no import/export is running");
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
            CommandStateQuery::SelectionAvailable
                if command.descriptor.id.as_str() == "object.array_selection"
                    && !self.selection_can_array() =>
            {
                CommandState::disabled(
                    "select supported objects, mesh entities, dimensions, or text first",
                )
            }
            CommandStateQuery::SelectionAvailable if self.selection.selected().is_empty() => {
                CommandState::disabled("select objects or mesh entities first")
            }
            CommandStateQuery::DeleteSelectionAvailable => {
                if modeling_objects::can_delete_selection(&self.document, &self.selection) {
                    CommandState::default()
                } else {
                    CommandState::disabled("select existing objects or mesh entities to delete")
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
        let Some(target) = self.selection.selected().iter().next() else {
            return CommandResult::failed("select one object instance to copy");
        };
        if self.selection.selected().len() != 1 || target.entity != SelectedEntity::Occurrence {
            return CommandResult::failed("copy currently requires one selected object instance");
        }
        let source = target.occurrence.clone();
        if source.path.is_empty() {
            return CommandResult::failed("the scene root cannot be copied as an object instance");
        }
        let Some(resolved) = self.document.resolve_occurrence(&source) else {
            return CommandResult::failed("the selected object no longer exists");
        };
        let Some(geometry) = self
            .document
            .geometry_in_context(&source, &self.selection.context)
        else {
            return CommandResult::failed("the selected object has no resolved geometry");
        };
        let parent = OccurrenceRef {
            path: source.path[..source.path.len() - 1].to_vec(),
        };
        let parent_world = if parent.path.is_empty() {
            Affine3::IDENTITY
        } else if let Some(parent) = self.document.resolve_occurrence(&parent) {
            parent.world_transform
        } else {
            return CommandResult::failed("the selected object's parent no longer exists");
        };
        let Some(parent_inverse) = parent_world.inverse(GeometryTolerance::default()) else {
            return CommandResult::failed("the selected object's parent transform is singular");
        };
        let local_to_parent = parent_inverse.compose(resolved.world_transform);
        let span_x = geometry
            .vertices
            .values()
            .map(|vertex| local_to_parent.transform_point(vertex.position).x)
            .fold(None, |span: Option<(f64, f64)>, x| {
                Some(span.map_or((x, x), |(min, max)| (min.min(x), max.max(x))))
            })
            .map_or(1.0, |(min, max)| max - min);
        let context = self.selection.context.clone();
        match self.commit_document_edit("Copy object", |document| {
            modeling_objects::duplicate_occurrence_in_context(
                document,
                &context,
                &source,
                Vec3::new(span_x + 0.5, 0.0, 0.0),
            )
            .map(|(copy, _)| copy)
        }) {
            Ok(copy) => {
                self.set_selection([copy.clone()]);
                self.status = "Copied the selected object instance.".to_owned();
                command_result_with_occurrences(
                    CommandResult::completed(),
                    std::slice::from_ref(&copy),
                )
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn create_object_from_selection(&mut self, name: &str) -> CommandResult {
        let selection = self.selection.clone();
        match self.commit_document_edit("Create object", |document| {
            modeling_objects::group_selection(document, &selection, name)
        }) {
            Ok(object) => {
                self.set_selection([object.clone()]);
                self.status = "Created an object definition from the selection.".to_owned();
                command_result_with_occurrences(
                    CommandResult::completed(),
                    std::slice::from_ref(&object),
                )
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn explode_selection(&mut self) -> CommandResult {
        let selection = modeling_objects::selection_with_architecture_instances(
            &self.document,
            &self.selection,
        );
        if self
            .selection
            .selected()
            .iter()
            .chain(selection.selected())
            .any(|target| {
                !self.native_target_editable(target)
                    || self
                        .document
                        .architecture_element(target)
                        .is_some_and(|e| !self.arch_editable(&e))
            })
        {
            return CommandResult::failed(
                "Explode requires editable objects or architecture in the active context.",
            );
        }
        let report = match self.commit_document_edit("Explode selection", |document| {
            modeling_objects::explode_selection(
                document,
                &selection,
                &modeling_objects::LegacyAnnotationExplodeAdapter,
            )
        }) {
            Ok(report) => report,
            Err(error) => return CommandResult::failed(error),
        };
        self.selection
            .apply(report.selected_results.clone(), SelectionMode::Replace);
        self.status = format!(
            "Exploded {} object(s) into {} edge(s), {} face(s), {} dimension(s), {} text item(s) and {} vector text outline(s).",
            report.exploded_occurrences,
            report.segments,
            report.triangles,
            report.dimensions,
            report.texts,
            report.vector_texts
        );
        let mut result = CommandResult::completed();
        result.outputs.insert(
            "exploded_count".to_owned(),
            report.exploded_occurrences.into(),
        );
        result
            .outputs
            .insert("edge_count".to_owned(), report.segments.into());
        result
            .outputs
            .insert("face_count".to_owned(), report.triangles.into());
        result
            .outputs
            .insert("dimension_count".to_owned(), report.dimensions.into());
        result
            .outputs
            .insert("text_count".to_owned(), report.texts.into());
        result
            .outputs
            .insert("vector_text_count".to_owned(), report.vector_texts.into());
        let result_occurrences = report
            .selected_results
            .iter()
            .map(|target| target.occurrence.clone())
            .collect::<BTreeSet<_>>();
        result.outputs.insert(
            "selection".to_owned(),
            serde_json::to_value(result_occurrences).unwrap_or_default(),
        );
        result
    }

    fn begin_object_placement(&mut self) -> CommandResult {
        if let Some(id) = self.native_selected_prototype() {
            return self.begin_placing_definition(id);
        }
        let selected = self
            .selection
            .selected()
            .iter()
            .filter(|target| target.entity == SelectedEntity::Occurrence)
            .collect::<Vec<_>>();
        if self.selection.selected().len() != 1 || selected.len() != 1 {
            return CommandResult::failed("select one object instance whose definition to place");
        }
        let Some(resolved) = self.document.resolve_occurrence(&selected[0].occurrence) else {
            return CommandResult::failed("the selected object instance no longer exists");
        };
        let definition = resolved.definition.id;
        self.begin_placing_definition(definition)
    }

    pub fn begin_placing_definition(&mut self, definition: DefinitionId) -> CommandResult {
        let Some(prototype) = self.document.definition(definition) else {
            return CommandResult::failed("the object prototype no longer exists");
        };
        if definition == Document::SCENE_ROOT_DEFINITION_ID {
            return CommandResult::failed("the scene root cannot be inserted into itself");
        }
        let name = prototype.name.clone();
        self.cancel_active_tool();
        self.object_place_tool_state.activate(definition, false);
        self.active_tool = "Object Place".to_owned();
        self.cuboid_first = None;
        self.drawing_tool
            .handle(ToolEvent::Activate(DrawingTool::Select));
        self.shape_tool_state.activate(None);
        self.primitive_tool_state.cancel();
        self.transform_tool_state.activate(None);
        self.status = format!("Place {name}: click a point; Escape cancels.");
        CommandResult::completed()
    }

    fn place_object_at_hit(&mut self, hit: PlacementHit) -> CommandResult {
        let context = self.selection.context.clone();
        let mut next_tool = self.object_place_tool_state.clone();
        let mut placed = None;
        let result = self.commit_document_edit("Place object instance", |document| match next_tool
            .pick(document, &context, hit)
        {
            ObjectPlaceEffect::Placed(occurrence) => {
                placed = Some(occurrence);
                Ok(())
            }
            ObjectPlaceEffect::Invalid(error) => Err(error),
            ObjectPlaceEffect::Inactive => Err("Object Place is not active".to_owned()),
            ObjectPlaceEffect::NeedHit | ObjectPlaceEffect::Preview(_) => {
                Err("pick a valid object placement point".to_owned())
            }
            ObjectPlaceEffect::Canceled => Err("object placement was canceled".to_owned()),
        });
        match result {
            Ok(()) => {
                self.object_place_tool_state = next_tool;
                self.placement_preview = Default::default();
                let occurrence = placed.expect("a successful placement returns an occurrence");
                self.set_selection([occurrence.clone()]);
                self.active_tool = "Select".to_owned();
                self.status = "Object placed; the new instance is selected.".to_owned();
                command_result_with_occurrences(
                    CommandResult::completed(),
                    std::slice::from_ref(&occurrence),
                )
            }
            Err(error) => {
                self.status = error.clone();
                CommandResult::failed(error)
            }
        }
    }

    fn array_selection(&mut self, count: usize, spacing: f64) -> CommandResult {
        let mut result_selection = self.selection.clone();
        match self.commit_document_edit("Array selection", |document| {
            modeling_copy::linear_array(
                document,
                &mut result_selection,
                count,
                Vec3::new(spacing, 0.0, 0.0),
            )
        }) {
            Ok(report) => {
                self.array_preview = false;
                self.selection = result_selection;
                self.status = format!("Created {} array copies.", count);
                let occurrences = report
                    .selected_results
                    .iter()
                    .filter(|target| target.entity == SelectedEntity::Occurrence)
                    .map(|target| target.occurrence.clone())
                    .collect::<Vec<_>>();
                let mut result =
                    command_result_with_occurrences(CommandResult::completed(), &occurrences);
                result
                    .outputs
                    .insert("copy_count".to_owned(), report.copies.into());
                result
                    .outputs
                    .insert("vertex_count".to_owned(), report.copied_vertices.into());
                result
                    .outputs
                    .insert("edge_count".to_owned(), report.copied_segments.into());
                result
                    .outputs
                    .insert("face_count".to_owned(), report.copied_triangles.into());
                result.outputs.insert(
                    "annotation_count".to_owned(),
                    report.copied_domain_entities.into(),
                );
                result
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn cancel_active_tool(&mut self) {
        self.numeric_entry = Default::default();
        self.clipboard.placement = None;
        self.annotations.points.clear();
        self.architecture = Default::default();
        self.tool_cycle_state.cancel_hotspot_placement();
        self.object_place_tool_state.cancel();
        self.placement_preview = Default::default();
        self.array_tool_state.cancel();
        self.rotational_array_tool_state.cancel();
        self.array_preview = false;
        self.active_tool = "Select".to_owned();
        self.cuboid_first = None;
        self.drawing_tool.handle(ToolEvent::Escape);
        self.shape_tool_state.activate(None);
        self.primitive_tool_state.cancel();
        self.revolve_tool_state.activate(false);
        self.transform_tool_state.cancel();
        self.polyline_tool_state.cancel();
        self.mesh_tool_state.cancel();
        self.line_offset_tool_state.cancel();
        self.clear_push_pull_transient();
        self.status = "Current tool operation canceled.".to_owned();
    }

    fn handle_escape(&mut self) {
        if self.clipboard.placement.is_some() {
            self.cancel_active_tool();
            return;
        }
        if self.selection.context != EditContext::Document {
            self.cancel_active_tool();
            self.exit_object_edit();
        } else if self.active_tool != "Select" || self.tool_cycle_state.hotspot_placement_active() {
            self.cancel_active_tool();
            self.status = "Current tool canceled; Select active.".to_owned();
        } else if !self.selection.selected().is_empty() {
            self.selection.clear();
            self.hotspot_inspector = None;
            self.status = "Selection cleared.".to_owned();
        } else {
            self.guides.clear();
            self.status = "Construction guides cleared.".to_owned();
        }
    }

    fn cached_transform_wireframe(&mut self, transform: Affine3) -> Result<Vec<[Vec3; 2]>, String> {
        if !transform.is_finite() || transform.inverse(GeometryTolerance::default()).is_none() {
            return Err("preview transform must be finite and invertible".into());
        }
        // Explicit vertex edits can keep the opposite endpoint anchored.
        if self
            .selection
            .selected()
            .iter()
            .any(|t| matches!(t.entity, SelectedEntity::Vertex(_)))
        {
            return modeling_transforms::preview_selection_transform(
                &self.document,
                &self.selection,
                transform,
            );
        }
        let key = SelectionQueryKey {
            generation: self.document_generation,
            visual_revision: self.document.visual_revision(),
            selection_revision: self.selection.revision(),
            context: self.selection.context.clone(),
        };
        if self
            .transform_wireframe
            .as_ref()
            .is_none_or(|cache| cache.key != key)
        {
            #[cfg(not(target_arch = "wasm32"))]
            self.native_prepare_spatial_view();
            #[cfg(not(target_arch = "wasm32"))]
            let view = self
                .native_edit_view
                .as_ref()
                .map_or(&self.document, |(_, view)| view.as_ref());
            #[cfg(target_arch = "wasm32")]
            let view = &self.document;
            let edges = modeling_transforms::preview_selection_transform(
                view,
                &self.selection,
                Affine3::IDENTITY,
            )?;
            self.transform_wireframe = Some(TransformWireframe { key, edges });
        }
        Ok(self
            .transform_wireframe
            .as_ref()
            .unwrap()
            .edges
            .iter()
            .map(|edge| edge.map(|p| transform.transform_point(p)))
            .collect())
    }

    fn commit_transform(
        &mut self,
        commit: meshcraft_core::tools::transforms::TransformCommit,
    ) -> Result<modeling_transforms::TransformReport, String> {
        let mut next_selection = self.selection.clone();
        let connected = matches!(
            self.transform_tool_state.active(),
            Some(
                TransformTool::Stretch | TransformTool::StretchScale | TransformTool::RotateStretch
            )
        );
        let report = self.commit_document_edit("Transform selection", |document| {
            if connected && !commit.copy {
                modeling_transforms::stretch_selection(
                    document,
                    &mut next_selection,
                    commit.transform,
                )
            } else {
                modeling_transforms::transform_selection(
                    document,
                    &mut next_selection,
                    commit.transform,
                    commit.copy,
                )
            }
        })?;
        self.selection = next_selection;
        self.selection.retain_valid(&self.document);
        Ok(report)
    }

    fn finish_transform_input(&mut self) -> CommandResult {
        let mut next_state = self.transform_tool_state.clone();
        let commit = match next_state.finish() {
            Ok(commit) => commit,
            Err(error) => return CommandResult::failed(error),
        };
        match self.commit_transform(commit) {
            Ok(report) => {
                self.transform_tool_state = next_state;
                self.status = transform_status(&report, commit.copy);
                CommandResult::completed()
            }
            Err(error) => CommandResult::failed(error),
        }
    }

    fn handle_drawing_tool_effect(&mut self, effect: ToolEffect, snap: Option<SnapResult>) {
        match effect {
            ToolEffect::None => {}
            ToolEffect::AnchorPlaced(_) => {
                self.status = match self.active_tool.as_str() {
                    "Line" => {
                        "Line: choose the next point; type a distance to constrain it.".to_owned()
                    }
                    "Construction Line" => {
                        "Construction Line: choose the second point or type a distance.".to_owned()
                    }
                    _ => self.status.clone(),
                };
            }
            ToolEffect::CommitSegment {
                start,
                end,
                chain_continues,
            } => match self.create_line_in_active_context(start, end) {
                Ok(targets) => {
                    self.selection.apply(targets, SelectionMode::Replace);
                    let snap_note = snap.map_or(String::new(), |snap| {
                        format!(" · {} snap", snap.kind.label())
                    });
                    self.status = if chain_continues {
                        format!(
                            "Created line segment {:.2} units long; choose the next point or press Enter.{snap_note}",
                            start.distance(end)
                        )
                    } else {
                        format!(
                            "Created construction line {:.2} units long; choose another start point.{snap_note}",
                            start.distance(end)
                        )
                    };
                }
                Err(error) => self.status = error,
            },
            ToolEffect::FinishChain => {
                self.status = "Line chain finished; the Line tool remains active.".to_owned();
            }
            ToolEffect::Cancel => {
                self.active_tool = "Select".to_owned();
                self.status = "Current tool operation canceled.".to_owned();
            }
            ToolEffect::InvalidInput => {
                self.status = "Enter a finite, nonzero distance and choose a direction.".to_owned();
            }
        }
    }

    fn sync_drawing_tool_state(&mut self) {
        let expected = match self.active_tool.as_str() {
            "Line" => DrawingTool::Line,
            "Construction Line" => DrawingTool::ConstructionLine,
            _ => DrawingTool::Select,
        };
        if self.drawing_tool.active() != expected {
            self.drawing_tool.handle(ToolEvent::Activate(expected));
        }
    }

    #[cfg(test)]
    fn handle_viewport_click(
        &mut self,
        rect: egui::Rect,
        position: egui::Pos2,
        snap: Option<SnapResult>,
    ) {
        let construction_point = snap
            .map(|snap| snap.point)
            .or_else(|| self.viewport.screen_to_construction_plane(rect, position));
        self.handle_viewport_click_at_point(rect, position, snap, construction_point);
    }

    fn handle_viewport_click_input(
        &mut self,
        rect: egui::Rect,
        click: ViewportClick,
        snap: Option<SnapResult>,
    ) {
        let pixels_per_point = click.pixels_per_point;
        let click_count = if self.active_tool != "Select" {
            self.selection_click_tracker.reset();
            1
        } else if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            self.selection_click_tracker.record(
                click.time_millis,
                meshcraft_core::geometry::Vec2::new(
                    f64::from(click.position.x * pixels_per_point),
                    f64::from(click.position.y * pixels_per_point),
                ),
            )
        } else {
            1
        };
        let construction_point = snap.map(|snap| snap.point).or_else(|| {
            self.viewport
                .screen_to_construction_plane(rect, click.position)
        });
        self.handle_viewport_click_at_point_with_selection(
            rect,
            click.position,
            snap,
            construction_point,
            click.modifiers,
            click_count,
        );
    }

    /// Routes a world-space construction point through the active tool's shared click path.
    pub fn handle_world_pointer_request(
        &mut self,
        rect: egui::Rect,
        point: Vec3,
        pixels_per_point: f32,
    ) {
        if !point.is_finite() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            self.status =
                "World pointer needs finite coordinates and a valid device scale.".to_owned();
            return;
        }
        let position = self.viewport.world_to_screen(rect, point);
        if !rect.contains(position)
            || self
                .viewport
                .screen_to_construction_plane(rect, position)
                .is_none_or(|mapped| mapped.distance(point) > 1.0e-4)
        {
            self.status = "World pointer point is outside the active construction view.".to_owned();
            return;
        }
        let snap = self.snap_at_viewport_position(rect, position, pixels_per_point);
        let construction_point = snap.map_or(point, |snap| snap.point);
        self.handle_viewport_click_at_point(rect, position, snap, Some(construction_point));
    }

    fn handle_viewport_click_at_point(
        &mut self,
        rect: egui::Rect,
        position: egui::Pos2,
        snap: Option<SnapResult>,
        construction_point: Option<Vec3>,
    ) {
        self.handle_viewport_click_at_point_with_selection(
            rect,
            position,
            snap,
            construction_point,
            egui::Modifiers::default(),
            1,
        );
    }

    fn handle_viewport_click_at_point_with_selection(
        &mut self,
        rect: egui::Rect,
        position: egui::Pos2,
        snap: Option<SnapResult>,
        construction_point: Option<Vec3>,
        modifiers: egui::Modifiers,
        click_count: u8,
    ) {
        if !self.numeric_entry.text.is_empty() {
            self.numeric_entry.pointer = construction_point;
            self.finish_numeric_entry();
            return;
        }
        if architecture_workflow::active(&self.active_tool) {
            if let Some(point) = construction_point {
                self.arch_click(point, snap, None);
            }
            return;
        }
        if self.active_tool == "Planar Section" {
            if let Some(point) = construction_point {
                self.section_pick(point);
            }
            return;
        }
        if self.active_tool == "Paste" {
            if let Some(point) = construction_point {
                self.paste_at(point);
            }
            return;
        }
        if self.tool_cycle_state.hotspot_placement_active() || self.hotspot_reference_pick {
            let Some(point) = construction_point else {
                self.status = "Pick a snapped point in the construction view.".into();
                return;
            };
            let result = if self.hotspot_reference_pick {
                self.pick_hotspot_reference_at_world(point)
            } else {
                self.add_hotspot_at_world(point).map(|_| ())
            };
            if let Err(error) = result {
                self.status = error;
            }
            return;
        }
        if matches!(
            self.active_tool.as_str(),
            "Linear Dimension" | "Text" | "Leader"
        ) {
            if let Some(point) = construction_point {
                self.annotation_click(point);
            }
            return;
        }
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
            let point =
                construction_point.map(|point| (point, snap.map_or(Vec3::Y, |snap| snap.normal)));
            let Some((point, normal)) = point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
            let kind = if self.active_tool == "Axis Guide" {
                ConstructionGuideKind::Axis
            } else {
                ConstructionGuideKind::Grid
            };
            let cursor = SnapResult {
                point,
                normal,
                kind: snap.map_or(SnapKind::Grid, |s| s.kind),
                screen_distance: 0.,
            };
            if let Some(guide) = self.cursor_aligned_guide(kind, cursor) {
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
        if self.rotational_array_tool_state.active() {
            let Some(point) = construction_point else {
                self.status =
                    "Rotational array needs a point in the active construction view.".to_owned();
                return;
            };
            let selection_span = self.rotational_array_selection_span();
            match self
                .rotational_array_tool_state
                .pick_with_projected_span(point, selection_span)
            {
                Ok(RotationalArrayPick::NeedReference) => {
                    self.status = match self.rotational_array_tool_state.kind() {
                        Some(RotationalArrayKind::RotationalArray) => {
                            "Rotational Array: pick the first reference point (A).".to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArray) => {
                            "Helical Array: pick the first reference point (A).".to_owned()
                        }
                        Some(RotationalArrayKind::RotationalArrayEven) => {
                            "Rotational Array Even: pick the first reference point (A).".to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArrayEven) => {
                            "Helical Array Even: pick the first reference point (A).".to_owned()
                        }
                        _ => "Rotational array: pick the reference point (U).".to_owned(),
                    };
                }
                Ok(RotationalArrayPick::NeedIncrement) => {
                    self.status = match self.rotational_array_tool_state.kind() {
                        Some(RotationalArrayKind::RotationalArray) => {
                            "Rotational Array: pick the increment point (B).".to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArray) => {
                            "Helical Array: pick the helical unit point (B).".to_owned()
                        }
                        Some(RotationalArrayKind::RotationalArrayEven) => {
                            "Rotational Array Even: pick the angular hint point (B).".to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArrayEven) => {
                            "Helical Array Even: pick the helical unit point (B).".to_owned()
                        }
                        _ => "Rotational array: pick the angular increment point (V).".to_owned(),
                    };
                }
                Ok(RotationalArrayPick::NeedExtent) => {
                    self.status = match self.rotational_array_tool_state.kind() {
                        Some(RotationalArrayKind::RotationalArray) => {
                            "Rotational Array: pick the sweep end point (D).".to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArray) => {
                            "Helical Array: pick the sweep/height end point (D).".to_owned()
                        }
                        Some(RotationalArrayKind::RotationalArrayEven) => {
                            "Rotational Array Even: pick the full or partial sweep end point (D)."
                                .to_owned()
                        }
                        Some(RotationalArrayKind::HelicalArrayEven) => {
                            "Helical Array Even: pick the sweep/height end point (D).".to_owned()
                        }
                        Some(RotationalArrayKind::Helicoidal) => {
                            "Helicoidal Rotate Multiple: pick the sweep/lift end point (W)."
                                .to_owned()
                        }
                        _ => "Planar Rotate Multiple: pick the sweep end point (W).".to_owned(),
                    };
                }
                Ok(RotationalArrayPick::Complete { transforms, capped })
                    if transforms.is_empty() =>
                {
                    self.status = if capped {
                        "Rotational array produced no copies before reaching the copy limit."
                            .to_owned()
                    } else {
                        "Rotational array: no angular steps fit inside the sweep.".to_owned()
                    };
                }
                Ok(RotationalArrayPick::Complete { transforms, capped }) => {
                    let mut next_selection = self.selection.clone();
                    let history_label = self.active_tool.clone();
                    match self.commit_document_edit(&history_label, |document| {
                        modeling_copy::transformed_array(document, &mut next_selection, &transforms)
                    }) {
                        Ok(report) => {
                            self.selection = next_selection;
                            self.rotational_array_tool_state.cancel();
                            self.active_tool = "Select".to_owned();
                            self.status = if capped {
                                format!(
                                    "Created {} rotational array copies; the legacy copy limit was reached.",
                                    report.copies
                                )
                            } else {
                                format!("Created {} rotational array copies.", report.copies)
                            };
                        }
                        Err(error) => self.status = error,
                    }
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if self.array_tool_state.active() {
            let Some(point) = construction_point else {
                self.status =
                    "Array tool needs a point in the active construction view.".to_owned();
                return;
            };
            match self.array_tool_state.pick(point) {
                Ok(ArrayToolPick::NeedMeasure) => {
                    self.status = match self.array_tool_state.kind() {
                        Some(ArrayToolKind::CopyMultiple) => {
                            "Copy Multiple: pick the size point (V).".to_owned()
                        }
                        _ => format!("{}: pick the translation step (A).", self.active_tool),
                    };
                }
                Ok(ArrayToolPick::NeedExtent) => {
                    self.status = match self.array_tool_state.kind() {
                        Some(ArrayToolKind::CopyMultiple) => {
                            "Copy Multiple: pick the span point (D).".to_owned()
                        }
                        _ => format!("{}: pick the extent point (B).", self.active_tool),
                    };
                }
                Ok(ArrayToolPick::Complete { offsets, capped }) if offsets.is_empty() => {
                    self.status = if capped {
                        "Array produced no copies before reaching the copy limit.".to_owned()
                    } else {
                        "Array: no copies fit inside the measured extent.".to_owned()
                    };
                }
                Ok(ArrayToolPick::Complete { offsets, capped }) => {
                    let mut next_selection = self.selection.clone();
                    let history_label = format!("{} array", self.active_tool);
                    match self.commit_document_edit(&history_label, |document| {
                        modeling_copy::translated_array(document, &mut next_selection, &offsets)
                    }) {
                        Ok(report) => {
                            self.selection = next_selection;
                            self.array_tool_state.cancel();
                            self.active_tool = "Select".to_owned();
                            self.status = if capped {
                                format!(
                                    "Created {} array copies; the legacy copy limit was reached.",
                                    report.copies
                                )
                            } else {
                                format!("Created {} array copies.", report.copies)
                            };
                        }
                        Err(error) => self.status = error,
                    }
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if self.object_place_tool_state.active_definition().is_some() {
            let Some(point) = construction_point else {
                self.status =
                    "Object Place needs a point in the active construction view.".to_owned();
                return;
            };
            let hit = PlacementHit {
                position: point,
                surface_normal: snap
                    .filter(|result| result.kind == SnapKind::Face)
                    .map(|result| result.normal),
            };
            self.place_object_at_hit(hit);
            return;
        }

        if matches!(self.active_tool.as_str(), "Line" | "Construction Line") {
            self.sync_drawing_tool_state();
            let point = construction_point;
            let Some(point) = point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
            let effect = self.drawing_tool.handle(ToolEvent::Pick(point));
            self.handle_drawing_tool_effect(effect, snap);
            return;
        }

        if self.active_tool == "Mesh" {
            let Some(point) = construction_point else {
                self.status = "Mesh needs a point in the active construction view.".to_owned();
                return;
            };
            self.pick_mesh_point(point, snap.map(|result| result.normal));
            return;
        }

        if self.active_tool == "Push/Pull" {
            let Some(point) = construction_point else {
                self.status = "Push/Pull needs a point in the active construction view.".to_owned();
                return;
            };
            self.pick_push_pull_point(point);
            return;
        }

        if self.active_tool == "Line Offset" {
            let Some(point) = construction_point else {
                self.status =
                    "Line Offset needs a point in the active construction view.".to_owned();
                return;
            };
            self.pick_line_offset_point(point);
            return;
        }

        if matches!(
            self.active_tool.as_str(),
            "Polyline"
                | "Double Line"
                | "Extrude Swipe"
                | "Extrude Swipe 3D"
                | "Ribbon 3D"
                | "Freeform Slab"
        ) {
            let tool_name = self.active_tool.clone();
            if matches!(tool_name.as_str(), "Extrude Swipe" | "Extrude Swipe 3D")
                && self.selected_sweep_profile().is_err()
            {
                self.status =
                    "Select one or more profile edges in the active geometry graph first."
                        .to_owned();
                return;
            }
            let Some(point) = construction_point else {
                self.status = format!("{tool_name} needs a point in the active construction view.");
                return;
            };
            let effect = self
                .polyline_tool_state
                .pick(point, self.document.model_settings().arc_max_length);
            match effect {
                Ok(PolylinePickEffect::Started) => {
                    self.status = if matches!(tool_name.as_str(), "Extrude Swipe 3D" | "Ribbon 3D")
                    {
                        format!(
                            "{tool_name}: choose the next spatial path point or press Enter to finish."
                        )
                    } else {
                        format!(
                            "{tool_name}: choose the next point; A arc through, C arc center, L line."
                        )
                    };
                }
                Ok(PolylinePickEffect::Added) => {
                    self.status =
                        format!("{tool_name}: choose the next point or press Enter to finish.");
                }
                Ok(PolylinePickEffect::NeedArcEndpoint) => {
                    self.status = match self.polyline_tool_state.mode() {
                        PolylineSegmentMode::ArcThrough => {
                            format!("{tool_name} arc: choose its endpoint.")
                        }
                        PolylineSegmentMode::ArcCenter => {
                            format!("{tool_name} arc: choose its endpoint after the center.")
                        }
                        PolylineSegmentMode::Line => format!("{tool_name}: choose the next point."),
                    };
                }
                Ok(PolylinePickEffect::Closed) => {
                    self.finish_polyline(true);
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if self.revolve_tool_state.active() {
            if self.selected_sweep_profile().is_err() {
                self.status =
                    "Select one or more profile edges in the active geometry graph first."
                        .to_owned();
                return;
            }
            let Some(point) = construction_point else {
                self.status = "Revolve needs points in the active construction view.".to_owned();
                return;
            };
            match self.revolve_tool_state.pick(point) {
                Ok(RevolvePick::AxisOrigin) => {
                    self.status = "Revolve: pick a point on the axis direction.".to_owned();
                }
                Ok(RevolvePick::AxisDirection) => {
                    self.status =
                        "Revolve: type an angle in degrees, or press Enter for 360°.".to_owned();
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if let Some(kind) = self.primitive_tool_state.active() {
            let Some(point) = construction_point else {
                self.status = format!(
                    "{} needs a point in the active construction view.",
                    kind.label()
                );
                return;
            };
            match self.primitive_tool_state.pick(point) {
                Ok(PrimitivePick::NeedPoint { prompt }) => self.status = prompt.to_owned(),
                Ok(PrimitivePick::Complete(picks)) => {
                    let (_, _, inverse) = self.line_geometry_context();
                    let local_picks = picks
                        .into_iter()
                        .map(|point| inverse.transform_point(point))
                        .collect::<Vec<_>>();
                    match modeling_primitives::build_primitive(
                        kind,
                        &local_picks,
                        self.document.model_settings().circle_segments,
                    ) {
                        Ok(mesh) => match self.create_primitive_in_active_context(mesh, kind) {
                            Ok(targets) => {
                                let edges = targets
                                    .iter()
                                    .filter(|target| {
                                        matches!(target.entity, SelectedEntity::Segment(_))
                                    })
                                    .count();
                                let faces = targets.len() - edges;
                                self.selection.apply(targets, SelectionMode::Replace);
                                self.status = format!(
                                    "Created {} with {faces} face(s) and {edges} edge(s) in the active geometry graph.",
                                    kind.label()
                                );
                            }
                            Err(error) => self.status = error,
                        },
                        Err(error) => self.status = error.to_owned(),
                    }
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if self.transform_tool_state.active().is_some() {
            let Some(point) = construction_point else {
                self.status = "Transform needs a point in the active construction view.".to_owned();
                return;
            };
            let previous_state = self.transform_tool_state.clone();
            match self.transform_tool_state.pick(point) {
                TransformPick::NeedPoint { prompt, .. } => self.status = prompt.to_owned(),
                TransformPick::Commit(commit) => match self.commit_transform(commit) {
                    Ok(report) => self.status = transform_status(&report, commit.copy),
                    Err(error) => {
                        self.transform_tool_state = previous_state;
                        self.status = error;
                    }
                },
                TransformPick::Invalid(error) => self.status = error,
            }
            return;
        }

        if self.active_tool == "Paint" {
            let scope = match &self.selection.context {
                EditContext::Document => SnapScope::Document,
                EditContext::Definition(definition) => SnapScope::Definition(*definition),
                EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
            };
            let Some(target) = self.viewport.pick_selection_target(
                rect,
                &self.document,
                position,
                scope,
                &self.selection_display_policy,
            ) else {
                self.status = "Paint: click a visible edge or face.".to_owned();
                return;
            };
            self.paint_native_target(Some(target));
            return;
        }

        if let Some(shape_tool) = self.shape_tool_state.active() {
            let Some(point) = construction_point else {
                self.status = "The current view has no construction-plane intersection.".to_owned();
                return;
            };
            let pick_normal = snap.map(|result| result.normal);
            let fallback_normal = self.viewport.construction_plane_normal();
            let pick_count = self.shape_tool_state.picks().len();
            match self.shape_tool_state.pick(
                point,
                pick_normal,
                fallback_normal,
                self.document.model_settings().circle_segments,
            ) {
                Ok(None) => {
                    self.status = match (shape_tool, pick_count) {
                        (PlanarShapeTool::Rectangle, _) => {
                            "Rectangle: choose the opposite corner.".to_owned()
                        }
                        (PlanarShapeTool::SurfaceRectangle, _) => {
                            "Surface Rect: choose the opposite rectangle corner.".to_owned()
                        }
                        (PlanarShapeTool::Quad, 0) => "Quad: pick the second point.".to_owned(),
                        (PlanarShapeTool::Quad, _) => "Quad: pick the third point.".to_owned(),
                        (PlanarShapeTool::Circle, _) => "Circle: choose a radius point.".to_owned(),
                    };
                }
                Ok(Some(mesh)) => {
                    match self.create_planar_shape_in_active_context(mesh, shape_tool.label()) {
                        Ok(targets) => {
                            self.selection.apply(targets, SelectionMode::Replace);
                            self.status = format!(
                                "Created {} in the active geometry graph.",
                                shape_tool.label()
                            );
                        }
                        Err(error) => self.status = error,
                    }
                }
                Err(error) => self.status = error.to_owned(),
            }
            return;
        }

        if self.active_tool == "Cuboid" {
            let Some(point) = snap.map(|snap| snap.point).or(construction_point) else {
                self.status = "Cuboid needs two snapped 3D corner points.".to_owned();
                return;
            };
            let Some(first) = self.cuboid_first else {
                self.cuboid_first = Some(point);
                self.status = "Cuboid: choose the opposite 3D corner.".to_owned();
                return;
            };
            let Some(preview) = cuboid_preview_from_corners(first, point) else {
                self.status =
                    "Cuboid corners must differ by at least 0.05 units on every axis.".to_owned();
                return;
            };
            match self.create_cuboid_in_active_context(preview) {
                Ok(targets) => {
                    self.selection.apply(targets, SelectionMode::Replace);
                    self.active_tool = "Select".to_owned();
                    self.cuboid_first = None;
                    self.status = format!(
                        "Created cuboid {:.2} × {:.2} × {:.2}.",
                        preview.dimensions.x, preview.dimensions.y, preview.dimensions.z
                    );
                }
                Err(error) => self.status = error,
            }
            return;
        }

        if self.active_tool == "Select" {
            if modifiers.alt {
                self.selection_click_tracker.reset();
                self.status = "Alt-drag to select a screen window.".to_owned();
                return;
            }
            if let Some(first) = self.pending_selection_volume.take() {
                let Some(second) = construction_point else {
                    self.pending_selection_volume = Some(first);
                    self.status = "Volume select needs a second world-space corner.".to_owned();
                    return;
                };
                let scope = match &self.selection.context {
                    EditContext::Document => SnapScope::Document,
                    EditContext::Definition(definition) => SnapScope::Definition(*definition),
                    EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
                };
                let targets = self.viewport.select_world_volume(
                    &self.document,
                    first,
                    second,
                    scope,
                    &self.selection_display_policy,
                );
                let mode = if modifiers.ctrl || modifiers.mac_cmd {
                    SelectionMode::Remove
                } else if modifiers.shift {
                    SelectionMode::Add
                } else {
                    SelectionMode::Replace
                };
                let targets = targets
                    .into_iter()
                    .map(|t| self.native_normalize_selection_target(t))
                    .filter(|t| self.native_target_editable(t))
                    .collect::<Vec<_>>();
                self.selection.apply(targets, mode);
                self.selection_click_tracker.reset();
                let summary = self.selection.summary();
                self.status = format!(
                    "Volume selection: {} object(s), {} face(s), {} edge(s), {} vertex/vertices.",
                    summary.objects, summary.faces, summary.edges, summary.vertices
                );
                return;
            }
            let scope = match &self.selection.context {
                EditContext::Document => SnapScope::Document,
                EditContext::Definition(definition) => SnapScope::Definition(*definition),
                EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
            };
            let target = self.viewport.pick_selection_target(
                rect,
                &self.document,
                position,
                scope,
                &self.selection_display_policy,
            );
            if target.is_none()
                && let Some(point) = construction_point
            {
                self.pending_selection_volume = Some(point);
                self.selection_click_tracker.reset();
                self.status = "Volume select: pick the second world-space corner.".to_owned();
                return;
            }
            if click_count >= 3
                && let Some(target) = target.as_ref()
            {
                let connected = self.connected_selection_component(target);
                self.selection.apply(connected, SelectionMode::Replace);
                self.status = format!(
                    "{} Connected entity/entities selected.",
                    self.selection.summary().total()
                );
                return;
            }
            if click_count == 2
                && !modifiers.shift
                && !modifiers.ctrl
                && !modifiers.mac_cmd
                && let Some(target) = target.as_ref()
            {
                if let Some(group) = self.editable_group_ancestor(target) {
                    if self.block_gpu_object_edit(&group) {
                        return;
                    }
                    self.selection
                        .set_context(EditContext::Occurrence(group.clone()));
                    self.selection_click_tracker.reset();
                    self.status = format!("Editing object instance at path {:?}.", group.path);
                    return;
                }
                match &target.entity {
                    SelectedEntity::Occurrence => {
                        if self.block_gpu_object_edit(&target.occurrence) {
                            return;
                        }
                        self.selection
                            .set_context(EditContext::Occurrence(target.occurrence.clone()));
                        self.selection_click_tracker.reset();
                        self.status = format!(
                            "Editing object instance at path {:?}.",
                            target.occurrence.path
                        );
                        return;
                    }
                    SelectedEntity::Triangle(_) => {
                        let coplanar = self.coplanar_faces(target);
                        self.selection.apply(coplanar, SelectionMode::Replace);
                        self.status = format!(
                            "{} Coplanar face(s) selected.",
                            self.selection.summary().faces
                        );
                        return;
                    }
                    SelectedEntity::Segment(_) => {
                        let connected = self.connected_line_segments(target);
                        self.selection.apply(connected, SelectionMode::Replace);
                        self.status = format!(
                            "{} Connected line segment(s) selected.",
                            self.selection.summary().edges
                        );
                        return;
                    }
                    _ => {}
                }
            }
            let mode = if modifiers.ctrl || modifiers.mac_cmd {
                SelectionMode::Remove
            } else if modifiers.shift {
                SelectionMode::Add
            } else if target
                .as_ref()
                .is_some_and(|target| self.selection.selected().contains(target))
            {
                SelectionMode::Remove
            } else {
                SelectionMode::Replace
            };
            self.selection.apply(target, mode);
            let summary = self.selection.summary();
            self.status = if summary.total() == 0 {
                "Selection cleared.".to_owned()
            } else {
                format!(
                    "Selected {} object(s), {} face(s), {} edge(s), {} vertex/vertices.",
                    summary.objects, summary.faces, summary.edges, summary.vertices
                )
            };
        }
    }

    fn editable_group_ancestor(&self, target: &SelectionTarget) -> Option<OccurrenceRef> {
        for length in (1..=target.occurrence.path.len()).rev() {
            let occurrence = OccurrenceRef {
                path: target.occurrence.path[..length].to_vec(),
            };
            if self
                .document
                .resolve_occurrence(&occurrence)
                .is_some_and(|resolved| resolved.definition.default_child_layout.is_some())
            {
                return Some(occurrence);
            }
        }
        None
    }

    fn coplanar_faces(&self, target: &SelectionTarget) -> Vec<SelectionTarget> {
        let SelectedEntity::Triangle(start_id) = &target.entity else {
            return vec![target.clone()];
        };
        let Some(geometry) = self
            .document
            .geometry_in_context(&target.occurrence, &self.selection.context)
        else {
            return vec![target.clone()];
        };
        let connections = GeometryConnections::new(geometry);
        let triangle_data = |triangle: &meshcraft_core::document::Triangle| {
            let [Some(a), Some(b), Some(c)] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference))
            else {
                return None;
            };
            let normal = (b - a).cross(c - a).try_normalized(1.0e-12)?;
            Some((
                triangle
                    .vertices
                    .map(|vertex| connections.representative(vertex.id)),
                a,
                normal,
            ))
        };
        let Some(start_triangle) = geometry.triangles.get(start_id) else {
            return vec![target.clone()];
        };
        let Some((_, start_point, start_normal)) = triangle_data(start_triangle) else {
            return vec![target.clone()];
        };
        let is_coplanar = |triangle: &meshcraft_core::document::Triangle| {
            triangle_data(triangle).is_some_and(|(_, point, normal)| {
                normal.dot(start_normal).abs() >= 1.0 - 1.0e-8
                    && (point - start_point).dot(start_normal).abs() <= CONNECTION_DISTANCE
            })
        };
        let mut coplanar_vertices = BTreeMap::new();
        let mut edge_faces = BTreeMap::<(VertexId, VertexId), Vec<_>>::new();
        for triangle in geometry
            .triangles
            .values()
            .filter(|triangle| is_coplanar(triangle))
        {
            let Some((vertices, _, _)) = triangle_data(triangle) else {
                continue;
            };
            coplanar_vertices.insert(triangle.id, vertices);
            for [first, second] in [[0, 1], [1, 2], [2, 0]] {
                let mut edge = [vertices[first], vertices[second]];
                edge.sort_unstable();
                edge_faces
                    .entry((edge[0], edge[1]))
                    .or_default()
                    .push(triangle.id);
            }
        }
        let mut selected = BTreeSet::new();
        let mut pending = vec![*start_id];
        while let Some(triangle_id) = pending.pop() {
            if !selected.insert(triangle_id) {
                continue;
            }
            let Some(vertices) = coplanar_vertices.get(&triangle_id) else {
                continue;
            };
            for [first, second] in [[0, 1], [1, 2], [2, 0]] {
                let mut edge = [vertices[first], vertices[second]];
                edge.sort_unstable();
                if let Some(neighbors) = edge_faces.get(&(edge[0], edge[1])) {
                    pending.extend(
                        neighbors
                            .iter()
                            .copied()
                            .filter(|neighbor| !selected.contains(neighbor)),
                    );
                }
            }
        }
        selected
            .into_iter()
            .map(|id| SelectionTarget {
                occurrence: target.occurrence.clone(),
                entity: SelectedEntity::Triangle(id),
            })
            .collect()
    }

    fn connected_line_segments(&self, target: &SelectionTarget) -> Vec<SelectionTarget> {
        let SelectedEntity::Segment(start_id) = target.entity else {
            return vec![target.clone()];
        };
        let Some(geometry) = self
            .document
            .geometry_in_context(&target.occurrence, &self.selection.context)
        else {
            return vec![target.clone()];
        };
        let Some(start) = geometry.segments.get(&start_id) else {
            return vec![target.clone()];
        };
        let connections = GeometryConnections::new(geometry);
        let mut by_vertex = BTreeMap::<VertexId, Vec<_>>::new();
        for segment in geometry.segments.values() {
            for vertex in segment.endpoints {
                by_vertex
                    .entry(connections.representative(vertex.id))
                    .or_default()
                    .push(segment.id);
            }
        }
        let mut pending = start
            .endpoints
            .map(|r| connections.representative(r.id))
            .to_vec();
        let mut vertices = BTreeSet::new();
        let mut segments = BTreeSet::new();
        while let Some(vertex) = pending.pop() {
            if !vertices.insert(vertex) {
                continue;
            }
            for id in by_vertex.get(&vertex).into_iter().flatten() {
                if segments.insert(*id) {
                    pending.extend(
                        geometry.segments[id]
                            .endpoints
                            .map(|r| connections.representative(r.id)),
                    );
                }
            }
        }
        segments
            .into_iter()
            .map(|id| SelectionTarget {
                occurrence: target.occurrence.clone(),
                entity: SelectedEntity::Segment(id),
            })
            .collect()
    }

    fn connected_selection_component(&self, target: &SelectionTarget) -> Vec<SelectionTarget> {
        let Some(geometry) = self
            .document
            .geometry_in_context(&target.occurrence, &self.selection.context)
        else {
            return vec![target.clone()];
        };
        let mut vertices = BTreeSet::new();
        match &target.entity {
            SelectedEntity::Occurrence => return vec![target.clone()],
            SelectedEntity::SemanticHandle { .. } => return vec![target.clone()],
            SelectedEntity::Vertex(id) => {
                vertices.insert(*id);
            }
            SelectedEntity::Segment(id) => {
                if let Some(segment) = geometry.segments.get(id) {
                    vertices.extend(segment.endpoints.map(|vertex| vertex.id));
                }
            }
            SelectedEntity::Triangle(id) => {
                if let Some(triangle) = geometry.triangles.get(id) {
                    vertices.extend(triangle.vertices.map(|vertex| vertex.id));
                }
            }
        }
        if vertices.is_empty() {
            return vec![target.clone()];
        }
        let connections = GeometryConnections::new(geometry);
        let mut by_vertex = BTreeMap::<VertexId, Vec<SelectedEntity>>::new();
        for s in geometry.segments.values() {
            for r in s.endpoints {
                by_vertex
                    .entry(connections.representative(r.id))
                    .or_default()
                    .push(SelectedEntity::Segment(s.id));
            }
        }
        for t in geometry.triangles.values() {
            for r in t.vertices {
                by_vertex
                    .entry(connections.representative(r.id))
                    .or_default()
                    .push(SelectedEntity::Triangle(t.id));
            }
        }
        let mut pending = vertices
            .iter()
            .map(|id| connections.representative(*id))
            .collect::<Vec<_>>();
        vertices.clear();
        let mut segments = BTreeSet::new();
        let mut triangles = BTreeSet::new();
        while let Some(vertex) = pending.pop() {
            if !vertices.insert(vertex) {
                continue;
            }
            for entity in by_vertex.get(&vertex).into_iter().flatten() {
                match *entity {
                    SelectedEntity::Segment(id) if segments.insert(id) => pending.extend(
                        geometry.segments[&id]
                            .endpoints
                            .map(|r| connections.representative(r.id)),
                    ),
                    SelectedEntity::Triangle(id) if triangles.insert(id) => pending.extend(
                        geometry.triangles[&id]
                            .vertices
                            .map(|r| connections.representative(r.id)),
                    ),
                    _ => {}
                }
            }
        }
        let mut targets = geometry
            .vertices
            .keys()
            .copied()
            .filter(|id| vertices.contains(&connections.representative(*id)))
            .map(|id| SelectionTarget {
                occurrence: target.occurrence.clone(),
                entity: SelectedEntity::Vertex(id),
            })
            .collect::<Vec<_>>();
        targets.extend(segments.into_iter().map(|id| SelectionTarget {
            occurrence: target.occurrence.clone(),
            entity: SelectedEntity::Segment(id),
        }));
        targets.extend(triangles.into_iter().map(|id| SelectionTarget {
            occurrence: target.occurrence.clone(),
            entity: SelectedEntity::Triangle(id),
        }));
        targets
    }

    fn tool_preview(
        &mut self,
        rect: egui::Rect,
        pointer: Option<egui::Pos2>,
        snap: Option<SnapResult>,
    ) -> Option<ToolPreview> {
        if self.active_tool == "Planar Section" {
            return snap.map(|s| self.section_preview(s.point));
        }
        let raw_point = snap
            .map(|s| s.point)
            .or_else(|| pointer.and_then(|p| self.viewport.screen_to_construction_plane(rect, p)));
        let snap = raw_point.map(|raw| {
            let point = self.numeric_preview_point(raw);
            if self.push_pull_reference.is_some() {
                self.push_pull_pointer = Some(point);
            }
            self.transform_tool_state.set_pointer(Some(point));
            snap.map_or(
                SnapResult {
                    point,
                    normal: self.preferences.grid_normal_axis.vector(),
                    kind: SnapKind::Free,
                    screen_distance: 0.,
                },
                |mut s| {
                    s.point = point;
                    s
                },
            )
        });
        if architecture_workflow::active(&self.active_tool) {
            let point = snap.map(|s| s.point).or_else(|| {
                pointer.and_then(|p| self.viewport.screen_to_construction_plane(rect, p))
            })?;
            return self
                .arch_preview(point)
                .or(Some(ToolPreview::Point { position: point }));
        }
        if self.active_tool == "Paste" {
            let point = snap.map(|s| s.point).or_else(|| {
                pointer.and_then(|p| self.viewport.screen_to_construction_plane(rect, p))
            });
            return self.paste_preview_at(point);
        }
        if matches!(
            self.active_tool.as_str(),
            "Linear Dimension" | "Text" | "Leader"
        ) {
            return snap.and_then(|s| self.annotation_preview(s.point));
        }
        if self.rotational_array_tool_state.active() {
            let point = snap.map(|result| result.point).or_else(|| {
                pointer
                    .and_then(|position| self.viewport.screen_to_construction_plane(rect, position))
            });
            let selection_span = self.rotational_array_selection_span();
            let (transforms, capped) = self
                .rotational_array_tool_state
                .preview_transforms_with_projected_span(point?, selection_span)?;
            if transforms.is_empty()
                || !modeling_copy::can_linear_array_selection(&self.document, &self.selection)
            {
                return None;
            }
            let edges = modeling_transforms::preview_selection_transform(
                &self.document,
                &self.selection,
                Affine3::IDENTITY,
            )
            .ok()?;
            return (!edges.is_empty()).then_some(ToolPreview::ArrayTransforms {
                edges,
                transforms,
                capped,
            });
        }
        if self.array_tool_state.active() {
            let point = snap.map(|result| result.point).or_else(|| {
                pointer
                    .and_then(|position| self.viewport.screen_to_construction_plane(rect, position))
            });
            let (offsets, capped) = self.array_tool_state.preview_offsets(point?)?;
            if offsets.is_empty()
                || !modeling_copy::can_linear_array_selection(&self.document, &self.selection)
            {
                return None;
            }
            let edges = modeling_transforms::preview_selection_transform(
                &self.document,
                &self.selection,
                Affine3::IDENTITY,
            )
            .ok()?;
            return (!edges.is_empty()).then_some(ToolPreview::ArrayOffsets {
                edges,
                offsets,
                capped,
            });
        }
        if self.array_preview {
            if !modeling_copy::can_linear_array_selection(&self.document, &self.selection) {
                self.array_preview = false;
                return None;
            }
            if self.array_count == 0
                || self.array_count > modeling_copy::MAX_LINEAR_ARRAY_COPIES
                || !self.array_spacing.is_finite()
                || self.array_spacing.abs() <= GeometryTolerance::default().absolute
            {
                return None;
            }
            let edges = modeling_transforms::preview_selection_transform(
                &self.document,
                &self.selection,
                Affine3::IDENTITY,
            )
            .ok()?;
            if !edges.is_empty() {
                return Some(ToolPreview::Array {
                    edges,
                    copies: self.array_count,
                    displacement: Vec3::X * self.array_spacing,
                });
            }
            return None;
        }
        let position = pointer?;
        let point = snap
            .map(|result| result.point)
            .or_else(|| self.viewport.screen_to_construction_plane(rect, position));
        if self.tool_cycle_state.hotspot_placement_active() {
            return point.map(|position| ToolPreview::Point { position });
        }
        if self.revolve_tool_state.active() {
            let point_position = point?;
            self.revolve_tool_state.set_pointer(Some(point_position));
            match self.revolve_tool_state.axis_points() {
                [] => return point.and_then(|position| self.feedback_preview(position)),
                [origin] => {
                    return Some(ToolPreview::Line {
                        start: *origin,
                        end: point_position,
                    });
                }
                [_, _] => {
                    return self
                        .revolve_preview()
                        .or_else(|| point.and_then(|position| self.feedback_preview(position)));
                }
                _ => unreachable!("Revolve only accepts two axis points"),
            }
        }
        if let Some(definition) = self.object_place_tool_state.active_definition() {
            if let Some(point) = point {
                let hit = PlacementHit {
                    position: point,
                    surface_normal: snap.filter(|s| s.kind == SnapKind::Face).map(|s| s.normal),
                };
                if let Ok(preview) = self.object_placement_preview(definition, hit) {
                    return Some(preview);
                }
            }
            return point.and_then(|position| self.feedback_preview(position));
        }
        let point_preview =
            |this: &Self| point.and_then(|position| this.feedback_preview(position));
        if self.active_tool == "Paint" {
            let scope = match &self.selection.context {
                EditContext::Document => SnapScope::Document,
                EditContext::Definition(definition) => SnapScope::Definition(*definition),
                EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
            };
            let mut targets = self
                .selection
                .selected()
                .iter()
                .filter(|target| {
                    matches!(
                        target.entity,
                        SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
                    )
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            if let Some(target) = self.viewport.pick_selection_target(
                rect,
                &self.document,
                position,
                scope,
                &self.selection_display_policy,
            ) && matches!(
                target.entity,
                SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
            ) {
                targets.insert(target);
            }
            if targets.is_empty() {
                return None;
            }
            let mut edges = Vec::new();
            let mut triangles = Vec::new();
            for target in targets {
                let Some(resolved) = self.document.resolve_occurrence(&target.occurrence) else {
                    continue;
                };
                let Some(geometry) = self
                    .document
                    .geometry_in_context(&target.occurrence, &self.selection.context)
                else {
                    continue;
                };
                match target.entity {
                    SelectedEntity::Segment(id) => {
                        let Some(segment) = geometry.segments.get(&id) else {
                            continue;
                        };
                        let [Some(start), Some(end)] = segment
                            .endpoints
                            .map(|vertex| geometry.vertex_position(vertex))
                        else {
                            continue;
                        };
                        edges.push([
                            resolved.world_transform.transform_point(start),
                            resolved.world_transform.transform_point(end),
                        ]);
                    }
                    SelectedEntity::Triangle(id) => {
                        let Some(triangle) = geometry.triangles.get(&id) else {
                            continue;
                        };
                        let [Some(a), Some(b), Some(c)] = triangle
                            .vertices
                            .map(|vertex| geometry.vertex_position(vertex))
                        else {
                            continue;
                        };
                        triangles.push(
                            [a, b, c].map(|point| resolved.world_transform.transform_point(point)),
                        );
                    }
                    _ => continue,
                }
            }
            return Some(ToolPreview::Paint {
                edges,
                triangles,
                color: self.current_paint_color,
            });
        }
        if self.transform_tool_state.active().is_some() {
            let point = point?;
            let cached = matches!(
                self.transform_tool_state.active(),
                Some(
                    TransformTool::Move
                        | TransformTool::Rotate
                        | TransformTool::Rotate2
                        | TransformTool::Mirror
                )
            );
            if let Ok(transforms) = self.numeric_copy_transforms(point, 64) {
                let edges = if cached {
                    self.cached_transform_wireframe(Affine3::IDENTITY)
                } else {
                    modeling_transforms::preview_selection_transform(
                        &self.document,
                        &self.selection,
                        Affine3::IDENTITY,
                    )
                }
                .ok()?;
                let capped = meshcraft_core::tools::numeric::parse(
                    &self.numeric_entry.text,
                    self.document.model_settings(),
                )
                .ok()
                .and_then(|e| e.repetition)
                .is_some_and(|r| r.count > transforms.len());
                return Some(ToolPreview::ArrayTransforms {
                    edges,
                    transforms,
                    capped,
                });
            }
            let preview = if matches!(
                self.transform_tool_state.active(),
                Some(
                    TransformTool::Stretch
                        | TransformTool::StretchScale
                        | TransformTool::RotateStretch
                )
            ) {
                modeling_transforms::preview_stretch_selection_transform
            } else {
                modeling_transforms::preview_selection_transform
            };
            let mut edges = self
                .transform_tool_state
                .preview(Some(point))
                .and_then(|transform| {
                    if cached {
                        self.cached_transform_wireframe(transform).ok()
                    } else {
                        preview(&self.document, &self.selection, transform).ok()
                    }
                })
                .unwrap_or_default();
            if let Some(rotation) = self.transform_tool_state.rotation_feedback(point) {
                edges.extend(rotation.edges());
            }
            if self.transform_tool_state.active() == Some(TransformTool::Move)
                && let Some(origin) = self.transform_tool_state.points().first()
            {
                edges.push([*origin, point]);
            }
            if !edges.is_empty() {
                return Some(ToolPreview::Transform { edges });
            }
            return point_preview(self);
        }
        if let Some(kind) = self.primitive_tool_state.active() {
            let point = point?;
            let Some(world_picks) = self.primitive_tool_state.preview_picks(point) else {
                return point_preview(self);
            };
            if world_picks.len() < kind.pick_count() {
                return Some(ToolPreview::Line {
                    start: *world_picks.last().unwrap_or(&point),
                    end: point,
                });
            }
            let (_, _, inverse) = self.line_geometry_context();
            let local_picks = world_picks
                .iter()
                .map(|point| inverse.transform_point(*point))
                .collect::<Vec<_>>();
            let mesh = modeling_primitives::build_primitive(
                kind,
                &local_picks,
                self.document.model_settings().circle_segments,
            )
            .ok()?;
            let world_transform = inverse.inverse(GeometryTolerance::default())?;
            let edges = mesh
                .segments
                .iter()
                .map(|[start, end]| {
                    [
                        world_transform.transform_point(mesh.vertices[*start]),
                        world_transform.transform_point(mesh.vertices[*end]),
                    ]
                })
                .collect();
            let color = self.current_face_color();
            let triangles = mesh
                .triangles
                .iter()
                .map(|triangle| {
                    (
                        triangle.map(|index| world_transform.transform_point(mesh.vertices[index])),
                        color,
                    )
                })
                .collect();
            return Some(ToolPreview::Primitive { edges, triangles });
        }
        if self.shape_tool_state.active().is_some() {
            let point = point?;
            return Some(
                self.shape_tool_state
                    .preview(
                        point,
                        self.viewport.construction_plane_normal(),
                        self.document.model_settings().circle_segments,
                    )
                    .map(|preview| ToolPreview::PlanarShape {
                        outline: preview.outline,
                        closed: preview.closed,
                        measurements: preview.measurements,
                    })
                    .unwrap_or_else(|| {
                        self.feedback_preview(point)
                            .unwrap_or(ToolPreview::Point { position: point })
                    }),
            );
        }
        if self.polyline_tool_state.active() {
            let point = point?;
            let Some(preview) = self
                .polyline_tool_state
                .preview(point, self.document.model_settings().arc_max_length)
            else {
                if self.active_tool == "Extrude Swipe"
                    && let Some(preview) = self.extrude_swipe_preview(None)
                {
                    return Some(preview);
                }
                if matches!(self.active_tool.as_str(), "Extrude Swipe 3D" | "Ribbon 3D")
                    && let Some(preview) = self.spatial_sweep_preview(None)
                {
                    return Some(preview);
                }
                return point_preview(self);
            };
            if self.active_tool == "Double Line" {
                let (_, occurrence, inverse) = self.line_geometry_context();
                let local_path = PolylinePath {
                    points: preview
                        .points
                        .iter()
                        .map(|point| inverse.transform_point(*point))
                        .collect(),
                    closed: preview.closed,
                };
                let settings = self.document.model_settings();
                let local_plane_normal = inverse
                    .transform_vector(self.viewport.construction_plane_normal())
                    .try_normalized(1.0e-12)
                    .unwrap_or(Vec3::Y);
                let mesh = build_double_line_mesh_in_plane(
                    &local_path,
                    settings.double_line_size,
                    settings.double_line_offset,
                    local_plane_normal,
                )
                .ok();
                let Some(mesh) = mesh else {
                    return point_preview(self);
                };
                let world_transform = self
                    .document
                    .resolve_occurrence(&occurrence)
                    .map(|resolved| resolved.world_transform);
                let Some(world_transform) = world_transform else {
                    return point_preview(self);
                };
                let edges = mesh
                    .segments
                    .iter()
                    .map(|[start, end]| {
                        [
                            world_transform.transform_point(mesh.vertices[*start]),
                            world_transform.transform_point(mesh.vertices[*end]),
                        ]
                    })
                    .collect();
                return Some(ToolPreview::Transform { edges });
            }
            if self.active_tool == "Extrude Swipe"
                && let Some(preview) = self.extrude_swipe_preview(Some(&preview))
            {
                return Some(preview);
            }
            if matches!(self.active_tool.as_str(), "Extrude Swipe 3D" | "Ribbon 3D")
                && let Some(preview) = self.spatial_sweep_preview(Some(&preview))
            {
                return Some(preview);
            }
            return Some(ToolPreview::Polyline {
                outline: preview.points,
                closed: preview.closed,
            });
        }
        if self.mesh_tool_state.active() {
            let edges = self.mesh_tool_state.preview();
            if !edges.is_empty() {
                return Some(ToolPreview::Transform { edges });
            }
            return point_preview(self);
        }
        if let (Some(reference), Some(target)) = (
            self.line_offset_tool_state.reference(),
            self.line_offset_tool_state.pointer(),
        ) {
            let result = build_line_offset_mesh_from_segments(
                &self.selected_world_segments(),
                reference,
                target,
                LINE_OFFSET_MERGE_TOLERANCE,
            );
            let (edges, signed_offset) = match result {
                Ok(result) => (result.preview_edges, result.offsets.first().copied()),
                Err(_) => (vec![[reference, target]], None),
            };
            if !edges.is_empty() {
                return Some(ToolPreview::LineOffset {
                    edges,
                    target,
                    signed_offset,
                });
            }
        }
        if self.active_tool == "Line Offset" {
            return point_preview(self);
        }
        if let (Some(reference), Some(displacement)) =
            (self.push_pull_reference, self.push_pull_displacement())
        {
            let selected = self
                .push_pull_faces
                .iter()
                .map(|(_, face)| *face)
                .collect::<Vec<_>>();
            if let Ok(mesh) = extrude_region(&selected, displacement, FACE_OUTLINE_MERGE_TOLERANCE)
            {
                let triangles = mesh
                    .triangles
                    .iter()
                    .map(|triangle| {
                        (
                            triangle.vertices.map(|index| mesh.vertices[index]),
                            triangle.color,
                        )
                    })
                    .collect();
                let mut edges = mesh
                    .segments
                    .iter()
                    .map(|segment| segment.endpoints.map(|index| mesh.vertices[index]))
                    .collect::<Vec<_>>();
                edges.push([reference, reference + displacement]);
                return Some(ToolPreview::PushPull { edges, triangles });
            }
            return Some(ToolPreview::Line {
                start: reference,
                end: reference + displacement,
            });
        }
        if self.active_tool == "Push/Pull" {
            return point_preview(self);
        }
        match self.active_tool.as_str() {
            "Cuboid" => {
                let point = snap
                    .map(|snap| snap.point)
                    .or_else(|| self.viewport.screen_to_construction_plane(rect, position))?;
                let Some(first) = self.cuboid_first else {
                    return self.feedback_preview(point);
                };
                cuboid_preview_from_corners(first, point)
                    .map(ToolPreview::Cuboid)
                    .or_else(|| self.feedback_preview(point))
            }
            "Line" | "Construction Line" => {
                let preview = self.drawing_tool.preview(point);
                preview.segment.map_or_else(
                    || point_preview(self),
                    |[start, end]| Some(ToolPreview::Line { start, end }),
                )
            }
            "Axis Guide" | "Grid Guide" => point_preview(self),
            _ => None,
        }
    }

    fn tool_feedback(
        &self,
        rect: egui::Rect,
        pointer: Option<egui::Pos2>,
        snap: Option<SnapResult>,
        preview: Option<&ToolPreview>,
    ) -> Option<ToolFeedback> {
        let screen_position = pointer?;
        if !self.array_preview
            && ((self.active_tool == "Select" && !self.tool_cycle_state.hotspot_placement_active())
                || matches!(
                    self.active_tool.as_str(),
                    "Orbit" | "Pan" | "Zoom" | "Remove Guide"
                ))
        {
            return None;
        }

        let units = self.document.model_settings();
        let point = snap.map(|snap| snap.point).or_else(|| {
            self.viewport
                .screen_to_construction_plane(rect, screen_position)
        });
        let feedback_label = if self.tool_cycle_state.hotspot_placement_active() {
            "Hotspot"
        } else {
            self.active_tool.as_str()
        };
        let mut measurement = point
            .map(|point| {
                self.tool_reference_point().map_or_else(
                    || point_feedback_text(feedback_label, point, units),
                    |reference| vector_feedback_text(feedback_label, point - reference, units),
                )
            })
            .unwrap_or_default();
        if let Some(point) = point
            && let Some(center) = self.rotational_reference_points().first()
        {
            let vectors = self
                .rotational_reference_points()
                .iter()
                .skip(1)
                .copied()
                .chain([point])
                .map(|p| p - *center)
                .collect::<Vec<_>>();
            for i in 0..vectors.len() {
                for j in i + 1..vectors.len() {
                    if let (Some(a), Some(b)) = (
                        vectors[i].try_normalized(1e-12),
                        vectors[j].try_normalized(1e-12),
                    ) {
                        measurement.push_str(&format!(
                            " · ∠v{}/v{} {:.1}°",
                            i + 1,
                            j + 1,
                            a.dot(b).clamp(-1., 1.).acos().to_degrees()
                        ));
                    }
                }
            }
        }
        if let Some(ToolPreview::Array {
            copies,
            displacement,
            ..
        }) = preview
        {
            return Some(ToolFeedback {
                screen_position,
                text: format!(
                    "Array preview: {copies} copies | {}",
                    vector_feedback_text("Copy", *displacement, units)
                ),
            });
        }
        if let Some(ToolPreview::ArrayOffsets {
            offsets, capped, ..
        }) = preview
        {
            return Some(ToolFeedback {
                screen_position,
                text: format!(
                    "Array preview: {} copies{} | {}",
                    offsets.len(),
                    if *capped { " (preview capped)" } else { "" },
                    measurement
                ),
            });
        }
        if let Some(ToolPreview::ArrayTransforms {
            transforms, capped, ..
        }) = preview
        {
            return Some(ToolFeedback {
                screen_position,
                text: format!(
                    "Copy array preview: {} copies{} | {}",
                    transforms.len(),
                    if *capped { " (preview capped)" } else { "" },
                    measurement
                ),
            });
        }
        if self.active_tool == "Paint" {
            if !matches!(preview, Some(ToolPreview::Paint { .. })) {
                return None;
            }
            let color = self.current_face_color();
            return Some(ToolFeedback {
                screen_position,
                text: format!(
                    "Paint preview: #{:02x}{:02x}{:02x}{:02x} | {}",
                    color.red, color.green, color.blue, color.alpha, measurement
                ),
            });
        }

        if self.transform_tool_state.active() == Some(TransformTool::Squash)
            && let [center, reference] = self.transform_tool_state.points()
            && let Some(point) = point
        {
            let v = *reference - *center;
            if v.length_squared() > 1e-12 {
                let factor = self
                    .transform_tool_state
                    .numeric_input()
                    .parse::<f64>()
                    .ok()
                    .unwrap_or((point - *center).dot(v) / v.length_squared());
                return Some(ToolFeedback {
                    screen_position,
                    text: format!("{measurement} · Axis factor {factor:.3}"),
                });
            }
        }
        if let Some(rotation) = point.and_then(|p| self.transform_tool_state.rotation_feedback(p)) {
            let mut angles = rotation
                .pair_angles
                .iter()
                .map(|(a, b, angle)| format!("∠v{a}/v{b} {angle:.1}°"))
                .collect::<Vec<_>>();
            if let Some(angle) = rotation.angle_degrees {
                angles.insert(0, format!("Rotation {angle:.1}°"));
            }
            return Some(ToolFeedback {
                screen_position,
                text: format!("{measurement} · {}", angles.join(" · ")),
            });
        }
        let point = point?;

        if let Some(kind) = self.primitive_tool_state.active()
            && !self.primitive_tool_state.picks().is_empty()
        {
            return Some(ToolFeedback {
                screen_position,
                text: primitive_feedback_text(
                    kind,
                    self.primitive_tool_state.picks(),
                    point,
                    units,
                ),
            });
        }

        let label = self.active_tool.as_str();
        let reference = self.tool_reference_point();

        let displacement = match self.active_tool.as_str() {
            "Line" | "Construction Line" => self
                .drawing_tool
                .preview(Some(point))
                .segment
                .map(|[start, end]| end - start),
            "Push/Pull" => self.push_pull_displacement(),
            _ => reference.map(|reference| point - reference),
        };
        let text = displacement.map_or_else(
            || point_feedback_text(label, point, units),
            |delta| vector_feedback_text(label, delta, units),
        );
        Some(ToolFeedback {
            screen_position,
            text,
        })
    }

    fn feedback_preview(&self, point: Vec3) -> Option<ToolPreview> {
        Some(match self.tool_reference_point() {
            Some(start) if start.distance(point) > 1.0e-12 => {
                ToolPreview::Line { start, end: point }
            }
            _ => ToolPreview::Point { position: point },
        })
    }

    fn tool_reference_point(&self) -> Option<Vec3> {
        if self.active_tool == "Planar Section" {
            return self.section.origin;
        }
        if architecture_workflow::active(&self.active_tool) {
            return self
                .architecture
                .points
                .last()
                .copied()
                .or_else(|| self.polyline_tool_state.points().last().copied());
        }
        if let Some(placement) = &self.clipboard.placement {
            return Some(placement.anchor);
        }
        if matches!(
            self.active_tool.as_str(),
            "Linear Dimension" | "Text" | "Leader"
        ) {
            return self.annotations.points.last().copied();
        }
        if self.hotspot_reference_pick {
            let (selected, hotspot) = self.native_hotspot_inspector();
            return selected.and_then(|(path, _)| {
                self.document
                    .resolve_occurrence(&path)
                    .map(|r| r.world_transform.transform_point(hotspot.position))
            });
        }
        if self.tool_cycle_state.hotspot_placement_active() {
            return self
                .hotspot_owner()
                .ok()
                .map(|(_, _, world)| world.transform_point(Vec3::ZERO));
        }
        if self.array_tool_state.active() {
            return self.array_tool_state.points().last().copied();
        }
        if self.rotational_array_tool_state.active() {
            return self.rotational_array_tool_state.points().first().copied();
        }
        if self.revolve_tool_state.active() {
            return self.revolve_tool_state.axis_points().first().copied();
        }
        if self.transform_tool_state.active() == Some(TransformTool::Squash) {
            return self.transform_tool_state.points().first().copied();
        }
        if matches!(
            self.transform_tool_state.active(),
            Some(TransformTool::Rotate | TransformTool::Rotate2 | TransformTool::RotateStretch)
        ) {
            return self.transform_tool_state.points().first().copied();
        }
        match self.active_tool.as_str() {
            "Line" | "Construction Line" => self.drawing_tool.anchor(),
            "Rectangle" | "Surface Rect" | "Quad" | "Circle" => {
                self.shape_tool_state.picks().last().copied()
            }
            "Polyline" | "Double Line" | "Extrude Swipe" | "Extrude Swipe 3D" | "Ribbon 3D"
            | "Freeform Slab" => self.polyline_tool_state.points().last().copied(),
            "Mesh" => self.mesh_tool_state.points().last().copied(),
            "Sphere" | "Cylinder" | "Cone" | "Pill" => {
                self.primitive_tool_state.picks().last().copied()
            }
            "Line Offset" => self.line_offset_tool_state.reference(),
            "Push/Pull" => self.push_pull_reference,
            "Move" | "Rotate" | "Rotate 2" | "Scale" | "Squash" | "Stretch" | "Stretch Scale"
            | "Rotate Stretch" => self.transform_tool_state.points().last().copied(),
            "Cuboid" => self.cuboid_first,
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
        let atlas_width = assets::ICON_ATLAS_WIDTH as f32;
        let atlas_height = assets::ICON_ATLAS_HEIGHT as f32;
        let uv = egui::Rect::from_min_max(
            egui::pos2(
                region.x as f32 / atlas_width,
                region.y as f32 / atlas_height,
            ),
            egui::pos2(
                (region.x + region.width) as f32 / atlas_width,
                (region.y + region.height) as f32 / atlas_height,
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
        let fallback_label = match tool {
            Some("Construction Line") => "CL".to_owned(),
            Some(tool) => tool.chars().next().unwrap_or('?').to_string(),
            None => command
                .descriptor
                .label
                .chars()
                .next()
                .unwrap_or('?')
                .to_string(),
        };
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
            .id(egui::Id::new("command-palette"))
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

    fn show_recent_projects_menu(&mut self, ui: &mut egui::Ui) {
        let recent_projects = self.preferences.recent_projects.clone();
        let open_state = self.query_command_state("file.open");
        ui.menu_button("Recent Files", |ui| {
            self.show_command_menu_item(ui, "file.open_recent");
            if !recent_projects.is_empty() {
                ui.separator();
            } else {
                ui.weak("No recently opened projects.");
            }
            for path in recent_projects {
                let response = ui.add_enabled(
                    open_state.enabled,
                    egui::Button::new(recent_project_display_name(&path)),
                );
                response.clone().on_hover_text(&path);
                if response.clicked() {
                    let mut payload = CommandPayload::default();
                    payload
                        .values
                        .insert("path".to_owned(), path.clone().into());
                    self.dispatch_command("file.open", &payload);
                }
            }
        });
    }

    fn show_recent_projects_dialog(&mut self, ctx: &egui::Context) {
        if !self.recent_projects_dialog_open {
            return;
        }
        let recent_projects = self.preferences.recent_projects.clone();
        let mut open = self.recent_projects_dialog_open;
        let mut selected_path = None;
        egui::Window::new("Open Recent Project")
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if recent_projects.is_empty() {
                    ui.weak("No recently opened projects.");
                }
                for path in &recent_projects {
                    let response = ui.button(recent_project_display_name(path));
                    response.clone().on_hover_text(path);
                    if response.clicked() {
                        selected_path = Some(path.clone());
                    }
                }
            });
        self.recent_projects_dialog_open = open;
        if let Some(path) = selected_path {
            let mut payload = CommandPayload::default();
            payload.values.insert("path".to_owned(), path.into());
            self.dispatch_command("file.open", &payload);
            self.recent_projects_dialog_open = false;
        }
    }

    fn show_command_menu_branch(&mut self, ui: &mut egui::Ui, path: &[String]) {
        let commands = shell_commands()
            .entries()
            .filter(|command| command_menu_paths(&command.descriptor).any(|item| item == path))
            .map(|command| command.descriptor.id.as_str().to_owned())
            .collect::<Vec<_>>();
        let mut children = Vec::new();
        for command in shell_commands().entries() {
            for menu_path in command_menu_paths(&command.descriptor) {
                if menu_path.len() > path.len()
                    && menu_path.starts_with(path)
                    && !children.contains(&menu_path[path.len()])
                {
                    children.push(menu_path[path.len()].clone());
                }
            }
        }
        children.sort_by_key(|title| {
            crate::functional_ui::SECTIONS
                .iter()
                .position(|s| s.root == path[0] && s.title == title)
                .unwrap_or(usize::MAX)
        });

        for id in &commands {
            if id == "file.open_recent" {
                self.show_recent_projects_menu(ui);
            } else {
                self.show_command_menu_item(ui, id);
            }
        }
        if !commands.is_empty() && !children.is_empty() {
            ui.separator();
        }
        for child in children {
            let mut child_path = path.to_vec();
            child_path.push(child.clone());
            ui.menu_button(child, |ui| self.show_command_menu_branch(ui, &child_path));
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        let request_files = std::mem::take(&mut self.open_file_menu);
        egui::MenuBar::new().ui(ui, |ui| {
            for root in crate::functional_ui::GROUPS {
                let response = ui.menu_button(*root, |ui| {
                    if *root == "Files" {
                        for section in crate::functional_ui::sections(root) {
                            ui.weak(section.title);
                            for (id, _) in section.commands {
                                if *id == "file.open_recent" {
                                    self.show_recent_projects_menu(ui);
                                } else {
                                    self.show_command_menu_item(ui, id);
                                }
                            }
                        }
                    } else {
                        self.show_command_menu_branch(ui, &[(*root).to_owned()]);
                    }
                });
                if request_files && *root == "Files" {
                    let id = egui::Popup::default_response_id(&response.response);
                    ui.memory_mut(|memory| memory.request_focus(response.response.id));
                    egui::Popup::open_id(ui.ctx(), id);
                    ui.ctx().request_repaint();
                }
            }
        });
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let tools = shell_commands()
            .entries()
            .filter(|command| {
                matches!(
                    command.descriptor.id.as_str(),
                    "tool.select" | "tool.line" | "tool.rectangle" | "tool.cuboid" | "view.orbit"
                )
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

    fn show_floating_toolbars(&mut self, ctx: &egui::Context, viewport_rect: egui::Rect) {
        let size = self.preferences.toolbar_icon_size as f32;
        let mut vertical_offset = 8.0;
        for (toolbar_id, label) in LEGACY_TOOLBARS {
            let Some(stem) = toolbar_id.strip_prefix("builtin_toolbar_") else {
                continue;
            };
            if !self
                .preferences
                .toolbar_visibility
                .get(stem)
                .copied()
                .unwrap_or(true)
            {
                continue;
            }

            let command_ids = legacy_toolbar_command_ids(stem);
            let mut open = true;
            let mut selected_command = None;
            let position = egui::pos2(
                viewport_rect.left() + 8.0,
                viewport_rect.top() + vertical_offset,
            );
            vertical_offset += size + 26.0;
            egui::Window::new(*label)
                .id(egui::Id::new(("legacy-floating-toolbar", stem)))
                .default_pos(position)
                .resizable(false)
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.set_max_width(620.0);
                    if stem == "in_tool_operators" {
                        let mode = if self.tool_cycle_state.hotspot_placement_active() {
                            "Hotspot placement host action"
                        } else {
                            self.active_tool.as_str()
                        };
                        ui.weak(format!("Active tool: {mode}"));
                    }
                    ui.horizontal_wrapped(|ui| {
                        for id in &command_ids {
                            if let Some(command) = shell_commands().get(id) {
                                let clicked = if stem == "rendering" {
                                    let state =
                                        self.query_command_state(command.descriptor.id.as_str());
                                    let response = ui
                                        .add_enabled_ui(state.enabled, |ui| {
                                            ui.add_sized(
                                                [size * 1.7, size],
                                                egui::Button::new(&command.descriptor.label),
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
                                    response.on_hover_text(&command.descriptor.label).clicked()
                                } else {
                                    self.show_tool_button(ui, command, size, true)
                                };
                                if clicked {
                                    selected_command = Some(id.clone());
                                }
                            }
                        }
                    });
                });
            if !open {
                self.run_command(&format!("view.toolbar.{stem}.hide"));
            }
            if let Some(id) = selected_command {
                self.run_command(&id);
            }
        }
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui, size: Vec2) -> egui::Response {
        self.sync_drawing_tool_state();
        if self.transform_tool_state.active() != TransformTool::from_label(&self.active_tool) {
            self.transform_tool_state
                .activate(TransformTool::from_label(&self.active_tool));
        }
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
        let snap = pointer.and_then(|position| {
            self.snap_at_viewport_position(predicted_rect, position, ui.ctx().pixels_per_point())
        });
        if self.transform_tool_state.active().is_some() {
            let point = pointer.and_then(|position| {
                snap.map(|snap| snap.point).or_else(|| {
                    self.viewport
                        .screen_to_construction_plane(predicted_rect, position)
                })
            });
            self.transform_tool_state.set_pointer(point);
        }
        if self.object_place_tool_state.active_definition().is_some() {
            let hit = pointer
                .and_then(|position| {
                    snap.map(|snap| snap.point).or_else(|| {
                        self.viewport
                            .screen_to_construction_plane(predicted_rect, position)
                    })
                })
                .map(|position| PlacementHit {
                    position,
                    surface_normal: snap
                        .filter(|result| result.kind == SnapKind::Face)
                        .map(|result| result.normal),
                });
            self.object_place_tool_state.update_pointer(hit);
        }
        if matches!(self.active_tool.as_str(), "Line" | "Construction Line")
            && let Some(point) = pointer.and_then(|position| {
                snap.map(|snap| snap.point).or_else(|| {
                    self.viewport
                        .screen_to_construction_plane(predicted_rect, position)
                })
            })
        {
            self.drawing_tool.handle(ToolEvent::PointerMoved(point));
        }
        if self.mesh_tool_state.active() {
            let point = pointer.and_then(|position| {
                snap.map(|snap| snap.point).or_else(|| {
                    self.viewport
                        .screen_to_construction_plane(predicted_rect, position)
                })
            });
            self.mesh_tool_state.set_pointer(point);
        }
        if self.line_offset_tool_state.active() {
            let point = pointer.and_then(|position| {
                snap.map(|snap| snap.point).or_else(|| {
                    self.viewport
                        .screen_to_construction_plane(predicted_rect, position)
                })
            });
            self.line_offset_tool_state.set_pointer(point);
        }
        if self.active_tool == "Push/Pull" && self.push_pull_reference.is_some() {
            self.push_pull_pointer = pointer.and_then(|position| {
                snap.map(|snap| snap.point).or_else(|| {
                    self.viewport
                        .screen_to_construction_plane(predicted_rect, position)
                })
            });
        }
        let visible_guides = if self.preferences.show_construction_guides {
            self.guides.clone()
        } else {
            Vec::new()
        };
        let preview = self.tool_preview(predicted_rect, pointer, snap);
        let feedback = self.tool_feedback(predicted_rect, pointer, snap, preview.as_ref());
        let guide_preview = if matches!(self.active_tool.as_str(), "Axis Guide" | "Grid Guide") {
            snap.and_then(|snap| {
                let kind = if self.active_tool == "Axis Guide" {
                    ConstructionGuideKind::Axis
                } else {
                    ConstructionGuideKind::Grid
                };
                self.cursor_aligned_guide(kind, snap)
            })
        } else {
            None
        };
        let selection = self
            .selection
            .selected()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        self.viewport
            .set_black_model_lines(self.preferences.black_model_lines);
        let input = self.viewport.show(
            ui,
            &ViewportScene {
                size,
                document: &self.document,
                selection: &selection,
                scope,
                display_policy: &self.selection_display_policy,
                active_tool: &self.active_tool,
                preview,
                feedback,
                snap,
                guides: &visible_guides,
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
        let response = input.response.clone();
        self.viewport_rect = input.rect;
        self.pointer_in_viewport = input.pointer_local;
        if pointer.is_some() {
            self.cursor_snap = snap;
        }
        if let Some(click) = input.click {
            self.handle_viewport_click_input(input.rect, click, snap);
        }
        if let Some(gesture) = input.selection_gesture {
            self.handle_selection_gesture(input.rect, gesture);
        }
        if self.drawing_tool.anchor().is_some()
            && self.drawing_tool.distance_focused()
            && matches!(self.active_tool.as_str(), "Line" | "Construction Line")
        {
            let position = input.rect.left_top() + egui::vec2(18.0, 54.0);
            let text = if self.drawing_tool.numeric_input().is_empty() {
                "Distance: type a value".to_owned()
            } else {
                format!("Distance: {}", self.drawing_tool.numeric_input())
            };
            egui::Area::new(egui::Id::new("drawing-distance-input"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(text);
                    });
                });
        }
        if self.revolve_tool_state.active() && self.revolve_tool_state.axis_points().len() == 2 {
            let position = input.rect.left_top() + egui::vec2(18.0, 54.0);
            let text = if self.revolve_tool_state.numeric_input().is_empty() {
                "Revolve angle: 360° (default); type a signed angle".to_owned()
            } else {
                format!(
                    "Revolve angle: {}°",
                    self.revolve_tool_state.numeric_input()
                )
            };
            egui::Area::new(egui::Id::new("revolve-angle-input"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(text);
                    });
                });
        }
        if self.transform_tool_state.active().is_some()
            && !self.transform_tool_state.numeric_input().is_empty()
        {
            let position = input.rect.left_top() + egui::vec2(18.0, 54.0);
            let text = format!(
                "Transform value: {}",
                self.transform_tool_state.numeric_input()
            );
            egui::Area::new(egui::Id::new("transform-numeric-input"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(text);
                    });
                });
        }
        if self.active_tool == "Push/Pull"
            && let Some(displacement) = self.push_pull_displacement()
        {
            let position = input.rect.left_top() + egui::vec2(18.0, 82.0);
            let feedback = push_pull_feedback_text(displacement, self.document.model_settings());
            let numeric_hint = self.push_pull_distance_focused.then(|| {
                if self.push_pull_numeric_input.is_empty() {
                    "Signed distance: type a value".to_owned()
                } else {
                    format!("Signed distance: {}", self.push_pull_numeric_input)
                }
            });
            egui::Area::new(egui::Id::new("push-pull-feedback"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(feedback);
                        if let Some(hint) = numeric_hint {
                            ui.label(hint);
                        }
                    });
                });
        }
        response
    }

    fn handle_selection_gesture(&mut self, rect: egui::Rect, gesture: SelectionGesture) {
        if !gesture.pixels_per_point.is_finite() || gesture.pixels_per_point <= 0.0 {
            return;
        }
        let region = egui::Rect::from_two_pos(gesture.start, gesture.end);
        let crossing = gesture.end.x < gesture.start.x && !gesture.modifiers.alt;
        let scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        let targets = self.viewport.select_screen_region(
            rect,
            &self.document,
            region,
            scope,
            &self.selection_display_policy,
            crossing,
        );
        let mode = if gesture.modifiers.ctrl || gesture.modifiers.mac_cmd {
            SelectionMode::Remove
        } else if gesture.modifiers.shift {
            SelectionMode::Add
        } else {
            SelectionMode::Replace
        };
        self.selection.apply(targets, mode);
        let summary = self.selection.summary();
        self.status = format!(
            "{} selection: {} object(s), {} face(s), {} edge(s), {} vertex/vertices.",
            if crossing { "Crossing" } else { "Window" },
            summary.objects,
            summary.faces,
            summary.edges,
            summary.vertices,
        );
    }

    fn snap_at_viewport_position(
        &mut self,
        rect: egui::Rect,
        position: egui::Pos2,
        pixels_per_point: f32,
    ) -> Option<SnapResult> {
        let point_input_active = self.tool_cycle_state.hotspot_placement_active()
            || !matches!(
                self.active_tool.as_str(),
                "Select" | "Paint" | "Orbit" | "Pan" | "Zoom" | "Remove Guide"
            );
        if !point_input_active || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            return None;
        }
        let scope = match &self.selection.context {
            EditContext::Document => SnapScope::Document,
            EditContext::Definition(definition) => SnapScope::Definition(*definition),
            EditContext::Occurrence(occurrence) => SnapScope::Occurrence(occurrence),
        };
        let visible_guides = if self.preferences.show_construction_guides {
            self.guides.as_slice()
        } else {
            &[]
        };
        self.viewport.snap_point_with_options(
            rect,
            &self.document,
            position,
            SnapOptions {
                snap_radius: (self.document.model_settings().snap_radius
                    / f64::from(pixels_per_point)) as f32,
                grid_spacing: self.document.model_settings().grid_size,
                guides: visible_guides,
                scope,
                visibility: SnapVisibility {
                    objects_visible: self
                        .selection_display_policy
                        .can_draw(SelectionKind::Object),
                    edges_visible: self.selection_display_policy.can_draw(SelectionKind::Edge),
                    faces_visible: self.selection_display_policy.can_draw(SelectionKind::Face),
                    faces_wireframe: self
                        .selection_display_policy
                        .state(SelectionKind::Face)
                        .wireframe,
                },
            },
        )
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
        let mut totals = BTreeMap::new();
        for occurrence in self.document.scene_geometry_refs().unwrap_or_default() {
            if let Some(geometry) = self
                .document
                .geometry_in_context(&occurrence, &self.selection.context)
            {
                *totals.entry(SelectionKind::Vertex).or_insert(0) += geometry.vertices.len();
                *totals.entry(SelectionKind::Edge).or_insert(0) += geometry.segments.len();
                *totals.entry(SelectionKind::Face).or_insert(0) += geometry.triangles.len();
            }
        }
        *totals.entry(SelectionKind::Object).or_insert(0) = root_objects.len();
        let summary = self.selection.summary();
        let mesh = self.selection.mesh_counts(&self.document);
        ui.horizontal(|ui| {
            ui.weak("Type");
            ui.add_space(36.0);
            ui.weak("T");
            ui.weak("S");
            ui.weak("D");
            ui.weak("M");
            ui.weak("W");
        });
        for (kind, label) in [
            (SelectionKind::Vertex, "Vertices"),
            (SelectionKind::Edge, "Edges"),
            (SelectionKind::Face, "Faces"),
            (SelectionKind::Voxel, "Voxels"),
            (SelectionKind::Hotspot, "Hotspots"),
            (SelectionKind::Object, "Objects"),
            (SelectionKind::Dimension, "Dimensions"),
            (SelectionKind::Text, "Texts"),
            (SelectionKind::Wall, "Walls"),
            (SelectionKind::Slab, "Slabs"),
            (SelectionKind::Stair, "Stairs"),
            (SelectionKind::Frame, "Frames"),
            (SelectionKind::Hvac, "HVAC"),
        ] {
            let mut state = self.selection_display_policy.state(kind);
            let mut visible = state.visible;
            let mut unlocked = state.unlocked;
            let mut wireframe = state.wireframe;
            let total = totals.get(&kind).copied().unwrap_or(0);
            let selected = match kind {
                SelectionKind::Edge => mesh.edges,
                SelectionKind::Face => mesh.faces,
                _ => summary.count(kind),
            };
            ui.horizontal(|ui| {
                ui.add_sized([85.0, 18.0], egui::Label::new(label));
                ui.add_sized([30.0, 18.0], egui::Label::new(total.to_string()));
                ui.add_sized([30.0, 18.0], egui::Label::new(selected.to_string()));
                if ui.checkbox(&mut visible, "").changed() {
                    self.selection_display_policy.set_visible(kind, visible);
                    state = self.selection_display_policy.state(kind);
                    unlocked = state.unlocked;
                }
                if ui
                    .add_enabled(visible, egui::Checkbox::new(&mut unlocked, ""))
                    .changed()
                {
                    self.selection_display_policy.set_unlocked(kind, unlocked);
                }
                let wireframe_supported = matches!(
                    kind,
                    SelectionKind::Face
                        | SelectionKind::Voxel
                        | SelectionKind::Wall
                        | SelectionKind::Slab
                        | SelectionKind::Stair
                        | SelectionKind::Frame
                        | SelectionKind::Hvac
                );
                if wireframe_supported {
                    if ui.checkbox(&mut wireframe, "").changed() {
                        self.selection_display_policy.set_wireframe(kind, wireframe);
                    }
                } else {
                    ui.add_sized([18.0, 18.0], egui::Label::new("—"));
                }
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
                    let response = ui.selectable_label(selected, label);
                    if response.double_clicked() {
                        self.set_selection([occurrence.clone()]);
                        self.run_command("selection.edit_object");
                    } else if response.clicked() {
                        self.set_selection([occurrence.clone()]);
                    }
                }
            });
    }

    fn show_polyline_settings(&mut self, ui: &mut egui::Ui) {
        let mut settings = self.document.model_settings().clone();
        let mut changed = false;
        for (label, value, range) in [
            (
                "Arc max length",
                &mut settings.arc_max_length,
                0.0001..=1_000_000.0,
            ),
            (
                "Double line size",
                &mut settings.double_line_size,
                0.0001..=1_000_000.0,
            ),
            (
                "Double line offset",
                &mut settings.double_line_offset,
                -1_000_000.0..=1_000_000.0,
            ),
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
        if changed {
            let mut payload = CommandPayload::default();
            payload.values.insert(
                "arc_max_length".to_owned(),
                serde_json::json!(settings.arc_max_length),
            );
            payload.values.insert(
                "double_line_size".to_owned(),
                serde_json::json!(settings.double_line_size),
            );
            payload.values.insert(
                "double_line_offset".to_owned(),
                serde_json::json!(settings.double_line_offset),
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
                        .unwrap_or("Polyline settings could not be updated."),
                );
            }
        }
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
        ui.checkbox(&mut preferences.black_model_lines, "Black model lines");
        ui.weak("Turn off to display lines in their material colors.");
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
            "black_model_lines".into(),
            json!(preferences.black_model_lines),
        );
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
                            let array_state = self.query_command_state("object.array_selection");
                            let preview_label = if self.array_preview {
                                "Hide Array Preview"
                            } else {
                                "Preview Array"
                            };
                            if ui
                                .add_enabled(array_state.enabled, egui::Button::new(preview_label))
                                .clicked()
                            {
                                self.array_preview = !self.array_preview;
                            }
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
                            ui.label("Ctrl+N focuses distance input after a Line start point.");
                            ui.label("Shift+L cycles planar tools, D annotations, O operations, and M modifications. A/C/L selects arc/circular/line segments while entering paths.");
                            ui.label("G places a grid guide; T places an axis guide.");
                        });
                    }
                    if self.show_panel_header(ui, "polyline", "Polyline Settings") {
                        Self::show_panel_body(ui, |ui| self.show_polyline_settings(ui));
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

    /// Render the browser/headless workspace without requiring a native eframe host.
    pub fn show_workspace(&mut self, ui: &mut egui::Ui) {
        self.show_workspace_inner(ui);
    }

    fn show_workspace_inner(&mut self, ui: &mut egui::Ui) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.poll_project_load();
            self.poll_project_save();
            self.poll_object_import();
            self.poll_object_bake();
            self.poll_interchange();
            let autosave_wait = self.poll_autosave(Instant::now());
            if self.pending_project_load.is_some()
                || self.pending_project_save.is_some()
                || self.pending_interchange.is_some()
                || self.pending_object_import.is_some()
            {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            } else if self.pending_replacement.is_none()
                && let Some(wait) = autosave_wait
            {
                ui.ctx()
                    .request_repaint_after(wait.max(Duration::from_millis(1)));
            }
        }
        ui.ctx()
            .set_zoom_factor(self.preferences.ui_text_scale.clamp(0.5, 2.0));
        let popup_open = egui::Popup::is_any_open(ui.ctx());
        let shortcuts_allowed = !self.command_palette_open
            && self.pending_replacement.is_none()
            && self.native_prototype_deletion_prompt().is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && !popup_open;
        let mut tool_keyboard_handled = false;
        if !self.numeric_entry.text.is_empty()
            && ui.input(|i| {
                [
                    egui::Key::Delete,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                    egui::Key::Home,
                    egui::Key::End,
                ]
                .iter()
                .any(|k| i.key_pressed(*k))
            })
        {
            tool_keyboard_handled = true;
        }
        if shortcuts_allowed
            && !ui.input(|i| i.modifiers.ctrl || i.modifiers.alt || i.modifiers.mac_cmd)
        {
            let typed = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<String>()
            });
            tool_keyboard_handled |= self.append_tool_numeric_text(&typed);
            if self.tool_numeric_active() && ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
                self.run_command("tool.undo_point");
                tool_keyboard_handled = true;
            }
            if self.tool_numeric_active() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_command("tool.finish");
                tool_keyboard_handled = true;
            }
            if !tool_keyboard_handled && !ui.input(|i| i.modifiers.shift) {
                for (key, text) in [
                    (egui::Key::A, "a"),
                    (egui::Key::C, "c"),
                    (egui::Key::L, "l"),
                ] {
                    if ui.input(|i| i.key_pressed(key)) && self.set_path_segment_key(text) {
                        tool_keyboard_handled = true;
                    }
                }
            }
        }
        let shortcuts_allowed = shortcuts_allowed && !tool_keyboard_handled;
        let distance_input_shortcut = shortcuts_allowed
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::N));
        if distance_input_shortcut {
            self.run_command("tool.distance");
        }
        let open_project_shortcut = shortcuts_allowed
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::O));
        if open_project_shortcut {
            self.run_command("file.open");
        }
        let save_shortcut = shortcuts_allowed
            && ui.input(|input| {
                input.modifiers.ctrl && input.key_pressed(egui::Key::S) && !input.modifiers.shift
            });
        if save_shortcut {
            self.run_command("file.save");
        }
        let save_as_shortcut = shortcuts_allowed
            && ui.input(|input| {
                input.modifiers.ctrl && input.modifiers.shift && input.key_pressed(egui::Key::S)
            });
        if save_as_shortcut {
            self.run_command("file.save_as");
        }
        #[cfg(not(target_arch = "wasm32"))]
        if shortcuts_allowed
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Q))
        {
            self.run_command("app.quit");
        }
        let delete_shortcut =
            shortcuts_allowed && ui.input(|input| input.key_pressed(egui::Key::Delete));
        if delete_shortcut {
            self.run_command("edit.delete_selection");
        }
        let toggle_command_palette = !popup_open
            && self.pending_replacement.is_none()
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
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && (!self.polyline_tool_state.active() || input.modifiers.shift)
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::L)
            })
        {
            self.run_command("tool.cycle.planar_construction");
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::D)
            })
        {
            self.run_command("tool.cycle.annotation");
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::O)
            })
        {
            self.run_command("tool.cycle.operations");
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::M)
            })
        {
            self.run_command("tool.cycle.modification");
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::G)
            })
        {
            if let Some(position) = ui
                .ctx()
                .pointer_interact_pos()
                .filter(|position| self.viewport_rect.contains(*position))
            {
                self.cursor_snap = self.snap_at_viewport_position(
                    self.viewport_rect,
                    position,
                    ui.ctx().pixels_per_point(),
                );
            }
            self.run_command("view.grid_guide");
        } else if shortcuts_allowed
            && ui.input(|input| {
                !input.modifiers.ctrl
                    && !input.modifiers.shift
                    && !input.modifiers.alt
                    && !input.modifiers.mac_cmd
                    && input.key_pressed(egui::Key::T)
            })
        {
            if let Some(position) = ui
                .ctx()
                .pointer_interact_pos()
                .filter(|position| self.viewport_rect.contains(*position))
            {
                self.cursor_snap = self.snap_at_viewport_position(
                    self.viewport_rect,
                    position,
                    ui.ctx().pixels_per_point(),
                );
            }
            self.run_command("view.axis_guide");
        } else if shortcuts_allowed && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.handle_escape();
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
        let show_inspector = self.preferences.panels_visible && content_width >= 650.0;
        let side_width = if show_inspector { 420.0 } else { 0.0 };
        let viewport_width = (content_width - side_width - 16.0).max(120.0);

        let mut viewport_response = None;
        ui.horizontal(|ui| {
            viewport_response =
                Some(self.show_viewport(ui, Vec2::new(viewport_width, content_height)));
            if show_inspector {
                ui.allocate_ui_with_layout(
                    Vec2::new(420.0, content_height),
                    Layout::top_down(Align::Min),
                    |ui| self.show_inspector(ui, content_height),
                );
            }
        });
        if toolbar_visible {
            self.show_floating_toolbars(ui.ctx(), self.viewport_rect);
        }
        if let Some(response) = viewport_response {
            let _ = response.context_menu(|ui| {
                for group in crate::functional_ui::GROUPS {
                    ui.menu_button(*group, |ui| {
                        self.show_command_menu_branch(ui, &[(*group).to_owned()])
                    });
                }
            });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.weak(format!("Tool: {}", self.active_tool));
            if self
                .transform_tool_state
                .active()
                .is_some_and(TransformTool::supports_copy_mode)
            {
                let label = if self.transform_tool_state.copy_mode() {
                    "Copy: On"
                } else {
                    "Copy: Off"
                };
                if ui
                    .selectable_label(self.transform_tool_state.copy_mode(), label)
                    .clicked()
                {
                    self.run_command("tool.copy_mode");
                }
            }
            if self.active_tool == "Paint" {
                let mut color = Color32::from_rgba_unmultiplied(
                    self.current_paint_color.red,
                    self.current_paint_color.green,
                    self.current_paint_color.blue,
                    self.current_paint_color.alpha,
                );
                if ui.color_edit_button_srgba(&mut color).changed() {
                    let [red, green, blue, alpha] = color.to_srgba_unmultiplied();
                    self.current_paint_color = Rgba8::new(red, green, blue, alpha);
                }
                let enabled = self.query_command_state("paint.apply_selection").enabled;
                if ui
                    .add_enabled(enabled, egui::Button::new("Paint Selection"))
                    .clicked()
                {
                    self.run_command("paint.apply_selection");
                }
            }
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
        self.show_recent_projects_dialog(ui.ctx());
        self.show_document_replacement_dialog(ui.ctx());
        self.show_prototype_deletion_dialog(ui.ctx());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_project_path(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("octd");
    }
    path
}

pub(crate) fn recent_project_display_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|component| !component.is_empty())
        .unwrap_or(path)
}

fn cuboid_preview_from_corners(first: Vec3, opposite: Vec3) -> Option<CuboidPreview> {
    if !first.is_finite() || !opposite.is_finite() {
        return None;
    }
    let origin = Vec3::new(
        first.x.min(opposite.x),
        first.y.min(opposite.y),
        first.z.min(opposite.z),
    );
    let dimensions = Vec3::new(
        (opposite.x - first.x).abs(),
        (opposite.y - first.y).abs(),
        (opposite.z - first.z).abs(),
    );
    (dimensions.is_finite() && dimensions.x >= 0.05 && dimensions.y >= 0.05 && dimensions.z >= 0.05)
        .then_some(CuboidPreview { origin, dimensions })
}

fn cuboid_mesh(preview: CuboidPreview) -> PlanarMesh {
    let origin = preview.origin;
    let dimensions = preview.dimensions;
    PlanarMesh {
        vertices: vec![
            origin,
            origin + Vec3::new(dimensions.x, 0.0, 0.0),
            origin + Vec3::new(dimensions.x, dimensions.y, 0.0),
            origin + Vec3::new(0.0, dimensions.y, 0.0),
            origin + Vec3::new(0.0, 0.0, dimensions.z),
            origin + Vec3::new(dimensions.x, 0.0, dimensions.z),
            origin + dimensions,
            origin + Vec3::new(0.0, dimensions.y, dimensions.z),
        ],
        segments: vec![
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 0],
            [4, 5],
            [5, 6],
            [6, 7],
            [7, 4],
            [0, 4],
            [1, 5],
            [2, 6],
            [3, 7],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
        ],
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_project_file(
    path: &Path,
    document: &Document,
    history: Option<&OpaqueLegacyHistory>,
) -> Result<(), String> {
    write_project_file_with_replace(path, document, history, |from, to| fs::rename(from, to))
}

#[cfg(not(target_arch = "wasm32"))]
fn native_preferences_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from)?;

    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(PathBuf::from)?
        .join("Library")
        .join("Application Support");

    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    #[cfg(not(any(unix, target_os = "windows")))]
    return None;

    Some(root.join("meshcraft").join("preferences.json"))
}

#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn load_native_preferences() -> Preferences {
    native_preferences_path()
        .map(|path| load_preferences_file(&path))
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_preferences_file(path: &Path) -> Preferences {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_native_preferences(preferences: &Preferences) -> Result<(), String> {
    let path = native_preferences_path()
        .ok_or_else(|| "no native configuration directory is available".to_owned())?;
    write_native_preferences_file(&path, preferences)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_native_preferences_file(path: &Path, preferences: &Preferences) -> Result<(), String> {
    write_native_preferences_file_with_replace(path, preferences, replace_native_file)
}

#[cfg(not(target_arch = "wasm32"))]
fn replace_native_file(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_native_preferences_file_with_replace(
    path: &Path,
    preferences: &Preferences,
    replace_file: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create preferences directory: {error}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "preferences path has no valid file name".to_owned())?;
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
                    "could not create a temporary preferences file: {error}"
                ));
            }
        }
    }
    let Some(temporary_path) = temporary_path else {
        return Err("could not reserve a temporary preferences file".to_owned());
    };
    let mut temporary_file = temporary_file.expect("temporary path and file are created together");
    let write_result = temporary_file
        .write_all(&bytes)
        .and_then(|()| temporary_file.sync_all());
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "could not write temporary preferences file: {error}"
        ));
    }
    if let Err(error) = replace_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("could not replace {}: {error}", path.display()));
    }
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_project_file_with_replace(
    path: &Path,
    document: &Document,
    history: Option<&OpaqueLegacyHistory>,
    replace_file: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
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
    if let Err(error) = replace_file(&temporary_path, path) {
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
            ShellAction::SelectTool(name) | ShellAction::SelectToolAlias(name) if *name == tool => {
                Some(command.descriptor.id.as_str())
            }
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

fn transform_status(report: &modeling_transforms::TransformReport, copied: bool) -> String {
    if copied {
        format!(
            "Copied {} vertex/vertices, {} edge(s), {} face(s), {} annotation(s), and {} object(s).",
            report.copied_vertices,
            report.copied_segments,
            report.copied_triangles,
            report.copied_domain_entities,
            report.copied_occurrences
        )
    } else {
        format!(
            "Transformed {} vertex/vertices, {} annotation(s), and {} object(s).",
            report.transformed_vertices,
            report.transformed_domain_entities,
            report.transformed_occurrences
        )
    }
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

#[cfg(target_arch = "wasm32")]
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
        CommandPayload, CutTool, DrawingToolState, EditContext, LEGACY_TOOLBAR_COMMAND_IDS,
        LineOffsetToolState, MeshToolState, MeshcraftApp, OrthoView, PolylineToolState,
        Preferences, PrimitiveKind, PrimitiveToolState, ReplacementDecision, RevolveToolState,
        SelectedEntity, SelectionClickTracker, SelectionDisplayPolicy, SelectionKind,
        SelectionMode, SelectionSession, SelectionTarget, ShapeToolState, ToolCycleState,
        ToolEvent, TransactionHistory, TransformPick, TransformTool, TransformToolState, UiProfile,
        command_matches, load_preferences_file, point_feedback_text, primitive_feedback_text,
        push_pull_feedback_text, registry_has_unique_ids, shell_commands, tool_command_id,
        vector_feedback_text,
    };
    use crate::viewport::{
        ConstructionGuide, ConstructionGuideKind, CuboidPreview, GridNormalAxis, SelectionGesture,
        SnapKind, SnapResult, SnapScope, ToolPreview, Viewport, ViewportClick,
    };
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use egui::{Event, ImeEvent, Key, Modifiers, Pos2, RawInput, Rect, Vec2};
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use meshcraft_core::commands::{
        CommandCapability, CommandCompletion, CommandKind, CommandValueType,
    };
    use meshcraft_core::document::{
        DefinitionKind, Document, InstanceEdits, ModelSettings, OccurrenceRef, Rgba8,
    };
    use meshcraft_core::geometry::{Affine3, Vec3};
    use meshcraft_core::transactions::{OpaqueLegacyHistory, OpaqueLegacyUndoEntry};
    use meshcraft_io::project::read_project;
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, BTreeSet};
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

    pub(super) fn test_app() -> MeshcraftApp {
        MeshcraftApp {
            preferences: Preferences::default(),
            #[cfg(not(target_arch = "wasm32"))]
            persist_native_preferences: false,
            icon_atlas: None,
            icon_regions: BTreeMap::new(),
            active_tool: "Select".to_owned(),
            status: String::new(),
            pointer_in_viewport: None,
            cursor_snap: None,
            viewport_rect: egui::Rect::NOTHING,
            open_file_menu: false,
            recent_projects_dialog_open: false,
            command_palette_open: false,
            command_search: String::new(),
            command_selection: 0,
            focus_command_search: false,
            quit_requested: false,
            panel_focus: None,
            document: Document::new(),
            document_generation: 0,
            annotations: Default::default(),
            architecture: Default::default(),
            section: Default::default(),
            clipboard: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            mcp_jobs: Default::default(),
            saved_revision: 0,
            #[cfg(not(target_arch = "wasm32"))]
            autosave_observed_revision: 0,
            #[cfg(not(target_arch = "wasm32"))]
            autosave_deadline: None,
            history: TransactionHistory::default(),
            legacy_history: None,
            legacy_active_index: None,
            #[cfg(not(target_arch = "wasm32"))]
            current_project_path: None,
            next_job_id: 1,
            job_results: BTreeMap::new(),
            pending_replacement: None,
            pending_prototype_deletion: None,
            #[cfg(not(target_arch = "wasm32"))]
            replacement_after_save: None,
            pending_project_load: None,
            pending_project_save: None,
            selection: SelectionSession::default(),
            copy_availability: Default::default(),
            transform_wireframe: None,
            selection_display_policy: SelectionDisplayPolicy::default(),
            selection_click_tracker: SelectionClickTracker::default(),
            #[cfg(not(target_arch = "wasm32"))]
            native_spatial_context: None,
            #[cfg(not(target_arch = "wasm32"))]
            native_edit_view: None,
            object_name_edit: None,
            object_browser_selection: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_object_import: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_object_bake: None,
            #[cfg(not(target_arch = "wasm32"))]
            pending_interchange: None,
            hotspot_inspector: None,
            hotspot_defaults: Default::default(),
            hotspot_reference_pick: false,
            viewport: Default::default(),
            guides: Vec::new(),
            cuboid_first: None,
            drawing_tool: DrawingToolState::default(),
            polyline_tool_state: PolylineToolState::default(),
            mesh_tool_state: MeshToolState::default(),
            line_offset_tool_state: LineOffsetToolState::default(),
            push_pull_reference: None,
            push_pull_pointer: None,
            push_pull_faces: Vec::new(),
            push_pull_owner: None,
            push_pull_occurrence: None,
            push_pull_distance_focused: false,
            push_pull_numeric_input: String::new(),
            numeric_entry: Default::default(),
            tool_cycle_state: ToolCycleState::default(),
            primitive_tool_state: PrimitiveToolState::default(),
            revolve_tool_state: RevolveToolState::default(),
            shape_tool_state: ShapeToolState::default(),
            transform_tool_state: TransformToolState::default(),
            object_place_tool_state: super::ObjectPlaceToolState::default(),
            placement_preview: Default::default(),
            array_tool_state: super::ArrayToolState::default(),
            rotational_array_tool_state: super::RotationalArrayToolState::default(),
            current_paint_color: Rgba8::ANSI_GREY,
            current_paint_material: None,
            array_count: 12,
            array_spacing: 3.0,
            array_preview: false,
            pending_selection_volume: None,
        }
    }

    fn pick_array_tool_point(app: &mut MeshcraftApp, point: Vec3) {
        app.handle_viewport_click_at_point_with_selection(
            Rect::NOTHING,
            Pos2::ZERO,
            Some(SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Grid,
                screen_distance: 0.0,
            }),
            Some(point),
            Modifiers::default(),
            1,
        );
    }

    fn click_visible_cube_face(
        app: &mut MeshcraftApp,
        occurrence: &OccurrenceRef,
    ) -> SelectionTarget {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let resolved = app.document.resolve_occurrence(occurrence).unwrap();
        let geometry = app.document.effective_geometry(occurrence).unwrap();
        let (position, world_point, target) = geometry
            .triangles
            .values()
            .find_map(|triangle| {
                let points = triangle.vertices.map(|reference| {
                    resolved
                        .world_transform
                        .transform_point(geometry.vertex_position(reference).unwrap())
                });
                let world_point = (points[0] + points[1] + points[2]) / 3.0;
                let position = app.viewport.world_to_screen(rect, world_point);
                let target = app.viewport.pick_selection_target(
                    rect,
                    &app.document,
                    position,
                    SnapScope::Document,
                    &app.selection_display_policy,
                )?;
                matches!(target.entity, SelectedEntity::Triangle(_)).then_some((
                    position,
                    world_point,
                    target,
                ))
            })
            .expect("a cuboid triangle should be visible to the viewport picker");
        app.handle_viewport_click_at_point(rect, position, None, Some(world_point));
        assert!(app.selection.selected().contains(&target));
        target
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

    #[cfg(not(target_arch = "wasm32"))]
    fn run_native_workspace_frame(
        app: &mut MeshcraftApp,
        context: &egui::Context,
    ) -> egui::FullOutput {
        let mut output = context.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 900.0))),
                ..Default::default()
            },
            |root_ui| {
                let context = root_ui.ctx().clone();
                let mut workspace_ui = egui::Ui::new(
                    context.clone(),
                    egui::Id::new("meshcraft-native-workspace"),
                    egui::UiBuilder::new()
                        .layer_id(egui::LayerId::background())
                        .max_rect(context.viewport_rect()),
                );
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(&mut workspace_ui, |ui| app.show_workspace(ui));
            },
        );
        output.textures_delta.clear();
        output
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual move/rotate latency probe; set MESHCRAFT_BENCH_PROJECT"]
    fn transform_latency_probe() {
        use meshcraft_core::{document::architecture, geometry::Aabb3};
        let path = std::env::var("MESHCRAFT_BENCH_PROJECT").unwrap();
        let mut app = test_app();
        app.document = read_project(&std::fs::read(path).unwrap())
            .unwrap()
            .document;
        let root = app
            .document
            .scene_container_occurrence()
            .unwrap_or_else(OccurrenceRef::scene_root);
        let g = app.document.effective_geometry(&root).unwrap();
        let bounds = Aabb3::from_points(g.vertices.values().map(|v| v.position)).unwrap();
        let cut = bounds.min.x + (bounds.max.x - bounds.min.x) * 0.34;
        let mut targets = BTreeSet::new();
        for e in g.segments.values() {
            if e.endpoints
                .iter()
                .all(|r| g.vertex_position(*r).unwrap().x <= cut)
                && let Some(entity) = architecture::semantic_entity(&e.extensions)
            {
                targets.insert(SelectionTarget {
                    occurrence: root.clone(),
                    entity,
                });
            }
        }
        for path in app.document.occurrence_refs().unwrap() {
            if path == root || path.is_scene_root() {
                continue;
            }
            let r = app.document.resolve_occurrence(&path).unwrap();
            let Some(g) = app.document.effective_geometry(&path) else {
                continue;
            };
            if !g.vertices.is_empty()
                && g.vertices
                    .values()
                    .all(|v| r.world_transform.transform_point(v.position).x <= cut)
            {
                targets.insert(SelectionTarget {
                    occurrence: path,
                    entity: SelectedEntity::Occurrence,
                });
            }
        }
        if let Ok(path) = std::env::var("MESHCRAFT_BENCH_TARGETS") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(
                    &targets
                        .iter()
                        .map(super::mcp_workflow::target_json)
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            )
            .unwrap();
        }
        app.selection.apply(targets, SelectionMode::Replace);
        eprintln!(
            "Selected {} {:?}",
            app.selection.summary().total(),
            app.selection.mesh_counts(&app.document)
        );
        for (name, transform) in [
            ("move", Affine3::from_translation(Vec3::new(3., 0., 0.))),
            (
                "rotate",
                Affine3::from_linear_translation(
                    [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
                    Vec3::ZERO,
                ),
            ),
        ] {
            let t = std::time::Instant::now();
            let edges = super::modeling_transforms::preview_selection_transform(
                &app.document,
                &app.selection,
                transform,
            )
            .unwrap();
            eprintln!("{name} preview {:?} edges {}", t.elapsed(), edges.len());
            let t = std::time::Instant::now();
            app.cached_transform_wireframe(transform).unwrap();
            eprintln!("{name} first cached preview {:?}", t.elapsed());
            let t = std::time::Instant::now();
            for _ in 0..100 {
                app.cached_transform_wireframe(transform).unwrap();
            }
            eprintln!("{name} 100 warm previews {:?}", t.elapsed());
            let mut copy = app.document.clone();
            let mut selection = app.selection.clone();
            let t = std::time::Instant::now();
            super::modeling_transforms::transform_selection(
                &mut copy,
                &mut selection,
                transform,
                false,
            )
            .unwrap();
            eprintln!("{name} core commit {:?}", t.elapsed());
            let t = std::time::Instant::now();
            app.commit_transform(meshcraft_core::tools::transforms::TransformCommit {
                transform,
                copy: false,
            })
            .unwrap();
            eprintln!("{name} commit {:?}", t.elapsed());
        }
    }
    #[test]
    fn rigid_transform_wireframe_is_reused_and_invalidated_by_its_source() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("part", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });
        let before = app.document.clone();
        let base = app.cached_transform_wireframe(Affine3::IDENTITY).unwrap();
        assert!(!base.is_empty());
        let storage = app.transform_wireframe.as_ref().unwrap().edges.as_ptr();
        for i in 1..100 {
            let delta = Vec3::X * f64::from(i);
            assert_eq!(
                app.cached_transform_wireframe(Affine3::from_translation(delta))
                    .unwrap(),
                base.iter()
                    .map(|e| e.map(|p| p + delta))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                storage,
                app.transform_wireframe.as_ref().unwrap().edges.as_ptr()
            );
        }
        assert_eq!(
            app.document, before,
            "phantom movement must not edit the model"
        );
        let key = app.transform_wireframe.as_ref().unwrap().key.clone();
        app.document_generation += 1;
        app.cached_transform_wireframe(Affine3::IDENTITY).unwrap();
        assert_ne!(app.transform_wireframe.as_ref().unwrap().key, key);
        app.selection.clear();
        assert!(
            app.cached_transform_wireframe(Affine3::IDENTITY)
                .unwrap()
                .is_empty()
        );
        let g = app.document.effective_geometry(&occurrence).unwrap();
        let vertex = *g.vertices.keys().next().unwrap();
        app.selection.select(SelectionTarget {
            occurrence,
            entity: SelectedEntity::Vertex(vertex),
        });
        let transform = Affine3::from_translation(Vec3::X * 3.);
        assert_eq!(
            app.cached_transform_wireframe(transform).unwrap(),
            super::modeling_transforms::preview_selection_transform(
                &app.document,
                &app.selection,
                transform
            )
            .unwrap()
        );
    }

    #[test]
    fn array_availability_cache_tracks_selection_document_and_context() {
        let mut app = test_app();
        assert!(!app.selection_can_array());
        let p = app
            .document
            .add_cuboid("Part", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let def = app.document.resolve_occurrence(&p).unwrap().definition.id;
        let edge = *app
            .document
            .effective_geometry(&p)
            .unwrap()
            .segments
            .keys()
            .next()
            .unwrap();
        let target = SelectionTarget {
            occurrence: p,
            entity: SelectedEntity::Segment(edge),
        };
        app.selection.select(target.clone());
        assert!(app.selection_can_array());
        for _ in 0..100 {
            assert!(app.selection_can_array());
        }
        app.selection.set_context(EditContext::Definition(
            meshcraft_core::document::DefinitionId(def.0 + 1000),
        ));
        app.selection.select(target.clone());
        assert!(!app.selection_can_array());
        app.selection.set_context(EditContext::Definition(def));
        app.selection.select(target);
        assert!(app.selection_can_array());
        app.document
            .replace_definition_geometry_raw(def, vec![], vec![], vec![])
            .unwrap();
        assert!(!app.selection_can_array());
        app.selection.clear();
        assert!(!app.selection_can_array());
    }
    #[test]
    fn egui_workspace_renders_full_chrome_and_a_viewport() {
        let mut app = test_app();
        let context = egui::Context::default();

        let output = run_native_workspace_frame(&mut app, &context);

        assert!(app.viewport_rect.width() > 600.0);
        assert!(app.viewport_rect.height() > 400.0);
        assert!(
            !output.shapes.is_empty(),
            "native shell must draw UI shapes"
        );
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

    #[test]
    fn buttons_profile_keeps_toolbars_floating_and_exposes_tools_on_right_click() {
        let context = egui::Context::default();
        let mut app = test_app();
        app.preferences.profile = UiProfile::Buttons;
        app.run_command("tool.rectangle");

        run_workspace_frame(&mut app, &context, Vec::new());
        assert!(
            app.viewport_rect.width() > 760.0,
            "floating toolbars should not reserve a fixed side column: {}",
            app.viewport_rect.width()
        );

        let center = app.viewport_rect.center();
        let position = egui::pos2(app.viewport_rect.right() - 50.0, center.y);
        run_workspace_frame(&mut app, &context, vec![Event::PointerMoved(position)]);
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: Modifiers::default(),
            }],
        );
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: Modifiers::default(),
            }],
        );
        run_workspace_frame(&mut app, &context, Vec::new());
        let menu = run_workspace_frame(&mut app, &context, Vec::new());

        assert!(egui::Popup::is_any_open(&context));
        let root = text_center_in_row(&menu, "Construction", 850.);
        run_workspace_frame(&mut app, &context, vec![Event::PointerMoved(root)]);
        let submenu = run_workspace_frame(&mut app, &context, vec![]);
        let draw = text_center_in_row(&submenu, "Draw", 850.);
        run_workspace_frame(&mut app, &context, vec![Event::PointerMoved(draw)]);
        let submenu = run_workspace_frame(&mut app, &context, vec![]);
        let line = text_center_in_row(&submenu, "Line", 850.0);
        click_workspace_position(&mut app, &context, line);

        assert_eq!("Line", app.active_tool);
    }

    #[test]
    fn world_pointer_request_matches_equivalent_gui_line_hits() {
        let start = Vec3::new(2.0, 0.0, 0.0);
        let end = Vec3::new(4.0, 0.0, 2.0);

        let mut gui_app = test_app();
        gui_app.active_tool = "Line".to_owned();
        let gui_context = egui::Context::default();
        run_workspace_frame(&mut gui_app, &gui_context, Vec::new());
        let gui_rect = gui_app.viewport_rect;
        let start_screen = gui_app.viewport.world_to_screen(gui_rect, start);
        let end_screen = gui_app.viewport.world_to_screen(gui_rect, end);
        assert!(gui_rect.contains(start_screen) && gui_rect.contains(end_screen));
        click_workspace_position(&mut gui_app, &gui_context, start_screen);
        click_workspace_position(&mut gui_app, &gui_context, end_screen);

        let mut request_app = test_app();
        request_app.active_tool = "Line".to_owned();
        run_workspace_frame(&mut request_app, &egui::Context::default(), Vec::new());
        let request_rect = request_app.viewport_rect;
        request_app.handle_world_pointer_request(request_rect, start, 1.0);
        request_app.handle_world_pointer_request(request_rect, end, 1.0);

        assert_eq!("Line", gui_app.active_tool);
        assert_eq!("Line", request_app.active_tool);
        let gui_line = gui_app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        let requested_line = request_app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        let mut gui_points = gui_line
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        let mut requested_points = requested_line
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        gui_points.sort_by(|left, right| left.x.total_cmp(&right.x));
        requested_points.sort_by(|left, right| left.x.total_cmp(&right.x));
        assert_eq!(2, gui_points.len());
        assert_eq!(1, gui_line.segments.len());
        assert_eq!(1, requested_line.segments.len());
        assert!(
            gui_points
                .iter()
                .zip(&requested_points)
                .all(|(gui, requested)| gui.distance(*requested) < 1.0e-5)
        );
        assert!(
            requested_points
                .iter()
                .zip([start, end])
                .all(|(requested, expected)| requested.distance(expected) < 1.0e-5)
        );
    }

    #[test]
    fn open_menu_captures_wheel_and_clicks_from_an_overlapping_viewport_tool() {
        let mut app = test_app();
        app.active_tool = "Line".to_owned();
        let context = egui::Context::default();
        let menu_bar = run_workspace_frame(&mut app, &context, Vec::new());
        let file_menu = text_center_in_row(&menu_bar, "Files", 48.0);
        click_workspace_position(&mut app, &context, file_menu);
        let output = run_workspace_frame(&mut app, &context, Vec::new());
        let quit_item = text_center_in_row(&output, "Exit", 320.0);
        assert!(
            app.viewport_rect.contains(quit_item),
            "File menu item should overlap the viewport rectangle in this layout"
        );

        let revision = app.document.revision();
        let camera = app.viewport.legacy_camera_state();
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::PointerMoved(quit_item),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        assert_eq!(camera, app.viewport.legacy_camera_state());
        assert_eq!(None, app.pointer_in_viewport);

        click_workspace_position(&mut app, &context, quit_item);
        assert!(app.quit_requested);
        assert_eq!(None, app.drawing_tool.anchor());
        assert_eq!(revision, app.document.revision());
    }

    #[test]
    fn open_menu_escape_does_not_cancel_the_active_viewport_tool() {
        let mut app = test_app();
        app.active_tool = "Line".to_owned();
        let context = egui::Context::default();
        let menu_bar = run_workspace_frame(&mut app, &context, Vec::new());
        let file_menu = text_center_in_row(&menu_bar, "Files", 48.0);
        click_workspace_position(&mut app, &context, file_menu);
        assert!(egui::Popup::is_any_open(&context));

        let revision = app.document.revision();
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );

        assert_eq!("Line", app.active_tool);
        assert_eq!(None, app.drawing_tool.anchor());
        assert_eq!(revision, app.document.revision());
    }

    #[test]
    fn escape_cancels_tool_then_clears_selection_then_guides() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("escape selection", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.set_selection([occurrence]);
        app.guides.push(
            ConstructionGuide::at_point(ConstructionGuideKind::Grid, Vec3::ZERO, Vec3::Y).unwrap(),
        );
        app.run_command("tool.line");
        app.handle_viewport_click_at_point(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0)),
            Pos2::ZERO,
            None,
            Some(Vec3::ZERO),
        );
        let context = egui::Context::default();
        let escape = || Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::default(),
        };

        run_workspace_frame(&mut app, &context, vec![escape()]);

        assert_eq!("Select", app.active_tool);
        assert!(!app.selection.selected().is_empty());
        assert_eq!(1, app.guides.len());
        assert_eq!(None, app.drawing_tool.anchor());

        run_workspace_frame(&mut app, &context, vec![escape()]);
        assert!(app.selection.selected().is_empty());
        assert_eq!(1, app.guides.len());
        run_workspace_frame(&mut app, &context, vec![escape()]);

        assert_eq!("Select", app.active_tool);
        assert!(app.selection.selected().is_empty());
        assert!(app.guides.is_empty());
        assert_eq!(None, app.drawing_tool.anchor());
    }

    #[test]
    fn native_selector_toggles_selected_entities_and_keeps_multiclick_rules() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("cube", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let triangle = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let target = SelectionTarget {
            occurrence,
            entity: SelectedEntity::Triangle(triangle),
        };
        let position = meshcraft_core::geometry::Vec2::new(40.0, 50.0);

        app.handle_native_selection_click(
            Some(target.clone()),
            egui::Modifiers::default(),
            100,
            position,
        );
        assert_eq!(1, app.selection.selected().len());
        app.handle_native_selection_click(
            Some(target.clone()),
            egui::Modifiers::default(),
            600,
            position,
        );
        assert!(app.selection.selected().is_empty());

        app.handle_native_selection_click(
            Some(target.clone()),
            egui::Modifiers::default(),
            1_000,
            position,
        );
        app.handle_native_selection_click(
            Some(target),
            egui::Modifiers::default(),
            1_200,
            position,
        );
        assert!(matches!(app.selection.context, EditContext::Occurrence(_)));
        assert!(app.selection.selected().is_empty());
        // Once inside, a double-click expands coplanar faces rather than re-entering.
        let target = SelectionTarget {
            occurrence: app.document.occurrence_refs().unwrap()[0].clone(),
            entity: SelectedEntity::Triangle(triangle),
        };
        app.handle_native_selection_click(
            Some(target.clone()),
            egui::Modifiers::default(),
            2000,
            position,
        );
        app.handle_native_selection_click(Some(target), egui::Modifiers::default(), 2200, position);
        assert!(app.selection.summary().faces >= 2);
        app.handle_native_escape();
        assert_eq!(EditContext::Document, app.selection.context);
    }

    #[test]
    fn object_name_edit_renames_every_selected_prototype_in_one_undo() {
        let mut app = test_app();
        let a = app
            .document
            .add_cuboid("A", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let b = app
            .document
            .add_cuboid("B", Vec3::new(1., 1., 1.), Vec3::X * 3.)
            .unwrap();
        let c = app
            .document
            .add_cuboid("C", Vec3::new(1., 1., 1.), Vec3::X * 6.)
            .unwrap();
        app.set_selection([a.clone(), b.clone()]);
        let cursor = app.history.cursor();
        app.rename_native_selected_object("Renamed").unwrap();
        assert_eq!(cursor + 1, app.history.cursor());
        for p in [&a, &b] {
            assert_eq!(
                "Renamed",
                app.document.resolve_occurrence(p).unwrap().definition.name
            );
        }
        assert_eq!(
            "C",
            app.document.resolve_occurrence(&c).unwrap().definition.name
        );
        app.run_command("edit.undo");
        assert_eq!(
            "A",
            app.document.resolve_occurrence(&a).unwrap().definition.name
        );
        assert_eq!(
            "B",
            app.document.resolve_occurrence(&b).unwrap().definition.name
        );
    }

    #[test]
    fn native_selected_object_rename_is_transactional_and_undoable() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("cube", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .expect("cuboid occurrence is valid");
        app.handle_native_selection_click(
            Some(SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Occurrence,
            }),
            egui::Modifiers::default(),
            100,
            meshcraft_core::geometry::Vec2::ZERO,
        );

        assert!(app.rename_native_selected_object("renamed cube").unwrap());
        assert_eq!(
            "renamed cube",
            app.document
                .resolve_occurrence(&occurrence)
                .unwrap()
                .definition
                .name
        );
        assert!(app.can_undo_document());

        app.run_command("edit.undo");

        assert_eq!(
            "cube",
            app.document
                .resolve_occurrence(&occurrence)
                .unwrap()
                .definition
                .name
        );
    }

    #[test]
    fn native_object_inspector_selects_instances_and_enters_edit_context() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("cube", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .expect("cuboid occurrence is valid");

        assert!(app.native_top_level_occurrences().contains(&occurrence));
        app.select_native_object_from_inspector(&occurrence, false)
            .expect("the inspector can select a valid object instance");
        assert_eq!(
            vec![SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Occurrence,
            }],
            app.native_selection_targets()
        );

        app.select_native_object_from_inspector(&occurrence, true)
            .expect("the inspector can enter a valid object instance");
        assert_eq!(
            EditContext::Occurrence(occurrence),
            app.native_edit_context()
        );
    }

    #[test]
    fn native_prototype_view_is_shared_until_geometry_context_or_document_changes() {
        let mut app = test_app();
        let path = app
            .document
            .add_cuboid("prototype", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let external = app
            .document
            .add_cuboid("reference", Vec3::new(2., 2., 2.), Vec3::X * 5.)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&path)
            .unwrap()
            .definition
            .id;
        app.selection
            .set_context(EditContext::Occurrence(path.clone()));
        let snapshot = app.native_view_snapshot();
        let revision = app.document.revision();
        let origin = Vec3::new(0.3, 10., 0.4);
        let snap = app
            .native_snap_point_on_ray(origin, -Vec3::Y, 0.01)
            .unwrap();
        app.native_cursor_axes(snap).unwrap();
        let target = app.pick_native_target(origin, -Vec3::Y, 0.01).unwrap();
        app.handle_native_selection_click(
            Some(target),
            Modifiers::default(),
            1000,
            meshcraft_core::geometry::Vec2::ZERO,
        );
        assert!(std::sync::Arc::ptr_eq(
            &snapshot,
            &app.native_view_snapshot()
        ));
        assert_eq!(revision, app.document.revision());
        // External geometry remains a snapping reference but cannot be selected in this context.
        let outside = Vec3::new(5.3, 10., 0.4);
        assert!(
            app.native_snap_point_on_ray(outside, -Vec3::Y, 0.01)
                .is_some()
        );
        assert!(app.pick_native_target(outside, -Vec3::Y, 0.01).is_none());
        app.document
            .append_planar_rectangle_to_definition(
                definition,
                [
                    Vec3::new(0., 4., 0.),
                    Vec3::new(2., 4., 0.),
                    Vec3::new(2., 4., 2.),
                    Vec3::new(0., 4., 2.),
                ],
            )
            .unwrap();
        let snap = app
            .native_snap_point_on_ray(origin, -Vec3::Y, 0.01)
            .unwrap();
        assert!((snap.point.y - 4.).abs() < 1e-9);
        let changed = app.native_view_snapshot();
        assert!(!std::sync::Arc::ptr_eq(&snapshot, &changed));
        app.selection.set_context(EditContext::Occurrence(external));
        assert!(!std::sync::Arc::ptr_eq(
            &changed,
            &app.native_view_snapshot()
        ));
        app.selection.set_context(EditContext::Document);
        app.native_view_snapshot();
        assert!(app.native_edit_view.is_none());
        app.selection.set_context(EditContext::Occurrence(path));
        let old = app.native_view_snapshot();
        app.document_generation += 1;
        let replaced = app.native_view_snapshot();
        assert!(!std::sync::Arc::ptr_eq(&old, &replaced));
    }

    #[test]
    fn native_prototype_view_invalidates_on_script_frames_without_authored_revision_change() {
        use meshcraft_core::document::scripts::{ScriptFrame, ScriptState};
        let mut app = test_app();
        let path = app
            .document
            .add_cuboid("prototype", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        app.selection
            .set_context(EditContext::Occurrence(path.clone()));
        let old = app.native_view_snapshot();
        let revision = app.document.revision();
        app.document.install_script_frames(
            [(
                path.clone(),
                ScriptFrame {
                    geometry: app.document.effective_geometry(&path).unwrap().clone(),
                    hotspots: vec![],
                    state: ScriptState::default(),
                },
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(revision, app.document.revision());
        assert!(!std::sync::Arc::ptr_eq(&old, &app.native_view_snapshot()));
    }

    #[test]
    fn native_toolbar_catalog_has_unique_ordered_functional_groups() {
        assert_eq!(
            &crate::functional_ui::GROUPS[..3],
            &["Files", "Clipboard & History", "Selection"]
        );
        let groups = MeshcraftApp::native_toolbar_catalog();
        assert_eq!(
            crate::functional_ui::GROUPS,
            groups
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
        );
        let mut ids = std::collections::BTreeSet::new();
        for (root, commands) in &groups {
            assert!(!commands.is_empty());
            for command in commands {
                assert!(
                    ids.insert(command.id.as_str()),
                    "duplicate toolbar command {}",
                    command.id.as_str()
                );
                assert_eq!(command.presentation.menu_path[0], *root);
            }
        }
        for (root, id) in [
            ("Construction", "tool.builtin.polyline"),
            ("Meshing", "geometry.fill_loops"),
            ("Solids", "tool.builtin.mesh_intersection"),
            ("Civil", "tool.builtin.arch_wall"),
            ("Materials", "material.new_wood"),
            ("Materials", "material.new_shader"),
            ("Objects", "object.delete_definition"),
            ("Objects", "object.bake_to_shader"),
            ("Hotspots", "hotspot.begin_placement"),
            ("Files", "file.open"),
            ("Settings", "view.model_settings"),
        ] {
            assert!(
                groups
                    .iter()
                    .find(|(name, _)| name == root)
                    .unwrap()
                    .1
                    .iter()
                    .any(|c| c.id.as_str() == id)
            );
        }
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        assert!(session.native_command_state("tool.line").enabled);
        assert!(
            !session
                .native_command_state("tool.builtin.push_pull")
                .enabled
        );
        assert!(
            !session
                .native_command_state("tool.builtin.object_cut")
                .enabled
        );

        let visibility = session.native_toolbar_visibility();
        assert_eq!(crate::functional_ui::GROUPS.len(), visibility.len());
        assert!(visibility.iter().all(|shown| *shown));
        let objects = crate::functional_ui::GROUPS
            .iter()
            .position(|root| *root == "Objects")
            .unwrap();
        session.run_command("view.toolbar.actions.hide");
        assert!(!session.native_toolbar_visibility()[objects]);
        session.run_command("view.toolbars.toggle");
        assert!(
            session
                .native_toolbar_visibility()
                .iter()
                .all(|shown| !shown)
        );
        session.run_command("view.toolbars.toggle");
        assert!(session.native_toolbar_visibility()[0]);
        assert!(!session.native_toolbar_visibility()[objects]);
        assert!(session.native_panels_visible());
        session.run_command("view.panels.toggle");
        assert!(!session.native_panels_visible());
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
        assert!(
            entries.len() >= 231,
            "the pinned legacy catalog must remain available"
        );
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
            34,
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
        assert!(shell_commands().get("tool.copy_mode").is_some());
        assert!(shell_commands().get("paint.set_color").is_some());
        assert!(shell_commands().get("paint.apply_selection").is_some());
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
    fn shell_visibility_persists_and_invalid_updates_are_atomic() {
        let mut app = test_app();
        let document = app.document.clone();
        let mut payload = CommandPayload::default();
        payload.values.extend([
            ("menu_bar_visible".into(), json!(false)),
            ("ribbon_visible".into(), json!(false)),
            ("left_panel_visible".into(), json!(false)),
            (
                "menu_visibility".into(),
                json!({"File":false,"Render":false}),
            ),
        ]);
        assert!(
            app.dispatch_command("ui.update_preferences", &payload)
                .accepted
        );
        let native = app.native_ui_preferences();
        assert!(!native.menu_bar_visible && !native.ribbon_visible && !native.left_panel_visible);
        assert_eq!(Some(&false), native.menu_visibility.get("File"));
        assert_eq!(app.document, document);
        assert!(!app.document_is_dirty());
        let encoded = serde_json::to_string(&app.preferences).unwrap();
        let restored: Preferences = serde_json::from_str(&encoded).unwrap();
        assert_eq!(app.preferences, restored);
        let roundtrip_payload = app.native_ui_preferences_payload();
        assert!(
            app.dispatch_command("ui.update_preferences", &roundtrip_payload)
                .accepted
        );
        assert_eq!(app.preferences, restored);
        for invalid in [
            json!({"Unknown":true}),
            json!({"File":"hidden"}),
            json!(false),
        ] {
            payload
                .values
                .insert("menu_bar_visible".into(), json!(true));
            payload.values.insert("menu_visibility".into(), invalid);
            assert_eq!(
                CommandCompletion::Failed,
                app.dispatch_command("ui.update_preferences", &payload)
                    .result
                    .completion
            );
            assert_eq!(app.preferences, restored);
        }
        let legacy: Preferences = serde_json::from_str("{}").unwrap();
        assert!(legacy.menu_bar_visible && legacy.ribbon_visible && legacy.left_panel_visible);
        assert!(legacy.menu_visibility.is_empty());
    }

    #[test]
    fn left_panel_width_roundtrips_and_rejects_out_of_range_updates() {
        let mut app = test_app();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("left_panel_width".into(), json!(370.));
        assert_eq!(
            app.dispatch_command("ui.update_preferences", &payload)
                .result
                .completion,
            CommandCompletion::Completed
        );
        assert_eq!(app.native_ui_preferences().left_panel_width, 370.);
        let saved = app.preferences.clone();
        assert_eq!(
            saved,
            serde_json::from_str::<Preferences>(&serde_json::to_string(&saved).unwrap()).unwrap()
        );
        for value in [100., 601., -10.] {
            payload
                .values
                .insert("left_panel_width".into(), json!(value));
            assert_eq!(
                app.dispatch_command("ui.update_preferences", &payload)
                    .result
                    .completion,
                CommandCompletion::Failed
            );
            assert_eq!(saved, app.preferences);
        }
        assert_eq!(
            serde_json::from_str::<Preferences>("{}")
                .unwrap()
                .left_panel_width,
            230.
        );
    }

    #[test]
    fn settings_accordion_and_shader_quality_persist_without_changing_toolbars() {
        let mut app = test_app();
        let revision = app.document.revision();
        let old_toolbar_behavior = app.preferences.auto_collapse_toolbars;
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("accordion_settings_panels".into(), json!(true));
        payload
            .values
            .insert("antialiasing_quality".into(), json!(3));
        assert!(
            app.dispatch_command("ui.update_preferences", &payload)
                .accepted
        );
        assert!(app.native_ui_preferences().accordion_settings_panels);
        assert_eq!(app.native_ui_preferences().antialiasing_quality, 3);
        assert_eq!(app.preferences.auto_collapse_toolbars, old_toolbar_behavior);
        assert_eq!(app.document.revision(), revision);
        let encoded = serde_json::to_string(&app.preferences).unwrap();
        assert_eq!(
            serde_json::from_str::<Preferences>(&encoded).unwrap(),
            app.preferences
        );
        let previous = app.preferences.clone();
        payload
            .values
            .insert("antialiasing_quality".into(), json!(4));
        payload
            .values
            .insert("accordion_settings_panels".into(), json!(false));
        assert_eq!(
            CommandCompletion::Failed,
            app.dispatch_command("ui.update_preferences", &payload)
                .result
                .completion
        );
        assert_eq!(
            app.preferences, previous,
            "invalid settings update must be atomic"
        );
        let legacy: Preferences = serde_json::from_str("{}").unwrap();
        assert!(!legacy.accordion_settings_panels);
        assert_eq!(legacy.antialiasing_quality, 2);
    }

    #[test]
    fn toolbar_positions_persist_and_invalid_layout_updates_are_atomic() {
        let mut app = test_app();
        let mut payload = CommandPayload::default();
        payload.values.insert(
            "toolbar_positions".to_owned(),
            json!({"actions": [132.5, 84.25], "camera": [900.0, 41.0]}),
        );
        assert!(
            app.dispatch_command("ui.update_preferences", &payload)
                .accepted
        );
        assert_eq!(
            Some(&[132.5, 84.25]),
            app.preferences.toolbar_positions.get("actions")
        );
        assert_eq!(
            Some(&[900.0, 41.0]),
            app.preferences.toolbar_positions.get("camera")
        );
        assert_eq!(
            Some([132.5, 84.25]),
            app.native_ui_preferences().toolbar_positions[1]
        );

        let serialized = serde_json::to_string(&app.preferences).unwrap();
        let restored: Preferences = serde_json::from_str(&serialized).unwrap();
        assert_eq!(app.preferences, restored);
        let legacy: Preferences = serde_json::from_value(json!({
            "toolbar_visibility": {"actions": false}
        }))
        .unwrap();
        assert!(legacy.toolbar_positions.is_empty());

        let previous = app.preferences.clone();
        for invalid in [
            json!({"unknown": [10.0, 20.0]}),
            json!({"actions": [-1.0, 20.0]}),
            json!({"actions": [10.0]}),
            json!({"actions": ["10", 20.0]}),
        ] {
            let mut payload = CommandPayload::default();
            payload
                .values
                .insert("toolbar_positions".to_owned(), invalid.clone());
            assert_eq!(
                meshcraft_core::commands::CommandCompletion::Failed,
                app.dispatch_command("ui.update_preferences", &payload)
                    .result
                    .completion,
                "invalid toolbar position payload should fail: {invalid}"
            );
            assert_eq!(previous, app.preferences);
        }
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
        assert_eq!(
            ["tool.select", "tool.distance"],
            super::legacy_toolbar_command_ids("in_tool_operators").as_slice()
        );
    }

    #[test]
    fn tool_cycle_commands_match_legacy_order_and_keep_hotspot_outside_tool_ids() {
        let mut app = test_app();
        let initial_revision = app.document.revision();

        for (id, shortcut) in [
            ("tool.cycle.planar_construction", "Shift+L"),
            ("tool.cycle.annotation", "D"),
            ("tool.cycle.operations", "O"),
            ("tool.cycle.modification", "M"),
        ] {
            let command = shell_commands()
                .get(id)
                .expect("cycle command is registered");
            assert_eq!(Some(shortcut), command.descriptor.shortcut.as_deref());
            assert!(command_matches(&command.descriptor, shortcut));
        }

        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Select", app.active_tool);
        assert!(app.tool_cycle_state.hotspot_placement_active());
        assert!(app.query_command_state("hotspot.cancel_placement").enabled);

        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Construction Line", app.active_tool);
        assert!(!app.tool_cycle_state.hotspot_placement_active());
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Line", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Polyline", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Double Line", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Rectangle", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Surface Rect", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Mesh", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Quad", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert_eq!("Circle", app.active_tool);
        app.run_command("tool.cycle.planar_construction");
        assert!(app.tool_cycle_state.hotspot_placement_active());
        app.run_command("hotspot.cancel_placement");
        assert!(!app.tool_cycle_state.hotspot_placement_active());

        app.run_command("tool.select");
        app.run_command("tool.cycle.annotation");
        assert_eq!("Linear Dimension", app.active_tool);
        app.run_command("tool.cycle.annotation");
        assert_eq!("Text", app.active_tool);
        app.run_command("tool.cycle.annotation");
        assert_eq!("Vector Text", app.active_tool);
        app.run_command("tool.cycle.annotation");
        assert_eq!("Linear Dimension", app.active_tool);

        app.run_command("tool.select");
        app.run_command("tool.cycle.operations");
        assert_eq!("Line Offset", app.active_tool);
        app.run_command("tool.cycle.operations");
        assert_eq!("Push/Pull", app.active_tool);
        app.run_command("tool.cycle.operations");
        assert_eq!("Extrude Swipe", app.active_tool);
        app.run_command("tool.cycle.operations");
        assert_eq!("Extrude Swipe 3D", app.active_tool);
        app.run_command("tool.cycle.operations");
        assert_eq!("Plane Section", app.active_tool);
        app.run_command("tool.cycle.operations");
        assert_eq!("Line Offset", app.active_tool);

        app.run_command("tool.select");
        app.run_command("tool.cycle.modification");
        assert_eq!("Move", app.active_tool);
        for (expected, current) in [
            ("Rotate", "Move"),
            ("Rotate 2", "Rotate"),
            ("Scale", "Rotate 2"),
            ("Squash", "Scale"),
            ("Stretch", "Squash"),
            ("Stretch Scale", "Stretch"),
            ("Rotate Stretch", "Stretch Scale"),
            ("Copy Multiple", "Rotate Stretch"),
            ("Planar Translate Multiple", "Copy Multiple"),
            ("Volumetric Translate Multiple", "Planar Translate Multiple"),
            ("Planar Rotate Multiple", "Volumetric Translate Multiple"),
            ("Helicoidal Rotate Multiple", "Planar Rotate Multiple"),
            ("Rotational Array", "Helicoidal Rotate Multiple"),
            ("Helical Array", "Rotational Array"),
            ("Rotational Array Even", "Helical Array"),
            ("Helical Array Even", "Rotational Array Even"),
            ("Move", "Helical Array Even"),
        ] {
            assert_eq!(current, app.active_tool);
            app.run_command("tool.cycle.modification");
            assert_eq!(expected, app.active_tool);
        }
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(0, app.history.entries().len());
    }

    #[test]
    fn hotspot_placement_authors_a_scene_handle_in_one_undoable_edit() {
        let mut app = test_app();
        let revision = app.document.revision();
        app.run_command("hotspot.begin_placement");
        assert!(app.tool_cycle_state.hotspot_placement_active());
        assert!(app.query_command_state("tool.cancel").enabled);

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::new(2.0, 0.0, 3.0)));

        assert!(!app.tool_cycle_state.hotspot_placement_active());
        assert_eq!("Select", app.active_tool);
        assert!(app.document.revision() > revision);
        assert_eq!(
            1,
            app.document
                .hotspots(Document::SCENE_ROOT_DEFINITION_ID)
                .unwrap()
                .len()
        );
        assert_eq!(1, app.history.entries().len());
        app.undo_document();
        assert!(
            app.document
                .hotspots(Document::SCENE_ROOT_DEFINITION_ID)
                .unwrap()
                .is_empty()
        );
        assert!(!app.query_command_state("hotspot.cancel_placement").enabled);
    }

    #[test]
    fn tool_cycle_shortcuts_obey_modifier_and_palette_focus_rules() {
        let mut app = test_app();
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());

        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: Key::L,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(app.tool_cycle_state.hotspot_placement_active());
        assert_eq!("Select", app.active_tool);

        let control = Modifiers::CTRL;
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::ModifiersChanged(control),
                Event::Key {
                    key: Key::D,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: control,
                },
            ],
        );
        assert!(app.tool_cycle_state.hotspot_placement_active());
        assert_eq!("Select", app.active_tool);

        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::ModifiersChanged(Modifiers::default()),
                Event::Key {
                    key: Key::D,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        assert!(!app.tool_cycle_state.hotspot_placement_active());
        assert_eq!("Linear Dimension", app.active_tool);

        app.run_command("tool.select");
        app.command_palette_open = true;
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::ModifiersChanged(Modifiers::default()),
                Event::Text("l".to_owned()),
                Event::Key {
                    key: Key::L,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        assert!(!app.tool_cycle_state.hotspot_placement_active());
        assert_eq!("Select", app.active_tool);

        app.command_palette_open = false;
        app.run_command("tool.cycle.planar_construction");
        assert!(app.tool_cycle_state.hotspot_placement_active());
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert!(!app.tool_cycle_state.hotspot_placement_active());
    }

    #[test]
    fn every_visible_tool_has_a_registry_command() {
        for tool in [
            "Select",
            "Cuboid",
            "Line",
            "Construction Line",
            "Rectangle",
            "Orbit",
        ] {
            let id = tool_command_id(tool);
            assert!(shell_commands().get(id).is_some());
        }
        assert_eq!(
            5,
            shell_commands()
                .entries()
                .filter(|command| {
                    matches!(
                        command.descriptor.id.as_str(),
                        "tool.select"
                            | "tool.line"
                            | "tool.rectangle"
                            | "tool.cuboid"
                            | "view.orbit"
                    )
                })
                .count()
        );
        for section in crate::functional_ui::SECTIONS {
            for (id, _) in section.commands {
                assert!(shell_commands().get(id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn functional_menu_metadata_has_no_aliases_or_duplicated_actions() {
        let mut seen = std::collections::BTreeSet::new();
        for section in crate::functional_ui::SECTIONS {
            for (id, label) in section.commands {
                assert!(seen.insert(*id), "{id} appears twice");
                let c = &shell_commands().get(id).unwrap().descriptor;
                assert_eq!(c.label, *label);
                assert_eq!(
                    c.presentation.menu_path,
                    vec![section.root.to_owned(), section.title.to_owned()]
                );
                assert!(c.presentation.menu_aliases.is_empty());
            }
        }
        for alias in [
            "tool.line",
            "tool.rectangle",
            "tool.builtin.select",
            "tool.builtin.object_place",
            "view.camera.orbit",
            "view.panel.objects.toggle",
            "view.panel.object.toggle",
            "material.save",
            "edit.group",
            "edit.ungroup",
            "paint.set_color",
        ] {
            assert!(
                shell_commands()
                    .get(alias)
                    .unwrap()
                    .descriptor
                    .presentation
                    .menu_path
                    .is_empty(),
                "API alias {alias} must not duplicate a visible action"
            );
        }
        assert_eq!(
            seen.len(),
            shell_commands()
                .entries()
                .filter(|c| !c.descriptor.presentation.menu_path.is_empty())
                .count()
        );
    }

    #[test]
    fn functional_menu_opens_draw_section_and_runs_the_registered_line_tool() {
        let context = egui::Context::default();
        let mut app = test_app();
        let bar = run_workspace_frame(&mut app, &context, vec![]);
        let root = text_center_in_row(&bar, "Construction", 48.);
        click_workspace_position(&mut app, &context, root);
        let menu = run_workspace_frame(&mut app, &context, vec![]);
        for title in ["Draw", "Annotations", "Guides", "Input & Settings"] {
            text_center_in_row(&menu, title, 650.);
        }
        let draw = text_center_in_row(&menu, "Draw", 650.);
        run_workspace_frame(&mut app, &context, vec![Event::PointerMoved(draw)]);
        let submenu = run_workspace_frame(&mut app, &context, vec![]);
        let line = text_center_in_row(&submenu, "Line", 750.);
        click_workspace_position(&mut app, &context, line);
        assert_eq!(app.active_tool, "Line");
        assert!(app.history.entries().is_empty());
    }

    #[test]
    fn menu_presentation_aliases_are_additive_for_older_command_metadata() {
        let old_presentation = serde_json::json!({
            "palette_path": "Edit> Copy",
            "menu_path": ["Edit"],
            "toolbar_group": null,
            "icon_key": null,
            "tutorial_action_id": null
        });
        let parsed: meshcraft_core::commands::CommandPresentation =
            serde_json::from_value(old_presentation).unwrap();
        assert!(parsed.menu_aliases.is_empty());
    }

    #[test]
    fn distance_operator_requires_a_line_anchor_and_does_not_edit_the_document() {
        let mut app = test_app();
        let initial_revision = app.document.revision();
        assert_eq!(
            Some("Ctrl+N"),
            shell_commands()
                .get("tool.distance")
                .unwrap()
                .descriptor
                .shortcut
                .as_deref()
        );
        assert!(!app.query_command_state("tool.distance").enabled);

        app.run_command("tool.line");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        assert!(app.query_command_state("tool.distance").enabled);
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());
        let control = Modifiers::CTRL;
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::ModifiersChanged(control),
                Event::Key {
                    key: Key::N,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: control,
                },
            ],
        );

        assert!(app.drawing_tool.distance_focused());
        assert_eq!(initial_revision, app.document.revision());
        assert_eq!(0, app.history.entries().len());
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
        let tools_menu = text_center_in_row(&menu_bar, "Solids", 48.0);
        click_workspace_position(&mut menu_path, &context, tools_menu);
        let open_menu = run_workspace_frame(&mut menu_path, &context, vec![]);
        let primitives = text_center_in_row(&open_menu, "Primitives", 500.0);
        run_workspace_frame(
            &mut menu_path,
            &context,
            vec![Event::PointerMoved(primitives)],
        );
        let primitives_menu = run_workspace_frame(&mut menu_path, &context, vec![]);
        let cuboid_item = text_center_in_row(&primitives_menu, "Cuboid", 700.0);
        click_workspace_position(&mut menu_path, &context, cuboid_item);
        assert_eq!("Cuboid", menu_path.active_tool);
        assert_eq!(
            vec!["Solids", "Primitives"],
            shell_commands()
                .get("tool.cuboid")
                .unwrap()
                .descriptor
                .presentation
                .menu_path
        );

        let toolbar_command_ids = super::legacy_toolbar_command_ids("primitives");
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
    fn guide_commands_keep_the_legacy_click_fallback_without_a_cursor_position() {
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
            assert_eq!(CommandKind::SessionMutation, descriptor.kind);
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
    fn guide_commands_place_at_cursor_without_interrupting_a_staged_tool() {
        let mut app = test_app();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let first_point = Vec3::new(2.0, 0.0, 1.0);
        app.activate_tool("Line");
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(first_point));
        assert_eq!(Some(first_point), app.drawing_tool.anchor());

        let cursor = SnapResult {
            point: Vec3::new(4.0, 3.0, -2.0),
            normal: Vec3::new(1.0, 2.0, 3.0).try_normalized(1.0e-12).unwrap(),
            kind: SnapKind::Face,
            screen_distance: 0.0,
        };
        app.cursor_snap = Some(cursor);
        let document_revision = app.document.revision();
        app.run_command("view.axis_guide");

        assert_eq!("Line", app.active_tool);
        assert_eq!(Some(first_point), app.drawing_tool.anchor());
        assert_eq!(1, app.guides.len());
        assert_eq!(ConstructionGuideKind::Axis, app.guides[0].kind);
        assert_eq!(cursor.point, app.guides[0].origin);
        assert!(app.guides[0].axis_w.distance(cursor.normal) < 1.0e-12);
        assert_eq!(document_revision, app.document.revision());

        app.run_command("view.grid_guide");
        assert_eq!("Line", app.active_tool);
        assert_eq!(Some(first_point), app.drawing_tool.anchor());
        assert_eq!(2, app.guides.len());
        assert_eq!(ConstructionGuideKind::Grid, app.guides[1].kind);
        assert_eq!(cursor.point, app.guides[1].origin);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn guides_keep_the_exact_segment_cursor_basis_for_commands_and_point_placement() {
        let mut app = test_app();
        let end = Vec3::new(4., 2., 3.);
        app.document
            .add_line_segment("guide alignment", Vec3::ZERO, end)
            .unwrap();
        let snap = SnapResult {
            point: end * 0.5,
            normal: Vec3::Y,
            kind: SnapKind::Midpoint,
            screen_distance: 0.,
        };
        let axes = app.native_cursor_axes(snap).unwrap();
        assert!(axes[2].distance(end.try_normalized(1e-12).unwrap()) < 1e-12);
        app.cursor_snap = Some(snap);
        app.run_command("view.axis_guide");
        app.run_command("view.grid_guide");
        app.active_tool = "Grid Guide".into();
        app.handle_native_world_pick(snap.point, Some(snap), None);
        assert_eq!(app.guides.len(), 3);
        for guide in &app.guides {
            assert_eq!(guide.origin, snap.point);
            assert_eq!([guide.axis_u, guide.axis_v, guide.axis_w], axes);
        }
    }

    #[test]
    fn guide_command_without_cursor_does_not_interrupt_an_active_tool() {
        let mut app = test_app();
        let first_point = Vec3::new(1.0, 0.0, 2.0);
        app.activate_tool("Line");
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(first_point));

        let result = app.dispatch_command("view.axis_guide", &CommandPayload::default());
        assert_eq!(CommandCompletion::Failed, result.result.completion);
        assert_eq!("Line", app.active_tool);
        assert_eq!(Some(first_point), app.drawing_tool.anchor());
        assert!(app.guides.is_empty());
    }

    #[test]
    fn guide_shortcut_places_at_the_live_cursor_while_a_tool_is_staged() {
        let mut app = test_app();
        let context = egui::Context::default();
        app.activate_tool("Line");
        let first_point = Vec3::new(2.0, 0.0, 1.0);
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(first_point));
        run_workspace_frame(&mut app, &context, Vec::new());

        let rect = app.viewport_rect;
        let pointer = app.viewport.world_to_screen(rect, Vec3::new(4.0, 0.0, 3.0));
        assert!(rect.contains(pointer));
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::PointerMoved(pointer),
                Event::Key {
                    key: egui::Key::G,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );

        assert_eq!("Line", app.active_tool);
        assert_eq!(Some(first_point), app.drawing_tool.anchor());
        assert_eq!(1, app.guides.len());
        assert_eq!(ConstructionGuideKind::Grid, app.guides[0].kind);
        assert_eq!(
            Some(app.guides[0].origin),
            app.cursor_snap.map(|snap| snap.point)
        );
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
    fn move_tool_transforms_a_selected_face_once_and_keeps_selection() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let selected = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        };
        app.selection.select(selected.clone());

        app.run_command("tool.builtin.move");
        assert_eq!(Some(TransformTool::Move), app.transform_tool_state.active());
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::X * 3.0));

        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        let face = &geometry.triangles[&face];
        assert!(face.vertices.iter().all(|reference| {
            geometry
                .vertex_position(*reference)
                .is_some_and(|point| point.x >= 3.0)
        }));
        assert_eq!(1, app.history.entries().len());
        assert_eq!(1, app.selection.selected().len());
        assert!(app.selection.selected().contains(&selected));
    }

    #[test]
    fn copy_transform_buttons_accept_mesh_fragments_and_preserve_all_sources() {
        for (command, picks) in [
            ("tool.move_copy", vec![Vec3::ZERO, Vec3::X * 3.]),
            ("tool.rotate_copy", vec![Vec3::ZERO, Vec3::X, Vec3::Z]),
        ] {
            let mut app = test_app();
            let instance = app
                .document
                .add_cuboid("Shared", Vec3::new(2., 2., 2.), Vec3::ZERO)
                .unwrap();
            let before = app.document.effective_geometry(&instance).unwrap().clone();
            let face = *before.triangles.keys().next().unwrap();
            app.selection.select(SelectionTarget {
                occurrence: instance.clone(),
                entity: SelectedEntity::Triangle(face),
            });
            assert!(app.query_command_state(command).enabled);
            app.run_command(command);
            assert!(app.transform_tool_state.copy_mode());
            for point in picks {
                app.handle_native_world_pick(point, None, None);
            }
            let after = app.document.effective_geometry(&instance).unwrap();
            assert_eq!(
                before.triangles.len() + 1,
                after.triangles.len(),
                "{command}"
            );
            for (id, v) in &before.vertices {
                assert_eq!(v, &after.vertices[id], "{command} changed source vertex");
            }
            for (id, t) in &before.triangles {
                assert_eq!(t, &after.triangles[id], "{command} changed source face");
            }
            assert!(
                app.selection
                    .selected()
                    .iter()
                    .all(|t| t.entity != SelectedEntity::Triangle(face))
            );
            app.run_command("edit.undo");
            assert_eq!(before, *app.document.effective_geometry(&instance).unwrap());
        }
    }

    #[test]
    fn native_push_pull_motion_updates_geometry_feedback_without_accepting_a_second_pick() {
        let mut app = test_app();
        let (_, faces) = app
            .document
            .append_planar_mesh_to_scene_root(
                vec![Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Z, Vec3::Z],
                vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                vec![[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        app.selection.apply(
            faces.into_iter().map(|id| SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::Triangle(id),
            }),
            SelectionMode::Replace,
        );
        app.run_command("tool.builtin.push_pull");
        app.handle_native_world_pick(Vec3::ZERO, None, None);
        let before = app.document.clone();
        let (preview, _) = app.native_tool_feedback_for_snap(
            SnapResult {
                point: Vec3::Y * 2.,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.,
            },
            Pos2::ZERO,
        );
        assert!(
            matches!(preview,Some(ToolPreview::PushPull { ref edges, ref triangles }) if !edges.is_empty() && triangles.iter().any(|(p,_)| p.iter().any(|v| v.y == 2.)))
        );
        assert_eq!(before, app.document);
        app.handle_native_escape();
        assert_eq!(before, app.document);
    }

    #[test]
    fn move_copy_mode_duplicates_selected_face_and_selects_the_copy() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        });
        app.run_command("tool.builtin.move");
        app.run_command("tool.copy_mode");
        assert!(app.transform_tool_state.copy_mode());
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::X * 3.0));

        assert_eq!(1, app.history.entries().len());
        assert_eq!(1, app.selection.selected().len());
        assert!(
            app.selection.selected().iter().all(|target| {
                matches!(target.entity, SelectedEntity::Triangle(id) if id != face)
            })
        );
        assert_eq!(
            13,
            app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .len()
        );
    }

    #[test]
    fn rotate_2_copy_mode_rotates_the_selection_about_its_derived_axis() {
        let mut app = test_app();
        let source_origin = Vec3::X * 2.0;
        let occurrence = app
            .document
            .add_cuboid("box", Vec3::new(1.0, 1.0, 1.0), source_origin)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });

        app.run_command("tool.builtin.rotate_2");
        assert!(app.status.contains("end angle point (B)"));
        app.run_command("tool.copy_mode");
        assert!(app.transform_tool_state.copy_mode());
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::X);
        pick_array_tool_point(&mut app, Vec3::Y);

        assert_eq!("Rotate 2", app.active_tool);
        assert_eq!(1, app.selection.selected().len());
        let copy = app.selection.selected().iter().next().unwrap();
        assert_ne!(occurrence, copy.occurrence);
        let copy_origin = app
            .document
            .resolve_occurrence(&copy.occurrence)
            .unwrap()
            .world_transform
            .translation;
        assert!(copy_origin.distance(Vec3::Y * 2.0) < 1.0e-9);
        assert_eq!(
            source_origin,
            app.document
                .resolve_occurrence(&occurrence)
                .unwrap()
                .world_transform
                .translation
        );
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn transform_numeric_input_finishes_the_same_pointer_preview() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_line_segment(
                "reference",
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
            )
            .unwrap();
        let segment = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .segments
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Segment(segment),
        });
        app.run_command("tool.builtin.move");
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());
        let rect = app.viewport_rect;
        let start = Vec3::new(2.0, 0.0, 0.0);
        let pointer = Vec3::new(3.0, 0.0, 0.0);
        let start_screen = app.viewport.world_to_screen(rect, start);
        let pointer_screen = app.viewport.world_to_screen(rect, pointer);
        click_workspace_position(&mut app, &context, start_screen);
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::PointerMoved(pointer_screen)],
        );
        app.run_command("tool.distance");
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::PointerMoved(pointer_screen),
                Event::Text("3".to_owned()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );

        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        let segment = &geometry.segments[&segment];
        let endpoints = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference).unwrap());
        assert!(
            endpoints
                .iter()
                .any(|point| point.distance(Vec3::new(5.0, 0.0, 0.0)) < 1.0e-6)
        );
        assert!(
            endpoints
                .iter()
                .any(|point| point.distance(Vec3::new(7.0, 0.0, 0.0)) < 1.0e-6)
        );
        assert_eq!(1, app.history.entries().len());
        assert_eq!("Move", app.active_tool);
    }

    #[test]
    fn paint_commands_apply_rgba_to_only_the_selected_face() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        });
        let color = Rgba8::new(12, 34, 56, 78);
        let mut payload = CommandPayload::default();
        for (key, value) in [
            ("red", color.red),
            ("green", color.green),
            ("blue", color.blue),
            ("alpha", color.alpha),
        ] {
            payload.values.insert(key.to_owned(), json!(value));
        }
        let color_result = app.dispatch_command("paint.set_color", &payload);
        assert_eq!(CommandCompletion::Completed, color_result.result.completion);
        app.activate_tool("Paint");
        let paint_preview = app
            .tool_preview(
                Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
                Some(Pos2::ZERO),
                None,
            )
            .expect("selected faces preview the paint-selection operation");
        assert!(matches!(
            &paint_preview,
            ToolPreview::Paint {
                triangles,
                color: preview_color,
                ..
            } if !triangles.is_empty() && *preview_color == color
        ));
        let paint_result =
            app.dispatch_command("paint.apply_selection", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, paint_result.result.completion);
        let repeated = app.dispatch_command("paint.apply_selection", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, repeated.result.completion);

        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(color, geometry.triangles[&face].color);
        assert_eq!(1, app.history.entries().len());
        assert_eq!(1, app.selection.selected().len());
    }

    #[test]
    fn paint_tool_click_paints_the_visible_face_under_the_pointer() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::new(-1.0, -1.0, -1.0))
            .unwrap();
        let resolved = app.document.resolve_occurrence(&occurrence).unwrap();
        let candidates = app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .values()
            .map(|triangle| {
                triangle.vertices.map(|reference| {
                    resolved.world_transform.transform_point(
                        app.document
                            .effective_geometry(&occurrence)
                            .unwrap()
                            .vertex_position(reference)
                            .unwrap(),
                    )
                })
            })
            .collect::<Vec<_>>();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        app.activate_tool("Paint");
        let (position, point, target) = candidates
            .into_iter()
            .find_map(|triangle| {
                let point = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                let position = app.viewport.world_to_screen(rect, point);
                let target = app.viewport.pick_selection_target(
                    rect,
                    &app.document,
                    position,
                    SnapScope::Document,
                    &app.selection_display_policy,
                )?;
                matches!(target.entity, SelectedEntity::Triangle(_))
                    .then_some((position, point, target))
            })
            .expect("a cuboid face should be visible to the viewport picker");
        let color = Rgba8::new(222, 44, 91, 177);
        app.current_paint_color = color;
        let preview = app.tool_preview(rect, Some(position), None);
        let Some(ToolPreview::Paint { triangles, .. }) = preview.as_ref() else {
            panic!("Paint should preview the hovered face in the current color");
        };
        assert_eq!(1, triangles.len());
        let feedback = app
            .tool_feedback(rect, Some(position), None, preview.as_ref())
            .expect("Paint shows feedback for its hovered target");
        assert!(feedback.text.contains("#de2c5bb1"), "{}", feedback.text);
        app.handle_viewport_click_at_point(rect, position, None, Some(point));

        let painted_id = match target.entity {
            SelectedEntity::Triangle(id) => id,
            _ => unreachable!(),
        };
        assert_eq!(
            color,
            app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles[&painted_id]
                .color
        );
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn primitive_commands_preview_and_commit_all_four_mesh_generators() {
        let cases = [
            ("primitive_sphere", "Sphere", vec![Vec3::ZERO, Vec3::X]),
            (
                "primitive_cylinder",
                "Cylinder",
                vec![Vec3::ZERO, Vec3::X, Vec3::Y * 2.0],
            ),
            (
                "primitive_cone",
                "Cone",
                vec![Vec3::ZERO, Vec3::X, Vec3::Y * 2.0],
            ),
            (
                "primitive_pill",
                "Pill",
                vec![Vec3::ZERO, Vec3::X, Vec3::Y * 2.0],
            ),
        ];
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for (tool_id, label, picks) in cases {
            let mut app = test_app();
            let command_id = format!("tool.builtin.{tool_id}");
            assert!(app.query_command_state(&command_id).enabled);
            let activated = app.dispatch_command(&command_id, &CommandPayload::default());
            assert!(activated.accepted, "{}: {}", label, app.status);
            assert_eq!(label, app.active_tool);

            for (index, point) in picks.iter().copied().enumerate() {
                if index + 1 == picks.len() {
                    let preview = app
                        .tool_preview(
                            rect,
                            Some(Pos2::ZERO),
                            Some(SnapResult {
                                point,
                                normal: Vec3::Y,
                                kind: SnapKind::Free,
                                screen_distance: 0.0,
                            }),
                        )
                        .expect("the last primitive pick previews a closed solid");
                    assert!(matches!(
                        preview,
                        ToolPreview::Primitive { ref edges, ref triangles }
                            if !edges.is_empty() && !triangles.is_empty()
                    ));
                    let feedback = app
                        .tool_feedback(
                            rect,
                            Some(Pos2::ZERO),
                            Some(SnapResult {
                                point,
                                normal: Vec3::Y,
                                kind: SnapKind::Free,
                                screen_distance: 0.0,
                            }),
                            Some(&preview),
                        )
                        .expect("primitive input has dimensional feedback");
                    assert!(
                        feedback.text.to_lowercase().contains("radius"),
                        "{}: {}",
                        label,
                        feedback.text
                    );
                }
                app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
            }

            let geometry = app
                .document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap();
            assert!(!geometry.segments.is_empty(), "{label} has wireframe edges");
            assert!(!geometry.triangles.is_empty(), "{label} has closed faces");
            assert_eq!(
                geometry.segments.len() + geometry.triangles.len(),
                app.selection.selected().len(),
                "all generated edges and faces should be selected"
            );
            assert_eq!(1, app.history.entries().len());
            assert!(
                geometry
                    .segments
                    .values()
                    .all(|segment| segment.color == Rgba8::BLACK)
            );
            assert!(
                geometry
                    .triangles
                    .values()
                    .all(|triangle| triangle.color == Rgba8::ANSI_GREY)
            );

            app.run_command("edit.undo");
            let geometry = app
                .document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap();
            assert!(geometry.segments.is_empty());
            assert!(geometry.triangles.is_empty());
        }
    }

    #[test]
    fn primitive_creation_in_a_transformed_instance_uses_its_local_definition() {
        let mut app = test_app();
        let prototype = app
            .document
            .add_cuboid("primitive host", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&prototype)
            .unwrap()
            .definition
            .id;
        let occurrence = app
            .document
            .add_layout_entry(
                app.document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::X * 10.0),
            )
            .map(OccurrenceRef::root)
            .unwrap();
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        assert!(
            app.dispatch_command(
                "tool.builtin.primitive_cylinder",
                &CommandPayload::default()
            )
            .accepted
        );

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [
            Vec3::X * 10.0,
            Vec3::X * 12.0,
            Vec3::X * 10.0 + Vec3::Y * 3.0,
        ] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
        }

        let resolved = app.document.resolve_occurrence(&occurrence).unwrap();
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(108, geometry.triangles.len());
        let world_vertices = geometry
            .vertices
            .values()
            .map(|vertex| resolved.world_transform.transform_point(vertex.position))
            .collect::<Vec<_>>();
        assert!(world_vertices.contains(&(Vec3::X * 12.0)));
        assert!(world_vertices.contains(&(Vec3::X * 10.0 + Vec3::Y * 3.0)));
        assert_eq!(
            108,
            app.document
                .effective_geometry(&prototype)
                .unwrap()
                .triangles
                .len()
        );
        assert_eq!(1, app.history.entries().len());
        assert_eq!(definition, app.document.definition(definition).unwrap().id);
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| target.occurrence == occurrence)
        );
    }

    #[test]
    fn canceling_a_primitive_pick_leaves_the_document_unchanged() {
        let mut app = test_app();
        let revision = app.document.revision();
        app.dispatch_command("tool.builtin.primitive_pill", &CommandPayload::default());
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        assert_eq!(1, app.primitive_tool_state.picks().len());
        app.handle_escape();
        assert_eq!("Select", app.active_tool);
        assert_eq!(None, app.primitive_tool_state.active());
        assert!(app.primitive_tool_state.picks().is_empty());
        assert_eq!(revision, app.document.revision());
        assert!(app.history.entries().is_empty());
    }

    #[test]
    fn rectangle_tool_creates_ground_mesh_geometry_in_the_active_scene_graph() {
        let mut app = test_app();
        app.activate_tool("Rectangle");
        app.viewport.set_orthographic_view(OrthoView::Top);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

        app.handle_viewport_click(rect, rect.center(), None);
        app.handle_viewport_click(rect, Pos2::new(456.0, 244.0), None);

        assert_eq!("Rectangle", app.active_tool);
        assert_eq!(6, app.selection.selected().len());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert!(
            geometry
                .vertices
                .values()
                .all(|vertex| vertex.position.y == 0.0)
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
                .any(|vertex| vertex.position.z.abs() == 2.0)
        );
        assert_eq!(1, app.history.entries().len());
        app.run_command("edit.undo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert!(geometry.vertices.is_empty());
        assert!(geometry.segments.is_empty());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn quad_and_circle_commands_create_configured_geometry_in_the_scene_root() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut quad = test_app();
        let command = quad.dispatch_command("tool.builtin.quad", &CommandPayload::default());
        assert!(command.accepted);
        for point in [Vec3::ZERO, Vec3::X * 2.0, Vec3::Z * 3.0] {
            quad.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
        }
        assert_eq!("Quad", quad.active_tool);
        let geometry = quad
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert_eq!(6, quad.selection.selected().len());
        assert_eq!(1, quad.history.entries().len());
        quad.run_command("edit.undo");
        assert!(
            quad.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .triangles
                .is_empty()
        );

        let mut circle = test_app();
        assert!(
            circle
                .dispatch_command("tool.builtin.circle", &CommandPayload::default())
                .accepted
        );
        let mut settings = CommandPayload::default();
        settings
            .values
            .insert("circle_segments".to_owned(), json!(12));
        assert!(
            circle
                .dispatch_command("model.update_settings", &settings)
                .accepted
        );
        let face_pick = |point| {
            Some(SnapResult {
                point,
                normal: Vec3::X,
                kind: SnapKind::Face,
                screen_distance: 0.0,
            })
        };
        circle.handle_viewport_click_at_point(
            rect,
            Pos2::ZERO,
            face_pick(Vec3::ZERO),
            Some(Vec3::ZERO),
        );
        circle.handle_viewport_click_at_point(
            rect,
            Pos2::ZERO,
            face_pick(Vec3::Y * 2.0),
            Some(Vec3::Y * 2.0),
        );
        assert_eq!("Circle", circle.active_tool);
        let geometry = circle
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(13, geometry.vertices.len());
        assert_eq!(12, geometry.segments.len());
        assert_eq!(12, geometry.triangles.len());
        assert_eq!(24, circle.selection.selected().len());
        assert_eq!(2, circle.history.entries().len());
        circle.run_command("edit.undo");
        assert!(
            circle
                .document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .triangles
                .is_empty()
        );
    }

    #[test]
    fn polyline_tool_creates_open_paths_and_closes_concave_meshes() {
        let mut open = test_app();
        open.run_command("tool.builtin.polyline");
        let pick = |app: &mut MeshcraftApp, point| {
            app.handle_viewport_click_at_point_with_selection(
                Rect::NOTHING,
                Pos2::ZERO,
                None,
                Some(point),
                egui::Modifiers::default(),
                1,
            );
        };
        pick(&mut open, Vec3::ZERO);
        pick(&mut open, Vec3::X * 2.0);
        pick(&mut open, Vec3::new(2.0, 0.0, 2.0));
        assert!(open.query_command_state("tool.finish").enabled);
        let finish = open.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(
            CommandCompletion::Completed,
            finish.result.completion,
            "{}",
            open.status
        );
        let open_occurrence = open
            .selection
            .selected()
            .iter()
            .next()
            .unwrap()
            .occurrence
            .clone();
        let geometry = open.document.effective_geometry(&open_occurrence).unwrap();
        assert_eq!(2, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
        assert_eq!(2, open.selection.summary().edges);
        assert_eq!(1, open.history.entries().len());

        let mut closed = test_app();
        closed.run_command("tool.builtin.polyline");
        let concave = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 3.0),
            Vec3::new(0.0, 0.0, 3.0),
        ];
        for point in concave.into_iter().chain([Vec3::new(0.05, 0.0, 0.05)]) {
            pick(&mut closed, point);
        }
        let closed_occurrence = closed
            .selection
            .selected()
            .iter()
            .next()
            .unwrap()
            .occurrence
            .clone();
        let geometry = closed
            .document
            .effective_geometry(&closed_occurrence)
            .unwrap();
        assert_eq!(6, geometry.segments.len());
        assert_eq!(4, geometry.triangles.len());
        assert_eq!(1, closed.history.entries().len());
        assert_eq!("Polyline", closed.active_tool);
    }

    #[test]
    fn mesh_command_builds_incremental_faces_in_the_active_geometry_graph() {
        let mut app = test_app();
        let activation = app.dispatch_command("tool.builtin.mesh", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Mesh", app.active_tool);
        assert!(app.mesh_tool_state.active());

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let picks = [Vec3::ZERO, Vec3::X * 2.0, Vec3::new(2.0, 2.0, 0.0)];
        for point in picks {
            let normal = Vec3::Z * -1.0;
            let snap = SnapResult {
                point,
                normal,
                kind: SnapKind::Face,
                screen_distance: 0.0,
            };
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap), Some(point));
        }
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(2, geometry.segments.len());
        assert_eq!(1, geometry.triangles.len());
        assert_eq!(2, app.history.entries().len());
        let face = geometry.triangles.values().next().unwrap();
        let [Some(a), Some(b), Some(c)] =
            face.vertices.map(|vertex| geometry.vertex_position(vertex))
        else {
            panic!("mesh triangle vertices should resolve");
        };
        assert!((b - a).cross(c - a).dot(Vec3::Z * -1.0) > 0.0);
        assert!(app.query_command_state("tool.finish").enabled);

        app.run_command("tool.finish");
        assert!(app.mesh_tool_state.active());
        assert!(app.mesh_tool_state.points().is_empty());
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::Y * 3.0));
        assert_eq!("Mesh", app.active_tool);
        assert_eq!(1, app.mesh_tool_state.points().len());

        app.handle_escape();
        assert_eq!("Select", app.active_tool);
        assert!(!app.mesh_tool_state.active());
    }

    #[test]
    fn push_pull_extrudes_selected_faces_with_color_preview_and_selected_results() {
        let mut app = test_app();
        let (_, face_ids) = app
            .document
            .append_planar_mesh_to_scene_root(
                vec![Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Z, Vec3::Z],
                vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                vec![[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        app.selection.apply(
            face_ids.iter().copied().map(|face| SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::Triangle(face),
            }),
            SelectionMode::Replace,
        );
        let color = Rgba8::new(35, 120, 210, 180);
        app.current_paint_color = color;
        app.run_command("paint.apply_selection");
        assert!(app.query_command_state("tool.builtin.push_pull").enabled);

        let activation = app.dispatch_command("tool.builtin.push_pull", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Push/Pull", app.active_tool);
        assert_eq!(
            "Push/Pull: 3.742 meter | ΔX -1.000, ΔY +2.000, ΔZ +3.000 meter",
            push_pull_feedback_text(
                Vec3::new(-1.0, 2.0, 3.0),
                &ModelSettings {
                    unit_name: "meter".into(),
                    ..Default::default()
                }
            )
        );
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click(
            rect,
            Pos2::ZERO,
            Some(SnapResult {
                point: Vec3::ZERO,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }),
        );
        app.push_pull_pointer = Some(Vec3::Y * 2.0);
        let Some(ToolPreview::PushPull { triangles, edges }) =
            app.tool_preview(rect, Some(Pos2::ZERO), None)
        else {
            panic!("Push/Pull should preview its colored top and side faces");
        };
        assert_eq!(10, triangles.len());
        assert!(triangles.iter().all(|(_, face_color)| *face_color == color));
        assert_eq!(9, edges.len());

        app.handle_viewport_click(
            rect,
            Pos2::ZERO,
            Some(SnapResult {
                point: Vec3::Y * 2.0,
                normal: Vec3::Y,
                kind: SnapKind::Grid,
                screen_distance: 0.0,
            }),
        );

        assert_eq!(2, app.history.entries().len());
        assert_eq!(10, app.selection.summary().faces);
        assert_eq!(8, app.selection.summary().edges);
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(12, geometry.triangles.len());
        assert_eq!(12, geometry.segments.len());
        assert!(app.selection.selected().iter().all(|target| {
            !matches!(target.entity, SelectedEntity::Triangle(id) if geometry.triangles[&id].color != color)
        }));

        app.run_command("edit.undo");
        assert_eq!(
            2,
            app.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .triangles
                .len()
        );
    }

    #[test]
    fn push_pull_numeric_input_uses_signed_distance_and_zero_is_atomic() {
        let mut app = test_app();
        let (_, face_ids) = app
            .document
            .append_planar_mesh_to_scene_root(
                vec![Vec3::ZERO, Vec3::X, Vec3::Z],
                vec![[0, 1], [1, 2], [2, 0]],
                vec![[0, 1, 2]],
            )
            .unwrap();
        let source_face = face_ids[0];
        let source_winding = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap()
            .triangles[&source_face]
            .vertices;
        app.selection.apply(
            face_ids.into_iter().map(|face| SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::Triangle(face),
            }),
            SelectionMode::Replace,
        );
        app.run_command("tool.builtin.push_pull");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.push_pull_pointer = Some(Vec3::Y * 2.0);
        assert!(app.query_command_state("tool.distance").enabled);
        app.run_command("tool.distance");
        app.push_pull_numeric_input = "-1.5".to_owned();
        assert!(app.query_command_state("tool.finish").enabled);
        app.run_command("tool.finish");
        assert!(app.status.contains("1.50 unit"));
        let extruded_winding = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap()
            .triangles[&source_face]
            .vertices;
        assert_eq!(source_winding[0], extruded_winding[0]);
        assert_eq!(source_winding[1], extruded_winding[2]);
        assert_eq!(source_winding[2], extruded_winding[1]);
        let before = app.document.clone();
        let history_count = app.history.entries().len();

        app.run_command("tool.builtin.push_pull");
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));

        assert_eq!(before, app.document);
        assert_eq!(history_count, app.history.entries().len());
        assert!(app.status.contains("too small"));
        assert_eq!("Push/Pull", app.active_tool);
    }

    #[test]
    fn push_pull_in_an_entered_instance_edits_its_definition_and_is_undoable() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "translated editable",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        let definition_id = app
            .document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let geometry_before = app.document.effective_geometry(&occurrence).unwrap();
        let triangle = *geometry_before.triangles.keys().next().unwrap();
        let triangle_count_before = geometry_before.triangles.len();
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        app.selection.apply(
            [SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(triangle),
            }],
            SelectionMode::Replace,
        );
        let before_extrusion = app.document.clone();
        let definition_geometry_before = before_extrusion
            .definition(definition_id)
            .unwrap()
            .geometry
            .clone();
        app.run_command("tool.builtin.push_pull");
        let reference = Vec3::new(11.0, 1.0, 1.0);
        let target = reference + Vec3::Y;
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(reference));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(target));

        let edited = app.document.definition(definition_id).unwrap();
        assert!(edited.geometry.triangles.len() > triangle_count_before);
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| { target.occurrence == occurrence })
        );
        assert!(
            app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .len()
                > triangle_count_before
        );

        app.run_command("edit.undo");
        assert_eq!(
            &definition_geometry_before,
            &app.document.definition(definition_id).unwrap().geometry
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn move_feedback_keeps_reference_line_with_phantom_in_move_and_copy_modes() {
        for copy in [false, true] {
            let mut app = test_app();
            app.run_command("geometry.add_cuboid");
            let before = app.document.clone();
            app.activate_tool("Move");
            app.transform_tool_state.set_copy_mode(copy);
            let origin = Vec3::new(7., 2., 5.);
            app.transform_tool_state.pick(origin);
            let target = origin + Vec3::new(4., 3., -2.);
            let (preview, _) = app.native_tool_feedback_at(target, Pos2::ZERO);
            let Some(ToolPreview::Transform { edges }) = preview else {
                panic!("move preview missing")
            };
            assert!(edges.contains(&[origin, target]));
            assert!(edges.len() > 1, "retain the selection phantom");
            assert_eq!(before, app.document, "preview must not modify the model");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn rotation_feedback_uses_center_for_measurements_and_retains_chosen_rays() {
        let mut app = test_app();
        let center = Vec3::new(7., 8., 9.);
        app.activate_tool("Rotate");
        for p in [center, center + Vec3::Y * 2., center + Vec3::X + Vec3::Y] {
            app.transform_tool_state.pick(p);
        }
        let target = center + Vec3::Z + Vec3::Y * 3.;
        assert_eq!(app.tool_reference_point(), Some(center));
        let (preview, feedback) = app.native_tool_feedback_at(target, Pos2::ZERO);
        let Some(ToolPreview::Transform { edges }) = preview else {
            panic!("rotation preview missing")
        };
        assert!(edges.contains(&[center, center + Vec3::Y * 2.]));
        assert!(edges.contains(&[center, center + Vec3::X + Vec3::Y]));
        assert!(edges.contains(&[center, target]));
        let text = feedback.unwrap().text;
        assert!(text.contains("Rotation -90.0°"), "{text}");
        assert!(text.contains("∠v1/v2"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn measurement_feedback_converts_coordinates_distances_and_radius_only_for_display() {
        let settings = ModelSettings {
            unit_name: "cm".into(),
            unit_size: 30.,
            grid_size: 0.5,
            ..Default::default()
        };
        let displacement = Vec3::new(0.5, 0., 0.);
        assert!(vector_feedback_text("Line", displacement, &settings).contains("15.000 cm"));
        assert!(point_feedback_text("Point", displacement, &settings).contains("X 15.000"));
        assert!(push_pull_feedback_text(displacement, &settings).contains("15.000 cm"));
        let primitive = primitive_feedback_text(
            PrimitiveKind::Cone,
            &[Vec3::ZERO, Vec3::X * 0.5],
            Vec3::Y * 2.,
            &settings,
        );
        assert!(primitive.contains("radius 15 cm"), "{primitive}");
        assert!(primitive.contains("60.000 cm"), "{primitive}");
        assert_eq!(Vec3::new(0.5, 0., 0.), displacement);
    }

    #[test]
    fn squash_command_scales_only_its_reference_axis_with_one_undo() {
        let mut app = test_app();
        let path = app
            .document
            .add_cuboid("squash", Vec3::new(4., 2., 2.), Vec3::ZERO)
            .unwrap();
        app.selection.apply(
            [SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Occurrence,
            }],
            SelectionMode::Replace,
        );
        let before = app.document.clone();
        app.run_command("tool.squash");
        assert_eq!(app.active_tool, "Squash");
        for p in [Vec3::ZERO, Vec3::X * 4., Vec3::X * 2.] {
            app.handle_native_world_pick(p, None, None);
        }
        let world = app
            .document
            .resolve_occurrence(&path)
            .unwrap()
            .world_transform;
        assert!(world.transform_vector(Vec3::X).distance(Vec3::X * 0.5) < 1e-9);
        assert!(world.transform_vector(Vec3::Y).distance(Vec3::Y) < 1e-9);
        app.run_command("edit.undo");
        assert_eq!(
            app.document
                .resolve_occurrence(&path)
                .unwrap()
                .world_transform,
            before.resolve_occurrence(&path).unwrap().world_transform
        );
        app.run_command("edit.redo");
        assert!(
            app.document
                .resolve_occurrence(&path)
                .unwrap()
                .world_transform
                .transform_vector(Vec3::X)
                .distance(Vec3::X * 0.5)
                < 1e-9
        );
    }

    #[test]
    fn push_pull_escape_cancels_without_changing_geometry_or_history() {
        let mut app = test_app();
        let (_, faces) = app
            .document
            .append_planar_mesh_to_scene_root(
                vec![Vec3::ZERO, Vec3::X, Vec3::Z],
                vec![[0, 1], [1, 2], [2, 0]],
                vec![[0, 1, 2]],
            )
            .unwrap();
        app.selection.apply(
            faces.into_iter().map(|face| SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::Triangle(face),
            }),
            SelectionMode::Replace,
        );
        let before = app.document.clone();
        let history_count = app.history.entries().len();
        app.run_command("tool.builtin.push_pull");
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.handle_escape();

        assert_eq!(before, app.document);
        assert_eq!(history_count, app.history.entries().len());
        assert_eq!("Select", app.active_tool);
        assert!(app.push_pull_reference.is_none());
    }

    #[test]
    fn point_input_feedback_reports_coordinates_and_signed_vectors_to_three_decimals() {
        assert_eq!(
            "Line: 0.500 meter | ΔX +0.500, ΔY +0.000, ΔZ +0.000 meter",
            vector_feedback_text(
                "Line",
                Vec3::new(0.5, 0.0, 0.0),
                &ModelSettings {
                    unit_name: "meter".into(),
                    ..Default::default()
                }
            )
        );
        assert_eq!(
            "Circle: X 1.235, Y -2.000, Z 0.000 meter",
            point_feedback_text(
                "Circle",
                Vec3::new(1.23456, -2.0, 0.0),
                &ModelSettings {
                    unit_name: "meter".into(),
                    ..Default::default()
                }
            )
        );

        let mut app = test_app();
        app.activate_tool("Line");
        app.drawing_tool.handle(ToolEvent::Pick(Vec3::ZERO));
        let snap = SnapResult {
            point: Vec3::X * 0.5,
            normal: Vec3::Y,
            kind: SnapKind::Endpoint,
            screen_distance: 0.0,
        };
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap));
        let Some(ToolPreview::Line { start, end }) = preview.as_ref() else {
            panic!("line tool should show a live endpoint vector");
        };
        assert_eq!(Vec3::ZERO, *start);
        assert_eq!(snap.point, *end);
        let feedback = app
            .tool_feedback(rect, Some(Pos2::ZERO), Some(snap), preview.as_ref())
            .expect("point-input tools display live measurement feedback");
        assert!(feedback.text.contains("0.500 unit"));
        assert!(feedback.text.contains("ΔX +0.500"));
    }

    #[test]
    fn every_transform_pick_stage_shows_a_point_vector_or_transformed_selection_preview() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let cases = [
            (TransformTool::Move, vec![Vec3::ZERO], Vec3::X * 2.0),
            (TransformTool::Stretch, vec![Vec3::ZERO], Vec3::X * 2.0),
            (
                TransformTool::Scale,
                vec![Vec3::ZERO, Vec3::X],
                Vec3::X * 2.0,
            ),
            (
                TransformTool::StretchScale,
                vec![Vec3::ZERO, Vec3::X],
                Vec3::X * 2.0,
            ),
            (
                TransformTool::Rotate,
                vec![Vec3::ZERO, Vec3::Y, Vec3::X],
                Vec3::Z,
            ),
            (TransformTool::Rotate2, vec![Vec3::ZERO, Vec3::X], Vec3::Y),
            (
                TransformTool::RotateStretch,
                vec![Vec3::ZERO, Vec3::Y, Vec3::X],
                Vec3::Z,
            ),
        ];

        for (tool, picks, pointer) in cases {
            for accepted in 0..=picks.len() {
                let mut app = test_app();
                let occurrence = app
                    .document
                    .add_cuboid("feedback target", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
                    .unwrap();
                app.set_selection([occurrence]);
                app.activate_tool(tool.label());
                for point in picks.iter().take(accepted) {
                    assert!(matches!(
                        app.transform_tool_state.pick(*point),
                        TransformPick::NeedPoint { .. }
                    ));
                }
                let snap = SnapResult {
                    point: pointer,
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                };
                let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap));
                if accepted == 0 {
                    assert!(
                        matches!(&preview, Some(ToolPreview::Point { .. })),
                        "{}",
                        tool.label()
                    );
                } else if accepted == picks.len() {
                    assert!(
                        matches!(&preview, Some(ToolPreview::Transform { .. })),
                        "{}",
                        tool.label()
                    );
                } else if matches!(
                    tool,
                    TransformTool::Rotate | TransformTool::Rotate2 | TransformTool::RotateStretch
                ) {
                    assert!(
                        matches!(&preview, Some(ToolPreview::Transform { edges }) if !edges.is_empty()),
                        "{}",
                        tool.label()
                    );
                } else {
                    assert!(
                        matches!(&preview, Some(ToolPreview::Line { .. })),
                        "{}",
                        tool.label()
                    );
                }
                let feedback = app
                    .tool_feedback(rect, Some(Pos2::ZERO), Some(snap), preview.as_ref())
                    .expect("transform stages report pointer feedback");
                if accepted == 0 {
                    assert!(feedback.text.contains("X "), "{}", tool.label());
                } else {
                    assert!(feedback.text.contains("ΔX "), "{}", tool.label());
                }
            }
        }
    }

    #[test]
    fn construction_tools_show_feedback_before_and_after_their_first_pick() {
        fn snap(point: Vec3) -> SnapResult {
            SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }
        }

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let start = Vec3::ZERO;
        let end = Vec3::new(2.0, 0.0, 1.0);

        for tool in ["Rectangle", "Surface Rect", "Quad", "Circle"] {
            let mut app = test_app();
            app.activate_tool(tool);
            let first_preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(start)));
            assert!(
                matches!(
                    &first_preview,
                    Some(ToolPreview::Point { position }) if *position == start
                ),
                "{tool} should show its first snapped point"
            );
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(start)), Some(start));
            let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(end)));
            assert!(
                matches!(&preview, Some(ToolPreview::PlanarShape { .. })),
                "{tool} should generate geometry preview after its reference pick"
            );
            let feedback = app
                .tool_feedback(rect, Some(Pos2::ZERO), Some(snap(end)), preview.as_ref())
                .expect("planar tools display measurement feedback");
            assert!(feedback.text.contains("ΔX +2.000"), "{tool}");
        }

        for tool in ["Polyline", "Double Line"] {
            let mut app = test_app();
            app.activate_tool(tool);
            let first_preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(start)));
            assert!(
                matches!(&first_preview, Some(ToolPreview::Point { .. })),
                "{tool} should show its first snapped point"
            );
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(start)), Some(start));
            let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(end)));
            if tool == "Polyline" {
                assert!(matches!(&preview, Some(ToolPreview::Polyline { .. })));
            } else {
                assert!(matches!(&preview, Some(ToolPreview::Transform { .. })));
            }
            let feedback = app
                .tool_feedback(rect, Some(Pos2::ZERO), Some(snap(end)), preview.as_ref())
                .expect("path tools display their current segment measurement");
            assert!(feedback.text.contains("ΔX +2.000"), "{tool}");
        }

        let mut mesh = test_app();
        mesh.activate_tool("Mesh");
        let first_preview = mesh.tool_preview(rect, Some(Pos2::ZERO), Some(snap(start)));
        assert!(matches!(&first_preview, Some(ToolPreview::Point { .. })));
        mesh.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(start)), Some(start));
        mesh.mesh_tool_state.set_pointer(Some(end));
        let preview = mesh.tool_preview(rect, Some(Pos2::ZERO), Some(snap(end)));
        assert!(matches!(&preview, Some(ToolPreview::Transform { .. })));
        assert!(
            mesh.tool_feedback(rect, Some(Pos2::ZERO), Some(snap(end)), preview.as_ref())
                .unwrap()
                .text
                .contains("ΔX +2.000")
        );

        let mut cuboid = test_app();
        cuboid.activate_tool("Cuboid");
        cuboid.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(start)), Some(start));
        let preview = cuboid.tool_preview(rect, Some(Pos2::ZERO), Some(snap(end)));
        assert!(matches!(&preview, Some(ToolPreview::Line { .. })));
        assert!(
            cuboid
                .tool_feedback(rect, Some(Pos2::ZERO), Some(snap(end)), preview.as_ref())
                .unwrap()
                .text
                .contains("ΔX +2.000")
        );
    }

    #[test]
    fn line_offset_command_offsets_selected_edges_and_creates_strip_faces() {
        let mut app = test_app();
        assert!(!app.query_command_state("tool.builtin.line_offset").enabled);

        let first = app
            .create_line_in_active_context(Vec3::ZERO, Vec3::X * 2.0)
            .unwrap();
        app.selection.apply(first, SelectionMode::Replace);
        let second = app
            .create_line_in_active_context(Vec3::X * 2.0, Vec3::X * 2.0 + Vec3::Z * 2.0)
            .unwrap();
        app.selection.apply(second, SelectionMode::Add);
        assert_eq!(2, app.selection.summary().edges);
        assert!(app.query_command_state("tool.builtin.line_offset").enabled);

        let activation =
            app.dispatch_command("tool.builtin.line_offset", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Line Offset", app.active_tool);
        assert!(app.line_offset_tool_state.active());
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::X));
        assert_eq!(Some(Vec3::X), app.line_offset_tool_state.reference());
        assert_eq!("Line Offset: pick target point.", app.status);
        app.line_offset_tool_state
            .set_pointer(Some(Vec3::X + Vec3::Z));
        let Some(ToolPreview::LineOffset {
            edges,
            signed_offset,
            target,
        }) = app.tool_preview(rect, Some(Pos2::ZERO), None)
        else {
            panic!("Line Offset should preview the source, offset and connector edges");
        };
        assert_eq!(8, edges.len());
        assert_eq!(Some(1.0), signed_offset);
        assert_eq!(Vec3::X + Vec3::Z, target);

        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::X + Vec3::Z));

        assert_eq!("Offset copy | edges 2 faces 4", app.status);
        assert_eq!(3, app.history.entries().len());
        assert_eq!(2, app.selection.summary().edges);
        assert_eq!(4, app.selection.summary().faces);
        assert!(app.line_offset_tool_state.active());
        assert_eq!(None, app.line_offset_tool_state.reference());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.segments.len());
        assert_eq!(4, geometry.triangles.len());
        let selected_edge_points = app
            .selection
            .selected()
            .iter()
            .filter_map(|target| {
                let SelectedEntity::Segment(segment_id) = target.entity else {
                    return None;
                };
                let segment = geometry.segments.get(&segment_id)?;
                Some(
                    segment
                        .endpoints
                        .map(|reference| geometry.vertex_position(reference))
                        .map(|point| point.unwrap()),
                )
            })
            .flatten()
            .collect::<Vec<_>>();
        assert!(selected_edge_points.iter().all(|point| point.z >= 1.0));
    }

    #[test]
    fn line_offset_rejects_no_selection_and_escape_clears_its_reference() {
        let mut app = test_app();
        app.run_command("tool.cycle.operations");
        assert_eq!("Line Offset", app.active_tool);
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::ZERO));
        assert_eq!("No edges selected.", app.status);
        assert_eq!(None, app.line_offset_tool_state.reference());

        let line = app
            .create_line_in_active_context(Vec3::ZERO, Vec3::X)
            .unwrap();
        app.selection.apply(line, SelectionMode::Replace);
        app.run_command("tool.builtin.line_offset");
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::ZERO));
        assert_eq!(Some(Vec3::ZERO), app.line_offset_tool_state.reference());
        app.handle_escape();
        assert_eq!("Select", app.active_tool);
        assert!(!app.line_offset_tool_state.active());
        assert_eq!(None, app.line_offset_tool_state.reference());
    }

    #[test]
    fn line_offset_rejection_preserves_the_selected_source_geometry() {
        let mut app = test_app();
        let first = app
            .create_line_in_active_context(Vec3::ZERO, Vec3::X * 2.0)
            .unwrap();
        app.selection.apply(first, SelectionMode::Replace);
        let second = app
            .create_line_in_active_context(Vec3::X * 2.0, Vec3::X * 2.0 + Vec3::Z * 2.0)
            .unwrap();
        app.selection.apply(second, SelectionMode::Add);
        let original_selection = app.selection.selected().clone();
        let revision = app.document.revision();
        let history_len = app.history.entries().len();

        app.run_command("tool.builtin.line_offset");
        app.handle_viewport_click_at_point(Rect::NOTHING, Pos2::ZERO, None, Some(Vec3::X));
        app.handle_viewport_click_at_point(
            Rect::NOTHING,
            Pos2::ZERO,
            None,
            Some(Vec3::X + Vec3::Z * 2.0),
        );

        assert_eq!(
            "Line Offset collapses a selected edge at this distance.",
            app.status
        );
        assert_eq!(revision, app.document.revision());
        assert_eq!(history_len, app.history.entries().len());
        assert_eq!(&original_selection, app.selection.selected());
        assert_eq!("Line Offset", app.active_tool);
        assert_eq!(None, app.line_offset_tool_state.reference());
    }

    #[test]
    fn line_offset_in_an_entered_instance_commits_to_its_definition_in_local_space() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "translated editable",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        let first = app
            .create_line_in_active_context(Vec3::new(10.0, 4.0, 0.0), Vec3::new(12.0, 4.0, 0.0))
            .unwrap();
        app.selection.apply(first, SelectionMode::Replace);
        let second = app
            .create_line_in_active_context(Vec3::new(12.0, 4.0, 0.0), Vec3::new(12.0, 4.0, 2.0))
            .unwrap();
        app.selection.apply(second, SelectionMode::Add);

        app.run_command("tool.builtin.line_offset");
        app.handle_viewport_click_at_point(
            Rect::NOTHING,
            Pos2::ZERO,
            None,
            Some(Vec3::new(11.0, 4.0, 0.0)),
        );
        app.handle_viewport_click_at_point(
            Rect::NOTHING,
            Pos2::ZERO,
            None,
            Some(Vec3::new(11.0, 4.0, 1.0)),
        );

        assert_eq!("Offset copy | edges 2 faces 4", app.status);
        let resolved = app.document.resolve_occurrence(&occurrence).unwrap();
        assert_eq!(definition, resolved.definition.id);
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(16, geometry.segments.len());
        assert_eq!(16, geometry.triangles.len());
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| { target.occurrence == occurrence })
        );
        for target in app.selection.selected() {
            let SelectedEntity::Segment(segment_id) = target.entity else {
                continue;
            };
            let segment = &geometry.segments[&segment_id];
            let world_points = segment.endpoints.map(|reference| {
                let local = geometry.vertex_position(reference).unwrap();
                resolved.world_transform.transform_point(local)
            });
            assert!(world_points.iter().all(|point| point.z >= 1.0));
        }
    }

    #[test]
    fn polyline_settings_are_validated_and_round_trip_with_the_document() {
        let mut app = test_app();
        assert_eq!(0.5, app.document.model_settings().arc_max_length);
        assert_eq!(1.0, app.document.model_settings().double_line_size);
        assert_eq!(0.5, app.document.model_settings().double_line_offset);
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("arc_max_length".to_owned(), json!(0.25));
        payload
            .values
            .insert("double_line_size".to_owned(), json!(2.0));
        payload
            .values
            .insert("double_line_offset".to_owned(), json!(-0.75));
        let result = app.dispatch_command("model.update_settings", &payload);
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!(0.25, app.document.model_settings().arc_max_length);
        assert_eq!(2.0, app.document.model_settings().double_line_size);
        assert_eq!(-0.75, app.document.model_settings().double_line_offset);
        assert_eq!(1, app.history.entries().len());
        let encoded = serde_json::to_vec(&app.document).unwrap();
        let reopened: Document = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(app.document.model_settings(), reopened.model_settings());

        let revision = app.document.revision();
        let mut invalid = CommandPayload::default();
        invalid
            .values
            .insert("arc_max_length".to_owned(), json!(0.0));
        let result = app.dispatch_command("model.update_settings", &invalid);
        assert_eq!(CommandCompletion::Failed, result.result.completion);
        assert_eq!(revision, app.document.revision());
    }

    #[test]
    fn native_enter_commits_point_arrays_and_escape_preserves_the_result() {
        for tool in ["Polyline", "Double Line"] {
            let mut app = test_app();
            app.activate_tool(tool);
            let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
            for point in [Vec3::ZERO, Vec3::X * 3., Vec3::new(3., 0., 2.)] {
                app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
            }
            assert!(app.handle_native_tool_key("enter", false));
            assert!(app.polyline_tool_state.points().is_empty());
            assert_eq!(1, app.history.entries().len(), "{tool} commits once");
            let committed = app.document.clone();
            app.handle_native_escape();
            assert_eq!(
                committed, app.document,
                "Escape cannot remove committed {tool}"
            );
        }
    }

    #[test]
    fn path_segment_letters_do_not_cycle_any_path_tool_and_shift_l_explicitly_cycles() {
        for tool in [
            "Polyline",
            "Double Line",
            "Extrude Swipe",
            "Extrude Swipe 3D",
            "Ribbon 3D",
        ] {
            let mut app = test_app();
            app.activate_tool(tool);
            app.polyline_tool_state.pick(Vec3::ZERO, 0.25).unwrap();
            for (key, mode) in [
                (
                    "a",
                    meshcraft_core::tools::paths::PolylineSegmentMode::ArcThrough,
                ),
                (
                    "c",
                    meshcraft_core::tools::paths::PolylineSegmentMode::ArcCenter,
                ),
                ("l", meshcraft_core::tools::paths::PolylineSegmentMode::Line),
            ] {
                assert!(app.handle_native_tool_key(key, false));
                assert_eq!(tool, app.active_tool, "{key} switched out of {tool}");
                assert_eq!(mode, app.polyline_tool_state.mode());
                assert_eq!(1, app.polyline_tool_state.points().len());
                assert!(!app.tool_cycle_state.hotspot_placement_active());
            }
            assert!(app.handle_native_tool_key("l", true));
            assert!(app.tool_cycle_state.hotspot_placement_active() || app.active_tool != tool);
        }
    }

    #[test]
    fn legacy_double_line_l_switches_segment_mode_without_cycling() {
        let mut app = test_app();
        let context = egui::Context::default();
        app.activate_tool("Double Line");
        app.polyline_tool_state
            .set_mode(meshcraft_core::tools::paths::PolylineSegmentMode::ArcThrough);
        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: Key::L,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
        assert_eq!("Double Line", app.active_tool);
        assert_eq!(
            meshcraft_core::tools::paths::PolylineSegmentMode::Line,
            app.polyline_tool_state.mode()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual insertion stage probe; MESHCRAFT_BENCH_PROJECT required"]
    fn insertion_latency_probe() {
        use super::{LineOwner, PlanarMesh, SweepProfileSegment, modeling_sweep};
        use meshcraft_core::{
            document::architecture::{Element, Slab, Wall},
            tools::paths::PolylinePath,
        };
        use std::time::Instant;
        fn root_definition(d: &Document) -> meshcraft_core::document::DefinitionId {
            let root = d
                .scene_container_occurrence()
                .unwrap_or_else(OccurrenceRef::scene_root);
            d.resolve_occurrence(&root).unwrap().definition.id
        }
        let path = std::env::var("MESHCRAFT_BENCH_PROJECT").unwrap();
        let start = Instant::now();
        let document = read_project(&std::fs::read(path).unwrap())
            .unwrap()
            .document;
        eprintln!("load {:?}", start.elapsed());
        let root = document
            .scene_container_occurrence()
            .unwrap_or_else(OccurrenceRef::scene_root);
        let g = document.effective_geometry(&root).unwrap();
        let bounds =
            meshcraft_core::geometry::Aabb3::from_points(g.vertices.values().map(|v| v.position))
                .unwrap();
        eprintln!(
            "root {:?}, {} vertices / {} edges / {} faces, bounds {bounds:?}",
            root.path,
            g.vertices.len(),
            g.segments.len(),
            g.triangles.len()
        );
        let far = bounds.max + Vec3::new(10., 0., 10.);
        let far = Vec3::new(far.x, 0., far.z);
        let rectangle = || PlanarMesh {
            vertices: vec![
                far,
                far + Vec3::X * 5.,
                far + (Vec3::X + Vec3::Z) * 5.,
                far + Vec3::Z * 5.,
            ],
            segments: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
        };
        let only = std::env::var("MESHCRAFT_INSERT_CASE").ok();
        for label in [
            "line",
            "rectangle",
            "extrude",
            "circle-far",
            "circle-model",
            "cuboid",
            "wall",
            "slab",
            "instance",
            "sweep",
        ] {
            if only.as_ref().is_some_and(|v| v != label) {
                continue;
            }
            let mut app = test_app();
            app.document = document.clone();
            if label == "extrude" {
                let targets = app
                    .create_planar_shape_in_active_context(rectangle(), "Rectangle")
                    .unwrap();
                app.selection.apply(targets, SelectionMode::Replace);
                app.pick_push_pull_point(far);
            }
            let before = app.document.clone();
            let start = Instant::now();
            match label {
                "rectangle" => {
                    app.create_planar_shape_in_active_context(rectangle(), "Rectangle")
                        .unwrap();
                }
                "extrude" => {
                    app.commit_push_pull(Vec3::Y * 3.).unwrap();
                }
                "line" => {
                    app.create_line_in_active_context(far, far + Vec3::X * 3.)
                        .unwrap();
                }
                "circle-far" | "circle-model" => {
                    let center = if label == "circle-far" {
                        far
                    } else {
                        Vec3::new(30., 0., 30.)
                    };
                    let mesh = meshcraft_core::tools::shapes::circle_mesh(
                        center,
                        center + Vec3::X * 3.,
                        Vec3::Y,
                        24,
                    )
                    .unwrap();
                    app.create_planar_shape_in_active_context(mesh, "Circle")
                        .unwrap();
                }
                "cuboid" => {
                    app.create_cuboid_in_active_context(CuboidPreview {
                        origin: far,
                        dimensions: Vec3::new(2., 3., 2.),
                    })
                    .unwrap();
                }
                "wall" => {
                    app.commit_document_edit("Wall", |d| {
                        d.put_architecture(
                            root_definition(d),
                            None,
                            Element::Wall(Wall {
                                start: far,
                                end: far + Vec3::X * 5.,
                                ..Default::default()
                            }),
                        )
                    })
                    .unwrap();
                }
                "slab" => {
                    app.commit_document_edit("Slab", |d| {
                        d.put_architecture(
                            root_definition(d),
                            None,
                            Element::Slab(Slab {
                                profile: PolylinePath {
                                    points: vec![
                                        far,
                                        far + Vec3::X * 5.,
                                        far + (Vec3::X + Vec3::Z) * 5.,
                                        far + Vec3::Z * 5.,
                                    ],
                                    closed: true,
                                },
                                ..Default::default()
                            }),
                        )
                    })
                    .unwrap();
                }
                "sweep" => {
                    let profile = (0..4)
                        .map(|i| {
                            let p = [Vec3::ZERO, Vec3::Y * 3., Vec3::Y * 3. + Vec3::Z, Vec3::Z];
                            SweepProfileSegment {
                                endpoints: [far + p[i], far + p[(i + 1) % 4]],
                            }
                        })
                        .collect::<Vec<_>>();
                    let path = (0..=125)
                        .map(|i| {
                            let angle = f64::from(i) * std::f64::consts::PI / 125.;
                            far + Vec3::new(angle.sin() * 20., 0., (1. - angle.cos()) * 20.)
                        })
                        .collect::<Vec<_>>();
                    let mesh = modeling_sweep::extrude_swipe(&profile, &path, false, Vec3::Y, 1e-9)
                        .unwrap();
                    eprintln!(
                        "sweep incoming: {} vertices, {} edges, {} faces",
                        mesh.vertices.len(),
                        mesh.segments.len(),
                        mesh.triangles.len()
                    );
                    let result = app.commit_sweep_mesh(
                        LineOwner::Definition(root_definition(&app.document)),
                        root.clone(),
                        mesh,
                        "Sweep",
                    );
                    assert_eq!(
                        CommandCompletion::Completed,
                        result.completion,
                        "{result:?}"
                    );
                }
                "instance" => {
                    let def = document
                        .occurrence_refs()
                        .unwrap()
                        .into_iter()
                        .find(|p| document.is_object_occurrence(p))
                        .and_then(|p| document.resolve_occurrence(&p).map(|r| r.definition.id))
                        .unwrap();
                    app.commit_document_edit("Place object", |d| {
                        meshcraft_core::operations::objects::place_definition(
                            d,
                            &EditContext::Document,
                            def,
                            far,
                            None,
                            false,
                        )
                    })
                    .unwrap();
                }
                _ => unreachable!(),
            }
            eprintln!("{label}: commit {:?}", start.elapsed());
            let start = Instant::now();
            app.native_snap_point_on_ray(far + Vec3::Y * 10., -Vec3::Y, 0.1);
            eprintln!("{label}: spatial {:?}", start.elapsed());
            let authored = |d: &Document| {
                let mut value = serde_json::to_value(d).unwrap();
                value.as_object_mut().unwrap().remove("revision");
                value
            };
            let after = authored(&app.document);
            app.history.undo(&mut app.document).unwrap();
            assert_eq!(authored(&before), authored(&app.document), "{label} undo");
            app.history.redo(&mut app.document).unwrap();
            assert_eq!(after, authored(&app.document), "{label} redo");
            app.document.validate().unwrap();
        }
    }

    #[test]
    #[ignore = "manual latency probe; optional MESHCRAFT_BENCH_PROJECT input"]
    fn circle_construction_latency_probe() {
        let mut app = test_app();
        if let Ok(path) = std::env::var("MESHCRAFT_BENCH_PROJECT") {
            app.document = meshcraft_io::project::read_project(&std::fs::read(path).unwrap())
                .unwrap()
                .document;
        }
        let mesh = meshcraft_core::tools::shapes::circle_mesh(
            Vec3::new(30., 0., 30.),
            Vec3::new(33., 0., 30.),
            Vec3::Y,
            24,
        )
        .unwrap();
        let start = std::time::Instant::now();
        app.create_planar_shape_in_active_context(mesh, "Circle")
            .unwrap();
        println!(
            "circle commit: {:?}; definitions: {}",
            start.elapsed(),
            app.document.definitions().count()
        );
        let start = std::time::Instant::now();
        app.native_snap_point_on_ray(Vec3::new(30., 10., 30.), -Vec3::Y, 0.1);
        println!("post-commit spatial update: {:?}", start.elapsed());
    }

    fn dense_prototype_fixture() -> (MeshcraftApp, OccurrenceRef) {
        use meshcraft_core::document::{RawGeometryTriangle, RawGeometryVertex, RawVertexRef};
        let mut app = test_app();
        let path = app
            .document
            .add_cuboid("dense prototype", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&path)
            .unwrap()
            .definition
            .id;
        let side = 160;
        let vertices = (0..=side)
            .flat_map(|z| {
                (0..=side).map(move |x| RawGeometryVertex {
                    position: Vec3::new(x as f64, 0., z as f64),
                    legacy_aliases: vec![],
                })
            })
            .collect();
        let mut triangles = vec![];
        for z in 0..side {
            for x in 0..side {
                let a = (z * (side + 1) + x) as usize;
                let b = a + 1;
                let c = a + (side + 1) as usize;
                for indices in [[a, c, b], [b, c, c + 1]] {
                    triangles.push(RawGeometryTriangle {
                        vertices: indices.map(|vertex| RawVertexRef {
                            vertex,
                            position_offset: Vec3::ZERO,
                        }),
                        color: Rgba8::WHITE,
                        legacy_aliases: vec![],
                        extensions: Default::default(),
                    });
                }
            }
        }
        app.document
            .replace_definition_geometry_raw(definition, vertices, vec![], triangles)
            .unwrap();
        (app, path)
    }

    #[test]
    #[ignore = "manual prototype picking latency probe; no timing threshold"]
    fn prototype_picking_latency_probe() {
        let (mut app, path) = dense_prototype_fixture();
        app.selection.set_context(EditContext::Occurrence(path));
        let origin = Vec3::new(30.25, 10., 30.4);
        let start = std::time::Instant::now();
        app.native_snap_point_on_ray(origin, -Vec3::Y, 0.01)
            .unwrap();
        println!(
            "51,200-face prototype cold spatial query: {:?}",
            start.elapsed()
        );
        let revision = app.document.revision();
        let start = std::time::Instant::now();
        for i in 0..20 {
            let snap = app
                .native_snap_point_on_ray(origin, -Vec3::Y, 0.01)
                .unwrap();
            app.native_cursor_axes(snap).unwrap();
            let target = app.pick_native_target(origin, -Vec3::Y, 0.01).unwrap();
            assert!(matches!(target.entity, SelectedEntity::Triangle(_)));
            app.handle_native_selection_click(
                Some(target),
                Modifiers::default(),
                1000 * (i + 1),
                meshcraft_core::geometry::Vec2::ZERO,
            );
            app.native_cursor_axes(snap).unwrap();
            for descriptor in MeshcraftApp::command_catalog() {
                std::hint::black_box(app.query_command_state(descriptor.id.as_str()));
            }
        }
        println!(
            "20 warm snap/axes/pick/select/command-state cycles: {:?}",
            start.elapsed()
        );
        assert_eq!(app.document.revision(), revision);
    }

    #[test]
    #[ignore = "manual placement preview latency probe; no timing threshold"]
    fn object_placement_preview_latency_probe() {
        let (mut app, path) = dense_prototype_fixture();
        let definition = app
            .document
            .resolve_occurrence(&path)
            .unwrap()
            .definition
            .id;
        app.begin_placing_definition(definition);
        let revision = app.document.revision();
        let start = std::time::Instant::now();
        assert!(
            app.native_tool_feedback_at(Vec3::new(200., 0., 0.), Pos2::ZERO)
                .0
                .is_some()
        );
        println!("51,200-face placement first preview: {:?}", start.elapsed());
        let start = std::time::Instant::now();
        for i in 0..20 {
            std::hint::black_box(
                app.native_tool_feedback_at(Vec3::new(200. + f64::from(i), 0., 0.), Pos2::ZERO),
            );
        }
        println!("20 warm placement preview moves: {:?}", start.elapsed());
        assert_eq!(revision, app.document.revision());
    }

    #[test]
    fn double_line_command_creates_and_selects_a_ribbon_from_a_point_path() {
        let mut app = test_app();
        let activated =
            app.dispatch_command("tool.builtin.double_line", &CommandPayload::default());
        assert!(activated.accepted);
        assert_eq!("Double Line", app.active_tool);
        assert!(app.polyline_tool_state.active());

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [Vec3::ZERO, Vec3::X * 3.0] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
        }
        assert!(app.query_command_state("tool.finish").enabled);
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);

        let occurrence = app
            .selection
            .selected()
            .iter()
            .next()
            .unwrap()
            .occurrence
            .clone();
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert_eq!(4, app.selection.summary().edges);
        assert_eq!(2, app.selection.summary().faces);
        assert_eq!(1, app.history.entries().len());

        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert!(geometry.segments.is_empty());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn extrude_swipe_sweeps_selected_profile_edges_and_is_undoable() {
        let mut app = test_app();
        assert!(
            !app.query_command_state("tool.builtin.extrude_swipe")
                .enabled
        );
        let profile = app
            .create_line_in_active_context(Vec3::ZERO, Vec3::Y)
            .unwrap();
        app.selection.apply(profile, SelectionMode::Replace);
        assert!(
            app.query_command_state("tool.builtin.extrude_swipe")
                .enabled
        );
        let activated =
            app.dispatch_command("tool.builtin.extrude_swipe", &CommandPayload::default());
        assert!(activated.accepted, "{}", app.status);
        assert_eq!("Extrude Swipe", app.active_tool);
        assert!(app.polyline_tool_state.active());

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        let end = Vec3::X * 2.0;
        let end_screen = app.viewport.world_to_screen(rect, end);
        let preview = app.tool_preview(rect, Some(end_screen), None);
        assert!(matches!(preview, Some(ToolPreview::Primitive { .. })));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(end));
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!("Select", app.active_tool);

        let occurrence = OccurrenceRef::scene_root();
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert_eq!(4, app.selection.summary().edges);
        assert_eq!(2, app.selection.summary().faces);
        assert_eq!(2, app.history.entries().len());

        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(1, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn extrude_swipe_commits_in_entered_instance_local_space() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "editable profile",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        let profile = app
            .create_line_in_active_context(Vec3::new(10.0, 4.0, 0.0), Vec3::new(10.0, 5.0, 0.0))
            .unwrap();
        app.selection.apply(profile, SelectionMode::Replace);
        let before = app.document.effective_geometry(&occurrence).unwrap();
        let before_segments = before.segments.len();
        let before_triangles = before.triangles.len();
        app.run_command("tool.builtin.extrude_swipe");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [Vec3::new(10.0, 4.0, 0.0), Vec3::new(12.0, 4.0, 0.0)] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
        }
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);

        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(before_segments + 3, geometry.segments.len());
        assert_eq!(before_triangles + 2, geometry.triangles.len());
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| { target.occurrence == occurrence })
        );
        let resolved = app.document.resolve_occurrence(&occurrence).unwrap();
        let swept_points = app
            .selection
            .selected()
            .iter()
            .filter_map(|target| {
                let SelectedEntity::Triangle(id) = target.entity else {
                    return None;
                };
                let triangle = geometry.triangles.get(&id)?;
                Some(triangle.vertices.map(|reference| {
                    resolved
                        .world_transform
                        .transform_point(geometry.vertex_position(reference).unwrap())
                }))
            })
            .flatten()
            .collect::<Vec<_>>();
        assert!(swept_points.iter().all(|point| point.x >= 10.0 - 1.0e-9));
        assert!(swept_points.iter().all(|point| point.x <= 12.0 + 1.0e-9));
    }

    #[test]
    fn extrude_swipe_3d_sweeps_selected_edges_along_a_spatial_path() {
        fn snap(point: Vec3) -> SnapResult {
            SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }
        }

        let mut app = test_app();
        assert!(
            !app.query_command_state("tool.builtin.extrude_swipe_3d")
                .enabled
        );
        let profile = app
            .create_line_in_active_context(Vec3::ZERO, Vec3::Y)
            .unwrap();
        app.selection.apply(profile, SelectionMode::Replace);
        let activation =
            app.dispatch_command("tool.builtin.extrude_swipe_3d", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Extrude Swipe 3D", app.active_tool);
        assert!(app.polyline_tool_state.active());

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(
            rect,
            Pos2::ZERO,
            Some(snap(Vec3::ZERO)),
            Some(Vec3::ZERO),
        );
        let first_span_preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(Vec3::X)));
        assert!(matches!(
            first_span_preview,
            Some(ToolPreview::Primitive { .. })
        ));
        for point in [Vec3::X, Vec3::X + Vec3::Y + Vec3::Z] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(point)), Some(point));
        }
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!("Select", app.active_tool);

        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(7, geometry.segments.len());
        assert_eq!(4, geometry.triangles.len());
        assert_eq!(4, app.selection.summary().faces);
        assert_eq!(2, app.history.entries().len());
        app.run_command("edit.undo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(1, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn ribbon_3d_uses_model_width_and_offset_and_is_undoable() {
        fn snap(point: Vec3) -> SnapResult {
            SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }
        }

        let mut app = test_app();
        let mut settings = CommandPayload::default();
        settings
            .values
            .insert("double_line_size".to_owned(), json!(2.0));
        settings
            .values
            .insert("double_line_offset".to_owned(), json!(3.0));
        assert_eq!(
            CommandCompletion::Completed,
            app.dispatch_command("model.update_settings", &settings)
                .result
                .completion
        );
        let activation = app.dispatch_command("tool.builtin.ribbon_3d", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Ribbon 3D", app.active_tool);

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [Vec3::ZERO, Vec3::X * 2.0] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(point)), Some(point));
        }
        let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(Vec3::X * 3.0)));
        assert!(matches!(preview, Some(ToolPreview::Primitive { .. })));
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!("Select", app.active_tool);

        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert!(
            geometry
                .triangles
                .values()
                .all(|triangle| triangle.color == Rgba8::ANSI_GREY)
        );
        assert_eq!(2, app.history.entries().len());
        app.run_command("edit.undo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert!(geometry.segments.is_empty());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn revolve_command_defaults_to_full_turn_and_commits_selected_closed_profile() {
        fn snap(point: Vec3) -> SnapResult {
            SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }
        }

        let mut app = test_app();
        assert!(!app.query_command_state("tool.builtin.revolve").enabled);
        let mut profile = Vec::new();
        for [start, end] in [
            [Vec3::ZERO, Vec3::X],
            [Vec3::X, Vec3::X + Vec3::Y],
            [Vec3::X + Vec3::Y, Vec3::Y],
            [Vec3::Y, Vec3::ZERO],
        ] {
            profile.extend(app.create_line_in_active_context(start, end).unwrap());
        }
        app.selection.apply(profile, SelectionMode::Replace);
        assert!(app.query_command_state("tool.builtin.revolve").enabled);
        let activation = app.dispatch_command("tool.builtin.revolve", &CommandPayload::default());
        assert!(activation.accepted, "{}", app.status);
        assert_eq!("Revolve", app.active_tool);

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [Vec3::ZERO, Vec3::Y] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(point)), Some(point));
        }
        assert!(app.query_command_state("tool.finish").enabled);
        let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap(Vec3::X * 2.0)));
        assert!(matches!(preview, Some(ToolPreview::Primitive { .. })));
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!("Select", app.active_tool);

        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(96, geometry.triangles.len());
        assert_eq!(96, app.selection.summary().faces);
        assert!(
            geometry
                .triangles
                .values()
                .all(|triangle| triangle.color == Rgba8::ANSI_GREY)
        );
        assert_eq!(5, app.history.entries().len());

        app.run_command("edit.undo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn revolve_accepts_typed_signed_angle_through_the_workspace_input() {
        fn snap(point: Vec3) -> SnapResult {
            SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            }
        }

        let mut app = test_app();
        let profile = app
            .create_line_in_active_context(Vec3::X, Vec3::X + Vec3::Y)
            .unwrap();
        app.selection.apply(profile, SelectionMode::Replace);
        app.run_command("tool.builtin.revolve");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        for point in [Vec3::ZERO, Vec3::Y] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap(point)), Some(point));
        }
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());
        app.run_command("tool.distance");
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::Text("-90".to_owned()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );

        assert_eq!("Select", app.active_tool);
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(48, geometry.triangles.len());
        let swept_points = geometry
            .triangles
            .values()
            .flat_map(|triangle| triangle.vertices)
            .filter_map(|vertex| geometry.vertex_position(vertex))
            .collect::<Vec<_>>();
        assert!(swept_points.iter().any(|point| point.z > 0.99));
        assert_eq!(2, app.history.entries().len());
    }

    #[test]
    fn face_outline_command_uses_selected_faces_and_omits_the_interior_diagonal() {
        let mut app = test_app();
        let (_, face_ids) = app
            .document
            .append_planar_mesh_to_scene_root(
                vec![
                    Vec3::ZERO,
                    Vec3::X * 2.0,
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::Z * 2.0,
                ],
                vec![[0, 1]],
                vec![[0, 1, 2], [0, 2, 3]],
            )
            .unwrap();
        let occurrence = OccurrenceRef::scene_root();
        app.selection.apply(
            face_ids.into_iter().map(|face| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(face),
            }),
            SelectionMode::Replace,
        );
        assert!(app.query_command_state("tool.builtin.face_outline").enabled);

        let dispatch =
            app.dispatch_command("tool.builtin.face_outline", &CommandPayload::default());

        assert!(dispatch.accepted, "{}", app.status);
        assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
        assert_eq!(
            json!({"edges": 4, "faces": 2}),
            dispatch.result.outputs["outlined"]
        );
        assert_eq!(4, app.selection.summary().edges);
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| matches!(target.entity, SelectedEntity::Segment(_)))
        );
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(4, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert_eq!(1, app.history.entries().len());

        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(1, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
    }

    #[test]
    fn face_outline_command_is_disabled_without_selected_faces() {
        let app = test_app();

        let state = app.query_command_state("tool.builtin.face_outline");

        assert!(!state.enabled);
        assert_eq!(
            "select one or more faces first",
            state.disabled_reason.as_deref().unwrap()
        );
    }

    #[test]
    fn polyline_mesh_command_fills_selected_segments_without_adding_edges() {
        let mut app = test_app();
        let points = [
            Vec3::ZERO,
            Vec3::X * 2.0,
            Vec3::new(2.0, 0.0, 2.0),
            Vec3::Z * 2.0,
        ];
        for [start, end] in [[0, 1], [1, 2], [2, 3]] {
            app.document
                .append_line_segment_to_scene_root(points[start], points[end])
                .unwrap();
        }
        let occurrence = OccurrenceRef::scene_root();
        let segment_ids = app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .segments
            .keys()
            .copied()
            .collect::<Vec<_>>();
        app.selection.apply(
            segment_ids.into_iter().map(|segment| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(segment),
            }),
            SelectionMode::Replace,
        );
        assert!(
            app.query_command_state("tool.builtin.polyline_mesh")
                .enabled
        );

        let dispatch =
            app.dispatch_command("tool.builtin.polyline_mesh", &CommandPayload::default());

        assert!(dispatch.accepted, "{}", app.status);
        assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
        assert_eq!(
            json!({"faces": 2, "segments": 3}),
            dispatch.result.outputs["polyline_mesh"]
        );
        assert_eq!(2, app.selection.summary().faces);
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(3, geometry.segments.len());
        assert_eq!(2, geometry.triangles.len());
        assert_eq!(1, app.history.entries().len());

        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(3, geometry.segments.len());
        assert!(geometry.triangles.is_empty());
    }

    #[test]
    fn polyline_mesh_command_is_disabled_without_selected_segments() {
        let app = test_app();

        let state = app.query_command_state("tool.builtin.polyline_mesh");

        assert!(!state.enabled);
        assert_eq!(
            "select connected line segments first",
            state.disabled_reason.as_deref().unwrap()
        );
    }

    #[test]
    fn all_cut_commands_replace_selected_faces_keep_colors_and_undo_atomically() {
        let cases = [
            ("tool.builtin.cut_holes", CutTool::Holes, "CUT_HOLES", 3.0),
            (
                "tool.builtin.cut_holes_2",
                CutTool::Holes2,
                "CUT_HOLES_2",
                4.0,
            ),
            ("tool.builtin.cut_out_3", CutTool::Cutout, "CUT_OUT_3", 4.0),
        ];
        for (command, _tool, expected_tool_id, expected_area) in cases {
            let mut app = test_app();
            let face_color = Rgba8::new(36, 72, 144, 255);
            let (boundary, faces) = app
                .document
                .append_colored_indexed_mesh_to_scene_root(
                    vec![
                        Vec3::ZERO,
                        Vec3::X * 2.0,
                        Vec3::new(2.0, 0.0, 2.0),
                        Vec3::Z * 2.0,
                    ],
                    vec![
                        ([0, 1], face_color),
                        ([1, 2], face_color),
                        ([2, 3], face_color),
                        ([3, 0], face_color),
                    ],
                    vec![([0, 1, 2], face_color), ([0, 2, 3], face_color)],
                )
                .unwrap();
            let loop_points = [
                Vec3::new(0.5, 0.0, 0.5),
                Vec3::new(1.5, 0.0, 0.5),
                Vec3::new(1.5, 0.0, 1.5),
                Vec3::new(0.5, 0.0, 1.5),
            ];
            let loop_edges = (0..loop_points.len())
                .map(|index| {
                    app.document
                        .append_line_segment_to_scene_root(
                            loop_points[index],
                            loop_points[(index + 1) % loop_points.len()],
                        )
                        .unwrap()
                        .affected_segments[0]
                })
                .collect::<Vec<_>>();
            let occurrence = OccurrenceRef::scene_root();
            let before_document = app.document.clone();
            let before_ids = faces.iter().copied().collect::<BTreeSet<_>>();
            let mut selected = faces
                .iter()
                .copied()
                .map(|face| SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Triangle(face),
                })
                .collect::<Vec<_>>();
            selected.extend(loop_edges.iter().copied().map(|edge| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(edge),
            }));
            app.selection.apply(selected, SelectionMode::Replace);

            assert!(
                app.query_command_state(command).enabled,
                "{command}: {}",
                app.status
            );
            assert_ne!(
                CommandCapability::Deferred,
                shell_commands().get(command).unwrap().descriptor.capability
            );
            let dispatch = app.dispatch_command(command, &CommandPayload::default());
            assert!(dispatch.accepted, "{command}: {}", app.status);
            assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
            assert_eq!(expected_tool_id, dispatch.result.outputs["cut"]["tool"]);

            let geometry = app.document.effective_geometry(&occurrence).unwrap();
            let new_faces = geometry
                .triangles
                .keys()
                .filter(|id| !before_ids.contains(id))
                .copied()
                .collect::<Vec<_>>();
            assert!(!new_faces.is_empty());
            assert!(
                before_ids
                    .iter()
                    .all(|id| !geometry.triangles.contains_key(id))
            );
            assert!(
                new_faces
                    .iter()
                    .all(|id| geometry.triangles[id].color == face_color)
            );
            let total_area = geometry
                .triangles
                .values()
                .map(|triangle| {
                    let [Some(a), Some(b), Some(c)] = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        panic!("replacement face has invalid vertices")
                    };
                    (b - a).cross(c - a).length() * 0.5
                })
                .sum::<f64>();
            assert!((expected_area - total_area).abs() < 1.0e-5);
            assert_eq!(loop_edges.len(), app.selection.summary().edges);
            assert_eq!(new_faces.len(), app.selection.summary().faces);
            assert_eq!(1, app.history.entries().len());

            app.run_command("edit.undo");
            let restored = app.document.effective_geometry(&occurrence).unwrap();
            assert_eq!(
                before_document
                    .definition(Document::SCENE_ROOT_DEFINITION_ID)
                    .unwrap()
                    .geometry
                    .triangles,
                restored.triangles
            );
            assert_eq!(boundary.len() + loop_edges.len(), restored.segments.len());
        }
    }

    #[test]
    fn cut_command_edits_an_entered_definition_in_its_local_coordinates() {
        let mut app = test_app();
        let definition = app
            .document
            .add_definition("editable face", DefinitionKind::Mesh);
        let color = Rgba8::new(90, 120, 150, 255);
        let (_, faces) = app
            .document
            .append_colored_indexed_mesh_to_definition(
                definition,
                vec![
                    Vec3::ZERO,
                    Vec3::X * 2.0,
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::Z * 2.0,
                ],
                vec![],
                vec![([0, 1, 2], color), ([0, 2, 3], color)],
            )
            .unwrap();
        let loop_points = [
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 1.5),
            Vec3::new(0.5, 0.0, 1.5),
        ];
        let edges = (0..loop_points.len())
            .map(|index| {
                app.document
                    .append_line_segment_to_definition(
                        definition,
                        loop_points[index],
                        loop_points[(index + 1) % loop_points.len()],
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let entry = app
            .document
            .add_layout_entry(
                app.document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::new(12.0, 5.0, -7.0)),
            )
            .unwrap();
        let occurrence = OccurrenceRef::root(entry);
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        let mut selected = faces
            .iter()
            .copied()
            .map(|face| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(face),
            })
            .collect::<Vec<_>>();
        selected.extend(edges.into_iter().map(|edge| SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Segment(edge),
        }));
        app.selection.apply(selected, SelectionMode::Replace);

        let dispatch = app.dispatch_command("tool.builtin.cut_holes_2", &CommandPayload::default());

        assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert!(geometry.triangles.len() > faces.len());
        assert!(
            geometry
                .triangles
                .values()
                .all(|triangle| triangle.color == color)
        );
        assert!(geometry.vertices.values().all(|vertex| {
            vertex.position.x.abs() <= 2.0
                && vertex.position.y.abs() <= 1.0e-8
                && vertex.position.z.abs() <= 2.0
        }));
        assert!(
            app.selection
                .selected()
                .iter()
                .all(|target| target.occurrence == occurrence)
        );
        assert!(app.document.validate().is_ok());
    }

    #[test]
    fn cut_command_in_prototype_editing_updates_the_shared_definition() {
        let mut app = test_app();
        let definition = app
            .document
            .add_definition("instance-local face", DefinitionKind::Mesh);
        let color = Rgba8::new(100, 130, 160, 255);
        let (_, faces) = app
            .document
            .append_colored_indexed_mesh_to_definition(
                definition,
                vec![
                    Vec3::ZERO,
                    Vec3::X * 2.0,
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::Z * 2.0,
                ],
                vec![],
                vec![([0, 1, 2], color), ([0, 2, 3], color)],
            )
            .unwrap();
        let loop_points = [
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 0.5),
            Vec3::new(1.5, 0.0, 1.5),
            Vec3::new(0.5, 0.0, 1.5),
        ];
        let edges = (0..loop_points.len())
            .map(|index| {
                app.document
                    .append_line_segment_to_definition(
                        definition,
                        loop_points[index],
                        loop_points[(index + 1) % loop_points.len()],
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let entry = app
            .document
            .add_layout_entry(
                app.document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::new(12.0, 5.0, -7.0)),
            )
            .unwrap();
        let occurrence = OccurrenceRef::root(entry);
        let prototype = app
            .document
            .definition(definition)
            .unwrap()
            .geometry
            .clone();
        assert!(app.document.set_occurrence_edits(
            occurrence.clone(),
            InstanceEdits {
                geometry_replacement: Some(prototype.clone()),
                ..InstanceEdits::default()
            }
        ));
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        let mut selected = faces
            .iter()
            .copied()
            .map(|face| SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Triangle(face),
            })
            .collect::<Vec<_>>();
        selected.extend(edges.into_iter().map(|edge| SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Segment(edge),
        }));
        app.selection.apply(selected, SelectionMode::Replace);

        let dispatch = app.dispatch_command("tool.builtin.cut_holes_2", &CommandPayload::default());

        assert_eq!(CommandCompletion::Completed, dispatch.result.completion);
        assert_ne!(
            prototype,
            app.document.definition(definition).unwrap().geometry
        );
        let local = app.document.effective_geometry(&occurrence).unwrap();
        assert!(local.triangles.len() > faces.len());
        assert!(
            local
                .triangles
                .values()
                .all(|triangle| triangle.color == color)
        );
        assert_eq!(1, app.history.entries().len());
        app.run_command("edit.undo");
        assert_eq!(
            prototype,
            *app.document.effective_geometry(&occurrence).unwrap()
        );
        assert!(app.document.validate().is_ok());
    }

    #[test]
    fn cut_failure_preserves_document_history_and_mixed_selection() {
        let mut app = test_app();
        let faces = app
            .document
            .append_triangles_to_scene_root(
                vec![Vec3::ZERO, Vec3::X * 2.0, Vec3::Z * 2.0],
                vec![[0, 1, 2]],
            )
            .unwrap();
        let edge = app
            .document
            .append_line_segment_to_scene_root(Vec3::new(0.25, 0.0, 0.25), Vec3::new(0.5, 0.0, 0.5))
            .unwrap()
            .affected_segments[0];
        let occurrence = OccurrenceRef::scene_root();
        app.selection.apply(
            [
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Triangle(faces[0]),
                },
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Segment(edge),
                },
            ],
            SelectionMode::Replace,
        );
        let document = app.document.clone();
        let selection = app.selection.selected().clone();

        let dispatch = app.dispatch_command("tool.builtin.cut_holes_2", &CommandPayload::default());

        assert_eq!(CommandCompletion::Failed, dispatch.result.completion);
        assert_eq!(document, app.document);
        assert_eq!(selection, *app.selection.selected());
        assert!(app.history.entries().is_empty());
    }

    #[test]
    fn surface_rectangle_command_uses_the_first_face_normal() {
        let mut app = test_app();
        assert!(
            app.dispatch_command("tool.builtin.surface_rectangle", &CommandPayload::default())
                .accepted
        );
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let normal = Vec3::new(1.0, 1.0, 0.0).try_normalized(1.0e-12).unwrap();
        let start = Vec3::new(1.0, 1.0, 0.0);
        let end = start + Vec3::new(2.0, 0.0, 4.0) + normal * 5.0;
        for point in [start, end] {
            app.handle_viewport_click_at_point(
                rect,
                Pos2::ZERO,
                Some(SnapResult {
                    point,
                    normal,
                    kind: SnapKind::Face,
                    screen_distance: 0.0,
                }),
                Some(point),
            );
        }
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(4, geometry.vertices.len());
        assert_eq!(2, geometry.triangles.len());
        assert!(
            geometry
                .vertices
                .values()
                .all(|vertex| ((vertex.position - start).dot(normal)).abs() < 1.0e-10)
        );
        assert_eq!("Surface Rect", app.active_tool);
        app.run_command("edit.undo");
        assert!(
            app.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .triangles
                .is_empty()
        );
    }

    #[test]
    fn canceling_a_pending_shape_pick_leaves_no_document_or_history_change() {
        let mut app = test_app();
        let revision = app.document.revision();
        app.dispatch_command("tool.builtin.circle", &CommandPayload::default());
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        assert_eq!(1, app.shape_tool_state.picks().len());

        let canceled = app.dispatch_command("tool.cancel", &CommandPayload::default());

        assert!(canceled.accepted);
        assert_eq!("Select", app.active_tool);
        assert!(app.shape_tool_state.picks().is_empty());
        let root = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert!(root.vertices.is_empty());
        assert_eq!(revision, app.document.revision());
        assert!(app.history.entries().is_empty());
    }

    #[test]
    fn planar_shape_tools_append_in_local_space_when_editing_a_rotated_instance() {
        let mut app = test_app();
        let prototype_instance = app
            .document
            .add_cuboid("rotated prototype", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&prototype_instance)
            .unwrap()
            .definition
            .id;
        let occurrence = app
            .document
            .add_layout_entry(
                app.document.root_layout(),
                definition,
                Affine3::from_linear_translation(
                    [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]],
                    Vec3::new(10.0, 0.0, 0.0),
                ),
            )
            .map(OccurrenceRef::root)
            .unwrap();
        let root_count = app.document.occurrence_refs().unwrap().len();
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));
        app.activate_tool("Rectangle");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        app.handle_viewport_click_at_point(
            rect,
            Pos2::ZERO,
            None,
            Some(Vec3::new(10.0, 2.0, -2.0)),
        );
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::new(12.0, 2.0, 0.0)));

        let resolved = app.document.resolve_occurrence(&occurrence).unwrap();
        assert_eq!(definition, resolved.definition.id);
        assert_eq!(root_count, app.document.occurrence_refs().unwrap().len());
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(8, geometry.vertices.len());
        assert_eq!(14, geometry.triangles.len());
        assert_eq!(
            8,
            app.document
                .effective_geometry(&prototype_instance)
                .unwrap()
                .vertices
                .len()
        );
        let world_vertices = geometry
            .vertices
            .values()
            .map(|vertex| resolved.world_transform.transform_point(vertex.position))
            .collect::<Vec<_>>();
        assert!(world_vertices.contains(&Vec3::new(10.0, 2.0, -2.0)));
        assert!(world_vertices.contains(&Vec3::new(12.0, 2.0, 0.0)));
        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(8, geometry.vertices.len());
        assert_eq!(12, geometry.triangles.len());
    }

    #[test]
    fn line_tool_commits_each_scene_segment_as_an_undoable_edit() {
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

        assert_eq!("Line", app.active_tool);
        assert_eq!(1, app.history.entries().len());
        assert!(app.status.contains("Grid snap"));
        assert_ne!(
            source,
            app.selection.selected().iter().next().unwrap().occurrence
        );
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        let positions = geometry
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        assert!(
            positions
                .iter()
                .any(|point| point.distance(start_snap.point) < 1.0e-9)
        );
        assert!(
            positions
                .iter()
                .any(|point| point.distance(end_snap.point) < 1.0e-9)
        );
    }

    #[test]
    fn transform_point_input_uses_the_shared_geometry_snap_result() {
        let mut app = test_app();
        app.document
            .add_line_segment(
                "snap target",
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
            )
            .unwrap();
        app.active_tool = "Move".to_owned();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let pointer = app.viewport.world_to_screen(rect, Vec3::new(2.0, 0.0, 0.0));

        let snap = app
            .snap_at_viewport_position(rect, pointer, 1.0)
            .expect("point-input tools should receive the shared snap result");

        assert_eq!(SnapKind::Endpoint, snap.kind);
        assert!(snap.point.distance(Vec3::new(2.0, 0.0, 0.0)) < 1.0e-9);
    }

    #[test]
    fn cuboid_uses_two_snapped_opposite_corners_with_preview_and_undo() {
        let mut app = test_app();
        app.activate_tool("Cuboid");
        let color = Rgba8::new(96, 144, 192, 255);
        app.current_paint_color = color;
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let snap_at = |point| SnapResult {
            point,
            normal: Vec3::Y,
            kind: SnapKind::Endpoint,
            screen_distance: 0.0,
        };
        let first = Vec3::new(2.0, 4.5, 3.0);
        let opposite = Vec3::new(0.25, -1.5, 0.5);
        let first_snap = snap_at(first);
        app.handle_viewport_click_at_point(
            rect,
            Pos2::ZERO,
            Some(first_snap),
            Some(first_snap.point),
        );
        assert_eq!(Some(first), app.cuboid_first);
        assert_eq!(0, app.history.entries().len());

        let invalid = snap_at(Vec3::new(first.x, 8.0, 4.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(invalid), Some(invalid.point));
        assert!(app.status.contains("every axis"));
        assert_eq!(Some(first), app.cuboid_first);
        assert_eq!(0, app.history.entries().len());

        let snap = snap_at(opposite);
        let preview = app.tool_preview(rect, Some(Pos2::ZERO), Some(snap));
        assert!(matches!(
            &preview,
            Some(ToolPreview::Cuboid(CuboidPreview { origin, dimensions }))
                if *origin == Vec3::new(0.25, -1.5, 0.5)
                    && *dimensions == Vec3::new(1.75, 6.0, 2.5)
        ));
        let feedback = app
            .tool_feedback(rect, Some(Pos2::ZERO), Some(snap), preview.as_ref())
            .expect("cuboid reports the snapped corner vector");
        assert!(feedback.text.contains("ΔX -1.750"));
        assert!(feedback.text.contains("ΔY -6.000"));
        assert!(feedback.text.contains("ΔZ -2.500"));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap), Some(snap.point));

        assert_eq!("Select", app.active_tool);
        assert_eq!(None, app.cuboid_first);
        assert_eq!(24, app.selection.selected().len());
        assert_eq!(1, app.history.entries().len());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(8, geometry.vertices.len());
        assert_eq!(12, geometry.segments.len());
        assert_eq!(12, geometry.triangles.len());
        assert!(
            geometry
                .segments
                .values()
                .all(|segment| segment.color == color)
        );
        assert!(
            geometry
                .triangles
                .values()
                .all(|triangle| triangle.color == color)
        );
        let positions = geometry
            .vertices
            .values()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>();
        assert!(positions.contains(&Vec3::new(0.25, -1.5, 0.5)));
        assert!(positions.contains(&Vec3::new(2.0, 4.5, 3.0)));
        let mut triangle_edge_counts = BTreeMap::<(usize, usize), usize>::new();
        for triangle in super::cuboid_mesh(CuboidPreview {
            origin: Vec3::new(0.25, -1.5, 0.5),
            dimensions: Vec3::new(1.75, 6.0, 2.5),
        })
        .triangles
        {
            for [start, end] in [
                [triangle[0], triangle[1]],
                [triangle[1], triangle[2]],
                [triangle[2], triangle[0]],
            ] {
                let key = (start.min(end), start.max(end));
                *triangle_edge_counts.entry(key).or_default() += 1;
            }
        }
        assert_eq!(18, triangle_edge_counts.len());
        assert!(triangle_edge_counts.values().all(|count| *count == 2));

        app.run_command("edit.undo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert!(geometry.vertices.is_empty());
        assert!(geometry.segments.is_empty());
        assert!(geometry.triangles.is_empty());
        app.run_command("edit.redo");
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(8, geometry.vertices.len());
        assert_eq!(12, geometry.triangles.len());
    }

    #[test]
    fn cuboid_is_inserted_into_the_active_object_definition() {
        let mut app = test_app();
        let host = app
            .document
            .add_cuboid("host", Vec3::new(2.0, 2.0, 2.0), Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        let occurrence_count = app.document.occurrence_refs().unwrap().len();
        app.saved_revision = app.document.revision();
        app.history.clear();
        app.selection
            .set_context(EditContext::Occurrence(host.clone()));
        app.activate_tool("Cuboid");

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        for point in [Vec3::new(10.5, 1.0, 0.5), Vec3::new(12.5, 3.0, 2.5)] {
            let snap = SnapResult {
                point,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.0,
            };
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, Some(snap), Some(point));
        }

        assert_eq!(
            occurrence_count,
            app.document.occurrence_refs().unwrap().len()
        );
        let geometry = app.document.effective_geometry(&host).unwrap();
        assert_eq!(16, geometry.vertices.len());
        assert_eq!(24, geometry.segments.len());
        assert_eq!(24, geometry.triangles.len());
        let resolved = app.document.resolve_occurrence(&host).unwrap();
        let world_positions = geometry
            .vertices
            .values()
            .map(|vertex| resolved.world_transform.transform_point(vertex.position))
            .collect::<Vec<_>>();
        assert!(world_positions.contains(&Vec3::new(10.5, 1.0, 0.5)));
        assert!(world_positions.contains(&Vec3::new(12.5, 3.0, 2.5)));
        assert_eq!(24, app.selection.selected().len());

        app.run_command("edit.undo");
        let geometry = app.document.effective_geometry(&host).unwrap();
        assert_eq!(8, geometry.vertices.len());
        assert_eq!(12, geometry.triangles.len());
    }

    #[test]
    fn escape_cancels_a_pending_cuboid_corner_and_returns_to_selection() {
        let mut app = test_app();
        app.activate_tool("Cuboid");
        app.cuboid_first = Some(Vec3::new(1.0, 2.0, 3.0));

        app.handle_escape();

        assert_eq!("Select", app.active_tool);
        assert_eq!(None, app.cuboid_first);
        assert_eq!(0, app.history.entries().len());
    }

    #[test]
    fn undo_removes_chained_line_segments_one_commit_at_a_time() {
        let mut app = test_app();
        app.run_command("tool.line");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let pick = |app: &mut MeshcraftApp, point: Vec3| {
            let position = app.viewport.world_to_screen(rect, point);
            app.handle_viewport_click_at_point(rect, position, None, Some(point));
        };

        pick(&mut app, Vec3::ZERO);
        pick(&mut app, Vec3::X * 2.0);
        pick(&mut app, Vec3::new(2.0, 0.0, 2.0));

        assert_eq!(2, app.history.entries().len());
        assert_eq!(
            2,
            app.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .segments
                .len()
        );

        app.run_command("edit.undo");
        assert_eq!(1, app.history.cursor());
        assert_eq!(
            1,
            app.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .segments
                .len()
        );

        app.run_command("edit.undo");
        assert_eq!(0, app.history.cursor());
        assert!(
            app.document
                .effective_geometry(&OccurrenceRef::scene_root())
                .unwrap()
                .segments
                .is_empty()
        );
        assert_eq!("Line", app.active_tool);
    }

    #[test]
    fn construction_line_commits_discrete_pairs_to_scene_root() {
        let mut app = test_app();
        app.run_command("tool.builtin.construction_line");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let pick = |app: &mut MeshcraftApp, point: Vec3| {
            let position = app.viewport.world_to_screen(rect, point);
            app.handle_viewport_click_at_point(rect, position, None, Some(point));
        };

        let first_start = Vec3::new(-8.0, 0.0, 0.0);
        let first_end = Vec3::new(-6.0, 0.0, 1.0);
        pick(&mut app, first_start);
        pick(&mut app, first_end);
        assert_eq!("Construction Line", app.active_tool);
        assert_eq!(None, app.drawing_tool.anchor());
        assert_eq!(1, app.history.entries().len());

        let second_start = Vec3::new(6.0, 0.0, 0.0);
        let second_end = Vec3::new(8.0, 0.0, 1.0);
        pick(&mut app, second_start);
        pick(&mut app, second_end);
        assert_eq!("Construction Line", app.active_tool);
        assert_eq!(2, app.history.entries().len());

        // Cancelling a pending pair leaves each committed pair intact.
        pick(&mut app, Vec3::new(10.0, 0.0, 0.0));
        app.cancel_active_tool();
        assert_eq!("Select", app.active_tool);
        assert_eq!(2, app.history.entries().len());
        assert_eq!(1, app.selection.summary().edges);
        assert_eq!(0, app.document.occurrence_refs().unwrap().len());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(2, geometry.segments.len());
    }

    #[test]
    fn typed_distance_commits_the_previewed_scene_line_length() {
        let mut app = test_app();
        app.run_command("tool.line");
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());

        let rect = app.viewport_rect;
        let start = Vec3::new(2.0, 0.0, 0.0);
        let pointer = Vec3::new(2.0, 0.0, 6.0);
        let start_screen = app.viewport.world_to_screen(rect, start);
        let pointer_screen = app.viewport.world_to_screen(rect, pointer);
        assert!(rect.contains(start_screen) && rect.contains(pointer_screen));
        click_workspace_position(&mut app, &context, start_screen);

        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::PointerMoved(pointer_screen)],
        );
        app.run_command("tool.distance");
        run_workspace_frame(
            &mut app,
            &context,
            vec![
                Event::PointerMoved(pointer_screen),
                Event::Text("2.5".to_owned()),
                Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );

        assert_eq!(1, app.history.entries().len());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        assert_eq!(1, geometry.segments.len());
        let segment = geometry.segments.values().next().unwrap();
        let endpoints = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference).unwrap());
        assert!(endpoints.iter().any(|point| point.distance(start) < 1.0e-6));
        assert!((endpoints[0].distance(endpoints[1]) - 2.5).abs() < 1.0e-6);
        assert_eq!("Line", app.active_tool);
    }

    #[test]
    fn command_palette_finish_commits_the_same_numeric_line_input() {
        let mut app = test_app();
        app.run_command("tool.line");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        let start = Vec3::new(2.0, 0.0, 0.0);
        let pointer = Vec3::new(2.0, 0.0, 6.0);
        let start_screen = app.viewport.world_to_screen(rect, start);
        app.handle_viewport_click_at_point(rect, start_screen, None, Some(start));
        app.drawing_tool.handle(ToolEvent::PointerMoved(pointer));
        app.drawing_tool.handle(ToolEvent::Text("2.5"));

        assert!(app.query_command_state("tool.finish").enabled);
        let result = app.dispatch_command("tool.finish", &CommandPayload::default());

        assert!(result.accepted);
        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!(1, app.history.entries().len());
        let geometry = app
            .document
            .effective_geometry(&OccurrenceRef::scene_root())
            .unwrap();
        let segment = geometry.segments.values().next().unwrap();
        let endpoints = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference).unwrap());
        assert!((endpoints[0].distance(endpoints[1]) - 2.5).abs() < 1.0e-6);
        assert!(app.drawing_tool.numeric_input().is_empty());
    }

    #[test]
    fn palette_undo_point_and_cancel_share_line_keyboard_state() {
        let mut app = test_app();
        assert!(!app.query_command_state("tool.cancel").enabled);
        app.run_command("tool.line");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 640.0));
        app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(Vec3::ZERO));
        app.drawing_tool.handle(ToolEvent::Text("12"));
        assert!(app.query_command_state("tool.undo_point").enabled);
        app.run_command("tool.undo_point");
        assert_eq!("1", app.drawing_tool.numeric_input());
        app.run_command("tool.undo_point");
        assert_eq!("", app.drawing_tool.numeric_input());
        assert_eq!(Some(Vec3::ZERO), app.drawing_tool.anchor());

        assert!(app.query_command_state("tool.cancel").enabled);
        app.run_command("tool.cancel");
        assert_eq!("Select", app.active_tool);
        assert_eq!(None, app.drawing_tool.anchor());
        assert_eq!(0, app.history.entries().len());
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
    fn delete_selection_removes_a_viewport_picked_face_and_is_undoable() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "picked face",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(-1.0, -1.0, -1.0),
            )
            .unwrap();
        let target = click_visible_cube_face(&mut app, &occurrence);
        let SelectedEntity::Triangle(face) = target.entity else {
            unreachable!();
        };
        let original = app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .len();

        assert!(app.query_command_state("edit.delete_selection").enabled);
        let deleted = app.dispatch_command("edit.delete_selection", &CommandPayload::default());

        assert!(deleted.accepted, "{:?}", deleted.result);
        assert_eq!(json!(1), deleted.result.outputs["deleted_count"]);
        assert!(
            !app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .contains_key(&face)
        );
        assert!(app.selection.selected().is_empty());
        assert_eq!(1, app.history.entries().len());

        app.run_command("edit.undo");
        assert_eq!(
            original,
            app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .len()
        );
        app.run_command("edit.redo");
        assert_eq!(
            original - 1,
            app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .len()
        );
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
    fn delete_key_removes_a_viewport_selected_mesh_face() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "keyboard face",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(-1.0, -1.0, -1.0),
            )
            .unwrap();
        let target = click_visible_cube_face(&mut app, &occurrence);
        let SelectedEntity::Triangle(face) = target.entity else {
            unreachable!();
        };
        assert!(app.query_command_state("edit.delete_selection").enabled);

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

        assert!(app.selection.selected().is_empty());
        assert!(
            !app.document
                .effective_geometry(&occurrence)
                .unwrap()
                .triangles
                .contains_key(&face)
        );
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn delete_command_is_searchable_from_the_palette() {
        let command = shell_commands()
            .get("edit.delete_selection")
            .expect("delete command is registered");
        assert!(command_matches(&command.descriptor, "delete selection"));
        assert_eq!(Some("Delete"), command.descriptor.shortcut.as_deref());
        assert_eq!(CommandKind::DocumentMutation, command.descriptor.kind);
        assert!(
            LEGACY_TOOLBAR_COMMAND_IDS
                .iter()
                .any(|(toolbar, ids)| *toolbar == "actions"
                    && ids.contains(&"edit.delete_selection"))
        );
        assert_eq!(
            vec!["Clipboard & History", "Clipboard & History"],
            command.descriptor.presentation.menu_path
        );
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
    fn selection_column_controls_prune_selection_and_keep_hidden_types_read_only() {
        let mut app = test_app();
        let instance = app
            .document
            .add_cuboid("Part", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        app.set_selection([instance.clone()]);
        let before = app.document.clone();
        let update = |app: &mut MeshcraftApp, kind: &str, property: &str, enabled: bool| {
            let mut payload = CommandPayload::default();
            payload.values.insert("kind".into(), kind.into());
            payload.values.insert("property".into(), property.into());
            payload.values.insert("enabled".into(), enabled.into());
            assert_eq!(
                CommandCompletion::Completed,
                app.dispatch_command("selection.update_display", &payload)
                    .result
                    .completion
            );
        };
        update(&mut app, "object", "editable", false);
        assert!(app.selection.selected().is_empty());
        app.set_selection([instance.clone()]);
        assert!(app.selection.selected().is_empty());
        app.apply_native_selection_region(
            vec![SelectionTarget {
                occurrence: instance.clone(),
                entity: SelectedEntity::Occurrence,
            }],
            false,
            SelectionMode::Replace,
        );
        assert!(app.selection.selected().is_empty());
        assert_eq!(before, app.document);
        update(&mut app, "all", "visible", false);
        update(&mut app, "all", "editable", true);
        assert!(
            SelectionKind::ALL
                .iter()
                .all(|k| !app.selection_display_policy.can_draw(*k)
                    && !app.selection_display_policy.can_select(*k))
        );
        update(&mut app, "all", "visible", true);
        assert!(
            SelectionKind::ALL
                .iter()
                .all(|k| app.selection_display_policy.can_select(*k))
        );
        update(&mut app, "all", "wireframe", true);
        assert!(
            SelectionKind::ALL
                .iter()
                .all(|k| app.selection_display_policy.state(*k).wireframe)
        );
        update(&mut app, "dimensions_texts", "visible", false);
        assert!(
            !app.selection_display_policy
                .can_draw(SelectionKind::Dimension)
        );
        assert!(!app.selection_display_policy.can_select(SelectionKind::Text));
        update(&mut app, "architecture", "editable", false);
        assert!(
            !app.selection_display_policy
                .can_select(SelectionKind::Stair)
        );
        assert!(!app.selection_display_policy.can_select(SelectionKind::Hvac));
        assert_eq!(before, app.document);
        update(&mut app, "all", "visible", false);
        app.activate_tool("Polyline");
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.));
        for point in [Vec3::ZERO, Vec3::X * 2.] {
            app.handle_viewport_click_at_point(rect, Pos2::ZERO, None, Some(point));
        }
        app.handle_native_tool_key("enter", false);
        assert!(
            app.selection.selected().is_empty(),
            "new hidden geometry must not remain selectable"
        );
    }

    #[test]
    fn selection_display_command_updates_visibility_modifiability_and_face_wireframe() {
        let mut app = test_app();
        let revision = app.document.revision();
        let command = shell_commands()
            .get("selection.update_display")
            .expect("selection display command is registered");
        assert_eq!(CommandKind::SessionMutation, command.descriptor.kind);
        assert!(
            command
                .descriptor
                .parameters
                .iter()
                .all(|parameter| parameter.required)
        );

        let dispatch_display = app.dispatch_command(
            "selection.update_display",
            &CommandPayload {
                schema_version: meshcraft_core::commands::COMMAND_SCHEMA_VERSION,
                values: BTreeMap::from([
                    ("kind".to_owned(), json!("face")),
                    ("property".to_owned(), json!("display")),
                    ("enabled".to_owned(), json!(false)),
                ]),
            },
        );
        assert!(dispatch_display.accepted);
        assert!(!app.selection_display_policy.can_draw(SelectionKind::Face));
        assert!(!app.selection_display_policy.can_select(SelectionKind::Face));

        let dispatch_modifiable = app.dispatch_command(
            "selection.update_display",
            &CommandPayload {
                schema_version: meshcraft_core::commands::COMMAND_SCHEMA_VERSION,
                values: BTreeMap::from([
                    ("kind".to_owned(), json!("object")),
                    ("property".to_owned(), json!("modifiable")),
                    ("enabled".to_owned(), json!(false)),
                ]),
            },
        );
        assert!(dispatch_modifiable.accepted);
        assert!(app.selection_display_policy.can_draw(SelectionKind::Object));
        assert!(
            !app.selection_display_policy
                .can_select(SelectionKind::Object)
        );

        let dispatch_wireframe = app.dispatch_command(
            "selection.update_display",
            &CommandPayload {
                schema_version: meshcraft_core::commands::COMMAND_SCHEMA_VERSION,
                values: BTreeMap::from([
                    ("kind".to_owned(), json!("face")),
                    ("property".to_owned(), json!("wireframe")),
                    ("enabled".to_owned(), json!(true)),
                ]),
            },
        );
        assert!(dispatch_wireframe.accepted);
        assert!(
            app.selection_display_policy
                .state(SelectionKind::Face)
                .wireframe
        );
        assert_eq!(revision, app.document.revision());

        let rejected_wireframe = app.dispatch_command(
            "selection.update_display",
            &CommandPayload {
                schema_version: meshcraft_core::commands::COMMAND_SCHEMA_VERSION,
                values: BTreeMap::from([
                    ("kind".to_owned(), json!("edge")),
                    ("property".to_owned(), json!("wireframe")),
                    ("enabled".to_owned(), json!(true)),
                ]),
            },
        );
        assert!(rejected_wireframe.accepted);
        assert_eq!(
            meshcraft_core::commands::CommandCompletion::Completed,
            rejected_wireframe.result.completion
        );
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
    fn array_command_copies_selected_mesh_and_selects_the_generated_faces() {
        let mut app = test_app();
        assert!(!app.query_command_state("object.array_selection").enabled);
        app.run_command("geometry.add_cuboid");
        let occurrence = app.selected_occurrences()[0].clone();
        let source_face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let original_face_count = app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .len();
        app.selection.clear();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(source_face),
        });
        app.array_count = 2;
        app.array_spacing = 3.0;

        assert!(app.query_command_state("object.array_selection").enabled);
        app.selection.clear();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::SemanticHandle {
                kind: super::SelectionKind::Hotspot,
                id: "hotspot-1".to_owned(),
            },
        });
        assert!(!app.query_command_state("object.array_selection").enabled);
        app.selection.clear();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(source_face),
        });
        app.array_preview = true;
        let preview = app
            .tool_preview(Rect::NOTHING, None, None)
            .expect("a mesh selection should have an array preview");
        assert!(matches!(preview, ToolPreview::Array { copies: 2, .. }));

        let mut payload = CommandPayload::default();
        payload.values.insert("count".to_owned(), json!(2));
        payload.values.insert("spacing".to_owned(), json!(3.0));
        let result = app.dispatch_command("object.array_selection", &payload);

        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!(2, result.result.outputs["copy_count"]);
        assert_eq!(2, result.result.outputs["face_count"]);
        assert_eq!(
            0,
            result.result.outputs["occurrences"]
                .as_array()
                .unwrap()
                .len()
        );
        assert_eq!(2, app.selection.selected().len());
        assert!(app.selection.selected().iter().all(|target| {
            target.occurrence == occurrence
                && matches!(target.entity, SelectedEntity::Triangle(id) if id != source_face)
        }));
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(original_face_count + 2, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&source_face));
        assert!(!app.array_preview);
    }

    #[test]
    fn array_command_copies_selected_dimension_and_selects_generated_handles() {
        let mut app = test_app();
        let occurrence = meshcraft_core::document::OccurrenceRef::scene_root();
        let definition_id = meshcraft_core::document::Document::SCENE_ROOT_DEFINITION_ID;
        let mut snapshot = serde_json::to_value(&app.document).unwrap();
        snapshot["definitions"]["0"]["extensions"]["k3d.dimensions"] = json!([{
            "start": {"x": 0.0, "y": 0.0, "z": 0.0},
            "end": {"x": 1.0, "y": 0.0, "z": 0.0},
            "offset": {"x": 0.0, "y": 1.0, "z": 0.0}
        }]);
        snapshot["definitions"]["0"]["domain_entity_ids"]["dimensions"] = json!(["dimension-1"]);
        app.document = serde_json::from_value(snapshot).unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::SemanticHandle {
                kind: super::SelectionKind::Dimension,
                id: "dimension-1".to_owned(),
            },
        });

        assert!(app.query_command_state("object.array_selection").enabled);
        app.array_preview = true;
        app.array_count = 2;
        app.array_spacing = 3.0;
        let preview = app
            .tool_preview(Rect::NOTHING, None, None)
            .expect("selected dimension should have an array preview");
        let ToolPreview::Array { edges, copies, .. } = preview else {
            panic!("array preview should use the fixed-count array visualization");
        };
        assert_eq!(2, copies);
        assert!(!edges.is_empty());

        let mut payload = CommandPayload::default();
        payload.values.insert("count".to_owned(), json!(2));
        payload.values.insert("spacing".to_owned(), json!(3.0));

        let result = app.dispatch_command("object.array_selection", &payload);

        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!(2, result.result.outputs["annotation_count"]);
        assert_eq!(2, app.selection.selected().len());
        assert!(app.selection.selected().iter().all(|target| {
            target.occurrence == occurrence
                && matches!(
                    target.entity,
                    SelectedEntity::SemanticHandle {
                        kind: super::SelectionKind::Dimension,
                        ..
                    }
                )
        }));
        let definition = app.document.definition(definition_id).unwrap();
        assert_eq!(
            3,
            definition.extensions["k3d.dimensions"]
                .as_array()
                .unwrap()
                .len()
        );
        assert_eq!(3, definition.domain_entity_ids["dimensions"].len());
        assert!(app.document.validate().is_ok());
    }

    #[test]
    fn planar_translation_array_uses_snapped_picks_preview_and_selection_targets() {
        let mut app = test_app();
        assert!(
            !app.query_command_state("tool.builtin.planar_translate_multiple")
                .enabled
        );
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let source_face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(source_face),
        });

        assert!(
            app.query_command_state("tool.builtin.planar_translate_multiple")
                .enabled
        );
        app.activate_tool("Planar Translate Multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::new(2.0, 5.0, 2.0));

        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: Vec3::new(4.0, 0.0, 4.0),
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("third snapped point produces the planar array preview");
        let ToolPreview::ArrayOffsets {
            offsets, capped, ..
        } = preview
        else {
            panic!("the planar array preview carries explicit translation offsets")
        };
        assert_eq!(8, offsets.len());
        assert!(!capped);

        pick_array_tool_point(&mut app, Vec3::new(4.0, 0.0, 4.0));

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("8 array copies"));
        assert_eq!(8, app.selection.selected().len());
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(20, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&source_face));
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn copy_multiple_runs_three_pick_preview_and_commit_from_selected_faces() {
        let mut app = test_app();
        assert!(
            !app.query_command_state("tool.builtin.copy_multiple")
                .enabled
        );
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let source_face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(source_face),
        });
        assert!(
            app.query_command_state("tool.builtin.copy_multiple")
                .enabled
        );

        app.run_command("tool.builtin.copy_multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        assert!(app.status.contains("size point (V)"));
        pick_array_tool_point(&mut app, Vec3::X * 2.0);
        assert!(app.status.contains("span point (D)"));
        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: Vec3::new(5.5, 0.0, 3.0),
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("Copy Multiple previews the measured copies before commit");
        let ToolPreview::ArrayOffsets { offsets, .. } = preview else {
            panic!("Copy Multiple previews each selected copy offset")
        };
        assert_eq!(vec![Vec3::X * 2.0, Vec3::X * 4.0], offsets);

        pick_array_tool_point(&mut app, Vec3::new(5.5, 0.0, 3.0));

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("2 array copies"));
        assert_eq!(2, app.selection.selected().len());
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(14, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&source_face));
    }

    #[test]
    fn planar_rotational_array_previews_and_copies_selected_faces() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let source_face = *app
            .document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(source_face),
        });

        app.run_command("tool.builtin.planar_rotate_multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::X);
        pick_array_tool_point(&mut app, Vec3::Y);
        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: Vec3::X,
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("the final point previews the full-circle array copies");
        let ToolPreview::ArrayTransforms {
            transforms, capped, ..
        } = preview
        else {
            panic!("rotational preview stores each affine copy transform")
        };
        assert_eq!(3, transforms.len());
        assert!(!capped);

        pick_array_tool_point(&mut app, Vec3::X);

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("3 rotational array copies"));
        assert_eq!(3, app.selection.selected().len());
        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        assert_eq!(15, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&source_face));
        assert_eq!(1, app.history.entries().len());
    }

    #[test]
    fn helicoidal_rotate_multiple_commits_object_instances_with_axial_lift() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });

        app.run_command("tool.builtin.helicoidal_rotate_multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::X);
        pick_array_tool_point(&mut app, Vec3::Y);
        pick_array_tool_point(&mut app, Vec3::new(-1.0, 0.0, 4.0));

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("2 rotational array copies"));
        assert_eq!(2, app.selection.selected().len());
        let z_positions = app
            .selection
            .selected()
            .iter()
            .map(|target| {
                app.document
                    .resolve_occurrence(&target.occurrence)
                    .unwrap()
                    .world_transform
                    .translation
                    .z
            })
            .collect::<Vec<_>>();
        assert_eq!(vec![2.0, 4.0], z_positions);
        assert_eq!(3, app.top_level_occurrences().len());
    }

    #[test]
    fn rotational_array_alias_uses_its_own_prompt_labels() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence,
            entity: SelectedEntity::Occurrence,
        });

        app.run_command("tool.builtin.rotational_array");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        assert!(app.status.contains("reference point (A)"));
        pick_array_tool_point(&mut app, Vec3::X);
        assert!(app.status.contains("increment point (B)"));
        pick_array_tool_point(&mut app, Vec3::Y);
        assert!(app.status.contains("sweep end point (D)"));

        app.cancel_active_tool();
        assert_eq!("Select", app.active_tool);
        assert!(!app.rotational_array_tool_state.active());
    }

    #[test]
    fn helical_array_uses_world_y_fixed_step_and_commits_selected_instances() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });

        assert!(
            app.query_command_state("tool.builtin.helical_array")
                .enabled
        );
        app.run_command("tool.builtin.helical_array");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        assert!(app.status.contains("reference point (A)"));
        pick_array_tool_point(&mut app, Vec3::X);
        assert!(app.status.contains("helical unit point (B)"));
        pick_array_tool_point(&mut app, Vec3::new(0.0, 2.0, -1.0));
        assert!(app.status.contains("sweep/height end point (D)"));
        let extent = Vec3::new(-1.0, 4.0, 0.0);
        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: extent,
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("Helical Array previews each fixed angle and lift step");
        let ToolPreview::ArrayTransforms { transforms, .. } = preview else {
            panic!("Helical Array preview carries affine transforms")
        };
        assert_eq!(2, transforms.len());
        assert_eq!(2.0, transforms[0].transform_point(Vec3::ZERO).y);
        assert_eq!(4.0, transforms[1].transform_point(Vec3::ZERO).y);

        pick_array_tool_point(&mut app, extent);

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("2 rotational array copies"));
        assert_eq!(2, app.selection.selected().len());
        let y_positions = app
            .selection
            .selected()
            .iter()
            .map(|target| {
                app.document
                    .resolve_occurrence(&target.occurrence)
                    .unwrap()
                    .world_transform
                    .translation
                    .y
            })
            .collect::<Vec<_>>();
        assert_eq!(vec![2.0, 4.0], y_positions);
        assert_eq!(3, app.top_level_occurrences().len());
    }

    #[test]
    fn rotational_array_even_previews_and_commits_span_equalized_positions() {
        let mut app = test_app();
        let origin = Vec3::X * 2.0;
        let occurrence = app
            .document
            .add_cuboid("wide source", Vec3::new(1.0, 2.0, 1.0), origin)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });
        assert!(
            app.query_command_state("tool.builtin.rotational_array_equalized")
                .enabled
        );

        app.run_command("tool.builtin.rotational_array_equalized");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        assert!(app.status.contains("reference point (A)"));
        pick_array_tool_point(&mut app, Vec3::X);
        assert!(app.status.contains("angular hint point (B)"));
        pick_array_tool_point(&mut app, Vec3::Y);
        assert!(app.status.contains("sweep end point (D)"));
        let extent = Vec3::X;
        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: extent,
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("Rotational Array Even uses selected geometry span in its preview");
        let ToolPreview::ArrayTransforms { transforms, .. } = preview else {
            panic!("Rotational Array Even previews each affine transform")
        };
        assert_eq!(1, transforms.len());
        assert!(transforms[0].transform_point(origin).distance(-origin) < 1.0e-9);

        pick_array_tool_point(&mut app, extent);

        assert_eq!("Select", app.active_tool);
        assert!(app.status.contains("1 rotational array copies"));
        assert_eq!(1, app.selection.selected().len());
        let result = app.selection.selected().iter().next().unwrap();
        assert_ne!(occurrence, result.occurrence);
        assert!(
            app.document
                .resolve_occurrence(&result.occurrence)
                .unwrap()
                .world_transform
                .translation
                .distance(-origin)
                < 1.0e-9
        );
        assert_eq!(2, app.top_level_occurrences().len());
    }

    #[test]
    fn helical_array_even_equalizes_full_turns_and_retains_axial_lift() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("helical source", Vec3::new(1.0, 1.0, 2.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });
        assert!(
            app.query_command_state("tool.builtin.helical_array_equalized")
                .enabled
        );

        app.run_command("tool.builtin.helical_array_equalized");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::X);
        pick_array_tool_point(&mut app, Vec3::new(0.0, 2.0, -1.0));
        assert!(app.status.contains("sweep/height end point (D)"));
        let extent = Vec3::new(1.0, 4.0, 0.0);
        let preview = app
            .tool_preview(
                Rect::NOTHING,
                None,
                Some(SnapResult {
                    point: extent,
                    normal: Vec3::Y,
                    kind: SnapKind::Grid,
                    screen_distance: 0.0,
                }),
            )
            .expect("Helical Array Even previews equalized turns and pitch");
        let ToolPreview::ArrayTransforms { transforms, .. } = preview else {
            panic!("Helical Array Even preview carries affine transforms")
        };
        assert_eq!(2, transforms.len());
        assert_eq!(2.0, transforms[0].transform_point(Vec3::ZERO).y);
        assert_eq!(4.0, transforms[1].transform_point(Vec3::ZERO).y);

        pick_array_tool_point(&mut app, extent);

        assert_eq!("Select", app.active_tool);
        assert_eq!(2, app.selection.selected().len());
        let mut y_positions = app
            .selection
            .selected()
            .iter()
            .map(|target| {
                app.document
                    .resolve_occurrence(&target.occurrence)
                    .unwrap()
                    .world_transform
                    .translation
                    .y
            })
            .collect::<Vec<_>>();
        y_positions.sort_by(f64::total_cmp);
        assert_eq!(vec![2.0, 4.0], y_positions);
        assert_eq!(3, app.top_level_occurrences().len());
    }

    #[test]
    fn volumetric_translation_toolbar_tool_copies_selected_object_occurrences() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        });
        app.activate_tool("Volumetric Translate Multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);
        pick_array_tool_point(&mut app, Vec3::new(2.0, 2.0, 0.0));
        pick_array_tool_point(&mut app, Vec3::new(4.0, 4.0, 0.0));

        assert_eq!(8, app.selection.selected().len());
        assert!(app.selection.selected().iter().all(|target| {
            target.entity == SelectedEntity::Occurrence
                && target.occurrence != occurrence
                && target.occurrence.path.len() == 1
        }));
        assert_eq!("Select", app.active_tool);
    }

    #[test]
    fn escape_cancels_translation_array_then_clears_source_on_the_next_press() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("source", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.selection.select(SelectionTarget {
            occurrence,
            entity: SelectedEntity::Occurrence,
        });
        app.activate_tool("Planar Translate Multiple");
        pick_array_tool_point(&mut app, Vec3::ZERO);

        app.handle_escape();

        assert_eq!("Select", app.active_tool);
        assert_eq!(app.selection.selected().len(), 1);
        app.handle_escape();
        assert!(app.selection.selected().is_empty());
        assert!(!app.array_tool_state.active());
    }

    #[test]
    fn array_preview_tracks_parameters_and_matches_committed_copy_geometry() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.array_count = 2;
        app.array_spacing = 4.0;
        app.array_preview = true;

        let preview = app
            .tool_preview(Rect::NOTHING, None, None)
            .expect("array parameter preview is independent of pointer position");
        let ToolPreview::Array {
            edges,
            copies,
            displacement,
        } = &preview
        else {
            panic!("Array should preview translated copies of the selected object");
        };
        assert_eq!(2, *copies);
        assert_eq!(Vec3::X * 4.0, *displacement);
        assert!(!edges.is_empty());
        let feedback = app
            .tool_feedback(Rect::NOTHING, Some(Pos2::ZERO), None, Some(&preview))
            .expect("array preview displays its per-copy vector and count");
        assert!(feedback.text.contains("2 copies"));
        assert!(feedback.text.contains("4.000 unit"));

        let expected_start = edges[0][0] + *displacement;
        let result = app.array_selection(2, 4.0);
        assert_eq!(CommandCompletion::Completed, result.completion);
        assert!(!app.array_preview);
        let copies = app.selected_occurrences();
        let actual_copy_points = copies
            .iter()
            .flat_map(|occurrence| {
                let resolved = app.document.resolve_occurrence(occurrence).unwrap();
                let geometry = app.document.effective_geometry(occurrence).unwrap();
                geometry.segments.values().flat_map(move |segment| {
                    segment.endpoints.into_iter().filter_map(move |vertex| {
                        geometry
                            .vertex_position(vertex)
                            .map(|point| resolved.world_transform.transform_point(point))
                    })
                })
            })
            .collect::<Vec<_>>();
        assert!(
            actual_copy_points
                .iter()
                .any(|point| { point.distance(expected_start) < 1.0e-9 })
        );
    }

    #[test]
    fn create_object_groups_selected_face_on_only_the_selected_instance() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let first = app.selected_occurrences()[0].clone();
        let definition = app
            .document
            .resolve_occurrence(&first)
            .unwrap()
            .definition
            .id;
        let root_layout = app.document.root_layout();
        let second_entry = app
            .document
            .add_layout_entry(
                root_layout,
                definition,
                Affine3::from_translation(Vec3::X * 10.0),
            )
            .unwrap();
        let second = OccurrenceRef::root(second_entry);
        let face = *app
            .document
            .effective_geometry(&first)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.apply(
            [SelectionTarget {
                occurrence: first.clone(),
                entity: SelectedEntity::Triangle(face),
            }],
            meshcraft_core::selection::SelectionMode::Replace,
        );
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("name".to_owned(), json!("Selected facade"));

        let result = app.dispatch_command("object.create_from_selection", &payload);

        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert_eq!(
            11,
            app.document
                .effective_geometry(&first)
                .unwrap()
                .triangles
                .len()
        );
        assert_eq!(
            12,
            app.document
                .effective_geometry(&second)
                .unwrap()
                .triangles
                .len()
        );
        assert_eq!(
            12,
            app.document
                .definition(definition)
                .unwrap()
                .geometry
                .triangles
                .len()
        );
        let object = app.selected_occurrences()[0].clone();
        assert_eq!(
            "Selected facade",
            app.document
                .resolve_occurrence(&object)
                .unwrap()
                .definition
                .name
        );
        assert_eq!(
            1,
            app.document
                .effective_geometry(&object)
                .unwrap()
                .triangles
                .len()
        );
        assert!(app.can_undo_document());
    }

    #[test]
    fn explode_selection_replaces_an_instance_with_selected_mesh_entities() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let original = app.selected_occurrences()[0].clone();

        let result = app.dispatch_command("object.explode_selection", &CommandPayload::default());

        assert_eq!(CommandCompletion::Completed, result.result.completion);
        assert!(app.document.resolve_occurrence(&original).is_none());
        assert_eq!(24, app.selection.selected().len());
        let result_occurrence = app
            .selection
            .selected()
            .iter()
            .next()
            .unwrap()
            .occurrence
            .clone();
        let geometry = app.document.effective_geometry(&result_occurrence).unwrap();
        assert_eq!(12, geometry.segments.len());
        assert_eq!(12, geometry.triangles.len());
        assert_eq!(json!(1), result.result.outputs["exploded_count"]);
    }

    #[test]
    fn object_place_has_live_preview_and_palette_control_for_surface_glue() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let source = app.selected_occurrences()[0].clone();
        let definition = app
            .document
            .resolve_occurrence(&source)
            .unwrap()
            .definition
            .id;
        assert!(!app.query_command_state("object.toggle_place_glue").enabled);

        let begin = app.dispatch_command(
            "object.place_selected_definition",
            &CommandPayload::default(),
        );
        assert_eq!(CommandCompletion::Completed, begin.result.completion);
        let glue = app.dispatch_command("object.toggle_place_glue", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, glue.result.completion);
        assert_eq!(json!(true), glue.result.outputs["enabled"]);
        assert_eq!(
            Some(true),
            app.query_command_state("object.toggle_place_glue").checked
        );

        let hit = super::PlacementHit {
            position: Vec3::new(4.0, 3.0, -2.0),
            surface_normal: Some(Vec3::X),
        };
        app.object_place_tool_state.update_pointer(Some(hit));
        assert!(
            app.tool_preview(
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
                Some(Pos2::new(40.0, 40.0)),
                None,
            )
            .is_some()
        );

        app.handle_viewport_click_at_point(
            Rect::NOTHING,
            Pos2::ZERO,
            Some(SnapResult {
                point: hit.position,
                normal: Vec3::X,
                kind: SnapKind::Face,
                screen_distance: 0.0,
            }),
            Some(hit.position),
        );

        let placed = app.selected_occurrences()[0].clone();
        let resolved = app.document.resolve_occurrence(&placed).unwrap();
        assert_eq!(definition, resolved.definition.id);
        assert!(
            resolved
                .world_transform
                .transform_point(Vec3::ZERO)
                .distance(hit.position)
                < 1e-10
        );
        let aligned_up = resolved
            .world_transform
            .transform_normal(
                Vec3::Y,
                meshcraft_core::geometry::GeometryTolerance::default(),
            )
            .unwrap();
        assert!(aligned_up.distance(Vec3::X) < 1e-10);
    }

    #[test]
    fn copy_selection_in_prototype_editing_updates_the_shared_subscene() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        app.run_command("object.create_from_selection");
        let parent = app.selected_occurrences()[0].clone();
        let definition = app
            .document
            .resolve_occurrence(&parent)
            .unwrap()
            .definition
            .id;
        let shared_layout = app
            .document
            .definition(definition)
            .unwrap()
            .default_child_layout
            .unwrap();
        let child_entry = app.document.child_layout(shared_layout).unwrap().entries[0].id;
        let child_transform =
            app.document.child_layout(shared_layout).unwrap().entries[0].transform;
        let child = parent.child(child_entry);
        app.selection
            .set_context(EditContext::Occurrence(parent.clone()));
        app.selection.select(SelectionTarget {
            occurrence: child.clone(),
            entity: SelectedEntity::Occurrence,
        });
        assert!(app.query_command_state("edit.duplicate_selection").enabled);

        let result = app.dispatch_command("edit.duplicate_selection", &CommandPayload::default());

        assert_eq!(CommandCompletion::Completed, result.result.completion);
        let copy = app.selected_occurrences()[0].clone();
        assert_eq!(2, copy.path.len());
        assert_eq!(parent.path[0], copy.path[0]);
        assert_ne!(child.path[1], copy.path[1]);
        assert!(app.document.resolve_occurrence(&copy).is_some());
        assert!(app.document.occurrence_edits(&parent).is_none());
        let private_layout = shared_layout;
        let private_entries = &app.document.child_layout(private_layout).unwrap().entries;
        assert_eq!(2, private_entries.len());
        let original_entry = private_entries
            .iter()
            .find(|entry| entry.id == child_entry)
            .unwrap();
        assert_eq!(original_entry.transform, child_transform);
        assert!(
            app.document
                .resolve_occurrence(&parent.child(original_entry.id))
                .is_some()
        );
        assert_eq!(
            2,
            app.document
                .child_layout(shared_layout)
                .unwrap()
                .entries
                .len()
        );
    }

    #[test]
    fn prototype_delete_command_refuses_in_use_definitions_and_deletes_orphans() {
        let mut app = test_app();
        app.run_command("geometry.add_cuboid");
        let instance = app.selected_occurrences()[0].clone();
        let definition = app
            .document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("definition_id".to_owned(), json!(definition.0));
        let before = app.document.clone();

        let refused = app.dispatch_command("object.delete_unused_definition", &payload);

        assert_eq!(CommandCompletion::Failed, refused.result.completion);
        assert!(refused.result.diagnostics[0].contains("still use it"));
        assert_eq!(before, app.document);
        app.run_command("edit.delete_selection");
        let deleted = app.dispatch_command("object.delete_unused_definition", &payload);
        assert_eq!(CommandCompletion::Completed, deleted.result.completion);
        assert!(app.document.definition(definition).is_none());
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
        assert!(app.preferences.black_model_lines);
        let initial_revision = app.document.revision();
        app.run_command("ui.update_preferences");
        assert!(app.preferences.panels_visible);
        assert_eq!(Some(&true), app.preferences.panel_open.get("ui_settings"));

        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("black_model_lines".into(), json!(false));
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
        assert!(!app.preferences.black_model_lines);
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_preferences_round_trip_through_atomic_config_file() {
        let directory = std::env::temp_dir().join(format!(
            "meshcraft-preferences-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("preferences.json");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(&path, br#"{"toolbar_icon_size": 40}"#).unwrap();
        let mut expected = load_preferences_file(&path);
        assert_eq!(40, expected.toolbar_icon_size);
        assert_eq!(Preferences::default().grid_extent, expected.grid_extent);
        assert!(
            expected.black_model_lines,
            "older preferences default to black lines"
        );

        expected.black_model_lines = false;
        expected.ui_text_scale = 1.5;
        expected.show_permanent_grid = false;
        expected.grid_extent = 48;
        super::write_native_preferences_file(&path, &expected).unwrap();

        assert_eq!(expected, load_preferences_file(&path));
        std::fs::remove_dir_all(directory).unwrap();
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
            ("edit.copy_selection", "clipboard"),
            ("object.create_from_selection", "prototype"),
            ("object.explode_selection", "explode"),
            ("object.place_selected_definition", "place object"),
            ("object.toggle_place_glue", "glue surface"),
            ("object.delete_unused_definition", "delete unused"),
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
    fn desktop_double_click_selects_connected_coplanar_faces_and_triple_clicks_connected_geometry()
    {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_planar_rectangle(
                "plane",
                [
                    Vec3::new(-2.0, 0.0, -2.0),
                    Vec3::new(2.0, 0.0, -2.0),
                    Vec3::new(2.0, 0.0, 2.0),
                    Vec3::new(-2.0, 0.0, 2.0),
                ],
            )
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        app.document
            .append_planar_mesh_to_definition(
                definition,
                vec![
                    Vec3::new(6.0, 0.0, 0.0),
                    Vec3::new(7.0, 0.0, 0.0),
                    Vec3::new(6.0, 0.0, 1.0),
                ],
                vec![[0, 1], [1, 2], [2, 0]],
                vec![[0, 1, 2]],
            )
            .unwrap();
        app.document
            .append_planar_mesh_to_definition(
                definition,
                vec![
                    Vec3::new(-2.0, 0.0, -2.0),
                    Vec3::new(2.0, 0.0, -2.0),
                    Vec3::new(0.0, 1.0, -2.0),
                ],
                vec![[0, 1], [1, 2], [2, 0]],
                vec![[0, 1, 2]],
            )
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let position = app.viewport.world_to_screen(rect, Vec3::new(0.7, 0.0, 0.4));
        for time_millis in [100, 200] {
            app.handle_viewport_click_input(
                rect,
                ViewportClick {
                    position,
                    modifiers: egui::Modifiers::default(),
                    time_millis,
                    pixels_per_point: 1.0,
                },
                None,
            );
        }
        assert_eq!(2, app.selection.summary().faces);
        assert_eq!(0, app.selection.summary().vertices);

        app.handle_viewport_click_input(
            rect,
            ViewportClick {
                position,
                modifiers: egui::Modifiers::default(),
                time_millis: 300,
                pixels_per_point: 1.0,
            },
            None,
        );
        let summary = app.selection.summary();
        assert_eq!(5, summary.vertices);
        assert_eq!(6, summary.edges);
        assert_eq!(3, summary.faces);
        assert_eq!(
            occurrence,
            app.selection.selected().first().unwrap().occurrence
        );
    }

    #[test]
    fn plain_click_toggles_a_selected_entity_off_without_dropping_other_selection() {
        let mut app = test_app();
        app.viewport.set_orthographic_view(OrthoView::Top);
        let first = app
            .document
            .add_line_segment(
                "first selected line",
                Vec3::new(-2.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 0.0),
            )
            .unwrap();
        let second = app
            .document
            .add_line_segment(
                "second selected line",
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
            )
            .unwrap();
        let segment_target = |document: &Document, occurrence: &OccurrenceRef| {
            let geometry = document.effective_geometry(occurrence).unwrap();
            SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Segment(*geometry.segments.keys().next().unwrap()),
            }
        };
        let first_target = segment_target(&app.document, &first);
        let second_target = segment_target(&app.document, &second);
        app.selection.apply(
            [first_target.clone(), second_target.clone()],
            SelectionMode::Replace,
        );

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let position = app
            .viewport
            .world_to_screen(rect, Vec3::new(-1.0, 0.0, 0.0));
        app.handle_viewport_click_at_point_with_selection(
            rect,
            position,
            None,
            None,
            egui::Modifiers::default(),
            1,
        );

        assert_eq!(1, app.selection.selected().len());
        assert!(app.selection.selected().contains(&second_target));
    }

    #[test]
    fn connected_selection_bridges_small_gaps_without_welding_or_selecting_distant_geometry() {
        let mut app = test_app();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let (edges, faces) = app
            .document
            .append_colored_indexed_mesh_to_definition(
                root,
                vec![
                    Vec3::ZERO,
                    Vec3::X,
                    Vec3::Z,
                    Vec3::new(1.006, 0., 0.),
                    Vec3::new(0.006, 0., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0.02, 0., 0.),
                    Vec3::new(1.02, 0., 0.),
                    Vec3::new(0.02, 0., -1.),
                ],
                vec![
                    ([0, 1], Rgba8::WHITE),
                    ([3, 5], Rgba8::WHITE),
                    ([6, 8], Rgba8::WHITE),
                ],
                vec![
                    ([0, 1, 2], Rgba8::WHITE),
                    ([3, 5, 4], Rgba8::WHITE),
                    ([6, 7, 8], Rgba8::WHITE),
                ],
            )
            .unwrap();
        let before = app.document.clone();
        let face = SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::Triangle(faces[0]),
        };
        assert_eq!(2, app.coplanar_faces(&face).len());
        let line = SelectionTarget {
            entity: SelectedEntity::Segment(edges[0]),
            ..face.clone()
        };
        assert_eq!(2, app.connected_line_segments(&line).len());
        let connected = app.connected_selection_component(&face);
        assert_eq!(
            2,
            connected
                .iter()
                .filter(|t| matches!(t.entity, SelectedEntity::Triangle(_)))
                .count()
        );
        assert_eq!(
            6,
            connected
                .iter()
                .filter(|t| matches!(t.entity, SelectedEntity::Vertex(_)))
                .count()
        );
        assert!(
            !connected
                .iter()
                .any(|t| t.entity == SelectedEntity::Triangle(faces[2]))
        );
        assert_eq!(before, app.document);
    }

    #[test]
    fn desktop_double_click_selects_connected_line_segments_and_triple_clicks_component() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_line_segment("first", Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        app.document
            .append_line_segment_to_definition(
                definition,
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 2.0),
            )
            .unwrap();
        app.document
            .append_line_segment_to_definition(
                definition,
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(6.0, 0.0, 0.0),
            )
            .unwrap();

        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let position = app.viewport.world_to_screen(rect, Vec3::new(1.0, 0.0, 0.0));
        for time_millis in [100, 200] {
            app.handle_viewport_click_input(
                rect,
                ViewportClick {
                    position,
                    modifiers: egui::Modifiers::default(),
                    time_millis,
                    pixels_per_point: 1.0,
                },
                None,
            );
        }
        let summary = app.selection.summary();
        assert_eq!(2, summary.edges);
        assert_eq!(0, summary.vertices);
        assert_eq!(0, summary.faces);

        app.selection.clear();
        for time_millis in [1_000, 1_100, 1_200] {
            app.handle_viewport_click_input(
                rect,
                ViewportClick {
                    position,
                    modifiers: egui::Modifiers::default(),
                    time_millis,
                    pixels_per_point: 1.0,
                },
                None,
            );
        }
        let summary = app.selection.summary();
        assert_eq!(3, summary.vertices);
        assert_eq!(2, summary.edges);
        assert_eq!(0, summary.faces);
    }

    #[test]
    fn viewport_click_modifiers_add_and_remove_typed_entity_targets() {
        let mut app = test_app();
        app.document
            .add_line_segment("line", Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let position = app.viewport.world_to_screen(rect, Vec3::ZERO);
        for (time_millis, modifiers) in [
            (100, egui::Modifiers::default()),
            (
                600,
                egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            (
                1_100,
                egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
        ] {
            app.handle_viewport_click_input(
                rect,
                ViewportClick {
                    position,
                    modifiers,
                    time_millis,
                    pixels_per_point: 1.0,
                },
                None,
            );
            match time_millis {
                100 | 600 => assert_eq!(1, app.selection.summary().edges),
                1_100 => assert_eq!(0, app.selection.summary().total()),
                _ => unreachable!(),
            }
        }
        assert!(app.status.starts_with("Selection cleared"));
    }

    #[test]
    fn double_click_on_group_geometry_enters_its_object_instance() {
        let mut app = test_app();
        let child_root = app
            .document
            .add_cuboid("child", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let group = app
            .document
            .create_object_from_root_occurrences(&[child_root], "group")
            .unwrap();
        let child = app
            .document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|occurrence| {
                occurrence.path.starts_with(&group.path) && occurrence.path.len() > group.path.len()
            })
            .unwrap();
        let child_transform = app
            .document
            .resolve_occurrence(&child)
            .unwrap()
            .world_transform;
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        let world_face_point = child_transform.transform_point(Vec3::new(1.0, 1.0, 1.0));
        let position = app.viewport.world_to_screen(rect, world_face_point);
        for time_millis in [500, 600] {
            app.handle_viewport_click_input(
                rect,
                ViewportClick {
                    position,
                    modifiers: egui::Modifiers::default(),
                    time_millis,
                    pixels_per_point: 1.0,
                },
                None,
            );
        }
        assert_eq!(EditContext::Occurrence(group), app.selection.context);
        assert!(app.status.starts_with("Editing object instance"));
    }

    #[test]
    fn command_palette_can_enter_and_exit_object_editing_context() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid("editable", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        app.set_selection([occurrence.clone()]);
        assert!(app.query_command_state("selection.edit_object").enabled);
        let entered = app.dispatch_command("selection.edit_object", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, entered.result.completion);
        assert_eq!(EditContext::Occurrence(occurrence), app.selection.context);
        assert!(app.query_command_state("selection.exit_context").enabled);

        let exited = app.dispatch_command("selection.exit_context", &CommandPayload::default());
        assert_eq!(CommandCompletion::Completed, exited.result.completion);
        assert_eq!(EditContext::Document, app.selection.context);
        assert!(!app.query_command_state("selection.exit_context").enabled);
    }

    #[test]
    fn construction_in_an_entered_instance_appends_to_its_definition_in_local_space() {
        let mut app = test_app();
        let occurrence = app
            .document
            .add_cuboid(
                "translated",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        let definition = app
            .document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let root_count = app.document.occurrence_refs().unwrap().len();
        app.selection
            .set_context(EditContext::Occurrence(occurrence.clone()));

        let targets = app
            .create_line_in_active_context(Vec3::new(10.0, 4.0, 0.0), Vec3::new(12.0, 4.0, 0.0))
            .unwrap();
        assert_eq!(1, targets.len());
        let target = targets[0].clone();
        app.selection
            .apply([target.clone()], super::SelectionMode::Replace);

        let geometry = app.document.effective_geometry(&occurrence).unwrap();
        let super::SelectedEntity::Segment(segment_id) = target.entity else {
            panic!("in-place line selection is an edge target");
        };
        let segment = &geometry.segments[&segment_id];
        let world_points = segment.endpoints.map(|reference| {
            let local = geometry.vertex_position(reference).unwrap();
            app.document
                .resolve_occurrence(&occurrence)
                .unwrap()
                .world_transform
                .transform_point(local)
        });
        assert_eq!(
            [Vec3::new(10.0, 4.0, 0.0), Vec3::new(12.0, 4.0, 0.0)],
            world_points
        );
        assert_eq!(13, geometry.segments.len());
        assert_eq!(root_count, app.document.occurrence_refs().unwrap().len());
        assert_eq!(
            definition,
            app.document
                .resolve_occurrence(&occurrence)
                .unwrap()
                .definition
                .id
        );
        assert_eq!(1, app.selection.summary().edges);
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
    fn empty_space_click_starts_a_world_volume_and_second_corner_finishes_it() {
        let mut app = test_app();
        let cube = app
            .document
            .add_cuboid("volume target", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0));
        // Pick two blank projected points that bracket the cube. The perspective
        // projection can place a construction-plane point over the visible model,
        // so find valid screen positions instead of assuming world-space corners
        // are visually blank.
        let mut candidates = Vec::new();
        for x_step in -8..=16 {
            for z_step in -8..=16 {
                let point = Vec3::new(f64::from(x_step) * 0.25, 0.0, f64::from(z_step) * 0.25);
                let position = app.viewport.world_to_screen(rect, point);
                if rect.contains(position)
                    && app
                        .viewport
                        .pick_selection_target(
                            rect,
                            &app.document,
                            position,
                            SnapScope::Document,
                            &app.selection_display_policy,
                        )
                        .is_none()
                {
                    candidates.push((point, position));
                }
            }
        }
        let (_, first_screen, _, second_screen) = candidates
            .iter()
            .find_map(|(first, first_screen)| {
                candidates
                    .iter()
                    .find(|(second, _)| {
                        first.x.min(second.x) <= 0.0
                            && first.x.max(second.x) >= 2.0
                            && first.z.min(second.z) <= 0.0
                            && first.z.max(second.z) >= 2.0
                    })
                    .map(|(second, second_screen)| (*first, *first_screen, *second, *second_screen))
            })
            .expect("blank screen points should bracket the test cube");

        app.handle_viewport_click_input(
            rect,
            ViewportClick {
                position: first_screen,
                modifiers: egui::Modifiers::default(),
                time_millis: 100,
                pixels_per_point: 1.0,
            },
            None,
        );
        assert!(app.pending_selection_volume.is_some());

        app.handle_viewport_click_input(
            rect,
            ViewportClick {
                position: second_screen,
                modifiers: egui::Modifiers::default(),
                time_millis: 500,
                pixels_per_point: 1.0,
            },
            None,
        );
        assert!(app.pending_selection_volume.is_none());
        assert!(app.selection.selected().iter().any(|target| {
            target.occurrence == cube && target.entity == SelectedEntity::Occurrence
        }));
        assert!(app.status.starts_with("Volume selection"));
    }

    #[test]
    fn screen_selection_gesture_maps_direction_and_alt_to_window_or_crossing() {
        let mut app = test_app();
        app.viewport.set_orthographic_view(OrthoView::Top);
        let line = app
            .document
            .add_line_segment(
                "gesture line",
                Vec3::new(-3.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            )
            .unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let center = app.viewport.world_to_screen(rect, Vec3::ZERO);
        let left = center + Vec2::new(-6.0, -6.0);
        let right = center + Vec2::new(6.0, 6.0);

        app.handle_selection_gesture(
            rect,
            SelectionGesture {
                start: left,
                end: right,
                modifiers: Modifiers::default(),
                pixels_per_point: 1.0,
            },
        );
        assert!(app.selection.selected().is_empty());
        assert!(app.status.starts_with("Window selection"));

        app.handle_selection_gesture(
            rect,
            SelectionGesture {
                start: right,
                end: left,
                modifiers: Modifiers::default(),
                pixels_per_point: 1.0,
            },
        );
        assert!(app.selection.selected().iter().any(|target| {
            target.occurrence == line && matches!(target.entity, SelectedEntity::Segment(_))
        }));
        assert!(app.status.starts_with("Crossing selection"));

        app.handle_selection_gesture(
            rect,
            SelectionGesture {
                start: right,
                end: left,
                modifiers: Modifiers {
                    alt: true,
                    ..Default::default()
                },
                pixels_per_point: 1.0,
            },
        );
        assert!(app.selection.selected().is_empty());
        assert!(app.status.starts_with("Window selection"));
    }

    #[test]
    fn released_viewport_marquee_selects_scene_objects_in_both_directions() {
        let mut app = test_app();
        app.viewport.set_orthographic_view(OrthoView::Top);
        let object = app
            .document
            .add_cuboid(
                "marquee cube",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(-1.0, 0.0, -1.0),
            )
            .unwrap();
        let context = egui::Context::default();
        run_workspace_frame(&mut app, &context, Vec::new());
        let resolved = app.document.resolve_occurrence(&object).unwrap();
        let geometry = app.document.effective_geometry(&object).unwrap();
        let projected = geometry
            .vertices
            .values()
            .map(|vertex| {
                app.viewport.world_to_screen(
                    app.viewport_rect,
                    resolved.world_transform.transform_point(vertex.position),
                )
            })
            .collect::<Vec<_>>();
        let bounds = Rect::from_min_max(
            Pos2::new(
                projected
                    .iter()
                    .map(|point| point.x)
                    .fold(f32::INFINITY, f32::min),
                projected
                    .iter()
                    .map(|point| point.y)
                    .fold(f32::INFINITY, f32::min),
            ),
            Pos2::new(
                projected
                    .iter()
                    .map(|point| point.x)
                    .fold(f32::NEG_INFINITY, f32::max),
                projected
                    .iter()
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max),
            ),
        );

        let drag = |app: &mut MeshcraftApp, context: &egui::Context, start: Pos2, end: Pos2| {
            run_workspace_frame(
                app,
                context,
                vec![
                    Event::PointerMoved(start),
                    Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::default(),
                    },
                ],
            );
            run_workspace_frame(app, context, vec![Event::PointerMoved(end)]);
            run_workspace_frame(
                app,
                context,
                vec![
                    Event::PointerMoved(end),
                    Event::PointerButton {
                        pos: end,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::default(),
                    },
                ],
            );
        };

        let padding = 8.0;
        drag(
            &mut app,
            &context,
            bounds.left_top() - Vec2::splat(padding),
            bounds.right_bottom() + Vec2::splat(padding),
        );
        assert!(
            app.selection.selected().iter().any(|target| {
                target.occurrence == object && target.entity == SelectedEntity::Occurrence
            }),
            "left-to-right release did not select the contained object: {}",
            app.status
        );

        app.selection.clear();
        let middle = bounds.center();
        drag(
            &mut app,
            &context,
            Pos2::new(bounds.right() + 20.0, bounds.top() - 20.0),
            middle,
        );
        assert!(
            app.selection.selected().iter().any(|target| {
                target.occurrence == object && target.entity == SelectedEntity::Occurrence
            }),
            "right-to-left release did not select the crossed object: {}",
            app.status
        );
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_startup_path_is_in_recent_history_and_preferences_reopen_it() {
        let path = project_fixture_path();
        let mut app = test_app();
        app.set_native_project_path(path.clone());
        app.set_native_project_path(path.clone());
        let canonical = std::fs::canonicalize(path).unwrap().display().to_string();
        assert_eq!(app.native_recent_projects(), [canonical]);
        let prefs = std::env::temp_dir().join(format!(
            "meshcraft-recent-preferences-{}.json",
            std::process::id()
        ));
        super::write_native_preferences_file(&prefs, &app.preferences).unwrap();
        let reopened = super::load_preferences_file(&prefs);
        assert_eq!(reopened.recent_projects, app.native_recent_projects());
        std::fs::remove_file(prefs).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn recent_project_preferences_deduplicate_limit_and_round_trip() {
        let mut preferences = Preferences::default();
        for index in 0..12 {
            preferences.record_recent_project(format!("project-{index}.octd"));
        }
        preferences.record_recent_project("project-5.octd".to_owned());

        assert_eq!(
            super::MAX_RECENT_PROJECTS,
            preferences.recent_projects.len()
        );
        assert_eq!("project-5.octd", preferences.recent_projects[0]);
        assert_eq!("project-11.octd", preferences.recent_projects[1]);
        assert_eq!(
            preferences.recent_projects.len(),
            preferences
                .recent_projects
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
        );
        let reopened: Preferences =
            serde_json::from_str(&serde_json::to_string(&preferences).unwrap()).unwrap();
        assert_eq!(preferences, reopened);

        let mut old_preferences: Preferences =
            serde_json::from_str(r#"{"recent_projects":["same.octd","same.octd","older.octd"]}"#)
                .unwrap();
        old_preferences.normalize_recent_projects();
        assert_eq!(
            vec!["same.octd".to_owned(), "older.octd".to_owned()],
            old_preferences.recent_projects
        );
        assert_eq!(
            "project.octd",
            super::recent_project_display_name("C:\\files\\project.octd")
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn open_recent_is_a_palette_command_that_opens_the_recent_project_picker() {
        let path = project_fixture_path();
        let canonical_path = std::fs::canonicalize(&path).unwrap();
        let mut app = test_app();
        app.preferences
            .record_recent_project(canonical_path.display().to_string());
        let context = egui::Context::default();
        open_palette(&mut app, &context);
        app.command_search = "file.open_recent".to_owned();
        assert!(
            app.matching_commands()
                .iter()
                .any(|command| command.descriptor.id.as_str() == "file.open_recent")
        );

        run_workspace_frame(
            &mut app,
            &context,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );

        assert!(app.recent_projects_dialog_open);
        assert!(!app.command_palette_open);
        assert_eq!(
            CommandCapability::Desktop,
            shell_commands()
                .get("file.open_recent")
                .unwrap()
                .descriptor
                .capability
        );

        let picker = run_workspace_frame(&mut app, &context, Vec::new());
        let project_item = text_center_in_row(&picker, "arch_frame.octd", 900.0);
        click_workspace_position(&mut app, &context, project_item);
        let job_id = app
            .pending_project_load
            .as_ref()
            .expect("palette picker should use the normal project loader")
            .job_id
            .clone();
        wait_for_project_load(&mut app, &job_id);
        assert_eq!(
            CommandCompletion::Completed,
            app.command_job_result(&job_id).unwrap().completion
        );
        assert_eq!(
            vec![canonical_path.display().to_string()],
            app.preferences.recent_projects
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_recent_menu_opens_the_selected_project_through_the_shared_loader() {
        let path = project_fixture_path();
        let mut app = test_app();
        app.preferences
            .record_recent_project(std::fs::canonicalize(&path).unwrap().display().to_string());
        let context = egui::Context::default();

        let menu_bar = run_workspace_frame(&mut app, &context, Vec::new());
        let file_menu = text_center_in_row(&menu_bar, "Files", 48.0);
        click_workspace_position(&mut app, &context, file_menu);
        let file_contents = run_workspace_frame(&mut app, &context, Vec::new());
        let recent_menu = text_center_in_row(&file_contents, "Recent Files", 400.0);
        click_workspace_position(&mut app, &context, recent_menu);
        let recent_contents = run_workspace_frame(&mut app, &context, Vec::new());
        let project_item = text_center_in_row(&recent_contents, "arch_frame.octd", 500.0);
        click_workspace_position(&mut app, &context, project_item);

        let job_id = app
            .pending_project_load
            .as_ref()
            .expect("recent entry should start the normal project loader")
            .job_id
            .clone();
        wait_for_project_load(&mut app, &job_id);
        assert_eq!(
            CommandCompletion::Completed,
            app.command_job_result(&job_id).unwrap().completion
        );
        assert_eq!(1, app.preferences.recent_projects.len());
        assert!(app.preferences.recent_projects[0].ends_with("arch_frame.octd"));
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
        app.choose_native_material("ansi-red").unwrap();
        app.run_command("geometry.add_cuboid");
        let result = app.dispatch_command("file.new", &CommandPayload::default());

        assert!(result.accepted);
        assert_eq!(1, app.document.definitions().count());
        app.resolve_document_replacement(ReplacementDecision::Discard);

        assert!(app.pending_replacement.is_none());
        assert_eq!(0, app.document.definitions().count());
        assert_eq!("New empty document.", app.status);
        assert_eq!(Rgba8::BLACK, app.current_outline_color());
        assert_eq!(Rgba8::ANSI_GREY, app.current_face_color());
        assert_eq!(20, app.document.materials().len());
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
    fn new_document_detaches_save_path_and_cannot_autosave_over_the_previous_file() {
        let mut app = test_app();
        let path = std::env::temp_dir().join(format!(
            "meshcraft-new-detaches-{}.octd",
            std::process::id()
        ));
        let original = std::fs::read(project_fixture_path()).unwrap();
        std::fs::write(&path, &original).unwrap();
        app.run_command("geometry.add_cuboid");
        app.saved_revision = app.document.revision();
        app.set_native_project_path(path.clone());
        app.run_command("file.new");
        assert!(app.current_project_path.is_none());
        assert!(!app.document_is_dirty());
        app.run_command("geometry.add_cuboid");
        let now = std::time::Instant::now();
        assert!(app.poll_autosave(now).is_none());
        assert!(app.poll_autosave(now + Duration::from_secs(3)).is_none());
        assert!(app.pending_project_save.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_file(path).unwrap();
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
    fn autosave_debounces_named_documents_without_saving_untitled_documents() {
        let path = std::env::temp_dir().join(format!(
            "meshcraft-autosave-{}-{}.octd",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = test_app();
        app.current_project_path = Some(path.clone());
        let start = std::time::Instant::now();

        app.run_command("geometry.add_cuboid");
        assert_eq!(Some(Duration::from_millis(1500)), app.poll_autosave(start));
        app.run_command("geometry.add_cuboid");
        assert_eq!(
            Some(Duration::from_millis(1500)),
            app.poll_autosave(start + Duration::from_millis(700))
        );
        assert!(app.pending_project_save.is_none());
        assert_eq!(
            Some(Duration::from_millis(1)),
            app.poll_autosave(start + Duration::from_millis(2199))
        );
        assert!(app.pending_project_save.is_none());
        assert_eq!(None, app.poll_autosave(start + Duration::from_millis(2200)));

        let job_id = app.pending_project_save.as_ref().unwrap().job_id.clone();
        wait_for_project_save(&mut app, &job_id);
        assert!(!app.document_is_dirty());
        assert!(
            read_project(&std::fs::read(&path).unwrap())
                .unwrap()
                .document
                .occurrence_refs()
                .unwrap()
                .len()
                > 1
        );
        std::fs::remove_file(path).unwrap();

        let mut untitled = test_app();
        let untitled_start = std::time::Instant::now();
        untitled.run_command("geometry.add_cuboid");
        assert_eq!(None, untitled.poll_autosave(untitled_start));
        assert_eq!(
            None,
            untitled.poll_autosave(untitled_start + Duration::from_secs(2))
        );
        assert!(untitled.pending_project_save.is_none());
        assert!(untitled.document_is_dirty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_atomic_replace_keeps_the_previous_project_and_cleans_temporary_file() {
        let directory = std::env::temp_dir().join(format!(
            "meshcraft-atomic-replace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("design.octd");
        let original = std::fs::read(project_fixture_path()).unwrap();
        std::fs::write(&path, &original).unwrap();

        let error =
            super::write_project_file_with_replace(&path, &Document::new(), None, |_, _| {
                Err(std::io::Error::other("injected replacement failure"))
            })
            .unwrap_err();

        assert!(error.contains("injected replacement failure"));
        assert_eq!(original, std::fs::read(&path).unwrap());
        assert_eq!(1, std::fs::read_dir(&directory).unwrap().count());
        std::fs::remove_dir_all(directory).unwrap();
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
        assert_eq!(
            vec![
                std::fs::canonicalize(project_fixture_path())
                    .unwrap()
                    .display()
                    .to_string()
            ],
            app.preferences.recent_projects
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
        app.preferences
            .record_recent_project("already-opened.octd".to_owned());
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
        assert_eq!(
            vec!["already-opened.octd".to_owned()],
            app.preferences
                .recent_projects
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
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
        app.preferences
            .record_recent_project("already-opened.octd".to_owned());
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
        assert_eq!(
            vec!["already-opened.octd".to_owned()],
            app.preferences
                .recent_projects
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        assert!(app.status.contains("cancelled"));
    }
}
