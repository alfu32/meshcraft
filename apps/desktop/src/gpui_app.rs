//! GPUI desktop shell with a raylib RenderTexture presented as a normal GPUI image.
//!
//! The model/session stays on the GPUI thread. The raylib worker receives immutable scene
//! snapshots and owns its OpenGL context, render target, camera and frame readback.

use std::{
    collections::BTreeMap,
    slice,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "gpui_annotations.rs"]
mod annotations;
#[path = "gpui_architecture.rs"]
mod architecture;
#[path = "gpui_mcp.rs"]
mod automation;
#[path = "gpui_files.rs"]
mod files;
#[path = "gpui_helpers.rs"]
mod helpers;
#[path = "gpui_hotspots.rs"]
mod hotspots;
#[path = "gpui_layout.rs"]
mod layout;
#[path = "gpui_lighting.rs"]
mod lighting;
#[path = "gpui_materials.rs"]
mod materials;
#[path = "gpui_postprocess.rs"]
mod postprocess;
#[path = "gpui_scripts.rs"]
mod scripts;
#[path = "gpui_settings.rs"]
mod settings;

use crate::MeshcraftApp;
use crate::viewport::{
    ConstructionGuide, ConstructionGuideKind, GridNormalAxis, SnapKind, SnapResult, ToolFeedback,
    ToolPreview,
};
use gpui::{
    App, Application, Context, Entity, ImageSource, Render, RenderImage, SharedString, Styled,
    Window, WindowBounds, WindowOptions, canvas, div, img, prelude::*, px, rgb, size,
};
use gpui_component::{
    Disableable, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use image::{Frame, ImageBuffer, Rgba};
use meshcraft_core::{
    commands::{CommandCompletion, CommandDescriptor, CommandPayload, CommandValueType},
    document::{Document, ModelSettings, Rgba8, VertexId},
    geometry::{Vec2 as ModelVec2, Vec3},
    selection::{EditContext, SelectedEntity, SelectionMode, SelectionTarget},
};
use raylib::{
    consts::PixelFormat,
    prelude::{
        Camera3D, Color, RaylibDraw, RaylibDraw3D, RaylibMode3DExt, RaylibRenderTexture2D,
        RaylibTexture2D, RaylibTextureModeExt, RenderTexture2D, TraceLogLevel,
        Vector3 as RayVector3,
    },
};
use serde_json::{Value, json};

const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
use crate::app::DROPDOWN_MENU_NAMES as MENU_NAMES;
const RIBBON_TABS: &[&str] = &[
    "Home",
    "Construction",
    "Modify",
    "Arrays",
    "Architecture",
    "Inspect",
    "View",
    "Settings",
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ViewportLayout {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OrbitCamera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    field_of_view: f32,
    orthographic: bool,
    up: Vec3,
    eye: Option<Vec3>,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 0.0, 0.0),
            yaw: 45.0_f32.to_radians(),
            pitch: 28.0_f32.to_radians(),
            distance: 15.0,
            field_of_view: 45.0,
            orthographic: false,
            up: Vec3::Y,
            eye: None,
        }
    }
}

impl OrbitCamera {
    fn orbit_by_drag(&mut self, dx: f32, dy: f32) {
        self.up = Vec3::Y;
        self.yaw -= dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-85.0_f32.to_radians(), 85.0_f32.to_radians());
    }

    fn set_perspective(&mut self) {
        let offset = self.position() - self.target;
        self.yaw = offset.x.atan2(offset.z) as f32;
        self.pitch = (offset.y / offset.length().max(1e-12))
            .clamp(-1., 1.)
            .asin() as f32;
        self.pitch = self.pitch.clamp(-85_f32.to_radians(), 85_f32.to_radians());
        self.eye = None;
        self.up = Vec3::Y;
        self.orthographic = false;
    }

    fn position(self) -> Vec3 {
        if let Some(eye) = self.eye {
            return eye;
        }
        let horizontal = self.distance * self.pitch.cos();
        self.target
            + Vec3::new(
                f64::from(horizontal * self.yaw.sin()),
                self.distance as f64 * self.pitch.sin() as f64,
                f64::from(horizontal * self.yaw.cos()),
            )
    }

    fn ray(self, x: f32, y: f32, width: f32, height: f32) -> Option<(Vec3, Vec3)> {
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let position = self.position();
        let forward = (self.target - position).try_normalized(1.0e-12)?;
        let right = forward.cross(self.up).try_normalized(1.0e-12)?;
        let up = right.cross(forward).try_normalized(1.0e-12)?;
        let scale = (self.field_of_view.to_radians() * 0.5).tan() as f64;
        let aspect = f64::from(width / height);
        let ndc_x = (2.0 * f64::from(x) / f64::from(width) - 1.0) * aspect * scale;
        let ndc_y = (1.0 - 2.0 * f64::from(y) / f64::from(height)) * scale;
        if self.orthographic {
            return Some((
                position + (right * ndc_x + up * ndc_y) * f64::from(self.distance),
                forward,
            ));
        }
        let direction = (forward + right * ndc_x + up * ndc_y).try_normalized(1.0e-12)?;
        Some((position, direction))
    }

    fn fit(&mut self, document: &Document) {
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return;
        };
        let mut minimum = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut maximum = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut found = false;
        for occurrence in occurrences {
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            for vertex in geometry.vertices.values() {
                let point = resolved.world_transform.transform_point(vertex.position);
                minimum.x = minimum.x.min(point.x);
                minimum.y = minimum.y.min(point.y);
                minimum.z = minimum.z.min(point.z);
                maximum.x = maximum.x.max(point.x);
                maximum.y = maximum.y.max(point.y);
                maximum.z = maximum.z.max(point.z);
                found = true;
            }
        }
        if found {
            self.target = (minimum + maximum) * 0.5;
            self.distance = (maximum - minimum).length().max(3.0) as f32 * 1.6;
        }
    }
}

#[derive(Clone)]
struct RenderScene {
    document: Arc<Document>,
    annotations: Arc<Vec<meshcraft_core::document::annotations::Annotation>>,
    grid_visible: bool,
    grid_normal_axis: String,
    grid_extent: i32,
    selection: Vec<SelectionTarget>,
    display_policy: meshcraft_core::selection::SelectionDisplayPolicy,
    edit_context: EditContext,
    guides: Vec<ConstructionGuide>,
    guides_visible: bool,
    cursor: Option<SnapResult>,
    cursor_axes: Option<[Vec3; 3]>,
    preview: Option<ToolPreview>,
    selection_rect: Option<([f32; 4], bool)>,
    grid_spacing: f64,
    normal_line_width: f32,
    thick_line_width: f32,
    antialiasing: postprocess::Antialiasing,
    lighting: lighting::Lighting,
}

impl Default for RenderScene {
    fn default() -> Self {
        Self {
            document: Arc::new(Document::new()),
            annotations: Arc::default(),
            grid_visible: true,
            grid_normal_axis: "y".to_owned(),
            grid_extent: 32,
            selection: Vec::new(),
            display_policy: meshcraft_core::selection::SelectionDisplayPolicy::default(),
            edit_context: EditContext::Document,
            guides: Vec::new(),
            guides_visible: true,
            cursor: None,
            cursor_axes: None,
            preview: None,
            selection_rect: None,
            grid_spacing: 1.0,
            normal_line_width: 1.0,
            thick_line_width: 3.0,
            antialiasing: postprocess::Antialiasing::default(),
            lighting: lighting::Lighting::default(),
        }
    }
}

#[derive(Clone)]
struct ViewFrame {
    image: Arc<RenderImage>,
    sequence: u64,
    frame_ms: f32,
    request_revision: u64,
}

#[derive(Clone)]
struct ViewportBridge {
    layout: Arc<Mutex<ViewportLayout>>,
    camera: Arc<Mutex<OrbitCamera>>,
    scene: Arc<RwLock<Arc<RenderScene>>>,
    frame: Arc<Mutex<Option<ViewFrame>>>,
    redraw: Arc<(Mutex<u64>, Condvar)>,
    preview: Arc<Mutex<Option<ViewportBridge>>>,
}

impl ViewportBridge {
    fn new() -> Self {
        Self {
            layout: Arc::new(Mutex::new(ViewportLayout::default())),
            camera: Arc::new(Mutex::new(OrbitCamera::default())),
            scene: Arc::new(RwLock::new(Arc::new(RenderScene::default()))),
            frame: Arc::new(Mutex::new(None)),
            redraw: Arc::new((Mutex::new(1), Condvar::new())),
            preview: Arc::new(Mutex::new(None)),
        }
    }

    fn publish_layout(&self, layout: ViewportLayout) {
        if let Ok(mut current) = self.layout.lock() {
            if *current == layout {
                return;
            }
            *current = layout;
        }
        self.request_redraw();
    }

    fn publish_camera(&self, camera: OrbitCamera) {
        if let Ok(mut current) = self.camera.lock() {
            if *current == camera {
                return;
            }
            *current = camera;
        }
        self.request_redraw();
    }

    fn publish_scene(&self, scene: RenderScene) {
        if let Ok(mut current) = self.scene.write() {
            *current = Arc::new(scene);
        }
        self.request_redraw();
    }

    fn request_redraw(&self) {
        let (revision, changed) = &*self.redraw;
        if let Ok(mut revision) = revision.lock() {
            *revision = revision.wrapping_add(1);
            changed.notify_one();
        }
    }

    fn latest_frame(&self) -> Option<ViewFrame> {
        self.frame.lock().ok().and_then(|frame| frame.clone())
    }
}

#[derive(Clone)]
struct ShutdownSignal {
    stop: Arc<AtomicBool>,
    finished: Arc<(Mutex<bool>, Condvar)>,
    redraw: Arc<(Mutex<u64>, Condvar)>,
}

impl ShutdownSignal {
    fn stop_and_wait(&self) {
        self.stop.store(true, Ordering::Release);
        self.redraw.1.notify_all();
        let (finished, changed) = &*self.finished;
        let Ok(finished) = finished.lock() else {
            return;
        };
        let _finished = changed.wait_while(finished, |finished| !*finished);
    }
}

struct RenderThreadGuard(Arc<(Mutex<bool>, Condvar)>);

impl Drop for RenderThreadGuard {
    fn drop(&mut self) {
        let (finished, changed) = &*self.0;
        if let Ok(mut finished) = finished.lock() {
            *finished = true;
            changed.notify_all();
        }
    }
}

struct RaylibWorker {
    bridge: ViewportBridge,
    shutdown: ShutdownSignal,
    thread: Option<JoinHandle<()>>,
}

