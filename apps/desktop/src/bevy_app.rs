//! Native Bevy renderer and UI shell for the desktop migration.
//!
//! The document remains the source of truth. This module builds compact category-wide GPU batches
//! from resolved geometry and refreshes them only after a document generation or revision change.
//! Spatially incremental geometry updates remain migration work.

use crate::viewport::{ConstructionGuide, ConstructionGuideKind, GridNormalAxis, SnapResult};
use crate::{MeshcraftApp, NativeUiPreferences};
use bevy::{
    asset::RenderAssetUsages,
    image::{Image as BevyImage, TextureAtlas, TextureAtlasLayout},
    input::keyboard::KeyboardInput,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollPixelsPerLine},
    input_focus::InputFocus,
    light::{GlobalAmbientLight, NotShadowCaster, ShadowFilteringMethod},
    mesh::{Mesh, PrimitiveTopology},
    pbr::StandardMaterial,
    picking::hover::Hovered,
    prelude::*,
    render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat},
    text::{EditableText, TextCursorStyle, TextEdit},
    transform::TransformSystems,
    ui::{InteractionDisabled, Pressed, UiScale},
    ui_widgets::{
        Activate, Button, Slider, SliderRange, SliderThumb, SliderValue, TextInput, TrackClick,
    },
    window::{PrimaryWindow, WindowPlugin, WindowResizeConstraints, WindowResolution},
};
use eframe::egui;
use meshcraft_core::commands::{CommandDescriptor, CommandPayload, CommandState};
use meshcraft_core::{
    document::{DefinitionId, Document, GeometryBundle, OccurrenceRef, Rgba8, VertexId},
    geometry::{Vec2 as ModelVec2, Vec3 as ModelVec3},
    selection::{
        EditContext, SelectedEntity, SelectionDisplayPolicy, SelectionKind, SelectionMode,
        SelectionTarget,
    },
};

#[derive(Resource)]
struct SceneDocument {
    document: Document,
    generation: u64,
    revision: u64,
    status: String,
    active_tool: String,
    selection_revision: u64,
    selected_objects: usize,
    selected_vertices: usize,
    selected_faces: usize,
    selected_edges: usize,
    selection_targets: Vec<SelectionTarget>,
    selected_vertex_positions: Vec<ModelVec3>,
    selected_object_count: usize,
    selected_object_definition_id: Option<DefinitionId>,
    selected_object_name: Option<String>,
    object_entries: Vec<NativeObjectEntry>,
    occurrences: usize,
    triangles: usize,
    segments: usize,
}

#[derive(Resource)]
struct NativeSelectionDisplayState {
    policy: SelectionDisplayPolicy,
    render_revision: u64,
}

#[derive(Clone, Copy)]
enum NativeSelectionValue {
    Total,
    Selected,
}

#[derive(Component)]
struct NativeSelectionValueLabel {
    kind: SelectionKind,
    value: NativeSelectionValue,
}

#[derive(Clone, Copy)]
enum NativeSelectionDisplayProperty {
    Display,
    Modifiable,
    Wireframe,
}

impl NativeSelectionDisplayProperty {
    fn command_name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Modifiable => "modifiable",
            Self::Wireframe => "wireframe",
        }
    }
}

#[derive(Component)]
struct NativeSelectionDisplayToggle {
    kind: SelectionKind,
    property: NativeSelectionDisplayProperty,
}

#[derive(Component)]
struct NativeSelectionToggleLabel {
    kind: SelectionKind,
    property: NativeSelectionDisplayProperty,
}

#[derive(Clone)]
struct NativeObjectEntry {
    occurrence: OccurrenceRef,
    name: String,
}

#[derive(Resource, Default)]
struct CommandPaletteState {
    open: bool,
    query: String,
    results: Vec<String>,
    selected_index: usize,
    pending_command: Option<String>,
    parameter_json: String,
    parameter_cursor: usize,
    parameter_error: Option<String>,
}

#[derive(Resource)]
struct CommandCatalog(Vec<CommandDescriptor>);

#[derive(Resource)]
struct ToolbarCatalog(Vec<(String, Vec<CommandDescriptor>)>);

struct NativeIconAtlas {
    image: Handle<BevyImage>,
    layout: Handle<TextureAtlasLayout>,
    indices: std::collections::BTreeMap<String, usize>,
}

#[derive(Component)]
struct DispatchCommand(String);

#[derive(Component)]
struct NativeToolbarWindow {
    index: usize,
    width: f32,
    height: f32,
    window_size: Vec2,
    item_count: usize,
    preferences_revision: u64,
    icon_size: u32,
    flow_top_down: bool,
}

#[derive(Component)]
struct NativeToolbarDragHandle(usize);

#[derive(Resource, Default)]
struct NativeToolbarDrag {
    active_index: Option<usize>,
    last_cursor: Option<Vec2>,
    moved: bool,
}

#[derive(Component)]
struct NativeInspectorPanel;

#[derive(Component, Clone, Copy)]
enum NativeInspectorText {
    Objects {
        generation: u64,
        revision: u64,
    },
    Object {
        generation: u64,
        revision: u64,
        selection_revision: u64,
    },
}

#[derive(Component)]
struct NativeModelSettingsEditorButton;

#[derive(Component)]
struct NativeModelSettingsApplyButton;

#[derive(Component)]
struct NativePolylineSettingsApplyButton;

#[derive(Clone, Copy, Eq, PartialEq)]
enum NativeModelSettingField {
    UnitName,
    UnitSize,
    GridSize,
    CircleSegments,
    SnapRadius,
    ArcMaxLength,
    DoubleLineSize,
    DoubleLineOffset,
}

impl NativeModelSettingField {
    fn key(self) -> &'static str {
        match self {
            Self::UnitName => "unit_name",
            Self::UnitSize => "unit_size",
            Self::GridSize => "grid_size",
            Self::CircleSegments => "circle_segments",
            Self::SnapRadius => "snap_radius",
            Self::ArcMaxLength => "arc_max_length",
            Self::DoubleLineSize => "double_line_size",
            Self::DoubleLineOffset => "double_line_offset",
        }
    }

    fn is_polyline(self) -> bool {
        matches!(
            self,
            Self::ArcMaxLength | Self::DoubleLineSize | Self::DoubleLineOffset
        )
    }
}

#[derive(Component)]
struct NativeModelSettingsInput {
    generation: u64,
    revision: u64,
    field: NativeModelSettingField,
}

#[derive(Resource, Clone)]
struct NativeUiPreferencesState {
    value: NativeUiPreferences,
    revision: u64,
}

#[derive(Component)]
struct NativeUiSettingsApplyButton;

#[derive(Clone, Copy, Eq, PartialEq)]
enum NativeUiSettingInputField {
    TextScale,
    ToolbarIconSize,
    GridExtent,
    NormalGridWidth,
    ThickGridWidth,
}

impl NativeUiSettingInputField {
    fn key(self) -> &'static str {
        match self {
            Self::TextScale => "ui_text_scale",
            Self::ToolbarIconSize => "toolbar_icon_size",
            Self::GridExtent => "grid_extent",
            Self::NormalGridWidth => "normal_grid_width",
            Self::ThickGridWidth => "thick_grid_width",
        }
    }
}

#[derive(Component)]
struct NativeUiSettingInput {
    field: NativeUiSettingInputField,
    revision: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum NativeUiSettingToggleField {
    AutoCollapseToolbars,
    PermanentGrid,
    ConstructionGuides,
}

impl NativeUiSettingToggleField {
    fn key(self) -> &'static str {
        match self {
            Self::AutoCollapseToolbars => "auto_collapse_toolbars",
            Self::PermanentGrid => "show_permanent_grid",
            Self::ConstructionGuides => "show_construction_guides",
        }
    }

    fn value(self, preferences: &NativeUiPreferences) -> bool {
        match self {
            Self::AutoCollapseToolbars => preferences.auto_collapse_toolbars,
            Self::PermanentGrid => preferences.show_permanent_grid,
            Self::ConstructionGuides => preferences.show_construction_guides,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::AutoCollapseToolbars => "Auto-collapse toolbars",
            Self::PermanentGrid => "Show permanent grid",
            Self::ConstructionGuides => "Show construction guides",
        }
    }
}

#[derive(Component)]
struct NativeUiSettingToggleButton(NativeUiSettingToggleField);

#[derive(Component)]
struct NativeUiSettingToggleLabel(NativeUiSettingToggleField);

#[derive(Clone, Copy, Eq, PartialEq)]
enum NativeUiSettingChoice {
    GridAxis(GridNormalAxis),
    ToolbarFlow(bool),
}

#[derive(Component)]
struct NativeUiSettingChoiceButton(NativeUiSettingChoice);

#[derive(Component)]
struct NativeUiSettingChoiceLabel(NativeUiSettingChoice);

#[derive(Component)]
struct NativeToolbarItems {
    item_count: usize,
}

#[derive(Component)]
struct NativeToolbarButton;

#[derive(Component)]
struct NativeToolbarIcon;

#[derive(Component)]
struct NativePermanentGrid {
    extent: u32,
    normal_axis: GridNormalAxis,
    visible: bool,
}

#[derive(Component)]
struct NativeRenameObjectButton;

#[derive(Component)]
struct NativeObjectNameInput {
    generation: u64,
    revision: u64,
    selection_revision: u64,
    definition_id: Option<DefinitionId>,
}

#[derive(Component)]
struct NativeObjectListContainer {
    generation: u64,
    revision: u64,
}

#[derive(Component)]
struct NativeObjectListRow {
    occurrence: OccurrenceRef,
}

#[derive(Resource, Default)]
struct NativeObjectListClick {
    entity: Option<Entity>,
    at_seconds: f64,
}

#[derive(Resource, Default)]
struct NativeToolsContextMenuState {
    open: bool,
    position: Vec2,
    block_world_input: bool,
}

#[derive(Resource, Default)]
struct NativeRightClickGesture {
    start: Option<Vec2>,
    dragged: bool,
}

#[derive(Component)]
struct NativeToolsContextMenuRoot;

#[derive(Component)]
struct ContextMenuCommand(String);

#[derive(Component)]
struct NativeUiInteraction;

#[derive(Component)]
struct CommandPaletteOverlay;

#[derive(Component)]
struct CommandPaletteQuery;

#[derive(Component)]
struct CommandPaletteParameterPanel;

#[derive(Component)]
struct CommandPaletteParameterHint;

#[derive(Component)]
struct CommandPaletteParameterInput;

#[derive(Component)]
struct CommandPaletteParameterError;

#[derive(Component)]
struct CommandPaletteResult {
    slot: usize,
    command_id: Option<String>,
    enabled: bool,
    checked: Option<bool>,
    disabled_reason: Option<String>,
}

#[derive(Component)]
struct CommandPaletteResultLabel(usize);

#[derive(Resource, Default)]
struct NativeSelectionDrag {
    start: Option<Vec2>,
    current: Vec2,
}

#[derive(Component)]
struct SelectionWindowOverlay;

#[derive(Component)]
struct SelectionWindowLabel;

#[derive(Component)]
struct NativeCursorDot;

#[derive(Component)]
struct NativeSelectedVertexDotLayer;

#[derive(Component)]
struct NativeSelectedVertexDot(usize);

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
struct LightingSettings {
    shadow: f32,
    directional: f32,
    ambient: f32,
    specular: f32,
    shadow_filter: ShadowFilter,
    use_csm: bool,
    shadow_bias: f32,
    shadow_normal_bias: f32,
}

impl Default for LightingSettings {
    fn default() -> Self {
        // Values match the visible K3D shader panel and LightingSettings defaults.
        Self {
            shadow: 0.59,
            directional: 0.73,
            ambient: 0.59,
            specular: 0.20,
            shadow_filter: ShadowFilter::Hardware2x2,
            use_csm: true,
            shadow_bias: 2500.0,
            shadow_normal_bias: 5620.0,
        }
    }
}

impl LightingSettings {
    fn ambient_brightness(self) -> f32 {
        // Bevy's ambient light uses physical units, while the reference shader uses a color
        // multiplier. This scale keeps the reference's 0.59 setting bright enough to lift shade.
        self.ambient.clamp(0.0, 1.0) * 240.0
    }

    fn bevy_shadow_depth_bias(self) -> f32 {
        DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS * (self.shadow_bias / 2500.0).max(0.0)
    }

    fn bevy_shadow_normal_bias(self) -> f32 {
        DirectionalLight::DEFAULT_SHADOW_NORMAL_BIAS * (self.shadow_normal_bias / 5620.0).max(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowFilter {
    Hardware2x2,
    Gaussian,
    Temporal,
}

impl ShadowFilter {
    const fn next(self) -> Self {
        match self {
            Self::Hardware2x2 => Self::Gaussian,
            Self::Gaussian => Self::Temporal,
            Self::Temporal => Self::Hardware2x2,
        }
    }

    const fn method(self) -> ShadowFilteringMethod {
        match self {
            Self::Hardware2x2 => ShadowFilteringMethod::Hardware2x2,
            Self::Gaussian => ShadowFilteringMethod::Gaussian,
            Self::Temporal => ShadowFilteringMethod::Temporal,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Hardware2x2 => "2x2",
            Self::Gaussian => "Gaussian",
            Self::Temporal => "Temporal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LightingChannel {
    Shadow,
    Directional,
    Ambient,
    Specular,
}

impl LightingChannel {
    const fn label(self) -> &'static str {
        match self {
            Self::Shadow => "Shadow",
            Self::Directional => "Directional",
            Self::Ambient => "Ambient",
            Self::Specular => "Specular",
        }
    }

    const fn value(self, settings: LightingSettings) -> f32 {
        match self {
            Self::Shadow => settings.shadow,
            Self::Directional => settings.directional,
            Self::Ambient => settings.ambient,
            Self::Specular => settings.specular,
        }
    }

    fn set_value(self, settings: &mut LightingSettings, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match self {
            Self::Shadow => settings.shadow = value,
            Self::Directional => settings.directional = value,
            Self::Ambient => settings.ambient = value,
            Self::Specular => settings.specular = value,
        }
    }
}

#[derive(Component)]
struct LightingSlider(LightingChannel);

#[derive(Component)]
struct LightingValueLabel(LightingChannel);

#[derive(Component)]
struct ShadowLight;

#[derive(Component)]
struct ShadowFilterButton;

#[derive(Component)]
struct CsmButton;

#[derive(Component)]
struct ShadowFilterLabel;

#[derive(Component)]
struct CsmLabel;

#[derive(Resource)]
struct OrbitCamera {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct SceneMesh;

#[derive(Resource, Default)]
struct SceneMeshEntities {
    front_faces: Option<Entity>,
    back_faces: Option<Entity>,
    segments: Option<Entity>,
    face_wireframe: Option<Entity>,
    selected_faces: Option<Entity>,
    selected_edges: Option<Entity>,
}

#[derive(Resource)]
struct RenderedSceneRevision {
    generation: u64,
    revision: u64,
    display_revision: u64,
}

#[derive(Resource)]
struct RenderedSelectionRevision {
    selection_revision: u64,
    display_revision: u64,
}

#[derive(Resource, Default)]
struct NativeViewportOverlay {
    cursor_snap: Option<SnapResult>,
    cursor_axes: Option<[ModelVec3; 3]>,
    cursor_screen: Option<Vec2>,
    guides: Vec<ConstructionGuide>,
    guides_visible: bool,
    grid_size: f64,
}

impl NativeViewportOverlay {
    fn clear_cursor(&mut self) {
        self.cursor_snap = None;
        self.cursor_axes = None;
        self.cursor_screen = None;
    }
}

struct SelectionMeshes {
    faces: Option<Mesh>,
    edges: Option<Mesh>,
}

struct SceneMeshes {
    front_faces: Option<Mesh>,
    back_faces: Option<Mesh>,
    segments: Option<Mesh>,
    face_wireframe: Option<Mesh>,
    target: Vec3,
    radius: f32,
}

/// Starts the native Bevy application with the requested window and optional imported document.
pub fn run_native(window_size: [f32; 2], maximized: bool, session: MeshcraftApp) {
    let document = session.document_snapshot();
    let generation = session.document_generation();
    let revision = session.document_revision();
    let (occurrences, triangles, segments) = scene_counts(&document);
    let selection = session.native_selection_summary();
    let selection_display = session.native_selection_display_policy();
    let selection_targets = session.native_selection_targets();
    let selected_vertex_positions = selected_vertex_positions(&document, &selection_targets);
    let (selected_object_count, selected_object_definition_id, selected_object_name) =
        selected_object_info(&document, &selection_targets);
    let object_entries = native_object_entries(&document, session.native_top_level_occurrences());
    let ui_preferences = session.native_ui_preferences();
    let mut app = App::new();
    app.insert_resource(SceneDocument {
        document,
        generation,
        revision,
        status: session.status_text().to_owned(),
        active_tool: session.active_tool_label().to_owned(),
        selection_revision: session.native_selection_revision(),
        selected_objects: selection.objects,
        selected_vertices: selection.vertices,
        selected_faces: selection.faces,
        selected_edges: selection.edges,
        selection_targets,
        selected_vertex_positions,
        selected_object_count,
        selected_object_definition_id,
        selected_object_name,
        object_entries,
        occurrences,
        triangles,
        segments,
    })
    .insert_resource(ClearColor(Color::srgb(0.78, 0.86, 0.94)))
    .insert_resource(CommandCatalog(MeshcraftApp::command_catalog()))
    .insert_resource(ToolbarCatalog(MeshcraftApp::native_toolbar_catalog()))
    .insert_resource(NativeUiPreferencesState {
        value: ui_preferences,
        revision: 0,
    })
    .insert_resource(NativeSelectionDisplayState {
        policy: selection_display,
        render_revision: 0,
    })
    .insert_resource(CommandPaletteState::default())
    .insert_resource(NativeSelectionDrag::default())
    .insert_resource(NativeToolbarDrag::default())
    .insert_resource(NativeToolsContextMenuState::default())
    .insert_resource(NativeRightClickGesture::default())
    .insert_resource(NativeObjectListClick::default())
    .insert_resource(NativeViewportOverlay::default())
    .insert_resource(LightingSettings::default())
    .insert_resource(OrbitCamera {
        target: Vec3::ZERO,
        distance: 24.0,
        yaw: 0.75,
        pitch: 0.55,
    })
    .insert_resource(StartupWindowState { maximized })
    .insert_non_send(session)
    .add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Meshcraft".to_owned(),
            resolution: WindowResolution::new(window_size[0] as u32, window_size[1] as u32),
            resize_constraints: WindowResizeConstraints {
                min_width: 900.0,
                min_height: 600.0,
                ..default()
            },
            ..default()
        }),
        ..default()
    }))
    .add_systems(Startup, (setup_scene, setup_ui, apply_window_state))
    .add_systems(
        Update,
        (
            (route_native_escape, route_native_guide_shortcuts).chain(),
            orbit_camera,
            toggle_command_palette,
            handle_command_palette_keyboard,
            refresh_command_palette,
            sync_command_palette_states,
            sync_command_palette_parameter_ui,
            sync_native_toolbar_visibility,
            sync_native_inspector_visibility,
            (
                sync_native_ui_preferences,
                sync_native_ui_setting_inputs,
                sync_native_ui_setting_labels,
                sync_native_ui_preference_presentation,
                update_native_permanent_grid,
            )
                .chain(),
            sync_native_context_menu_ui,
            drag_native_toolbars,
            sync_lighting_sliders,
            update_slider_thumbs,
            apply_lighting_settings,
            (
                sync_native_selection_display_state,
                sync_scene_document,
                sync_native_settings_sections,
                sync_native_selection_table,
                sync_native_model_settings_inputs,
                sync_native_object_name_input,
                sync_native_object_list,
            )
                .chain(),
            (
                sync_toolbar_button_states,
                sync_context_menu_command_states,
                update_scene_meshes,
                update_selection_meshes,
                refresh_status,
            )
                .chain(),
        )
            .chain(),
    )
    .add_systems(
        PostUpdate,
        (
            native_viewport_pointer,
            refresh_selection_window,
            draw_native_viewport_overlays,
            sync_native_selected_vertex_dots,
            sync_native_cursor_dot,
        )
            .chain()
            .after(TransformSystems::Propagate),
    );
    app.run();
}

#[derive(Resource)]
struct StartupWindowState {
    maximized: bool,
}

fn scene_counts(document: &Document) -> (usize, usize, usize) {
    let occurrences = document.scene_geometry_refs().unwrap_or_default();
    let mut triangles = 0;
    let mut segments = 0;
    for occurrence in &occurrences {
        if let Some(geometry) = document.effective_geometry(occurrence) {
            triangles += geometry.triangles.len();
            segments += geometry.segments.len();
        }
    }
    (occurrences.len().saturating_sub(1), triangles, segments)
}

fn sync_native_selection_display_state(
    session: NonSend<MeshcraftApp>,
    mut state: ResMut<NativeSelectionDisplayState>,
) {
    let policy = session.native_selection_display_policy();
    if policy == state.policy {
        return;
    }
    let affects_rendering = [
        SelectionKind::Object,
        SelectionKind::Face,
        SelectionKind::Edge,
    ]
    .into_iter()
    .any(|kind| {
        let before = state.policy.state(kind);
        let after = policy.state(kind);
        before.visible != after.visible
            || (kind == SelectionKind::Face && before.wireframe != after.wireframe)
    });
    state.policy = policy;
    if affects_rendering {
        state.render_revision = state.render_revision.wrapping_add(1);
    }
}

fn selected_vertex_positions(document: &Document, selection: &[SelectionTarget]) -> Vec<ModelVec3> {
    selection
        .iter()
        .filter_map(|target| {
            let SelectedEntity::Vertex(vertex_id) = &target.entity else {
                return None;
            };
            let resolved = document.resolve_occurrence(&target.occurrence)?;
            let geometry = document.effective_geometry(&target.occurrence)?;
            let vertex = geometry.vertices.get(vertex_id)?;
            let position = resolved.world_transform.transform_point(vertex.position);
            position.is_finite().then_some(position)
        })
        .collect()
}

fn selected_object_info(
    document: &Document,
    selection: &[SelectionTarget],
) -> (usize, Option<DefinitionId>, Option<String>) {
    let selected: Vec<_> = selection
        .iter()
        .filter(|target| target.entity == SelectedEntity::Occurrence)
        .collect();
    if selected.len() != 1 {
        return (selected.len(), None, None);
    }
    let Some(resolved) = document.resolve_occurrence(&selected[0].occurrence) else {
        return (selected.len(), None, None);
    };
    (
        1,
        Some(resolved.definition.id),
        Some(resolved.definition.name.clone()),
    )
}

fn native_object_entries(
    document: &Document,
    occurrences: Vec<OccurrenceRef>,
) -> Vec<NativeObjectEntry> {
    occurrences
        .into_iter()
        .filter_map(|occurrence| {
            let name = document
                .resolve_occurrence(&occurrence)?
                .definition
                .name
                .clone();
            Some(NativeObjectEntry { occurrence, name })
        })
        .collect()
}

fn object_selection_summary(scene: &SceneDocument) -> String {
    match scene.selected_object_count {
        0 => "No object selected".to_owned(),
        1 => format!(
            "1 object selected\nDefinition {}",
            scene.selected_object_definition_id.map_or_else(
                || "unavailable".to_owned(),
                |definition| definition.0.to_string()
            )
        ),
        count => format!("{count} objects selected"),
    }
}

fn create_native_icon_atlas(
    images: &mut Assets<BevyImage>,
    atlas_layouts: &mut Assets<TextureAtlasLayout>,
) -> NativeIconAtlas {
    let rgba = image::load_from_memory(crate::assets::icon_atlas_png())
        .expect("embedded toolbar icon atlas is a valid image")
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let dimensions = UVec2::new(width, height);
    let mut layout = TextureAtlasLayout::new_empty(dimensions);
    let mut indices = std::collections::BTreeMap::new();
    for (name, region) in crate::assets::icon_regions() {
        if region.x + region.width > width || region.y + region.height > height {
            continue;
        }
        let index = layout.add_texture(URect {
            min: UVec2::new(region.x, region.y),
            max: UVec2::new(region.x + region.width, region.y + region.height),
        });
        indices.insert(name.to_owned(), index);
    }
    let image = images.add(BevyImage::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    NativeIconAtlas {
        image,
        layout: atlas_layouts.add(layout),
        indices,
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_scene(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    ui_preferences: Res<NativeUiPreferencesState>,
    lighting: Res<LightingSettings>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scene_meshes = build_scene_meshes_with_policy(&scene.document, &selection_display.policy);
    let mut scene_entities = SceneMeshEntities::default();
    ambient_light.brightness = lighting.ambient_brightness();
    commands.insert_resource(OrbitCamera {
        target: scene_meshes.target,
        distance: scene_meshes.radius.max(8.0) * 2.2,
        yaw: 0.75,
        pitch: 0.55,
    });
    let grid = spawn_mesh(
        &mut commands,
        &mut meshes,
        &mut materials,
        build_oriented_grid_mesh(
            ui_preferences.value.grid_extent as i32,
            ui_preferences.value.grid_normal_axis,
        ),
        StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        },
        Transform::default(),
        false,
        false,
    );
    commands.entity(grid).insert((
        NativePermanentGrid {
            extent: ui_preferences.value.grid_extent,
            normal_axis: ui_preferences.value.grid_normal_axis,
            visible: ui_preferences.value.show_permanent_grid,
        },
        if ui_preferences.value.show_permanent_grid {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    ));
    if let Some(mesh) = scene_meshes.front_faces {
        scene_entities.front_faces = Some(spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            mesh,
            StandardMaterial {
                cull_mode: Some(Face::Back),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 1.0,
                reflectance: lighting.specular,
                ..default()
            },
            Transform::default(),
            true,
            true,
        ));
    }
    if let Some(mesh) = scene_meshes.back_faces {
        scene_entities.back_faces = Some(spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            mesh,
            StandardMaterial {
                cull_mode: Some(Face::Front),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 1.0,
                reflectance: lighting.specular,
                ..default()
            },
            Transform::default(),
            true,
            false,
        ));
    }
    if let Some(mesh) = scene_meshes.segments {
        scene_entities.segments = Some(spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            mesh,
            StandardMaterial {
                unlit: true,
                cull_mode: None,
                ..default()
            },
            Transform::default(),
            false,
            false,
        ));
    }
    if let Some(mesh) = scene_meshes.face_wireframe {
        scene_entities.face_wireframe = Some(spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            mesh,
            StandardMaterial {
                unlit: true,
                cull_mode: None,
                ..default()
            },
            Transform::default(),
            false,
            false,
        ));
    }

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(500.0, 500.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.82, 0.92),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.004, 0.0),
        NotShadowCaster,
    ));
    commands.spawn((
        ShadowLight,
        DirectionalLight {
            illuminance: 10_000.0 * lighting.shadow,
            shadow_maps_enabled: lighting.use_csm,
            shadow_depth_bias: lighting.bevy_shadow_depth_bias(),
            shadow_normal_bias: lighting.bevy_shadow_normal_bias(),
            ..default()
        },
        Transform::from_xyz(-8.0, 18.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 10_000.0 * lighting.directional,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, 18.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        lighting.shadow_filter.method(),
        Transform::from_xyz(14.0, 13.0, 18.0).looking_at(scene_meshes.target, Vec3::Y),
    ));
    commands.insert_resource(scene_entities);
    commands.insert_resource(RenderedSceneRevision {
        generation: scene.generation,
        revision: scene.revision,
        display_revision: selection_display.render_revision,
    });
    commands.insert_resource(RenderedSelectionRevision {
        selection_revision: u64::MAX,
        display_revision: u64::MAX,
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    mesh: Mesh,
    material: StandardMaterial,
    transform: Transform,
    is_scene_mesh: bool,
    casts_shadows: bool,
) -> Entity {
    let mut entity = commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        transform,
    ));
    if is_scene_mesh {
        entity.insert(SceneMesh);
    }
    if !casts_shadows {
        entity.insert(NotShadowCaster);
    }
    entity.id()
}

