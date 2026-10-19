//! Native Bevy renderer and UI shell for the desktop migration.
//!
//! The document remains the source of truth. This module builds compact category-wide GPU batches
//! from resolved geometry and refreshes them only after a document generation or revision change.
//! Viewport picking and spatially incremental geometry updates remain migration work.

use crate::MeshcraftApp;
use bevy::{
    asset::RenderAssetUsages,
    image::{Image as BevyImage, TextureAtlas, TextureAtlasLayout},
    input::keyboard::KeyboardInput,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollPixelsPerLine},
    light::{GlobalAmbientLight, NotShadowCaster, ShadowFilteringMethod},
    mesh::{Mesh, PrimitiveTopology},
    pbr::StandardMaterial,
    prelude::*,
    render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat},
    transform::TransformSystems,
    ui_widgets::{Activate, Button, Slider, SliderRange, SliderThumb, SliderValue, TrackClick},
    window::{PrimaryWindow, WindowPlugin, WindowResizeConstraints, WindowResolution},
};
use eframe::egui;
use meshcraft_core::commands::{CommandDescriptor, CommandPayload};
use meshcraft_core::{
    document::{Document, GeometryBundle, Rgba8},
    geometry::{Vec2 as ModelVec2, Vec3 as ModelVec3},
    selection::{EditContext, SelectedEntity, SelectionKind, SelectionMode, SelectionTarget},
};

const GRID_EXTENT: i32 = 40;

#[derive(Resource)]
struct SceneDocument {
    document: Document,
    generation: u64,
    revision: u64,
    status: String,
    active_tool: String,
    selection_revision: u64,
    selected_objects: usize,
    selected_faces: usize,
    selected_edges: usize,
    selection_targets: Vec<SelectionTarget>,
    occurrences: usize,
    triangles: usize,
    segments: usize,
}