impl RaylibWorker {
    fn start() -> Self {
        let bridge = ViewportBridge::new();
        let worker_bridge = bridge.clone();
        let shutdown = ShutdownSignal {
            stop: Arc::new(AtomicBool::new(false)),
            finished: Arc::new((Mutex::new(false), Condvar::new())),
            redraw: bridge.redraw.clone(),
        };
        let worker_stop = shutdown.stop.clone();
        let worker_finished = shutdown.finished.clone();
        let thread = thread::Builder::new()
            .name("meshcraft-raylib-offscreen".into())
            .spawn(move || render_loop(worker_bridge, worker_stop, worker_finished))
            .expect("start raylib offscreen renderer");
        Self {
            bridge,
            shutdown,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.shutdown.stop_and_wait();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RaylibWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn render_loop(
    bridge: ViewportBridge,
    stop: Arc<AtomicBool>,
    finished: Arc<(Mutex<bool>, Condvar)>,
) {
    let _finished = RenderThreadGuard(finished);
    let (mut rl, thread) = raylib::init()
        .log_level(TraceLogLevel::LOG_WARNING)
        .size(640, 480)
        .title("Meshcraft offscreen renderer")
        .hidden()
        .build();
    rl.set_target_fps(60);
    let mut size = (640_u32, 480_u32);
    let mut target = match rl.load_render_texture(&thread, size.0, size.1) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("create raylib viewport render target: {error}");
            return;
        }
    };
    let mut resolved = rl
        .load_render_texture(&thread, size.0, size.1)
        .expect("antialias target");
    let mut resolve = postprocess::Resolve::new(&mut rl, &thread);
    target.texture_mut().set_texture_filter(
        &thread,
        raylib::consts::TextureFilter::TEXTURE_FILTER_BILINEAR,
    );
    let mut lighting = lighting::Renderer::new(&mut rl, &thread);
    let mut sequence = 0_u64;
    let mut rendered_revision = 0;

    while !stop.load(Ordering::Acquire) && !rl.window_should_close() {
        let (revision, changed) = &*bridge.redraw;
        let Ok(revision) = revision.lock() else {
            break;
        };
        let next_revision = match changed.wait_while(revision, |revision| {
            *revision == rendered_revision && !stop.load(Ordering::Acquire)
        }) {
            Ok(revision) if !stop.load(Ordering::Acquire) => *revision,
            _ => break,
        };
        rendered_revision = next_revision;

        let active_bridge = bridge
            .preview
            .lock()
            .ok()
            .and_then(|p| p.clone())
            .unwrap_or_else(|| bridge.clone());
        let bridge = &active_bridge;
        let frame_start = Instant::now();
        let layout = bridge.layout.lock().map(|value| *value).unwrap_or_default();
        let wanted = (layout.width.max(1), layout.height.max(1));
        if wanted != size {
            match rl.load_render_texture(&thread, wanted.0, wanted.1) {
                Ok(new_target) => {
                    target = new_target;
                    target.texture_mut().set_texture_filter(
                        &thread,
                        raylib::consts::TextureFilter::TEXTURE_FILTER_BILINEAR,
                    );
                    resolved = rl
                        .load_render_texture(&thread, wanted.0, wanted.1)
                        .expect("resize antialias target");
                    size = wanted;
                }
                Err(error) => eprintln!("resize raylib viewport render target: {error}"),
            }
        }
        let camera = bridge.camera.lock().map(|value| *value).unwrap_or_default();
        let scene = bridge
            .scene
            .read()
            .map(|value| Arc::clone(&value))
            .unwrap_or_else(|_| Arc::new(RenderScene::default()));
        lighting.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            sequence,
            &mut lighting,
        );
        let output = if scene.antialiasing == postprocess::Antialiasing::Off {
            &target
        } else {
            resolve.draw(&mut rl, &thread, &target, &mut resolved, scene.antialiasing);
            &resolved
        };
        let Some(image) = copy_target_to_gpui_image(output) else {
            eprintln!("could not copy raylib viewport target into a GPUI image");
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        sequence = sequence.saturating_add(1);
        if let Ok(mut current) = bridge.frame.lock() {
            *current = Some(ViewFrame {
                image: Arc::new(RenderImage::new(vec![Frame::new(image)])),
                sequence,
                frame_ms: frame_start.elapsed().as_secs_f32() * 1_000.0,
                request_revision: next_revision,
            });
        }
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_INTERVAL {
            thread::sleep(FRAME_INTERVAL - elapsed);
        }
    }
}

fn draw_offscreen_scene(
    rl: &mut raylib::RaylibHandle,
    thread: &raylib::RaylibThread,
    target: &mut RenderTexture2D,
    orbit: OrbitCamera,
    scene_data: &RenderScene,
    _sequence: u64,
    lighting: &mut lighting::Renderer,
) {
    let position = orbit.position();
    let target_height = target.height() as f32;
    let target_width = target.width() as f32;
    let camera = if orbit.orthographic {
        Camera3D::orthographic(
            ray_vec3(position),
            ray_vec3(orbit.target),
            ray_vec3(orbit.up),
            orbit.distance * 2. * (orbit.field_of_view.to_radians() * 0.5).tan(),
        )
    } else {
        Camera3D::perspective(
            ray_vec3(position),
            ray_vec3(orbit.target),
            ray_vec3(orbit.up),
            orbit.field_of_view,
        )
    };
    let mut drawing = rl.begin_texture_mode(thread, target);
    drawing.clear_background(Color::new(224, 236, 247, 255));
    {
        let mut view = drawing.begin_mode3D(camera);
        lighting.draw(&mut view, scene_data, orbit, target_height);
        annotations::draw(
            &mut view,
            scene_data,
            orbit,
            target_height,
            &lighting.selection,
        );
        if scene_data.grid_visible {
            draw_grid(
                &mut view,
                scene_data.grid_spacing,
                scene_data.grid_extent,
                &scene_data.grid_normal_axis,
            );
        }
        if scene_data.guides_visible {
            draw_guides(&mut view, &scene_data.guides, scene_data.grid_spacing);
        }
        if let (Some(cursor), Some(axes)) = (scene_data.cursor, scene_data.cursor_axes) {
            helpers::draw_point_field(&mut view, cursor.point, axes, scene_data.grid_spacing);
            draw_cursor(&mut view, cursor, axes);
        }
        if let Some(preview) = scene_data.preview.as_ref() {
            draw_preview(
                &mut view,
                preview,
                scene_data
                    .thick_line_width
                    .max(scene_data.normal_line_width),
                orbit,
                target_height,
            );
        }
    }
    hotspots::draw(
        &mut drawing,
        scene_data,
        orbit,
        (target_width, target_height),
        &lighting.handles,
        &lighting.selection,
    );
    if let Some((rect, crossing)) = scene_data.selection_rect {
        let fill = if crossing {
            Color::new(27, 176, 132, 56)
        } else {
            Color::new(40, 125, 230, 48)
        };
        let outline = if crossing {
            Color::new(25, 170, 125, 255)
        } else {
            Color::new(36, 112, 225, 255)
        };
        let x = rect[0].min(rect[2]) as i32;
        let y = rect[1].min(rect[3]) as i32;
        let width = (rect[2] - rect[0]).abs() as i32;
        let height = (rect[3] - rect[1]).abs() as i32;
        drawing.draw_rectangle(x, y, width, height, fill);
        drawing.draw_rectangle_lines(x, y, width, height, outline);
    }
}

fn draw_grid(scene: &mut impl RaylibDraw3D, spacing: f64, extent: i32, normal_axis: &str) {
    let spacing = if spacing.is_finite() && spacing > 0.0 {
        spacing as f32
    } else {
        1.0
    };
    let extent = extent.clamp(1, 1000);
    let minor = Color::new(142, 160, 176, 255);
    let major = Color::new(97, 119, 139, 255);
    for step in -extent..=extent {
        let coordinate = step as f32 * spacing;
        let color = if step % 5 == 0 { major } else { minor };
        match normal_axis {
            "x" => {
                scene.draw_line3D(
                    RayVector3::new(0.0, -(extent as f32) * spacing, coordinate),
                    RayVector3::new(0.0, extent as f32 * spacing, coordinate),
                    color,
                );
                scene.draw_line3D(
                    RayVector3::new(0.0, coordinate, -(extent as f32) * spacing),
                    RayVector3::new(0.0, coordinate, extent as f32 * spacing),
                    color,
                );
            }
            "z" => {
                scene.draw_line3D(
                    RayVector3::new(-(extent as f32) * spacing, coordinate, 0.0),
                    RayVector3::new(extent as f32 * spacing, coordinate, 0.0),
                    color,
                );
                scene.draw_line3D(
                    RayVector3::new(coordinate, -(extent as f32) * spacing, 0.0),
                    RayVector3::new(coordinate, extent as f32 * spacing, 0.0),
                    color,
                );
            }
            _ => {
                scene.draw_line3D(
                    RayVector3::new(-(extent as f32) * spacing, 0.0, coordinate),
                    RayVector3::new(extent as f32 * spacing, 0.0, coordinate),
                    color,
                );
                scene.draw_line3D(
                    RayVector3::new(coordinate, 0.0, -(extent as f32) * spacing),
                    RayVector3::new(coordinate, 0.0, extent as f32 * spacing),
                    color,
                );
            }
        }
    }
    let axis_origin = match normal_axis {
        "x" => RayVector3::new(0.0, 0.0, 0.0),
        "z" => RayVector3::new(0.0, 0.0, 0.01),
        _ => RayVector3::new(0.0, 0.01, 0.0),
    };
    let (red_end, blue_end) = match normal_axis {
        "x" => (
            RayVector3::new(axis_origin.x, axis_origin.y + 40.0 * spacing, axis_origin.z),
            RayVector3::new(axis_origin.x, axis_origin.y, axis_origin.z + 40.0 * spacing),
        ),
        "z" => (
            RayVector3::new(axis_origin.x + 40.0 * spacing, axis_origin.y, axis_origin.z),
            RayVector3::new(axis_origin.x, axis_origin.y + 40.0 * spacing, axis_origin.z),
        ),
        _ => (
            RayVector3::new(axis_origin.x + 40.0 * spacing, axis_origin.y, axis_origin.z),
            RayVector3::new(axis_origin.x, axis_origin.y, axis_origin.z + 40.0 * spacing),
        ),
    };
    scene.draw_line3D(axis_origin, red_end, Color::new(198, 76, 70, 255));
    scene.draw_line3D(axis_origin, blue_end, Color::new(68, 117, 186, 255));
}

fn ray_color(color: Rgba8) -> Color {
    Color::new(color.red, color.green, color.blue, color.alpha)
}
fn ray_vec3(point: Vec3) -> RayVector3 {
    RayVector3::new(point.x as f32, point.y as f32, point.z as f32)
}

fn draw_guides(scene: &mut impl RaylibDraw3D, guides: &[ConstructionGuide], spacing: f64) {
    helpers::draw_guides(scene, guides, spacing);
}

fn draw_cursor(scene: &mut impl RaylibDraw3D, cursor: SnapResult, axes: [Vec3; 3]) {
    let origin = ray_vec3(cursor.point);
    let colors = [
        Color::new(217, 69, 62, 255),
        Color::new(51, 155, 94, 255),
        Color::new(58, 112, 204, 255),
    ];
    for (axis, color) in axes.into_iter().zip(colors) {
        let tip = cursor.point + axis * 0.65;
        scene.draw_line3D(origin, ray_vec3(tip), color);
    }
    scene.draw_sphere(origin, 0.055, Color::new(55, 66, 78, 255));
}

fn draw_preview(
    scene: &mut impl RaylibDraw3D,
    preview: &ToolPreview,
    width: f32,
    camera: OrbitCamera,
    height: f32,
) {
    let mut edges = Vec::<[Vec3; 2]>::new();
    let mut triangles = Vec::<([Vec3; 3], Rgba8)>::new();
    match preview {
        ToolPreview::Point { position } => {
            scene.draw_sphere(ray_vec3(*position), 0.075, Color::new(235, 128, 38, 255));
        }
        ToolPreview::Cuboid(cuboid) => {
            let p = cuboid.origin;
            let d = cuboid.dimensions;
            let points = [
                p,
                p + Vec3::new(d.x, 0.0, 0.0),
                p + Vec3::new(d.x, d.y, 0.0),
                p + Vec3::new(0.0, d.y, 0.0),
                p + Vec3::new(0.0, 0.0, d.z),
                p + Vec3::new(d.x, 0.0, d.z),
                p + d,
                p + Vec3::new(0.0, d.y, d.z),
            ];
            for [a, b] in [
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
            ] {
                edges.push([points[a], points[b]]);
            }
        }
        ToolPreview::Line { start, end } => edges.push([*start, *end]),
        ToolPreview::PlanarShape {
            outline, closed, ..
        }
        | ToolPreview::Polyline { outline, closed } => add_path_edges(&mut edges, outline, *closed),
        ToolPreview::Transform { edges: source }
        | ToolPreview::LineOffset { edges: source, .. } => {
            for edge in source {
                if edge[0] == edge[1] {
                    scene.draw_sphere(ray_vec3(edge[0]), 0.075, Color::new(235, 128, 38, 255));
                } else {
                    edges.push(*edge);
                }
            }
        }
        ToolPreview::Array {
            edges: source,
            copies,
            displacement,
        } => {
            for copy in 1..=*copies {
                edges.extend(source.iter().map(|[a, b]| {
                    [
                        *a + *displacement * copy as f64,
                        *b + *displacement * copy as f64,
                    ]
                }));
            }
        }
        ToolPreview::ArrayOffsets {
            edges: source,
            offsets,
            ..
        } => {
            for offset in offsets {
                edges.extend(source.iter().map(|[a, b]| [*a + *offset, *b + *offset]));
            }
        }
        ToolPreview::ArrayTransforms {
            edges: source,
            transforms,
            ..
        } => {
            for transform in transforms {
                edges.extend(
                    source.iter().map(|[a, b]| {
                        [transform.transform_point(*a), transform.transform_point(*b)]
                    }),
                );
            }
        }
        ToolPreview::PushPull {
            edges: source,
            triangles: faces,
        }
        | ToolPreview::Primitive {
            edges: source,
            triangles: faces,
        } => {
            edges.extend(source.iter().copied());
            triangles.extend(faces.iter().copied());
        }
        ToolPreview::Paint {
            edges: source,
            triangles: faces,
            color,
        } => {
            edges.extend(source.iter().copied());
            triangles.extend(faces.iter().copied().map(|triangle| (triangle, *color)));
        }
    }
    unsafe {
        raylib::ffi::rlDisableBackfaceCulling();
    }
    for ([a, b, c], face_color) in triangles {
        scene.draw_triangle3D(ray_vec3(a), ray_vec3(b), ray_vec3(c), ray_color(face_color));
    }
    let color = Color::new(235, 118, 38, 255);
    for [a, b] in edges {
        lighting::stroke(scene, [a, b], color, width, camera, height);
    }
    unsafe {
        raylib::ffi::rlDrawRenderBatchActive();
        raylib::ffi::rlEnableBackfaceCulling();
    }
}

fn add_path_edges(edges: &mut Vec<[Vec3; 2]>, points: &[Vec3], closed: bool) {
    edges.extend(points.windows(2).map(|pair| [pair[0], pair[1]]));
    if closed && points.len() > 2 {
        edges.push([points[points.len() - 1], points[0]]);
    }
}

fn copy_target_to_gpui_image(
    target: &impl RaylibRenderTexture2D,
) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let image = target.texture().load_image().ok()?;
    let width = u32::try_from(image.width()).ok()?;
    let height = u32::try_from(image.height()).ok()?;
    if image.format() != PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 {
        return None;
    }
    let bytes = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    // Raylib's texture readback owns a tightly packed RGBA buffer until the image is dropped.
    let rgba = unsafe { slice::from_raw_parts(image.data().cast::<u8>(), bytes) };
    let mut bgra = rgba.to_vec();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mut output = ImageBuffer::from_raw(width, height, bgra)?;
    image::imageops::flip_vertical_in_place(&mut output);
    Some(output)
}

#[derive(Clone, Copy)]
struct PointerPick {
    origin: Vec3,
    direction: Vec3,
    radius: f64,
    point: Vec3,
    snap: SnapResult,
}

#[derive(Clone, Copy)]
struct SelectionGesture {
    start: (f32, f32),
    modifiers: egui::Modifiers,
}

struct RaylibCanvas {
    bridge: ViewportBridge,
    frame_watch_started: bool,
    presented_image: Option<Arc<RenderImage>>,
}