fn build_scene_meshes_with_policy(
    document: &Document,
    policy: &SelectionDisplayPolicy,
) -> SceneMeshes {
    let mut front_positions = Vec::new();
    let mut front_normals = Vec::new();
    let mut front_colors = Vec::new();
    let mut back_positions = Vec::new();
    let mut back_normals = Vec::new();
    let mut back_colors = Vec::new();
    let mut line_positions = Vec::new();
    let mut line_colors = Vec::new();
    let mut wireframe_positions = Vec::new();
    let mut wireframe_colors = Vec::new();
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    if let Ok(occurrences) = document.scene_geometry_refs() {
        for occurrence in occurrences {
            if !occurrence.is_scene_root() && !policy.can_draw(SelectionKind::Object) {
                continue;
            }
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            append_geometry(
                geometry,
                resolved.world_transform,
                &mut front_positions,
                &mut front_normals,
                &mut front_colors,
                &mut back_positions,
                &mut back_normals,
                &mut back_colors,
                &mut line_positions,
                &mut line_colors,
                &mut wireframe_positions,
                &mut wireframe_colors,
                &mut bounds_min,
                &mut bounds_max,
                policy,
            );
        }
    }

    let bounds_valid = bounds_min.is_finite() && bounds_max.is_finite();
    let target = if bounds_valid {
        (bounds_min + bounds_max) * 0.5
    } else {
        Vec3::ZERO
    };
    let radius = if bounds_valid {
        (bounds_max - bounds_min).length() * 0.5
    } else {
        8.0
    };
    SceneMeshes {
        front_faces: triangle_mesh(front_positions, front_normals, front_colors),
        back_faces: triangle_mesh(back_positions, back_normals, back_colors),
        segments: line_mesh(line_positions, line_colors),
        face_wireframe: line_mesh(wireframe_positions, wireframe_colors),
        target,
        radius,
    }
}

#[allow(clippy::too_many_arguments)]
fn append_geometry(
    geometry: &GeometryBundle,
    transform: meshcraft_core::geometry::Affine3,
    front_positions: &mut Vec<[f32; 3]>,
    front_normals: &mut Vec<[f32; 3]>,
    front_colors: &mut Vec<[f32; 4]>,
    back_positions: &mut Vec<[f32; 3]>,
    back_normals: &mut Vec<[f32; 3]>,
    back_colors: &mut Vec<[f32; 4]>,
    line_positions: &mut Vec<[f32; 3]>,
    line_colors: &mut Vec<[f32; 4]>,
    wireframe_positions: &mut Vec<[f32; 3]>,
    wireframe_colors: &mut Vec<[f32; 4]>,
    bounds_min: &mut Vec3,
    bounds_max: &mut Vec3,
    policy: &SelectionDisplayPolicy,
) {
    let draw_faces =
        policy.can_draw(SelectionKind::Face) && !policy.state(SelectionKind::Face).wireframe;
    let draw_wireframe =
        policy.can_draw(SelectionKind::Face) && policy.state(SelectionKind::Face).wireframe;
    let draw_edges = policy.can_draw(SelectionKind::Edge);
    let mut wireframe_edges = std::collections::BTreeSet::<(VertexId, VertexId)>::new();
    for triangle in geometry.triangles.values() {
        let Some(points) = triangle
            .vertices
            .map(|reference| geometry.vertex_position(reference))
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|points| <[ModelVec3; 3]>::try_from(points).ok())
            .map(|points| points.map(|point| transform.transform_point(point)))
        else {
            continue;
        };
        let Some(points) = points
            .map(to_bevy_vec3)
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|points| <[Vec3; 3]>::try_from(points).ok())
        else {
            continue;
        };
        let Some(normal) = (points[1] - points[0])
            .cross(points[2] - points[0])
            .try_normalize()
        else {
            continue;
        };
        if draw_faces {
            let front = rgba_to_color(triangle.color);
            let back = back_face_color(triangle.color);
            let edge_scale = (points[1] - points[0])
                .length()
                .max((points[2] - points[1]).length())
                .max((points[0] - points[2]).length());
            let back_offset = normal * -(edge_scale * 2.0e-5).max(1.0e-7);
            for index in 0..3 {
                front_positions.push(points[index].to_array());
                front_normals.push(normal.to_array());
                front_colors.push(front);
                let back_index = [0, 2, 1][index];
                back_positions.push((points[back_index] + back_offset).to_array());
                back_normals.push((-normal).to_array());
                back_colors.push(back);
            }
        } else if draw_wireframe {
            for edge in [[0, 1], [1, 2], [2, 0]] {
                let (left, right) = (triangle.vertices[edge[0]].id, triangle.vertices[edge[1]].id);
                let key = if left <= right {
                    (left, right)
                } else {
                    (right, left)
                };
                if wireframe_edges.insert(key) {
                    for point in [points[edge[0]], points[edge[1]]] {
                        wireframe_positions.push((point + normal * 0.001).to_array());
                        wireframe_colors.push([0.05, 0.06, 0.08, 1.0]);
                    }
                }
            }
        }
        for point in points {
            *bounds_min = bounds_min.min(point);
            *bounds_max = bounds_max.max(point);
        }
    }

    if !draw_edges {
        return;
    }
    for segment in geometry.segments.values() {
        let Some(points) = segment
            .endpoints
            .map(|reference| geometry.vertex_position(reference))
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|points| <[ModelVec3; 2]>::try_from(points).ok())
            .map(|points| points.map(|point| transform.transform_point(point)))
        else {
            continue;
        };
        let Some(points) = points
            .map(to_bevy_vec3)
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|points| <[Vec3; 2]>::try_from(points).ok())
        else {
            continue;
        };
        let color = rgba_to_color(segment.color);
        for point in points {
            line_positions.push(point.to_array());
            line_colors.push(color);
            *bounds_min = bounds_min.min(point);
            *bounds_max = bounds_max.max(point);
        }
    }
}

fn triangle_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
) -> Option<Mesh> {
    if positions.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    Some(mesh)
}

fn line_mesh(positions: Vec<[f32; 3]>, colors: Vec<[f32; 4]>) -> Option<Mesh> {
    if positions.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    Some(mesh)
}

fn build_oriented_grid_mesh(extent: i32, normal_axis: GridNormalAxis) -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let minor = [0.52, 0.60, 0.68, 1.0];
    let (u_axis, v_axis, normal) = match normal_axis {
        GridNormalAxis::X => (Vec3::Y, Vec3::Z, Vec3::X),
        GridNormalAxis::Y => (Vec3::X, Vec3::Z, Vec3::Y),
        GridNormalAxis::Z => (Vec3::X, Vec3::Y, Vec3::Z),
    };
    let (u_color, v_color, normal_color) = match normal_axis {
        GridNormalAxis::X => (
            [0.2, 0.67, 0.3, 1.0],
            [0.24, 0.40, 0.86, 1.0],
            [0.85, 0.26, 0.24, 1.0],
        ),
        GridNormalAxis::Y => (
            [0.85, 0.26, 0.24, 1.0],
            [0.24, 0.40, 0.86, 1.0],
            [0.2, 0.67, 0.3, 1.0],
        ),
        GridNormalAxis::Z => (
            [0.85, 0.26, 0.24, 1.0],
            [0.2, 0.67, 0.3, 1.0],
            [0.24, 0.40, 0.86, 1.0],
        ),
    };
    for step in -extent..=extent {
        let value = step as f32;
        let u_line_color = if step == 0 { u_color } else { minor };
        let v_line_color = if step == 0 { v_color } else { minor };
        push_line(
            &mut positions,
            &mut colors,
            u_axis * -(extent as f32) + v_axis * value,
            u_axis * extent as f32 + v_axis * value,
            u_line_color,
        );
        push_line(
            &mut positions,
            &mut colors,
            u_axis * value + v_axis * -(extent as f32),
            u_axis * value + v_axis * extent as f32,
            v_line_color,
        );
    }
    push_line(
        &mut positions,
        &mut colors,
        normal * -(extent as f32),
        normal * extent as f32,
        normal_color,
    );
    line_mesh(positions, colors).expect("the fixed grid contains lines")
}

fn push_line(
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    start: Vec3,
    end: Vec3,
    color: [f32; 4],
) {
    positions.extend([start.to_array(), end.to_array()]);
    colors.extend([color, color]);
}

fn rgba_to_color(color: Rgba8) -> [f32; 4] {
    [
        f32::from(color.red) / 255.0,
        f32::from(color.green) / 255.0,
        f32::from(color.blue) / 255.0,
        f32::from(color.alpha) / 255.0,
    ]
}

fn back_face_color(color: Rgba8) -> [f32; 4] {
    [
        f32::from(color.red) / 255.0 * 0.72,
        f32::from(color.green) / 255.0 * 0.76,
        (f32::from(color.blue) / 255.0 * 1.10).min(1.0),
        f32::from(color.alpha) / 255.0,
    ]
}

fn to_bevy_vec3(point: ModelVec3) -> Option<Vec3> {
    if !point.is_finite() {
        return None;
    }
    let converted = Vec3::new(point.x as f32, point.y as f32, point.z as f32);
    converted.is_finite().then_some(converted)
}

#[allow(clippy::too_many_arguments)]
fn setup_ui(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    ui_preferences: Res<NativeUiPreferencesState>,
    lighting: Res<LightingSettings>,
    catalog: Res<CommandCatalog>,
    toolbars: Res<ToolbarCatalog>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<BevyImage>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let icon_atlas = create_native_icon_atlas(&mut images, &mut atlas_layouts);
    let window_size = windows
        .single()
        .map(|window| Vec2::new(window.width(), window.height()))
        .unwrap_or(Vec2::new(1920.0, 1080.0));
    let root = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .id();
    commands.entity(root).with_children(|root| {
        root.spawn((
            NativeInspectorPanel,
            NativeUiInteraction,
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(10),
                left: px(10),
                right: px(318),
                height: px(42),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(8),
                padding: UiRect::axes(px(12), px(4)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.14, 0.17)),
        ))
        .with_children(|bar| {
            bar.spawn((Text::new("Meshcraft"), TextFont::from_font_size(18.0), TextColor::WHITE));
            bar.spawn((
                Text::new("Bevy 0.20 · GPU viewport"),
                TextFont::from_font_size(14.0),
                TextColor(Color::srgb(0.72, 0.80, 0.89)),
                Node { margin: UiRect::left(px(24)), ..default() },
            ));
            bar.spawn((
                Text::new("Right-drag: orbit · Wheel: zoom"),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.72, 0.80, 0.89)),
                Node { margin: UiRect::left(px(24)), ..default() },
            ));
        });
        let max_toolbar_width = (window_size.x - 370.0).clamp(220.0, 620.0);
        let icon_size = ui_preferences.value.toolbar_icon_size as f32;
        let max_vertical_rows = ((window_size.y - 150.0) / icon_size)
            .floor()
            .max(1.0) as usize;
        let mut toolbar_left = 10.0;
        let mut toolbar_top = 62.0;
        let mut toolbar_column_width: f32 = 0.0;
        for (index, (title, registered_commands)) in toolbars.0.iter().enumerate() {
            let item_count = registered_commands.len().max(1);
            let dimensions = native_toolbar_dimensions(
                item_count,
                icon_size,
                max_toolbar_width,
                max_vertical_rows,
                ui_preferences.value.toolbar_flow_top_down,
            );
            let width = dimensions.x;
            let height = dimensions.y;
            if toolbar_top + height > window_size.y - 36.0 && toolbar_top > 62.0 {
                toolbar_left += toolbar_column_width + 8.0;
                toolbar_top = 62.0;
                toolbar_column_width = 0.0;
            }
            if toolbar_left + width > window_size.x - 350.0 {
                toolbar_left = 10.0;
                toolbar_top = (toolbar_top + height + 5.0).min(window_size.y - height - 36.0);
                toolbar_column_width = 0.0;
            }
            let restored_position = ui_preferences
                .value
                .toolbar_positions
                .get(index)
                .copied()
                .flatten();
            let toolbar_position = native_toolbar_position(
                Vec2::new(toolbar_left, toolbar_top),
                restored_position,
                window_size,
                Vec2::new(width, height),
            );
            root.spawn((
                NativeToolbarWindow {
                    index,
                    width,
                    height,
                    window_size,
                    item_count,
                    preferences_revision: ui_preferences.revision,
                    icon_size: ui_preferences.value.toolbar_icon_size,
                    flow_top_down: ui_preferences.value.toolbar_flow_top_down,
                },
                NativeUiInteraction,
                Hovered::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(toolbar_position.x),
                    top: px(toolbar_position.y),
                    width: px(width),
                    height: px(height),
                    padding: UiRect::all(px(3)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.12, 0.15, 0.92)),
            ))
            .with_children(|toolbar| {
                toolbar
                    .spawn((
                        Button,
                        NativeUiInteraction,
                        NativeToolbarDragHandle(index),
                        Hovered::default(),
                        Node {
                            width: Val::Percent(100.0),
                            height: px(19),
                            padding: UiRect::horizontal(px(4)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
                    ))
                    .with_child((
                        Text::new(title),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::WHITE),
                    ));
                toolbar
                    .spawn((NativeToolbarItems { item_count }, Node {
                        width: Val::Percent(100.0),
                        height: px(height - 25.0),
                        flex_direction: if ui_preferences.value.toolbar_flow_top_down {
                            FlexDirection::Column
                        } else {
                            FlexDirection::Row
                        },
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(1),
                        row_gap: px(1),
                        ..default()
                    }))
                    .with_children(|buttons| {
                        for command in registered_commands {
                            spawn_tool_button(buttons, command, &icon_atlas, icon_size);
                        }
                    });
            });
            toolbar_top += height + 4.0;
            toolbar_column_width = toolbar_column_width.max(width);
        }
        root.spawn((
            NativeInspectorPanel,
            NativeUiInteraction,
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                top: px(62),
                right: px(10),
                width: px(330),
                bottom: px(38),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(10)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.13, 0.14, 0.16, 0.94)),
        ))
        .with_children(|panel| {
            inspector_header(panel, "Selection");
            spawn_native_selection_table(panel, &scene, &selection_display.policy);
            inspector_dynamic_section(
                panel,
                "Object",
                object_selection_summary(&scene),
                NativeInspectorText::Object {
                    generation: scene.generation,
                    revision: scene.revision,
                    selection_revision: scene.selection_revision,
                },
            );
            panel.spawn((
                Text::new("Name"),
                TextFont::from_font_size(13.0),
                TextColor::WHITE,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::left(px(4)),
                    ..default()
                },
            ));
            panel.spawn((
                TextInput,
                EditableText::new(scene.selected_object_name.as_deref().unwrap_or_default()),
                TextCursorStyle::default(),
                NativeUiInteraction,
                Hovered::default(),
                NativeObjectNameInput {
                    generation: scene.generation,
                    revision: scene.revision,
                    selection_revision: scene.selection_revision,
                    definition_id: scene.selected_object_definition_id,
                },
                Node {
                    width: Val::Percent(100.0),
                    height: px(27),
                    padding: UiRect::horizontal(px(6)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.16, 0.17, 0.18)),
                BorderColor::all(Color::srgb(0.08, 0.08, 0.08)),
                TextFont::from_font_size(13.0),
                TextColor::WHITE,
            ));
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    NativeRenameObjectButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(26),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
                ))
                .with_child((
                    Text::new("Rename Object"),
                    TextFont::from_font_size(12.0),
                    TextColor::WHITE,
                ))
                .observe(rename_selected_native_object);
            inspector_dynamic_section(
                panel,
                "Objects",
                objects_summary(scene.occurrences),
                NativeInspectorText::Objects {
                    generation: scene.generation,
                    revision: scene.revision,
                },
            );
            panel
                .spawn((
                    NativeObjectListContainer {
                        generation: scene.generation,
                        revision: scene.revision,
                    },
                    NativeUiInteraction,
                    Hovered::default(),
                    Node {
                        width: Val::Percent(100.0),
                        height: px(118),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        padding: UiRect::all(px(4)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.18, 0.19, 0.20)),
                ))
                .with_children(|rows| spawn_native_object_rows(rows, &scene));
            inspector_header(panel, "Model Settings");
            for (field, label, value) in native_model_setting_fields(&scene.document) {
                spawn_native_model_setting_input(
                    panel,
                    scene.generation,
                    scene.revision,
                    field,
                    label,
                    value,
                );
            }
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    NativeModelSettingsApplyButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(27),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.17, 0.37, 0.29)),
                ))
                .with_child((
                    Text::new("Apply Model Settings"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                ))
                .observe(apply_native_model_settings);
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    NativeModelSettingsEditorButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(26),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(px(7)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
                ))
                .with_child((
                    Text::new("Edit as JSON…"),
                    TextFont::from_font_size(13.0),
                    TextColor::WHITE,
                ))
                .observe(open_native_model_settings_editor);
            spawn_native_ui_settings(panel, &ui_preferences);
            inspector_section(panel, "Help / About", "Meshcraft · Bevy 0.20\nDesktop viewport · GPU rendering\nCommands: Ctrl+Shift+P");
            inspector_header(panel, "Polyline Settings");
            for (field, label, value) in native_polyline_setting_fields(&scene.document) {
                spawn_native_model_setting_input(
                    panel,
                    scene.generation,
                    scene.revision,
                    field,
                    label,
                    value,
                );
            }
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    NativePolylineSettingsApplyButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(27),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.17, 0.37, 0.29)),
                ))
                .with_child((
                    Text::new("Apply Polyline Settings"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                ))
                .observe(apply_native_polyline_settings);
            inspector_section(panel, "Vector Text", "Text  TEXT\nSize  1.000\nTracking  0.100\nLine spacing  1.250\nGlyph source  embedded:alphabet");
            inspector_section(panel, "Architecture - Walls", "Wall thickness  0.200\nWall height  2.700\nInclination  0°\nExterior / interior color");
            inspector_section(panel, "Architecture - Slabs", "Slab thickness  0.200\nTop / bottom / side color");
            inspector_section(panel, "Architecture - Stairs", "Stair height  2.700\nSteps  14\nSupport thickness  0.200");
            inspector_section(panel, "Architecture - Frames", "Frame depth  0.120\nFrame width  0.060\nGlazing  On");
            inspector_section(panel, "HVAC Settings", "Pipe diameter  0.200\nPipe sides  16\nDuct width  0.500\nDuct height  0.250");
            inspector_section(panel, "Hotspot Settings", "Operation  MOVE\nShape  CIRCLE\nAttached edges  0\nAttached faces  0");
            inspector_section(panel, "Random Tools", "Fuzzy offset strength\nSurface array copies  12\nScale / rotation fuzz  0");
            inspector_section(panel, "Tutorials", "Mode: Idle\nStep: 0/0\nBasic training");
            panel.spawn((
                Text::new("Lighting / Shadows"),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgb(0.84, 0.89, 0.94)),
                Node { width: Val::Percent(100.0), padding: UiRect::all(px(5)), ..default() },
                BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
            ));
            spawn_lighting_slider(panel, LightingChannel::Shadow, lighting.shadow);
            spawn_lighting_slider(panel, LightingChannel::Directional, lighting.directional);
            spawn_lighting_slider(panel, LightingChannel::Ambient, lighting.ambient);
            spawn_lighting_slider(panel, LightingChannel::Specular, lighting.specular);
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    ShadowFilterButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(26),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(px(7)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
                ))
                .with_child((
                    Text::new(format!("Shadow filter: {}", lighting.shadow_filter.label())),
                    TextFont::from_font_size(13.0),
                    TextColor::WHITE,
                    ShadowFilterLabel,
                ))
                .observe(cycle_shadow_filter);
            panel
                .spawn((
                    Button,
                    NativeUiInteraction,
                    Hovered::default(),
                    CsmButton,
                    Node {
                        width: Val::Percent(100.0),
                        height: px(26),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(px(7)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
                ))
                .with_child((
                    Text::new(format!("Cascaded shadows: {}", on_off(lighting.use_csm))),
                    TextFont::from_font_size(13.0),
                    TextColor::WHITE,
                    CsmLabel,
                ))
                .observe(toggle_csm);
        });
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                left: px(268),
                height: px(38),
                padding: UiRect::horizontal(px(10)),
                align_items: AlignItems::Center,
                ..default()
            },
            Button,
            NativeUiInteraction,
            Hovered::default(),
            BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
        ))
        .with_child((
            Text::new("Commands  Ctrl+Shift+P"),
            TextFont::from_font_size(14.0),
            TextColor::WHITE,
        ))
        .observe(open_command_palette);
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: percent(22.0),
                left: percent(35.0),
                width: percent(30.0),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.12, 0.15, 0.98)),
            CommandPaletteOverlay,
        ))
        .with_children(|palette| {
            palette.spawn((
                Text::new(format!("Command Palette · {} commands", catalog.0.len())),
                TextFont::from_font_size(20.0),
                TextColor::WHITE,
            ));
            palette.spawn((
                Text::new("> "),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgb(0.76, 0.84, 0.92)),
                CommandPaletteQuery,
            ));
            palette
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        padding: UiRect::all(px(7)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.15, 0.17, 0.20)),
                    CommandPaletteParameterPanel,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new(String::new()),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::srgb(0.76, 0.84, 0.92)),
                        CommandPaletteParameterHint,
                    ));
                    panel.spawn((
                        Text::new("{}"),
                        TextFont::from_font_size(14.0),
                        TextColor(Color::WHITE),
                        CommandPaletteParameterInput,
                    ));
                    panel.spawn((
                        Text::new(String::new()),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::srgb(0.98, 0.56, 0.48)),
                        CommandPaletteParameterError,
                    ));
                    panel.spawn((
                        Text::new("Edit JSON · Enter runs · Esc closes"),
                        TextFont::from_font_size(11.0),
                        TextColor(Color::srgb(0.64, 0.67, 0.70)),
                    ));
                });
            palette
                .spawn(Node {
                    width: Val::Percent(100.0),
                    max_height: px(520),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3),
                    overflow: Overflow::scroll_y(),
                    ..default()
                })
                .with_children(|results| {
                    for slot in 0..24 {
                        results
                            .spawn((
                                Button,
                                CommandPaletteResult {
                                    slot,
                                    command_id: None,
                                    enabled: false,
                                    checked: None,
                                    disabled_reason: None,
                                },
                                Node {
                                    width: Val::Percent(100.0),
                                    min_height: px(26),
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(px(7)),
                                    display: Display::None,
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
                            ))
                            .with_child((
                                Text::new(String::new()),
                                TextFont::from_font_size(12.0),
                                TextColor::WHITE,
                                CommandPaletteResultLabel(slot),
                            ))
                            .observe(dispatch_palette_command);
                    }
                });
        });
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                left: px(0),
                top: px(0),
                width: px(0),
                height: px(0),
                border: UiRect::all(px(2)),
                padding: UiRect::all(px(3)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.12, 0.42, 0.74, 0.12)),
            BorderColor::all(Color::srgb(0.14, 0.46, 0.92)),
            SelectionWindowOverlay,
        ))
        .with_child((
            Text::new("Window"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.14, 0.46, 0.92)),
            SelectionWindowLabel,
        ));
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(0),
                height: px(28),
                padding: UiRect::axes(px(8), px(4)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.14, 0.17)),
        ))
        .with_children(|status| {
            status.spawn((
                Text::new(format!("{}  |  {}", scene.status, scene.active_tool)),
                TextFont::from_font_size(13.0),
                TextColor::WHITE,
                StatusText,
            ));
        });
        let tool_groups = tool_context_groups(&catalog.0);
        root.spawn((
            NativeToolsContextMenuRoot,
            NativeUiInteraction,
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(250),
                height: px(460),
                padding: UiRect::all(px(6)),
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.10, 0.13, 0.98)),
        ))
        .with_children(|menu| {
            menu.spawn((
                Text::new("Tools"),
                TextFont::from_font_size(16.0),
                TextColor(Color::WHITE),
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::all(px(5)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
            ));
            for (group, group_commands) in &tool_groups {
                menu.spawn((
                    Text::new(group.clone()),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.70, 0.78, 0.86)),
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::axes(px(4), px(3)),
                        ..default()
                    },
                ));
                for command in group_commands {
                    let command_id = command.id.as_str().to_owned();
                    menu.spawn((
                        Button,
                        NativeUiInteraction,
                        ContextMenuCommand(command_id),
                        Hovered::default(),
                        Node {
                            width: Val::Percent(100.0),
                            height: px(25),
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(px(7)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.20, 0.22, 0.26)),
                    ))
                    .with_child((
                        Text::new(command.label.clone()),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::WHITE),
                    ))
                    .observe(dispatch_context_menu_command);
                }
            }
        });
        root.spawn((
            NativeSelectedVertexDotLayer,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ));
        root.spawn((
            NativeCursorDot,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(7),
                height: px(7),
                display: Display::None,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(Color::WHITE),
            BorderColor::all(Color::BLACK),
        ));
    });
}