#[derive(Resource, Default)]
struct CommandPaletteState {
    open: bool,
    query: String,
    results: Vec<String>,
    selected_index: usize,
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
struct ToolbarCommandLabel(String);

#[derive(Component)]
struct CommandPaletteOverlay;

#[derive(Component)]
struct CommandPaletteQuery;

#[derive(Component)]
struct CommandPaletteResult {
    slot: usize,
    command_id: Option<String>,
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
struct SelectionStatsText;

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
    selected_faces: Option<Entity>,
    selected_edges: Option<Entity>,
}

#[derive(Resource)]
struct RenderedSceneRevision {
    generation: u64,
    revision: u64,
}

#[derive(Resource)]
struct RenderedSelectionRevision(u64);

struct SelectionMeshes {
    faces: Option<Mesh>,
    edges: Option<Mesh>,
}

struct SceneMeshes {
    front_faces: Option<Mesh>,
    back_faces: Option<Mesh>,
    segments: Option<Mesh>,
    grid: Mesh,
    target: Vec3,
    radius: f32,
}

/// Starts the native Bevy application with the requested window and optional imported document.
pub fn run_native(window_size: [f32; 2], maximized: bool, session: MeshcraftApp) {
    let document = session.document_snapshot();
    let generation = session.document_generation();
    let revision = session.document_revision();
    let (occurrences, triangles, segments) = scene_counts(&document);
    let mut app = App::new();
    app.insert_resource(SceneDocument {
        document,
        generation,
        revision,
        status: session.status_text().to_owned(),
        active_tool: session.active_tool_label().to_owned(),
        selection_revision: session.native_selection_revision(),
        selected_objects: 0,
        selected_faces: 0,
        selected_edges: 0,
        selection_targets: session.native_selection_targets(),
        occurrences,
        triangles,
        segments,
    })
    .insert_resource(ClearColor(Color::srgb(0.78, 0.86, 0.94)))
    .insert_resource(CommandCatalog(MeshcraftApp::command_catalog()))
    .insert_resource(ToolbarCatalog(MeshcraftApp::native_toolbar_catalog()))
    .insert_resource(CommandPaletteState::default())
    .insert_resource(NativeSelectionDrag::default())
    .insert_resource(RenderedSelectionRevision(0))
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
            route_native_escape,
            orbit_camera,
            toggle_command_palette,
            handle_command_palette_keyboard,
            refresh_command_palette,
            sync_lighting_sliders,
            update_slider_thumbs,
            apply_lighting_settings,
            sync_scene_document,
            sync_toolbar_button_states,
            update_scene_meshes,
            update_selection_meshes,
            refresh_status,
        )
            .chain(),
    )
    .add_systems(
        PostUpdate,
        (native_viewport_pointer, refresh_selection_window)
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

fn setup_scene(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    lighting: Res<LightingSettings>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scene_meshes = build_scene_meshes(&scene.document);
    let mut scene_entities = SceneMeshEntities::default();
    ambient_light.brightness = lighting.ambient_brightness();
    commands.insert_resource(OrbitCamera {
        target: scene_meshes.target,
        distance: scene_meshes.radius.max(8.0) * 2.2,
        yaw: 0.75,
        pitch: 0.55,
    });
    spawn_mesh(
        &mut commands,
        &mut meshes,
        &mut materials,
        scene_meshes.grid,
        StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        },
        Transform::default(),
        false,
        false,
    );
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

fn build_scene_meshes(document: &Document) -> SceneMeshes {
    let mut front_positions = Vec::new();
    let mut front_normals = Vec::new();
    let mut front_colors = Vec::new();
    let mut back_positions = Vec::new();
    let mut back_normals = Vec::new();
    let mut back_colors = Vec::new();
    let mut line_positions = Vec::new();
    let mut line_colors = Vec::new();
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    if let Ok(occurrences) = document.scene_geometry_refs() {
        for occurrence in occurrences {
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
                &mut bounds_min,
                &mut bounds_max,
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
        grid: build_grid_mesh(GRID_EXTENT),
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
    bounds_min: &mut Vec3,
    bounds_max: &mut Vec3,
) {
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
        for point in points {
            *bounds_min = bounds_min.min(point);
            *bounds_max = bounds_max.max(point);
        }
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

fn build_grid_mesh(extent: i32) -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let minor = [0.52, 0.60, 0.68, 1.0];
    for step in -extent..=extent {
        let value = step as f32;
        let color_x = if step == 0 {
            [0.85, 0.26, 0.24, 1.0]
        } else {
            minor
        };
        let color_z = if step == 0 {
            [0.24, 0.40, 0.86, 1.0]
        } else {
            minor
        };
        push_line(
            &mut positions,
            &mut colors,
            Vec3::new(-extent as f32, 0.002, value),
            Vec3::new(extent as f32, 0.002, value),
            color_z,
        );
        push_line(
            &mut positions,
            &mut colors,
            Vec3::new(value, 0.002, -extent as f32),
            Vec3::new(value, 0.002, extent as f32),
            color_x,
        );
    }
    for step in -extent..=extent {
        let value = step as f32;
        push_line(
            &mut positions,
            &mut colors,
            Vec3::new(0.0, value, 0.002),
            Vec3::new(0.0, value + 1.0, 0.002),
            [0.2, 0.67, 0.3, 1.0],
        );
    }
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

fn setup_ui(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    lighting: Res<LightingSettings>,
    catalog: Res<CommandCatalog>,
    toolbars: Res<ToolbarCatalog>,
    mut images: ResMut<Assets<BevyImage>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let icon_atlas = create_native_icon_atlas(&mut images, &mut atlas_layouts);
    let root = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .id();
    commands.entity(root).with_children(|root| {
        root.spawn((
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
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(62),
                left: px(10),
                width: px(250),
                bottom: px(38),
                flex_direction: FlexDirection::Column,
                row_gap: px(7),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.12, 0.15, 0.92)),
        ))
        .with_children(|toolbar| {
            for (title, registered_commands) in &toolbars.0 {
                toolbar
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            padding: UiRect::axes(px(5), px(3)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(3),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.12, 0.14, 0.17, 0.95)),
                    ))
                    .with_children(|group| {
                        group.spawn((
                            Text::new(title),
                            TextFont::from_font_size(13.0),
                            TextColor(Color::srgb(0.85, 0.90, 0.96)),
                        ));
                        group
                            .spawn(Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: px(3),
                                row_gap: px(3),
                                ..default()
                            })
                            .with_children(|buttons| {
                                for command in registered_commands {
                                    spawn_tool_button(buttons, command, &icon_atlas);
                                }
                            });
                    });
            }
        });
        root.spawn((
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
            inspector_section(
                panel,
                "Selection",
                &format!(
                    "Selected: {} object(s), {} face(s), {} edge(s)\nTriangles  {}\nEdges  {}\nObjects  {}\nWalls / slabs / stairs / frames  0",
                    scene.selected_objects,
                    scene.selected_faces,
                    scene.selected_edges,
                    scene.triangles,
                    scene.segments,
                    scene.occurrences
                ),
            );
            inspector_section(panel, "Object", "No object selected\nName");
            inspector_section(panel, "Objects", &format!("{} object instances in the scene", scene.occurrences));
            inspector_section(panel, "Model Settings", "Unit name  unit\nUnit size  1.0000\nGrid size  1.0000\nCircle segments  24\nSnap radius");
            inspector_section(panel, "UI Settings", "Text size  1x\nToolbar icon size  32 × 32\nPermanent grid  On\nPermanent grid normal  Y up\nGrid extent  20");
            inspector_section(panel, "Help / About", "Meshcraft · Bevy 0.20\nDesktop viewport · GPU rendering\nCommands: Ctrl+Shift+P");
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
    let mut body_text = parent.spawn((
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
    if title == "Selection" {
        body_text.insert(SelectionStatsText);
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
            DispatchCommand(command_id.clone()),
            Node {
                width: px(112),
                height: px(36),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(5),
                padding: UiRect::horizontal(px(4)),
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
                    Node {
                        width: px(28),
                        height: px(28),
                        ..default()
                    },
                ));
            }
            button.spawn((
                Text::new(command.label.clone()),
                TextFont::from_font_size(11.0),
                TextColor::WHITE,
                ToolbarCommandLabel(command_id),
            ));
        })
        .observe(dispatch_toolbar_command);
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
    mut labels: Query<(&ToolbarCommandLabel, &mut TextColor)>,
) {
    if !scene.is_changed() {
        return;
    }
    let mut states = std::collections::BTreeMap::new();
    for (entity, command, button, mut background) in &mut buttons {
        let state = session.native_command_state(&command.0);
        states.insert(
            command.0.clone(),
            (state.enabled, state.checked == Some(true)),
        );
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
    for (label, mut color) in &mut labels {
        let enabled = states.get(&label.0).is_some_and(|(enabled, _)| *enabled);
        color.0 = if enabled {
            Color::WHITE
        } else {
            Color::srgb(0.48, 0.49, 0.51)
        };
    }
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

fn dispatch_palette_command(
    event: On<Activate>,
    commands: Query<&CommandPaletteResult>,
    mut session: NonSendMut<MeshcraftApp>,
    mut palette_state: ResMut<CommandPaletteState>,
    mut palettes: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    if let Ok(Some(command_id)) = commands.get(event.entity).map(|item| &item.command_id) {
        session.dispatch_command(command_id, &CommandPayload::default());
        palette_state.open = false;
        for mut palette in &mut palettes {
            palette.display = Display::None;
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
    for mut palette in &mut palettes {
        palette.display = Display::Flex;
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
    mut palette: ResMut<CommandPaletteState>,
    mut session: NonSendMut<MeshcraftApp>,
    mut overlays: Query<&mut Node, With<CommandPaletteOverlay>>,
) {
    let control = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    let alt = keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight);
    for event in keyboard_events.read() {
        if !palette.open || event.state != bevy::input::ButtonState::Pressed || control || alt {
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
                    session.dispatch_command(&command_id, &CommandPayload::default());
                    palette.open = false;
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

fn route_native_escape(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut palette: ResMut<CommandPaletteState>,
    mut drag: ResMut<NativeSelectionDrag>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    drag.start = None;
    if palette.open {
        palette.open = false;
    } else {
        session.handle_native_escape();
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

fn sync_scene_document(mut scene: ResMut<SceneDocument>, session: NonSend<MeshcraftApp>) {
    let generation = session.document_generation();
    let revision = session.document_revision();
    if scene.generation != generation || scene.revision != revision {
        scene.document = session.document_snapshot();
        scene.generation = generation;
        scene.revision = revision;
        (scene.occurrences, scene.triangles, scene.segments) = scene_counts(&scene.document);
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
        scene.selected_faces = summary.faces;
        scene.selected_edges = summary.edges;
        scene.selection_targets = session.native_selection_targets();
    }
}

fn update_scene_meshes(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    mut rendered_revision: ResMut<RenderedSceneRevision>,
    mut entities: ResMut<SceneMeshEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lighting: Res<LightingSettings>,
) {
    if rendered_revision.generation == scene.generation
        && rendered_revision.revision == scene.revision
    {
        return;
    }
    let scene_meshes = build_scene_meshes(&scene.document);
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
    rendered_revision.generation = scene.generation;
    rendered_revision.revision = scene.revision;
}

fn update_selection_meshes(
    mut commands: Commands,
    scene: Res<SceneDocument>,
    mut rendered_revision: ResMut<RenderedSelectionRevision>,
    mut entities: ResMut<SceneMeshEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if rendered_revision.0 == scene.selection_revision {
        return;
    }
    let selected = build_selection_meshes(&scene.document, &scene.selection_targets);
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
    rendered_revision.0 = scene.selection_revision;
}

fn build_selection_meshes(document: &Document, selection: &[SelectionTarget]) -> SelectionMeshes {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut line_positions = Vec::new();
    let mut line_colors = Vec::new();
    let highlight = [1.0, 0.34, 0.015, 0.48];
    let edge_highlight = [1.0, 0.24, 0.015, 1.0];

    for target in selection {
        let (Some(resolved), Some(geometry)) = (
            document.resolve_occurrence(&target.occurrence),
            document.effective_geometry(&target.occurrence),
        ) else {
            continue;
        };
        let include_all = matches!(target.entity, SelectedEntity::Occurrence);
        if include_all || matches!(target.entity, SelectedEntity::Triangle(_)) {
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
        if include_all || matches!(target.entity, SelectedEntity::Segment(_)) {
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

fn refresh_status(
    scene: Res<SceneDocument>,
    mut labels: Query<&mut Text, (With<StatusText>, Without<SelectionStatsText>)>,
    mut selection_stats: Query<&mut Text, (With<SelectionStatsText>, Without<StatusText>)>,
) {
    if !scene.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.0 = format!("{}  |  {}", scene.status, scene.active_tool);
    }
    for mut stats in &mut selection_stats {
        stats.0 = format!(
            "Selected: {} object(s), {} face(s), {} edge(s)\nTriangles  {}\nEdges  {}\nObjects  {}\nWalls / slabs / stairs / frames  0",
            scene.selected_objects,
            scene.selected_faces,
            scene.selected_edges,
            scene.triangles,
            scene.segments,
            scene.occurrences
        );
    }
}

// Bevy system parameters are injected independently by the ECS scheduler.
#[allow(clippy::too_many_arguments)]
fn native_viewport_pointer(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    palette: Res<CommandPaletteState>,
    orbit: Res<OrbitCamera>,
    mut drag: ResMut<NativeSelectionDrag>,
    mut session: NonSendMut<MeshcraftApp>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    if palette.open {
        drag.start = None;
        return;
    }
    let cursor = window.cursor_position();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(position) =
            cursor.filter(|position| native_viewport_contains(window, *position))
    {
        if session.active_tool_label() == "Select" {
            drag.start = Some(position);
            drag.current = position;
        } else if let Ok(ray) = camera.viewport_to_world(camera_transform, position) {
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
            let target = session.pick_native_target(origin, direction, radius);
            let snap = session.native_snap_point_on_ray(origin, direction, radius);
            let (point, snap) = snap
                .map_or_else(
                    || {
                        native_grid_pick(
                            ray,
                            session.document_snapshot().model_settings().grid_size,
                        )
                    },
                    |snap| Some((snap.point, Some(snap))),
                )
                .unzip();
            if let Some(point) = point {
                session.handle_native_world_pick(point, snap.flatten(), target);
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
        if !native_context_contains(&document, context, &occurrence) {
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

fn orbit_camera(
    mut camera: Single<&mut Transform, With<Camera3d>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    scroll_conversion: Res<MouseScrollPixelsPerLine>,
    mut orbit: ResMut<OrbitCamera>,
) {
    if buttons.pressed(MouseButton::Right) {
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
        BevyImage, Button, DispatchCommand, LightingChannel, LightingSettings, NativeIconAtlas,
        SceneDocument, ShadowFilter, TextureAtlasLayout, ToolbarCommandLabel, back_face_color,
        build_scene_meshes, create_native_icon_atlas, dispatch_palette_command,
        dispatch_toolbar_command, line_mesh, native_segment_intersects_rect,
        native_triangle_intersects_rect, scene_counts, search_command_ids,
        sync_toolbar_button_states, triangle_mesh,
    };
    use crate::MeshcraftApp;
    use bevy::{
        asset::Assets,
        mesh::{Mesh, VertexAttributeValues},
        prelude::{App, BackgroundColor, Color, TextColor, Update, Vec2, Vec3},
        ui_widgets::Activate,
    };
    use meshcraft_core::commands::CommandPayload;
    use meshcraft_core::{document::Document, geometry::Vec3 as ModelVec3};

    #[test]
    fn scene_meshes_contain_front_back_faces_edges_and_grid() {
        let mut document = Document::new();
        document
            .add_cuboid("sample", ModelVec3::new(2.0, 3.0, 4.0), ModelVec3::ZERO)
            .expect("valid cuboid");

        let built = build_scene_meshes(&document);
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
        assert!(attribute_len(&built.grid, Mesh::ATTRIBUTE_POSITION) > 300);
        assert_eq!(built.target, Vec3::new(1.0, 1.5, 2.0));
        assert_eq!(scene_counts(&document), (1, 12, 12));
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
        let meshes = build_scene_meshes(&rendered_document);
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
            selected_faces: 0,
            selected_edges: 0,
            selection_targets: Vec::new(),
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
        let push_pull_label = app
            .world_mut()
            .spawn((
                ToolbarCommandLabel("tool.builtin.push_pull".to_owned()),
                TextColor(Color::WHITE),
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
            Color::srgb(0.48, 0.49, 0.51),
            app.world().get::<TextColor>(push_pull_label).unwrap().0
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
            })
            .add_systems(Update, super::refresh_command_palette);

        app.update();

        let state = app.world().resource::<super::CommandPaletteState>();
        assert!(state.results.contains(&"tool.cuboid".to_owned()));
        assert!(state.results.contains(&"geometry.add_cuboid".to_owned()));
    }

    #[test]
    fn toolbar_and_palette_activation_dispatch_shared_commands() {
        let mut app = App::new();
        app.insert_resource(super::CommandPaletteState::default())
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
            })
            .id();

        app.world_mut().trigger(Activate { entity: toolbar });
        assert!(app.world().non_send::<MeshcraftApp>().document_revision() > 0);
        app.world_mut().trigger(Activate { entity: palette });

        let session = app.world().non_send::<MeshcraftApp>();
        assert_eq!("Line", session.active_tool_label());
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