fn replace_presented_image(
    current: &mut Option<Arc<RenderImage>>,
    next: &Arc<RenderImage>,
) -> Option<Arc<RenderImage>> {
    if current.as_ref().is_some_and(|image| image.id == next.id) {
        return None;
    }
    current.replace(Arc::clone(next))
}

impl Render for RaylibCanvas {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.frame_watch_started {
            self.frame_watch_started = true;
            cx.on_release_in(window, |canvas, window, _| {
                if let Some(image) = canvas.presented_image.take() {
                    let _ = window.drop_image(image);
                }
            })
            .detach();
            let bridge = self.bridge.clone();
            let initial_sequence = bridge.latest_frame().map_or(0, |frame| frame.sequence);
            cx.spawn_in(window, async move |canvas, cx| {
                let mut last_sequence = initial_sequence;
                loop {
                    gpui::Timer::after(Duration::from_millis(16)).await;
                    if canvas.upgrade().is_none() {
                        break;
                    }
                    let sequence = bridge.latest_frame().map_or(0, |frame| frame.sequence);
                    if sequence > last_sequence {
                        last_sequence = sequence;
                        if canvas.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    }
                }
            })
            .detach();
        }
        match self.bridge.latest_frame() {
            Some(frame) => {
                if let Some(retired) =
                    replace_presented_image(&mut self.presented_image, &frame.image)
                {
                    // RenderImage's Arc only owns CPU pixels. Atlas entries require explicit eviction.
                    let _ = window.drop_image(retired);
                }
                img(ImageSource::Render(frame.image))
                    .size_full()
                    .into_any_element()
            }
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x334155))
                .child("Starting raylib viewport…")
                .into_any_element(),
        }
    }
}

struct MeshcraftShell {
    annotations: annotations::Ui,
    architecture: architecture::Ui,
    mcp: automation::Ui,
    cached_annotations: Arc<Vec<meshcraft_core::document::annotations::Annotation>>,
    scripts: scripts::ScriptsUi,
    materials: materials::MaterialsUi,
    focus: gpui::FocusHandle,
    settings: settings::Editors,
    settings_scroll: gpui::ScrollHandle,
    open_settings_panel: Option<&'static str>,
    collapsed_settings_panels: std::collections::BTreeSet<&'static str>,
    bridge: ViewportBridge,
    viewport_canvas: Entity<RaylibCanvas>,
    session: MeshcraftApp,
    camera: OrbitCamera,
    layout: ViewportLayout,
    active_ribbon_tab: &'static str,
    open_menu: Option<&'static str>,
    files: files::Ui,
    menu_bounds: Arc<Mutex<BTreeMap<&'static str, gpui::Bounds<gpui::Pixels>>>>,
    lighting: lighting::Lighting,
    cached_document_revision: Option<u64>,
    cached_document_generation: Option<u64>,
    cached_edit_context: Option<EditContext>,
    cached_document: Arc<Document>,
    cached_scene_counts: layout::SceneCounts,
    cursor_local: Option<(f32, f32)>,
    cursor_snap: Option<SnapResult>,
    last_pick: Option<PointerPick>,
    camera_drag: Option<(gpui::MouseButton, f32, f32)>,
    walking: bool,
    walk_keys: [bool; 7],
    walk_tick: Instant,
    hotspot_drag: Option<hotspots::Drag>,
    selection_gesture: Option<SelectionGesture>,
    selection_cursor: Option<(f32, f32)>,
    command_palette_open: bool,
    palette_input: Option<Entity<InputState>>,
    palette_subscription: Option<gpui::Subscription>,
    palette_query: String,
    palette_focus_pending: bool,
    feedback: Option<ToolFeedback>,
    close_requested: bool,
    host_watch_started: bool,
    last_sequence: u64,
}

impl MeshcraftShell {
    fn new(bridge: ViewportBridge, session: MeshcraftApp, cx: &mut Context<Self>) -> Self {
        let viewport_canvas = cx.new(|_| RaylibCanvas {
            bridge: bridge.clone(),
            frame_watch_started: false,
            presented_image: None,
        });
        let mut this = Self {
            annotations: Default::default(),
            architecture: Default::default(),
            mcp: Default::default(),
            cached_annotations: Arc::default(),
            materials: materials::MaterialsUi::default(),
            scripts: scripts::ScriptsUi::default(),
            focus: cx.focus_handle(),
            settings: settings::Editors::default(),
            settings_scroll: gpui::ScrollHandle::new(),
            open_settings_panel: Some("UI Settings"),
            collapsed_settings_panels: Default::default(),
            camera: OrbitCamera::default(),
            layout: ViewportLayout::default(),
            active_ribbon_tab: "Home",
            open_menu: None,
            files: files::Ui::default(),
            menu_bounds: Arc::default(),
            lighting: lighting::Lighting::default(),
            cached_document_revision: None,
            cached_document_generation: None,
            cached_edit_context: None,
            cached_document: Arc::new(session.document_snapshot()),
            cached_scene_counts: layout::SceneCounts::default(),
            cursor_local: None,
            cursor_snap: None,
            last_pick: None,
            camera_drag: None,
            walking: false,
            walk_keys: [false; 7],
            walk_tick: Instant::now(),
            hotspot_drag: None,
            selection_gesture: None,
            selection_cursor: None,
            command_palette_open: false,
            palette_input: None,
            palette_subscription: None,
            palette_query: String::new(),
            palette_focus_pending: false,
            feedback: None,
            close_requested: false,
            host_watch_started: false,
            last_sequence: 0,
            bridge,
            viewport_canvas,
            session,
        };
        this.camera.fit(&this.session.document_snapshot());
        this.bridge.publish_camera(this.camera);
        this.refresh_scene();
        this
    }

    fn dispatch_with_clipboard(&mut self, id: &str, cx: &mut Context<Self>) {
        if matches!(id, "edit.paste" | "edit.paste_in_place") {
            self.sync_clipboard(cx);
        }
        self.dispatch(id);
    }