fn inspector_section(parent: &mut ChildSpawnerCommands, title: &str, body: &str) {
    parent.spawn((
        Text::new(title.to_owned()),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.84, 0.89, 0.94)),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(5)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
    ));
    parent.spawn((
        Text::new(body.to_owned()),
        TextFont::from_font_size(13.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(7)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.30, 0.31, 0.32)),
    ));
}

fn inspector_header(parent: &mut ChildSpawnerCommands, title: &str) {
    parent.spawn((
        Text::new(title.to_owned()),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(0.84, 0.89, 0.94)),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(5)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.12, 0.38, 0.62)),
    ));
}

fn inspector_dynamic_section(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    body: String,
    marker: NativeInspectorText,
) {
    inspector_header(parent, title);
    parent.spawn((
        Text::new(body),
        TextFont::from_font_size(13.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(7)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.30, 0.31, 0.32)),
        marker,
    ));
}

fn native_model_setting_fields(
    document: &Document,
) -> [(NativeModelSettingField, &'static str, String); 5] {
    let settings = document.model_settings();
    [
        (
            NativeModelSettingField::UnitName,
            "Unit name",
            settings.unit_name.clone(),
        ),
        (
            NativeModelSettingField::UnitSize,
            "Unit size",
            format!("{:.4}", settings.unit_size),
        ),
        (
            NativeModelSettingField::GridSize,
            "Grid size",
            format!("{:.4}", settings.grid_size),
        ),
        (
            NativeModelSettingField::CircleSegments,
            "Circle segments",
            settings.circle_segments.to_string(),
        ),
        (
            NativeModelSettingField::SnapRadius,
            "Snap radius",
            format!("{:.1}", settings.snap_radius),
        ),
    ]
}

fn native_polyline_setting_fields(
    document: &Document,
) -> [(NativeModelSettingField, &'static str, String); 3] {
    let settings = document.model_settings();
    [
        (
            NativeModelSettingField::ArcMaxLength,
            "Arc max length",
            format!("{:.3}", settings.arc_max_length),
        ),
        (
            NativeModelSettingField::DoubleLineSize,
            "Double line size",
            format!("{:.3}", settings.double_line_size),
        ),
        (
            NativeModelSettingField::DoubleLineOffset,
            "Double line offset",
            format!("{:.3}", settings.double_line_offset),
        ),
    ]
}

fn spawn_native_model_setting_input(
    parent: &mut ChildSpawnerCommands,
    generation: u64,
    revision: u64,
    field: NativeModelSettingField,
    label: &str,
    value: String,
) {
    parent.spawn((
        Text::new(label.to_owned()),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::left(px(4)),
            ..default()
        },
    ));
    parent.spawn((
        TextInput,
        EditableText::new(value),
        TextCursorStyle::default(),
        NativeUiInteraction,
        Hovered::default(),
        NativeModelSettingsInput {
            generation,
            revision,
            field,
        },
        Node {
            width: Val::Percent(100.0),
            height: px(26),
            padding: UiRect::horizontal(px(6)),
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.16, 0.17, 0.18)),
        BorderColor::all(Color::srgb(0.08, 0.08, 0.08)),
        TextFont::from_font_size(13.0),
        TextColor(Color::WHITE),
    ));
}

fn native_ui_setting_fields(
    preferences: &NativeUiPreferences,
) -> [(NativeUiSettingInputField, &'static str, String); 5] {
    [
        (
            NativeUiSettingInputField::TextScale,
            "UI text size",
            format!("{:.2}", preferences.ui_text_scale),
        ),
        (
            NativeUiSettingInputField::ToolbarIconSize,
            "Toolbar icon/button size (px)",
            preferences.toolbar_icon_size.to_string(),
        ),
        (
            NativeUiSettingInputField::GridExtent,
            "Permanent grid extent (steps)",
            preferences.grid_extent.to_string(),
        ),
        (
            NativeUiSettingInputField::NormalGridWidth,
            "Normal line width (px)",
            format!("{:.2}", preferences.normal_grid_width),
        ),
        (
            NativeUiSettingInputField::ThickGridWidth,
            "Thick line width (px)",
            format!("{:.2}", preferences.thick_grid_width),
        ),
    ]
}

fn spawn_native_ui_settings(parent: &mut ChildSpawnerCommands, state: &NativeUiPreferencesState) {
    inspector_header(parent, "UI Settings");
    for (field, label, value) in native_ui_setting_fields(&state.value) {
        spawn_native_ui_setting_input(parent, field, label, value, state.revision);
        if field == NativeUiSettingInputField::TextScale {
            spawn_native_ui_setting_toggle(
                parent,
                NativeUiSettingToggleField::AutoCollapseToolbars,
                &state.value,
            );
        }
    }
    parent
        .spawn((
            Button,
            NativeUiInteraction,
            Hovered::default(),
            NativeUiSettingsApplyButton,
            Node {
                width: Val::Percent(100.0),
                height: px(27),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.17, 0.37, 0.29)),
        ))
        .with_child((
            Text::new("Apply UI Settings"),
            TextFont::from_font_size(12.0),
            TextColor(Color::WHITE),
        ))
        .observe(apply_native_ui_preferences);

    for field in [
        NativeUiSettingToggleField::PermanentGrid,
        NativeUiSettingToggleField::ConstructionGuides,
    ] {
        spawn_native_ui_setting_toggle(parent, field, &state.value);
    }

    parent.spawn((
        Text::new("Permanent grid normal"),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::left(px(4)),
            ..default()
        },
    ));
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: px(3),
            ..default()
        })
        .with_children(|choices| {
            for axis in [GridNormalAxis::Y, GridNormalAxis::X, GridNormalAxis::Z] {
                spawn_native_ui_choice_button(
                    choices,
                    NativeUiSettingChoice::GridAxis(axis),
                    axis.label(),
                    state.value.grid_normal_axis == axis,
                );
            }
        });

    parent.spawn((
        Text::new("Arrange toolbars"),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::left(px(4)),
            ..default()
        },
    ));
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: px(3),
            ..default()
        })
        .with_children(|choices| {
            spawn_native_ui_choice_button(
                choices,
                NativeUiSettingChoice::ToolbarFlow(false),
                "Flow L→R",
                !state.value.toolbar_flow_top_down,
            );
            spawn_native_ui_choice_button(
                choices,
                NativeUiSettingChoice::ToolbarFlow(true),
                "Flow T→D",
                state.value.toolbar_flow_top_down,
            );
        });
    parent.spawn((
        Text::new("Grid and toolbar changes apply immediately and save for the next launch. Pixel line widths remain in progress."),
        TextFont::from_font_size(11.0),
        TextColor(Color::srgb(0.72, 0.74, 0.78)),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(4)),
            ..default()
        },
    ));
}

fn spawn_native_ui_setting_toggle(
    parent: &mut ChildSpawnerCommands,
    field: NativeUiSettingToggleField,
    preferences: &NativeUiPreferences,
) {
    parent
        .spawn((
            Button,
            NativeUiInteraction,
            Hovered::default(),
            NativeUiSettingToggleButton(field),
            Node {
                width: Val::Percent(100.0),
                height: px(25),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(px(6)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
        ))
        .with_child((
            Text::new(format!(
                "{}: {}",
                field.label(),
                if field.value(preferences) {
                    "On"
                } else {
                    "Off"
                }
            )),
            TextFont::from_font_size(12.0),
            TextColor(Color::WHITE),
            NativeUiSettingToggleLabel(field),
        ))
        .observe(toggle_native_ui_setting);
}

fn spawn_native_ui_setting_input(
    parent: &mut ChildSpawnerCommands,
    field: NativeUiSettingInputField,
    label: &str,
    value: String,
    revision: u64,
) {
    parent.spawn((
        Text::new(label.to_owned()),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        Node {
            width: Val::Percent(100.0),
            padding: UiRect::left(px(4)),
            ..default()
        },
    ));
    parent.spawn((
        TextInput,
        EditableText::new(value),
        TextCursorStyle::default(),
        NativeUiInteraction,
        Hovered::default(),
        NativeUiSettingInput { field, revision },
        Node {
            width: Val::Percent(100.0),
            height: px(26),
            padding: UiRect::horizontal(px(6)),
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.16, 0.17, 0.18)),
        BorderColor::all(Color::srgb(0.08, 0.08, 0.08)),
        TextFont::from_font_size(13.0),
        TextColor(Color::WHITE),
    ));
}

fn spawn_native_ui_choice_button(
    parent: &mut ChildSpawnerCommands,
    choice: NativeUiSettingChoice,
    label: &str,
    selected: bool,
) {
    parent
        .spawn((
            Button,
            NativeUiInteraction,
            Hovered::default(),
            NativeUiSettingChoiceButton(choice),
            Node {
                flex_grow: 1.0,
                height: px(26),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgb(0.12, 0.38, 0.62)
            } else {
                Color::srgb(0.22, 0.24, 0.28)
            }),
        ))
        .with_child((
            Text::new(label.to_owned()),
            TextFont::from_font_size(11.0),
            TextColor(Color::WHITE),
            NativeUiSettingChoiceLabel(choice),
        ))
        .observe(update_native_ui_setting_choice);
}

fn spawn_native_object_rows(parent: &mut ChildSpawnerCommands, scene: &SceneDocument) {
    for entry in &scene.object_entries {
        let selected = scene.selection_targets.iter().any(|target| {
            target.entity == SelectedEntity::Occurrence && target.occurrence == entry.occurrence
        });
        parent
            .spawn((
                Button,
                NativeUiInteraction,
                Hovered::default(),
                NativeObjectListRow {
                    occurrence: entry.occurrence.clone(),
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: px(23),
                    padding: UiRect::horizontal(px(5)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.12, 0.38, 0.62)
                } else {
                    Color::srgb(0.23, 0.24, 0.25)
                }),
            ))
            .with_child((
                Text::new(entry.name.clone()),
                TextFont::from_font_size(13.0),
                TextColor(Color::WHITE),
            ))
            .observe(select_native_object_row);
    }
}

fn objects_summary(occurrences: usize) -> String {
    format!("{occurrences} object instances in the scene")
}

fn native_selection_kind_name(kind: SelectionKind) -> &'static str {
    match kind {
        SelectionKind::Vertex => "vertex",
        SelectionKind::Edge => "edge",
        SelectionKind::Face => "face",
        SelectionKind::Object => "object",
        SelectionKind::Voxel => "voxel",
        SelectionKind::Hotspot => "hotspot",
        SelectionKind::Dimension => "dimension",
        SelectionKind::Text => "text",
        SelectionKind::Wall => "wall",
        SelectionKind::Slab => "slab",
        SelectionKind::Stair => "stair",
        SelectionKind::Frame => "frame",
        SelectionKind::Hvac => "hvac",
    }
}

fn native_selection_kind_label(kind: SelectionKind) -> &'static str {
    match kind {
        SelectionKind::Vertex => "Vertices",
        SelectionKind::Edge => "Edges",
        SelectionKind::Face => "Faces",
        SelectionKind::Object => "Objects",
        SelectionKind::Voxel => "Voxels",
        SelectionKind::Hotspot => "Hotspots",
        SelectionKind::Dimension => "Dimensions",
        SelectionKind::Text => "Texts",
        SelectionKind::Wall => "Walls",
        SelectionKind::Slab => "Slabs",
        SelectionKind::Stair => "Stairs",
        SelectionKind::Frame => "Frames",
        SelectionKind::Hvac => "HVAC",
    }
}

fn native_selection_value(
    scene: &SceneDocument,
    kind: SelectionKind,
    value: NativeSelectionValue,
) -> usize {
    match (kind, value) {
        (SelectionKind::Edge, NativeSelectionValue::Total) => scene.segments,
        (SelectionKind::Face, NativeSelectionValue::Total) => scene.triangles,
        (SelectionKind::Object, NativeSelectionValue::Total) => scene.occurrences,
        (SelectionKind::Edge, NativeSelectionValue::Selected) => scene.selected_edges,
        (SelectionKind::Face, NativeSelectionValue::Selected) => scene.selected_faces,
        (SelectionKind::Object, NativeSelectionValue::Selected) => scene.selected_objects,
        _ => 0,
    }
}

fn native_selection_toggle_value(
    policy: &SelectionDisplayPolicy,
    kind: SelectionKind,
    property: NativeSelectionDisplayProperty,
) -> bool {
    let state = policy.state(kind);
    match property {
        NativeSelectionDisplayProperty::Display => state.visible,
        NativeSelectionDisplayProperty::Modifiable => state.unlocked,
        NativeSelectionDisplayProperty::Wireframe => state.wireframe,
    }
}

fn native_selection_toggle_available(
    policy: &SelectionDisplayPolicy,
    kind: SelectionKind,
    property: NativeSelectionDisplayProperty,
) -> bool {
    !matches!(property, NativeSelectionDisplayProperty::Modifiable) || policy.state(kind).visible
}

fn spawn_native_selection_table(
    parent: &mut ChildSpawnerCommands,
    scene: &SceneDocument,
    policy: &SelectionDisplayPolicy,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            padding: UiRect::all(px(5)),
            flex_direction: FlexDirection::Column,
            row_gap: px(1),
            ..default()
        })
        .with_children(|table| {
            table
                .spawn(Node {
                    width: Val::Percent(100.0),
                    height: px(20),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    spawn_selection_column_header(row, "Type", Val::Auto);
                    spawn_selection_column_header(row, "T", px(32));
                    spawn_selection_column_header(row, "S", px(28));
                    spawn_selection_column_header(row, "D", px(25));
                    spawn_selection_column_header(row, "M", px(25));
                    spawn_selection_column_header(row, "W", px(25));
                });

            for kind in [
                SelectionKind::Edge,
                SelectionKind::Face,
                SelectionKind::Voxel,
                SelectionKind::Hotspot,
                SelectionKind::Object,
                SelectionKind::Dimension,
                SelectionKind::Text,
                SelectionKind::Wall,
                SelectionKind::Slab,
                SelectionKind::Stair,
                SelectionKind::Frame,
            ] {
                table
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        height: px(22),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new(native_selection_kind_label(kind)),
                            TextFont::from_font_size(13.0),
                            TextColor(Color::WHITE),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        for value in [NativeSelectionValue::Total, NativeSelectionValue::Selected] {
                            row.spawn((
                                Text::new(native_selection_value(scene, kind, value).to_string()),
                                TextFont::from_font_size(13.0),
                                TextColor(Color::WHITE),
                                Node {
                                    width: match value {
                                        NativeSelectionValue::Total => px(32),
                                        NativeSelectionValue::Selected => px(28),
                                    },
                                    ..default()
                                },
                                NativeSelectionValueLabel { kind, value },
                            ));
                        }
                        for property in [
                            NativeSelectionDisplayProperty::Display,
                            NativeSelectionDisplayProperty::Modifiable,
                            NativeSelectionDisplayProperty::Wireframe,
                        ] {
                            let available =
                                !matches!(property, NativeSelectionDisplayProperty::Wireframe)
                                    || kind == SelectionKind::Face;
                            if available {
                                spawn_native_selection_toggle(
                                    row,
                                    kind,
                                    property,
                                    native_selection_toggle_value(policy, kind, property),
                                );
                            } else {
                                row.spawn((
                                    Text::new("—"),
                                    TextFont::from_font_size(12.0),
                                    TextColor(Color::srgb(0.56, 0.57, 0.59)),
                                    Node {
                                        width: px(25),
                                        ..default()
                                    },
                                ));
                            }
                        }
                    });
            }
        });
}

fn spawn_selection_column_header(row: &mut ChildSpawnerCommands, text: &str, width: Val) {
    row.spawn((
        Text::new(text),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.83, 0.84, 0.86)),
        Node {
            width,
            flex_grow: if text == "Type" { 1.0 } else { 0.0 },
            ..default()
        },
    ));
}

fn spawn_native_selection_toggle(
    row: &mut ChildSpawnerCommands,
    kind: SelectionKind,
    property: NativeSelectionDisplayProperty,
    enabled: bool,
) {
    row.spawn((
        Button,
        NativeUiInteraction,
        Hovered::default(),
        NativeSelectionDisplayToggle { kind, property },
        Node {
            width: px(23),
            height: px(18),
            margin: UiRect::horizontal(px(1)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(if enabled {
            Color::srgb(0.18, 0.37, 0.52)
        } else {
            Color::srgb(0.20, 0.21, 0.22)
        }),
    ))
    .with_child((
        Text::new(if enabled { "✓" } else { "" }),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
        NativeSelectionToggleLabel { kind, property },
    ))
    .observe(dispatch_native_selection_display_toggle);
}

fn sync_native_selection_table(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    mut values: Query<(&mut Text, &NativeSelectionValueLabel), Without<NativeSelectionToggleLabel>>,
    mut labels: Query<(&mut Text, &NativeSelectionToggleLabel), Without<NativeSelectionValueLabel>>,
    mut toggles: Query<(
        Entity,
        &NativeSelectionDisplayToggle,
        Option<&Button>,
        &mut BackgroundColor,
    )>,
) {
    if !scene.is_changed() && !selection_display.is_changed() {
        return;
    }
    for (mut text, value) in &mut values {
        text.0 = native_selection_value(&scene, value.kind, value.value).to_string();
    }
    for (mut text, label) in &mut labels {
        let enabled =
            native_selection_toggle_value(&selection_display.policy, label.kind, label.property);
        text.0 = if enabled { "✓" } else { "" }.to_owned();
    }
    for (entity, toggle, button, mut background) in &mut toggles {
        let available = native_selection_toggle_available(
            &selection_display.policy,
            toggle.kind,
            toggle.property,
        );
        if available && button.is_none() {
            commands.entity(entity).insert(Button);
        } else if !available && button.is_some() {
            commands.entity(entity).remove::<Button>();
        }
        let enabled =
            native_selection_toggle_value(&selection_display.policy, toggle.kind, toggle.property);
        background.0 = if !available {
            Color::srgb(0.15, 0.16, 0.17)
        } else if enabled {
            Color::srgb(0.18, 0.37, 0.52)
        } else {
            Color::srgb(0.20, 0.21, 0.22)
        };
    }
}

fn sync_native_settings_sections(
    scene: Res<SceneDocument>,
    mut sections: Query<(&mut Text, &mut NativeInspectorText)>,
) {
    for (mut text, mut marker) in &mut sections {
        let summary = match *marker {
            NativeInspectorText::Object {
                generation,
                revision,
                selection_revision,
            } if generation != scene.generation
                || revision != scene.revision
                || selection_revision != scene.selection_revision =>
            {
                *marker = NativeInspectorText::Object {
                    generation: scene.generation,
                    revision: scene.revision,
                    selection_revision: scene.selection_revision,
                };
                object_selection_summary(&scene)
            }
            NativeInspectorText::Objects {
                generation,
                revision,
            } if generation != scene.generation || revision != scene.revision => {
                *marker = NativeInspectorText::Objects {
                    generation: scene.generation,
                    revision: scene.revision,
                };
                objects_summary(scene.occurrences)
            }
            _ => continue,
        };
        text.0 = summary;
    }
}

fn spawn_lighting_slider(parent: &mut ChildSpawnerCommands, channel: LightingChannel, value: f32) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            height: px(24),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(6),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(format!("{}: {value:.2}", channel.label())),
                TextFont::from_font_size(12.0),
                TextColor(Color::WHITE),
                Node {
                    width: px(94),
                    ..default()
                },
                LightingValueLabel(channel),
            ));
            row.spawn((
                Slider {
                    track_click: TrackClick::Snap,
                    ..default()
                },
                NativeUiInteraction,
                Hovered::default(),
                SliderValue(value),
                SliderRange::new(0.0, 1.0),
                LightingSlider(channel),
                Node {
                    width: px(190),
                    height: px(16),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ))
            .with_children(|slider| {
                slider.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: px(4),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.06, 0.07, 0.08)),
                ));
                slider
                    .spawn(Node {
                        display: Display::Flex,
                        position_type: PositionType::Absolute,
                        left: px(0),
                        right: px(12),
                        top: px(0),
                        bottom: px(0),
                        ..default()
                    })
                    .with_child((
                        SliderThumb,
                        Node {
                            display: Display::Flex,
                            width: px(12),
                            height: px(12),
                            position_type: PositionType::Absolute,
                            left: percent(value * 100.0),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.38, 0.72, 0.98)),
                    ));
            });
        });
}

fn spawn_tool_button(
    parent: &mut ChildSpawnerCommands,
    command: &CommandDescriptor,
    icon_atlas: &NativeIconAtlas,
    icon_size: f32,
) {
    let command_id = command.id.as_str().to_owned();
    let icon_index = command
        .presentation
        .icon_key
        .as_ref()
        .and_then(|key| icon_atlas.indices.get(key))
        .copied();
    parent
        .spawn((
            Button,
            NativeUiInteraction,
            Hovered::default(),
            DispatchCommand(command_id.clone()),
            NativeToolbarButton,
            Node {
                width: px(icon_size - 1.0),
                height: px(icon_size - 1.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.22, 0.24, 0.28)),
        ))
        .with_children(|button| {
            if let Some(index) = icon_index {
                button.spawn((
                    ImageNode::from_atlas_image(
                        icon_atlas.image.clone(),
                        TextureAtlas {
                            layout: icon_atlas.layout.clone(),
                            index,
                        },
                    ),
                    NativeToolbarIcon,
                    Node {
                        width: px(icon_size - 4.0),
                        height: px(icon_size - 4.0),
                        ..default()
                    },
                ));
            } else {
                button.spawn((
                    Text::new(command.label.chars().take(2).collect::<String>()),
                    TextFont::from_font_size(9.0),
                    TextColor::WHITE,
                ));
            }
        })
        .observe(dispatch_toolbar_command);
}

fn sync_native_toolbar_visibility(
    session: NonSend<MeshcraftApp>,
    mut toolbars: Query<(&NativeToolbarWindow, &mut Node)>,
) {
    let visibility = session.native_toolbar_visibility();
    for (toolbar, mut node) in &mut toolbars {
        let shown = visibility.get(toolbar.index).copied().unwrap_or(false);
        node.display = if shown { Display::Flex } else { Display::None };
    }
}

fn sync_native_ui_preference_presentation(
    preferences: Res<NativeUiPreferencesState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    mut toolbars: Query<(&mut NativeToolbarWindow, &mut Node)>,
    mut toolbar_buttons: Query<&mut Node, With<NativeToolbarButton>>,
    mut toolbar_icons: Query<&mut Node, With<NativeToolbarIcon>>,
    mut toolbar_items: Query<(&NativeToolbarItems, &mut Node)>,
) {
    if (ui_scale.0 - preferences.value.ui_text_scale).abs() > f32::EPSILON {
        ui_scale.0 = preferences.value.ui_text_scale;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let icon_size = preferences.value.toolbar_icon_size;
    let max_toolbar_width = (window.width() - 370.0).clamp(220.0, 620.0);
    let window_size = Vec2::new(window.width(), window.height());
    let max_vertical_rows = ((window.height() - 150.0) / icon_size as f32)
        .floor()
        .max(1.0) as usize;
    for (mut toolbar, mut node) in &mut toolbars {
        let resized = toolbar.window_size != window_size;
        if toolbar.preferences_revision == preferences.revision && !resized {
            continue;
        }
        if resized
            || toolbar.icon_size != icon_size
            || toolbar.flow_top_down != preferences.value.toolbar_flow_top_down
        {
            let dimensions = native_toolbar_dimensions(
                toolbar.item_count,
                icon_size as f32,
                max_toolbar_width,
                max_vertical_rows,
                preferences.value.toolbar_flow_top_down,
            );
            toolbar.width = dimensions.x;
            toolbar.height = dimensions.y;
            toolbar.icon_size = icon_size;
            toolbar.flow_top_down = preferences.value.toolbar_flow_top_down;
            node.width = px(toolbar.width);
            node.height = px(toolbar.height);
        }
        node.left = match node.left {
            Val::Px(left) => px(left.clamp(0.0, (window.width() - toolbar.width).max(0.0))),
            other => other,
        };
        node.top = match node.top {
            Val::Px(top) => px(top.clamp(0.0, (window.height() - toolbar.height).max(0.0))),
            other => other,
        };
        toolbar.window_size = window_size;
        toolbar.preferences_revision = preferences.revision;
    }
    for mut node in &mut toolbar_buttons {
        node.width = px(icon_size as f32 - 1.0);
        node.height = px(icon_size as f32 - 1.0);
    }
    for mut node in &mut toolbar_icons {
        node.width = px(icon_size as f32 - 4.0);
        node.height = px(icon_size as f32 - 4.0);
    }
    for (items, mut node) in &mut toolbar_items {
        node.flex_direction = if preferences.value.toolbar_flow_top_down {
            FlexDirection::Column
        } else {
            FlexDirection::Row
        };
        let rows = if preferences.value.toolbar_flow_top_down {
            items.item_count.min(max_vertical_rows).max(1)
        } else {
            let columns = (((max_toolbar_width - 12.0) / icon_size as f32).floor() as usize).max(1);
            items.item_count.div_ceil(columns)
        };
        node.height = px(rows as f32 * icon_size as f32);
    }
}

fn update_native_permanent_grid(
    mut commands: Commands,
    preferences: Res<NativeUiPreferencesState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut grids: Query<(Entity, &mut NativePermanentGrid, &mut Visibility)>,
) {
    for (entity, mut grid, mut visibility) in &mut grids {
        if grid.extent != preferences.value.grid_extent
            || grid.normal_axis != preferences.value.grid_normal_axis
        {
            commands
                .entity(entity)
                .insert(Mesh3d(meshes.add(build_oriented_grid_mesh(
                    preferences.value.grid_extent as i32,
                    preferences.value.grid_normal_axis,
                ))));
            grid.extent = preferences.value.grid_extent;
            grid.normal_axis = preferences.value.grid_normal_axis;
        }
        if grid.visible != preferences.value.show_permanent_grid {
            *visibility = if preferences.value.show_permanent_grid {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            grid.visible = preferences.value.show_permanent_grid;
        }
    }
}

fn sync_native_inspector_visibility(
    session: NonSend<MeshcraftApp>,
    mut inspectors: Query<&mut Node, With<NativeInspectorPanel>>,
) {
    let display = if session.native_panels_visible() {
        Display::Flex
    } else {
        Display::None
    };
    for mut inspector in &mut inspectors {
        inspector.display = display;
    }
}

fn sync_native_context_menu_ui(
    context_menu: Res<NativeToolsContextMenuState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut menus: Query<&mut Node, With<NativeToolsContextMenuRoot>>,
) {
    let window_size = windows
        .single()
        .map(|window| Vec2::new(window.width(), window.height()))
        .unwrap_or(Vec2::new(1920.0, 1080.0));
    let left = context_menu
        .position
        .x
        .clamp(0.0, (window_size.x - 250.0).max(0.0));
    let top = context_menu
        .position
        .y
        .clamp(0.0, (window_size.y - 460.0).max(0.0));
    for mut menu in &mut menus {
        menu.display = if context_menu.open {
            Display::Flex
        } else {
            Display::None
        };
        menu.left = px(left);
        menu.top = px(top);
    }
}

fn drag_native_toolbars(
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut drag: ResMut<NativeToolbarDrag>,
    mut session: NonSendMut<MeshcraftApp>,
    handles: Query<(&NativeToolbarDragHandle, Has<Pressed>)>,
    mut toolbars: Query<(&NativeToolbarWindow, &mut Node)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let cursor = window.cursor_position();
    if mouse.just_pressed(MouseButton::Left) {
        drag.active_index = handles
            .iter()
            .find_map(|(handle, pressed)| pressed.then_some(handle.0));
        drag.last_cursor = cursor;
        drag.moved = false;
    }
    if mouse.pressed(MouseButton::Left)
        && let (Some(index), Some(position), Some(previous)) =
            (drag.active_index, cursor, drag.last_cursor)
    {
        let delta = position - previous;
        if delta != Vec2::ZERO {
            for (toolbar, mut node) in &mut toolbars {
                if toolbar.index != index {
                    continue;
                }
                let Val::Px(left) = node.left else {
                    continue;
                };
                let Val::Px(top) = node.top else {
                    continue;
                };
                node.left =
                    px((left + delta.x).clamp(0.0, (window.width() - toolbar.width).max(0.0)));
                node.top =
                    px((top + delta.y).clamp(0.0, (window.height() - toolbar.height).max(0.0)));
                drag.moved = true;
            }
        }
        drag.last_cursor = Some(position);
    }
    if mouse.just_released(MouseButton::Left) {
        if drag.moved
            && let Some(index) = drag.active_index
            && let Some((left, top)) = toolbars.iter().find_map(|(toolbar, node)| {
                if toolbar.index != index {
                    return None;
                }
                match (node.left, node.top) {
                    (Val::Px(left), Val::Px(top)) => Some((left, top)),
                    _ => None,
                }
            })
        {
            persist_native_toolbar_position(&mut session, index, left, top);
        }
        drag.active_index = None;
        drag.last_cursor = None;
        drag.moved = false;
    }
}

fn native_toolbar_position(
    default_position: Vec2,
    restored_position: Option<[f32; 2]>,
    window_size: Vec2,
    toolbar_size: Vec2,
) -> Vec2 {
    let max_x = (window_size.x - toolbar_size.x).max(0.0);
    let max_y = (window_size.y - toolbar_size.y).max(0.0);
    let restored = restored_position
        .filter(|position| position.iter().all(|coordinate| coordinate.is_finite()))
        .map(|[x, y]| Vec2::new(x, y))
        .unwrap_or(default_position);
    Vec2::new(restored.x.clamp(0.0, max_x), restored.y.clamp(0.0, max_y))
}

fn native_toolbar_dimensions(
    item_count: usize,
    icon_size: f32,
    max_width: f32,
    max_vertical_rows: usize,
    flow_top_down: bool,
) -> Vec2 {
    let (columns, rows) = if flow_top_down {
        let rows = item_count.min(max_vertical_rows).max(1);
        (item_count.div_ceil(rows), rows)
    } else {
        let columns = (((max_width - 12.0) / icon_size).floor() as usize).max(1);
        (columns, item_count.div_ceil(columns))
    };
    Vec2::new(
        (item_count.min(columns) as f32 * icon_size + 12.0).min(max_width),
        25.0 + rows as f32 * icon_size,
    )
}

fn persist_native_toolbar_position(session: &mut MeshcraftApp, index: usize, left: f32, top: f32) {
    let Some(key) = MeshcraftApp::native_toolbar_position_key(index) else {
        return;
    };
    let mut positions = serde_json::Map::new();
    positions.insert(key.to_owned(), serde_json::json!([left, top]));
    let mut payload = CommandPayload::default();
    payload.values.insert(
        "toolbar_positions".to_owned(),
        serde_json::Value::Object(positions),
    );
    session.dispatch_command("ui.update_preferences", &payload);
}

fn sync_toolbar_button_states(
    scene: Res<SceneDocument>,
    mut commands: Commands,
    session: NonSend<MeshcraftApp>,
    mut buttons: Query<(
        Entity,
        &DispatchCommand,
        Option<&Button>,
        &mut BackgroundColor,
    )>,
) {
    if !scene.is_changed() {
        return;
    }
    for (entity, command, button, mut background) in &mut buttons {
        let state = session.native_command_state(&command.0);
        if state.enabled {
            if button.is_none() {
                commands.entity(entity).insert(Button);
            }
            background.0 = if state.checked == Some(true) {
                Color::srgb(0.12, 0.38, 0.62)
            } else {
                Color::srgb(0.22, 0.24, 0.28)
            };
        } else {
            if button.is_some() {
                commands.entity(entity).remove::<Button>();
            }
            background.0 = Color::srgb(0.14, 0.15, 0.16);
        }
    }
}

fn dispatch_native_selection_display_toggle(
    event: On<Activate>,
    toggles: Query<&NativeSelectionDisplayToggle>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(toggle) = toggles.get(event.entity) else {
        return;
    };
    let policy = session.native_selection_display_policy();
    let enabled = !native_selection_toggle_value(&policy, toggle.kind, toggle.property);
    let mut payload = CommandPayload::default();
    payload.values.insert(
        "kind".to_owned(),
        serde_json::Value::String(native_selection_kind_name(toggle.kind).to_owned()),
    );
    payload.values.insert(
        "property".to_owned(),
        serde_json::Value::String(toggle.property.command_name().to_owned()),
    );
    payload
        .values
        .insert("enabled".to_owned(), serde_json::Value::Bool(enabled));
    session.dispatch_command("selection.update_display", &payload);
}

fn dispatch_toolbar_command(
    event: On<Activate>,
    commands: Query<&DispatchCommand>,
    mut session: NonSendMut<MeshcraftApp>,
    mut palette_state: ResMut<CommandPaletteState>,
    mut palettes: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    if let Ok(command) = commands.get(event.entity) {
        session.dispatch_command(&command.0, &CommandPayload::default());
        palette_state.open = false;
        for mut palette in &mut palettes {
            palette.display = Display::None;
        }
    }
}

fn dispatch_context_menu_command(
    event: On<Activate>,
    commands: Query<&ContextMenuCommand>,
    mut session: NonSendMut<MeshcraftApp>,
    mut context_menu: ResMut<NativeToolsContextMenuState>,
) {
    let Ok(command) = commands.get(event.entity) else {
        return;
    };
    let dispatch = session.dispatch_command(&command.0, &CommandPayload::default());
    if dispatch.accepted {
        context_menu.open = false;
        context_menu.block_world_input = true;
    }
}

fn sync_context_menu_command_states(
    mut commands: Commands,
    session: NonSend<MeshcraftApp>,
    mut buttons: Query<(
        Entity,
        &ContextMenuCommand,
        Option<&Button>,
        &mut BackgroundColor,
    )>,
) {
    for (entity, command, button, mut background) in &mut buttons {
        let state = session.native_command_state(&command.0);
        if state.enabled {
            if button.is_none() {
                commands.entity(entity).insert(Button);
            }
            background.0 = if state.checked == Some(true) {
                Color::srgb(0.12, 0.38, 0.62)
            } else {
                Color::srgb(0.20, 0.22, 0.26)
            };
        } else {
            if button.is_some() {
                commands.entity(entity).remove::<Button>();
            }
            background.0 = Color::srgb(0.14, 0.15, 0.16);
        }
    }
}

fn dispatch_palette_command(
    event: On<Activate>,
    commands: Query<&CommandPaletteResult>,
    catalog: Res<CommandCatalog>,
    mut session: NonSendMut<MeshcraftApp>,
    mut palette_state: ResMut<CommandPaletteState>,
    mut palettes: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    if let Ok(result) = commands.get(event.entity)
        && let Some(command_id) = result.command_id.as_deref()
    {
        if !result.enabled {
            return;
        }
        palette_state.selected_index = result.slot;
        if let Some(command) = catalog
            .0
            .iter()
            .find(|command| command.id.as_str() == command_id)
            && !command.parameters.is_empty()
        {
            begin_palette_parameter_entry(&mut palette_state, command_id);
            return;
        }
        let dispatch = session.dispatch_command(command_id, &CommandPayload::default());
        if dispatch.accepted {
            palette_state.open = false;
            palette_state.pending_command = None;
            for mut palette in &mut palettes {
                palette.display = Display::None;
            }
        } else {
            palette_state.parameter_error = dispatch.result.diagnostics.first().cloned();
        }
    }
}

fn open_command_palette(
    _event: On<Activate>,
    mut state: ResMut<CommandPaletteState>,
    mut palettes: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    state.open = true;
    state.query.clear();
    state.selected_index = 0;
    clear_palette_parameter_entry(&mut state);
    for mut palette in &mut palettes {
        palette.display = Display::Flex;
    }
}

fn open_native_model_settings_editor(
    event: On<Activate>,
    buttons: Query<(), With<NativeModelSettingsEditorButton>>,
    scene: Res<SceneDocument>,
    mut palette: ResMut<CommandPaletteState>,
    mut overlays: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    if !buttons.contains(event.entity) {
        return;
    }
    let settings = scene.document.model_settings();
    let parameters = serde_json::json!({
        "unit_name": settings.unit_name,
        "unit_size": settings.unit_size,
        "grid_size": settings.grid_size,
        "circle_segments": settings.circle_segments,
        "snap_radius": settings.snap_radius,
        "arc_max_length": settings.arc_max_length,
        "double_line_size": settings.double_line_size,
        "double_line_offset": settings.double_line_offset,
    });
    let Ok(parameter_json) = serde_json::to_string_pretty(&parameters) else {
        return;
    };
    palette.open = true;
    palette.query.clear();
    palette.selected_index = 0;
    clear_palette_parameter_entry(&mut palette);
    begin_palette_parameter_entry(&mut palette, "model.update_settings");
    palette.parameter_cursor = parameter_json.len().saturating_sub(1);
    palette.parameter_json = parameter_json;
    for mut overlay in &mut overlays {
        overlay.display = Display::Flex;
    }
}

fn apply_native_model_settings(
    event: On<Activate>,
    buttons: Query<(), With<NativeModelSettingsApplyButton>>,
    inputs: Query<(&NativeModelSettingsInput, &EditableText)>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !buttons.contains(event.entity) {
        return;
    }
    let mut payload = CommandPayload::default();
    for (input, editor) in &inputs {
        if input.field.is_polyline() {
            continue;
        }
        let source = editor.value().to_string();
        let value = match input.field {
            NativeModelSettingField::UnitName => serde_json::json!(source),
            NativeModelSettingField::CircleSegments => source.parse::<u64>().map_or_else(
                |_| serde_json::json!(source),
                |value| serde_json::json!(value),
            ),
            _ => source.parse::<f64>().map_or_else(
                |_| serde_json::json!(source),
                |value| serde_json::json!(value),
            ),
        };
        payload.values.insert(input.field.key().to_owned(), value);
    }
    if payload.values.len() != 5 {
        return;
    }
    session.dispatch_command("model.update_settings", &payload);
}

fn apply_native_polyline_settings(
    event: On<Activate>,
    buttons: Query<(), With<NativePolylineSettingsApplyButton>>,
    inputs: Query<(&NativeModelSettingsInput, &EditableText)>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !buttons.contains(event.entity) {
        return;
    }
    let mut payload = CommandPayload::default();
    for (input, editor) in &inputs {
        if !input.field.is_polyline() {
            continue;
        }
        let source = editor.value().to_string();
        let value = source.parse::<f64>().map_or_else(
            |_| serde_json::json!(source),
            |value| serde_json::json!(value),
        );
        payload.values.insert(input.field.key().to_owned(), value);
    }
    if payload.values.len() != 3 {
        return;
    }
    session.dispatch_command("model.update_settings", &payload);
}

fn apply_native_ui_preferences(
    event: On<Activate>,
    buttons: Query<(), With<NativeUiSettingsApplyButton>>,
    inputs: Query<(&NativeUiSettingInput, &EditableText)>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !buttons.contains(event.entity) {
        return;
    }
    let mut payload = CommandPayload::default();
    for (input, editor) in &inputs {
        let source = editor.value().to_string();
        let value = match input.field {
            NativeUiSettingInputField::TextScale
            | NativeUiSettingInputField::NormalGridWidth
            | NativeUiSettingInputField::ThickGridWidth => source.parse::<f64>().map_or_else(
                |_| serde_json::json!(source),
                |value| serde_json::json!(value),
            ),
            NativeUiSettingInputField::ToolbarIconSize | NativeUiSettingInputField::GridExtent => {
                source.parse::<u32>().map_or_else(
                    |_| serde_json::json!(source),
                    |value| serde_json::json!(value),
                )
            }
        };
        payload.values.insert(input.field.key().to_owned(), value);
    }
    if payload.values.len() != 5 {
        return;
    }
    session.dispatch_command("ui.update_preferences", &payload);
}

fn toggle_native_ui_setting(
    event: On<Activate>,
    buttons: Query<&NativeUiSettingToggleButton>,
    preferences: Res<NativeUiPreferencesState>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(button) = buttons.get(event.entity) else {
        return;
    };
    let mut payload = CommandPayload::default();
    payload.values.insert(
        button.0.key().to_owned(),
        serde_json::json!(!button.0.value(&preferences.value)),
    );
    session.dispatch_command("ui.update_preferences", &payload);
}

fn update_native_ui_setting_choice(
    event: On<Activate>,
    buttons: Query<&NativeUiSettingChoiceButton>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(button) = buttons.get(event.entity) else {
        return;
    };
    let (key, value) = match button.0 {
        NativeUiSettingChoice::GridAxis(axis) => ("grid_normal_axis", serde_json::json!(axis.id())),
        NativeUiSettingChoice::ToolbarFlow(top_down) => {
            ("toolbar_flow_top_down", serde_json::json!(top_down))
        }
    };
    let mut payload = CommandPayload::default();
    payload.values.insert(key.to_owned(), value);
    session.dispatch_command("ui.update_preferences", &payload);
}

fn sync_native_ui_preferences(
    mut preferences: ResMut<NativeUiPreferencesState>,
    session: NonSend<MeshcraftApp>,
) {
    let next = session.native_ui_preferences();
    if preferences.value != next {
        preferences.value = next;
        preferences.revision = preferences.revision.saturating_add(1);
    }
}

fn sync_native_ui_setting_inputs(
    preferences: Res<NativeUiPreferencesState>,
    input_focus: Res<InputFocus>,
    mut inputs: Query<(Entity, &mut EditableText, &mut NativeUiSettingInput)>,
) {
    for (entity, mut editor, mut state) in &mut inputs {
        if state.revision == preferences.revision || input_focus.get() == Some(entity) {
            continue;
        }
        let Some((_, _, value)) = native_ui_setting_fields(&preferences.value)
            .into_iter()
            .find(|(field, _, _)| *field == state.field)
        else {
            continue;
        };
        if editor.value().to_string() != value {
            editor.editor_mut().set_text(&value);
            editor.queue_edit(TextEdit::TextEnd(false));
        }
        state.revision = preferences.revision;
    }
}

fn sync_native_ui_setting_labels(
    preferences: Res<NativeUiPreferencesState>,
    mut toggles: Query<(&NativeUiSettingToggleLabel, &mut Text)>,
    mut choices: Query<(&NativeUiSettingChoiceLabel, &mut TextColor)>,
    mut choice_buttons: Query<(&NativeUiSettingChoiceButton, &mut BackgroundColor)>,
) {
    for (label, mut text) in &mut toggles {
        let value = label.0.value(&preferences.value);
        let next = format!("{}: {}", label.0.label(), if value { "On" } else { "Off" });
        if text.0 != next {
            text.0 = next;
        }
    }
    for (label, mut color) in &mut choices {
        let selected = native_ui_setting_choice_selected(label.0, &preferences.value);
        let next = if selected {
            Color::srgb(0.72, 0.88, 1.0)
        } else {
            Color::WHITE
        };
        if color.0 != next {
            color.0 = next;
        }
    }
    for (button, mut background) in &mut choice_buttons {
        let next = if native_ui_setting_choice_selected(button.0, &preferences.value) {
            Color::srgb(0.12, 0.38, 0.62)
        } else {
            Color::srgb(0.22, 0.24, 0.28)
        };
        if background.0 != next {
            background.0 = next;
        }
    }
}

fn native_ui_setting_choice_selected(
    choice: NativeUiSettingChoice,
    preferences: &NativeUiPreferences,
) -> bool {
    match choice {
        NativeUiSettingChoice::GridAxis(axis) => preferences.grid_normal_axis == axis,
        NativeUiSettingChoice::ToolbarFlow(top_down) => {
            preferences.toolbar_flow_top_down == top_down
        }
    }
}

fn rename_selected_native_object(
    event: On<Activate>,
    buttons: Query<(), With<NativeRenameObjectButton>>,
    names: Query<&EditableText, With<NativeObjectNameInput>>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !buttons.contains(event.entity) {
        return;
    }
    let Ok(name) = names.single() else {
        return;
    };
    let _ = session.rename_native_selected_object(&name.value().to_string());
}

fn sync_native_model_settings_inputs(
    scene: Res<SceneDocument>,
    input_focus: Res<InputFocus>,
    mut inputs: Query<(Entity, &mut EditableText, &mut NativeModelSettingsInput)>,
) {
    for (entity, mut editor, mut state) in &mut inputs {
        let generation_changed = state.generation != scene.generation;
        let revision_changed = state.revision != scene.revision;
        if !generation_changed && (!revision_changed || input_focus.get() == Some(entity)) {
            continue;
        }
        let Some((_, _, value)) = native_model_setting_fields(&scene.document)
            .into_iter()
            .chain(native_polyline_setting_fields(&scene.document))
            .find(|(field, _, _)| *field == state.field)
        else {
            continue;
        };
        editor.editor_mut().set_text(&value);
        editor.queue_edit(TextEdit::TextEnd(false));
        state.generation = scene.generation;
        state.revision = scene.revision;
    }
}

fn select_native_object_row(
    event: On<Activate>,
    rows: Query<&NativeObjectListRow>,
    time: Res<Time>,
    mut click: ResMut<NativeObjectListClick>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(row) = rows.get(event.entity) else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let edit = click.entity == Some(event.entity) && now - click.at_seconds <= 0.45;
    if edit {
        *click = NativeObjectListClick::default();
    } else {
        click.entity = Some(event.entity);
        click.at_seconds = now;
    }
    let _ = session.select_native_object_from_inspector(&row.occurrence, edit);
}

fn sync_native_object_name_input(
    scene: Res<SceneDocument>,
    input_focus: Res<InputFocus>,
    mut commands: Commands,
    mut names: Query<(Entity, &mut EditableText, &mut NativeObjectNameInput)>,
    mut rename_buttons: Query<(Entity, &mut BackgroundColor), With<NativeRenameObjectButton>>,
) {
    if let Ok((entity, mut name, mut state)) = names.single_mut() {
        let selection_changed = state.selection_revision != scene.selection_revision
            || state.definition_id != scene.selected_object_definition_id;
        let document_changed =
            state.generation != scene.generation || state.revision != scene.revision;
        if selection_changed || (document_changed && input_focus.get() != Some(entity)) {
            name.editor_mut()
                .set_text(scene.selected_object_name.as_deref().unwrap_or_default());
            name.queue_edit(TextEdit::TextEnd(false));
            state.generation = scene.generation;
            state.revision = scene.revision;
            state.selection_revision = scene.selection_revision;
            state.definition_id = scene.selected_object_definition_id;
        }
        if scene.selected_object_definition_id.is_some() {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
    let enabled = scene.selected_object_definition_id.is_some();
    for (entity, mut background) in &mut rename_buttons {
        if enabled {
            commands.entity(entity).remove::<InteractionDisabled>();
            background.0 = Color::srgb(0.22, 0.24, 0.28);
        } else {
            commands.entity(entity).insert(InteractionDisabled);
            background.0 = Color::srgb(0.14, 0.15, 0.16);
        }
    }
}

fn sync_native_object_list(
    scene: Res<SceneDocument>,
    mut commands: Commands,
    mut containers: Query<(Entity, &mut NativeObjectListContainer)>,
    mut rows: Query<(&NativeObjectListRow, &mut BackgroundColor)>,
) {
    for (entity, mut state) in &mut containers {
        if state.generation == scene.generation && state.revision == scene.revision {
            continue;
        }
        state.generation = scene.generation;
        state.revision = scene.revision;
        commands.entity(entity).despawn_related::<Children>();
        commands
            .entity(entity)
            .with_children(|parent| spawn_native_object_rows(parent, &scene));
    }
    for (row, mut background) in &mut rows {
        let selected = scene.selection_targets.iter().any(|target| {
            target.entity == SelectedEntity::Occurrence && target.occurrence == row.occurrence
        });
        background.0 = if selected {
            Color::srgb(0.12, 0.38, 0.62)
        } else {
            Color::srgb(0.23, 0.24, 0.25)
        };
    }
}

fn toggle_command_palette(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CommandPaletteState>,
    mut palettes: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    let control = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if control && shift && keyboard.just_pressed(KeyCode::KeyP) {
        state.open = !state.open;
        if state.open {
            state.query.clear();
            state.selected_index = 0;
            clear_palette_parameter_entry(&mut state);
        }
    }
    for mut palette in &mut palettes {
        palette.display = if state.open {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn handle_command_palette_keyboard(
    mut keyboard_events: MessageReader<KeyboardInput>,
    keyboard: Res<ButtonInput<KeyCode>>,
    catalog: Res<CommandCatalog>,
    mut palette: ResMut<CommandPaletteState>,
    mut session: NonSendMut<MeshcraftApp>,
    mut overlays: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    let control = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let alt = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    for event in keyboard_events.read() {
        if !palette.open || event.state != bevy::input::ButtonState::Pressed || alt {
            continue;
        }
        if palette.pending_command.is_some() {
            if control {
                if event.key_code == KeyCode::KeyA {
                    palette.parameter_json.clear();
                    palette.parameter_cursor = 0;
                    palette.parameter_error = None;
                }
                continue;
            }
            match event.key_code {
                KeyCode::Escape => {}
                KeyCode::Backspace => {
                    remove_parameter_character_before_cursor(&mut palette);
                    palette.parameter_error = None;
                }
                KeyCode::Delete => {
                    remove_parameter_character_at_cursor(&mut palette);
                    palette.parameter_error = None;
                }
                KeyCode::ArrowLeft => {
                    palette.parameter_cursor =
                        previous_char_boundary(&palette.parameter_json, palette.parameter_cursor);
                }
                KeyCode::ArrowRight => {
                    palette.parameter_cursor =
                        next_char_boundary(&palette.parameter_json, palette.parameter_cursor);
                }
                KeyCode::Home => palette.parameter_cursor = 0,
                KeyCode::End => palette.parameter_cursor = palette.parameter_json.len(),
                KeyCode::Enter => {
                    submit_palette_parameters(&mut palette, &mut session);
                }
                _ => {
                    if let Some(text) = event.text.as_deref() {
                        insert_parameter_text(&mut palette, text);
                    }
                }
            }
            continue;
        }
        if control {
            continue;
        }
        match event.key_code {
            KeyCode::Escape => {}
            KeyCode::Backspace => {
                palette.query.pop();
                palette.selected_index = 0;
            }
            KeyCode::ArrowDown => {
                if !palette.results.is_empty() {
                    palette.selected_index =
                        (palette.selected_index + 1).min(palette.results.len() - 1);
                }
            }
            KeyCode::ArrowUp => {
                palette.selected_index = palette.selected_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(command_id) = palette.results.get(palette.selected_index).cloned() {
                    let state = session.native_command_state(&command_id);
                    if !state.enabled {
                        palette.parameter_error = state.disabled_reason;
                    } else if let Some(command) = catalog
                        .0
                        .iter()
                        .find(|command| command.id.as_str() == command_id)
                        && !command.parameters.is_empty()
                    {
                        begin_palette_parameter_entry(&mut palette, &command_id);
                    } else {
                        let dispatch =
                            session.dispatch_command(&command_id, &CommandPayload::default());
                        if dispatch.accepted {
                            palette.open = false;
                        } else {
                            palette.parameter_error = dispatch.result.diagnostics.first().cloned();
                        }
                    }
                }
            }
            _ => {
                if let Some(text) = event.text.as_deref()
                    && text.chars().any(|character| !character.is_control())
                {
                    palette.query.push_str(text);
                    palette.selected_index = 0;
                }
            }
        }
    }
    for mut overlay in &mut overlays {
        overlay.display = if palette.open {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn begin_palette_parameter_entry(palette: &mut CommandPaletteState, command_id: &str) {
    palette.pending_command = Some(command_id.to_owned());
    palette.parameter_json = "{}".to_owned();
    palette.parameter_cursor = 1;
    palette.parameter_error = None;
}

fn clear_palette_parameter_entry(palette: &mut CommandPaletteState) {
    palette.pending_command = None;
    palette.parameter_json.clear();
    palette.parameter_cursor = 0;
    palette.parameter_error = None;
}

fn command_payload_from_json(source: &str) -> Result<CommandPayload, String> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|error| format!("Invalid JSON: {error}"))?;
    let serde_json::Value::Object(values) = value else {
        return Err("Parameters must be a JSON object, such as {\"count\": 3}.".to_owned());
    };
    Ok(CommandPayload {
        values: values.into_iter().collect(),
        ..default()
    })
}

fn submit_palette_parameters(
    palette: &mut CommandPaletteState,
    session: &mut MeshcraftApp,
) -> bool {
    let Some(command_id) = palette.pending_command.as_deref() else {
        return false;
    };
    let payload = match command_payload_from_json(&palette.parameter_json) {
        Ok(payload) => payload,
        Err(error) => {
            palette.parameter_error = Some(error);
            return false;
        }
    };
    let dispatch = session.dispatch_command(command_id, &payload);
    if dispatch.accepted {
        palette.open = false;
        clear_palette_parameter_entry(palette);
        true
    } else {
        palette.parameter_error = Some(
            dispatch
                .result
                .diagnostics
                .first()
                .cloned()
                .unwrap_or_else(|| "The command rejected these parameters.".to_owned()),
        );
        false
    }
}

fn insert_parameter_text(palette: &mut CommandPaletteState, text: &str) {
    let insertion = text
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if insertion.is_empty() {
        return;
    }
    palette
        .parameter_json
        .insert_str(palette.parameter_cursor, &insertion);
    palette.parameter_cursor += insertion.len();
    palette.parameter_error = None;
}

fn remove_parameter_character_before_cursor(palette: &mut CommandPaletteState) {
    if palette.parameter_cursor == 0 {
        return;
    }
    let previous = previous_char_boundary(&palette.parameter_json, palette.parameter_cursor);
    palette
        .parameter_json
        .replace_range(previous..palette.parameter_cursor, "");
    palette.parameter_cursor = previous;
}

fn remove_parameter_character_at_cursor(palette: &mut CommandPaletteState) {
    if palette.parameter_cursor >= palette.parameter_json.len() {
        return;
    }
    let next = next_char_boundary(&palette.parameter_json, palette.parameter_cursor);
    palette
        .parameter_json
        .replace_range(palette.parameter_cursor..next, "");
}

fn previous_char_boundary(source: &str, cursor: usize) -> usize {
    source[..cursor]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_char_boundary(source: &str, cursor: usize) -> usize {
    source[cursor..]
        .char_indices()
        .nth(1)
        .map_or(source.len(), |(index, _)| cursor + index)
}

#[allow(clippy::type_complexity)]
fn sync_command_palette_parameter_ui(
    palette: Res<CommandPaletteState>,
    catalog: Res<CommandCatalog>,
    mut panels: Query<&mut Node, With<CommandPaletteParameterPanel>>,
    mut texts: Query<(
        &mut Text,
        Option<&CommandPaletteParameterHint>,
        Option<&CommandPaletteParameterInput>,
        Option<&CommandPaletteParameterError>,
    )>,
) {
    let Some(command_id) = palette.pending_command.as_deref() else {
        for mut panel in &mut panels {
            panel.display = Display::None;
        }
        return;
    };
    let schema = catalog
        .0
        .iter()
        .find(|command| command.id.as_str() == command_id)
        .map(|command| {
            command
                .parameters
                .iter()
                .map(|parameter| {
                    format!(
                        "{}: {}{}",
                        parameter.name,
                        format!("{:?}", parameter.value_type).to_lowercase(),
                        if parameter.required {
                            " (required)"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_else(|| "No parameter schema is available.".to_owned());
    let cursor = palette.parameter_cursor.min(palette.parameter_json.len());
    let parameter_text = format!(
        "> {}|{}",
        &palette.parameter_json[..cursor],
        &palette.parameter_json[cursor..]
    );
    for mut panel in &mut panels {
        panel.display = Display::Flex;
    }
    for (mut text, hint, input, error) in &mut texts {
        if hint.is_some() {
            text.0 = format!("{command_id} parameters · {schema}");
        } else if input.is_some() {
            text.0 = parameter_text.clone();
        } else if error.is_some() {
            text.0 = palette.parameter_error.clone().unwrap_or_default();
        }
    }
}

fn route_native_escape(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut palette: ResMut<CommandPaletteState>,
    mut drag: ResMut<NativeSelectionDrag>,
    mut context_menu: ResMut<NativeToolsContextMenuState>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    drag.start = None;
    if palette.open {
        palette.open = false;
    } else if context_menu.open {
        context_menu.open = false;
        context_menu.block_world_input = true;
    } else {
        session.handle_native_escape();
    }
}

fn route_native_guide_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    input_focus: Res<InputFocus>,
    palette: Res<CommandPaletteState>,
    context_menu: Res<NativeToolsContextMenuState>,
    toolbar_drag: Res<NativeToolbarDrag>,
    ui_interactions: Query<(Option<&Hovered>, Has<Pressed>), With<NativeUiInteraction>>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if input_focus.get().is_some()
        || palette.open
        || context_menu.open
        || toolbar_drag.active_index.is_some()
    {
        return;
    }
    let over_interactive_ui = ui_interactions
        .iter()
        .any(|(hovered, pressed)| pressed || hovered.is_some_and(Hovered::get));
    if over_interactive_ui
        || keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::ShiftLeft)
        || keyboard.pressed(KeyCode::ShiftRight)
        || keyboard.pressed(KeyCode::AltLeft)
        || keyboard.pressed(KeyCode::AltRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight)
    {
        return;
    }
    let command = if keyboard.just_pressed(KeyCode::KeyG) {
        Some("view.grid_guide")
    } else if keyboard.just_pressed(KeyCode::KeyT) {
        Some("view.axis_guide")
    } else {
        None
    };
    if let Some(command) = command {
        session.dispatch_command(command, &CommandPayload::default());
    }
}

fn refresh_command_palette(
    catalog: Res<CommandCatalog>,
    mut palette: ResMut<CommandPaletteState>,
    mut query_labels: Query<
        &mut Text,
        (
            With<CommandPaletteQuery>,
            Without<CommandPaletteResultLabel>,
        ),
    >,
    mut results: Query<(&mut CommandPaletteResult, &mut Node, &mut BackgroundColor)>,
    mut result_labels: Query<(&CommandPaletteResultLabel, &mut Text), Without<CommandPaletteQuery>>,
) {
    if !palette.is_changed() {
        return;
    }
    let matches = search_command_ids(&catalog.0, &palette.query, 24);
    palette.results = matches
        .iter()
        .map(|command| command.id.as_str().to_owned())
        .collect();
    palette.selected_index = palette
        .selected_index
        .min(palette.results.len().saturating_sub(1));
    for mut label in &mut query_labels {
        label.0 = if palette.query.is_empty() {
            "> Type a command name or keyword".to_owned()
        } else {
            format!("> {}", palette.query)
        };
    }
    for (mut result, mut node, mut background) in &mut results {
        if let Some(command) = matches.get(result.slot) {
            result.command_id = Some(command.id.as_str().to_owned());
            node.display = Display::Flex;
            background.0 = if result.slot == palette.selected_index {
                Color::srgb(0.12, 0.38, 0.62)
            } else {
                Color::srgb(0.22, 0.24, 0.28)
            };
        } else {
            result.command_id = None;
            node.display = Display::None;
        }
    }
    for (label_slot, mut label) in &mut result_labels {
        if let Some(command) = matches.get(label_slot.0) {
            let shortcut = command
                .shortcut
                .as_ref()
                .map(|shortcut| format!("  [{shortcut}]"))
                .unwrap_or_default();
            label.0 = format!("{}{}", command.presentation.palette_path, shortcut);
        } else {
            label.0.clear();
        }
    }
}

fn sync_command_palette_states(
    palette: Res<CommandPaletteState>,
    catalog: Res<CommandCatalog>,
    session: NonSend<MeshcraftApp>,
    mut commands: Commands,
    mut results: Query<(
        Entity,
        &mut CommandPaletteResult,
        &mut Node,
        &mut BackgroundColor,
        Option<&Button>,
    )>,
    mut labels: Query<(&CommandPaletteResultLabel, &mut Text, &mut TextColor)>,
) {
    if !palette.open {
        return;
    }
    let mut states_by_slot = std::collections::BTreeMap::new();
    for (entity, mut result, mut node, mut background, button) in &mut results {
        let Some(command_id) = result.command_id.clone() else {
            continue;
        };
        let state = session.native_command_state(&command_id);
        let entry_state = PaletteEntryState {
            command_id: command_id.clone(),
            state: state.clone(),
        };
        states_by_slot.insert(result.slot, entry_state);
        if result.enabled != state.enabled
            || result.checked != state.checked
            || result.disabled_reason != state.disabled_reason
        {
            result.enabled = state.enabled;
            result.checked = state.checked;
            result.disabled_reason = state.disabled_reason;
        }
        if state.enabled {
            if button.is_none() {
                commands.entity(entity).insert(Button);
            }
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
            let color = if result.slot == palette.selected_index || state.checked == Some(true) {
                Color::srgb(0.12, 0.38, 0.62)
            } else {
                Color::srgb(0.22, 0.24, 0.28)
            };
            if background.0 != color {
                background.0 = color;
            }
        } else {
            if button.is_some() {
                commands.entity(entity).remove::<Button>();
            }
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
            let color = if result.slot == palette.selected_index {
                Color::srgb(0.22, 0.23, 0.25)
            } else {
                Color::srgb(0.14, 0.15, 0.16)
            };
            if background.0 != color {
                background.0 = color;
            }
        }
    }
    for (label_slot, mut label, mut color) in &mut labels {
        let Some(entry) = states_by_slot.get(&label_slot.0) else {
            continue;
        };
        let Some(command) = catalog
            .0
            .iter()
            .find(|command| command.id.as_str() == entry.command_id)
        else {
            continue;
        };
        let shortcut = command
            .shortcut
            .as_ref()
            .map(|shortcut| format!("  [{shortcut}]"))
            .unwrap_or_default();
        let active = if entry.state.checked == Some(true) {
            "✓ "
        } else {
            ""
        };
        let disabled = entry
            .state
            .disabled_reason
            .as_ref()
            .map(|reason| format!("  — {reason}"))
            .unwrap_or_default();
        let text = format!(
            "{active}{}{shortcut}{disabled}",
            command.presentation.palette_path
        );
        if label.0 != text {
            label.0 = text;
        }
        let text_color = if entry.state.enabled {
            Color::WHITE
        } else {
            Color::srgb(0.58, 0.59, 0.61)
        };
        if color.0 != text_color {
            color.0 = text_color;
        }
    }
}

struct PaletteEntryState {
    command_id: String,
    state: CommandState,
}

fn search_command_ids(
    catalog: &[CommandDescriptor],
    query: &str,
    limit: usize,
) -> Vec<CommandDescriptor> {
    let query = query.trim().to_lowercase();
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    let mut matches = catalog
        .iter()
        .filter_map(|command| {
            let searchable = format!(
                "{} {} {} {} {}",
                command.presentation.palette_path,
                command.label,
                command.category,
                command.id,
                command.keywords.join(" ")
            )
            .to_lowercase();
            tokens
                .iter()
                .all(|token| searchable.contains(token))
                .then(|| {
                    let rank = if !query.is_empty()
                        && command
                            .presentation
                            .palette_path
                            .to_lowercase()
                            .starts_with(&query)
                    {
                        0
                    } else if !query.is_empty() && command.label.to_lowercase().starts_with(&query)
                    {
                        1
                    } else if !query.is_empty() && command.id.as_str().starts_with(&query) {
                        2
                    } else {
                        3
                    };
                    (rank, command)
                })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|(rank_a, command_a), (rank_b, command_b)| {
        rank_a.cmp(rank_b).then_with(|| {
            command_a
                .presentation
                .palette_path
                .cmp(&command_b.presentation.palette_path)
        })
    });
    matches
        .into_iter()
        .take(limit)
        .map(|(_, command)| command.clone())
        .collect()
}

fn tool_context_groups(catalog: &[CommandDescriptor]) -> Vec<(String, Vec<CommandDescriptor>)> {
    let mut groups = std::collections::BTreeMap::<String, Vec<CommandDescriptor>>::new();
    let mut seen = std::collections::BTreeSet::new();
    for command in catalog {
        if command
            .presentation
            .menu_path
            .first()
            .is_some_and(|entry| entry == "Tools")
        {
            let group = command
                .presentation
                .menu_path
                .get(1)
                .cloned()
                .unwrap_or_else(|| "Common".to_owned());
            if seen.insert(command.id.as_str().to_owned()) {
                groups.entry(group).or_default().push(command.clone());
            }
        }
        for alias in &command.presentation.menu_aliases {
            if alias.first().is_some_and(|entry| entry == "Tools")
                && seen.insert(command.id.as_str().to_owned())
            {
                let group = alias.get(1).cloned().unwrap_or_else(|| "Common".to_owned());
                groups.entry(group).or_default().push(command.clone());
            }
        }
    }
    groups.into_iter().collect()
}

fn sync_scene_document(mut scene: ResMut<SceneDocument>, session: NonSend<MeshcraftApp>) {
    let generation = session.document_generation();
    let revision = session.document_revision();
    let mut refresh_selected_vertices = false;
    let mut refresh_object_panel = false;
    let mut refresh_object_list = false;
    if scene.generation != generation || scene.revision != revision {
        scene.document = session.document_snapshot();
        scene.generation = generation;
        scene.revision = revision;
        (scene.occurrences, scene.triangles, scene.segments) = scene_counts(&scene.document);
        refresh_selected_vertices = true;
        refresh_object_panel = true;
        refresh_object_list = true;
    }
    let status = session.status_text();
    if scene.status != status {
        scene.status = status.to_owned();
    }
    let active_tool = session.active_tool_label();
    if scene.active_tool != active_tool {
        scene.active_tool = active_tool.to_owned();
    }
    let selection_revision = session.native_selection_revision();
    if scene.selection_revision != selection_revision {
        let summary = session.native_selection_summary();
        scene.selection_revision = selection_revision;
        scene.selected_objects = summary.objects;
        scene.selected_vertices = summary.vertices;
        scene.selected_faces = summary.faces;
        scene.selected_edges = summary.edges;
        scene.selection_targets = session.native_selection_targets();
        refresh_selected_vertices = true;
        refresh_object_panel = true;
    }
    if refresh_selected_vertices {
        scene.selected_vertex_positions =
            selected_vertex_positions(&scene.document, &scene.selection_targets);
    }
    if refresh_object_panel {
        (
            scene.selected_object_count,
            scene.selected_object_definition_id,
            scene.selected_object_name,
        ) = selected_object_info(&scene.document, &scene.selection_targets);
    }
    if refresh_object_list {
        scene.object_entries =
            native_object_entries(&scene.document, session.native_top_level_occurrences());
    }
}

#[allow(clippy::too_many_arguments)]
fn update_scene_meshes(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    mut rendered_revision: ResMut<RenderedSceneRevision>,
    mut entities: ResMut<SceneMeshEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lighting: Res<LightingSettings>,
) {
    if rendered_revision.generation == scene.generation
        && rendered_revision.revision == scene.revision
        && rendered_revision.display_revision == selection_display.render_revision
    {
        return;
    }
    let scene_meshes = build_scene_meshes_with_policy(&scene.document, &selection_display.policy);
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.front_faces,
        scene_meshes.front_faces,
        StandardMaterial {
            cull_mode: Some(Face::Back),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            reflectance: lighting.specular,
            ..default()
        },
        true,
    );
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.back_faces,
        scene_meshes.back_faces,
        StandardMaterial {
            cull_mode: Some(Face::Front),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            reflectance: lighting.specular,
            ..default()
        },
        false,
    );
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.segments,
        scene_meshes.segments,
        StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        },
        false,
    );
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.face_wireframe,
        scene_meshes.face_wireframe,
        StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        },
        false,
    );
    rendered_revision.generation = scene.generation;
    rendered_revision.revision = scene.revision;
    rendered_revision.display_revision = selection_display.render_revision;
}

fn update_selection_meshes(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    mut rendered_revision: ResMut<RenderedSelectionRevision>,
    mut entities: ResMut<SceneMeshEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if rendered_revision.selection_revision == scene.selection_revision
        && rendered_revision.display_revision == selection_display.render_revision
    {
        return;
    }
    let selected = build_selection_meshes_with_policy(
        &scene.document,
        &scene.selection_targets,
        &selection_display.policy,
    );
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.selected_faces,
        selected.faces,
        StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            base_color: Color::WHITE,
            cull_mode: None,
            ..default()
        },
        false,
    );
    replace_scene_batch(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut entities.selected_edges,
        selected.edges,
        StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        },
        false,
    );
    rendered_revision.selection_revision = scene.selection_revision;
    rendered_revision.display_revision = selection_display.render_revision;
}

fn build_selection_meshes_with_policy(
    document: &Document,
    selection: &[SelectionTarget],
    policy: &SelectionDisplayPolicy,
) -> SelectionMeshes {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut line_positions = Vec::new();
    let mut line_colors = Vec::new();
    let highlight = [1.0, 0.34, 0.015, 0.48];
    let edge_highlight = [1.0, 0.24, 0.015, 1.0];

    for target in selection {
        if !target.occurrence.is_scene_root() && !policy.can_draw(SelectionKind::Object) {
            continue;
        }
        let (Some(resolved), Some(geometry)) = (
            document.resolve_occurrence(&target.occurrence),
            document.effective_geometry(&target.occurrence),
        ) else {
            continue;
        };
        let include_all = matches!(target.entity, SelectedEntity::Occurrence);
        if policy.can_draw(SelectionKind::Face)
            && !policy.state(SelectionKind::Face).wireframe
            && (include_all || matches!(target.entity, SelectedEntity::Triangle(_)))
        {
            for triangle in geometry.triangles.values().filter(|triangle| {
                include_all || target.entity == SelectedEntity::Triangle(triangle.id)
            }) {
                let [Some(a), Some(b), Some(c)] = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                else {
                    continue;
                };
                let [a, b, c] =
                    [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                let Some(normal) = (b - a).cross(c - a).try_normalized(1.0e-12) else {
                    continue;
                };
                for point in [a, b, c] {
                    let Some(point) = to_bevy_vec3(point + normal * 0.003) else {
                        continue;
                    };
                    positions.push(point.to_array());
                    normals.push(to_bevy_vec3(normal).unwrap_or(Vec3::Y).to_array());
                    colors.push(highlight);
                }
            }
        }
        if policy.can_draw(SelectionKind::Edge)
            && (include_all || matches!(target.entity, SelectedEntity::Segment(_)))
        {
            for segment in geometry.segments.values().filter(|segment| {
                include_all || target.entity == SelectedEntity::Segment(segment.id)
            }) {
                let (Some(start), Some(end)) = (
                    geometry.vertex_position(segment.endpoints[0]),
                    geometry.vertex_position(segment.endpoints[1]),
                ) else {
                    continue;
                };
                let [Some(start), Some(end)] = [start, end]
                    .map(|point| to_bevy_vec3(resolved.world_transform.transform_point(point)))
                else {
                    continue;
                };
                line_positions.extend([start.to_array(), end.to_array()]);
                line_colors.extend([edge_highlight, edge_highlight]);
            }
        }
    }
    SelectionMeshes {
        faces: triangle_mesh(positions, normals, colors),
        edges: line_mesh(line_positions, line_colors),
    }
}

fn replace_scene_batch(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    entity: &mut Option<Entity>,
    mesh: Option<Mesh>,
    material: StandardMaterial,
    casts_shadows: bool,
) {
    match (entity.as_mut(), mesh) {
        (Some(entity), Some(mesh)) => {
            commands.entity(*entity).insert(Mesh3d(meshes.add(mesh)));
        }
        (Some(_), None) => {
            if let Some(entity) = entity.take() {
                commands.entity(entity).despawn();
            }
        }
        (None, Some(mesh)) => {
            *entity = Some(spawn_mesh(
                commands,
                meshes,
                materials,
                mesh,
                material,
                Transform::default(),
                true,
                casts_shadows,
            ));
        }
        (None, None) => {}
    }
}

fn refresh_status(scene: Res<SceneDocument>, mut labels: Query<&mut Text, With<StatusText>>) {
    if !scene.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.0 = format!("{}  |  {}", scene.status, scene.active_tool);
    }
}

// Bevy system parameters are injected independently by the ECS scheduler.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn native_viewport_pointer(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    palette: Res<CommandPaletteState>,
    orbit: Res<OrbitCamera>,
    mut context_menu: ResMut<NativeToolsContextMenuState>,
    mut right_click: ResMut<NativeRightClickGesture>,
    scene: Res<SceneDocument>,
    mut overlay: ResMut<NativeViewportOverlay>,
    toolbar_drag: Res<NativeToolbarDrag>,
    ui_interactions: Query<(Option<&Hovered>, Has<Pressed>), With<NativeUiInteraction>>,
    mut drag: ResMut<NativeSelectionDrag>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    let cursor = window.cursor_position();
    overlay.guides = session.native_construction_guides().to_vec();
    overlay.guides_visible = session.native_guides_visible();
    overlay.grid_size = scene.document.model_settings().grid_size;
    if palette.open {
        overlay.clear_cursor();
        drag.start = None;
        *right_click = NativeRightClickGesture::default();
        return;
    }
    if toolbar_drag.active_index.is_some() {
        overlay.clear_cursor();
        drag.start = None;
        return;
    }
    if context_menu.block_world_input {
        overlay.clear_cursor();
        context_menu.block_world_input = false;
        drag.start = None;
        *right_click = NativeRightClickGesture::default();
        return;
    }
    if context_menu.open {
        let over_menu = ui_interactions
            .iter()
            .any(|(hovered, pressed)| pressed || hovered.is_some_and(Hovered::get));
        if mouse.just_pressed(MouseButton::Left) && !over_menu {
            context_menu.open = false;
            context_menu.block_world_input = true;
        }
        overlay.clear_cursor();
        drag.start = None;
        *right_click = NativeRightClickGesture::default();
        return;
    }
    let over_toolbar = ui_interactions
        .iter()
        .any(|(hovered, pressed)| pressed || hovered.is_some_and(Hovered::get));
    if over_toolbar {
        overlay.clear_cursor();
        drag.start = None;
        if mouse.just_pressed(MouseButton::Right) || mouse.just_released(MouseButton::Right) {
            *right_click = NativeRightClickGesture::default();
        }
        return;
    }
    if let Some(position) = update_right_click_gesture(
        &mut right_click,
        mouse.just_pressed(MouseButton::Right),
        mouse.pressed(MouseButton::Right),
        mouse.just_released(MouseButton::Right),
        cursor,
        cursor.is_some_and(|position| native_viewport_contains(window, position)),
    ) {
        context_menu.open = true;
        context_menu.position = position;
        context_menu.block_world_input = true;
    }
    if context_menu.open || context_menu.block_world_input {
        overlay.clear_cursor();
        drag.start = None;
        context_menu.block_world_input = false;
        return;
    }
    let mut hover_pick = None::<(ModelVec3, ModelVec3, f64)>;
    let mut hover_snap = None;
    let mut hover_point = None;
    if let Some(position) = cursor.filter(|position| native_viewport_contains(window, *position)) {
        if let Ok(ray) = camera.viewport_to_world(camera_transform, position) {
            let origin = ModelVec3::new(
                f64::from(ray.origin.x),
                f64::from(ray.origin.y),
                f64::from(ray.origin.z),
            );
            let ray_direction = ray.direction.as_vec3();
            let direction = ModelVec3::new(
                f64::from(ray_direction.x),
                f64::from(ray_direction.y),
                f64::from(ray_direction.z),
            );
            let radius = native_pick_radius(orbit.distance, window.height());
            let snap = session
                .native_snap_point_on_ray(origin, direction, radius)
                .or_else(|| native_grid_pick(ray, overlay.grid_size).and_then(|(_, snap)| snap));
            if let Some(snap) = snap {
                let axes = session.native_cursor_axes(snap);
                let screen = to_bevy_vec3(snap.point)
                    .and_then(|point| camera.world_to_viewport(camera_transform, point).ok());
                overlay.cursor_snap = Some(snap);
                overlay.cursor_axes = axes;
                overlay.cursor_screen = screen;
                hover_point = Some(snap.point);
                hover_snap = Some(snap);
                hover_pick = Some((origin, direction, radius));
                session.set_native_cursor_snap(Some(snap));
            } else {
                overlay.clear_cursor();
                session.set_native_cursor_snap(None);
            }
        } else {
            overlay.clear_cursor();
            session.set_native_cursor_snap(None);
        }
    } else {
        overlay.clear_cursor();
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(position) =
            cursor.filter(|position| native_viewport_contains(window, *position))
    {
        if session.active_tool_label() == "Select" {
            drag.start = Some(position);
            drag.current = position;
        } else if let (Some((origin, direction, radius)), Some(point)) = (hover_pick, hover_point) {
            let target = session.pick_native_target(origin, direction, radius);
            if let Some(snap) = hover_snap {
                session.handle_native_world_pick(point, Some(snap), target);
            }
        }
    }

    if drag.start.is_some()
        && let Some(position) = cursor
    {
        drag.current = position;
    }
    if mouse.just_released(MouseButton::Left)
        && let Some(start) = drag.start.take()
    {
        let end = cursor.unwrap_or(drag.current);
        if (end - start).length() >= 4.0 {
            let modifiers = native_modifiers(&keyboard);
            let crossing = end.x < start.x && !modifiers.alt;
            let targets = native_selection_region(
                session.document_snapshot(),
                camera,
                camera_transform,
                SelectionRegion {
                    start,
                    end,
                    crossing,
                },
                &session.native_edit_context(),
                &session.native_selection_display_policy(),
            );
            let mode = selection_mode(modifiers, None);
            session.apply_native_selection_region(targets, crossing, mode);
        } else if let Some(position) = cursor
            && let Ok(ray) = camera.viewport_to_world(camera_transform, position)
        {
            let origin = ModelVec3::new(
                f64::from(ray.origin.x),
                f64::from(ray.origin.y),
                f64::from(ray.origin.z),
            );
            let direction = ray.direction.as_vec3();
            let direction = ModelVec3::new(
                f64::from(direction.x),
                f64::from(direction.y),
                f64::from(direction.z),
            );
            let radius = native_pick_radius(orbit.distance, window.height());
            let mut target = session.pick_native_target(origin, direction, radius);
            let modifiers = native_modifiers(&keyboard);
            if modifiers.alt
                && let Some(target) = &mut target
                && !target.occurrence.is_scene_root()
            {
                target.entity = SelectedEntity::Occurrence;
            }
            let now_millis = (time.elapsed_secs_f64() * 1000.0).max(0.0) as u64;
            session.handle_native_selection_click(
                target,
                modifiers,
                now_millis,
                ModelVec2::new(f64::from(position.x), f64::from(position.y)),
            );
        }
    }
}

fn draw_native_viewport_overlays(
    mut gizmos: Gizmos,
    overlay: Res<NativeViewportOverlay>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    if let Some(snap) = overlay.cursor_snap
        && let Some(axes) = overlay.cursor_axes
        && let Some(origin) = to_bevy_vec3(snap.point)
    {
        let spacing = if overlay.grid_size.is_finite() && overlay.grid_size > 1.0e-6 {
            overlay.grid_size
        } else {
            1.0
        };
        let projected_origin = cameras
            .single()
            .ok()
            .and_then(|(camera, transform)| camera.world_to_viewport(transform, origin).ok());
        let cursor_colors = [
            Color::srgb(0.89, 0.49, 0.49),
            Color::srgb(0.41, 0.85, 0.59),
            Color::srgb(0.47, 0.82, 1.0),
        ];
        for (axis, color) in axes.into_iter().zip(cursor_colors) {
            let Some(axis) = to_bevy_vec3(axis) else {
                continue;
            };
            let projected_unit = cameras
                .single()
                .ok()
                .and_then(|(camera, transform)| {
                    let screen_origin = projected_origin?;
                    let screen_end = camera
                        .world_to_viewport(transform, origin + axis * spacing as f32)
                        .ok()?;
                    Some(screen_origin.distance(screen_end))
                })
                .unwrap_or_default();
            let extent = if projected_unit > 1.0e-3 {
                spacing * f64::from((25.0 / projected_unit).clamp(0.2, 12.0))
            } else {
                spacing
            };
            let lighter = color.mix(&Color::WHITE, 0.4);
            gizmos.line(origin - axis * (extent as f32 * 0.62), origin, lighter);
            gizmos.line(origin, origin + axis * extent as f32, color);
        }
    }

    if !overlay.guides_visible || !overlay.grid_size.is_finite() || overlay.grid_size <= 0.0 {
        return;
    }
    let spacing = overlay.grid_size;
    let extent = spacing * 10.0;
    for guide in &overlay.guides {
        let Some(origin) = to_bevy_vec3(guide.origin) else {
            continue;
        };
        match guide.kind {
            ConstructionGuideKind::Axis => {
                for (axis, color) in [
                    (guide.axis_u, Color::srgb(0.79, 0.41, 0.41)),
                    (guide.axis_v, Color::srgb(0.39, 0.63, 0.45)),
                    (guide.axis_w, Color::srgb(0.39, 0.51, 0.75)),
                ] {
                    let Some(axis) = to_bevy_vec3(axis) else {
                        continue;
                    };
                    gizmos.line(
                        origin - axis * extent as f32,
                        origin,
                        color.mix(&Color::WHITE, 0.35),
                    );
                    gizmos.line(origin, origin + axis * extent as f32, color);
                }
            }
            ConstructionGuideKind::Grid => {
                let planes = [
                    (guide.axis_w, guide.axis_u, guide.axis_v),
                    (guide.axis_v, guide.axis_u, guide.axis_w),
                    (guide.axis_u, guide.axis_v, guide.axis_w),
                ];
                let grid_color = Color::srgba(0.28, 0.77, 0.81, 0.52);
                for (_, axis_u, axis_v) in planes {
                    let (Some(axis_u), Some(axis_v)) = (to_bevy_vec3(axis_u), to_bevy_vec3(axis_v))
                    else {
                        continue;
                    };
                    for step in -10..=10 {
                        let offset = step as f32 * spacing as f32;
                        let color = if step == 0 {
                            Color::srgba(0.33, 0.86, 0.88, 0.72)
                        } else {
                            grid_color
                        };
                        for (start, end) in [
                            (
                                origin + axis_u * offset - axis_v * extent as f32,
                                origin + axis_u * offset + axis_v * extent as f32,
                            ),
                            (
                                origin + axis_v * offset - axis_u * extent as f32,
                                origin + axis_v * offset + axis_u * extent as f32,
                            ),
                        ] {
                            gizmos.line(start, end, color);
                        }
                    }
                }
            }
        }
    }
}

fn sync_native_cursor_dot(
    overlay: Res<NativeViewportOverlay>,
    mut dots: Query<&mut Node, With<NativeCursorDot>>,
) {
    for mut node in &mut dots {
        if let Some(position) = overlay.cursor_screen {
            node.display = Display::Flex;
            node.left = px(position.x - 3.5);
            node.top = px(position.y - 3.5);
        } else {
            node.display = Display::None;
        }
    }
}

fn sync_native_selected_vertex_dots(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    selection_display: Res<NativeSelectionDisplayState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    layers: Query<Entity, With<NativeSelectedVertexDotLayer>>,
    mut dots: Query<(Entity, &NativeSelectedVertexDot, &mut Node)>,
) {
    let (Ok(window), Ok((camera, camera_transform)), Ok(layer)) =
        (windows.single(), cameras.single(), layers.single())
    else {
        return;
    };
    let positions: Vec<Vec2> = if selection_display.policy.can_draw(SelectionKind::Vertex) {
        scene
            .selected_vertex_positions
            .iter()
            .filter_map(|position| {
                let world = to_bevy_vec3(*position)?;
                let screen = camera.world_to_viewport(camera_transform, world).ok()?;
                native_viewport_contains(window, screen).then_some(screen)
            })
            .collect()
    } else {
        Vec::new()
    };
    let existing: std::collections::BTreeMap<usize, Entity> = dots
        .iter()
        .map(|(entity, dot, _)| (dot.0, entity))
        .collect();

    for (index, position) in positions.iter().copied().enumerate() {
        if let Some(entity) = existing.get(&index) {
            if let Ok((_, _, mut node)) = dots.get_mut(*entity) {
                node.display = Display::Flex;
                node.left = px(position.x - 3.0);
                node.top = px(position.y - 3.0);
            }
        } else {
            commands.entity(layer).with_children(|layer| {
                layer.spawn((
                    NativeSelectedVertexDot(index),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(position.x - 3.0),
                        top: px(position.y - 3.0),
                        width: px(6),
                        height: px(6),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.98, 0.22, 0.18)),
                    BorderColor::all(Color::WHITE),
                ));
            });
        }
    }
    for (index, entity) in existing {
        if index >= positions.len() {
            commands.entity(entity).despawn();
        }
    }
}

fn update_right_click_gesture(
    gesture: &mut NativeRightClickGesture,
    just_pressed: bool,
    pressed: bool,
    just_released: bool,
    cursor: Option<Vec2>,
    within_viewport: bool,
) -> Option<Vec2> {
    if just_pressed {
        gesture.start = cursor.filter(|_| within_viewport);
        gesture.dragged = false;
    }
    if pressed
        && let (Some(start), Some(position)) = (gesture.start, cursor)
        && (position - start).length() >= 5.0
    {
        gesture.dragged = true;
    }

    let menu_position =
        (just_released && gesture.start.is_some() && !gesture.dragged && within_viewport)
            .then_some(cursor)
            .flatten();
    if just_released {
        *gesture = NativeRightClickGesture::default();
    }
    menu_position
}

fn native_viewport_contains(window: &Window, position: Vec2) -> bool {
    position.x >= 270.0
        && position.x <= window.width() - 350.0
        && position.y >= 58.0
        && position.y <= window.height() - 32.0
}

fn native_modifiers(keyboard: &ButtonInput<KeyCode>) -> egui::Modifiers {
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let alt = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    let mac_cmd = keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight);
    egui::Modifiers {
        alt,
        ctrl,
        shift,
        mac_cmd,
        command: ctrl || mac_cmd,
    }
}

fn selection_mode(modifiers: egui::Modifiers, target_is_selected: Option<bool>) -> SelectionMode {
    if modifiers.ctrl || modifiers.mac_cmd {
        SelectionMode::Remove
    } else if modifiers.shift {
        SelectionMode::Add
    } else if target_is_selected == Some(true) {
        SelectionMode::Remove
    } else {
        SelectionMode::Replace
    }
}

fn native_pick_radius(distance: f32, viewport_height: f32) -> f64 {
    if !distance.is_finite() || !viewport_height.is_finite() || viewport_height <= 0.0 {
        return 0.1;
    }
    (f64::from(distance) * 2.0 * f64::from(22.5_f32.to_radians().tan()) * 7.0
        / f64::from(viewport_height))
    .clamp(0.01, 0.75)
}

fn native_grid_pick(
    ray: Ray3d,
    grid_size: f64,
) -> Option<(ModelVec3, Option<crate::viewport::SnapResult>)> {
    if !grid_size.is_finite() || grid_size <= 0.0 {
        return None;
    }
    let origin = ModelVec3::new(
        f64::from(ray.origin.x),
        f64::from(ray.origin.y),
        f64::from(ray.origin.z),
    );
    let direction = ray.direction.as_vec3();
    let direction = ModelVec3::new(
        f64::from(direction.x),
        f64::from(direction.y),
        f64::from(direction.z),
    );
    if direction.y.abs() <= 1.0e-12 {
        return None;
    }
    let distance = -origin.y / direction.y;
    if !distance.is_finite() || distance < 0.0 {
        return None;
    }
    let intersection = origin + direction * distance;
    let point = ModelVec3::new(
        (intersection.x / grid_size).round() * grid_size,
        0.0,
        (intersection.z / grid_size).round() * grid_size,
    );
    let snap = crate::viewport::SnapResult {
        point,
        normal: ModelVec3::Y,
        kind: crate::viewport::SnapKind::Grid,
        screen_distance: 0.0,
    };
    Some((point, Some(snap)))
}

#[derive(Clone, Copy)]
struct SelectionRegion {
    start: Vec2,
    end: Vec2,
    crossing: bool,
}

fn native_selection_region(
    document: Document,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    selection_region: SelectionRegion,
    context: &EditContext,
    policy: &meshcraft_core::selection::SelectionDisplayPolicy,
) -> Vec<SelectionTarget> {
    let SelectionRegion {
        start,
        end,
        crossing,
    } = selection_region;
    let min = start.min(end);
    let max = start.max(end);
    let region = [min, max];
    let Ok(occurrences) = document.scene_geometry_refs() else {
        return Vec::new();
    };
    let mut projected = Vec::new();
    let mut subtree_points =
        std::collections::BTreeMap::<meshcraft_core::document::OccurrenceRef, Vec<Vec2>>::new();

    for occurrence in occurrences {
        if !native_context_contains(&document, context, &occurrence)
            || (!occurrence.is_scene_root() && !policy.can_draw(SelectionKind::Object))
        {
            continue;
        }
        let (Some(resolved), Some(geometry)) = (
            document.resolve_occurrence(&occurrence),
            document.effective_geometry(&occurrence),
        ) else {
            continue;
        };
        let mut item = ProjectedGeometry {
            points: Vec::new(),
            vertices: Vec::new(),
            segments: Vec::new(),
            triangles: Vec::new(),
        };
        for (id, vertex) in &geometry.vertices {
            let world = resolved.world_transform.transform_point(vertex.position);
            if let Some(point) = project_native_point(camera, camera_transform, world) {
                item.points.push(point);
                item.vertices.push((
                    point,
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Vertex(*id),
                    },
                ));
            }
        }
        for segment in geometry.segments.values() {
            let [Some(a), Some(b)] = segment
                .endpoints
                .map(|reference| geometry.vertex_position(reference))
            else {
                continue;
            };
            let [Some(a), Some(b)] = [a, b].map(|point| {
                project_native_point(
                    camera,
                    camera_transform,
                    resolved.world_transform.transform_point(point),
                )
            }) else {
                continue;
            };
            item.points.extend([a, b]);
            item.segments.push((
                [a, b],
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Segment(segment.id),
                },
            ));
        }
        for triangle in geometry.triangles.values() {
            let [Some(a), Some(b), Some(c)] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference))
            else {
                continue;
            };
            let [Some(a), Some(b), Some(c)] = [a, b, c].map(|point| {
                project_native_point(
                    camera,
                    camera_transform,
                    resolved.world_transform.transform_point(point),
                )
            }) else {
                continue;
            };
            item.points.extend([a, b, c]);
            item.triangles.push((
                [a, b, c],
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Triangle(triangle.id),
                },
            ));
        }
        if !occurrence.is_scene_root() {
            for length in 1..=occurrence.path.len() {
                let ancestor = meshcraft_core::document::OccurrenceRef {
                    path: occurrence.path[..length].to_vec(),
                };
                subtree_points
                    .entry(ancestor)
                    .or_default()
                    .extend(item.points.iter().copied());
            }
        }
        projected.push(item);
    }

    let mut selected = std::collections::BTreeSet::new();
    if policy.can_select(SelectionKind::Object) {
        for (occurrence, points) in subtree_points {
            if points.is_empty() {
                continue;
            }
            let contained = points
                .iter()
                .all(|point| native_rect_contains(region, *point));
            let intersects = projected_bounds_intersect(&points, region);
            if (crossing && intersects) || (!crossing && contained) {
                selected.insert(SelectionTarget {
                    occurrence,
                    entity: SelectedEntity::Occurrence,
                });
            }
        }
    }
    for item in projected {
        if policy.can_select(SelectionKind::Vertex) {
            selected.extend(
                item.vertices
                    .into_iter()
                    .filter(|(point, _)| native_rect_contains(region, *point))
                    .map(|(_, target)| target),
            );
        }
        if policy.can_select(SelectionKind::Edge) {
            selected.extend(
                item.segments
                    .into_iter()
                    .filter(|(points, _)| {
                        if crossing {
                            native_segment_intersects_rect(points[0], points[1], region)
                        } else {
                            points
                                .iter()
                                .all(|point| native_rect_contains(region, *point))
                        }
                    })
                    .map(|(_, target)| target),
            );
        }
        if policy.can_select(SelectionKind::Face) && !policy.state(SelectionKind::Face).wireframe {
            selected.extend(
                item.triangles
                    .into_iter()
                    .filter(|(points, _)| {
                        native_triangle_intersects_rect(*points, region, crossing)
                    })
                    .map(|(_, target)| target),
            );
        }
    }
    selected.into_iter().collect()
}