    fn sync_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.session.native_take_clipboard_write() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        self.session.native_set_clipboard(text);
    }

    fn toggle_command_palette(&mut self) {
        self.command_palette_open = !self.command_palette_open;
        self.open_menu = None;
        self.palette_focus_pending = self.command_palette_open;
    }

    fn prepare_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette_input.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search commands…"));
            self.palette_subscription = Some(cx.subscribe_in(
                &input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::Change => {
                            this.palette_query = input.read(cx).value().to_string()
                        }
                        InputEvent::PressEnter { .. } if this.command_palette_open => {
                            let command =
                                MeshcraftApp::command_catalog().into_iter().find(|command| {
                                    palette_matches(command, &this.palette_query)
                                        && command_enabled(&this.session, command)
                                });
                            if let Some(command) = command {
                                this.command_palette_open = false;
                                window.focus(&this.focus);
                                this.dispatch_with_clipboard(command.id.as_str(), cx);
                            }
                        }
                        _ => {}
                    }
                    cx.notify();
                },
            ));
            self.palette_input = Some(input);
        }
        if self.palette_focus_pending {
            self.palette_focus_pending = false;
            self.palette_query.clear();
            if let Some(input) = self.palette_input.clone() {
                input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
            }
        }
    }

    fn refresh_scene(&mut self) {
        let revision = self.session.native_visual_revision();
        let edit_context = self.session.native_edit_context();
        if self.cached_document_revision != Some(revision)
            || self.cached_document_generation != Some(self.session.document_generation())
            || self.cached_edit_context.as_ref() != Some(&edit_context)
        {
            self.cached_edit_context = Some(edit_context);
            self.cached_document_generation = Some(self.session.document_generation());
            self.cached_document = Arc::new(self.session.native_document_for_view());
            self.cached_scene_counts = layout::SceneCounts::from_document(&self.cached_document);
            self.cached_annotations = Arc::new(self.cached_document.annotations());
            self.cached_document_revision = Some(revision);
        }
        let (preview, feedback, cursor_axes) = match (self.cursor_snap, self.cursor_local) {
            (Some(snap), Some((x, y))) => {
                self.session.set_native_cursor_snap(Some(snap));
                let axes = self.session.native_cursor_axes(snap);
                let (preview, feedback) = self
                    .session
                    .native_tool_feedback_for_snap(snap, egui::Pos2::new(x, y));
                (preview, feedback, axes)
            }
            _ => {
                self.session.set_native_cursor_snap(None);
                (self.session.native_paste_preview(), None, None)
            }
        };
        self.feedback = feedback;
        if let Some(drag) = self.hotspot_drag.as_ref().filter(|drag| drag.moved) {
            let mut preview_document = if drag.authoring {
                self.session.native_document_for_view()
            } else {
                (*self.cached_document).clone()
            };
            let result = if drag.authoring {
                preview_document
                    .resolve_occurrence(&drag.handle.occurrence)
                    .map(|r| r.definition.id)
                    .ok_or_else(|| "object no longer exists".to_owned())
                    .and_then(|definition| {
                        let mut hotspot = drag.handle.hotspot.clone();
                        hotspot.position = drag.target;
                        preview_document
                            .put_hotspot(definition, hotspot)
                            .map(|_| ())
                    })
            } else {
                preview_document.set_hotspot_target(
                    &drag.handle.occurrence,
                    &drag.handle.hotspot.id,
                    drag.target,
                )
            };
            if result.is_ok() {
                self.cached_document = Arc::new(preview_document);
            }
            let delta = drag.target - drag.start_target;
            self.feedback = Some(ToolFeedback {
                screen_position: egui::Pos2::new(drag.start.0, drag.start.1),
                text: format!(
                    "{} · Δ ({:.3}, {:.3}, {:.3}) · {:.3} {}",
                    if drag.authoring {
                        "Hotspot position"
                    } else {
                        drag.handle.hotspot.operation.name()
                    },
                    delta.x,
                    delta.y,
                    delta.z,
                    delta.length(),
                    self.cached_document.model_settings().unit_name
                ),
            });
        }
        self.preview_architecture_drag();
        let preferences = self.session.native_ui_preferences();
        let selection_rect =
            self.selection_gesture
                .zip(self.selection_cursor)
                .map(|(gesture, cursor)| {
                    (
                        [gesture.start.0, gesture.start.1, cursor.0, cursor.1],
                        cursor.0 < gesture.start.0,
                    )
                });
        self.bridge.publish_scene(RenderScene {
            annotations: Arc::clone(&self.cached_annotations),
            document: Arc::clone(&self.cached_document),
            grid_visible: preferences.show_permanent_grid,
            grid_normal_axis: match preferences.grid_normal_axis {
                GridNormalAxis::X => "x",
                GridNormalAxis::Y => "y",
                GridNormalAxis::Z => "z",
            }
            .to_owned(),
            grid_extent: preferences.grid_extent as i32,
            selection: self.session.native_selection_targets(),
            display_policy: self.session.native_selection_display_policy(),
            edit_context: self.session.native_edit_context(),
            guides: self.session.native_construction_guides().to_vec(),
            guides_visible: self.session.native_guides_visible(),
            cursor: self.cursor_snap,
            cursor_axes,
            preview,
            selection_rect,
            grid_spacing: self.cached_document.model_settings().grid_size,
            normal_line_width: preferences.normal_grid_width,
            thick_line_width: preferences.thick_grid_width,
            lighting: self.lighting,
            antialiasing: postprocess::Antialiasing::from_level(preferences.antialiasing_quality),
        });
    }

    fn update_pointer(&mut self, x: f32, y: f32) {
        self.cursor_local = Some((x, y));
        let Some((origin, direction)) =
            self.camera
                .ray(x, y, self.layout.width as f32, self.layout.height as f32)
        else {
            self.cursor_snap = None;
            self.last_pick = None;
            self.refresh_scene();
            return;
        };
        let settings = self.cached_document.model_settings();
        let world_per_pixel = (self.camera.distance as f64
            * (self.camera.field_of_view.to_radians() * 0.5).tan() as f64
            * 2.0)
            / f64::from(self.layout.height.max(1));
        let radius = (world_per_pixel * settings.snap_radius).max(settings.grid_size * 0.015);
        let geometry_snap = self
            .session
            .native_snap_point_on_ray(origin, direction, radius);
        let geometry_snap =
            self.session
                .native_architecture_plane_snap(origin, direction, geometry_snap);
        let geometry_snap = self.architecture_drag_snap(origin, direction, geometry_snap);
        let preferences = self.session.native_ui_preferences();
        let snap = helpers::snap(
            self.camera,
            (self.layout.width as f32, self.layout.height as f32),
            (x, y),
            origin,
            direction,
            settings.grid_size,
            settings.snap_radius as f32,
            preferences.grid_normal_axis,
            if preferences.show_construction_guides {
                self.session.native_construction_guides()
            } else {
                &[]
            },
            geometry_snap,
        );
        let point = snap.map(|snap| snap.point);
        self.cursor_snap = snap;
        self.last_pick = point.zip(snap).map(|(point, snap)| PointerPick {
            origin,
            direction,
            radius,
            point,
            snap,
        });
        self.refresh_scene();
    }

    fn dispatch(&mut self, id: &str) {
        let Some(command) = MeshcraftApp::command_catalog()
            .into_iter()
            .find(|command| command.id.as_str() == id)
        else {
            return;
        };
        let payload = if id == "ui.update_preferences" {
            self.session.native_ui_preferences_payload()
        } else if id == "material.apply" {
            let mut payload = CommandPayload::default();
            payload.values.insert(
                "id".into(),
                serde_json::Value::String(
                    self.materials
                        .selected
                        .clone()
                        .unwrap_or_else(|| "default".into()),
                ),
            );
            payload
        } else {
            command_default_payload(&command, &self.cached_document)
        };
        self.dispatch_payload(id, &payload);
    }

    fn dispatch_payload(&mut self, id: &str, payload: &CommandPayload) {
        let dispatch = self.session.dispatch_command(id, payload);
        self.apply_command_effects(id, &dispatch);
    }

    fn apply_command_effects(
        &mut self,
        id: &str,
        dispatch: &meshcraft_core::commands::CommandDispatch,
    ) {
        if id == "edit.paste"
            && self.cursor_snap.is_none()
            && dispatch.result.completion == CommandCompletion::Completed
        {
            // With no previous canvas hover, start at the visible camera target rather
            // than an offscreen world origin (e.g. after opening a far-away model).
            self.session.paste_preview_at(Some(self.camera.target));
        }
        if dispatch.accepted {
            match id {
                "app.quit" => self.close_requested = true,
                "view.camera.walkthrough" => {
                    self.walking = true;
                    self.walk_tick = Instant::now();
                    self.camera.orthographic = false;
                    self.camera.up = Vec3::Y;
                    self.session.native_begin_walkthrough(
                        self.camera.position(),
                        (self.camera.target - self.camera.position())
                            .try_normalized(1e-12)
                            .unwrap_or(-Vec3::Z),
                    );
                }
                "view.camera.perspective" | "view.camera.orbit" | "view.orbit" => {
                    self.walking = false;
                    self.walk_keys = [false; 7];
                    self.camera.set_perspective();
                    self.bridge.publish_camera(self.camera);
                }
                id if id == "view.camera.orthographic" || id.starts_with("view.ortho.") => {
                    self.walking = false;
                    self.walk_keys = [false; 7];
                    self.camera.eye = None;
                    self.camera.orthographic = true;
                    let view = id
                        .strip_prefix("view.ortho.")
                        .and_then(crate::viewport::OrthoView::from_id)
                        .unwrap_or(crate::viewport::OrthoView::Top);
                    let (_, up, eye) = view.screen_axes();
                    self.camera.yaw = eye.x.atan2(eye.z) as f32;
                    self.camera.pitch = eye.y.asin() as f32;
                    self.camera.up = up;
                    self.bridge.publish_camera(self.camera);
                }
                "view.camera.set_pose"
                    if dispatch.result.completion == CommandCompletion::Completed =>
                {
                    let value = &dispatch.result.outputs["camera"];
                    let point = |key: &str| -> Option<Vec3> {
                        serde_json::from_value(value[key].clone()).ok()
                    };
                    if let (Some(eye), Some(target)) = (point("position"), point("target")) {
                        let offset = eye - target;
                        self.walking = false;
                        self.camera.eye = None;
                        self.camera.target = target;
                        self.camera.distance = offset.length() as f32;
                        self.camera.yaw = offset.x.atan2(offset.z) as f32;
                        self.camera.pitch =
                            (offset.y / offset.length()).clamp(-1., 1.).asin() as f32;
                        self.camera.up = if self.camera.pitch.abs() > 89_f32.to_radians() {
                            Vec3::Z
                        } else {
                            Vec3::Y
                        };
                        self.camera.field_of_view =
                            value["fieldOfView"].as_f64().unwrap_or(45.) as f32;
                        self.camera.orthographic = false;
                        self.bridge.publish_camera(self.camera);
                    }
                }
                "view.camera.reset" => {
                    self.walking = false;
                    self.walk_keys = [false; 7];
                    self.camera = OrbitCamera::default();
                    self.bridge.publish_camera(self.camera);
                }
                "view.frame_document" => {
                    if self.walking {
                        self.camera.set_perspective();
                    }
                    self.walking = false;
                    self.camera.eye = None;
                    self.camera.fit(&self.session.document_snapshot());
                    self.bridge.publish_camera(self.camera);
                }
                _ => {}
            }
        }
        self.session.poll_native_host_work();
        self.refresh_scene();
    }

    fn update_model_number(&mut self, field: &str, delta: f64) {
        let mut settings = self.cached_document.model_settings().clone();
        match field {
            "unit_size" => settings.unit_size = (settings.unit_size + delta).max(0.001),
            "grid_size" => settings.grid_size = (settings.grid_size + delta).max(0.001),
            "circle_segments" => {
                settings.circle_segments = (f64::from(settings.circle_segments) + delta)
                    .round()
                    .clamp(3.0, 256.0) as u32;
            }
            "snap_radius" => {
                settings.snap_radius = (settings.snap_radius + delta).clamp(0.0, 128.0)
            }
            "arc_max_length" => {
                settings.arc_max_length = (settings.arc_max_length + delta).max(0.001)
            }
            "double_line_size" => {
                settings.double_line_size = (settings.double_line_size + delta).max(0.001)
            }
            "double_line_offset" => settings.double_line_offset += delta,
            _ => return,
        }
        self.dispatch_payload("model.update_settings", &model_settings_payload(&settings));
    }

    fn update_ui_preference(&mut self, field: &str, value: Value) {
        let mut payload = self.session.native_ui_preferences_payload();
        payload.values.insert(field.to_owned(), value);
        self.dispatch_payload("ui.update_preferences", &payload);
    }

    fn toggle_ui_preference(&mut self, field: &str) {
        let preferences = self.session.native_ui_preferences();
        let current = match field {
            "menu_bar_visible" => preferences.menu_bar_visible,
            "ribbon_visible" => preferences.ribbon_visible,
            "left_panel_visible" => preferences.left_panel_visible,
            "show_permanent_grid" => preferences.show_permanent_grid,
            "show_construction_guides" => preferences.show_construction_guides,
            "auto_collapse_toolbars" => preferences.auto_collapse_toolbars,
            "accordion_settings_panels" => preferences.accordion_settings_panels,
            "toolbar_flow_top_down" => preferences.toolbar_flow_top_down,
            _ => return,
        };
        self.update_ui_preference(field, Value::Bool(!current));
    }

    fn adjust_lighting(&mut self, field: &str, delta: f32) {
        let value = match field {
            "ambient" => &mut self.lighting.ambient,
            "directional" => &mut self.lighting.directional,
            "shadow_strength" => &mut self.lighting.shadow_strength,
            "specular" => &mut self.lighting.specular,
            "depth_bias" => &mut self.lighting.depth_bias,
            "normal_bias" => &mut self.lighting.normal_bias,
            "azimuth" => &mut self.lighting.azimuth,
            "elevation" => &mut self.lighting.elevation,
            _ => return,
        };
        *value = (*value + delta).clamp(
            if field == "azimuth" {
                -180.0
            } else if field == "elevation" {
                5.0
            } else {
                0.0
            },
            match field {
                "azimuth" => 180.0,
                "elevation" => 85.0,
                "depth_bias" => 0.01,
                "normal_bias" => 0.1,
                _ => 1.0,
            },
        );
        self.refresh_scene();
    }

    fn adjust_ui_preference(&mut self, field: &str, delta: i32) {
        let preferences = self.session.native_ui_preferences();
        match field {
            "grid_extent" => {
                let value = (preferences.grid_extent as i32 + delta).clamp(1, 1000) as u32;
                self.update_ui_preference(field, json!(value));
            }
            "toolbar_icon_size" => {
                let sizes = [24_u32, 32, 40, 48];
                let current = sizes
                    .iter()
                    .position(|size| *size == preferences.toolbar_icon_size)
                    .unwrap_or(1) as i32;
                let index = (current + delta).clamp(0, sizes.len() as i32 - 1) as usize;
                self.update_ui_preference(field, json!(sizes[index]));
            }
            "ui_text_scale" => {
                let value = (preferences.ui_text_scale + delta as f32 * 0.1).clamp(0.5, 2.0);
                self.update_ui_preference(field, json!(value));
            }
            _ => {}
        }
    }

    fn viewport_point(&self, position: gpui::Point<gpui::Pixels>, window: &Window) -> (f32, f32) {
        let scale = window.scale_factor();
        (
            f32::from(position.x) * scale - self.layout.x as f32,
            f32::from(position.y) * scale - self.layout.y as f32,
        )
    }

    fn toggle_menu(&mut self, menu: &'static str) {
        self.open_menu = if self.open_menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
    }

    pub(super) fn viewport_input_blocked(&self) -> bool {
        self.command_palette_open
            || self.open_menu.is_some()
            || self.session.native_document_replacement_prompt().is_some()
    }

    fn pointer_down(
        &mut self,
        button: gpui::MouseButton,
        x: f32,
        y: f32,
        modifiers: gpui::Modifiers,
    ) {
        if self.viewport_input_blocked() {
            return;
        }
        if matches!(button, gpui::MouseButton::Right | gpui::MouseButton::Middle)
            && self.cursor_in_viewport(x, y)
        {
            self.camera_drag = Some((button, x, y));
        } else if button == gpui::MouseButton::Left && self.cursor_in_viewport(x, y) {
            self.update_pointer(x, y);
            if self.begin_architecture_drag(x, y, modifiers)
                || self.begin_hotspot_drag(x, y, modifiers)
            {
                return;
            }
            self.selection_gesture = Some(SelectionGesture {
                start: (x, y),
                modifiers: egui::Modifiers {
                    alt: modifiers.alt,
                    ctrl: modifiers.control,
                    shift: modifiers.shift,
                    mac_cmd: modifiers.platform,
                    command: modifiers.secondary(),
                },
            });
            self.selection_cursor = Some((x, y));
            self.refresh_scene();
        }
    }

    fn pointer_move(
        &mut self,
        x: f32,
        y: f32,
        modifiers: gpui::Modifiers,
        pressed: Option<gpui::MouseButton>,
    ) {
        if self.viewport_input_blocked() {
            return;
        }
        if let Some((button, last_x, last_y)) = self.camera_drag {
            if pressed == Some(button) {
                let dx = x - last_x;
                let dy = y - last_y;
                if self.walking {
                    self.session.native_walkthrough_drag(
                        dx,
                        dy,
                        modifiers.shift || button == gpui::MouseButton::Middle,
                    );
                    let (eye, forward) = self.session.native_step_walkthrough(0., [false; 7]);
                    self.camera.eye = Some(eye);
                    self.camera.target = eye + forward * f64::from(self.camera.distance);
                    self.bridge.publish_camera(self.camera);
                } else if modifiers.shift || button == gpui::MouseButton::Middle {
                    self.pan_camera(dx, dy);
                } else {
                    self.camera.orbit_by_drag(dx, dy);
                    self.bridge.publish_camera(self.camera);
                }
                self.camera_drag = Some((button, x, y));
            } else {
                self.camera_drag = None;
            }
        }
        if let Some(gesture) = self.selection_gesture.as_mut() {
            self.selection_cursor = Some((x, y));
            let dx = x - gesture.start.0;
            let dy = y - gesture.start.1;
            if dx.abs() + dy.abs() > 3.0 {
                // Keep the original corner. The release handler determines window/crossing
                // direction from the unnormalized endpoints.
            }
        }
        self.update_pointer(x, y);
        self.update_hotspot_drag(x, y);
        self.update_architecture_drag(x, y);
    }

    fn pointer_up(&mut self, button: gpui::MouseButton, x: f32, y: f32) {
        if self.viewport_input_blocked() {
            return;
        }
        if matches!(button, gpui::MouseButton::Right | gpui::MouseButton::Middle) {
            if self
                .camera_drag
                .is_some_and(|(active, _, _)| active == button)
            {
                self.camera_drag = None;
            }
            return;
        }
        if button != gpui::MouseButton::Left {
            return;
        }
        if self.finish_architecture_drag() || self.finish_hotspot_drag() {
            return;
        }
        let Some(gesture) = self.selection_gesture.take() else {
            return;
        };
        self.selection_cursor = None;
        let start = gesture.start;
        let dx = x - start.0;
        let dy = y - start.1;
        if !self.session.native_pointer_tool_active() && (dx * dx + dy * dy).sqrt() >= 4.0 {
            let crossing = dx < 0.0;
            let targets = selection_region_in_context(
                &self.cached_document,
                self.camera,
                (self.layout.width as f32, self.layout.height as f32),
                start,
                (x, y),
                crossing,
                &self.session.native_edit_context(),
            );
            let mode = if gesture.modifiers.ctrl || gesture.modifiers.mac_cmd {
                SelectionMode::Remove
            } else if gesture.modifiers.shift {
                SelectionMode::Add
            } else {
                SelectionMode::Replace
            };
            self.session
                .apply_native_selection_region(targets, crossing, mode);
        } else {
            if !self.session.native_pointer_tool_active() {
                let mut target = annotations::hit(self, x, y).or_else(|| {
                    self.last_pick.and_then(|pick| {
                        self.session
                            .pick_native_target(pick.origin, pick.direction, pick.radius)
                    })
                });
                if gesture.modifiers.alt
                    && let Some(target) = target.as_mut()
                    && !target.occurrence.is_scene_root()
                {
                    target.entity = SelectedEntity::Occurrence;
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |value| value.as_millis() as u64);
                self.session.handle_native_selection_click(
                    target,
                    gesture.modifiers,
                    now,
                    ModelVec2::new(f64::from(x), f64::from(y)),
                );
            } else if let Some(pick) = self.last_pick {
                let target =
                    self.session
                        .pick_native_target(pick.origin, pick.direction, pick.radius);
                self.session
                    .handle_native_world_pick(pick.point, Some(pick.snap), target);
            }
        }
        self.session.poll_native_host_work();
        self.refresh_scene();
    }

    fn cursor_in_viewport(&self, x: f32, y: f32) -> bool {
        x >= 0.0 && y >= 0.0 && x < self.layout.width as f32 && y < self.layout.height as f32
    }

    fn pan_camera(&mut self, dx: f32, dy: f32) {
        let position = self.camera.position();
        let forward = (self.camera.target - position)
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::new(0.0, -1.0, 0.0));
        let right = forward
            .cross(self.camera.up)
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::X);
        let up = right
            .cross(forward)
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::Y);
        let scale = f64::from(self.camera.distance * 0.0009375);
        let delta = (right * (-f64::from(dx)) + up * f64::from(dy)) * scale;
        self.camera.target += delta;
        self.bridge.publish_camera(self.camera);
    }

    fn zoom_camera(&mut self, steps: f32) {
        if self.walking {
            self.session.native_walkthrough_scroll(steps);
            let (eye, forward) = self.session.native_step_walkthrough(0., [false; 7]);
            self.camera.eye = Some(eye);
            self.camera.target = eye + forward * f64::from(self.camera.distance);
            self.bridge.publish_camera(self.camera);
            return;
        }
        self.camera.distance = (self.camera.distance * (-steps * 0.12).exp()).clamp(0.25, 1.0e6);
        self.bridge.publish_camera(self.camera);
    }

    fn shortcut(&mut self, event: &gpui::KeyDownEvent) {
        let key = event.keystroke.key.to_ascii_lowercase();
        let modifiers = event.keystroke.modifiers;
        if self.walking
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.platform
            && !self.viewport_input_blocked()
            && let Some(index) = walk_key(&key)
        {
            if !self.walk_keys.iter().any(|key| *key)
                && !self.session.native_walkthrough_animating()
            {
                self.walk_tick = Instant::now();
            }
            self.walk_keys[index] = true;
            return;
        }
        if key == "escape" {
            self.open_menu = None;
            self.command_palette_open = false;
            self.camera_drag = None;
            self.selection_gesture = None;
            self.selection_cursor = None;
            if self.architecture.drag.take().is_some() || self.hotspot_drag.take().is_some() {
                self.cached_document_revision = None;
            } else {
                self.session.handle_native_escape();
            }
            self.cursor_snap = None;
            self.last_pick = None;
            self.refresh_scene();
        } else if !modifiers.control
            && !modifiers.alt
            && !modifiers.platform
            && self.open_menu.is_none()
            && !self.command_palette_open
            && self.session.handle_native_tool_key(&key, modifiers.shift)
        {
            self.refresh_scene();
        } else if !modifiers.control
            && !modifiers.alt
            && !modifiers.platform
            && self.open_menu.is_none()
            && !self.command_palette_open
            && (key == "g" || key == "t")
        {
            self.dispatch(if key == "g" {
                "view.grid_guide"
            } else {
                "view.axis_guide"
            });
        } else if modifiers.control && modifiers.shift && key == "p" {
            self.toggle_command_palette();
        } else if self.command_palette_open || self.open_menu.is_some() {
        } else if key == "delete" {
            self.dispatch("edit.delete_selection");
        } else if (modifiers.control || modifiers.platform) && key == "c" {
            self.dispatch(if modifiers.shift {
                "edit.copy_as_script"
            } else {
                "edit.copy_selection"
            });
        } else if (modifiers.control || modifiers.platform) && key == "x" {
            self.dispatch("edit.cut_selection");
        } else if (modifiers.control || modifiers.platform) && key == "v" {
            self.dispatch(if modifiers.shift {
                "edit.paste_in_place"
            } else {
                "edit.paste"
            });
        } else if modifiers.control && key == "z" {
            self.dispatch("edit.undo");
        } else if modifiers.control && key == "y" {
            self.dispatch("edit.redo");
        }
    }
}