struct ProjectedGeometry {
    points: Vec<Vec2>,
    vertices: Vec<(Vec2, SelectionTarget)>,
    segments: Vec<([Vec2; 2], SelectionTarget)>,
    triangles: Vec<([Vec2; 3], SelectionTarget)>,
}

fn native_context_contains(
    document: &Document,
    context: &EditContext,
    occurrence: &meshcraft_core::document::OccurrenceRef,
) -> bool {
    match context {
        EditContext::Document => true,
        EditContext::Occurrence(scope) => occurrence.path.starts_with(&scope.path),
        EditContext::Definition(definition) if occurrence.is_scene_root() => {
            *definition == Document::SCENE_ROOT_DEFINITION_ID
        }
        EditContext::Definition(definition) => document
            .resolve_occurrence(occurrence)
            .is_some_and(|resolved| resolved.definition.id == *definition),
    }
}

fn project_native_point(
    camera: &Camera,
    transform: &GlobalTransform,
    point: ModelVec3,
) -> Option<Vec2> {
    let world = to_bevy_vec3(point)?;
    let projected = camera.world_to_viewport_with_depth(transform, world).ok()?;
    (projected.z > 0.0 && projected.truncate().is_finite()).then_some(projected.truncate())
}

fn native_rect_contains(region: [Vec2; 2], point: Vec2) -> bool {
    point.x >= region[0].x
        && point.x <= region[1].x
        && point.y >= region[0].y
        && point.y <= region[1].y
}

fn projected_bounds_intersect(points: &[Vec2], region: [Vec2; 2]) -> bool {
    if points.is_empty() {
        return false;
    }
    let min = points
        .iter()
        .copied()
        .reduce(Vec2::min)
        .unwrap_or(Vec2::ZERO);
    let max = points
        .iter()
        .copied()
        .reduce(Vec2::max)
        .unwrap_or(Vec2::ZERO);
    min.x <= region[1].x && max.x >= region[0].x && min.y <= region[1].y && max.y >= region[0].y
}

fn native_segment_intersects_rect(a: Vec2, b: Vec2, region: [Vec2; 2]) -> bool {
    if native_rect_contains(region, a) || native_rect_contains(region, b) {
        return true;
    }
    let delta = b - a;
    let mut low = 0.0_f32;
    let mut high = 1.0_f32;
    for (p, q) in [
        (-delta.x, a.x - region[0].x),
        (delta.x, region[1].x - a.x),
        (-delta.y, a.y - region[0].y),
        (delta.y, region[1].y - a.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            low = low.max(ratio);
        } else {
            high = high.min(ratio);
        }
        if low > high {
            return false;
        }
    }
    true
}

fn native_triangle_intersects_rect(points: [Vec2; 3], region: [Vec2; 2], crossing: bool) -> bool {
    if !crossing {
        return points
            .iter()
            .all(|point| native_rect_contains(region, *point));
    }
    points
        .iter()
        .any(|point| native_rect_contains(region, *point))
        || (0..3).any(|index| {
            native_segment_intersects_rect(points[index], points[(index + 1) % 3], region)
        })
        || [
            region[0],
            Vec2::new(region[1].x, region[0].y),
            region[1],
            Vec2::new(region[0].x, region[1].y),
        ]
        .into_iter()
        .any(|corner| native_point_in_triangle(corner, points))
}

fn native_point_in_triangle(point: Vec2, triangle: [Vec2; 3]) -> bool {
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let a = cross(triangle[0], triangle[1], point);
    let b = cross(triangle[1], triangle[2], point);
    let c = cross(triangle[2], triangle[0], point);
    (a >= 0.0 && b >= 0.0 && c >= 0.0) || (a <= 0.0 && b <= 0.0 && c <= 0.0)
}

fn refresh_selection_window(
    drag: Res<NativeSelectionDrag>,
    mut overlays: Query<
        (&mut Node, &mut BackgroundColor, &mut BorderColor),
        With<SelectionWindowOverlay>,
    >,
    mut labels: Query<(&mut Text, &mut TextColor), With<SelectionWindowLabel>>,
) {
    let Some(start) = drag.start else {
        for (mut node, _, _) in &mut overlays {
            node.display = Display::None;
        }
        return;
    };
    let min = start.min(drag.current);
    let max = start.max(drag.current);
    let crossing = drag.current.x < start.x;
    let (fill, outline, label) = if crossing {
        (
            Color::srgba(0.05, 0.72, 0.48, 0.14),
            Color::srgb(0.04, 0.68, 0.42),
            "Crossing",
        )
    } else {
        (
            Color::srgba(0.12, 0.42, 0.74, 0.12),
            Color::srgb(0.14, 0.46, 0.92),
            "Window",
        )
    };
    for (mut node, mut background, mut border) in &mut overlays {
        node.display = Display::Flex;
        node.left = px(min.x);
        node.top = px(min.y);
        node.width = px((max.x - min.x).max(2.0));
        node.height = px((max.y - min.y).max(2.0));
        background.0 = fill;
        *border = BorderColor::all(outline);
    }
    for (mut text, mut color) in &mut labels {
        text.0 = label.to_owned();
        color.0 = outline;
    }
}

fn sync_lighting_sliders(
    sliders: Query<(&SliderValue, &LightingSlider), Changed<SliderValue>>,
    mut settings: ResMut<LightingSettings>,
    mut labels: Query<(&mut Text, &LightingValueLabel)>,
) {
    let mut changed = false;
    for (value, slider) in &sliders {
        slider.0.set_value(&mut settings, value.0);
        changed = true;
    }
    if !changed {
        return;
    }
    for (mut label, marker) in &mut labels {
        label.0 = format!("{}: {:.2}", marker.0.label(), marker.0.value(*settings));
    }
}

#[allow(clippy::type_complexity)]
fn update_slider_thumbs(
    sliders: Query<
        (Entity, &SliderValue, &SliderRange),
        (Changed<SliderValue>, With<LightingSlider>),
    >,
    children: Query<&Children>,
    mut thumbs: Query<&mut Node, With<SliderThumb>>,
) {
    for (entity, value, range) in &sliders {
        for child in children.iter_descendants(entity) {
            if let Ok(mut node) = thumbs.get_mut(child) {
                node.left = percent(range.thumb_position(value.0) * 100.0);
            }
        }
    }
}

fn cycle_shadow_filter(
    event: On<Activate>,
    buttons: Query<(), With<ShadowFilterButton>>,
    mut settings: ResMut<LightingSettings>,
) {
    if buttons.contains(event.entity) {
        settings.shadow_filter = settings.shadow_filter.next();
    }
}

fn toggle_csm(
    event: On<Activate>,
    buttons: Query<(), With<CsmButton>>,
    mut settings: ResMut<LightingSettings>,
) {
    if buttons.contains(event.entity) {
        settings.use_csm = !settings.use_csm;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_lighting_settings(
    settings: Res<LightingSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lights: Query<(&mut DirectionalLight, Option<&ShadowLight>)>,
    mut cameras: Query<&mut ShadowFilteringMethod, With<Camera3d>>,
    scene_materials: Query<&MeshMaterial3d<StandardMaterial>, With<SceneMesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut filter_labels: Query<&mut Text, (With<ShadowFilterLabel>, Without<CsmLabel>)>,
    mut csm_labels: Query<&mut Text, (With<CsmLabel>, Without<ShadowFilterLabel>)>,
) {
    if !settings.is_changed() {
        return;
    }
    ambient.brightness = settings.ambient_brightness();
    for (mut light, is_shadow_light) in &mut lights {
        if is_shadow_light.is_some() {
            light.illuminance = 10_000.0 * settings.shadow;
            light.shadow_maps_enabled = settings.use_csm;
            light.shadow_depth_bias = settings.bevy_shadow_depth_bias();
            light.shadow_normal_bias = settings.bevy_shadow_normal_bias();
        } else {
            light.illuminance = 10_000.0 * settings.directional;
        }
    }
    for mut filter in &mut cameras {
        *filter = settings.shadow_filter.method();
    }
    for handle in &scene_materials {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.reflectance = settings.specular;
        }
    }
    for mut label in &mut filter_labels {
        label.0 = format!("Shadow filter: {}", settings.shadow_filter.label());
    }
    for mut label in &mut csm_labels {
        label.0 = format!("Cascaded shadows: {}", on_off(settings.use_csm));
    }
}

const fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn apply_window_state(
    state: Res<StartupWindowState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if state.maximized
        && let Ok(mut window) = windows.single_mut()
    {
        window.set_maximized(true);
    }
}

#[allow(clippy::too_many_arguments)]
fn orbit_camera(
    mut camera: Single<&mut Transform, With<Camera3d>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    scroll_conversion: Res<MouseScrollPixelsPerLine>,
    palette: Res<CommandPaletteState>,
    right_click: Res<NativeRightClickGesture>,
    context_menu: Res<NativeToolsContextMenuState>,
    ui_interactions: Query<(Option<&Hovered>, Has<Pressed>), With<NativeUiInteraction>>,
    mut orbit: ResMut<OrbitCamera>,
) {
    let over_ui = ui_interactions
        .iter()
        .any(|(hovered, pressed)| pressed || hovered.is_some_and(Hovered::get));
    if palette.open || context_menu.open || over_ui {
        return;
    }
    if buttons.pressed(MouseButton::Right) && right_click.dragged {
        orbit.yaw -= motion.delta.x * 0.006;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.006)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
    }
    let zoom = scroll.to_lines(&scroll_conversion).delta.y;
    if zoom != 0.0 {
        orbit.distance = (orbit.distance * (-zoom * 0.12).exp()).clamp(0.1, 100_000.0);
    }
    let horizontal = orbit.distance * orbit.pitch.cos();
    let offset = Vec3::new(
        horizontal * orbit.yaw.sin(),
        orbit.distance * orbit.pitch.sin(),
        horizontal * orbit.yaw.cos(),
    );
    **camera = Transform::from_translation(orbit.target + offset).looking_at(orbit.target, Vec3::Y);
}