fn walk_key(key: &str) -> Option<usize> {
    match key {
        "w" | "up" => Some(0),
        "s" | "down" => Some(1),
        "a" | "left" => Some(2),
        "d" | "right" => Some(3),
        "e" | "pageup" => Some(4),
        "q" | "pagedown" => Some(5),
        "space" => Some(6),
        _ => None,
    }
}

fn palette_matches(command: &CommandDescriptor, query: &str) -> bool {
    let searchable = format!(
        "{} {} {}",
        command.id.as_str(),
        command_label(command),
        command.presentation.palette_path
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| searchable.contains(&word.to_lowercase()))
}

fn command_icon(command: &CommandDescriptor) -> Option<Arc<RenderImage>> {
    command
        .presentation
        .icon_key
        .as_deref()
        .and_then(crate::assets::gpui_icon)
        .or_else(|| crate::assets::gpui_icon(command.id.as_str()))
}
fn labeled_command(
    label: impl Into<SharedString>,
    icon: Option<Arc<RenderImage>>,
    width: f32,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .w(px(width))
        .min_w(px(0.))
        .text_align(gpui::TextAlign::Left)
        .when_some(icon, |row, icon| {
            row.child(img(ImageSource::Render(icon)).size(px(20.)).flex_none())
        })
        .child(div().flex_1().min_w(px(0.)).truncate().child(label.into()))
        .into_any_element()
}

fn viewport_physical_rect(
    width: f32,
    height: f32,
    scale: f32,
    menu_bar_visible: bool,
    ribbon_visible: bool,
    left_panel_visible: bool,
) -> ViewportLayout {
    use layout::{RIGHT_PANEL_WIDTH as RIGHT_PANEL, STATUS_HEIGHT as STATUS};
    let header = layout::TITLE_BAR_HEIGHT
        + if menu_bar_visible {
            layout::MENU_HEIGHT
        } else {
            0.
        }
        + if ribbon_visible {
            layout::HEADER_HEIGHT - layout::MENU_HEIGHT
        } else {
            0.
        };
    let left = if left_panel_visible {
        layout::LEFT_PANEL_WIDTH
    } else {
        0.
    };
    ViewportLayout {
        x: (left * scale).round() as i32,
        y: (header * scale).round() as i32,
        width: ((width - left - RIGHT_PANEL).max(1.0) * scale)
            .round()
            .max(1.0) as u32,
        height: ((height - header - STATUS).max(1.0) * scale)
            .round()
            .max(1.0) as u32,
        scale,
    }
}

fn publish_layout(
    bridge: &ViewportBridge,
    window: &Window,
    preferences: &crate::app::NativeUiPreferences,
) -> ViewportLayout {
    let size = window.viewport_size();
    let layout = viewport_physical_rect(
        f32::from(size.width),
        f32::from(size.height),
        window.scale_factor(),
        preferences.menu_bar_visible,
        preferences.ribbon_visible,
        preferences.left_panel_visible,
    );
    bridge.publish_layout(layout);
    layout
}