#[cfg(test)]
mod tests {
    use super::{
        BevyImage, Button, CommandCatalog, CommandPaletteOverlay, CommandPaletteParameterError,
        CommandPaletteParameterHint, CommandPaletteParameterInput, CommandPaletteParameterPanel,
        CommandPaletteResult, CommandPaletteResultLabel, CommandPaletteState,
        ConstructionGuideKind, DispatchCommand, EditContext, GridNormalAxis, InputFocus,
        LightingChannel, LightingSettings, NativeCursorDot, NativeIconAtlas, NativeInspectorText,
        NativeModelSettingField, NativeModelSettingsApplyButton, NativeModelSettingsEditorButton,
        NativeModelSettingsInput, NativeObjectListClick, NativeObjectListContainer,
        NativeObjectListRow, NativeObjectNameInput, NativePolylineSettingsApplyButton,
        NativeRenameObjectButton, NativeRightClickGesture, NativeSelectionDisplayProperty,
        NativeSelectionDisplayState, NativeSelectionDisplayToggle, NativeSelectionToggleLabel,
        NativeSelectionValue, NativeSelectionValueLabel, NativeToolbarDrag,
        NativeToolsContextMenuState, NativeUiInteraction, NativeUiPreferencesState,
        NativeUiSettingChoice, NativeUiSettingChoiceButton, NativeUiSettingInput,
        NativeUiSettingInputField, NativeUiSettingToggleButton, NativeUiSettingToggleField,
        NativeUiSettingsApplyButton, NativeViewportOverlay, SceneDocument, ShadowFilter,
        SnapResult, TextureAtlasLayout, apply_native_model_settings,
        apply_native_polyline_settings, apply_native_ui_preferences, back_face_color,
        build_oriented_grid_mesh, build_scene_meshes_with_policy, command_payload_from_json,
        create_native_icon_atlas, dispatch_palette_command, dispatch_toolbar_command, line_mesh,
        native_model_setting_fields, native_object_entries, native_polyline_setting_fields,
        native_segment_intersects_rect, native_toolbar_dimensions, native_toolbar_position,
        native_triangle_intersects_rect, object_selection_summary, objects_summary,
        open_native_model_settings_editor, persist_native_toolbar_position,
        rename_selected_native_object, route_native_guide_shortcuts, scene_counts,
        search_command_ids, select_native_object_row, selected_object_info,
        selected_vertex_positions, submit_palette_parameters, sync_command_palette_parameter_ui,
        sync_command_palette_states, sync_native_cursor_dot, sync_native_model_settings_inputs,
        sync_native_object_list, sync_native_selection_table, sync_native_settings_sections,
        sync_toolbar_button_states, toggle_native_ui_setting, tool_context_groups, triangle_mesh,
        update_native_ui_setting_choice, update_right_click_gesture,
    };
    use crate::MeshcraftApp;
    use bevy::{
        asset::Assets,
        mesh::{Mesh, VertexAttributeValues},
        prelude::{
            App, BackgroundColor, ButtonInput, Color, Display, KeyCode, Node, Text, TextColor,
            Update, Val, Vec2, Vec3, default,
        },
        text::EditableText,
        time::Time,
        ui::Pressed,
        ui_widgets::Activate,
    };
    use meshcraft_core::commands::CommandPayload;
    use meshcraft_core::{
        document::Document,
        geometry::Vec3 as ModelVec3,
        selection::{
            SelectedEntity, SelectionDisplayPolicy, SelectionKind, SelectionMode, SelectionTarget,
        },
    };

    #[test]
    fn scene_meshes_contain_front_back_faces_edges_and_grid() {
        let mut document = Document::new();
        document
            .add_cuboid("sample", ModelVec3::new(2.0, 3.0, 4.0), ModelVec3::ZERO)
            .expect("valid cuboid");

        let built = build_scene_meshes_with_policy(&document, &SelectionDisplayPolicy::default());
        assert_eq!(
            attribute_len(
                built.front_faces.as_ref().unwrap(),
                Mesh::ATTRIBUTE_POSITION
            ),
            36
        );
        assert_eq!(
            attribute_len(built.back_faces.as_ref().unwrap(), Mesh::ATTRIBUTE_POSITION),
            36
        );
        assert_eq!(
            attribute_len(built.segments.as_ref().unwrap(), Mesh::ATTRIBUTE_POSITION),
            24
        );
        assert!(
            attribute_len(
                &build_oriented_grid_mesh(40, GridNormalAxis::Y),
                Mesh::ATTRIBUTE_POSITION
            ) > 300
        );
        assert_eq!(built.target, Vec3::new(1.0, 1.5, 2.0));
        assert_eq!(scene_counts(&document), (1, 12, 12));
    }

    #[test]
    fn selection_display_filters_scene_batches_and_builds_separate_face_wireframe() {
        let mut document = Document::new();
        document
            .add_cuboid("sample", ModelVec3::new(2.0, 3.0, 4.0), ModelVec3::ZERO)
            .expect("valid cuboid");

        let mut policy = SelectionDisplayPolicy::default();
        policy.set_visible(SelectionKind::Face, false);
        let without_faces = build_scene_meshes_with_policy(&document, &policy);
        assert!(without_faces.front_faces.is_none());
        assert!(without_faces.back_faces.is_none());
        assert!(without_faces.segments.is_some());

        policy.set_visible(SelectionKind::Face, true);
        policy.set_wireframe(SelectionKind::Face, true);
        let wireframe = build_scene_meshes_with_policy(&document, &policy);
        assert!(wireframe.front_faces.is_none());
        assert!(wireframe.back_faces.is_none());
        assert!(wireframe.face_wireframe.is_some());
        assert!(wireframe.segments.is_some());

        policy.set_visible(SelectionKind::Edge, false);
        let wireframe_without_authored_edges = build_scene_meshes_with_policy(&document, &policy);
        assert!(wireframe_without_authored_edges.segments.is_none());
        assert!(wireframe_without_authored_edges.face_wireframe.is_some());

        policy.set_visible(SelectionKind::Object, false);
        let hidden_objects = build_scene_meshes_with_policy(&document, &policy);
        assert!(hidden_objects.front_faces.is_none());
        assert!(hidden_objects.segments.is_none());
        assert!(hidden_objects.face_wireframe.is_none());
    }

    #[test]
    fn dragged_toolbar_positions_use_shared_preferences_and_restore_within_window() {
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        assert_eq!(
            Some("construction_points"),
            MeshcraftApp::native_toolbar_position_key(0)
        );
        persist_native_toolbar_position(&mut session, 0, 132.0, 84.0);
        assert_eq!(
            Some([132.0, 84.0]),
            session.native_ui_preferences().toolbar_positions[0]
        );

        let default = Vec2::new(10.0, 62.0);
        let window = Vec2::new(800.0, 600.0);
        let toolbar = Vec2::new(200.0, 100.0);
        assert_eq!(
            default,
            native_toolbar_position(default, None, window, toolbar)
        );
        assert_eq!(
            Vec2::new(600.0, 500.0),
            native_toolbar_position(default, Some([900.0, 700.0]), window, toolbar)
        );
        assert_eq!(
            default,
            native_toolbar_position(default, Some([f32::NAN, 20.0]), window, toolbar,)
        );
        assert_eq!(
            Vec2::new(300.0, 89.0),
            native_toolbar_dimensions(12, 32.0, 320.0, 4, false)
        );
        assert_eq!(
            Vec2::new(108.0, 153.0),
            native_toolbar_dimensions(12, 32.0, 320.0, 4, true)
        );
        assert_eq!(
            Vec2::new(204.0, 169.0),
            native_toolbar_dimensions(12, 48.0, 220.0, 3, false)
        );
    }

    #[test]
    fn selection_table_refreshes_totals_and_display_controls_from_session_policy() {
        let document = Document::new();
        let mut app = App::new();
        app.insert_resource(SceneDocument {
            document: document.clone(),
            generation: 1,
            revision: document.revision(),
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 1,
            selected_objects: 2,
            selected_vertices: 3,
            selected_faces: 4,
            selected_edges: 5,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 2,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries: Vec::new(),
            occurrences: 16,
            triangles: 460,
            segments: 524,
        })
        .insert_resource(NativeSelectionDisplayState {
            policy: SelectionDisplayPolicy::default(),
            render_revision: 0,
        })
        .add_systems(Update, sync_native_selection_table);
        let faces_total = app
            .world_mut()
            .spawn((
                Text::new(""),
                NativeSelectionValueLabel {
                    kind: SelectionKind::Face,
                    value: NativeSelectionValue::Total,
                },
            ))
            .id();
        let faces_selected = app
            .world_mut()
            .spawn((
                Text::new(""),
                NativeSelectionValueLabel {
                    kind: SelectionKind::Face,
                    value: NativeSelectionValue::Selected,
                },
            ))
            .id();
        let face_display_label = app
            .world_mut()
            .spawn((
                Text::new(""),
                NativeSelectionToggleLabel {
                    kind: SelectionKind::Face,
                    property: NativeSelectionDisplayProperty::Display,
                },
            ))
            .id();
        let face_display_toggle = app
            .world_mut()
            .spawn((
                Button,
                NativeSelectionDisplayToggle {
                    kind: SelectionKind::Face,
                    property: NativeSelectionDisplayProperty::Display,
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();

        app.update();

        assert_eq!("460", app.world().get::<Text>(faces_total).unwrap().0);
        assert_eq!("4", app.world().get::<Text>(faces_selected).unwrap().0);
        assert_eq!("✓", app.world().get::<Text>(face_display_label).unwrap().0);

        app.world_mut()
            .resource_mut::<NativeSelectionDisplayState>()
            .policy
            .set_visible(SelectionKind::Face, false);
        app.update();

        assert_eq!("", app.world().get::<Text>(face_display_label).unwrap().0);
        assert_eq!(
            Color::srgb(0.20, 0.21, 0.22),
            app.world()
                .get::<BackgroundColor>(face_display_toggle)
                .unwrap()
                .0
        );
    }

    #[test]
    fn native_permanent_grid_uses_the_selected_normal_axis() {
        for (axis, normal_endpoint) in [
            (GridNormalAxis::X, [2.0, 0.0, 0.0]),
            (GridNormalAxis::Y, [0.0, 2.0, 0.0]),
            (GridNormalAxis::Z, [0.0, 0.0, 2.0]),
        ] {
            let grid = build_oriented_grid_mesh(2, axis);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                grid.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("native grid has position data");
            };
            assert!(positions.contains(&normal_endpoint));
            assert!(
                positions
                    .iter()
                    .all(|point| point.iter().all(|value| value.is_finite()))
            );
        }
    }

    #[test]
    fn builders_skip_empty_meshes_and_tint_back_faces_blueward() {
        assert!(triangle_mesh(Vec::new(), Vec::new(), Vec::new()).is_none());
        assert!(line_mesh(Vec::new(), Vec::new()).is_none());
        let back = back_face_color(meshcraft_core::document::Rgba8::new(100, 100, 100, 255));
        assert!(back[0] < back[1] && back[1] < back[2]);
    }

    #[test]
    fn lighting_defaults_match_k3d_and_ambient_lifts_shaded_faces() {
        let settings = LightingSettings::default();
        assert_eq!(settings.shadow, 0.59);
        assert_eq!(settings.directional, 0.73);
        assert_eq!(settings.ambient, 0.59);
        assert_eq!(settings.specular, 0.20);
        assert_eq!(settings.shadow_filter, ShadowFilter::Hardware2x2);
        assert!(settings.ambient_brightness() > 80.0);
    }

    #[test]
    fn lighting_slider_values_are_clamped_to_their_shader_range() {
        let mut settings = LightingSettings::default();
        LightingChannel::Ambient.set_value(&mut settings, 2.0);
        assert_eq!(settings.ambient, 1.0);
        LightingChannel::Specular.set_value(&mut settings, -1.0);
        assert_eq!(settings.specular, 0.0);
    }

    #[test]
    fn command_palette_search_uses_the_shared_command_registry() {
        let catalog = MeshcraftApp::command_catalog();
        let results = search_command_ids(&catalog, "cuboid", 24);
        let ids = results
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"tool.cuboid"));
        assert!(ids.contains(&"geometry.add_cuboid"));

        let by_keyword = search_command_ids(&catalog, "undo", 24);
        assert!(
            by_keyword
                .iter()
                .any(|command| command.id.as_str() == "edit.undo")
        );
    }

    #[test]
    fn native_command_session_mutations_refresh_the_render_source_document() {
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        session.set_native_project_path(std::path::PathBuf::from("scene.octd"));
        assert_eq!(
            Some("scene.octd".to_owned()),
            session.current_project_path()
        );
        let before_revision = session.document_revision();
        let dispatched =
            session.dispatch_command("geometry.add_cuboid", &CommandPayload::default());
        assert!(dispatched.accepted);
        assert!(session.document_revision() > before_revision);

        let rendered_document = session.document_snapshot();
        let meshes =
            build_scene_meshes_with_policy(&rendered_document, &SelectionDisplayPolicy::default());
        assert!(meshes.front_faces.is_some());
        assert!(meshes.back_faces.is_some());
        assert_eq!(scene_counts(&rendered_document), (1, 12, 12));
    }

    #[test]
    fn native_selector_windows_distinguish_containment_from_crossing() {
        let region = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)];
        let contained_triangle = [
            Vec2::new(2.0, 2.0),
            Vec2::new(8.0, 2.0),
            Vec2::new(5.0, 8.0),
        ];
        let crossing_triangle = [
            Vec2::new(-5.0, 1.0),
            Vec2::new(15.0, 1.0),
            Vec2::new(5.0, 16.0),
        ];

        assert!(native_triangle_intersects_rect(
            contained_triangle,
            region,
            false
        ));
        assert!(native_triangle_intersects_rect(
            contained_triangle,
            region,
            true
        ));
        assert!(!native_triangle_intersects_rect(
            crossing_triangle,
            region,
            false
        ));
        assert!(native_triangle_intersects_rect(
            crossing_triangle,
            region,
            true
        ));
        assert!(native_segment_intersects_rect(
            Vec2::new(-2.0, 5.0),
            Vec2::new(3.0, 5.0),
            region
        ));
    }

    #[test]
    fn native_toolbar_disables_unavailable_commands_and_marks_active_tools() {
        let document = Document::new();
        let mut app = App::new();
        app.insert_resource(SceneDocument {
            document: document.clone(),
            generation: 0,
            revision: 0,
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 0,
            selected_objects: 0,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 0,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries: Vec::new(),
            occurrences: 0,
            triangles: 0,
            segments: 0,
        })
        .insert_non_send(MeshcraftApp::new_native_session(
            document,
            "Ready".to_owned(),
            None,
        ))
        .add_systems(Update, sync_toolbar_button_states);
        let select = app
            .world_mut()
            .spawn((
                Button,
                DispatchCommand("tool.select".to_owned()),
                BackgroundColor(Color::WHITE),
            ))
            .id();
        let push_pull = app
            .world_mut()
            .spawn((
                Button,
                DispatchCommand("tool.builtin.push_pull".to_owned()),
                BackgroundColor(Color::WHITE),
            ))
            .id();
        app.update();

        assert!(app.world().get::<Button>(select).is_some());
        assert_eq!(
            Color::srgb(0.12, 0.38, 0.62),
            app.world().get::<BackgroundColor>(select).unwrap().0
        );
        assert!(app.world().get::<Button>(push_pull).is_none());
        assert_eq!(
            Color::srgb(0.14, 0.15, 0.16),
            app.world().get::<BackgroundColor>(push_pull).unwrap().0
        );
    }

    #[test]
    fn right_click_tools_menu_groups_catalog_commands_without_duplicates() {
        let catalog = MeshcraftApp::command_catalog();
        let groups = tool_context_groups(&catalog);
        assert!(!groups.is_empty());

        let ids = groups
            .iter()
            .flat_map(|(_, commands)| commands.iter().map(|command| command.id.as_str()))
            .collect::<Vec<_>>();
        assert!(ids.contains(&"tool.select"));
        assert!(ids.contains(&"tool.line"));
        assert!(ids.contains(&"tool.builtin.move"));
        assert!(!ids.contains(&"edit.undo"));
        assert_eq!(
            ids.len(),
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
    }

    #[test]
    fn viewport_right_click_opens_tools_menu_while_drag_continues_to_orbit() {
        let start = Vec2::new(300.0, 240.0);
        let mut gesture = NativeRightClickGesture::default();
        assert_eq!(
            None,
            update_right_click_gesture(&mut gesture, true, true, false, Some(start), true)
        );
        assert_eq!(
            Some(start),
            update_right_click_gesture(&mut gesture, false, false, true, Some(start), true)
        );
        assert!(gesture.start.is_none());
        assert!(!gesture.dragged);

        update_right_click_gesture(&mut gesture, true, true, false, Some(start), true);
        update_right_click_gesture(
            &mut gesture,
            false,
            true,
            false,
            Some(start + Vec2::new(8.0, 0.0)),
            true,
        );
        assert!(gesture.dragged);
        assert_eq!(
            None,
            update_right_click_gesture(
                &mut gesture,
                false,
                false,
                true,
                Some(start + Vec2::new(8.0, 0.0)),
                true,
            )
        );
        assert!(!gesture.dragged);

        update_right_click_gesture(&mut gesture, true, true, false, Some(start), false);
        assert_eq!(
            None,
            update_right_click_gesture(&mut gesture, false, false, true, Some(start), true)
        );
    }

    #[test]
    fn native_toolbar_atlas_uses_the_embedded_legacy_icon_mapping() {
        let mut images = Assets::<BevyImage>::default();
        let mut layouts = Assets::<TextureAtlasLayout>::default();

        let atlas: NativeIconAtlas = create_native_icon_atlas(&mut images, &mut layouts);

        assert!(images.get(&atlas.image).is_some());
        let layout = layouts
            .get(&atlas.layout)
            .expect("registered texture atlas");
        assert!(layout.len() >= 80);
        for key in ["select", "construction_line", "revolve", "hvac_plumbing"] {
            assert!(
                atlas.indices.contains_key(key),
                "missing toolbar icon {key}"
            );
        }
    }

    #[test]
    fn palette_refresh_system_builds_results_in_a_bevy_schedule() {
        let mut app = App::new();
        app.insert_resource(super::CommandCatalog(MeshcraftApp::command_catalog()))
            .insert_resource(super::CommandPaletteState {
                open: true,
                query: "cuboid".to_owned(),
                results: Vec::new(),
                selected_index: 0,
                ..default()
            })
            .add_systems(Update, super::refresh_command_palette);

        app.update();

        let state = app.world().resource::<super::CommandPaletteState>();
        assert!(state.results.contains(&"tool.cuboid".to_owned()));
        assert!(state.results.contains(&"geometry.add_cuboid".to_owned()));
    }

    #[test]
    fn command_palette_marks_disabled_commands_and_shows_the_reason() {
        let mut app = App::new();
        app.insert_resource(CommandPaletteState {
            open: true,
            query: "push".to_owned(),
            results: vec![
                "tool.select".to_owned(),
                "tool.builtin.push_pull".to_owned(),
            ],
            selected_index: 1,
            ..default()
        })
        .insert_resource(CommandCatalog(MeshcraftApp::command_catalog()))
        .insert_non_send(MeshcraftApp::new_native_session(
            Document::new(),
            "Ready".to_owned(),
            None,
        ))
        .add_systems(Update, sync_command_palette_states);
        let select = app
            .world_mut()
            .spawn((
                Button,
                CommandPaletteResult {
                    slot: 0,
                    command_id: Some("tool.select".to_owned()),
                    enabled: false,
                    checked: None,
                    disabled_reason: None,
                },
                Node {
                    display: Display::Flex,
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();
        let push_pull = app
            .world_mut()
            .spawn((
                Button,
                CommandPaletteResult {
                    slot: 1,
                    command_id: Some("tool.builtin.push_pull".to_owned()),
                    enabled: true,
                    checked: None,
                    disabled_reason: None,
                },
                Node {
                    display: Display::Flex,
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ))
            .id();
        let push_pull_label = app
            .world_mut()
            .spawn((
                CommandPaletteResultLabel(1),
                Text::new("Push/Pull"),
                TextColor(Color::WHITE),
            ))
            .id();

        app.update();

        assert!(app.world().get::<Button>(select).is_some());
        assert!(app.world().get::<Button>(push_pull).is_none());
        let disabled = app.world().get::<CommandPaletteResult>(push_pull).unwrap();
        assert!(!disabled.enabled);
        assert!(
            disabled
                .disabled_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("select one or more faces"))
        );
        assert!(
            app.world()
                .get::<Text>(push_pull_label)
                .unwrap()
                .0
                .contains("select one or more faces")
        );
    }

    #[test]
    fn palette_payload_parser_requires_a_json_object_and_preserves_typed_values() {
        let payload = command_payload_from_json(
            r#"{"dimensions":[2.0,3.0,4.0],"origin":[-1.0,0.0,-2.0],"enabled":true}"#,
        )
        .expect("valid command payload");
        assert_eq!(1, payload.schema_version);
        assert_eq!(3, payload.values.len());
        assert_eq!(
            serde_json::json!([2.0, 3.0, 4.0]),
            payload.values["dimensions"]
        );
        assert_eq!(serde_json::json!(true), payload.values["enabled"]);
        assert!(command_payload_from_json("[1, 2, 3]").is_err());
        assert!(command_payload_from_json("{").is_err());
    }

    #[test]
    fn palette_parameter_entry_dispatches_a_typed_command_payload() {
        let mut palette = CommandPaletteState {
            open: true,
            pending_command: Some("geometry.add_cuboid".to_owned()),
            parameter_json: r#"{"dimensions":[2.0,3.0,4.0],"origin":[-1.0,0.0,-2.0]}"#.to_owned(),
            parameter_cursor: 0,
            ..default()
        };
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        let previous_revision = session.document_revision();

        assert!(submit_palette_parameters(&mut palette, &mut session));

        assert!(!palette.open);
        assert!(palette.pending_command.is_none());
        assert!(session.document_revision() > previous_revision);
        assert_eq!(1, scene_counts(&session.document_snapshot()).0);
    }

    #[test]
    fn parameter_entry_rejection_keeps_the_command_and_explains_the_error() {
        let mut palette = CommandPaletteState {
            open: true,
            pending_command: Some("geometry.add_cuboid".to_owned()),
            parameter_json: "[]".to_owned(),
            parameter_cursor: 2,
            ..default()
        };
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);

        assert!(!submit_palette_parameters(&mut palette, &mut session));

        assert!(palette.open);
        assert_eq!(
            Some("geometry.add_cuboid"),
            palette.pending_command.as_deref()
        );
        assert!(
            palette
                .parameter_error
                .as_deref()
                .is_some_and(|error| error.contains("JSON object"))
        );
        assert_eq!(0, session.document_revision());
    }

    #[test]
    fn model_settings_inspector_opens_the_shared_settings_command_editor() {
        let document = Document::new();
        let revision = document.revision();
        let (occurrences, triangles, segments) = scene_counts(&document);
        let scene = SceneDocument {
            document,
            generation: 1,
            revision,
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 0,
            selected_objects: 0,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 0,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries: Vec::new(),
            occurrences,
            triangles,
            segments,
        };
        let mut app = App::new();
        app.insert_resource(scene)
            .insert_resource(CommandPaletteState::default())
            .add_observer(open_native_model_settings_editor);
        app.world_mut().spawn((
            CommandPaletteOverlay,
            Node {
                display: Display::None,
                ..default()
            },
        ));
        let button = app.world_mut().spawn(NativeModelSettingsEditorButton).id();

        app.world_mut().trigger(Activate { entity: button });

        let palette = app.world().resource::<CommandPaletteState>();
        assert!(palette.open);
        assert_eq!(
            Some("model.update_settings"),
            palette.pending_command.as_deref()
        );
        let parameters: serde_json::Value =
            serde_json::from_str(&palette.parameter_json).expect("valid settings payload");
        assert_eq!(serde_json::json!("unit"), parameters["unit_name"]);
        assert_eq!(serde_json::json!(1.0), parameters["grid_size"]);
        assert_eq!(serde_json::json!(24), parameters["circle_segments"]);
    }

    #[test]
    fn native_model_settings_apply_is_atomic_undoable_and_validated() {
        let document = Document::new();
        let mut session =
            MeshcraftApp::new_native_session(document.clone(), "Ready".to_owned(), None);
        let mut app = App::new();
        app.insert_non_send(session)
            .add_observer(apply_native_model_settings)
            .add_observer(apply_native_polyline_settings);
        let mut grid_input = None;
        for (field, _, _) in native_model_setting_fields(&document)
            .into_iter()
            .chain(native_polyline_setting_fields(&document))
        {
            let value = match field {
                NativeModelSettingField::UnitName => "meter",
                NativeModelSettingField::UnitSize => "0.01",
                NativeModelSettingField::GridSize => "0.25",
                NativeModelSettingField::CircleSegments => "48",
                NativeModelSettingField::SnapRadius => "24",
                NativeModelSettingField::ArcMaxLength => "0.75",
                NativeModelSettingField::DoubleLineSize => "2.0",
                NativeModelSettingField::DoubleLineOffset => "-0.3",
            };
            let entity = app
                .world_mut()
                .spawn((
                    NativeModelSettingsInput {
                        generation: 1,
                        revision: document.revision(),
                        field,
                    },
                    EditableText::new(value),
                ))
                .id();
            if field == NativeModelSettingField::GridSize {
                grid_input = Some(entity);
            }
        }
        let model_button = app.world_mut().spawn(NativeModelSettingsApplyButton).id();
        let polyline_button = app
            .world_mut()
            .spawn(NativePolylineSettingsApplyButton)
            .id();

        app.world_mut().trigger(Activate {
            entity: model_button,
        });

        session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        let settings = session.document_snapshot().model_settings().clone();
        assert_eq!("meter", settings.unit_name);
        assert_eq!(0.01, settings.unit_size);
        assert_eq!(0.25, settings.grid_size);
        assert_eq!(48, settings.circle_segments);
        assert_eq!(24.0, settings.snap_radius);
        assert_eq!(
            document.model_settings().arc_max_length,
            settings.arc_max_length
        );
        assert_eq!(
            document.model_settings().double_line_size,
            settings.double_line_size
        );
        assert_eq!(
            document.model_settings().double_line_offset,
            settings.double_line_offset
        );
        assert!(session.native_command_state("edit.undo").enabled);
        session.dispatch_command("edit.undo", &CommandPayload::default());
        assert_eq!(
            document.model_settings(),
            session.document_snapshot().model_settings()
        );
        app.insert_non_send(session);

        app.world_mut().trigger(Activate {
            entity: polyline_button,
        });
        session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        let settings = session.document_snapshot().model_settings().clone();
        assert_eq!(document.model_settings().unit_name, settings.unit_name);
        assert_eq!(0.75, settings.arc_max_length);
        assert_eq!(2.0, settings.double_line_size);
        assert_eq!(-0.3, settings.double_line_offset);
        assert!(session.native_command_state("edit.undo").enabled);
        session.dispatch_command("edit.undo", &CommandPayload::default());
        assert_eq!(
            document.model_settings(),
            session.document_snapshot().model_settings()
        );
        let revision_after_undo = session.document_revision();
        app.insert_non_send(session);

        let grid_input = grid_input.expect("the grid-size input was created");
        app.world_mut()
            .get_mut::<EditableText>(grid_input)
            .expect("grid-size input remains in the inspector")
            .editor_mut()
            .set_text("0.0");
        app.world_mut().trigger(Activate {
            entity: model_button,
        });

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert_eq!(revision_after_undo, session.document_revision());
        assert_eq!(
            document.model_settings(),
            session.document_snapshot().model_settings()
        );
        assert!(
            session
                .status_text()
                .contains("grid size must be finite and positive")
        );
    }

    #[test]
    fn native_model_settings_inputs_refresh_and_preserve_the_focused_draft() {
        let mut document = Document::new();
        let mut settings = document.model_settings().clone();
        settings.unit_name = "meters".to_owned();
        settings.grid_size = 2.5;
        settings.arc_max_length = 0.75;
        document
            .update_model_settings(settings)
            .expect("model settings are valid");
        let revision = document.revision();
        let (occurrences, triangles, segments) = scene_counts(&document);
        let mut app = App::new();
        app.insert_resource(SceneDocument {
            document,
            generation: 2,
            revision,
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 0,
            selected_objects: 0,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 0,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries: Vec::new(),
            occurrences,
            triangles,
            segments,
        })
        .insert_resource(InputFocus::default())
        .add_systems(Update, sync_native_model_settings_inputs);
        let grid = app
            .world_mut()
            .spawn((
                EditableText::new("draft grid"),
                NativeModelSettingsInput {
                    generation: 2,
                    revision: 0,
                    field: NativeModelSettingField::GridSize,
                },
            ))
            .id();
        let unit = app
            .world_mut()
            .spawn((
                EditableText::new("unit"),
                NativeModelSettingsInput {
                    generation: 2,
                    revision: 0,
                    field: NativeModelSettingField::UnitName,
                },
            ))
            .id();
        let arc = app
            .world_mut()
            .spawn((
                EditableText::new("draft arc"),
                NativeModelSettingsInput {
                    generation: 2,
                    revision: 0,
                    field: NativeModelSettingField::ArcMaxLength,
                },
            ))
            .id();
        *app.world_mut().resource_mut::<InputFocus>() = InputFocus::from_entity(grid);

        app.update();

        assert_eq!(
            "draft grid",
            app.world()
                .get::<EditableText>(grid)
                .unwrap()
                .value()
                .to_string()
        );
        assert_eq!(
            "meters",
            app.world()
                .get::<EditableText>(unit)
                .unwrap()
                .value()
                .to_string()
        );
        assert_eq!(
            "0.750",
            app.world()
                .get::<EditableText>(arc)
                .unwrap()
                .value()
                .to_string()
        );

        app.world_mut()
            .get_mut::<EditableText>(arc)
            .unwrap()
            .editor_mut()
            .set_text("draft arc");
        *app.world_mut().resource_mut::<InputFocus>() = InputFocus::from_entity(arc);
        {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            let mut settings = scene.document.model_settings().clone();
            settings.arc_max_length = 0.9;
            scene.document.update_model_settings(settings).unwrap();
            scene.revision = scene.document.revision();
        }
        app.update();
        assert_eq!(
            "draft arc",
            app.world()
                .get::<EditableText>(arc)
                .unwrap()
                .value()
                .to_string()
        );

        *app.world_mut().resource_mut::<InputFocus>() = InputFocus::default();
        app.update();
        assert_eq!(
            "2.5000",
            app.world()
                .get::<EditableText>(grid)
                .unwrap()
                .value()
                .to_string()
        );
        assert_eq!(
            "0.900",
            app.world()
                .get::<EditableText>(arc)
                .unwrap()
                .value()
                .to_string()
        );

        *app.world_mut().resource_mut::<InputFocus>() = InputFocus::from_entity(grid);
        {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            scene.document = Document::new();
            scene.generation += 1;
            scene.revision = scene.document.revision();
        }
        app.update();
        assert_eq!(
            "1.0000",
            app.world()
                .get::<EditableText>(grid)
                .unwrap()
                .value()
                .to_string()
        );
    }

    #[test]
    fn native_ui_preferences_apply_atomically_and_validate_ranges() {
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        let mut app = App::new();
        app.insert_non_send(session)
            .add_observer(apply_native_ui_preferences);
        let values = [
            (NativeUiSettingInputField::TextScale, "1.25"),
            (NativeUiSettingInputField::ToolbarIconSize, "40"),
            (NativeUiSettingInputField::GridExtent, "12"),
            (NativeUiSettingInputField::NormalGridWidth, "1.5"),
            (NativeUiSettingInputField::ThickGridWidth, "4"),
        ];
        let mut grid_extent = None;
        let mut text_scale = None;
        for (field, value) in values {
            let entity = app
                .world_mut()
                .spawn((
                    NativeUiSettingInput { field, revision: 0 },
                    EditableText::new(value),
                ))
                .id();
            if field == NativeUiSettingInputField::GridExtent {
                grid_extent = Some(entity);
            } else if field == NativeUiSettingInputField::TextScale {
                text_scale = Some(entity);
            }
        }
        let button = app.world_mut().spawn(NativeUiSettingsApplyButton).id();
        app.world_mut().trigger(Activate { entity: button });

        session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        let applied = session.native_ui_preferences();
        assert_eq!(1.25, applied.ui_text_scale);
        assert_eq!(40, applied.toolbar_icon_size);
        assert_eq!(12, applied.grid_extent);
        assert_eq!(1.5, applied.normal_grid_width);
        assert_eq!(4.0, applied.thick_grid_width);
        app.insert_non_send(session);

        app.world_mut()
            .get_mut::<EditableText>(grid_extent.expect("grid extent input was created"))
            .expect("grid extent input remains available")
            .editor_mut()
            .set_text("0");
        app.world_mut()
            .get_mut::<EditableText>(text_scale.expect("text size input was created"))
            .expect("text size input remains available")
            .editor_mut()
            .set_text("1.5");
        app.world_mut().trigger(Activate { entity: button });

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert_eq!(applied, session.native_ui_preferences());
        assert!(
            session
                .status_text()
                .contains("grid_extent must be between 1 and 1000")
        );
    }

    #[test]
    fn native_ui_preference_toggles_and_choices_use_shared_settings_command() {
        let session = MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        let initial = session.native_ui_preferences();
        let mut app = App::new();
        app.insert_non_send(session)
            .insert_resource(NativeUiPreferencesState {
                value: initial,
                revision: 0,
            })
            .add_observer(toggle_native_ui_setting)
            .add_observer(update_native_ui_setting_choice);
        let auto_collapse = app
            .world_mut()
            .spawn(NativeUiSettingToggleButton(
                NativeUiSettingToggleField::AutoCollapseToolbars,
            ))
            .id();
        let permanent_grid = app
            .world_mut()
            .spawn(NativeUiSettingToggleButton(
                NativeUiSettingToggleField::PermanentGrid,
            ))
            .id();
        let guides = app
            .world_mut()
            .spawn(NativeUiSettingToggleButton(
                NativeUiSettingToggleField::ConstructionGuides,
            ))
            .id();
        let grid_axis = app
            .world_mut()
            .spawn(NativeUiSettingChoiceButton(
                NativeUiSettingChoice::GridAxis(GridNormalAxis::X),
            ))
            .id();
        let flow = app
            .world_mut()
            .spawn(NativeUiSettingChoiceButton(
                NativeUiSettingChoice::ToolbarFlow(true),
            ))
            .id();

        for entity in [auto_collapse, permanent_grid, guides, grid_axis, flow] {
            app.world_mut().trigger(Activate { entity });
        }

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        let preferences = session.native_ui_preferences();
        assert!(preferences.auto_collapse_toolbars);
        assert!(!preferences.show_permanent_grid);
        assert!(!preferences.show_construction_guides);
        assert_eq!(GridNormalAxis::X, preferences.grid_normal_axis);
        assert!(preferences.toolbar_flow_top_down);
    }

    #[test]
    fn native_g_and_t_shortcuts_place_guides_at_the_cached_cursor() {
        for (key, expected_kind) in [
            (KeyCode::KeyG, ConstructionGuideKind::Grid),
            (KeyCode::KeyT, ConstructionGuideKind::Axis),
        ] {
            let mut session =
                MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
            let snap = SnapResult {
                point: ModelVec3::new(2.0, 3.0, 4.0),
                normal: ModelVec3::Y,
                kind: crate::viewport::SnapKind::Face,
                screen_distance: 0.0,
            };
            session.set_native_cursor_snap(Some(snap));
            let mut keyboard = ButtonInput::<KeyCode>::default();
            keyboard.press(key);
            let mut app = App::new();
            app.insert_resource(keyboard)
                .insert_resource(InputFocus::default())
                .insert_resource(CommandPaletteState::default())
                .insert_resource(NativeToolsContextMenuState::default())
                .insert_resource(NativeToolbarDrag::default())
                .insert_non_send(session)
                .add_systems(Update, route_native_guide_shortcuts);

            app.update();

            let session = app
                .world_mut()
                .remove_non_send::<MeshcraftApp>()
                .expect("native session remains available");
            assert_eq!("Select", session.active_tool_label());
            let guide = session
                .native_construction_guides()
                .first()
                .expect("shortcut placed a guide at the cached cursor");
            assert_eq!(expected_kind, guide.kind);
            assert_eq!(snap.point, guide.origin);
        }
    }

    #[test]
    fn selected_vertex_bullets_resolve_world_positions_for_explicit_vertex_targets() {
        let mut document = Document::new();
        let occurrence = document
            .add_line_segment(
                "line",
                ModelVec3::new(1.0, 2.0, 3.0),
                ModelVec3::new(4.0, 2.0, 3.0),
            )
            .expect("line occurrence is valid");
        let (vertex_id, expected_position) = {
            let geometry = document
                .effective_geometry(&occurrence)
                .expect("line geometry is available");
            let vertex = geometry
                .vertices
                .values()
                .next()
                .expect("line has vertices");
            (vertex.id, vertex.position)
        };
        let selection = [SelectionTarget {
            occurrence,
            entity: SelectedEntity::Vertex(vertex_id),
        }];

        assert_eq!(
            vec![expected_position],
            selected_vertex_positions(&document, &selection)
        );
    }

    #[test]
    fn native_guide_shortcuts_are_suppressed_while_ui_is_pressed() {
        let mut session =
            MeshcraftApp::new_native_session(Document::new(), "Ready".to_owned(), None);
        session.set_native_cursor_snap(Some(SnapResult {
            point: ModelVec3::new(2.0, 3.0, 4.0),
            normal: ModelVec3::Y,
            kind: crate::viewport::SnapKind::Face,
            screen_distance: 0.0,
        }));
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyG);
        let mut app = App::new();
        app.insert_resource(keyboard)
            .insert_resource(InputFocus::default())
            .insert_resource(CommandPaletteState::default())
            .insert_resource(NativeToolsContextMenuState::default())
            .insert_resource(NativeToolbarDrag::default())
            .insert_non_send(session)
            .add_systems(Update, route_native_guide_shortcuts);
        let ui_entity = app.world_mut().spawn((NativeUiInteraction, Pressed)).id();
        app.world_mut()
            .insert_resource(InputFocus::from_entity(ui_entity));

        app.update();

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert!(session.native_construction_guides().is_empty());
    }

    #[test]
    fn native_cursor_dot_uses_screen_coordinates_and_hides_without_a_cursor() {
        let overlay = NativeViewportOverlay {
            cursor_screen: Some(Vec2::new(110.0, 70.0)),
            ..default()
        };
        let mut app = App::new();
        app.insert_resource(overlay)
            .add_systems(Update, sync_native_cursor_dot);
        let dot = app
            .world_mut()
            .spawn((
                NativeCursorDot,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.update();

        let node = app.world().get::<Node>(dot).unwrap();
        assert_eq!(Display::Flex, node.display);
        assert_eq!(Val::Px(106.5), node.left);
        assert_eq!(Val::Px(66.5), node.top);

        app.world_mut()
            .resource_mut::<NativeViewportOverlay>()
            .clear_cursor();
        app.update();
        assert_eq!(Display::None, app.world().get::<Node>(dot).unwrap().display);
    }

    #[test]
    fn native_object_inspector_renames_the_selected_definition_transactionally() {
        let mut document = Document::new();
        let occurrence = document
            .add_line_segment("original", ModelVec3::ZERO, ModelVec3::X)
            .expect("object occurrence is valid");
        let target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        };
        let mut session =
            MeshcraftApp::new_native_session(document.clone(), "Ready".to_owned(), None);
        session.apply_native_selection_region(vec![target], false, SelectionMode::Replace);
        let selection_targets = session.native_selection_targets();
        let (selected_object_count, selected_object_definition_id, selected_object_name) =
            selected_object_info(&document, &selection_targets);
        let object_entries = native_object_entries(&document, vec![occurrence.clone()]);
        let (occurrences, triangles, segments) = scene_counts(&document);
        let scene = SceneDocument {
            document,
            generation: 1,
            revision: session.document_revision(),
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: session.native_selection_revision(),
            selected_objects: 1,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets,
            selected_vertex_positions: Vec::new(),
            selected_object_count,
            selected_object_definition_id,
            selected_object_name,
            object_entries,
            occurrences,
            triangles,
            segments,
        };
        assert!(object_selection_summary(&scene).contains("1 object selected"));
        let mut app = App::new();
        app.insert_resource(scene)
            .insert_non_send(session)
            .add_observer(rename_selected_native_object);
        app.world_mut().spawn((
            NativeObjectNameInput {
                generation: 1,
                revision: 0,
                selection_revision: 1,
                definition_id: selected_object_definition_id,
            },
            EditableText::new("renamed"),
        ));
        let button = app
            .world_mut()
            .spawn((Button, NativeRenameObjectButton))
            .id();

        app.world_mut().trigger(Activate { entity: button });

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert_eq!(
            "renamed",
            session
                .document_snapshot()
                .resolve_occurrence(&occurrence)
                .unwrap()
                .definition
                .name
        );
        assert!(session.native_command_state("edit.undo").enabled);
    }

    #[test]
    fn native_object_list_double_click_enters_the_selected_instance() {
        let mut document = Document::new();
        let occurrence = document
            .add_line_segment("object", ModelVec3::ZERO, ModelVec3::X)
            .expect("object occurrence is valid");
        let session = MeshcraftApp::new_native_session(document, "Ready".to_owned(), None);
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(NativeObjectListClick::default())
            .insert_non_send(session)
            .add_observer(select_native_object_row);
        let row = app
            .world_mut()
            .spawn(NativeObjectListRow {
                occurrence: occurrence.clone(),
            })
            .id();

        app.world_mut().trigger(Activate { entity: row });
        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert_eq!(
            vec![SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::Occurrence,
            }],
            session.native_selection_targets()
        );
        app.insert_non_send(session);
        app.world_mut().trigger(Activate { entity: row });

        let session = app
            .world_mut()
            .remove_non_send::<MeshcraftApp>()
            .expect("native session remains available");
        assert_eq!(
            EditContext::Occurrence(occurrence),
            session.native_edit_context()
        );
    }

    #[test]
    fn native_object_list_refreshes_rows_and_selection_highlight() {
        let mut document = Document::new();
        let first = document
            .add_line_segment("first", ModelVec3::ZERO, ModelVec3::X)
            .expect("first object occurrence is valid");
        let revision = document.revision();
        let object_entries = native_object_entries(&document, vec![first.clone()]);
        let (occurrences, triangles, segments) = scene_counts(&document);
        let mut app = App::new();
        app.insert_resource(SceneDocument {
            document,
            generation: 1,
            revision,
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 0,
            selected_objects: 0,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 0,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries,
            occurrences,
            triangles,
            segments,
        })
        .add_systems(Update, sync_native_object_list);
        let container = app
            .world_mut()
            .spawn((
                Node::default(),
                NativeObjectListContainer {
                    generation: 0,
                    revision: 0,
                },
            ))
            .id();

        app.update();
        let mut rows = app.world_mut().query::<&NativeObjectListRow>();
        assert_eq!(1, rows.iter(app.world()).count());

        let second = {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            let second = scene
                .document
                .add_line_segment("second", ModelVec3::ZERO, ModelVec3::Y)
                .expect("second object occurrence is valid");
            scene.revision = scene.document.revision();
            scene.object_entries =
                native_object_entries(&scene.document, vec![first.clone(), second.clone()]);
            second
        };
        app.update();
        assert_eq!(
            2,
            rows.iter(app.world()).count(),
            "document edits rebuild the object rows",
        );

        {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            scene.selection_revision += 1;
            scene.selection_targets = vec![SelectionTarget {
                occurrence: first.clone(),
                entity: SelectedEntity::Occurrence,
            }];
        }
        app.update();
        let mut rows = app
            .world_mut()
            .query::<(&NativeObjectListRow, &BackgroundColor)>();
        let selected_color = rows
            .iter(app.world())
            .find_map(|(row, background)| (row.occurrence == first).then_some(background.0))
            .expect("selected object row remains visible");
        assert_eq!(Color::srgb(0.12, 0.38, 0.62), selected_color);
        assert!(app.world().get_entity(container).is_ok());
        assert!(
            app.world_mut()
                .query::<&NativeObjectListRow>()
                .iter(app.world())
                .any(|row| row.occurrence == second)
        );
    }

    #[test]
    fn native_object_inspector_summaries_refresh_after_document_changes() {
        let document = Document::new();
        let (occurrences, triangles, segments) = scene_counts(&document);
        let initial_objects_summary = objects_summary(occurrences);
        let mut app = App::new();
        app.insert_resource(SceneDocument {
            revision: document.revision(),
            document,
            generation: 1,
            status: "Ready".to_owned(),
            active_tool: "Select".to_owned(),
            selection_revision: 0,
            selected_objects: 0,
            selected_vertices: 0,
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
            selected_vertex_positions: Vec::new(),
            selected_object_count: 0,
            selected_object_definition_id: None,
            selected_object_name: None,
            object_entries: Vec::new(),
            occurrences,
            triangles,
            segments,
        })
        .add_systems(Update, sync_native_settings_sections);
        let objects = app
            .world_mut()
            .spawn((
                Text::new(initial_objects_summary),
                NativeInspectorText::Objects {
                    generation: 1,
                    revision: 0,
                },
            ))
            .id();
        app.update();
        {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            let mut settings = scene.document.model_settings().clone();
            settings.unit_name = "meter".to_owned();
            settings.grid_size = 0.25;
            scene.document.update_model_settings(settings).unwrap();
            scene.occurrences = 18;
            scene.revision = scene.document.revision();
        }
        app.update();

        assert_eq!(
            "18 object instances in the scene",
            app.world().get::<Text>(objects).unwrap().0
        );

        {
            let mut scene = app.world_mut().resource_mut::<SceneDocument>();
            scene.document = Document::new();
            scene.generation += 1;
            scene.revision = scene.document.revision();
            scene.occurrences = 0;
        }
        app.update();

        assert_eq!(
            "0 object instances in the scene",
            app.world().get::<Text>(objects).unwrap().0
        );
    }

    #[test]
    fn palette_parameter_panel_shows_schema_input_and_errors() {
        let mut app = App::new();
        app.insert_resource(CommandPaletteState {
            open: true,
            pending_command: Some("geometry.add_cuboid".to_owned()),
            parameter_json: "{}".to_owned(),
            parameter_cursor: 1,
            parameter_error: Some("Invalid dimensions".to_owned()),
            ..default()
        })
        .insert_resource(CommandCatalog(MeshcraftApp::command_catalog()))
        .add_systems(Update, sync_command_palette_parameter_ui);
        let panel = app
            .world_mut()
            .spawn((
                CommandPaletteParameterPanel,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        let hint = app
            .world_mut()
            .spawn((CommandPaletteParameterHint, Text::new(String::new())))
            .id();
        let input = app
            .world_mut()
            .spawn((CommandPaletteParameterInput, Text::new(String::new())))
            .id();
        let error = app
            .world_mut()
            .spawn((CommandPaletteParameterError, Text::new(String::new())))
            .id();

        app.update();

        assert_eq!(
            Display::Flex,
            app.world().get::<Node>(panel).unwrap().display
        );
        assert!(
            app.world()
                .get::<Text>(hint)
                .unwrap()
                .0
                .contains("dimensions: array")
        );
        assert_eq!("> {|}", app.world().get::<Text>(input).unwrap().0);
        assert_eq!(
            "Invalid dimensions",
            app.world().get::<Text>(error).unwrap().0
        );
    }

    #[test]
    fn toolbar_and_palette_activation_dispatch_shared_commands() {
        let mut app = App::new();
        app.insert_resource(super::CommandPaletteState::default())
            .insert_resource(super::CommandCatalog(MeshcraftApp::command_catalog()))
            .insert_non_send(MeshcraftApp::new_native_session(
                Document::new(),
                "Ready".to_owned(),
                None,
            ))
            .add_observer(dispatch_toolbar_command)
            .add_observer(dispatch_palette_command);
        let toolbar = app
            .world_mut()
            .spawn(super::DispatchCommand("geometry.add_cuboid".to_owned()))
            .id();
        let palette = app
            .world_mut()
            .spawn(super::CommandPaletteResult {
                slot: 0,
                command_id: Some("tool.line".to_owned()),
                enabled: true,
                checked: None,
                disabled_reason: None,
            })
            .id();

        app.world_mut().trigger(Activate { entity: toolbar });
        assert!(app.world().non_send::<MeshcraftApp>().document_revision() > 0);
        app.world_mut().trigger(Activate { entity: palette });

        let session = app.world().non_send::<MeshcraftApp>();
        assert_eq!("Line", session.active_tool_label());
    }

    #[test]
    fn activating_a_parameterized_palette_command_opens_its_editor() {
        let mut app = App::new();
        app.insert_resource(super::CommandPaletteState {
            open: true,
            ..default()
        })
        .insert_resource(super::CommandCatalog(MeshcraftApp::command_catalog()))
        .insert_non_send(MeshcraftApp::new_native_session(
            Document::new(),
            "Ready".to_owned(),
            None,
        ))
        .add_observer(dispatch_palette_command);
        let palette = app
            .world_mut()
            .spawn(super::CommandPaletteResult {
                slot: 0,
                command_id: Some("geometry.add_cuboid".to_owned()),
                enabled: true,
                checked: None,
                disabled_reason: None,
            })
            .id();

        app.world_mut().trigger(Activate { entity: palette });

        let state = app.world().resource::<super::CommandPaletteState>();
        assert!(state.open);
        assert_eq!(
            Some("geometry.add_cuboid"),
            state.pending_command.as_deref()
        );
        assert_eq!("{}", state.parameter_json);
        assert_eq!(
            0,
            app.world().non_send::<MeshcraftApp>().document_revision()
        );
    }

    fn attribute_len(mesh: &Mesh, name: MeshVertexAttribute) -> usize {
        match mesh.attribute(name).expect("mesh attribute") {
            VertexAttributeValues::Float32x3(values) => values.len(),
            VertexAttributeValues::Float32x4(values) => values.len(),
            _ => panic!("unexpected vertex attribute format"),
        }
    }

    use bevy::mesh::MeshVertexAttribute;
}