fn selection_region_in_context(
    document: &Document,
    camera: OrbitCamera,
    size: (f32, f32),
    start: (f32, f32),
    end: (f32, f32),
    crossing: bool,
    context: &EditContext,
) -> Vec<SelectionTarget> {
    let region = [
        start.0.min(end.0),
        start.1.min(end.1),
        start.0.max(end.0),
        start.1.max(end.1),
    ];
    let mut selected = Vec::new();
    let Ok(occurrences) = document.scene_geometry_refs() else {
        return selected;
    };
    for occurrence in occurrences {
        let Some(resolved) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        let mut projected_vertices =
            std::collections::BTreeMap::<VertexId, Option<(f32, f32)>>::new();
        for vertex in geometry.vertices.values() {
            if vertex
                .legacy_aliases
                .iter()
                .any(|a| a.source == meshcraft_core::document::architecture::GENERATED)
            {
                continue;
            }

            projected_vertices.insert(
                vertex.id,
                project_point(
                    camera,
                    size,
                    resolved.world_transform.transform_point(vertex.position),
                ),
            );
        }
        let object_points = projected_vertices
            .values()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        let active_depth = match context {
            EditContext::Occurrence(active) => active.path.len(),
            _ if document
                .scene_container_occurrence()
                .is_some_and(|root| occurrence.path.starts_with(&root.path)) =>
            {
                1
            }
            _ => 0,
        };
        if occurrence.path.len() > active_depth && !object_points.is_empty() {
            let bounds = projected_bounds(&object_points);
            let selected_object = if crossing {
                rects_intersect(bounds, region)
            } else {
                rect_contains(region, bounds[0], bounds[1])
                    && rect_contains(region, bounds[2], bounds[3])
            };
            if selected_object {
                selected.push(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Occurrence,
                });
            }
            continue; // A closed instance is atomic even when the region crosses only part of it.
        }
        for segment in geometry.segments.values() {
            let [Some(a), Some(b)] = segment.endpoints.map(|reference| {
                geometry.vertex_position(reference).and_then(|point| {
                    project_point(
                        camera,
                        size,
                        resolved.world_transform.transform_point(point),
                    )
                })
            }) else {
                continue;
            };
            let hit = if crossing {
                segment_intersects_rect(a, b, region)
            } else {
                rect_contains(region, a.0, a.1) && rect_contains(region, b.0, b.1)
            };
            if hit {
                selected.push(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: meshcraft_core::document::architecture::semantic_entity(
                        &segment.extensions,
                    )
                    .unwrap_or(SelectedEntity::Segment(segment.id)),
                });
            }
        }
        for triangle in geometry.triangles.values() {
            let Some(points) = triangle
                .vertices
                .map(|reference| {
                    geometry.vertex_position(reference).and_then(|point| {
                        project_point(
                            camera,
                            size,
                            resolved.world_transform.transform_point(point),
                        )
                    })
                })
                .into_iter()
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let hit = if crossing {
                triangle_intersects_rect([points[0], points[1], points[2]], region)
            } else {
                points
                    .iter()
                    .all(|point| rect_contains(region, point.0, point.1))
            };
            if hit {
                selected.push(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: meshcraft_core::document::architecture::semantic_entity(
                        &triangle.extensions,
                    )
                    .unwrap_or(SelectedEntity::Triangle(triangle.id)),
                });
            }
        }
    }
    for annotation in document.annotations() {
        let mut points = annotation
            .edges
            .iter()
            .flatten()
            .copied()
            .chain([annotation.position])
            .filter_map(|p| project_point(camera, size, p))
            .collect::<Vec<_>>();
        if let Some([x0, y0, x1, y1]) = annotations::label_rect(&annotation, camera, size) {
            points.extend([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        }
        if points.is_empty() {
            continue;
        }
        let hit = if crossing {
            points.iter().any(|p| rect_contains(region, p.0, p.1))
                || points
                    .windows(2)
                    .any(|p| segment_intersects_rect(p[0], p[1], region))
        } else {
            points.iter().all(|p| rect_contains(region, p.0, p.1))
        };
        if hit {
            selected.push(annotation.target);
        }
        points.clear();
    }
    for handle in hotspots::handles(document) {
        let authored = match context {
            EditContext::Document => {
                handle.occurrence.is_scene_root()
                    || document.scene_container_occurrence().as_ref() == Some(&handle.occurrence)
            }
            EditContext::Occurrence(active) => &handle.occurrence == active,
            EditContext::Definition(id) => document
                .resolve_occurrence(&handle.occurrence)
                .is_some_and(|r| r.definition.id == *id),
        };
        if !authored {
            continue;
        }
        if let Some((x, y)) = project_point(camera, size, handle.world)
            && rect_contains(region, x, y)
        {
            selected.push(SelectionTarget {
                occurrence: handle.occurrence,
                entity: SelectedEntity::SemanticHandle {
                    kind: meshcraft_core::selection::SelectionKind::Hotspot,
                    id: handle.hotspot.id,
                },
            });
        }
    }
    selected.retain(|target| {
        crossing
            || document.architecture_element(target).is_none_or(|e| {
                document.architecture_mesh(target).is_ok_and(|mesh| {
                    mesh.points.iter().all(|p| {
                        project_point(camera, size, e.transform.transform_point(*p))
                            .is_some_and(|(x, y)| rect_contains(region, x, y))
                    })
                })
            })
    });
    selected.sort();
    selected.dedup();
    selected
}

fn project_point(camera: OrbitCamera, size: (f32, f32), point: Vec3) -> Option<(f32, f32)> {
    let position = camera.position();
    let forward = (camera.target - position).try_normalized(1.0e-12)?;
    let right = forward.cross(camera.up).try_normalized(1.0e-12)?;
    let up = right.cross(forward).try_normalized(1.0e-12)?;
    let relative = point - position;
    let depth = relative.dot(forward);
    if depth <= 1.0e-6 {
        return None;
    }
    let tangent = (camera.field_of_view.to_radians() * 0.5).tan() as f64;
    let aspect = f64::from(size.0 / size.1.max(1.0));
    let scale = if camera.orthographic {
        f64::from(camera.distance)
    } else {
        depth
    };
    let x = relative.dot(right) / (scale * tangent * aspect);
    let y = relative.dot(up) / (scale * tangent);
    Some((
        ((x + 1.0) * 0.5 * f64::from(size.0)) as f32,
        ((1.0 - y) * 0.5 * f64::from(size.1)) as f32,
    ))
}

fn projected_bounds(points: &[(f32, f32)]) -> [f32; 4] {
    [
        points
            .iter()
            .map(|point| point.0)
            .fold(f32::INFINITY, f32::min),
        points
            .iter()
            .map(|point| point.1)
            .fold(f32::INFINITY, f32::min),
        points
            .iter()
            .map(|point| point.0)
            .fold(f32::NEG_INFINITY, f32::max),
        points
            .iter()
            .map(|point| point.1)
            .fold(f32::NEG_INFINITY, f32::max),
    ]
}

fn rect_contains(rect: [f32; 4], x: f32, y: f32) -> bool {
    x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3]
}
fn rects_intersect(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

fn segment_intersects_rect(a: (f32, f32), b: (f32, f32), rect: [f32; 4]) -> bool {
    if rect_contains(rect, a.0, a.1) || rect_contains(rect, b.0, b.1) {
        return true;
    }
    let corners = [
        (rect[0], rect[1]),
        (rect[2], rect[1]),
        (rect[2], rect[3]),
        (rect[0], rect[3]),
    ];
    (0..4).any(|index| segments_intersect(a, b, corners[index], corners[(index + 1) % 4]))
}

fn segments_intersect(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    fn orient(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    }
    let ab_c = orient(a, b, c);
    let ab_d = orient(a, b, d);
    let cd_a = orient(c, d, a);
    let cd_b = orient(c, d, b);
    ab_c * ab_d <= 0.0 && cd_a * cd_b <= 0.0
}

fn triangle_intersects_rect(triangle: [(f32, f32); 3], rect: [f32; 4]) -> bool {
    triangle
        .iter()
        .any(|point| rect_contains(rect, point.0, point.1))
        || (0..4).any(|index| {
            let a = (
                if index < 2 { rect[0] } else { rect[2] },
                if index == 0 || index == 3 {
                    rect[1]
                } else {
                    rect[3]
                },
            );
            point_in_triangle(a, triangle)
        })
        || (0..3)
            .any(|index| segment_intersects_rect(triangle[index], triangle[(index + 1) % 3], rect))
}

fn point_in_triangle(point: (f32, f32), triangle: [(f32, f32); 3]) -> bool {
    let [a, b, c] = triangle;
    let ab = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
    let bc = (c.0 - b.0) * (point.1 - b.1) - (c.1 - b.1) * (point.0 - b.0);
    let ca = (a.0 - c.0) * (point.1 - c.1) - (a.1 - c.1) * (point.0 - c.0);
    (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
}

fn model_settings_payload(settings: &ModelSettings) -> CommandPayload {
    let mut payload = CommandPayload::default();
    if let Ok(Value::Object(values)) = serde_json::to_value(settings) {
        payload.values = values.into_iter().collect();
    }
    payload
}

fn command_default_payload(command: &CommandDescriptor, document: &Document) -> CommandPayload {
    let mut payload = CommandPayload::default();
    match command.id.as_str() {
        "geometry.add_cuboid" => {
            payload
                .values
                .insert("dimensions".into(), json!([2.0, 2.0, 2.0]));
            payload
                .values
                .insert("origin".into(), json!([0.0, 0.0, 0.0]));
            return payload;
        }
        "model.update_settings" => return model_settings_payload(document.model_settings()),
        "object.array_selection" => {
            payload.values.insert("count".into(), json!(2));
            payload.values.insert("spacing".into(), json!(2.0));
        }
        "object.create_from_selection" => {
            payload.values.insert("name".into(), json!("Object"));
        }
        "paint.set_color" => {
            payload.values.insert("red".into(), json!(210));
            payload.values.insert("green".into(), json!(210));
            payload.values.insert("blue".into(), json!(210));
            payload.values.insert("alpha".into(), json!(255));
        }
        _ => {}
    }
    for parameter in &command.parameters {
        if payload.values.contains_key(&parameter.name) || !parameter.required {
            continue;
        }
        let value = match parameter.value_type {
            CommandValueType::Boolean => Value::Bool(false),
            CommandValueType::Integer => json!(1),
            CommandValueType::Number => json!(1.0),
            CommandValueType::String => Value::String(String::new()),
            CommandValueType::Array => Value::Array(Vec::new()),
            CommandValueType::Object => Value::Object(Default::default()),
            CommandValueType::Any => Value::Null,
        };
        payload.values.insert(parameter.name.clone(), value);
    }
    payload
}

fn command_button_id(area: &str, command: &CommandDescriptor) -> SharedString {
    format!("{area}/{}", command.id.as_str()).into()
}

fn command_enabled(session: &MeshcraftApp, command: &CommandDescriptor) -> bool {
    session.native_command_state(command.id.as_str()).enabled
        && !matches!(
            command.id.as_str(),
            "object.delete_unused_definition" | "selection.update_display" | "view.camera.set_pose"
        )
}

fn command_label(command: &CommandDescriptor) -> &str {
    command.label.rsplit("> ").next().unwrap_or(&command.label)
}

fn ribbon_command_for_key<'a>(
    key: &str,
    catalog: &'a [CommandDescriptor],
) -> Option<&'a CommandDescriptor> {
    let ids: &[&str] = match key {
        "line" => &["tool.builtin.line", "tool.line"],
        "point" => &["tool.builtin.point"],
        "polyline" => &["tool.builtin.polyline"],
        "construction-polyline" => &["tool.builtin.construction_line"],
        "double-line" => &["tool.builtin.double_line"],
        "meshed-double-line" => &["tool.builtin.ribbon_3d"],
        "closed-mesh" => &["tool.builtin.mesh"],
        "rectangle" => &["tool.builtin.rectangle", "tool.rectangle"],
        "circle" => &["tool.builtin.circle"],
        "arc" => &["tool.builtin.arc"],
        "hotspot" => &["hotspot.begin_placement"],
        "add-hotspot" => &["hotspot.add"],
        "attach-selection" => &["hotspot.attach"],
        "select-attached" => &["hotspot.select_attached"],
        "pick-reference" => &["hotspot.reference"],
        "clear-reference" => &["hotspot.clear_reference"],
        "delete-hotspot" => &["hotspot.delete"],
        "select" | "window-select" | "select-operator" => &["tool.select"],
        "move" => &["tool.builtin.move"],
        "copy" => &["edit.copy_selection"],
        "cut" => &["edit.cut_selection"],
        "paste" => &["edit.paste"],
        "paste-in-place" => &["edit.paste_in_place"],
        "copy-as-script" => &["edit.copy_as_script"],
        "move-copy" => &["tool.move_copy"],
        "rotate-3d" => &["tool.builtin.rotate", "tool.builtin.rotate_2"],
        "rotate-copy-3d" | "rotate-copy" => &["tool.rotate_copy"],
        "scale" => &["tool.builtin.scale"],
        "stretch" => &["tool.builtin.stretch"],
        "extrude" => &["tool.builtin.push_pull", "tool.builtin.extrude_swipe"],
        "flip" => &["geometry.flip_selection"],
        "mirror" => &["tool.builtin.mirror"],
        "make-object" | "create-object" => &["object.create_from_selection"],
        "edit-object" | "edit-definition" => &["selection.edit_object"],
        "exit-object" => &["selection.exit_context"],
        "place-object" => &["object.place_selected_definition"],
        "explode-object" | "explode" => &["object.explode_selection"],
        "array" => &["object.array_selection"],
        "dimension" => &["tool.builtin.linear_dimension"],
        "text" => &["tool.builtin.text"],
        "leader" => &["tool.leader"],
        "wall" => &["tool.builtin.arch_wall"],
        "slab" => &["tool.builtin.arch_slab"],
        "freeform-slab" => &["tool.arch_slab_freeform"],
        "add-hole" => &["tool.builtin.arch_add_hole"],
        "stair" => &["tool.builtin.arch_stair"],
        "frame" => &[
            "tool.builtin.arch_window_frame",
            "tool.builtin.arch_door_frame",
        ],
        "door" => &["tool.builtin.arch_door_frame"],
        "window" => &["tool.builtin.arch_window_frame"],
        "cuboid" => &["tool.cuboid"],
        "sphere" => &["tool.builtin.primitive_sphere"],
        "cylinder" => &["tool.builtin.primitive_cylinder"],
        "cone" => &["tool.builtin.primitive_cone"],
        "voxel" | "voxel-edit" => &["tool.builtin.voxel"],
        "voxel-fill" => &["tool.builtin.voxel_volume"],
        "pipe" | "plumbing" => &["tool.builtin.hvac_plumbing"],
        "duct" | "ventilation" => &["tool.builtin.hvac_ventilation"],
        "distance" | "distance-operator" => &["tool.distance"],
        "random-offset" => &["tool.builtin.random_offset"],
        "surface-array" => &["tool.builtin.random_surface_array"],
        "mesh-intersection" => &["tool.builtin.mesh_intersection"],
        "offset" => &["tool.builtin.line_offset"],
        "sweep" => &[
            "tool.builtin.extrude_swipe",
            "tool.builtin.extrude_swipe_3d",
        ],
        "planar-regularize" => &["tool.builtin.mesh_regularize"],
        "surface-remesh" => &["tool.builtin.surface_remesh"],
        "linear-array" => &["tool.builtin.copy_multiple"],
        "planar-array" => &["tool.builtin.planar_translate_multiple"],
        "spatial-array" => &["tool.builtin.volumetric_translate_multiple"],
        "rotational-array" => &["tool.builtin.rotational_array"],
        "helical-array" => &["tool.builtin.helical_array"],
        "orbit" => &["view.orbit"],
        "pan" | "zoom" => &[],
        "fit-model" => &["view.frame_document"],
        "reset-camera" => &["view.camera.reset"],
        "front-view" => &["view.ortho.front"],
        "top-view" => &["view.ortho.top"],
        "perspective" => &["view.camera.perspective"],
        "walkthrough" => &["view.camera.walkthrough"],
        "orthographic" => &["view.camera.orthographic"],
        "permanent-grid" => &["view.grid.toggle"],
        "guides" => &["view.guides.toggle"],
        "grid-guide" => &["view.grid_guide"],
        "axis-guide" => &["view.axis_guide"],
        "selection-info" => &["view.selection"],
        "object-info" => &["view.object_info"],
        "objects" => &["view.objects"],
        "architecture-settings" => &["view.architecture_settings"],
        "hvac-settings" => &["view.hvac_settings"],
        "hotspot-settings" => &["view.hotspot_settings"],
        "shader-settings" => &["view.panel.shaders.toggle"],
        "realtime-render" | "ray-trace" => &["render.raytrace"],
        "global-illumination" => &["render.global_illumination"],
        "stop-render" => &["render.stop"],
        "save-render" => &["render.save"],
        "new" => &["file.new"],
        "open" => &["file.open"],
        "save" => &["file.save"],
        "save-as" => &["file.save_as"],
        "recent-projects" | "recent-files" => &["file.open_recent"],
        "exit" => &["app.quit"],
        "undo" => &["edit.undo"],
        "redo" => &["edit.redo"],
        "delete" => &["edit.delete_selection"],
        "import-objects" => &["object.import_scene"],
        "reference-object" => &["object.reference_scene"],
        "refresh-object" => &["object.refresh_reference"],
        "export" => &["export.mesh"],
        "model-settings" | "settings-panel" => &["view.model_settings"],
        "ui-settings" => &["view.panel.ui_settings.toggle"],
        "mcp-settings" => &["view.panel.mcp.toggle"],
        "selection-panel" => &["view.selection"],
        "object-panel" => &["view.object_info"],
        "reset-layout" => &["view.workspace.reset_layout"],
        "tutorials" => &["view.panel.tutorials.toggle"],
        "about-meshcraft" => &["help.about"],
        _ => return None,
    };
    ids.iter()
        .find_map(|id| catalog.iter().find(|command| command.id.as_str() == *id))
}

impl Render for MeshcraftShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(text) = self.session.native_take_clipboard_write() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
        if self.close_requested {
            window.remove_window();
            cx.quit();
        }
        if !self.host_watch_started {
            self.host_watch_started = true;
            cx.spawn_in(window, async move |shell, cx| {
                loop {
                    gpui::Timer::after(Duration::from_millis(16)).await;
                    if shell
                        .update(cx, |this, cx| {
                            if this.walking
                                && (this.walk_keys.iter().any(|key| *key)
                                    || this.session.native_walkthrough_animating())
                            {
                                cx.notify();
                            }
                            let revision = this.session.document_revision();
                            let status = this.session.status_text().to_owned();
                            this.session.poll_native_host_work();
                            if automation::poll(this) {
                                cx.notify();
                            }
                            if scripts::tick(this) {
                                this.refresh_scene();
                                cx.notify();
                            }
                            if revision != this.session.document_revision()
                                || status != this.session.status_text()
                            {
                                this.refresh_scene();
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        if self.walking {
            let now = Instant::now();
            let dt = now.duration_since(self.walk_tick).as_secs_f64();
            self.walk_tick = now;
            if !window.is_window_active()
                || !self.focus.is_focused(window)
                || self.viewport_input_blocked()
                || self.settings.active.is_some()
            {
                self.walk_keys = [false; 7];
            }
            let (eye, forward) = self.session.native_step_walkthrough(dt, self.walk_keys);
            self.walk_keys[6] = false;
            self.camera.eye = Some(eye);
            self.camera.target = eye + forward * f64::from(self.camera.distance);
            self.bridge.publish_camera(self.camera);
        }
        let preferences = self.session.native_ui_preferences();
        self.layout = publish_layout(&self.bridge, window, &preferences);
        if self.open_menu.is_some_and(|menu| {
            !preferences.menu_bar_visible
                || !preferences
                    .menu_visibility
                    .get(menu)
                    .copied()
                    .unwrap_or(true)
        }) {
            self.open_menu = None;
        }
        let rem = px(16.0 * self.session.native_ui_preferences().ui_text_scale);
        if cx.global::<Theme>().font_size != rem {
            cx.global_mut::<Theme>().font_size = rem;
        }
        if window.rem_size() != rem {
            window.set_rem_size(rem);
        }
        self.session.poll_native_host_work();
        if self.cached_document_revision != Some(self.session.native_visual_revision()) {
            self.refresh_scene();
        }
        if let Some(panel) = self.session.native_take_panel_focus() {
            if panel == "script_new" || panel == "script_edit" {
                scripts::edit(self, panel == "script_new", window, cx);
            }
            if panel == "material_editor_new" {
                materials::edit(self, self.cached_document.next_material(), window, cx);
            } else if panel == "material_editor" {
                let material = self
                    .cached_document
                    .materials()
                    .into_iter()
                    .find(|m| self.materials.selected.as_ref() == Some(&m.id))
                    .unwrap_or_default();
                materials::edit(self, material, window, cx);
            } else if let Some((index, title)) = layout::panel_location(&panel) {
                self.open_settings_panel = Some(title);
                self.collapsed_settings_panels.remove(title);
                self.settings_scroll.scroll_to_top_of_item(index);
            }
        }
        settings::prepare(self, window, cx);
        annotations::prepare(self, window, cx);
        architecture::prepare(self, window, cx);
        automation::prepare(self, window, cx);
        self.prepare_palette(window, cx);
        let frame = self.bridge.latest_frame();
        if let Some(frame) = frame.as_ref() {
            self.last_sequence = frame.sequence;
        }

        let menu_bar = div()
            .h(px(layout::MENU_HEIGHT))
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(8.))
            .bg(rgb(0x171b22))
            .child(
                div()
                    .mr(px(7.))
                    .text_lg()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(0xe8eff7))
                    .child("Meshcraft"),
            )
            .children(
                MENU_NAMES
                    .iter()
                    .copied()
                    .filter(|menu| {
                        preferences
                            .menu_visibility
                            .get(*menu)
                            .copied()
                            .unwrap_or(true)
                    })
                    .map(|menu| {
                        let active = self.open_menu == Some(menu);
                        let button = Button::new(SharedString::from(format!("menu/{menu}")))
                            .label(menu)
                            .xsmall()
                            .text()
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.toggle_menu(menu);
                                    cx.notify();
                                }),
                            );
                        let bounds_store = Arc::clone(&self.menu_bounds);
                        div()
                            .relative()
                            .child(if active { button.primary() } else { button })
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        if let Ok(mut menu_bounds) = bounds_store.lock() {
                                            menu_bounds.insert(menu, bounds);
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                    }),
            )
            .child(div().flex_1())
            .child(
                Button::new("commands/open")
                    .label("Commands  Ctrl+Shift+P")
                    .small()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_command_palette();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .ml(px(8.))
                    .text_xs()
                    .text_color(rgb(0x9db0c4))
                    .child("GPUI · raylib 6"),
            );

        let ribbon_tabs = div()
            .h(px(layout::TAB_HEIGHT))
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .bg(rgb(0x222a34))
            .children(RIBBON_TABS.iter().copied().map(|tab| {
                let active = self.active_ribbon_tab == tab;
                let button = Button::new(SharedString::from(format!("ribbon-tab/{tab}")))
                    .label(tab)
                    .small()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.active_ribbon_tab = tab;
                            this.open_menu = None;
                            cx.notify();
                        }),
                    );
                if active {
                    button.primary()
                } else {
                    button.ghost()
                }
            }));

        let (ribbon, left_panel, right_panel) = {
            let tab = self.active_ribbon_tab;
            let mut bindings = layout::LayoutBindings::new(self, cx);
            (
                layout::ribbon_content(tab, &mut bindings),
                layout::left_toolbars(&mut bindings),
                layout::right_settings_panel(&mut bindings),
            )
        };

        let viewport_bridge_down_left =
            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                let (x, y) = this.viewport_point(event.position, window);
                window.focus(&this.focus);
                this.pointer_down(event.button, x, y, event.modifiers);
                cx.notify();
            });
        let viewport_bridge_move = cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
            let (x, y) = this.viewport_point(event.position, window);
            this.pointer_move(x, y, event.modifiers, event.pressed_button);
            cx.notify();
        });
        let viewport_bridge_up_left =
            cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                let (x, y) = this.viewport_point(event.position, window);
                this.pointer_up(event.button, x, y);
                cx.notify();
            });
        let viewport_bridge_scroll =
            cx.listener(|this, event: &gpui::ScrollWheelEvent, window, cx| {
                let (x, y) = this.viewport_point(event.position, window);
                if !this.viewport_input_blocked() && this.cursor_in_viewport(x, y) {
                    let steps = match event.delta {
                        gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y) / 80.0,
                        gpui::ScrollDelta::Lines(delta) => delta.y,
                    };
                    this.zoom_camera(wheel_zoom_steps(steps, event.modifiers));
                    cx.notify();
                }
            });
        let viewport = div()
            .id("meshcraft-viewport")
            .relative()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_hidden()
            .bg(rgb(0xe0ecf7))
            .child(self.viewport_canvas.clone())
            .on_mouse_down(gpui::MouseButton::Left, viewport_bridge_down_left)
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let (x, y) = this.viewport_point(event.position, window);
                    window.focus(&this.focus);
                    this.pointer_down(event.button, x, y, event.modifiers);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Middle,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let (x, y) = this.viewport_point(event.position, window);
                    window.focus(&this.focus);
                    this.pointer_down(event.button, x, y, event.modifiers);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Middle,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    let (x, y) = this.viewport_point(event.position, window);
                    this.pointer_up(event.button, x, y);
                    cx.notify();
                }),
            )
            .on_mouse_move(viewport_bridge_move)
            .on_mouse_up(gpui::MouseButton::Left, viewport_bridge_up_left)
            .on_mouse_up(
                gpui::MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    let (x, y) = this.viewport_point(event.position, window);
                    this.pointer_up(event.button, x, y);
                    cx.notify();
                }),
            )
            .on_scroll_wheel(viewport_bridge_scroll)
            .child(
                div()
                    .absolute()
                    .top(px(10.))
                    .left(px(12.))
                    .px(px(9.))
                    .py(px(5.))
                    .rounded(px(3.))
                    .bg(rgb(0xf4f7fa))
                    .text_sm()
                    .text_color(rgb(0x27394b))
                    .child(format!(
                        "{} · {} · middle-drag / Shift+right-drag pan · wheel {}{}{}",
                        self.session.active_tool_label(),
                        if self.walking {
                            "Walkthrough: WASD move · Q/E height · Space jump · right-drag look"
                        } else if self.camera.orthographic {
                            "Orthographic · right-drag orbit"
                        } else {
                            "Perspective · right-drag orbit"
                        },
                        if self.walking { "move" } else { "zoom" },
                        if self.session.native_point_input_hint().is_empty() {
                            ""
                        } else {
                            " · "
                        },
                        self.session.native_point_input_hint()
                    )),
            );

        let viewport = viewport
            .children(annotations::labels(self))
            .children(architecture::controls(self));
        let viewport = viewport.when(!self.scripts.errors.is_empty(), |view| {
            view.child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .left(px(12.))
                    .right(px(12.))
                    .p(px(8.))
                    .bg(rgb(0x42292b))
                    .text_color(rgb(0xffc3b8))
                    .child(self.scripts.errors.join("\n")),
            )
        });
        let viewport = viewport.when(
            self.session.native_edit_context() != EditContext::Document,
            |viewport| {
                viewport.child(
                    div()
                        .absolute()
                        .top(px(10.))
                        .right(px(12.))
                        .occlude()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            Button::new("viewport/selection.exit_context")
                                .label("Finish Editing")
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dispatch("selection.exit_context");
                                    cx.notify();
                                })),
                        ),
                )
            },
        );

        let viewport = viewport.when_some(self.feedback.as_ref(), |viewport, feedback| {
            viewport.child(
                div()
                    .absolute()
                    .top(px(47.))
                    .left(px(12.))
                    .right(px(12.))
                    .child(
                        div()
                            .px(px(9.))
                            .py(px(5.))
                            .rounded(px(3.))
                            .bg(rgb(0xf4f7fa))
                            .text_sm()
                            .text_color(rgb(0x27394b))
                            .child(feedback.text.clone()),
                    ),
            )
        });

        let body = div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .when(preferences.left_panel_visible, |body| {
                body.child(left_panel)
            })
            .child(viewport)
            .child(right_panel);
        let status = div()
            .h(px(layout::STATUS_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(9.))
            .bg(rgb(0x171b22))
            .text_xs()
            .text_color(rgb(0xb7c3d0))
            .child(self.session.status_text().to_owned())
            .child(div().flex_1())
            .child(frame.map_or_else(
                || "Viewport starting".to_owned(),
                |frame| format!("raylib frame {} · {:.1} ms", frame.sequence, frame.frame_ms),
            ));

        if self.open_menu != Some("File") {
            self.files = files::Ui::default();
        }
        let popup = self.open_menu.and_then(|menu| {
            let bounds = self.menu_bounds.lock().ok()?.get(menu).copied()?;
            let catalog = MeshcraftApp::command_catalog();
            let mut entries = layout::menu_items(menu)
                .iter()
                .map(|label| {
                    let key = layout::menu_command_key(label);
                    let command = ribbon_command_for_key(&key, &catalog).cloned();
                    ((*label).to_owned(), command)
                })
                .collect::<Vec<_>>();
            let bound_ids = entries
                .iter()
                .filter_map(|(_, command)| command.as_ref())
                .map(|command| command.id.clone())
                .collect::<Vec<_>>();
            entries.extend(
                catalog
                    .into_iter()
                    .filter(|command| {
                        command
                            .presentation
                            .menu_path
                            .first()
                            .is_some_and(|root| root == menu)
                            && !bound_ids.contains(&command.id)
                    })
                    .map(|command| (command_label(&command).to_owned(), Some(command))),
            );
            if menu == "File" {
                return Some(files::popup(self, entries, bounds, window, cx));
            }
            Some(
                div()
                    .id("menu-popup")
                    .absolute()
                    .top(bounds.origin.y + bounds.size.height + px(3.))
                    .left(bounds.origin.x)
                    .w(px(235.))
                    .max_h(px(640.))
                    .p(px(5.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .border_1()
                    .border_color(rgb(0x536477))
                    .bg(rgb(0x202832))
                    .shadow_lg()
                    .occlude()
                    .overflow_y_scroll()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.open_menu = None;
                        cx.notify();
                    }))
                    .children(entries.into_iter().map(|(label, command)| {
                        let button =
                            Button::new(SharedString::from(format!("menu/{menu}/{label}")))
                                .child(labeled_command(
                                    label,
                                    command.as_ref().and_then(command_icon),
                                    201.,
                                ))
                                .small()
                                .text();
                        if let Some(command) = command {
                            let id = command.id.as_str().to_owned();
                            button
                                .disabled(!command_enabled(&self.session, &command))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.open_menu = None;
                                        this.dispatch_with_clipboard(&id, cx);
                                        cx.notify();
                                    }),
                                )
                        } else {
                            button.disabled(true)
                        }
                    }))
                    .into_any_element(),
            )
        });

        let command_palette = self.command_palette_open.then(|| {
            let commands = MeshcraftApp::command_catalog()
                .into_iter()
                .filter(|command| palette_matches(command, &self.palette_query))
                .collect::<Vec<_>>();
            let width = (f32::from(window.viewport_size().width) - 32.).clamp(220., 580.);
            let left = ((f32::from(window.viewport_size().width) - width) / 2.).max(0.);
            let list_height = (f32::from(window.viewport_size().height) - 240.).clamp(100., 390.);
            div()
                .id("command-palette")
                .occlude()
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.command_palette_open = false;
                    window.focus(&this.focus);
                    cx.notify();
                }))
                .absolute()
                .top(px(80.))
                .left(px(left))
                .w(px(width))
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_1()
                .border_color(rgb(0x60768a))
                .bg(rgb(0x1e2630))
                .shadow_lg()
                .child(
                    div()
                        .text_sm()
                        .child(format!("Commands · {} matches", commands.len())),
                )
                .child(
                    Input::new(self.palette_input.as_ref().expect("prepared palette input"))
                        .w_full(),
                )
                .child(
                    div()
                        .id("command-palette-results")
                        .max_h(px(list_height))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .when(commands.is_empty(), |list| {
                            list.child(
                                div()
                                    .p(px(10.))
                                    .text_sm()
                                    .text_color(rgb(0xa4b2c1))
                                    .child("No matching commands"),
                            )
                        })
                        .children(commands.into_iter().map(|command| {
                            let id = command.id.as_str().to_owned();
                            Button::new(command_button_id("palette", &command))
                                .child(labeled_command(
                                    command.presentation.palette_path.clone(),
                                    command_icon(&command),
                                    width - 52.,
                                ))
                                .small()
                                .text()
                                .w_full()
                                .disabled(!command_enabled(&self.session, &command))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.command_palette_open = false;
                                    window.focus(&this.focus);
                                    this.dispatch_with_clipboard(&id, cx);
                                    cx.notify();
                                }))
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xa4b2c1))
                        .child("Enter: run first available match · Escape: close"),
                )
                .into_any_element()
        });

        let close_view = cx.entity().downgrade();
        let title_bar = gpui_component::TitleBar::new()
            .child(div().text_sm().child("Meshcraft"))
            .on_close_window(move |_, _, cx| {
                let _ = close_view.update(cx, |shell, cx| {
                    shell.dispatch("app.quit");
                    cx.notify();
                });
            });
        let mut root = div()
            .id("meshcraft-shell")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(0x1b2027))
            .text_color(rgb(0xe3e8ee))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                    && event.keystroke.key == "v"
                {
                    this.sync_clipboard(cx);
                }
                if scripts::capture_key(this, event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.scripts.editor.is_some() {
                    if event.keystroke.key == "escape" {
                        scripts::close(this, window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.materials.editor.is_some() {
                    if event.keystroke.key == "escape" {
                        this.materials.editor = None;
                        window.focus(&this.focus);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.command_palette_open {
                    if event.keystroke.key == "escape"
                        || (event.keystroke.key == "p"
                            && event.keystroke.modifiers.control
                            && event.keystroke.modifiers.shift)
                    {
                        this.command_palette_open = false;
                        window.focus(&this.focus);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    return;
                }
                if this.session.native_document_replacement_prompt().is_some() {
                    if event.keystroke.key == "escape" {
                        this.session.native_resolve_document_replacement(
                            crate::app::ReplacementDecision::Cancel,
                        );
                        this.refresh_scene();
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.open_menu.is_some() && event.keystroke.key == "escape" {
                    this.open_menu = None;
                    window.focus(&this.focus);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.settings.active.is_some() {
                    if event.keystroke.key == "escape" {
                        this.settings.active = None;
                        window.focus(&this.focus);
                    }
                    return;
                }
                this.shortcut(event);
                cx.notify();
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, _| {
                if let Some(index) = walk_key(&event.keystroke.key) {
                    this.walk_keys[index] = false;
                }
            }))
            .child(title_bar)
            .when(preferences.menu_bar_visible, |root| root.child(menu_bar))
            .when(preferences.ribbon_visible, |root| {
                root.child(ribbon_tabs).child(ribbon)
            })
            .child(body)
            .child(status);
        if let Some(popup) = popup {
            root = root.child(popup);
        }
        if let Some(command_palette) = command_palette {
            root = root.child(command_palette);
        }
        if let Some(editor) = scripts::overlay(self, cx) {
            root = root.child(editor);
        }
        if let Some(material_editor) = materials::overlay(self, cx) {
            root = root.child(material_editor);
        }
        root = root.children(files::replacement_dialog(self, cx));
        root
    }
}

pub fn run_native(window_size: [f32; 2], maximized: bool, session: MeshcraftApp) {
    let mut renderer = RaylibWorker::start();
    let bridge = renderer.bridge.clone();
    let shutdown = renderer.shutdown.clone();
    Application::new()
        .with_assets(crate::assets::ComponentAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            // Strong, consistent button/list hover contrast on the dark shell (#28).
            {
                let theme = cx.global_mut::<Theme>();
                theme.colors.secondary = rgb(0x31465b).into();
                theme.colors.secondary_hover = rgb(0x4b6682).into();
                theme.colors.secondary_active = rgb(0x58799a).into();
                theme.colors.list_hover = rgb(0x405975).into();
            }
            let requested_size = size(px(window_size[0]), px(window_size[1]));
            let centered = WindowBounds::centered(requested_size, cx);
            let bounds = if maximized {
                match centered {
                    WindowBounds::Windowed(bounds) => WindowBounds::Maximized(bounds),
                    other => other,
                }
            } else {
                centered
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Meshcraft".into()),
                        ..gpui_component::TitleBar::title_bar_options()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    window.on_window_should_close(cx, move |_, cx| {
                        shutdown.stop_and_wait();
                        cx.quit();
                        true
                    });
                    let bridge_for_view = bridge.clone();
                    let shell = cx.new(|view_cx| {
                        let bridge_for_observer = bridge_for_view.clone();
                        publish_layout(&bridge_for_view, window, &session.native_ui_preferences());
                        view_cx
                        .observe_window_bounds(
                            window,
                            move |view: &mut MeshcraftShell,
                                  window: &mut Window,
                                  cx: &mut Context<MeshcraftShell>| {
                                view.layout = publish_layout(&bridge_for_observer, window, &view.session.native_ui_preferences());
                                cx.notify();
                            },
                        )
                        .detach();
                        let shell = MeshcraftShell::new(bridge_for_view, session, view_cx);
                        window.focus(&shell.focus);
                        shell
                    });
                    cx.new(|cx| gpui_component::Root::new(shell, window, cx))
                },
            )
            .expect("open Meshcraft GPUI window");
            cx.activate(true);
        });
    renderer.stop();
}

// Scale the logarithmic zoom rate; Alt takes precedence when both modifiers are held.
fn wheel_zoom_steps(steps: f32, modifiers: gpui::Modifiers) -> f32 {
    steps
        * if modifiers.alt {
            0.05
        } else if modifiers.control {
            0.1
        } else {
            1.0
        }
}

#[cfg(test)]
mod tests {
    use super::layout::ribbon_groups;
    use super::{
        OrbitCamera, ViewportBridge, ViewportLayout, ribbon_command_for_key, viewport_physical_rect,
    };
    use super::{palette_matches, replace_presented_image};
    use crate::MeshcraftApp;
    use gpui::RenderImage;
    use image::{Frame, ImageBuffer};
    use std::sync::Arc;

    #[test]
    fn wheel_modifiers_reduce_the_zoom_rate() {
        let plain = gpui::Modifiers::default();
        let ctrl = gpui::Modifiers {
            control: true,
            ..plain
        };
        let alt = gpui::Modifiers { alt: true, ..plain };
        let both = gpui::Modifiers {
            control: true,
            alt: true,
            ..plain
        };
        for steps in [-3., 1., 4.] {
            assert_eq!(super::wheel_zoom_steps(steps, plain), steps);
            assert_eq!(super::wheel_zoom_steps(steps, ctrl), steps * 0.1);
            assert_eq!(super::wheel_zoom_steps(steps, alt), steps * 0.05);
            assert_eq!(super::wheel_zoom_steps(steps, both), steps * 0.05);
        }
    }
    #[test]
    fn viewport_bounds_follow_visibility_at_each_dpi() {
        for scale in [1.0, 1.5, 2.0] {
            for (menu, ribbon, header) in [
                (true, true, 159.),
                (true, false, 34.),
                (false, true, 125.),
                (false, false, 0.),
            ] {
                for (left, margin) in [(true, 230.), (false, 0.)] {
                    let bounds = viewport_physical_rect(1882., 1043., scale, menu, ribbon, left);
                    assert_eq!(bounds.x, (margin * scale) as i32);
                    assert_eq!(bounds.y, ((header + 34.) * scale).round() as i32);
                    assert_eq!(
                        bounds.width,
                        ((1882. - margin - 330.) * scale).round() as u32
                    );
                    assert_eq!(
                        bounds.height,
                        ((1043. - header - 34. - 26.) * scale).round() as u32
                    );
                }
            }
        }
    }

    #[test]
    fn retired_viewport_frames_are_returned_for_atlas_eviction() {
        fn image() -> Arc<RenderImage> {
            Arc::new(RenderImage::new(vec![Frame::new(ImageBuffer::new(2, 2))]))
        }
        let first = image();
        let weak = Arc::downgrade(&first);
        let mut current = None;
        assert!(replace_presented_image(&mut current, &first).is_none());
        assert!(
            replace_presented_image(&mut current, &first).is_none(),
            "repainting must reuse the atlas entry"
        );
        drop(first);
        for _ in 0..1000 {
            let next = image();
            let previous = current.as_ref().unwrap().id;
            let retired =
                replace_presented_image(&mut current, &next).expect("evict previous GPU frame");
            assert_eq!(retired.id, previous);
            assert_ne!(retired.id, next.id);
            drop(retired);
            assert_eq!(
                Arc::strong_count(&next),
                2,
                "only incoming and displayed CPU frame retained"
            );
        }
        assert!(
            weak.upgrade().is_none(),
            "the initial CPU frame must be released too"
        );
    }

    #[test]
    fn palette_search_matches_all_tokens_case_insensitively_in_names_and_ids() {
        let catalog = MeshcraftApp::command_catalog();
        let line = catalog
            .iter()
            .find(|command| command.id.as_str() == "tool.builtin.line")
            .unwrap();
        assert!(palette_matches(line, ""));
        assert!(palette_matches(line, "LINE tool"));
        assert!(palette_matches(line, "tool.builtin line"));
        assert!(!palette_matches(line, "line sphere"));
    }

    #[test]
    fn experiment_ribbon_structure_and_command_bindings_are_preserved() {
        let commands = MeshcraftApp::command_catalog();
        for tab in [
            "Home",
            "Construction",
            "Modify",
            "Arrays",
            "Architecture",
            "Inspect",
            "View",
            "Settings",
        ] {
            let groups = ribbon_groups(tab);
            assert!(!groups.is_empty(), "{tab} has ribbon groups");
            for group in groups {
                assert!(
                    !group.commands.is_empty(),
                    "{tab} / {} has buttons",
                    group.title
                );
            }
        }

        for (key, id) in [
            ("line", "tool.builtin.line"),
            ("move", "tool.builtin.move"),
            ("copy", "edit.copy_selection"),
            ("move-copy", "tool.move_copy"),
            ("rotate-copy-3d", "tool.rotate_copy"),
            ("make-object", "object.create_from_selection"),
            ("linear-array", "tool.builtin.copy_multiple"),
            ("planar-array", "tool.builtin.planar_translate_multiple"),
            (
                "spatial-array",
                "tool.builtin.volumetric_translate_multiple",
            ),
            ("rotational-array", "tool.builtin.rotational_array"),
            ("helical-array", "tool.builtin.helical_array"),
            ("hotspot", "hotspot.begin_placement"),
            ("wall", "tool.builtin.arch_wall"),
            ("reset-camera", "view.camera.reset"),
            ("permanent-grid", "view.grid.toggle"),
            ("guides", "view.guides.toggle"),
            ("architecture-settings", "view.architecture_settings"),
            ("hvac-settings", "view.hvac_settings"),
            ("realtime-render", "render.raytrace"),
        ] {
            let command = ribbon_command_for_key(key, &commands)
                .unwrap_or_else(|| panic!("ribbon button {key} should be connected"));
            assert_eq!(command.id.as_str(), id, "{key} binds to {id}");
        }
    }

    #[test]
    fn orbit_drag_reverses_both_axes_and_keeps_the_target_fixed() {
        let mut camera = OrbitCamera::default();
        let original_pitch = camera.pitch;
        let original_yaw = camera.yaw;

        camera.orbit_by_drag(12.0, 10.0);

        assert!(camera.pitch > original_pitch);
        assert!(camera.yaw < original_yaw);
        assert_eq!(camera.target, OrbitCamera::default().target);
        assert!(
            (camera.position().distance(camera.target) - f64::from(camera.distance)).abs() < 1.0e-5
        );
    }

    #[test]
    fn orthographic_rays_are_parallel_and_round_trip_in_all_axis_views() {
        use meshcraft_core::geometry::Vec3;
        for id in ["top", "bottom", "left", "right", "front", "back"] {
            let (_, up, eye) = crate::viewport::OrthoView::from_id(id)
                .unwrap()
                .screen_axes();
            let camera = OrbitCamera {
                orthographic: true,
                up,
                yaw: eye.x.atan2(eye.z) as f32,
                pitch: eye.y.asin() as f32,
                ..Default::default()
            };
            let (a, direction) = camera.ray(160., 120., 800., 600.).unwrap();
            let (b, other) = camera.ray(640., 480., 800., 600.).unwrap();
            assert!(direction.distance(other) < 1e-9);
            assert!(a.distance(b) > 1.);
            for depth in [2., 10., 100.] {
                let p = super::project_point(camera, (800., 600.), a + direction * depth).unwrap();
                assert!((p.0 - 160.).abs() < 0.001 && (p.1 - 120.).abs() < 0.001);
            }
            assert!(super::project_point(camera, (800., 600.), Vec3::ZERO).is_some());
        }
    }

    #[test]
    fn leaving_a_vertical_view_keeps_perspective_and_walkthrough_camera_bases_valid() {
        use meshcraft_core::geometry::Vec3;
        for sign in [-1., 1.] {
            let mut camera = OrbitCamera {
                pitch: sign * std::f32::consts::FRAC_PI_2,
                up: Vec3::Z,
                orthographic: true,
                ..Default::default()
            };
            camera.set_perspective();
            assert!(camera.ray(400., 300., 800., 600.).is_some());
            assert!(super::project_point(camera, (800., 600.), Vec3::ZERO).is_some());
            let mut controller = crate::viewport::Viewport::default();
            controller.native_walkthrough_pose(Vec3::Y * 2., Vec3::Y * f64::from(sign));
            let (_, forward) = controller.native_walkthrough_step(
                &meshcraft_core::document::Document::new(),
                0.,
                [false; 7],
            );
            assert!(forward.cross(Vec3::Y).length() > 0.01);
        }
    }

    #[test]
    fn viewport_redraws_only_when_camera_or_layout_changes() {
        let bridge = ViewportBridge::new();
        let initial_revision = *bridge.redraw.0.lock().unwrap();

        bridge.publish_layout(ViewportLayout::default());
        bridge.publish_camera(OrbitCamera::default());
        assert_eq!(*bridge.redraw.0.lock().unwrap(), initial_revision);

        let layout = ViewportLayout {
            x: 10,
            y: 20,
            width: 640,
            height: 480,
            scale: 1.0,
        };
        bridge.publish_layout(layout);
        bridge.publish_layout(layout);
        assert_eq!(*bridge.redraw.0.lock().unwrap(), initial_revision + 1);

        let mut camera = OrbitCamera::default();
        camera.orbit_by_drag(1.0, 1.0);
        bridge.publish_camera(camera);
        assert_eq!(*bridge.redraw.0.lock().unwrap(), initial_revision + 2);
    }
}
