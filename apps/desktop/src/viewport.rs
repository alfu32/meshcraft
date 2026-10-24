#[cfg(not(target_arch = "wasm32"))]
use crate::native_snap::SnapTolerance;
#[cfg(not(target_arch = "wasm32"))]
use crate::native_spatial::{NativeSpatialIndex, NativeSpatialPrimitive};
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use meshcraft_core::document::derived::DerivedIndexCache;
use meshcraft_core::document::{
    DefinitionId, Document, OccurrenceRef, Rgba8, TriangleId, VertexId,
};
use meshcraft_core::geometry::{Affine3, Vec3};
use meshcraft_core::selection::{
    SelectedEntity, SelectionDisplayPolicy, SelectionKind, SelectionTarget,
};
use meshcraft_core::tools::shapes::ShapeMeasurement;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const ISO_X: f64 = std::f64::consts::FRAC_1_SQRT_2;
const DEFAULT_ELEVATION: f64 = 0.615_479_708_670_387_4;
const MIN_ELEVATION: f64 = -1.553_343_034_274_953_2;
const MAX_ELEVATION: f64 = 1.553_343_034_274_953_2;
const DEFAULT_PERSPECTIVE_DISTANCE: f64 = 10.392_304_845_413_264;
const DEFAULT_FIELD_OF_VIEW: f64 = 67.0;
const DEFAULT_NEAR_PLANE: f64 = 0.1;
const DEFAULT_FAR_PLANE: f64 = 500.0;
const WALK_EYE_HEIGHT: f64 = 6.0;
const WALK_LOOK_RADIANS_PER_PIXEL: f64 = 0.25_f64.to_radians();
const WALK_PAN_UNITS_PER_PIXEL: f64 = 0.05;
const WALK_SCROLL_MOVE_SCALE: f64 = 0.2;
const SHADOW_MAP_RESOLUTION: usize = 192;
const GROUND_SHADOW_RESOLUTION: usize = 96;

fn shadow_light_direction() -> Vec3 {
    Vec3::new(-0.45, 0.82, 0.35)
        .try_normalized(1.0e-12)
        .unwrap_or(Vec3::Y)
}

#[derive(Clone, Copy)]
struct WorldTriangle {
    points: [Vec3; 3],
}

#[derive(Clone, Copy)]
struct ProjectedOccluder {
    points: [Pos2; 3],
    depths: [f64; 3],
}

struct ProjectedDepthBuffer {
    rect: Rect,
    cell_size: f32,
    columns: usize,
    rows: usize,
    depths: Vec<f64>,
}

impl ProjectedDepthBuffer {
    fn from_occluders(rect: Rect, occluders: &[ProjectedOccluder], perspective: bool) -> Self {
        let cell_size = (rect.width() / 2048.0).max(rect.height() / 2048.0).max(1.0);
        let columns = (rect.width().max(1.0) / cell_size).ceil() as usize;
        let rows = (rect.height().max(1.0) / cell_size).ceil() as usize;
        let mut buffer = Self {
            rect,
            cell_size,
            columns,
            rows,
            depths: vec![f64::INFINITY; columns.saturating_mul(rows)],
        };
        for occluder in occluders {
            let minimum_x = occluder
                .points
                .iter()
                .map(|point| point.x)
                .fold(f32::INFINITY, f32::min)
                .max(rect.min.x);
            let maximum_x = occluder
                .points
                .iter()
                .map(|point| point.x)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(rect.max.x);
            let minimum_y = occluder
                .points
                .iter()
                .map(|point| point.y)
                .fold(f32::INFINITY, f32::min)
                .max(rect.min.y);
            let maximum_y = occluder
                .points
                .iter()
                .map(|point| point.y)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(rect.max.y);
            if minimum_x > maximum_x || minimum_y > maximum_y {
                continue;
            }
            let first_column = (((minimum_x - rect.min.x) / cell_size).floor() as usize)
                .min(columns.saturating_sub(1));
            let last_column = (((maximum_x - rect.min.x) / cell_size).floor() as usize)
                .min(columns.saturating_sub(1));
            let first_row = (((minimum_y - rect.min.y) / cell_size).floor() as usize)
                .min(rows.saturating_sub(1));
            let last_row = (((maximum_y - rect.min.y) / cell_size).floor() as usize)
                .min(rows.saturating_sub(1));
            for row in first_row..=last_row {
                for column in first_column..=last_column {
                    let sample = Pos2::new(
                        rect.min.x + (column as f32 + 0.5) * cell_size,
                        rect.min.y + (row as f32 + 0.5) * cell_size,
                    );
                    let Some(weights) = screen_barycentric(sample, occluder.points) else {
                        continue;
                    };
                    let depth = perspective_correct_depth(weights, occluder.depths, perspective);
                    if depth.is_finite() && (!perspective || depth > 0.0) {
                        let index = row * columns + column;
                        buffer.depths[index] = buffer.depths[index].min(depth);
                    }
                }
            }
        }
        buffer
    }

    fn depth_at(&self, point: Pos2) -> Option<f64> {
        if !self.rect.contains(point) {
            return None;
        }
        let column = (((point.x - self.rect.min.x) / self.cell_size).floor() as usize)
            .min(self.columns.saturating_sub(1));
        let row = (((point.y - self.rect.min.y) / self.cell_size).floor() as usize)
            .min(self.rows.saturating_sub(1));
        self.depths[row * self.columns + column]
            .is_finite()
            .then(|| self.depths[row * self.columns + column])
    }
}

struct ShadowMap {
    light_direction: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    min_u: f64,
    min_v: f64,
    span_u: f64,
    span_v: f64,
    depth_bias: f64,
    depths: Vec<f64>,
}

impl ShadowMap {
    fn build(triangles: &[WorldTriangle]) -> Option<Self> {
        if triangles.is_empty() {
            return None;
        }
        let light_direction = shadow_light_direction();
        let reference = if light_direction.y.abs() < 0.95 {
            Vec3::Y
        } else {
            Vec3::X
        };
        let axis_u = reference.cross(light_direction).try_normalized(1.0e-12)?;
        let axis_v = light_direction.cross(axis_u).try_normalized(1.0e-12)?;
        let projected = triangles
            .iter()
            .flat_map(|triangle| triangle.points)
            .map(|point| (point.dot(axis_u), point.dot(axis_v)))
            .collect::<Vec<_>>();
        let (mut min_u, mut max_u, mut min_v, mut max_v) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for (u, v) in projected {
            min_u = min_u.min(u);
            max_u = max_u.max(u);
            min_v = min_v.min(v);
            max_v = max_v.max(v);
        }
        let padding = ((max_u - min_u).max(max_v - min_v) * 0.02).max(1.0e-4);
        min_u -= padding;
        max_u += padding;
        min_v -= padding;
        max_v += padding;
        let span_u = (max_u - min_u).max(1.0e-4);
        let span_v = (max_v - min_v).max(1.0e-4);
        let cell_world_size = span_u.max(span_v) / SHADOW_MAP_RESOLUTION as f64;
        let mut map = Self {
            light_direction,
            axis_u,
            axis_v,
            min_u,
            min_v,
            span_u,
            span_v,
            depth_bias: cell_world_size * 0.6,
            depths: vec![f64::NEG_INFINITY; SHADOW_MAP_RESOLUTION * SHADOW_MAP_RESOLUTION],
        };
        for triangle in triangles {
            map.rasterize(triangle.points);
        }
        Some(map)
    }

    fn screen_point(&self, point: Vec3) -> [f64; 2] {
        [
            (point.dot(self.axis_u) - self.min_u) / self.span_u
                * (SHADOW_MAP_RESOLUTION - 1) as f64,
            (point.dot(self.axis_v) - self.min_v) / self.span_v
                * (SHADOW_MAP_RESOLUTION - 1) as f64,
        ]
    }

    fn rasterize(&mut self, points: [Vec3; 3]) {
        let screen = points.map(|point| self.screen_point(point));
        let min_x = screen
            .iter()
            .map(|point| point[0].floor() as i32)
            .min()
            .unwrap_or(0)
            .clamp(0, SHADOW_MAP_RESOLUTION as i32 - 1);
        let max_x = screen
            .iter()
            .map(|point| point[0].ceil() as i32)
            .max()
            .unwrap_or(0)
            .clamp(0, SHADOW_MAP_RESOLUTION as i32 - 1);
        let min_y = screen
            .iter()
            .map(|point| point[1].floor() as i32)
            .min()
            .unwrap_or(0)
            .clamp(0, SHADOW_MAP_RESOLUTION as i32 - 1);
        let max_y = screen
            .iter()
            .map(|point| point[1].ceil() as i32)
            .max()
            .unwrap_or(0)
            .clamp(0, SHADOW_MAP_RESOLUTION as i32 - 1);
        let denominator = (screen[1][1] - screen[2][1]) * (screen[0][0] - screen[2][0])
            + (screen[2][0] - screen[1][0]) * (screen[0][1] - screen[2][1]);
        if denominator.abs() <= 1.0e-12 {
            return;
        }
        let depths = points.map(|point| point.dot(self.light_direction));
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let sample = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let first = ((screen[1][1] - screen[2][1]) * (sample[0] - screen[2][0])
                    + (screen[2][0] - screen[1][0]) * (sample[1] - screen[2][1]))
                    / denominator;
                let second = ((screen[2][1] - screen[0][1]) * (sample[0] - screen[2][0])
                    + (screen[0][0] - screen[2][0]) * (sample[1] - screen[2][1]))
                    / denominator;
                let third = 1.0 - first - second;
                if first < -1.0e-8 || second < -1.0e-8 || third < -1.0e-8 {
                    continue;
                }
                let depth = first * depths[0] + second * depths[1] + third * depths[2];
                let index = y as usize * SHADOW_MAP_RESOLUTION + x as usize;
                self.depths[index] = self.depths[index].max(depth);
            }
        }
    }

    fn shadow_coverage(&self, point: Vec3, normal: Vec3) -> f32 {
        let screen = self.screen_point(point);
        if screen.iter().any(|coordinate| {
            !coordinate.is_finite()
                || *coordinate < 0.0
                || *coordinate >= SHADOW_MAP_RESOLUTION as f64
        }) {
            return 0.0;
        }
        let receiver_depth = point.dot(self.light_direction);
        let grazing_bias = self.depth_bias
            + (1.0 - normal.dot(self.light_direction).abs().clamp(0.0, 1.0))
                * self.depth_bias
                * 2.0;
        let center = [screen[0].round() as i32, screen[1].round() as i32];
        let mut occluded = 0_u32;
        let mut samples = 0_u32;
        for offset_y in -1..=1 {
            for offset_x in -1..=1 {
                let x = center[0] + offset_x;
                let y = center[1] + offset_y;
                if !(0..SHADOW_MAP_RESOLUTION as i32).contains(&x)
                    || !(0..SHADOW_MAP_RESOLUTION as i32).contains(&y)
                {
                    continue;
                }
                samples += 1;
                let depth = self.depths[y as usize * SHADOW_MAP_RESOLUTION + x as usize];
                if depth.is_finite() && depth > receiver_depth + grazing_bias {
                    occluded += 1;
                }
            }
        }
        if samples == 0 {
            0.0
        } else {
            occluded as f32 / samples as f32
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkthroughTuning {
    pub move_speed: f64,
    pub jump_velocity: f64,
    pub gravity: f64,
    pub height_adjust_speed: f64,
}

impl Default for WalkthroughTuning {
    fn default() -> Self {
        Self {
            move_speed: 24.0,
            jump_velocity: 16.0,
            gravity: 48.0,
            height_adjust_speed: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WalkthroughCamera {
    position: Vec3,
    yaw: f64,
    pitch: f64,
    eye_offset: f64,
    jump_offset: f64,
    vertical_velocity: f64,
    jump_queued: bool,
    grounded: bool,
}

impl WalkthroughCamera {
    fn forward(self) -> Vec3 {
        let horizontal = self.pitch.cos();
        Vec3::new(
            horizontal * self.yaw.cos(),
            self.pitch.sin(),
            horizontal * self.yaw.sin(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CuboidPreview {
    pub origin: Vec3,
    pub dimensions: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolPreview {
    Point {
        position: Vec3,
    },
    Cuboid(CuboidPreview),
    Line {
        start: Vec3,
        end: Vec3,
    },
    PlanarShape {
        outline: Vec<Vec3>,
        closed: bool,
        measurements: Vec<ShapeMeasurement>,
    },
    Polyline {
        outline: Vec<Vec3>,
        closed: bool,
    },
    Transform {
        edges: Vec<[Vec3; 2]>,
    },
    Placement {
        edges: std::sync::Arc<Vec<[Vec3; 2]>>,
        transform: Affine3,
    },
    Array {
        edges: Vec<[Vec3; 2]>,
        copies: usize,
        displacement: Vec3,
    },
    ArrayOffsets {
        edges: Vec<[Vec3; 2]>,
        offsets: Vec<Vec3>,
        capped: bool,
    },
    ArrayTransforms {
        edges: Vec<[Vec3; 2]>,
        transforms: Vec<Affine3>,
        capped: bool,
    },
    LineOffset {
        edges: Vec<[Vec3; 2]>,
        target: Vec3,
        signed_offset: Option<f64>,
    },
    PushPull {
        edges: Vec<[Vec3; 2]>,
        triangles: Vec<([Vec3; 3], meshcraft_core::document::Rgba8)>,
    },
    Primitive {
        edges: Vec<[Vec3; 2]>,
        triangles: Vec<([Vec3; 3], meshcraft_core::document::Rgba8)>,
    },
    Paint {
        edges: Vec<[Vec3; 2]>,
        triangles: Vec<[Vec3; 3]>,
        color: meshcraft_core::document::Rgba8,
    },
}

#[derive(Clone, Debug)]
pub struct ToolFeedback {
    pub screen_position: Pos2,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrthoView {
    Top,
    Bottom,
    Left,
    Right,
    Front,
    Back,
}

impl OrthoView {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
            Self::Front => "front",
            Self::Back => "back",
        }
    }

    pub(crate) fn screen_axes(self) -> (Vec3, Vec3, Vec3) {
        match self {
            Self::Top => (
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            Self::Bottom => (
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, -1.0, 0.0),
            ),
            Self::Left => (
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
            ),
            Self::Right => (
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            ),
            Self::Front => (
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            Self::Back => (
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ViewportInput {
    pub rect: Rect,
    pub response: egui::Response,
    pub click: Option<ViewportClick>,
    pub selection_gesture: Option<SelectionGesture>,
    pub pointer_local: Option<Vec2>,
}

#[derive(Clone, Copy, Debug)]
pub struct ViewportClick {
    pub position: Pos2,
    pub modifiers: egui::Modifiers,
    pub time_millis: u64,
    pub pixels_per_point: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionGesture {
    pub start: Pos2,
    pub end: Pos2,
    pub modifiers: egui::Modifiers,
    pub pixels_per_point: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapKind {
    Free,
    Face,
    Line,
    Grid,
    GridLine,
    GridGuide,
    AxisGuide,
    Endpoint,
    Midpoint,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Face => "Face",
            Self::Line => "Line",
            Self::Grid => "Grid",
            Self::GridLine => "Grid Line",
            Self::GridGuide => "Grid Guide",
            Self::AxisGuide => "Axis Guide",
            Self::Endpoint => "Endpoint",
            Self::Midpoint => "Midpoint",
        }
    }

    fn priority(self) -> u8 {
        match self {
            Self::Free => 0,
            Self::Face => 1,
            Self::Line | Self::GridLine => 2,
            Self::Grid | Self::GridGuide | Self::AxisGuide => 3,
            Self::Endpoint | Self::Midpoint => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapResult {
    pub point: Vec3,
    pub normal: Vec3,
    pub kind: SnapKind,
    pub screen_distance: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapVisibility {
    pub objects_visible: bool,
    pub edges_visible: bool,
    pub faces_visible: bool,
    pub faces_wireframe: bool,
}

impl Default for SnapVisibility {
    fn default() -> Self {
        Self {
            objects_visible: true,
            edges_visible: true,
            faces_visible: true,
            faces_wireframe: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SnapScope<'a> {
    Document,
    Occurrence(&'a OccurrenceRef),
    Definition(DefinitionId),
}

impl SnapScope<'_> {
    fn contains(self, document: &Document, occurrence: &OccurrenceRef) -> bool {
        match self {
            Self::Document => true,
            Self::Occurrence(scope) => occurrence.path.starts_with(&scope.path),
            Self::Definition(definition_id) if occurrence.is_scene_root() => {
                definition_id == Document::SCENE_ROOT_DEFINITION_ID
            }
            Self::Definition(definition_id) => (1..=occurrence.path.len()).any(|length| {
                let prefix = OccurrenceRef {
                    path: occurrence.path[..length].to_vec(),
                };
                document
                    .resolve_occurrence(&prefix)
                    .is_some_and(|resolved| resolved.definition.id == definition_id)
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SnapOptions<'a> {
    pub snap_radius: f32,
    pub grid_spacing: f64,
    pub guides: &'a [ConstructionGuide],
    pub scope: SnapScope<'a>,
    pub visibility: SnapVisibility,
}

#[derive(Clone, Copy, Debug)]
struct SnapCandidate {
    point: Vec3,
    normal: Vec3,
    kind: SnapKind,
    screen_distance: f32,
    depth: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AdaptiveGridCloud {
    center: Vec3,
    normal: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    spacing: f64,
    half_extent: f64,
}

const ADAPTIVE_GRID_RADIUS_UNITS: i32 = 7;

impl AdaptiveGridCloud {
    fn at_point(point: Vec3, normal: Vec3, spacing: f64) -> Option<Self> {
        let normal = normal.try_normalized(1.0e-12)?;
        let reference = if normal.y.abs() < 0.95 {
            Vec3::Y
        } else {
            Vec3::X
        };
        let axis_u = (reference.cross(normal))
            .try_normalized(1.0e-12)
            .or_else(|| Vec3::Z.cross(normal).try_normalized(1.0e-12))?;
        let axis_v = normal.cross(axis_u).try_normalized(1.0e-12)?;
        let plane_origin = normal * point.dot(normal);
        let local = point - plane_origin;
        let snapped_u = (local.dot(axis_u) / spacing).round() * spacing;
        let snapped_v = (local.dot(axis_v) / spacing).round() * spacing;
        Some(Self {
            center: plane_origin + axis_u * snapped_u + axis_v * snapped_v,
            normal,
            axis_u,
            axis_v,
            spacing,
            half_extent: f64::from(ADAPTIVE_GRID_RADIUS_UNITS) * spacing,
        })
    }

    fn should_reanchor(self, point: Vec3, normal: Vec3, spacing: f64) -> bool {
        if (self.spacing - spacing).abs() > f64::EPSILON {
            return true;
        }
        if self.normal.dot(normal) < 0.98 {
            return true;
        }
        let local = point - self.center;
        if local.dot(self.normal).abs() > spacing * 0.35 {
            return true;
        }
        local.dot(self.axis_u).abs() > self.half_extent
            || local.dot(self.axis_v).abs() > self.half_extent
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructionGuideKind {
    Grid,
    Axis,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstructionGuide {
    pub kind: ConstructionGuideKind,
    pub origin: Vec3,
    pub axis_u: Vec3,
    pub axis_v: Vec3,
    pub axis_w: Vec3,
}

impl ConstructionGuide {
    pub fn at_point(kind: ConstructionGuideKind, origin: Vec3, normal: Vec3) -> Option<Self> {
        let axis_w = normal.try_normalized(1.0e-12)?;
        let reference = if axis_w.y.abs() < 0.95 {
            Vec3::Y
        } else {
            Vec3::Z
        };
        let axis_v = (reference - axis_w * reference.dot(axis_w)).try_normalized(1.0e-12)?;
        let axis_u = axis_v.cross(axis_w).try_normalized(1.0e-12)?;
        Some(Self {
            kind,
            origin,
            axis_u,
            axis_v,
            axis_w,
        })
    }

    fn planes(self) -> [(Vec3, Vec3, Vec3); 3] {
        [
            (self.axis_w, self.axis_u, self.axis_v),
            (self.axis_v, self.axis_u, self.axis_w),
            (self.axis_u, self.axis_v, self.axis_w),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridStyle {
    pub visible: bool,
    pub spacing: f64,
    pub extent: u32,
    pub normal_axis: GridNormalAxis,
    pub normal_width: f32,
    pub thick_width: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GridNormalAxis {
    X,
    #[default]
    Y,
    Z,
}

impl GridNormalAxis {
    pub fn id(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X => "X (right)",
            Self::Y => "Y (up, default)",
            Self::Z => "Z (back)",
        }
    }

    pub(crate) fn vector(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }
}

pub struct ViewportScene<'a> {
    pub size: Vec2,
    pub document: &'a Document,
    pub selection: &'a [SelectionTarget],
    pub scope: SnapScope<'a>,
    pub display_policy: &'a SelectionDisplayPolicy,
    pub active_tool: &'a str,
    pub preview: Option<ToolPreview>,
    pub feedback: Option<ToolFeedback>,
    pub snap: Option<SnapResult>,
    pub guides: &'a [ConstructionGuide],
    pub guide_preview: Option<ConstructionGuide>,
    pub grid: GridStyle,
}

pub struct Viewport {
    zoom: f32,
    pan: Vec2,
    yaw: f64,
    elevation: f64,
    target: Vec3,
    distance: f64,
    field_of_view: f64,
    near_plane: f64,
    far_plane: f64,
    orthographic: Option<OrthoView>,
    grid_normal_axis: GridNormalAxis,
    walkthrough: Option<WalkthroughCamera>,
    walkthrough_tuning: WalkthroughTuning,
    camera_drag: Option<egui::PointerButton>,
    selection_drag_origin: Option<Pos2>,
    frame_pending: bool,
    black_model_lines: bool,
    adaptive_grid_cloud: Option<AdaptiveGridCloud>,
    derived: DerivedIndexCache,
    #[cfg(not(target_arch = "wasm32"))]
    native_pick_index: Option<NativeSpatialIndex>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 28.0,
            pan: Vec2::ZERO,
            yaw: 0.0,
            elevation: DEFAULT_ELEVATION,
            target: Vec3::ZERO,
            distance: DEFAULT_PERSPECTIVE_DISTANCE,
            field_of_view: DEFAULT_FIELD_OF_VIEW,
            near_plane: DEFAULT_NEAR_PLANE,
            far_plane: DEFAULT_FAR_PLANE,
            orthographic: None,
            grid_normal_axis: GridNormalAxis::default(),
            walkthrough: None,
            walkthrough_tuning: WalkthroughTuning::default(),
            camera_drag: None,
            selection_drag_origin: None,
            frame_pending: false,
            black_model_lines: true,
            adaptive_grid_cloud: None,
            derived: DerivedIndexCache::default(),
            #[cfg(not(target_arch = "wasm32"))]
            native_pick_index: None,
        }
    }
}

impl Viewport {
    pub(crate) fn set_black_model_lines(&mut self, black: bool) {
        self.black_model_lines = black;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn invalidate_document_caches(&mut self) {
        self.derived = DerivedIndexCache::default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.native_pick_index = None;
        }
    }

    pub fn set_grid_normal_axis(&mut self, axis: GridNormalAxis) {
        self.grid_normal_axis = axis;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, scene: &ViewportScene<'_>) -> ViewportInput {
        let (response, painter) = ui.allocate_painter(scene.size, Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(200, 222, 244));

        if std::mem::take(&mut self.frame_pending) {
            self.frame_document(rect, scene.document);
        }

        let (pointer_position, press_origin, primary_pressed, primary_released, modifiers) = ui
            .input(|input| {
                (
                    input.pointer.interact_pos(),
                    input.pointer.press_origin(),
                    input.pointer.button_pressed(egui::PointerButton::Primary),
                    input.pointer.button_released(egui::PointerButton::Primary),
                    input.modifiers,
                )
            });
        if scene.active_tool == "Select" {
            if primary_pressed {
                self.selection_drag_origin = press_origin
                    .or(pointer_position)
                    .filter(|position| rect.contains(*position));
            }
        } else {
            self.selection_drag_origin = None;
        }
        let drag_origin = self.selection_drag_origin.or(press_origin);
        let pointer_over_viewport = pointer_position.is_some_and(|position| {
            rect.contains(position) && ui.ctx().layer_id_at(position) == Some(ui.layer_id())
        });
        if pointer_over_viewport {
            let modifiers = ui.input(|input| input.modifiers);
            let scroll = ui.input(|input| {
                let delta = input.smooth_scroll_delta;
                if modifiers.shift {
                    delta.x + delta.y
                } else {
                    delta.y
                }
            }) * if modifiers.alt {
                0.05
            } else if modifiers.ctrl {
                0.1
            } else {
                1.0
            };
            if scroll.abs() > f32::EPSILON && !ui.ctx().egui_wants_keyboard_input() {
                if modifiers.shift && self.walkthrough.is_some() {
                    self.travel_walkthrough(
                        scene.document,
                        -self.camera_direction() * (self.distance * f64::from(scroll) * 0.001),
                    );
                } else if let Some(walkthrough) = &mut self.walkthrough {
                    let scroll_steps = f64::from(scroll) / 40.0;
                    walkthrough.position += walkthrough.forward()
                        * (scroll_steps
                            * self.walkthrough_tuning.move_speed
                            * WALK_SCROLL_MOVE_SCALE);
                } else if modifiers.shift {
                    self.target -=
                        self.camera_direction() * (self.distance * f64::from(scroll) * 0.001);
                } else {
                    self.zoom_at(
                        rect,
                        pointer_position.expect("viewport pointer was checked"),
                        scroll,
                    );
                }
            }
            if self.camera_drag.is_none() {
                self.camera_drag = ui.input(|input| {
                    let pointer = &input.pointer;
                    if pointer.button_pressed(egui::PointerButton::Secondary) {
                        Some(egui::PointerButton::Secondary)
                    } else if pointer.button_pressed(egui::PointerButton::Middle) {
                        Some(egui::PointerButton::Middle)
                    } else if scene.active_tool == "Orbit"
                        && pointer.button_pressed(egui::PointerButton::Primary)
                    {
                        Some(egui::PointerButton::Primary)
                    } else {
                        None
                    }
                });
            }
        }
        if let Some(button) = self.camera_drag {
            let (button_down, shift_held, pointer_delta) = ui.input(|input| {
                (
                    input.pointer.button_down(button),
                    input.modifiers.shift,
                    input.pointer.delta(),
                )
            });
            if button_down {
                if self.walkthrough.is_some() {
                    self.drag_walkthrough(button, shift_held, pointer_delta);
                } else {
                    self.drag_camera(
                        rect,
                        button,
                        shift_held,
                        pointer_delta,
                        scene.active_tool == "Orbit",
                    );
                }
            } else {
                self.camera_drag = None;
            }
        }

        if self.walkthrough.is_some() {
            let keyboard_allowed = pointer_over_viewport && !ui.ctx().egui_wants_keyboard_input();
            let (forward, backward, left, right, raise, lower, jump, delta) = ui.input(|input| {
                (
                    keyboard_allowed && input.key_down(egui::Key::W),
                    keyboard_allowed && input.key_down(egui::Key::S),
                    keyboard_allowed
                        && (input.key_down(egui::Key::A) || input.key_down(egui::Key::ArrowLeft)),
                    keyboard_allowed
                        && (input.key_down(egui::Key::D) || input.key_down(egui::Key::ArrowRight)),
                    keyboard_allowed && input.key_down(egui::Key::ArrowUp),
                    keyboard_allowed && input.key_down(egui::Key::ArrowDown),
                    keyboard_allowed && input.key_pressed(egui::Key::Space),
                    f64::from(input.stable_dt.clamp(0.0, 0.1)),
                )
            });
            self.update_walkthrough(
                scene.document,
                delta,
                forward,
                backward,
                left,
                right,
                raise,
                lower,
                jump,
            );
            if forward
                || backward
                || left
                || right
                || raise
                || lower
                || self
                    .walkthrough
                    .is_some_and(|walkthrough| !walkthrough.grounded)
            {
                ui.ctx().request_repaint();
            }
        }

        let world_triangles =
            collect_world_triangles(scene.document, scene.scope, scene.display_policy);
        let shadow_map = ShadowMap::build(&world_triangles);
        self.draw_ground_shadows(&painter, rect, &world_triangles, shadow_map.as_ref());
        self.update_adaptive_grid(scene.snap, scene.grid, scene.guides);
        self.draw_grid(&painter, rect, scene.grid);
        self.draw_document(&painter, rect, scene, shadow_map.as_ref());
        self.draw_guides(&painter, rect, scene.guides, scene.grid, false);
        if let Some(cloud) = self.adaptive_grid_cloud {
            self.draw_adaptive_grid_cloud(&painter, rect, cloud, scene.grid.spacing);
        }
        if let Some(guide) = scene.guide_preview {
            self.draw_guides(
                &painter,
                rect,
                std::slice::from_ref(&guide),
                scene.grid,
                true,
            );
        }
        if let Some(preview) = scene.preview.as_ref() {
            match preview {
                ToolPreview::Point { position } => {
                    self.draw_point_preview(&painter, rect, *position)
                }
                ToolPreview::Cuboid(cuboid) => self.draw_cuboid_preview(&painter, rect, *cuboid),
                ToolPreview::Line { start, end } => {
                    self.draw_line_preview(&painter, rect, *start, *end)
                }
                ToolPreview::PlanarShape {
                    outline,
                    closed,
                    measurements,
                } => {
                    self.draw_planar_shape_preview(&painter, rect, outline, *closed, measurements);
                }
                ToolPreview::Polyline { outline, closed } => {
                    self.draw_polyline_preview(&painter, rect, outline, *closed);
                }
                ToolPreview::Placement { edges, transform } => {
                    let edges = edges
                        .iter()
                        .map(|edge| edge.map(|p| transform.transform_point(p)))
                        .collect::<Vec<_>>();
                    self.draw_transform_preview(&painter, rect, &edges);
                }
                ToolPreview::Transform { edges } => {
                    self.draw_transform_preview(&painter, rect, edges);
                }
                ToolPreview::Array {
                    edges,
                    copies,
                    displacement,
                } => {
                    for index in 1..=*copies {
                        let translation = *displacement * index as f64;
                        self.draw_translated_transform_preview(&painter, rect, edges, translation);
                    }
                }
                ToolPreview::ArrayOffsets { edges, offsets, .. } => {
                    for translation in offsets {
                        self.draw_translated_transform_preview(&painter, rect, edges, *translation);
                    }
                }
                ToolPreview::ArrayTransforms {
                    edges, transforms, ..
                } => {
                    for transform in transforms {
                        self.draw_affine_transform_preview(&painter, rect, edges, *transform);
                    }
                }
                ToolPreview::LineOffset {
                    edges,
                    target,
                    signed_offset,
                    ..
                } => {
                    self.draw_transform_preview(&painter, rect, edges);
                    if let Some(offset) = signed_offset.filter(|offset| offset.is_finite())
                        && self.point_in_front_of_camera(*target)
                    {
                        painter.text(
                            self.world_to_screen(rect, *target) + Vec2::new(10.0, 12.0),
                            egui::Align2::LEFT_TOP,
                            format!("Offset {offset:.2}"),
                            egui::FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                    }
                }
                ToolPreview::PushPull { edges, triangles } => {
                    self.draw_push_pull_preview(&painter, rect, edges, triangles);
                }
                ToolPreview::Primitive { edges, triangles } => {
                    self.draw_push_pull_preview(&painter, rect, edges, triangles);
                }
                ToolPreview::Paint {
                    edges,
                    triangles,
                    color,
                } => self.draw_paint_preview(&painter, rect, edges, triangles, *color),
            }
        }

        if let Some(feedback) = scene.feedback.as_ref() {
            let font = egui::FontId::proportional(12.0);
            let text_color = Color32::WHITE;
            let galley = painter.layout_no_wrap(feedback.text.clone(), font, text_color);
            let desired = feedback.screen_position + Vec2::new(14.0, 14.0);
            let min_x = rect.left() + 4.0;
            let min_y = rect.top() + 4.0;
            let max_x = (rect.right() - galley.size().x - 4.0).max(min_x);
            let max_y = (rect.bottom() - galley.size().y - 4.0).max(min_y);
            let position = Pos2::new(desired.x.clamp(min_x, max_x), desired.y.clamp(min_y, max_y));
            let background = Rect::from_min_size(
                position - Vec2::new(4.0, 3.0),
                galley.size() + Vec2::new(8.0, 6.0),
            );
            painter.rect_filled(background, 2.0, Color32::from_black_alpha(210));
            painter.galley(position, galley, text_color);
        }

        if scene.active_tool == "Select"
            && (response.dragged_by(egui::PointerButton::Primary)
                || response.drag_stopped_by(egui::PointerButton::Primary))
        {
            let (origin, end) = (drag_origin, pointer_position);
            if let Some((origin, end)) = origin.zip(end) {
                let region = Rect::from_two_pos(origin, end);
                if region.width() >= 2.0 || region.height() >= 2.0 {
                    let crossing = end.x < origin.x && !modifiers.alt;
                    let (fill, stroke, label) = if crossing {
                        (
                            Color32::from_rgba_unmultiplied(42, 190, 126, 38),
                            Color32::from_rgb(32, 164, 105),
                            "Crossing",
                        )
                    } else {
                        (
                            Color32::from_rgba_unmultiplied(53, 132, 238, 36),
                            Color32::from_rgb(37, 111, 220),
                            "Window",
                        )
                    };
                    let marquee_painter = painter.with_clip_rect(rect);
                    marquee_painter.rect_filled(region, 0.0, fill);
                    marquee_painter.rect_stroke(
                        region,
                        0.0,
                        Stroke::new(1.5, stroke),
                        egui::StrokeKind::Inside,
                    );
                    marquee_painter.text(
                        region.left_top() + Vec2::new(4.0, 3.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(11.0),
                        stroke,
                    );
                }
            }
        }

        // Keep the construction cursor above the world, feedback and selection-window overlays.
        if let Some(snap) = scene.snap {
            let line_axis = (snap.kind == SnapKind::Line)
                .then(|| snapped_segment_axis(scene.document, scene.scope, snap.point))
                .flatten();
            self.draw_cursor(
                &painter,
                rect,
                snap,
                scene.grid.spacing,
                line_axis,
                self.grid_normal_axis.vector(),
            );
        }

        let pointer = pointer_position.filter(|_| pointer_over_viewport);
        let click =
            if scene.active_tool != "Orbit" && response.clicked_by(egui::PointerButton::Primary) {
                response.interact_pointer_pos().map(|position| {
                    let (modifiers, time) = ui.input(|input| (input.modifiers, input.time));
                    ViewportClick {
                        position,
                        modifiers,
                        time_millis: (time.max(0.0) * 1000.0).min(u64::MAX as f64) as u64,
                        pixels_per_point: ui.ctx().pixels_per_point(),
                    }
                })
            } else {
                None
            };
        let selection_gesture = if scene.active_tool == "Select"
            && response.drag_stopped_by(egui::PointerButton::Primary)
        {
            let gesture = drag_origin
                .zip(pointer_position)
                .map(|(start, end)| SelectionGesture {
                    start,
                    end,
                    modifiers,
                    pixels_per_point: ui.ctx().pixels_per_point(),
                });
            self.selection_drag_origin = None;
            gesture
        } else {
            if primary_released {
                self.selection_drag_origin = None;
            }
            None
        };
        ViewportInput {
            rect,
            response,
            click,
            selection_gesture,
            pointer_local: pointer.map(|position| position - rect.min),
        }
    }

    pub fn screen_to_ground(&self, rect: Rect, position: Pos2) -> Option<Vec3> {
        if let Some(view) = self.orthographic {
            if !matches!(view, OrthoView::Top | OrthoView::Bottom) {
                return None;
            }
            let (right, up, _) = view.screen_axes();
            let screen = Vec2::new(
                (position.x - rect.center().x - self.pan.x) / self.zoom,
                -(position.y - rect.center().y - self.pan.y) / self.zoom,
            );
            let world = self.target + right * f64::from(screen.x) + up * f64::from(screen.y);
            return Some(Vec3::new(world.x, 0.0, world.z));
        }
        let (origin, direction) = self.ray_from_screen(rect, position)?;
        let parameter = -origin.y / direction.y;
        (parameter.is_finite() && parameter >= 0.0).then(|| origin + direction * parameter)
    }

    /// Maps a pointer onto the ground plane or, when edge-on, the active orthographic view plane.
    pub fn screen_to_construction_plane(&self, rect: Rect, position: Pos2) -> Option<Vec3> {
        if let Some(view) = self.orthographic {
            let (right, up, normal) = view.screen_axes();
            let screen = Vec2::new(
                (position.x - rect.center().x - self.pan.x) / self.zoom,
                -(position.y - rect.center().y - self.pan.y) / self.zoom,
            );
            let target_on_plane = self.target - normal * normal.dot(self.target);
            return Some(target_on_plane + right * f64::from(screen.x) + up * f64::from(screen.y));
        }
        let (origin, direction) = self.ray_from_screen(rect, position)?;
        self.ray_plane_intersection(
            origin,
            direction,
            Vec3::ZERO,
            self.grid_normal_axis.vector(),
        )
    }

    pub fn guide_index_at(
        &self,
        rect: Rect,
        position: Pos2,
        guides: &[ConstructionGuide],
        grid_spacing: f64,
        snap_radius: f32,
    ) -> Option<usize> {
        if !grid_spacing.is_finite()
            || grid_spacing <= 0.0
            || !snap_radius.is_finite()
            || snap_radius < 0.0
        {
            return None;
        }
        let extent = grid_spacing * 10.0;
        let radius_squared = snap_radius * snap_radius;
        let mut best: Option<(f32, usize)> = None;
        for (index, guide) in guides.iter().enumerate() {
            let mut consider_segment = |start: Vec3, end: Vec3| {
                let (Some(start), Some(end)) = (
                    self.project_if_visible(rect, start),
                    self.project_if_visible(rect, end),
                ) else {
                    return;
                };
                let distance = distance_to_segment_squared(position, start, end);
                if distance <= radius_squared
                    && best.is_none_or(|(best_distance, _)| distance < best_distance)
                {
                    best = Some((distance, index));
                }
            };
            match guide.kind {
                ConstructionGuideKind::Grid => {
                    for (_, axis_u, axis_v) in guide.planes() {
                        for step in -10..=10 {
                            let offset = f64::from(step) * grid_spacing;
                            consider_segment(
                                guide.origin + axis_u * offset - axis_v * extent,
                                guide.origin + axis_u * offset + axis_v * extent,
                            );
                            consider_segment(
                                guide.origin + axis_v * offset - axis_u * extent,
                                guide.origin + axis_v * offset + axis_u * extent,
                            );
                        }
                    }
                }
                ConstructionGuideKind::Axis => {
                    for axis in [guide.axis_u, guide.axis_v, guide.axis_w] {
                        consider_segment(
                            guide.origin - axis * extent,
                            guide.origin + axis * extent,
                        );
                    }
                }
            }
        }
        best.map(|(_, index)| index)
    }

    /// Snaps a logical-pixel pointer to visible model geometry or the active construction grid.
    /// The caller supplies a radius in egui points, converted from the configured pixel radius.
    #[cfg(test)]
    pub fn snap_point(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        snap_radius: f32,
        grid_spacing: f64,
    ) -> Option<SnapResult> {
        self.snap_point_with_options(
            rect,
            document,
            position,
            SnapOptions {
                snap_radius,
                grid_spacing,
                guides: &[],
                scope: SnapScope::Document,
                visibility: SnapVisibility::default(),
            },
        )
    }

    #[cfg(test)]
    pub fn snap_point_with_guides(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        snap_radius: f32,
        grid_spacing: f64,
        guides: &[ConstructionGuide],
    ) -> Option<SnapResult> {
        self.snap_point_with_options(
            rect,
            document,
            position,
            SnapOptions {
                snap_radius,
                grid_spacing,
                guides,
                scope: SnapScope::Document,
                visibility: SnapVisibility::default(),
            },
        )
    }

    pub fn snap_point_with_options(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        options: SnapOptions<'_>,
    ) -> Option<SnapResult> {
        let SnapOptions {
            snap_radius,
            grid_spacing,
            guides,
            scope,
            visibility,
        } = options;
        let construction_point = self.screen_to_construction_plane(rect, position);
        let ray = self.ray_for_screen(rect, position);
        let mut best = None;
        let radius = snap_radius.max(0.0);
        let mut base_point = construction_point;
        let mut base_normal = self.construction_plane_normal();
        let mut base_depth = f64::INFINITY;

        let nearest_surface_depth = if visibility.faces_visible && !visibility.faces_wireframe {
            ray.and_then(|(ray_origin, ray_direction)| {
                let mut nearest = None;
                for occurrence in document.scene_geometry_refs().ok()? {
                    if !scope.contains(document, &occurrence)
                        || (!occurrence.is_scene_root() && !visibility.objects_visible)
                        || !self.occurrence_bounds_near_pointer(
                            rect,
                            document,
                            &occurrence,
                            position,
                            radius,
                        )
                    {
                        continue;
                    }
                    let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                        continue;
                    };
                    let Some(geometry) = document.effective_geometry(&occurrence) else {
                        continue;
                    };
                    for triangle in geometry.triangles.values() {
                        let [Some(a), Some(b), Some(c)] = triangle
                            .vertices
                            .map(|reference| geometry.vertex_position(reference))
                        else {
                            continue;
                        };
                        let Some(depth) = ray_triangle_distance(
                            ray_origin,
                            ray_direction,
                            resolved.world_transform.transform_point(a),
                            resolved.world_transform.transform_point(b),
                            resolved.world_transform.transform_point(c),
                        ) else {
                            continue;
                        };
                        nearest = Some(nearest.map_or(depth, |current: f64| current.min(depth)));
                    }
                }
                nearest
            })
        } else {
            None
        };

        if let (Some((ray_origin, _ray_direction)), Ok(occurrences)) =
            (ray, document.scene_geometry_refs())
        {
            for occurrence in occurrences {
                if !scope.contains(document, &occurrence)
                    || (!occurrence.is_scene_root() && !visibility.objects_visible)
                {
                    continue;
                }
                if !self.occurrence_bounds_near_pointer(
                    rect,
                    document,
                    &occurrence,
                    position,
                    radius,
                ) {
                    continue;
                }

                let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                    continue;
                };
                let Some(geometry) = document.effective_geometry(&occurrence) else {
                    continue;
                };

                for segment in geometry
                    .segments
                    .values()
                    .filter(|_| visibility.edges_visible)
                {
                    let (Some(start_local), Some(end_local)) = (
                        geometry.vertex_position(segment.endpoints[0]),
                        geometry.vertex_position(segment.endpoints[1]),
                    ) else {
                        continue;
                    };
                    let start = resolved.world_transform.transform_point(start_local);
                    let end = resolved.world_transform.transform_point(end_local);
                    let start_screen = self.project_if_visible(rect, start);
                    let end_screen = self.project_if_visible(rect, end);

                    for (point, screen) in [(start, start_screen), (end, end_screen)] {
                        if let Some(screen) = screen {
                            let distance = screen.distance(position);
                            self.consider_visible_model_snap(
                                &mut best,
                                SnapCandidate {
                                    point,
                                    normal: self.construction_plane_normal(),
                                    kind: SnapKind::Endpoint,
                                    screen_distance: distance,
                                    depth: point.distance(ray_origin),
                                },
                                radius,
                                grid_spacing,
                                nearest_surface_depth,
                            );
                        }
                    }

                    let midpoint = (start + end) * 0.5;
                    if let Some(screen) = self.project_if_visible(rect, midpoint) {
                        let distance = screen.distance(position);
                        self.consider_visible_model_snap(
                            &mut best,
                            SnapCandidate {
                                point: midpoint,
                                normal: self.construction_plane_normal(),
                                kind: SnapKind::Midpoint,
                                screen_distance: distance,
                                depth: midpoint.distance(ray_origin),
                            },
                            radius,
                            grid_spacing,
                            nearest_surface_depth,
                        );
                    }

                    if let (Some(a), Some(b)) = (start_screen, end_screen) {
                        let screen_direction = b - a;
                        let length_squared = screen_direction.length_sq();
                        let fraction = if length_squared <= f32::EPSILON {
                            0.0
                        } else {
                            ((position - a).dot(screen_direction) / length_squared).clamp(0.0, 1.0)
                        };
                        let screen_closest = a + screen_direction * fraction;
                        let distance = screen_closest.distance(position);
                        let world_fraction =
                            self.world_fraction_for_screen_fraction(start, end, fraction);
                        let line_point = start + (end - start) * world_fraction;
                        self.consider_visible_model_snap(
                            &mut best,
                            SnapCandidate {
                                point: line_point,
                                normal: self.construction_plane_normal(),
                                kind: SnapKind::Line,
                                screen_distance: distance,
                                depth: line_point.distance(ray_origin),
                            },
                            radius,
                            grid_spacing,
                            nearest_surface_depth,
                        );
                    }
                }

                if visibility.faces_visible
                    && !visibility.faces_wireframe
                    && let Some((ray_origin, ray_direction)) = ray
                {
                    for triangle in geometry.triangles.values() {
                        let [Some(a), Some(b), Some(c)] = triangle
                            .vertices
                            .map(|reference| geometry.vertex_position(reference))
                        else {
                            continue;
                        };
                        let [a, b, c] =
                            [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                        let Some(distance_along_ray) =
                            ray_triangle_distance(ray_origin, ray_direction, a, b, c)
                        else {
                            continue;
                        };
                        let point = ray_origin + ray_direction * distance_along_ray;
                        let triangle_normal = (b - a)
                            .cross(c - a)
                            .try_normalized(1.0e-12)
                            .unwrap_or(base_normal);
                        let facing_normal = if triangle_normal.dot(ray_direction) > 0.0 {
                            -triangle_normal
                        } else {
                            triangle_normal
                        };
                        if distance_along_ray < base_depth {
                            base_normal = facing_normal;
                            base_point = Some(point);
                            base_depth = distance_along_ray;
                        }
                        let Some(screen) = self.project_if_visible(rect, point) else {
                            continue;
                        };
                        self.consider_visible_model_snap(
                            &mut best,
                            SnapCandidate {
                                point,
                                normal: facing_normal,
                                kind: SnapKind::Face,
                                screen_distance: screen.distance(position),
                                depth: distance_along_ray,
                            },
                            radius,
                            grid_spacing,
                            nearest_surface_depth,
                        );
                        self.consider_face_edge_snaps(
                            &mut best,
                            rect,
                            position,
                            ray_origin,
                            point,
                            facing_normal,
                            [a, b, c],
                            radius,
                            grid_spacing,
                            nearest_surface_depth,
                        );
                    }
                }
            }
        }

        if grid_spacing.is_finite()
            && grid_spacing > 0.0
            && let Some(point) = base_point
            && let Some((origin, axis_u, axis_v)) = self.grid_basis_at(point, base_normal)
        {
            let relative = point - origin;
            let u = (relative.dot(axis_u) / grid_spacing).round() * grid_spacing;
            let v = (relative.dot(axis_v) / grid_spacing).round() * grid_spacing;
            let grid_point = origin + axis_u * u + axis_v * v;
            if let Some(screen) = self.project_if_visible(rect, grid_point) {
                let distance = screen.distance(position);
                let depth = ray.map_or(0.0, |(ray_origin, _)| grid_point.distance(ray_origin));
                self.consider_snap(
                    &mut best,
                    SnapCandidate {
                        point: grid_point,
                        normal: base_normal,
                        kind: SnapKind::Grid,
                        screen_distance: distance,
                        depth,
                    },
                    radius,
                    grid_spacing,
                );
            }

            let relative = point - origin;
            let u = relative.dot(axis_u);
            let v = relative.dot(axis_v);
            for line_point in [
                origin + axis_u * (u / grid_spacing).round() * grid_spacing + axis_v * v,
                origin + axis_u * u + axis_v * (v / grid_spacing).round() * grid_spacing,
            ] {
                if let Some(screen) = self.project_if_visible(rect, line_point) {
                    let depth = ray.map_or(0.0, |(ray_origin, _)| line_point.distance(ray_origin));
                    self.consider_snap(
                        &mut best,
                        SnapCandidate {
                            point: line_point,
                            normal: base_normal,
                            kind: SnapKind::GridLine,
                            screen_distance: screen.distance(position),
                            depth,
                        },
                        radius,
                        grid_spacing,
                    );
                }
            }
        }

        if let Some((ray_origin, ray_direction)) = ray {
            for guide in guides {
                match guide.kind {
                    ConstructionGuideKind::Grid => {
                        for (normal, axis_u, axis_v) in guide.planes() {
                            let Some(hit) = self.ray_plane_intersection(
                                ray_origin,
                                ray_direction,
                                guide.origin,
                                normal,
                            ) else {
                                continue;
                            };
                            let relative = hit - guide.origin;
                            let u = relative.dot(axis_u);
                            let v = relative.dot(axis_v);
                            let snapped = guide.origin
                                + axis_u * ((u / grid_spacing).round() * grid_spacing)
                                + axis_v * ((v / grid_spacing).round() * grid_spacing);
                            if let Some(screen) = self.project_if_visible(rect, snapped) {
                                self.consider_snap(
                                    &mut best,
                                    SnapCandidate {
                                        point: snapped,
                                        normal,
                                        kind: SnapKind::GridGuide,
                                        screen_distance: screen.distance(position),
                                        depth: snapped.distance(ray_origin),
                                    },
                                    radius,
                                    grid_spacing,
                                );
                            }
                        }
                    }
                    ConstructionGuideKind::Axis => {
                        let extent = grid_spacing * 10.0;
                        for axis in [guide.axis_u, guide.axis_v, guide.axis_w] {
                            let Some((closest, along)) = closest_point_on_ray_line(
                                ray_origin,
                                ray_direction,
                                guide.origin,
                                axis,
                            ) else {
                                continue;
                            };
                            if along.abs() > extent {
                                continue;
                            }
                            let rounded = (along / grid_spacing).round() * grid_spacing;
                            let point = if rounded.abs() <= extent {
                                guide.origin + axis * rounded
                            } else {
                                closest
                            };
                            if let Some(screen) = self.project_if_visible(rect, point) {
                                self.consider_snap(
                                    &mut best,
                                    SnapCandidate {
                                        point,
                                        normal: base_normal,
                                        kind: SnapKind::AxisGuide,
                                        screen_distance: screen.distance(position),
                                        depth: point.distance(ray_origin),
                                    },
                                    radius,
                                    grid_spacing,
                                );
                            }
                        }
                    }
                }
            }
        }

        best.map(|candidate| SnapResult {
            point: candidate.point,
            normal: candidate.normal,
            kind: candidate.kind,
            screen_distance: candidate.screen_distance,
        })
        .or_else(|| {
            construction_point.map(|point| SnapResult {
                point,
                normal: self.construction_plane_normal(),
                kind: SnapKind::Free,
                screen_distance: 0.0,
            })
        })
    }

    fn occurrence_bounds_near_pointer(
        &mut self,
        rect: Rect,
        document: &Document,
        occurrence: &OccurrenceRef,
        position: Pos2,
        radius: f32,
    ) -> bool {
        let Some(bounds) = self.derived.world_bounds(document, occurrence) else {
            return true;
        };
        let projected = bounds_corners(bounds)
            .into_iter()
            .filter_map(|corner| self.project_if_visible(rect, corner))
            .collect::<Vec<_>>();
        if projected.is_empty() {
            return true;
        }
        let (min_x, max_x) = projected
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |range, point| {
                (range.0.min(point.x), range.1.max(point.x))
            });
        let (min_y, max_y) = projected
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |range, point| {
                (range.0.min(point.y), range.1.max(point.y))
            });
        position.x >= min_x - radius
            && position.x <= max_x + radius
            && position.y >= min_y - radius
            && position.y <= max_y + radius
    }

    fn consider_snap(
        &self,
        best: &mut Option<SnapCandidate>,
        candidate: SnapCandidate,
        radius: f32,
        grid_spacing: f64,
    ) {
        if candidate.screen_distance > radius {
            return;
        }
        let Some(current) = *best else {
            *best = Some(candidate);
            return;
        };
        let depth_tolerance = (grid_spacing * 2.0).max(1.5);
        let priority_difference = candidate.kind.priority().cmp(&current.kind.priority());
        if priority_difference.is_gt() && candidate.depth <= current.depth + depth_tolerance {
            *best = Some(candidate);
            return;
        }
        if priority_difference.is_lt() && current.depth <= candidate.depth + depth_tolerance {
            return;
        }
        if candidate.depth + 0.35 < current.depth {
            *best = Some(candidate);
            return;
        }
        if current.depth + 0.35 < candidate.depth {
            return;
        }
        let candidate_score = f64::from(candidate.screen_distance) + candidate.depth * 0.02;
        let current_score = f64::from(current.screen_distance) + current.depth * 0.02;
        if candidate_score + 1.0e-4 < current_score {
            *best = Some(candidate);
        }
    }

    fn consider_visible_model_snap(
        &self,
        best: &mut Option<SnapCandidate>,
        candidate: SnapCandidate,
        radius: f32,
        grid_spacing: f64,
        nearest_surface_depth: Option<f64>,
    ) {
        if nearest_surface_depth.is_some_and(|surface_depth| {
            candidate.depth > surface_depth + 1.0e-5 * surface_depth.abs().max(1.0)
        }) {
            return;
        }
        self.consider_snap(best, candidate, radius, grid_spacing);
    }

    #[allow(clippy::too_many_arguments)]
    fn consider_face_edge_snaps(
        &self,
        best: &mut Option<SnapCandidate>,
        rect: Rect,
        pointer: Pos2,
        ray_origin: Vec3,
        face_hit: Vec3,
        normal: Vec3,
        vertices: [Vec3; 3],
        radius: f32,
        grid_spacing: f64,
        nearest_surface_depth: Option<f64>,
    ) {
        for [start, end] in [
            [vertices[0], vertices[1]],
            [vertices[1], vertices[2]],
            [vertices[2], vertices[0]],
        ] {
            let midpoint = (start + end) * 0.5;
            if let Some(screen) = self.project_if_visible(rect, midpoint) {
                self.consider_visible_model_snap(
                    best,
                    SnapCandidate {
                        point: midpoint,
                        normal,
                        kind: SnapKind::Midpoint,
                        screen_distance: screen.distance(pointer),
                        depth: midpoint.distance(ray_origin),
                    },
                    radius,
                    grid_spacing,
                    nearest_surface_depth,
                );
            }

            let closest = closest_point_on_segment(face_hit, start, end);
            if let Some(screen) = self.project_if_visible(rect, closest) {
                self.consider_visible_model_snap(
                    best,
                    SnapCandidate {
                        point: closest,
                        normal,
                        kind: SnapKind::Line,
                        screen_distance: screen.distance(pointer),
                        depth: closest.distance(ray_origin),
                    },
                    radius,
                    grid_spacing,
                    nearest_surface_depth,
                );
            }
        }
    }

    pub fn construction_plane_normal(&self) -> Vec3 {
        self.orthographic.map_or_else(
            || self.grid_normal_axis.vector(),
            |view| view.screen_axes().2,
        )
    }

    fn grid_basis_at(&self, point: Vec3, normal: Vec3) -> Option<(Vec3, Vec3, Vec3)> {
        let normal = normal.try_normalized(1.0e-12)?;
        let reference = if normal.y.abs() < 0.95 {
            Vec3::Y
        } else {
            Vec3::X
        };
        let axis_u = reference
            .cross(normal)
            .try_normalized(1.0e-12)
            .or_else(|| Vec3::Z.cross(normal).try_normalized(1.0e-12))?;
        let axis_v = normal.cross(axis_u).try_normalized(1.0e-12)?;
        let origin = normal * normal.dot(point);
        Some((origin, axis_u, axis_v))
    }

    fn world_fraction_for_screen_fraction(
        &self,
        start: Vec3,
        end: Vec3,
        screen_fraction: f32,
    ) -> f64 {
        let fraction = f64::from(screen_fraction);
        if self.orthographic.is_some() {
            return fraction;
        }
        let forward = -self.camera_direction();
        let camera_position = self.camera_position();
        let start_depth = forward.dot(start - camera_position);
        let end_depth = forward.dot(end - camera_position);
        let denominator = end_depth * (1.0 - fraction) + start_depth * fraction;
        if denominator <= 1.0e-12 || !denominator.is_finite() {
            fraction
        } else {
            (fraction * start_depth / denominator).clamp(0.0, 1.0)
        }
    }

    fn ray_for_screen(&self, rect: Rect, position: Pos2) -> Option<(Vec3, Vec3)> {
        if let Some(view) = self.orthographic {
            let point_on_view_plane = self.screen_to_construction_plane(rect, position)?;
            let normal = view.screen_axes().2;
            return Some((point_on_view_plane + normal * self.far_plane, -normal));
        }
        self.ray_from_screen(rect, position)
    }

    pub fn request_frame(&mut self) {
        self.frame_pending = true;
    }

    pub fn reset_camera(&mut self) {
        self.zoom = 28.0;
        self.pan = Vec2::ZERO;
        self.yaw = 0.0;
        self.elevation = DEFAULT_ELEVATION;
        self.target = Vec3::ZERO;
        self.distance = DEFAULT_PERSPECTIVE_DISTANCE;
        self.field_of_view = DEFAULT_FIELD_OF_VIEW;
        self.near_plane = DEFAULT_NEAR_PLANE;
        self.far_plane = DEFAULT_FAR_PLANE;
        self.orthographic = None;
        self.walkthrough = None;
        self.camera_drag = None;
        self.frame_pending = false;
    }

    pub fn set_orthographic_view(&mut self, view: OrthoView) {
        self.leave_walkthrough();
        self.orthographic = Some(view);
        self.camera_drag = None;
        self.frame_pending = true;
    }

    pub fn set_perspective_view(&mut self) {
        self.leave_walkthrough();
        self.orthographic = None;
        self.camera_drag = None;
        self.frame_pending = true;
    }

    pub fn orthographic_view(&self) -> Option<OrthoView> {
        self.orthographic
    }

    pub fn walkthrough_active(&self) -> bool {
        self.walkthrough.is_some()
    }

    pub fn set_walkthrough_tuning(&mut self, tuning: WalkthroughTuning) {
        self.walkthrough_tuning = tuning;
    }

    pub fn set_walkthrough_view(&mut self) {
        if self.walkthrough.is_some() {
            return;
        }
        let position = self.camera_position();
        let eye = self.camera_direction();
        let forward = -eye;
        let horizontal = forward.x.hypot(forward.z);
        let yaw = if horizontal > 1.0e-9 {
            forward.z.atan2(forward.x)
        } else {
            0.0
        };
        let pitch = forward
            .y
            .clamp(-1.0, 1.0)
            .asin()
            .clamp(-89.0_f64.to_radians(), 89.0_f64.to_radians());
        self.orthographic = None;
        self.walkthrough = Some(WalkthroughCamera {
            position,
            yaw,
            pitch,
            eye_offset: 0.0,
            jump_offset: 0.0,
            vertical_velocity: 0.0,
            jump_queued: false,
            grounded: true,
        });
        self.pan = Vec2::ZERO;
        self.camera_drag = None;
        self.frame_pending = false;
    }

    pub(crate) fn native_walkthrough_pose(&mut self, eye: Vec3, forward: Vec3) {
        self.set_walkthrough_view();
        if let Some(walk) = &mut self.walkthrough {
            walk.position = eye;
            walk.yaw = forward.z.atan2(forward.x);
            walk.pitch = forward
                .y
                .clamp(-1., 1.)
                .asin()
                .clamp(-89_f64.to_radians(), 89_f64.to_radians());
        }
    }

    pub(crate) fn native_walkthrough_animating(&self) -> bool {
        self.walkthrough.is_some_and(|walk| !walk.grounded)
    }

    pub(crate) fn native_walkthrough_scroll(&mut self, steps: f32) {
        if let Some(walk) = &mut self.walkthrough {
            walk.position +=
                walk.forward() * (f64::from(steps) * self.walkthrough_tuning.move_speed * 0.1);
        }
    }

    /// Preserve the new eye height on the next ground-following controller tick.
    pub(crate) fn travel_walkthrough(&mut self, document: &Document, delta: Vec3) {
        let Some(mut walk) = self.walkthrough else {
            return;
        };
        walk.position += delta;
        let support =
            self.walk_support_height(document, walk.position.x, walk.position.z, walk.position.y);
        walk.eye_offset = walk.position.y - support - WALK_EYE_HEIGHT - walk.jump_offset;
        self.walkthrough = Some(walk);
    }

    pub(crate) fn native_walkthrough_drag(&mut self, dx: f32, dy: f32, pan: bool) {
        self.drag_walkthrough(egui::PointerButton::Secondary, pan, Vec2::new(dx, dy));
    }

    pub(crate) fn native_walkthrough_step(
        &mut self,
        document: &Document,
        dt: f64,
        keys: [bool; 7],
    ) -> (Vec3, Vec3) {
        self.update_walkthrough(
            document, dt, keys[0], keys[1], keys[2], keys[3], keys[4], keys[5], keys[6],
        );
        (self.camera_position(), -self.camera_direction())
    }

    fn leave_walkthrough(&mut self) {
        let Some(walkthrough) = self.walkthrough.take() else {
            return;
        };
        let forward = walkthrough.forward();
        let eye = -forward;
        let horizontal = eye.x.hypot(eye.z);
        self.yaw = if horizontal > 1.0e-9 {
            eye.z.atan2(eye.x) - std::f64::consts::FRAC_PI_4
        } else {
            0.0
        };
        self.elevation = eye
            .y
            .clamp(-1.0, 1.0)
            .asin()
            .clamp(MIN_ELEVATION, MAX_ELEVATION);
        self.distance = self.distance.clamp(1.0, 1000.0);
        self.target = walkthrough.position + forward * self.distance;
        self.pan = Vec2::ZERO;
    }

    fn drag_walkthrough(&mut self, button: egui::PointerButton, shift_held: bool, delta: Vec2) {
        let Some(walkthrough) = &mut self.walkthrough else {
            return;
        };
        if button == egui::PointerButton::Middle
            || (button == egui::PointerButton::Secondary && shift_held)
        {
            let forward = walkthrough.forward();
            let right = forward
                .cross(Vec3::Y)
                .try_normalized(1.0e-12)
                .unwrap_or(Vec3::X);
            let screen_up = right
                .cross(forward)
                .try_normalized(1.0e-12)
                .unwrap_or(Vec3::Y);
            let movement = right * (-f64::from(delta.x) * WALK_PAN_UNITS_PER_PIXEL)
                + screen_up * (f64::from(delta.y) * WALK_PAN_UNITS_PER_PIXEL);
            walkthrough.position += movement;
            walkthrough.eye_offset = (walkthrough.eye_offset + movement.y).clamp(-5., 60.);
        } else if button == egui::PointerButton::Secondary {
            walkthrough.yaw += f64::from(delta.x) * WALK_LOOK_RADIANS_PER_PIXEL;
            walkthrough.pitch = (walkthrough.pitch
                - f64::from(delta.y) * WALK_LOOK_RADIANS_PER_PIXEL)
                .clamp(-89.0_f64.to_radians(), 89.0_f64.to_radians());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_walkthrough(
        &mut self,
        document: &Document,
        delta: f64,
        moving_forward: bool,
        moving_backward: bool,
        moving_left: bool,
        moving_right: bool,
        raising_height: bool,
        lowering_height: bool,
        jump: bool,
    ) {
        let Some(mut walkthrough) = self.walkthrough else {
            return;
        };
        let delta = delta.clamp(0.0, 0.1);
        let forward = Vec3::new(walkthrough.yaw.cos(), 0.0, walkthrough.yaw.sin());
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let mut movement = Vec3::ZERO;
        if moving_forward {
            movement += forward;
        }
        if moving_backward {
            movement += -forward;
        }
        if moving_right {
            movement += right;
        }
        if moving_left {
            movement += -right;
        }
        if let Some(direction) = movement.try_normalized(1.0e-12) {
            walkthrough.position += direction * (self.walkthrough_tuning.move_speed * delta);
        }
        if raising_height {
            walkthrough.eye_offset = (walkthrough.eye_offset
                + self.walkthrough_tuning.height_adjust_speed * delta)
                .clamp(-5.0, 60.0);
        }
        if lowering_height {
            walkthrough.eye_offset = (walkthrough.eye_offset
                - self.walkthrough_tuning.height_adjust_speed * delta)
                .clamp(-5.0, 60.0);
        }

        let support_y = self.walk_support_height(
            document,
            walkthrough.position.x,
            walkthrough.position.z,
            walkthrough.position.y,
        );
        let standing_y = support_y + WALK_EYE_HEIGHT + walkthrough.eye_offset;
        walkthrough.jump_queued |= jump;
        if walkthrough.jump_queued && walkthrough.grounded {
            walkthrough.grounded = false;
            walkthrough.vertical_velocity = self.walkthrough_tuning.jump_velocity;
            walkthrough.jump_queued = false;
        }
        if !walkthrough.grounded {
            walkthrough.vertical_velocity -= self.walkthrough_tuning.gravity * delta;
            walkthrough.jump_offset += walkthrough.vertical_velocity * delta;
            if walkthrough.jump_offset <= 0.0 {
                walkthrough.jump_offset = 0.0;
                walkthrough.vertical_velocity = 0.0;
                walkthrough.grounded = true;
            }
        }
        walkthrough.position.y = standing_y + walkthrough.jump_offset;
        self.walkthrough = Some(walkthrough);
    }

    fn walk_support_height(&mut self, document: &Document, x: f64, z: f64, current_y: f64) -> f64 {
        let ray_y = current_y + 0.25;
        let mut best_y = None::<f64>;
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return 0.0;
        };
        for occurrence in occurrences {
            let Some(bounds) = self.derived.world_bounds(document, &occurrence) else {
                continue;
            };
            if x < bounds.min.x
                || x > bounds.max.x
                || z < bounds.min.z
                || z > bounds.max.z
                || bounds.min.y > ray_y
            {
                continue;
            }
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            for triangle in geometry.triangles.values() {
                let [Some(a), Some(b), Some(c)] = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                else {
                    continue;
                };
                let [a, b, c] =
                    [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                let Some(y) = height_in_triangle_xz(x, z, a, b, c) else {
                    continue;
                };
                if y <= ray_y + 1.0e-3 && best_y.is_none_or(|current| y > current) {
                    best_y = Some(y);
                }
            }
        }
        best_y.unwrap_or(0.0)
    }

    /// Restores the schema-17 camera pose using the legacy loader's position/target semantics.
    pub fn restore_legacy_camera_state(&mut self, state: &Value) -> bool {
        let Some(fields) = state.as_object() else {
            return false;
        };
        let Some(position) =
            read_camera_vector(fields.get("position"), Vec3::new(10.0, 10.0, 10.0))
        else {
            return false;
        };
        let Some(target) = read_camera_vector(fields.get("target"), Vec3::ZERO) else {
            return false;
        };
        let offset = position - target;
        let Some(eye) = offset.try_normalized(1.0e-9) else {
            return false;
        };
        let distance = offset.length();
        if !distance.is_finite() || distance > 1.0e12 {
            return false;
        }
        let horizontal = eye.x.hypot(eye.z);
        let yaw = if horizontal > 1.0e-9 {
            eye.z.atan2(eye.x) - std::f64::consts::FRAC_PI_4
        } else {
            0.0
        };
        let elevation = eye
            .y
            .clamp(-1.0, 1.0)
            .asin()
            .clamp(MIN_ELEVATION, MAX_ELEVATION);
        let field_of_view = read_camera_number(fields.get("fieldOfView"))
            .filter(|value| (1.0..179.0).contains(value))
            .unwrap_or(DEFAULT_FIELD_OF_VIEW);
        let near_plane = read_camera_number(fields.get("near"))
            .filter(|value| *value > 0.0)
            .unwrap_or(DEFAULT_NEAR_PLANE);
        let far_plane = read_camera_number(fields.get("far"))
            .filter(|value| *value > near_plane)
            .unwrap_or(DEFAULT_FAR_PLANE.max(near_plane * 2.0));

        self.target = target;
        self.distance = distance;
        self.yaw = yaw;
        self.elevation = elevation;
        self.field_of_view = field_of_view;
        self.near_plane = near_plane;
        self.far_plane = far_plane;
        self.pan = Vec2::ZERO;
        self.orthographic = None;
        self.camera_drag = None;
        self.frame_pending = false;
        true
    }

    /// Produces the legacy schema-17 camera payload for this viewport's perspective pose.
    pub fn legacy_camera_state(&self) -> Value {
        let (_, up, eye) = self.perspective_camera_axes();
        let position = self.target + eye * self.distance;
        json!({
            "position": position,
            "direction": -eye,
            "up": up,
            "target": self.target,
            "near": self.near_plane,
            "far": self.far_plane,
            "fieldOfView": self.field_of_view,
        })
    }

    fn orbit_by(&mut self, delta: Vec2) {
        self.yaw += f64::from(delta.x) * 0.008;
        self.elevation =
            (self.elevation + f64::from(delta.y) * 0.008).clamp(MIN_ELEVATION, MAX_ELEVATION);
    }

    fn drag_camera(
        &mut self,
        rect: Rect,
        button: egui::PointerButton,
        shift_held: bool,
        delta: Vec2,
        orbit_tool_active: bool,
    ) {
        if self.orthographic.is_some() {
            if button == egui::PointerButton::Secondary || button == egui::PointerButton::Middle {
                self.pan += delta;
            }
            return;
        }
        if button == egui::PointerButton::Middle
            || (button == egui::PointerButton::Secondary && shift_held)
        {
            if self.orthographic.is_some() {
                self.pan += delta;
            } else {
                self.pan_perspective(rect, delta);
            }
        } else if button == egui::PointerButton::Secondary
            || (button == egui::PointerButton::Primary && orbit_tool_active)
        {
            self.orbit_by(delta);
        }
    }

    fn zoom_at(&mut self, rect: Rect, position: Pos2, scroll: f32) {
        if self.orthographic.is_some() {
            let anchor = (position - rect.center() - self.pan) / self.zoom;
            self.zoom = (self.zoom * (scroll * 0.001).exp()).clamp(5.0, 160.0);
            self.pan = position - rect.center() - anchor * self.zoom;
            return;
        }
        let anchor = self.screen_to_ground(rect, position).or_else(|| {
            let (origin, direction) = self.ray_from_screen(rect, position)?;
            self.ray_plane_intersection(origin, direction, self.target, -self.camera_direction())
        });
        let Some(anchor) = anchor else {
            return;
        };
        self.distance = (self.distance * (-f64::from(scroll) * 0.001).exp()).clamp(1.0, 1000.0);
        self.pan = position - rect.center() - self.projected_offset(rect, anchor);
    }

    fn pan_perspective(&mut self, rect: Rect, delta: Vec2) {
        let (right, up, _) = self.camera_axes();
        let scale = self.distance / self.focal_length(rect.height());
        self.target += right * (-f64::from(delta.x) * scale);
        self.target += up * (f64::from(delta.y) * scale);
    }

    fn frame_document(&mut self, rect: Rect, document: &Document) {
        self.leave_walkthrough();
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return;
        };
        let mut points = Vec::new();
        for occurrence in occurrences {
            let Some(world_bounds) = self.derived.world_bounds(document, &occurrence) else {
                continue;
            };
            points.extend(bounds_corners(world_bounds));
        }
        let Some(first) = points.first().copied() else {
            return;
        };
        let (world_min, world_max) = points.iter().fold((first, first), |(min, max), point| {
            (
                Vec3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z)),
                Vec3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z)),
            )
        });
        self.target = (world_min + world_max) * 0.5;
        if self.orthographic.is_none() {
            self.pan = Vec2::ZERO;
            let (right, up, eye) = self.camera_axes();
            let available = rect.size() - Vec2::splat(72.0);
            let focal = self.focal_length(rect.height());
            let half_width = f64::from(available.x.max(1.0)) * 0.5;
            let half_height = f64::from(available.y.max(1.0)) * 0.5;
            let required_distance = points.iter().fold(1.0_f64, |required, point| {
                let relative = *point - self.target;
                let depth_offset = eye.dot(relative);
                let horizontal = depth_offset + focal * right.dot(relative).abs() / half_width;
                let vertical = depth_offset + focal * up.dot(relative).abs() / half_height;
                required.max(horizontal).max(vertical)
            });
            self.distance = (required_distance * 1.08).clamp(1.0, 1.0e12);
            return;
        }

        let mut bounds: Option<[f64; 4]> = None;
        for point in points {
            let (projected_x, projected_y) = self.projected_position(point);
            match &mut bounds {
                Some([min_x, min_y, max_x, max_y]) => {
                    *min_x = min_x.min(projected_x);
                    *min_y = min_y.min(projected_y);
                    *max_x = max_x.max(projected_x);
                    *max_y = max_y.max(projected_y);
                }
                None => bounds = Some([projected_x, projected_y, projected_x, projected_y]),
            }
        }
        let Some([min_x, min_y, max_x, max_y]) = bounds else {
            return;
        };
        let span_x = (max_x - min_x).max(1.0);
        let span_y = (max_y - min_y).max(1.0);
        let available = rect.size() - Vec2::splat(72.0);
        self.zoom = (f64::from(available.x) / span_x).min(f64::from(available.y) / span_y) as f32;
        self.zoom = self.zoom.clamp(5.0, 160.0);
        self.pan = Vec2::new(
            -((min_x + max_x) * 0.5) as f32 * self.zoom,
            -((min_y + max_y) * 0.5) as f32 * self.zoom,
        );
    }

    pub fn world_to_screen(&self, rect: Rect, point: Vec3) -> Pos2 {
        rect.center() + self.pan + self.projected_offset(rect, point)
    }

    fn project_if_visible(&self, rect: Rect, point: Vec3) -> Option<Pos2> {
        self.point_in_front_of_camera(point)
            .then(|| self.world_to_screen(rect, point))
    }

    fn camera_depth(&self, point: Vec3) -> f64 {
        -(self.camera_direction()).dot(point - self.camera_position())
    }

    fn clip_segment_to_camera(&self, segment: [Vec3; 2]) -> Option<[Vec3; 2]> {
        if self.orthographic.is_some() {
            return Some(segment);
        }
        let [start, end] = segment;
        let start_depth = self.camera_depth(start);
        let end_depth = self.camera_depth(end);
        if !start_depth.is_finite() || !end_depth.is_finite() {
            return None;
        }
        let mut start_t: f64 = 0.0;
        let mut end_t: f64 = 1.0;
        for (boundary, keep_greater) in [(self.near_plane, true), (self.far_plane, false)] {
            let start_inside = if keep_greater {
                start_depth >= boundary
            } else {
                start_depth <= boundary
            };
            let end_inside = if keep_greater {
                end_depth >= boundary
            } else {
                end_depth <= boundary
            };
            if !start_inside && !end_inside {
                return None;
            }
            if start_inside != end_inside {
                let denominator = end_depth - start_depth;
                if denominator.abs() <= f64::EPSILON {
                    return None;
                }
                let crossing = ((boundary - start_depth) / denominator).clamp(0.0, 1.0);
                if start_inside {
                    end_t = end_t.min(crossing);
                } else {
                    start_t = start_t.max(crossing);
                }
            }
        }
        (start_t <= end_t).then(|| {
            [
                start + (end - start) * start_t,
                start + (end - start) * end_t,
            ]
        })
    }

    fn clip_polygon_to_camera(&self, points: &[Vec3]) -> Vec<Vec3> {
        if self.orthographic.is_some() {
            return points.to_vec();
        }
        let mut polygon = points.to_vec();
        for (boundary, keep_greater) in [(self.near_plane, true), (self.far_plane, false)] {
            if polygon.is_empty() {
                break;
            }
            let mut clipped = Vec::with_capacity(polygon.len() + 1);
            let mut previous = *polygon.last().expect("nonempty polygon");
            let mut previous_depth = self.camera_depth(previous);
            let mut previous_inside = if keep_greater {
                previous_depth >= boundary
            } else {
                previous_depth <= boundary
            };
            for current in polygon.iter().copied() {
                let current_depth = self.camera_depth(current);
                let current_inside = if keep_greater {
                    current_depth >= boundary
                } else {
                    current_depth <= boundary
                };
                if previous_inside != current_inside {
                    let denominator = current_depth - previous_depth;
                    if denominator.abs() > f64::EPSILON {
                        let fraction = ((boundary - previous_depth) / denominator).clamp(0.0, 1.0);
                        clipped.push(previous + (current - previous) * fraction);
                    }
                }
                if current_inside {
                    clipped.push(current);
                }
                previous = current;
                previous_depth = current_depth;
                previous_inside = current_inside;
            }
            polygon = clipped;
        }
        polygon
    }

    fn point_in_front_of_camera(&self, point: Vec3) -> bool {
        self.orthographic.is_some()
            || (self.camera_depth(point) >= self.near_plane
                && self.camera_depth(point) <= self.far_plane)
    }

    fn projected_position(&self, point: Vec3) -> (f64, f64) {
        if let Some(view) = self.orthographic {
            let (right, up, _) = view.screen_axes();
            let relative = point - self.target;
            return (right.dot(relative), -up.dot(relative));
        }
        let (right, up, _) = self.camera_axes();
        let relative = point - self.camera_position();
        (right.dot(relative), -up.dot(relative))
    }

    fn projected_offset(&self, rect: Rect, point: Vec3) -> Vec2 {
        if self.orthographic.is_some() {
            let (x, y) = self.projected_position(point);
            return Vec2::new(x as f32, y as f32) * self.zoom;
        }
        let (right, up, eye) = self.camera_axes();
        let relative = point - self.camera_position();
        let depth = (-eye.dot(relative)).max(self.near_plane);
        let focal = self.focal_length(rect.height());
        Vec2::new(
            (focal * right.dot(relative) / depth) as f32,
            (-focal * up.dot(relative) / depth) as f32,
        )
    }

    fn focal_length(&self, viewport_height: f32) -> f64 {
        f64::from(viewport_height.max(1.0)) * 0.5 / (self.field_of_view.to_radians() * 0.5).tan()
    }

    fn camera_axes(&self) -> (Vec3, Vec3, Vec3) {
        Self::axes_for_eye(self.camera_direction())
    }

    fn perspective_camera_axes(&self) -> (Vec3, Vec3, Vec3) {
        Self::axes_for_eye(self.perspective_camera_direction())
    }

    fn axes_for_eye(eye: Vec3) -> (Vec3, Vec3, Vec3) {
        let forward = -eye;
        let right = forward
            .cross(Vec3::Y)
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::X);
        let up = right
            .cross(forward)
            .try_normalized(1.0e-12)
            .unwrap_or(Vec3::Y);
        (right, up, eye)
    }

    fn ray_from_screen(&self, rect: Rect, position: Pos2) -> Option<(Vec3, Vec3)> {
        if self.orthographic.is_some() {
            return None;
        }
        let (right, up, eye) = self.camera_axes();
        let focal = self.focal_length(rect.height());
        let pixel = position - rect.center() - self.pan;
        let direction =
            (-eye + right * (f64::from(pixel.x) / focal) + up * (-f64::from(pixel.y) / focal))
                .try_normalized(1.0e-12)?;
        Some((self.camera_position(), direction))
    }

    fn ray_plane_intersection(
        &self,
        origin: Vec3,
        direction: Vec3,
        plane_point: Vec3,
        plane_normal: Vec3,
    ) -> Option<Vec3> {
        let denominator = plane_normal.dot(direction);
        if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
            return None;
        }
        let distance = plane_normal.dot(plane_point - origin) / denominator;
        (distance.is_finite() && distance >= 0.0).then(|| origin + direction * distance)
    }

    fn camera_direction(&self) -> Vec3 {
        if let Some(view) = self.orthographic {
            return view.screen_axes().2;
        }
        if let Some(walkthrough) = self.walkthrough {
            return -walkthrough.forward();
        }
        self.perspective_camera_direction()
    }

    fn camera_position(&self) -> Vec3 {
        self.walkthrough.map_or_else(
            || self.target + self.camera_direction() * self.distance,
            |walkthrough| walkthrough.position,
        )
    }

    fn perspective_camera_direction(&self) -> Vec3 {
        let horizontal = self.elevation.cos() * ISO_X;
        Vec3::new(
            (self.yaw.cos() - self.yaw.sin()) * horizontal,
            self.elevation.sin(),
            (self.yaw.sin() + self.yaw.cos()) * horizontal,
        )
    }

    fn view_depth(&self, point: Vec3) -> f64 {
        self.camera_direction().dot(point)
    }

    #[cfg(test)]
    pub fn pick(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
    ) -> Option<OccurrenceRef> {
        self.pick_selection_target(
            rect,
            document,
            position,
            SnapScope::Document,
            &SelectionDisplayPolicy::default(),
        )
        .map(|target| target.occurrence)
    }

    pub fn pick_selection_target(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
    ) -> Option<SelectionTarget> {
        self.pick_selection_target_for_kind(rect, document, position, scope, policy, None)
    }

    /// Picks a model entity along a host-provided world ray, for native renderers that own
    /// their camera and viewport input.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn pick_selection_target_on_ray(
        &mut self,
        document: &Document,
        ray_origin: Vec3,
        ray_direction: Vec3,
        radius: f64,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
    ) -> Option<SelectionTarget> {
        if !ray_origin.is_finite() || !radius.is_finite() || radius < 0.0 {
            return None;
        }
        let direction = ray_direction.try_normalized(1.0e-12)?;
        if self
            .native_pick_index
            .as_ref()
            .is_none_or(|index| index.revision() != document.visual_revision())
        {
            self.native_pick_index = NativeSpatialIndex::build(document).ok();
        }
        let index = self.native_pick_index.as_ref()?;
        let candidates = index.query_ray(ray_origin, direction, radius);
        let mut nearest_face_depth = None::<f64>;
        let mut nearest = None::<(f64, u8, SelectionTarget)>;

        for item in &candidates {
            let NativeSpatialPrimitive::Triangle([a, b, c]) = item.primitive else {
                continue;
            };
            if !scope.contains(document, &item.target.occurrence)
                || !policy.occurrence_visible(document, &item.target.occurrence)
                || (!policy.can_draw(SelectionKind::Face)
                    && item.target.entity.kind() == SelectionKind::Face)
                || policy.target_wireframe(document, &item.target)
            {
                continue;
            }
            if let Some(depth) = ray_triangle_distance(ray_origin, direction, a, b, c) {
                nearest_face_depth =
                    Some(nearest_face_depth.map_or(depth, |current| current.min(depth)));
            }
        }

        for item in &candidates {
            let target = &item.target;
            if !scope.contains(document, &target.occurrence)
                || !policy.can_edit_target(document, target)
            {
                continue;
            }
            match item.primitive {
                NativeSpatialPrimitive::Vertex(point)
                    if policy.can_select(SelectionKind::Vertex) =>
                {
                    let depth = (point - ray_origin).dot(direction);
                    if depth >= 0.0
                        && !nearest_face_depth.is_some_and(|face| depth > face + radius)
                        && (point - (ray_origin + direction * depth)).length() <= radius
                    {
                        consider_ray_selection_hit(&mut nearest, depth, 0, target.clone());
                    }
                }
                NativeSpatialPrimitive::Segment([start, end])
                    if (policy.can_select(SelectionKind::Edge)
                        || item.target.entity.kind() != SelectionKind::Edge) =>
                {
                    let Some((depth, miss_distance)) =
                        ray_segment_distance(ray_origin, direction, start, end)
                    else {
                        continue;
                    };
                    if miss_distance <= radius
                        && !nearest_face_depth.is_some_and(|face| depth > face + radius)
                    {
                        consider_ray_selection_hit(&mut nearest, depth, 1, target.clone());
                    }
                }
                NativeSpatialPrimitive::Triangle([a, b, c])
                    if (policy.can_select(SelectionKind::Face)
                        || item.target.entity.kind() != SelectionKind::Face)
                        && !policy.target_wireframe(document, target) =>
                {
                    if let Some(depth) = ray_triangle_distance(ray_origin, direction, a, b, c) {
                        consider_ray_selection_hit(&mut nearest, depth, 2, target.clone());
                    }
                }
                // A mesh-free owner's bounds are its actual picking representation,
                // so compare its depth with model faces instead of treating it as
                // the ordinary mesh-object fallback after all primitive hits.
                NativeSpatialPrimitive::Occurrence
                    if document.procedural_object(&target.occurrence).is_some()
                        && policy.can_select(SelectionKind::Object) =>
                {
                    if let Some(depth) = ray_aabb_distance(ray_origin, direction, item.bounds) {
                        consider_ray_selection_hit(&mut nearest, depth, 3, target.clone());
                    }
                }
                NativeSpatialPrimitive::Vertex(_)
                | NativeSpatialPrimitive::Segment(_)
                | NativeSpatialPrimitive::Triangle(_)
                | NativeSpatialPrimitive::Occurrence => {}
            }
        }

        if let Some((_, _, target)) = nearest {
            return Some(target);
        }

        for item in candidates {
            if !matches!(item.primitive, NativeSpatialPrimitive::Occurrence)
                || item.target.occurrence.is_scene_root()
                || !scope.contains(document, &item.target.occurrence)
                || !policy.can_draw(SelectionKind::Object)
                || !policy.can_select(SelectionKind::Object)
            {
                continue;
            }
            if let Some(depth) = ray_aabb_distance(ray_origin, direction, item.bounds) {
                consider_ray_selection_hit(&mut nearest, depth, 3, item.target.clone());
            }
        }

        nearest.map(|(_, _, target)| target)
    }

    /// Returns the world-space snap point represented by a ray-picked geometry target.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn snap_selection_target_on_ray(
        &self,
        document: &Document,
        target: &SelectionTarget,
        ray_origin: Vec3,
        ray_direction: Vec3,
    ) -> Option<SnapResult> {
        let direction = ray_direction.try_normalized(1.0e-12)?;
        let resolved = document.resolve_occurrence(&target.occurrence)?;
        let geometry = document.effective_geometry(&target.occurrence)?;
        let (point, normal, kind) = match &target.entity {
            SelectedEntity::Vertex(id) => (
                resolved
                    .world_transform
                    .transform_point(geometry.vertices.get(id)?.position),
                Vec3::Y,
                SnapKind::Endpoint,
            ),
            SelectedEntity::Segment(id) => {
                let segment = geometry.segments.get(id)?;
                let start = resolved
                    .world_transform
                    .transform_point(geometry.vertex_position(segment.endpoints[0])?);
                let end = resolved
                    .world_transform
                    .transform_point(geometry.vertex_position(segment.endpoints[1])?);
                let (depth, _) = ray_segment_distance(ray_origin, direction, start, end)?;
                let ray_point = ray_origin + direction * depth;
                (
                    closest_point_on_segment(ray_point, start, end),
                    Vec3::Y,
                    SnapKind::Line,
                )
            }
            SelectedEntity::Triangle(id) => {
                let triangle = geometry.triangles.get(id)?;
                let [a, b, c] = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                    .map(|point| {
                        point.map(|point| resolved.world_transform.transform_point(point))
                    });
                let [Some(a), Some(b), Some(c)] = [a, b, c] else {
                    return None;
                };
                let depth = ray_triangle_distance(ray_origin, direction, a, b, c)?;
                let normal = (b - a).cross(c - a).try_normalized(1.0e-12)?;
                (ray_origin + direction * depth, normal, SnapKind::Face)
            }
            SelectedEntity::Occurrence | SelectedEntity::SemanticHandle { .. } => return None,
        };
        point.is_finite().then_some(SnapResult {
            point,
            normal,
            kind,
            screen_distance: 0.0,
        })
    }

    /// World-radius adapter retained for non-projected ray-query regression tests.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub fn snap_point_on_ray(
        &mut self,
        document: &Document,
        ray_origin: Vec3,
        ray_direction: Vec3,
        radius: f64,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
    ) -> Option<SnapResult> {
        self.snap_geometry_on_ray(
            document,
            ray_origin,
            ray_direction,
            SnapTolerance::World(radius),
            scope,
            policy,
            None,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn snap_geometry_on_ray(
        &mut self,
        document: &Document,
        ray_origin: Vec3,
        ray_direction: Vec3,
        tolerance: SnapTolerance,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
        excluded: Option<&SelectionTarget>,
    ) -> Option<SnapResult> {
        if !ray_origin.is_finite() || !tolerance.radius().is_finite() || tolerance.radius() < 0. {
            return None;
        }
        let direction = ray_direction.try_normalized(1e-12)?;
        if self
            .native_pick_index
            .as_ref()
            .is_none_or(|i| i.revision() != document.visual_revision())
        {
            self.native_pick_index = NativeSpatialIndex::build(document).ok();
        }
        let candidates =
            self.native_pick_index
                .as_ref()?
                .query_ray_with_margin(ray_origin, direction, |b| tolerance.margin(b));
        let visible = |item: &&crate::native_spatial::NativeSpatialItem| {
            excluded != Some(&item.target)
                && scope.contains(document, &item.target.occurrence)
                && policy.can_draw_target(document, &item.target)
        };
        let nearest_face_depth = candidates
            .iter()
            .copied()
            .filter(visible)
            .filter_map(|item| {
                let NativeSpatialPrimitive::Triangle([a, b, c]) = item.primitive else {
                    return None;
                };
                if (!policy.can_draw(SelectionKind::Face)
                    && item.target.entity.kind() == SelectionKind::Face)
                    || policy.target_wireframe(document, &item.target)
                {
                    return None;
                }
                ray_triangle_distance(ray_origin, direction, a, b, c)
            })
            .min_by(f64::total_cmp);
        let mut nearest = None;
        let mut consider = |point: Vec3, normal: Vec3, kind: SnapKind, priority: u8| {
            let depth = (point - ray_origin).dot(direction);
            let Some(distance) = tolerance.distance(point, ray_origin, direction) else {
                return;
            };
            if depth >= 0.
                && distance <= tolerance.radius()
                && !nearest_face_depth
                    .is_some_and(|face| depth > face + tolerance.world_radius(point))
            {
                consider_ray_snap(
                    &mut nearest,
                    depth,
                    priority,
                    SnapResult {
                        point,
                        normal,
                        kind,
                        screen_distance: distance as f32,
                    },
                );
            }
        };
        for item in candidates.iter().copied().filter(visible) {
            let mut segment = |start: Vec3, end: Vec3, normal: Vec3| {
                for (point, kind) in [
                    (start, SnapKind::Endpoint),
                    (end, SnapKind::Endpoint),
                    ((start + end) * 0.5, SnapKind::Midpoint),
                ] {
                    consider(point, normal, kind, 0);
                }
                let point = tolerance.segment_point(start, end).or_else(|| {
                    ray_segment_distance(ray_origin, direction, start, end).map(|(d, _)| {
                        closest_point_on_segment(ray_origin + direction * d, start, end)
                    })
                });
                if let Some(p) = point {
                    consider(p, normal, SnapKind::Line, 1);
                }
            };
            match item.primitive {
                NativeSpatialPrimitive::Vertex(p) if policy.can_draw(SelectionKind::Vertex) => {
                    consider(p, Vec3::Y, SnapKind::Endpoint, 0)
                }
                NativeSpatialPrimitive::Segment([a, b])
                    if policy.can_draw(SelectionKind::Edge)
                        || item.target.entity.kind() != SelectionKind::Edge =>
                {
                    segment(a, b, Vec3::Y)
                }
                NativeSpatialPrimitive::Triangle([a, b, c])
                    if policy.can_draw(SelectionKind::Face)
                        || item.target.entity.kind() != SelectionKind::Face =>
                {
                    let Some(normal) = (b - a).cross(c - a).try_normalized(1e-12) else {
                        continue;
                    };
                    for [start, end] in [[a, b], [b, c], [c, a]] {
                        segment(start, end, normal);
                    }
                    if !policy.target_wireframe(document, &item.target)
                        && let Some(d) = ray_triangle_distance(ray_origin, direction, a, b, c)
                    {
                        consider(ray_origin + direction * d, normal, SnapKind::Face, 2);
                    }
                }
                _ => {}
            }
        }
        nearest.map(|(_, _, snap)| snap)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_cursor_axes(
        &mut self,
        document: &Document,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
        snap: SnapResult,
        ground_normal: Vec3,
    ) -> Option<[Vec3; 3]> {
        let segment_axis = if matches!(snap.kind, SnapKind::Line | SnapKind::Midpoint) {
            if self
                .native_pick_index
                .as_ref()
                .is_none_or(|index| index.revision() != document.visual_revision())
            {
                self.native_pick_index = NativeSpatialIndex::build(document).ok();
            }
            self.native_pick_index.as_ref().and_then(|index| {
                let candidates = index.query_point(snap.point, 1.0e-6);
                let mut nearest = None::<(f64, Vec3)>;
                for item in candidates {
                    if !scope.contains(document, &item.target.occurrence)
                        || !policy.occurrence_visible(document, &item.target.occurrence)
                    {
                        continue;
                    }
                    let NativeSpatialPrimitive::Segment([start, end]) = item.primitive else {
                        continue;
                    };
                    let Some(axis) = (end - start).try_normalized(1.0e-12) else {
                        continue;
                    };
                    let distance = snap
                        .point
                        .distance(closest_point_on_segment(snap.point, start, end));
                    if distance <= 1.0e-5 && nearest.is_none_or(|(current, _)| distance < current) {
                        nearest = Some((distance, axis));
                    }
                }
                nearest.map(|(_, axis)| axis)
            })
        } else {
            None
        };
        cursor_axes(snap.normal, segment_axis, ground_normal)
    }

    pub fn pick_selection_target_for_kind(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
        preferred_kind: Option<SelectionKind>,
    ) -> Option<SelectionTarget> {
        let occurrences = document.scene_geometry_refs().ok()?;
        let mut occluding_face_depth = None::<f64>;
        if policy.can_draw(SelectionKind::Face) && !policy.state(SelectionKind::Face).wireframe {
            for occurrence in &occurrences {
                if !scope.contains(document, occurrence)
                    || !policy.occurrence_visible(document, occurrence)
                {
                    continue;
                }
                let (Some(resolved), Some(geometry)) = (
                    document.resolve_occurrence(occurrence),
                    document.effective_geometry(occurrence),
                ) else {
                    continue;
                };
                for triangle in geometry.triangles.values() {
                    let [Some(a), Some(b), Some(c)] = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        continue;
                    };
                    let world =
                        [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                    if world
                        .iter()
                        .any(|point| !self.point_in_front_of_camera(*point))
                    {
                        continue;
                    }
                    let [a, b, c] = world.map(|point| self.world_to_screen(rect, point));
                    if point_in_triangle(position, a, b, c) {
                        let depth = world
                            .iter()
                            .map(|point| self.view_depth(*point))
                            .sum::<f64>()
                            / 3.0;
                        occluding_face_depth =
                            Some(occluding_face_depth.map_or(depth, |current| current.max(depth)));
                    }
                }
            }
        }
        let mut nearest: Option<(f32, f64, SelectionTarget)> = None;
        for occurrence in occurrences {
            if !scope.contains(document, &occurrence)
                || !policy.occurrence_visible(document, &occurrence)
            {
                continue;
            }
            let resolved = document.resolve_occurrence(&occurrence)?;
            let geometry = document.effective_geometry(&occurrence)?;
            if let Some(bounds) = self.derived.world_bounds(document, &occurrence) {
                let points = bounds_corners(bounds)
                    .into_iter()
                    .filter_map(|point| self.project_if_visible(rect, point))
                    .collect::<Vec<_>>();
                if points.is_empty() {
                    continue;
                }
                let (min_x, max_x) = points
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |range, point| {
                        (range.0.min(point.x), range.1.max(point.x))
                    });
                let (min_y, max_y) = points
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |range, point| {
                        (range.0.min(point.y), range.1.max(point.y))
                    });
                if position.x < min_x
                    || position.x > max_x
                    || position.y < min_y
                    || position.y > max_y
                {
                    // A precise edge or face can extend beyond a loose derived bound.
                    // Continue testing it below, but omit the occurrence candidate.
                } else if !occurrence.is_scene_root()
                    && policy.can_select(SelectionKind::Object)
                    && preferred_kind.is_none_or(|kind| kind == SelectionKind::Object)
                    && (preferred_kind == Some(SelectionKind::Object)
                        || !geometry.triangles.is_empty()
                        || geometry.segments.is_empty())
                {
                    let center = Vec3::new(
                        (bounds.min.x + bounds.max.x) * 0.5,
                        (bounds.min.y + bounds.max.y) * 0.5,
                        (bounds.min.z + bounds.max.z) * 0.5,
                    );
                    consider_selection_hit(
                        &mut nearest,
                        if preferred_kind == Some(SelectionKind::Object) {
                            0.0
                        } else {
                            18.0
                        },
                        self.view_depth(center),
                        SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Occurrence,
                        },
                    );
                }
            }
            if preferred_kind.is_none_or(|kind| kind == SelectionKind::Vertex)
                && policy.can_select(SelectionKind::Vertex)
            {
                for (vertex_id, vertex) in &geometry.vertices {
                    let world = resolved.world_transform.transform_point(vertex.position);
                    let Some(screen) = self.project_if_visible(rect, world) else {
                        continue;
                    };
                    let distance = screen.distance(position);
                    if distance <= 5.0
                        && !occluding_face_depth
                            .is_some_and(|face_depth| self.view_depth(world) < face_depth - 1.0e-4)
                    {
                        consider_selection_hit(
                            &mut nearest,
                            distance,
                            self.view_depth(world),
                            SelectionTarget {
                                occurrence: occurrence.clone(),
                                entity: SelectedEntity::Vertex(*vertex_id),
                            },
                        );
                    }
                }
            }
            if preferred_kind.is_none_or(|kind| kind == SelectionKind::Face)
                && policy.can_select(SelectionKind::Face)
                && !policy.state(SelectionKind::Face).wireframe
            {
                for triangle in geometry.triangles.values() {
                    let [Some(a), Some(b), Some(c)] = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        continue;
                    };
                    let world_points =
                        [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                    if world_points
                        .iter()
                        .any(|point| !self.point_in_front_of_camera(*point))
                    {
                        continue;
                    }
                    let [a, b, c] = world_points.map(|point| self.world_to_screen(rect, point));
                    if point_in_triangle(position, a, b, c) {
                        let depth = world_points
                            .iter()
                            .map(|point| self.view_depth(*point))
                            .sum::<f64>()
                            / 3.0;
                        consider_selection_hit(
                            &mut nearest,
                            12.0,
                            depth,
                            SelectionTarget {
                                occurrence: occurrence.clone(),
                                entity: SelectedEntity::Triangle(triangle.id),
                            },
                        );
                    }
                }
            }
            if preferred_kind.is_none_or(|kind| kind == SelectionKind::Edge)
                && policy.can_select(SelectionKind::Edge)
            {
                for segment in geometry.segments.values() {
                    let (Some(start_local), Some(end_local)) = (
                        geometry.vertex_position(segment.endpoints[0]),
                        geometry.vertex_position(segment.endpoints[1]),
                    ) else {
                        continue;
                    };
                    let start = resolved.world_transform.transform_point(start_local);
                    let end = resolved.world_transform.transform_point(end_local);
                    let (Some(a), Some(b)) = (
                        self.project_if_visible(rect, start),
                        self.project_if_visible(rect, end),
                    ) else {
                        continue;
                    };
                    let distance = distance_to_segment_squared(position, a, b).sqrt();
                    if distance > 5.0 {
                        continue;
                    }
                    let depth = self.view_depth((start + end) * 0.5);
                    if occluding_face_depth.is_some_and(|face_depth| depth < face_depth - 1.0e-4) {
                        continue;
                    }
                    consider_selection_hit(
                        &mut nearest,
                        6.0 + distance,
                        depth,
                        SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Segment(segment.id),
                        },
                    );
                }
            }
        }
        nearest.map(|(_, _, target)| target)
    }

    pub fn select_screen_region(
        &mut self,
        rect: Rect,
        document: &Document,
        region: Rect,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
        crossing: bool,
    ) -> Vec<SelectionTarget> {
        if region.width() <= f32::EPSILON || region.height() <= f32::EPSILON {
            return Vec::new();
        }
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return Vec::new();
        };
        let mut selected = BTreeSet::new();
        for occurrence in occurrences {
            if !scope.contains(document, &occurrence)
                || !policy.occurrence_visible(document, &occurrence)
            {
                continue;
            }
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            if !occurrence.is_scene_root()
                && policy.can_select(SelectionKind::Object)
                && let Some(bounds) = self.derived.world_bounds(document, &occurrence)
            {
                let points = bounds_corners(bounds)
                    .into_iter()
                    .filter_map(|point| self.project_if_visible(rect, point))
                    .collect::<Vec<_>>();
                if !points.is_empty() {
                    let projected_bounds = Rect::from_min_max(
                        Pos2::new(
                            points
                                .iter()
                                .map(|point| point.x)
                                .fold(f32::INFINITY, f32::min),
                            points
                                .iter()
                                .map(|point| point.y)
                                .fold(f32::INFINITY, f32::min),
                        ),
                        Pos2::new(
                            points
                                .iter()
                                .map(|point| point.x)
                                .fold(f32::NEG_INFINITY, f32::max),
                            points
                                .iter()
                                .map(|point| point.y)
                                .fold(f32::NEG_INFINITY, f32::max),
                        ),
                    );
                    let contained = points.iter().all(|point| region.contains(*point));
                    if (crossing && region.intersects(projected_bounds)) || (!crossing && contained)
                    {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Occurrence,
                        });
                    }
                }
            }
            if policy.can_select(SelectionKind::Vertex) {
                for (vertex_id, vertex) in &geometry.vertices {
                    let world = resolved.world_transform.transform_point(vertex.position);
                    let Some(screen) = self.project_if_visible(rect, world) else {
                        continue;
                    };
                    if region.contains(screen) {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Vertex(*vertex_id),
                        });
                    }
                }
            }
            if policy.can_select(SelectionKind::Edge) {
                for segment in geometry.segments.values() {
                    let (Some(start), Some(end)) = (
                        geometry.vertex_position(segment.endpoints[0]),
                        geometry.vertex_position(segment.endpoints[1]),
                    ) else {
                        continue;
                    };
                    let (Some(a), Some(b)) = (
                        self.project_if_visible(
                            rect,
                            resolved.world_transform.transform_point(start),
                        ),
                        self.project_if_visible(
                            rect,
                            resolved.world_transform.transform_point(end),
                        ),
                    ) else {
                        continue;
                    };
                    let hit = if crossing {
                        region.contains(a)
                            || region.contains(b)
                            || segment_intersects_rect(a, b, region)
                    } else {
                        region.contains(a) && region.contains(b)
                    };
                    if hit {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Segment(segment.id),
                        });
                    }
                }
            }
            if policy.can_select(SelectionKind::Face)
                && !policy.state(SelectionKind::Face).wireframe
            {
                for triangle in geometry.triangles.values() {
                    let [Some(a), Some(b), Some(c)] = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        continue;
                    };
                    let world_points =
                        [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                    if world_points
                        .iter()
                        .any(|point| !self.point_in_front_of_camera(*point))
                    {
                        continue;
                    }
                    let points = world_points.map(|point| self.world_to_screen(rect, point));
                    let hit = if crossing {
                        points.iter().any(|point| region.contains(*point))
                            || (0..3).any(|index| {
                                segment_intersects_rect(
                                    points[index],
                                    points[(index + 1) % 3],
                                    region,
                                )
                            })
                            || [
                                region.left_top(),
                                region.right_top(),
                                region.right_bottom(),
                                region.left_bottom(),
                            ]
                            .into_iter()
                            .any(|corner| {
                                point_in_triangle(corner, points[0], points[1], points[2])
                            })
                    } else {
                        points.iter().all(|point| region.contains(*point))
                    };
                    if hit {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Triangle(triangle.id),
                        });
                    }
                }
            }
        }
        selected.into_iter().collect()
    }

    pub fn select_world_volume(
        &mut self,
        document: &Document,
        first: Vec3,
        second: Vec3,
        scope: SnapScope<'_>,
        policy: &SelectionDisplayPolicy,
    ) -> Vec<SelectionTarget> {
        if !first.is_finite() || !second.is_finite() {
            return Vec::new();
        }
        let volume = meshcraft_core::geometry::Aabb3 {
            min: Vec3::new(
                first.x.min(second.x),
                first.y.min(second.y),
                first.z.min(second.z),
            ),
            max: Vec3::new(
                first.x.max(second.x),
                first.y.max(second.y),
                first.z.max(second.z),
            ),
        };
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return Vec::new();
        };
        let mut selected = BTreeSet::new();
        for occurrence in occurrences {
            if !scope.contains(document, &occurrence)
                || !policy.occurrence_visible(document, &occurrence)
            {
                continue;
            }
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            if document.is_object_occurrence(&occurrence)
                && policy.can_select(SelectionKind::Object)
                && self
                    .derived
                    .world_bounds(document, &occurrence)
                    .is_some_and(|bounds| aabb_intersects(bounds, volume))
            {
                selected.insert(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Occurrence,
                });
            }
            if policy.can_select(SelectionKind::Vertex) {
                for (id, vertex) in &geometry.vertices {
                    if vertex
                        .legacy_aliases
                        .iter()
                        .any(|a| a.source == meshcraft_core::document::architecture::GENERATED)
                    {
                        continue;
                    }
                    let point = resolved.world_transform.transform_point(vertex.position);
                    if point_in_aabb(point, volume) {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::Vertex(*id),
                        });
                    }
                }
            }
            {
                for segment in geometry.segments.values() {
                    let entity = meshcraft_core::document::architecture::semantic_entity(
                        &segment.extensions,
                    )
                    .unwrap_or(SelectedEntity::Segment(segment.id));
                    if !policy.can_select(entity.kind()) {
                        continue;
                    }
                    let (Some(start), Some(end)) = (
                        geometry.vertex_position(segment.endpoints[0]),
                        geometry.vertex_position(segment.endpoints[1]),
                    ) else {
                        continue;
                    };
                    let start = resolved.world_transform.transform_point(start);
                    let end = resolved.world_transform.transform_point(end);
                    if segment_intersects_aabb(start, end, volume) {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity,
                        });
                    }
                }
            }
            {
                for triangle in geometry.triangles.values() {
                    let entity = meshcraft_core::document::architecture::semantic_entity(
                        &triangle.extensions,
                    )
                    .unwrap_or(SelectedEntity::Triangle(triangle.id));
                    if !policy.can_select(entity.kind())
                        || (matches!(entity, SelectedEntity::Triangle(_))
                            && policy.state(SelectionKind::Face).wireframe)
                    {
                        continue;
                    }
                    let [Some(a), Some(b), Some(c)] = triangle
                        .vertices
                        .map(|reference| geometry.vertex_position(reference))
                    else {
                        continue;
                    };
                    let triangle_bounds = meshcraft_core::geometry::Aabb3::from_points(
                        [a, b, c].map(|point| resolved.world_transform.transform_point(point)),
                    );
                    if triangle_bounds.is_some_and(|bounds| aabb_intersects(bounds, volume)) {
                        selected.insert(SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity,
                        });
                    }
                }
            }
        }
        selected.into_iter().collect()
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect, style: GridStyle) {
        if !style.visible || !style.spacing.is_finite() || style.spacing <= 0.0 {
            return;
        }
        let extent = style.extent.min(1000) as i32;
        let world_extent = f64::from(extent) * style.spacing;
        let (axis_u, axis_v) = match style.normal_axis {
            GridNormalAxis::X => (Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
            GridNormalAxis::Y => (Vec3::new(-1.0, 0.0, 0.0), Vec3::Z),
            GridNormalAxis::Z => (Vec3::X, Vec3::Y),
        };
        let projected_origin = self.project_if_visible(rect, Vec3::ZERO);
        let step_u = self.project_if_visible(rect, axis_u * style.spacing);
        let step_v = self.project_if_visible(rect, axis_v * style.spacing);
        let pixels_per_step = projected_origin
            .zip(step_u)
            .map(|(origin, step)| origin.distance(step))
            .into_iter()
            .chain(
                projected_origin
                    .zip(step_v)
                    .map(|(origin, step)| origin.distance(step)),
            )
            .fold(f32::INFINITY, f32::min);
        let stride = if pixels_per_step.is_finite() && pixels_per_step < 8.0 {
            (8.0 / pixels_per_step.max(0.25)).ceil().clamp(1.0, 1000.0) as i32
        } else {
            1
        };
        for step in -extent..=extent {
            if step % stride != 0 && step.abs() % 5 != 0 {
                continue;
            }
            let coordinate = f64::from(step) * style.spacing;
            let color = if step == 0 {
                Color32::from_rgb(129, 171, 127)
            } else if step.abs() % 10 == 0 {
                Color32::from_rgb(145, 171, 187)
            } else if step.abs() % 5 == 0 {
                Color32::from_rgb(158, 179, 193)
            } else {
                Color32::from_rgb(169, 186, 199)
            };
            let width = if step == 0 {
                style.thick_width
            } else if step.abs() % 5 == 0 {
                style.normal_width.max(1.25)
            } else {
                style.normal_width
            };
            let parallel_v_start = Vec3::ZERO + axis_u * coordinate - axis_v * world_extent;
            let parallel_v_end = Vec3::ZERO + axis_u * coordinate + axis_v * world_extent;
            let parallel_u_start = Vec3::ZERO + axis_v * coordinate - axis_u * world_extent;
            let parallel_u_end = Vec3::ZERO + axis_v * coordinate + axis_u * world_extent;
            if let Some([start, end]) =
                self.clip_segment_to_camera([parallel_v_start, parallel_v_end])
            {
                painter.line_segment(
                    [
                        self.world_to_screen(rect, start),
                        self.world_to_screen(rect, end),
                    ],
                    Stroke::new(width, color),
                );
            }
            if let Some([start, end]) =
                self.clip_segment_to_camera([parallel_u_start, parallel_u_end])
            {
                painter.line_segment(
                    [
                        self.world_to_screen(rect, start),
                        self.world_to_screen(rect, end),
                    ],
                    Stroke::new(width, color),
                );
            }
        }
        for (axis, color) in [
            (
                Vec3::new(world_extent, 0.0, 0.0),
                Color32::from_rgb(202, 105, 105),
            ),
            (
                Vec3::new(0.0, 0.0, world_extent),
                Color32::from_rgb(100, 160, 115),
            ),
            (
                Vec3::new(0.0, world_extent, 0.0),
                Color32::from_rgb(100, 130, 190),
            ),
        ] {
            for (start, end, axis_color) in [
                (-axis, Vec3::ZERO, lighter_axis_color(color)),
                (Vec3::ZERO, axis, color),
            ] {
                if let Some([start, end]) = self.clip_segment_to_camera([start, end]) {
                    painter.line_segment(
                        [
                            self.world_to_screen(rect, start),
                            self.world_to_screen(rect, end),
                        ],
                        Stroke::new(style.thick_width, axis_color),
                    );
                }
            }
        }
    }

    fn update_adaptive_grid(
        &mut self,
        snap: Option<SnapResult>,
        style: GridStyle,
        guides: &[ConstructionGuide],
    ) {
        let Some(snap) = snap else {
            self.adaptive_grid_cloud = None;
            return;
        };
        let spacing = style.spacing;
        let Some(normal) = snap.normal.try_normalized(1.0e-12) else {
            self.adaptive_grid_cloud = None;
            return;
        };
        if !spacing.is_finite() || spacing <= 0.0 {
            self.adaptive_grid_cloud = None;
            return;
        }

        let (base_u, base_v) = match style.normal_axis {
            GridNormalAxis::X => (Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
            GridNormalAxis::Y => (Vec3::new(-1.0, 0.0, 0.0), Vec3::Z),
            GridNormalAxis::Z => (Vec3::X, Vec3::Y),
        };
        let base_extent = f64::from(style.extent.min(1000)) * spacing;
        let on_base_plane = normal.dot(style.normal_axis.vector()).abs() >= 0.98
            && snap.point.dot(style.normal_axis.vector()).abs() <= spacing * 0.2;
        let base_coordinate_u = snap.point.dot(base_u);
        let base_coordinate_v = snap.point.dot(base_v);
        let near_base_edge = on_base_plane
            && (base_coordinate_u.abs() >= base_extent * 0.75
                || base_coordinate_v.abs() >= base_extent * 0.75);
        let outside_base = !on_base_plane
            || base_coordinate_u.abs() > base_extent
            || base_coordinate_v.abs() > base_extent;
        let should_activate = near_base_edge
            || outside_base
            || guides
                .iter()
                .any(|guide| guide.kind == ConstructionGuideKind::Grid);

        if self.adaptive_grid_cloud.is_none() && !should_activate {
            return;
        }
        if self
            .adaptive_grid_cloud
            .is_none_or(|cloud| cloud.should_reanchor(snap.point, normal, spacing))
        {
            self.adaptive_grid_cloud = AdaptiveGridCloud::at_point(snap.point, normal, spacing);
        }
    }

    fn draw_adaptive_grid_cloud(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        cloud: AdaptiveGridCloud,
        spacing: f64,
    ) {
        if !spacing.is_finite() || spacing <= 0.0 {
            return;
        }
        let cross_half = (spacing * 0.08).clamp(0.05, 0.18);
        let color = Color32::from_rgba_unmultiplied(20, 41, 82, 185);
        let stroke = Stroke::new(1.0, color);
        for i in -ADAPTIVE_GRID_RADIUS_UNITS..=ADAPTIVE_GRID_RADIUS_UNITS {
            for j in -ADAPTIVE_GRID_RADIUS_UNITS..=ADAPTIVE_GRID_RADIUS_UNITS {
                let point = cloud.center
                    + cloud.axis_u * (f64::from(i) * spacing)
                    + cloud.axis_v * (f64::from(j) * spacing);
                for axis in [cloud.axis_u, cloud.axis_v] {
                    let start = point - axis * cross_half;
                    let end = point + axis * cross_half;
                    if let Some([start, end]) = self.clip_segment_to_camera([start, end]) {
                        painter.line_segment(
                            [
                                self.world_to_screen(rect, start),
                                self.world_to_screen(rect, end),
                            ],
                            stroke,
                        );
                    }
                }
            }
        }
    }

    fn draw_guides(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        guides: &[ConstructionGuide],
        style: GridStyle,
        preview: bool,
    ) {
        let spacing = style.spacing;
        if !spacing.is_finite() || spacing <= 0.0 {
            return;
        }
        let extent = 10.0 * spacing;
        let steps = 10_i32;
        for guide in guides {
            match guide.kind {
                ConstructionGuideKind::Grid => {
                    let color = if preview {
                        Color32::from_rgba_unmultiplied(84, 225, 231, 185)
                    } else {
                        Color32::from_rgba_unmultiplied(72, 196, 207, 125)
                    };
                    for (plane_index, (_, axis_u, axis_v)) in guide.planes().into_iter().enumerate()
                    {
                        for step in -steps..=steps {
                            let offset = f64::from(step) * spacing;
                            let line_color = if step == 0 {
                                Color32::from_rgba_unmultiplied(83, 220, 225, 190)
                            } else {
                                color
                            };
                            for (start, end) in [
                                (
                                    guide.origin + axis_u * offset - axis_v * extent,
                                    guide.origin + axis_u * offset + axis_v * extent,
                                ),
                                (
                                    guide.origin + axis_v * offset - axis_u * extent,
                                    guide.origin + axis_v * offset + axis_u * extent,
                                ),
                            ] {
                                if let Some([start, end]) =
                                    self.clip_segment_to_camera([start, end])
                                {
                                    painter.line_segment(
                                        [
                                            self.world_to_screen(rect, start),
                                            self.world_to_screen(rect, end),
                                        ],
                                        Stroke::new(
                                            if step == 0 { style.thick_width } else { 1.0 },
                                            line_color,
                                        ),
                                    );
                                }
                            }
                        }
                        if plane_index == 0 {
                            let origin = guide.origin;
                            if let Some(screen) = self.project_if_visible(rect, origin) {
                                painter.circle_stroke(
                                    screen,
                                    4.0,
                                    Stroke::new(1.5, Color32::from_rgb(100, 235, 239)),
                                );
                            }
                        }
                    }
                }
                ConstructionGuideKind::Axis => {
                    for (axis, axis_color) in [
                        (guide.axis_u, axis_guide_colors(preview)[0]),
                        (guide.axis_v, axis_guide_colors(preview)[1]),
                        (guide.axis_w, axis_guide_colors(preview)[2]),
                    ] {
                        for (start, end, color) in [
                            (
                                guide.origin - axis * extent,
                                guide.origin,
                                lighter_axis_color(axis_color),
                            ),
                            (guide.origin, guide.origin + axis * extent, axis_color),
                        ] {
                            if let Some([start, end]) = self.clip_segment_to_camera([start, end]) {
                                painter.line_segment(
                                    [
                                        self.world_to_screen(rect, start),
                                        self.world_to_screen(rect, end),
                                    ],
                                    Stroke::new(style.thick_width.max(1.4), color),
                                );
                            }
                        }
                    }
                    if let Some(origin) = self.project_if_visible(rect, guide.origin) {
                        painter.circle_stroke(
                            origin,
                            5.0,
                            Stroke::new(1.5, axis_guide_colors(preview)[2]),
                        );
                    }
                }
            }
        }
    }

    fn draw_cursor(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        snap: SnapResult,
        spacing: f64,
        line_axis: Option<Vec3>,
        ground_normal: Vec3,
    ) {
        let Some(origin_screen) = self.project_if_visible(rect, snap.point) else {
            return;
        };
        let Some([red_axis, green_axis, blue_axis]) =
            cursor_axes(snap.normal, line_axis, ground_normal)
        else {
            return;
        };
        let spacing = if spacing.is_finite() && spacing > 1.0e-6 {
            spacing
        } else {
            1.0
        };
        let [red, green, blue] = axis_guide_colors(true);
        for (axis, color) in [(red_axis, red), (green_axis, green), (blue_axis, blue)] {
            let projected_unit = self
                .project_if_visible(rect, snap.point + axis * spacing)
                .map_or(0.0, |screen| screen.distance(origin_screen));
            let extent = spacing
                * f64::from(if projected_unit > 1.0e-3 {
                    (25.0 / projected_unit).clamp(0.2, 12.0)
                } else {
                    1.0
                });
            let negative = snap.point - axis * (extent * 0.62);
            let positive = snap.point + axis * extent;

            if let Some([start, _]) = self.clip_segment_to_camera([negative, snap.point]) {
                let screen_segment = [self.world_to_screen(rect, start), origin_screen];
                painter.line_segment(screen_segment, Stroke::new(2.0, lighter_axis_color(color)));
            }
            if let Some([start, end]) = self.clip_segment_to_camera([snap.point, positive]) {
                let start_screen = if start == snap.point {
                    origin_screen
                } else {
                    self.world_to_screen(rect, start)
                };
                let end_screen = self.world_to_screen(rect, end);
                painter.line_segment([start_screen, end_screen], Stroke::new(2.0, color));
            }
        }
        painter.circle_filled(origin_screen, 3.0, Color32::BLACK);
        painter.circle_filled(origin_screen, 1.6, Color32::WHITE);
    }

    fn draw_ground_shadows(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        triangles: &[WorldTriangle],
        shadow_map: Option<&ShadowMap>,
    ) {
        let Some(shadow_map) = shadow_map else {
            return;
        };
        let light = shadow_map.light_direction;
        if light.y <= 1.0e-6 {
            return;
        }
        let mut bounds = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        for point in triangles.iter().flat_map(|triangle| triangle.points) {
            let mut include = |x: f64, z: f64| {
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].max(x);
                bounds[2] = bounds[2].min(z);
                bounds[3] = bounds[3].max(z);
            };
            if point.y >= 0.0 {
                include(point.x, point.z);
            }
            if point.y > 1.0e-8 {
                let distance = point.y / light.y;
                include(point.x - light.x * distance, point.z - light.z * distance);
            }
        }
        let extent = (bounds[1] - bounds[0]).max(bounds[3] - bounds[2]);
        if !extent.is_finite() || extent <= 1.0e-8 {
            return;
        }
        let spacing_x = (bounds[1] - bounds[0]) / GROUND_SHADOW_RESOLUTION as f64;
        let spacing_z = (bounds[3] - bounds[2]) / GROUND_SHADOW_RESOLUTION as f64;
        if spacing_x <= 1.0e-8 || spacing_z <= 1.0e-8 {
            return;
        }
        for row in 0..GROUND_SHADOW_RESOLUTION {
            for column in 0..GROUND_SHADOW_RESOLUTION {
                let min_x = bounds[0] + column as f64 * spacing_x;
                let min_z = bounds[2] + row as f64 * spacing_z;
                let center = Vec3::new(min_x + spacing_x * 0.5, 0.0, min_z + spacing_z * 0.5);
                let coverage = shadow_map.shadow_coverage(center, Vec3::Y);
                if coverage < 0.15 {
                    continue;
                }
                let corners = [
                    Vec3::new(min_x, 0.0, min_z),
                    Vec3::new(min_x + spacing_x, 0.0, min_z),
                    Vec3::new(min_x + spacing_x, 0.0, min_z + spacing_z),
                    Vec3::new(min_x, 0.0, min_z + spacing_z),
                ];
                let Some(points) = corners
                    .into_iter()
                    .map(|point| self.project_if_visible(rect, point))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let alpha = (coverage * 58.0).round().clamp(0.0, 58.0) as u8;
                painter.add(Shape::convex_polygon(
                    points,
                    Color32::from_rgba_unmultiplied(34, 43, 55, alpha),
                    Stroke::NONE,
                ));
            }
        }
    }

    fn draw_document(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        scene: &ViewportScene<'_>,
        shadow_map: Option<&ShadowMap>,
    ) {
        let document = scene.document;
        let selection = scene.selection;
        let scope = scene.scope;
        let policy = scene.display_policy;
        let Ok(occurrences) = document.scene_geometry_refs() else {
            return;
        };
        let mut faces = Vec::new();
        let mut segments = Vec::new();
        let mut occluders = Vec::new();
        let mut vertex_markers = Vec::new();
        let view_direction = self.camera_direction();
        for occurrence in occurrences {
            if !scope.contains(document, &occurrence)
                || !policy.occurrence_visible(document, &occurrence)
            {
                continue;
            }
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            let selected = selection.iter().any(|target| {
                target.occurrence == occurrence && target.entity == SelectedEntity::Occurrence
            });
            let triangle_records = geometry
                .triangles
                .values()
                .filter_map(|triangle| {
                    let points = triangle.vertices.map(|reference| {
                        geometry
                            .vertex_position(reference)
                            .map(|point| resolved.world_transform.transform_point(point))
                    });
                    let [Some(a), Some(b), Some(c)] = points else {
                        return None;
                    };
                    Some((
                        triangle.id,
                        triangle.vertices.map(|reference| reference.id),
                        [a, b, c],
                    ))
                })
                .collect::<Vec<_>>();
            let winding_flips = coplanar_triangle_winding_flips(&triangle_records);
            let coplanar_shared_edges = coplanar_shared_triangle_edges(&triangle_records);
            let mut surface_edges = BTreeSet::new();
            let mut visible_surface_edges = BTreeSet::new();
            for triangle in geometry.triangles.values() {
                let [Some(a), Some(b), Some(c)] = triangle
                    .vertices
                    .map(|reference| geometry.vertex_position(reference))
                else {
                    continue;
                };
                let world_points =
                    [a, b, c].map(|point| resolved.world_transform.transform_point(point));
                let mut normal = (world_points[1] - world_points[0])
                    .cross(world_points[2] - world_points[0])
                    .try_normalized(1.0e-12)
                    .unwrap_or(Vec3::Y);
                if winding_flips.contains(&triangle.id) {
                    normal = -normal;
                }
                let facing = normal.dot(view_direction);
                for [a, b] in [[0, 1], [1, 2], [2, 0]] {
                    let edge = edge_key(triangle.vertices[a].id, triangle.vertices[b].id);
                    surface_edges.insert(edge);
                    if facing > 0.0 {
                        visible_surface_edges.insert(edge);
                    }
                }
                let clipped_world = self.clip_polygon_to_camera(&world_points);
                if clipped_world.len() < 3 {
                    continue;
                }
                if policy.can_draw(SelectionKind::Face)
                    && !policy.state(SelectionKind::Face).wireframe
                {
                    let points = clipped_world
                        .iter()
                        .map(|point| self.world_to_screen(rect, *point))
                        .collect::<Vec<_>>();
                    let base_color = color32(triangle.color);
                    let center = (world_points[0] + world_points[1] + world_points[2]) / 3.0;
                    let shadow = shadow_map
                        .map_or(0.0, |shadow_map| shadow_map.shadow_coverage(center, normal));
                    let front_color =
                        shade_face_for_shadow(orient_face_color(base_color, true), shadow);
                    let back_color =
                        shade_face_for_shadow(orient_face_color(base_color, false), shadow);
                    for index in 1..clipped_world.len() - 1 {
                        let world_triangle = [
                            clipped_world[0],
                            clipped_world[index],
                            clipped_world[index + 1],
                        ];
                        let screen_triangle = [points[0], points[index], points[index + 1]];
                        occluders.push(ProjectedOccluder {
                            points: screen_triangle,
                            depths: world_triangle.map(|point| self.camera_depth(point)),
                        });
                    }
                    let back_offset = triangle_back_face_offset(world_points, normal);
                    for (face_points, color) in [
                        (
                            world_points,
                            if facing >= 0.0 {
                                front_color
                            } else {
                                back_color
                            },
                        ),
                        (
                            world_points.map(|point| point - back_offset),
                            if facing >= 0.0 {
                                back_color
                            } else {
                                front_color
                            },
                        ),
                    ] {
                        let clipped_face = self.clip_polygon_to_camera(&face_points);
                        if clipped_face.len() < 3 {
                            continue;
                        }
                        let screen_points = clipped_face
                            .iter()
                            .map(|point| self.world_to_screen(rect, *point))
                            .collect::<Vec<_>>();
                        let depth = clipped_face
                            .iter()
                            .map(|point| self.view_depth(*point))
                            .sum::<f64>()
                            / clipped_face.len() as f64;
                        faces.push((depth, screen_points, color));
                    }
                }
            }
            if !policy.can_draw(SelectionKind::Edge) {
                continue;
            }
            for segment in geometry.segments.values() {
                let edge = edge_key(segment.endpoints[0].id, segment.endpoints[1].id);
                let selected_edge = selection.iter().any(|target| {
                    target.occurrence == occurrence
                        && target.entity == SelectedEntity::Segment(segment.id)
                });
                if coplanar_shared_edges.contains(&edge) && !selected_edge {
                    continue;
                }
                if surface_edges.contains(&edge) && !visible_surface_edges.contains(&edge) {
                    continue;
                }
                let (Some(start_local), Some(end_local)) = (
                    geometry.vertex_position(segment.endpoints[0]),
                    geometry.vertex_position(segment.endpoints[1]),
                ) else {
                    continue;
                };
                let start = resolved.world_transform.transform_point(start_local);
                let end = resolved.world_transform.transform_point(end_local);
                let Some([start, end]) = self.clip_segment_to_camera([start, end]) else {
                    continue;
                };
                let depth = (self.view_depth(start) + self.view_depth(end)) * 0.5;
                let edge_color = if selected || selected_edge {
                    Color32::from_rgb(244, 166, 77)
                } else if self.black_model_lines {
                    Color32::BLACK
                } else {
                    color32(segment.color)
                };
                segments.push((depth, [start, end], edge_color));
            }
            for target in selection.iter().filter(|target| {
                target.occurrence == occurrence
                    && matches!(&target.entity, SelectedEntity::Vertex(_))
            }) {
                let SelectedEntity::Vertex(vertex_id) = &target.entity else {
                    continue;
                };
                let Some(vertex) = geometry.vertices.get(vertex_id) else {
                    continue;
                };
                let world = resolved.world_transform.transform_point(vertex.position);
                if let Some(screen) = self.project_if_visible(rect, world) {
                    vertex_markers.push(screen);
                }
            }
        }
        faces.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (_, points, color) in faces {
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        let depth_buffer = (!segments.is_empty() && !occluders.is_empty()).then(|| {
            ProjectedDepthBuffer::from_occluders(rect, &occluders, self.orthographic.is_none())
        });
        segments.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (_, [start, end], color) in segments {
            let screen_start = self.world_to_screen(rect, start);
            let screen_end = self.world_to_screen(rect, end);
            let fragments = depth_buffer.as_ref().map_or_else(
                || vec![[screen_start, screen_end]],
                |depth_buffer| {
                    visible_line_fragments(
                        screen_start,
                        screen_end,
                        self.camera_depth(start),
                        self.camera_depth(end),
                        depth_buffer,
                        self.orthographic.is_none(),
                    )
                },
            );
            for fragment in fragments {
                painter.line_segment(fragment, Stroke::new(1.4, color));
            }
        }
        for screen in vertex_markers {
            painter.circle_filled(screen, 2.8, Color32::from_rgb(232, 51, 62));
        }
    }

    fn draw_cuboid_preview(&self, painter: &egui::Painter, rect: Rect, cuboid: CuboidPreview) {
        let corners = cuboid_corners(cuboid.origin, cuboid.dimensions);
        if corners
            .iter()
            .any(|point| !self.point_in_front_of_camera(*point))
        {
            return;
        }
        for [a, b] in cuboid_edges() {
            painter.line_segment(
                [
                    self.world_to_screen(rect, corners[a]),
                    self.world_to_screen(rect, corners[b]),
                ],
                Stroke::new(1.5, Color32::from_rgb(235, 105, 70)),
            );
        }
    }

    fn draw_point_preview(&self, painter: &egui::Painter, rect: Rect, point: Vec3) {
        let Some(position) = self.project_if_visible(rect, point) else {
            return;
        };
        let color = Color32::from_rgb(235, 105, 70);
        painter.circle_filled(position, 4.0, Color32::BLACK);
        painter.circle_filled(position, 2.5, color);
        painter.line_segment(
            [
                position - Vec2::new(7.0, 0.0),
                position + Vec2::new(7.0, 0.0),
            ],
            Stroke::new(1.0, color),
        );
        painter.line_segment(
            [
                position - Vec2::new(0.0, 7.0),
                position + Vec2::new(0.0, 7.0),
            ],
            Stroke::new(1.0, color),
        );
    }

    fn draw_paint_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        edges: &[[Vec3; 2]],
        triangles: &[[Vec3; 3]],
        color: meshcraft_core::document::Rgba8,
    ) {
        let color =
            Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha);
        for triangle in triangles {
            if triangle
                .iter()
                .any(|point| !self.point_in_front_of_camera(*point))
            {
                continue;
            }
            let points = triangle
                .iter()
                .map(|point| self.world_to_screen(rect, *point))
                .collect();
            painter.add(Shape::convex_polygon(
                points,
                color.gamma_multiply(0.68),
                Stroke::new(1.0, color),
            ));
        }
        for [start, end] in edges {
            if !self.point_in_front_of_camera(*start) || !self.point_in_front_of_camera(*end) {
                continue;
            }
            painter.line_segment(
                [
                    self.world_to_screen(rect, *start),
                    self.world_to_screen(rect, *end),
                ],
                Stroke::new(3.0, color),
            );
        }
    }

    fn draw_line_preview(&self, painter: &egui::Painter, rect: Rect, start: Vec3, end: Vec3) {
        if !self.point_in_front_of_camera(start) || !self.point_in_front_of_camera(end) {
            return;
        }
        painter.line_segment(
            [
                self.world_to_screen(rect, start),
                self.world_to_screen(rect, end),
            ],
            Stroke::new(1.8, Color32::from_rgb(235, 105, 70)),
        );
    }

    fn draw_transform_preview(&self, painter: &egui::Painter, rect: Rect, edges: &[[Vec3; 2]]) {
        self.draw_translated_transform_preview(painter, rect, edges, Vec3::ZERO);
    }

    fn draw_translated_transform_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        edges: &[[Vec3; 2]],
        translation: Vec3,
    ) {
        let color = Color32::from_rgb(235, 105, 70);
        for [start, end] in edges {
            let start = *start + translation;
            let end = *end + translation;
            if !self.point_in_front_of_camera(start) || !self.point_in_front_of_camera(end) {
                continue;
            }
            painter.line_segment(
                [
                    self.world_to_screen(rect, start),
                    self.world_to_screen(rect, end),
                ],
                Stroke::new(1.8, color),
            );
        }
    }

    fn draw_affine_transform_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        edges: &[[Vec3; 2]],
        transform: Affine3,
    ) {
        let color = Color32::from_rgb(235, 105, 70);
        for [start, end] in edges {
            let start = transform.transform_point(*start);
            let end = transform.transform_point(*end);
            if !self.point_in_front_of_camera(start) || !self.point_in_front_of_camera(end) {
                continue;
            }
            painter.line_segment(
                [
                    self.world_to_screen(rect, start),
                    self.world_to_screen(rect, end),
                ],
                Stroke::new(1.8, color),
            );
        }
    }

    fn draw_push_pull_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        edges: &[[Vec3; 2]],
        triangles: &[([Vec3; 3], meshcraft_core::document::Rgba8)],
    ) {
        for (triangle, color) in triangles {
            if triangle
                .iter()
                .any(|point| !self.point_in_front_of_camera(*point))
            {
                continue;
            }
            let points = triangle
                .iter()
                .map(|point| self.world_to_screen(rect, *point))
                .collect();
            painter.add(Shape::convex_polygon(
                points,
                Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, 96),
                Stroke::new(0.8, Color32::from_rgb(245, 130, 58)),
            ));
        }
        self.draw_transform_preview(painter, rect, edges);
    }

    fn draw_planar_shape_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        outline: &[Vec3],
        closed: bool,
        measurements: &[ShapeMeasurement],
    ) {
        if outline.len() < 2
            || outline
                .iter()
                .any(|point| !self.point_in_front_of_camera(*point))
        {
            return;
        }
        let points = outline
            .iter()
            .map(|point| self.world_to_screen(rect, *point))
            .collect::<Vec<_>>();
        if closed && points.len() >= 3 {
            painter.add(Shape::convex_polygon(
                points.clone(),
                Color32::from_rgba_unmultiplied(235, 105, 70, 36),
                Stroke::NONE,
            ));
        }
        for pair in points.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                Stroke::new(1.8, Color32::from_rgb(235, 105, 70)),
            );
        }
        if closed {
            painter.line_segment(
                [*points.last().unwrap(), points[0]],
                Stroke::new(1.8, Color32::from_rgb(235, 105, 70)),
            );
        }
        if !measurements.is_empty() {
            let labels = measurements
                .iter()
                .map(|measurement| format!("{} {:.2}", measurement.label, measurement.value))
                .collect::<Vec<_>>()
                .join("  ");
            painter.text(
                *points.last().unwrap() + Vec2::new(10.0, 12.0),
                egui::Align2::LEFT_TOP,
                labels,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }

    fn draw_polyline_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        outline: &[Vec3],
        closed: bool,
    ) {
        if outline.len() < 2
            || outline
                .iter()
                .any(|point| !self.point_in_front_of_camera(*point))
        {
            return;
        }
        let points = outline
            .iter()
            .map(|point| self.world_to_screen(rect, *point))
            .collect::<Vec<_>>();
        let stroke = Stroke::new(1.8, Color32::from_rgb(235, 105, 70));
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], stroke);
        }
        if closed {
            painter.line_segment([*points.last().unwrap(), points[0]], stroke);
        }
    }
}

fn bounds_corners(bounds: meshcraft_core::geometry::Aabb3) -> [Vec3; 8] {
    let min = bounds.min;
    let max = bounds.max;
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

fn color32(color: Rgba8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha)
}

fn axis_guide_colors(preview: bool) -> [Color32; 3] {
    if preview {
        [
            Color32::from_rgb(227, 126, 126),
            Color32::from_rgb(104, 218, 150),
            Color32::from_rgb(120, 219, 255),
        ]
    } else {
        [
            Color32::from_rgb(202, 105, 105),
            Color32::from_rgb(100, 160, 115),
            Color32::from_rgb(100, 130, 190),
        ]
    }
}

pub(crate) fn cursor_axes(
    surface_normal: Vec3,
    segment_axis: Option<Vec3>,
    ground_normal: Vec3,
) -> Option<[Vec3; 3]> {
    if let Some(blue) = segment_axis.and_then(|axis| axis.try_normalized(1.0e-12)) {
        let ground = ground_normal.try_normalized(1.0e-12)?;
        let red = ground.cross(blue).try_normalized(1.0e-10).or_else(|| {
            let reference = if ground.dot(Vec3::X).abs() < 0.9 {
                Vec3::X
            } else {
                Vec3::Z
            };
            let ground_tangent =
                (reference - ground * reference.dot(ground)).try_normalized(1.0e-12)?;
            (ground_tangent - blue * ground_tangent.dot(blue)).try_normalized(1.0e-12)
        })?;
        let green = blue.cross(red).try_normalized(1.0e-12)?;
        return Some([red, green, blue]);
    }
    let guide =
        ConstructionGuide::at_point(ConstructionGuideKind::Axis, Vec3::ZERO, surface_normal)?;
    Some([guide.axis_u, guide.axis_v, guide.axis_w])
}

pub(crate) fn snapped_segment_axis(
    document: &Document,
    scope: SnapScope<'_>,
    point: Vec3,
) -> Option<Vec3> {
    let occurrences = document.scene_geometry_refs().ok()?;
    let mut nearest: Option<(f64, Vec3)> = None;
    for occurrence in occurrences {
        if !scope.contains(document, &occurrence) {
            continue;
        }
        let Some(resolved) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        for segment in geometry.segments.values() {
            let (Some(start), Some(end)) = (
                geometry.vertex_position(segment.endpoints[0]),
                geometry.vertex_position(segment.endpoints[1]),
            ) else {
                continue;
            };
            let start = resolved.world_transform.transform_point(start);
            let end = resolved.world_transform.transform_point(end);
            let Some(axis) = (end - start).try_normalized(1.0e-12) else {
                continue;
            };
            let distance = point.distance(closest_point_on_segment(point, start, end));
            if nearest.is_none_or(|(current, _)| distance < current) {
                nearest = Some((distance, axis));
            }
        }
    }
    nearest.map(|(_, axis)| axis)
}

fn triangle_back_face_offset(points: [Vec3; 3], normal: Vec3) -> Vec3 {
    let scale = points[0]
        .distance(points[1])
        .max(points[1].distance(points[2]))
        .max(points[2].distance(points[0]));
    normal * (scale * 1.0e-5)
}

fn lighter_axis_color(color: Color32) -> Color32 {
    let lighten = |channel: u8| channel + ((u16::from(255 - channel) * 40) / 100) as u8;
    Color32::from_rgba_unmultiplied(
        lighten(color.r()),
        lighten(color.g()),
        lighten(color.b()),
        color.a(),
    )
}

fn orient_face_color(color: Color32, front_facing: bool) -> Color32 {
    if front_facing {
        return color;
    }
    let [red, green, blue, alpha] = color.to_array();
    let tint = |channel: u8, factor: f32| (f32::from(channel) * factor).round() as u8;
    Color32::from_rgba_premultiplied(tint(red, 0.76), tint(green, 0.80), tint(blue, 0.90), alpha)
}

fn shade_face_for_shadow(color: Color32, coverage: f32) -> Color32 {
    let factor = 1.0 - coverage.clamp(0.0, 1.0) * 0.45;
    let [red, green, blue, alpha] = color.to_array();
    let shade = |channel: u8| (f32::from(channel) * factor).round() as u8;
    Color32::from_rgba_premultiplied(shade(red), shade(green), shade(blue), alpha)
}

fn collect_world_triangles(
    document: &Document,
    scope: SnapScope<'_>,
    policy: &SelectionDisplayPolicy,
) -> Vec<WorldTriangle> {
    if !policy.can_draw(SelectionKind::Face) || policy.state(SelectionKind::Face).wireframe {
        return Vec::new();
    }
    let Ok(occurrences) = document.scene_geometry_refs() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for occurrence in occurrences {
        if !scope.contains(document, &occurrence)
            || !policy.occurrence_visible(document, &occurrence)
        {
            continue;
        }
        let Some(resolved) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        for triangle in geometry.triangles.values() {
            let [Some(a), Some(b), Some(c)] = triangle
                .vertices
                .map(|reference| geometry.vertex_position(reference))
            else {
                continue;
            };
            result.push(WorldTriangle {
                points: [a, b, c].map(|point| resolved.world_transform.transform_point(point)),
            });
        }
    }
    result
}

fn edge_key(first: VertexId, second: VertexId) -> (VertexId, VertexId) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn coplanar_triangle_winding_flips(
    triangles: &[(TriangleId, [VertexId; 3], [Vec3; 3])],
) -> BTreeSet<TriangleId> {
    let normals = triangles
        .iter()
        .map(|(_, _, points)| {
            (points[1] - points[0])
                .cross(points[2] - points[0])
                .try_normalized(1.0e-12)
        })
        .collect::<Vec<_>>();
    let mut points = triangles.iter().flat_map(|(_, _, points)| points).copied();
    let Some(first_point) = points.next() else {
        return BTreeSet::new();
    };
    let (minimum, maximum) =
        points.fold((first_point, first_point), |(minimum, maximum), point| {
            (
                Vec3::new(
                    minimum.x.min(point.x),
                    minimum.y.min(point.y),
                    minimum.z.min(point.z),
                ),
                Vec3::new(
                    maximum.x.max(point.x),
                    maximum.y.max(point.y),
                    maximum.z.max(point.z),
                ),
            )
        });
    let coordinate_scale = (maximum - minimum).length().max(1.0);
    let position_tolerance = coordinate_scale * 1.0e-8;
    let position_key = |point: Vec3| {
        (
            ((point.x - minimum.x) / position_tolerance).round() as i64,
            ((point.y - minimum.y) / position_tolerance).round() as i64,
            ((point.z - minimum.z) / position_tolerance).round() as i64,
        )
    };
    let position_edge_key = |first: Vec3, second: Vec3| {
        let (first, second) = (position_key(first), position_key(second));
        if first <= second {
            (first, second)
        } else {
            (second, first)
        }
    };
    let mut edge_users = BTreeMap::<(VertexId, VertexId), Vec<usize>>::new();
    let mut position_edge_users = BTreeMap::<((i64, i64, i64), (i64, i64, i64)), Vec<usize>>::new();
    for (index, (_, vertices, points)) in triangles.iter().enumerate() {
        for [first, second] in [[0, 1], [1, 2], [2, 0]] {
            edge_users
                .entry(edge_key(vertices[first], vertices[second]))
                .or_default()
                .push(index);
            position_edge_users
                .entry(position_edge_key(points[first], points[second]))
                .or_default()
                .push(index);
        }
    }
    let plane_tolerance = coordinate_scale * 1.0e-8;
    let mut signs = vec![None::<i8>; triangles.len()];
    let mut flipped = BTreeSet::new();
    for seed in 0..triangles.len() {
        if signs[seed].is_some() || normals[seed].is_none() {
            continue;
        }
        signs[seed] = Some(1);
        let mut pending = vec![seed];
        while let Some(current) = pending.pop() {
            let Some(current_normal) = normals[current] else {
                continue;
            };
            let (triangle_id, vertices, points) = triangles[current];
            if signs[current] == Some(-1) {
                flipped.insert(triangle_id);
            }
            for [first, second] in [[0, 1], [1, 2], [2, 0]] {
                let mut neighbors = BTreeSet::new();
                if let Some(users) = edge_users.get(&edge_key(vertices[first], vertices[second])) {
                    neighbors.extend(users.iter().copied());
                }
                if let Some(users) =
                    position_edge_users.get(&position_edge_key(points[first], points[second]))
                {
                    neighbors.extend(users.iter().copied());
                }
                for neighbor in neighbors {
                    if neighbor == current {
                        continue;
                    }
                    let Some(neighbor_normal) = normals[neighbor] else {
                        continue;
                    };
                    let alignment = current_normal.dot(neighbor_normal);
                    if alignment.abs() < 1.0 - 1.0e-8
                        || triangles[neighbor].2.iter().any(|point| {
                            current_normal.dot(*point - points[0]).abs() > plane_tolerance
                        })
                    {
                        continue;
                    }
                    let relation = if alignment < 0.0 { -1 } else { 1 };
                    let candidate = signs[current].unwrap_or(1) * relation;
                    if signs[neighbor].is_none() {
                        signs[neighbor] = Some(candidate);
                        pending.push(neighbor);
                    }
                }
            }
        }
    }
    flipped
}

fn coplanar_shared_triangle_edges(
    triangles: &[(TriangleId, [VertexId; 3], [Vec3; 3])],
) -> BTreeSet<(VertexId, VertexId)> {
    if triangles.len() < 2 {
        return BTreeSet::new();
    }
    let mut points = triangles.iter().flat_map(|(_, _, points)| points).copied();
    let Some(first_point) = points.next() else {
        return BTreeSet::new();
    };
    let (minimum, maximum) =
        points.fold((first_point, first_point), |(minimum, maximum), point| {
            (
                Vec3::new(
                    minimum.x.min(point.x),
                    minimum.y.min(point.y),
                    minimum.z.min(point.z),
                ),
                Vec3::new(
                    maximum.x.max(point.x),
                    maximum.y.max(point.y),
                    maximum.z.max(point.z),
                ),
            )
        });
    let scale = (maximum - minimum).length().max(1.0);
    let tolerance = scale * 1.0e-8;
    let position_key = |point: Vec3| {
        (
            ((point.x - minimum.x) / tolerance).round() as i64,
            ((point.y - minimum.y) / tolerance).round() as i64,
            ((point.z - minimum.z) / tolerance).round() as i64,
        )
    };
    let position_edge_key = |first: Vec3, second: Vec3| {
        let (first, second) = (position_key(first), position_key(second));
        if first <= second {
            (first, second)
        } else {
            (second, first)
        }
    };
    let normals = triangles
        .iter()
        .map(|(_, _, points)| {
            (points[1] - points[0])
                .cross(points[2] - points[0])
                .try_normalized(1.0e-12)
        })
        .collect::<Vec<_>>();
    let mut edge_users =
        BTreeMap::<((i64, i64, i64), (i64, i64, i64)), Vec<(usize, (VertexId, VertexId))>>::new();
    for (index, (_, vertices, triangle_points)) in triangles.iter().enumerate() {
        for [first, second] in [[0, 1], [1, 2], [2, 0]] {
            edge_users
                .entry(position_edge_key(
                    triangle_points[first],
                    triangle_points[second],
                ))
                .or_default()
                .push((index, edge_key(vertices[first], vertices[second])));
        }
    }

    let mut hidden = BTreeSet::new();
    for users in edge_users.values() {
        for first_index in 0..users.len() {
            for second_index in first_index + 1..users.len() {
                let (first_triangle, first_edge) = users[first_index];
                let (second_triangle, second_edge) = users[second_index];
                if first_triangle == second_triangle {
                    continue;
                }
                let (Some(first_normal), Some(second_normal)) =
                    (normals[first_triangle], normals[second_triangle])
                else {
                    continue;
                };
                if first_normal.dot(second_normal).abs() < 1.0 - 1.0e-8
                    || triangles[second_triangle].2.iter().any(|point| {
                        first_normal
                            .dot(*point - triangles[first_triangle].2[0])
                            .abs()
                            > tolerance
                    })
                {
                    continue;
                }
                hidden.insert(first_edge);
                hidden.insert(second_edge);
            }
        }
    }
    hidden
}

fn screen_barycentric(point: Pos2, triangle: [Pos2; 3]) -> Option<[f64; 3]> {
    let [a, b, c] = triangle;
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let denominator = f64::from(ab.x) * f64::from(ac.y) - f64::from(ab.y) * f64::from(ac.x);
    if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
        return None;
    }
    let second =
        (f64::from(ap.x) * f64::from(ac.y) - f64::from(ap.y) * f64::from(ac.x)) / denominator;
    let third =
        (f64::from(ab.x) * f64::from(ap.y) - f64::from(ab.y) * f64::from(ap.x)) / denominator;
    let first = 1.0 - second - third;
    (first >= -1.0e-7 && second >= -1.0e-7 && third >= -1.0e-7).then_some([first, second, third])
}

fn perspective_correct_depth(weights: [f64; 3], depths: [f64; 3], perspective: bool) -> f64 {
    if perspective {
        let reciprocal = weights[0] / depths[0] + weights[1] / depths[1] + weights[2] / depths[2];
        1.0 / reciprocal
    } else {
        weights[0] * depths[0] + weights[1] * depths[1] + weights[2] * depths[2]
    }
}

fn visible_line_fragments(
    start: Pos2,
    end: Pos2,
    start_depth: f64,
    end_depth: f64,
    depth_buffer: &ProjectedDepthBuffer,
    perspective: bool,
) -> Vec<[Pos2; 2]> {
    if !start_depth.is_finite()
        || !end_depth.is_finite()
        || (perspective && (start_depth <= 0.0 || end_depth <= 0.0))
    {
        return Vec::new();
    }
    let screen_delta = end - start;
    let steps = (screen_delta.length() / depth_buffer.cell_size)
        .ceil()
        .clamp(1.0, 8192.0) as usize;
    let screen_at = |step: usize| start + screen_delta * (step as f32 / steps as f32);
    let line_depth_at = |fraction: f64| {
        if perspective {
            1.0 / ((1.0 - fraction) / start_depth + fraction / end_depth)
        } else {
            start_depth + (end_depth - start_depth) * fraction
        }
    };
    let is_visible = |step: usize| {
        let fraction = step as f64 / steps as f64;
        let point = screen_at(step);
        let line_depth = line_depth_at(fraction);
        let nearest_surface = depth_buffer.depth_at(point);
        nearest_surface.is_none_or(|surface_depth| {
            line_depth <= surface_depth + surface_depth.abs().max(1.0) * 1.0e-6
        })
    };

    let mut visible = Vec::new();
    let mut run_start = None;
    for step in 0..=steps {
        if is_visible(step) {
            run_start.get_or_insert(step);
        } else if let Some(first) = run_start.take() {
            let last = step.saturating_sub(1);
            if first < last {
                visible.push([screen_at(first), screen_at(last)]);
            }
        }
    }
    if let Some(first) = run_start
        && first < steps
    {
        visible.push([screen_at(first), screen_at(steps)]);
    }
    visible
}

fn cuboid_corners(origin: Vec3, dimensions: Vec3) -> [Vec3; 8] {
    [
        origin,
        origin + Vec3::new(dimensions.x, 0.0, 0.0),
        origin + Vec3::new(dimensions.x, dimensions.y, 0.0),
        origin + Vec3::new(0.0, dimensions.y, 0.0),
        origin + Vec3::new(0.0, 0.0, dimensions.z),
        origin + Vec3::new(dimensions.x, 0.0, dimensions.z),
        origin + dimensions,
        origin + Vec3::new(0.0, dimensions.y, dimensions.z),
    ]
}

fn cuboid_edges() -> [[usize; 2]; 12] {
    [
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
    ]
}

fn point_in_triangle(point: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let cross = |first: Vec2, second: Vec2| first.x * second.y - first.y * second.x;
    let ab = point - a;
    let bc = point - b;
    let ca = point - c;
    let d1 = cross(b - a, ab);
    let d2 = cross(c - b, bc);
    let d3 = cross(a - c, ca);
    (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0)
}

fn height_in_triangle_xz(x: f64, z: f64, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let denominator = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
        return None;
    }
    let weight_a = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / denominator;
    let weight_b = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / denominator;
    let weight_c = 1.0 - weight_a - weight_b;
    let tolerance = 1.0e-9;
    if [weight_a, weight_b, weight_c]
        .into_iter()
        .all(|weight| (-tolerance..=1.0 + tolerance).contains(&weight))
    {
        Some(weight_a * a.y + weight_b * b.y + weight_c * c.y)
    } else {
        None
    }
}

fn read_camera_vector(value: Option<&Value>, default: Vec3) -> Option<Vec3> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Some(default);
    };
    let fields = value.as_object()?;
    let component = |name: &str| fields.get(name).map_or(Some(0.0), Value::as_f64);
    let vector = Vec3::new(component("x")?, component("y")?, component("z")?);
    vector.is_finite().then_some(vector)
}

fn read_camera_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

fn distance_to_segment_squared(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let direction = end - start;
    let length_squared = direction.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance_sq(start);
    }
    let t = ((point - start).dot(direction) / length_squared).clamp(0.0, 1.0);
    point.distance_sq(start + direction * t)
}

fn segment_intersects_rect(start: Pos2, end: Pos2, rect: Rect) -> bool {
    if rect.contains(start) || rect.contains(end) {
        return true;
    }
    let edges = [
        (rect.left_top(), rect.right_top()),
        (rect.right_top(), rect.right_bottom()),
        (rect.right_bottom(), rect.left_bottom()),
        (rect.left_bottom(), rect.left_top()),
    ];
    edges
        .into_iter()
        .any(|(edge_start, edge_end)| segments_intersect(start, end, edge_start, edge_end))
}

fn segments_intersect(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let cross = |first: Vec2, second: Vec2| first.x * second.y - first.y * second.x;
    let on_segment = |point: Pos2, start: Pos2, end: Pos2| {
        point.x >= start.x.min(end.x) - 1.0e-4
            && point.x <= start.x.max(end.x) + 1.0e-4
            && point.y >= start.y.min(end.y) - 1.0e-4
            && point.y <= start.y.max(end.y) + 1.0e-4
    };
    let ab_c = cross(b - a, c - a);
    let ab_d = cross(b - a, d - a);
    let cd_a = cross(d - c, a - c);
    let cd_b = cross(d - c, b - c);
    if (ab_c > 1.0e-5 && ab_d < -1.0e-5 || ab_c < -1.0e-5 && ab_d > 1.0e-5)
        && (cd_a > 1.0e-5 && cd_b < -1.0e-5 || cd_a < -1.0e-5 && cd_b > 1.0e-5)
    {
        return true;
    }
    (ab_c.abs() <= 1.0e-5 && on_segment(c, a, b))
        || (ab_d.abs() <= 1.0e-5 && on_segment(d, a, b))
        || (cd_a.abs() <= 1.0e-5 && on_segment(a, c, d))
        || (cd_b.abs() <= 1.0e-5 && on_segment(b, c, d))
}

fn point_in_aabb(point: Vec3, bounds: meshcraft_core::geometry::Aabb3) -> bool {
    point.x >= bounds.min.x
        && point.x <= bounds.max.x
        && point.y >= bounds.min.y
        && point.y <= bounds.max.y
        && point.z >= bounds.min.z
        && point.z <= bounds.max.z
}

fn aabb_intersects(
    first: meshcraft_core::geometry::Aabb3,
    second: meshcraft_core::geometry::Aabb3,
) -> bool {
    first.min.x <= second.max.x
        && first.max.x >= second.min.x
        && first.min.y <= second.max.y
        && first.max.y >= second.min.y
        && first.min.z <= second.max.z
        && first.max.z >= second.min.z
}

fn segment_intersects_aabb(
    start: Vec3,
    end: Vec3,
    bounds: meshcraft_core::geometry::Aabb3,
) -> bool {
    let direction = end - start;
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for (origin, delta, min, max) in [
        (start.x, direction.x, bounds.min.x, bounds.max.x),
        (start.y, direction.y, bounds.min.y, bounds.max.y),
        (start.z, direction.z, bounds.min.z, bounds.max.z),
    ] {
        if delta.abs() <= f64::EPSILON {
            if origin < min || origin > max {
                return false;
            }
            continue;
        }
        let first = (min - origin) / delta;
        let second = (max - origin) / delta;
        low = low.max(first.min(second));
        high = high.min(first.max(second));
        if low > high {
            return false;
        }
    }
    true
}

fn consider_selection_hit(
    nearest: &mut Option<(f32, f64, SelectionTarget)>,
    distance: f32,
    depth: f64,
    target: SelectionTarget,
) {
    if nearest
        .as_ref()
        .is_none_or(|(current_distance, current_depth, _)| {
            distance < *current_distance
                || (distance == *current_distance && depth > *current_depth)
        })
    {
        *nearest = Some((distance, depth, target));
    }
}

fn closest_point_on_ray_line(
    ray_origin: Vec3,
    ray_direction: Vec3,
    line_origin: Vec3,
    line_direction: Vec3,
) -> Option<(Vec3, f64)> {
    let offset = ray_origin - line_origin;
    let dot = ray_direction.dot(line_direction);
    let ray_offset = ray_direction.dot(offset);
    let line_offset = line_direction.dot(offset);
    let denominator = 1.0 - dot * dot;
    if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
        return None;
    }
    let mut ray_distance = (dot * line_offset - ray_offset) / denominator;
    let mut line_distance = (line_offset - dot * ray_offset) / denominator;
    if ray_distance < 0.0 {
        ray_distance = 0.0;
        line_distance = line_offset;
    }
    let closest_line = line_origin + line_direction * line_distance;
    (line_distance.is_finite() && ray_distance.is_finite()).then_some((closest_line, line_distance))
}

fn ray_triangle_distance(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let edge_ab = b - a;
    let edge_ac = c - a;
    let perpendicular = direction.cross(edge_ac);
    let determinant = edge_ab.dot(perpendicular);
    if !determinant.is_finite() || determinant.abs() <= 1.0e-12 {
        return None;
    }
    let inverse_determinant = 1.0 / determinant;
    let from_a = origin - a;
    let u = from_a.dot(perpendicular) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let across = from_a.cross(edge_ab);
    let v = direction.dot(across) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_ac.dot(across) * inverse_determinant;
    (distance.is_finite() && distance >= 0.0).then_some(distance)
}

#[cfg(not(target_arch = "wasm32"))]
fn consider_ray_selection_hit(
    nearest: &mut Option<(f64, u8, SelectionTarget)>,
    depth: f64,
    priority: u8,
    target: SelectionTarget,
) {
    if !depth.is_finite() || depth < 0.0 {
        return;
    }
    if nearest
        .as_ref()
        .is_none_or(|(best_depth, best_priority, _)| {
            depth < *best_depth - 1.0e-6
                || ((depth - *best_depth).abs() <= 1.0e-6 && priority < *best_priority)
        })
    {
        *nearest = Some((depth, priority, target));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn consider_ray_snap(
    nearest: &mut Option<(f64, u8, SnapResult)>,
    depth: f64,
    priority: u8,
    snap: SnapResult,
) {
    if !depth.is_finite() || depth < 0.0 {
        return;
    }
    if nearest
        .as_ref()
        .is_none_or(|(best_depth, best_priority, best)| {
            priority < *best_priority
                || (priority == *best_priority
                    && (snap.screen_distance < best.screen_distance
                        || (snap.screen_distance == best.screen_distance && depth < *best_depth)))
        })
    {
        *nearest = Some((depth, priority, snap));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ray_aabb_distance(
    origin: Vec3,
    direction: Vec3,
    bounds: meshcraft_core::geometry::Aabb3,
) -> Option<f64> {
    let mut near = 0.0_f64;
    let mut far = f64::INFINITY;
    for (origin, direction, min, max) in [
        (origin.x, direction.x, bounds.min.x, bounds.max.x),
        (origin.y, direction.y, bounds.min.y, bounds.max.y),
        (origin.z, direction.z, bounds.min.z, bounds.max.z),
    ] {
        if direction.abs() <= 1.0e-12 {
            if origin < min || origin > max {
                return None;
            }
            continue;
        }
        let first = (min - origin) / direction;
        let second = (max - origin) / direction;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if far < near {
            return None;
        }
    }
    (far >= 0.0 && near.is_finite()).then_some(near.max(0.0))
}

#[cfg(not(target_arch = "wasm32"))]
fn ray_segment_distance(
    ray_origin: Vec3,
    ray_direction: Vec3,
    start: Vec3,
    end: Vec3,
) -> Option<(f64, f64)> {
    let segment = end - start;
    let length_squared = segment.dot(segment);
    if !length_squared.is_finite() {
        return None;
    }
    if length_squared <= 1.0e-24 {
        let depth = (start - ray_origin).dot(ray_direction);
        if depth < 0.0 {
            return None;
        }
        return Some((
            depth,
            (start - (ray_origin + ray_direction * depth)).length(),
        ));
    }
    let from_start = ray_origin - start;
    let ray_segment_dot = ray_direction.dot(segment);
    let ray_offset = ray_direction.dot(from_start);
    let segment_offset = segment.dot(from_start);
    let denominator = length_squared - ray_segment_dot * ray_segment_dot;
    let mut fraction = if denominator.abs() > 1.0e-12 {
        ((segment_offset - ray_segment_dot * ray_offset) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut depth = (start + segment * fraction - ray_origin).dot(ray_direction);
    if depth < 0.0 {
        depth = 0.0;
        fraction = ((ray_origin - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    }
    let segment_point = start + segment * fraction;
    let ray_point = ray_origin + ray_direction * depth;
    let miss_distance = (segment_point - ray_point).length();
    (depth.is_finite() && miss_distance.is_finite()).then_some((depth, miss_distance))
}

fn closest_point_on_segment(point: Vec3, start: Vec3, end: Vec3) -> Vec3 {
    let direction = end - start;
    let length_squared = direction.dot(direction);
    if length_squared <= 1.0e-24 {
        return start;
    }
    let fraction = ((point - start).dot(direction) / length_squared).clamp(0.0, 1.0);
    start + direction * fraction
}

#[cfg(test)]
mod tests {
    use super::{
        ConstructionGuide, ConstructionGuideKind, DEFAULT_ELEVATION, DEFAULT_PERSPECTIVE_DISTANCE,
        GridNormalAxis, MAX_ELEVATION, MIN_ELEVATION, OrthoView, SelectedEntity,
        SelectionDisplayPolicy, SelectionKind, ShadowMap, SnapKind, SnapOptions, SnapResult,
        SnapScope, SnapVisibility, Viewport, WalkthroughCamera, WalkthroughTuning, WorldTriangle,
        axis_guide_colors, coplanar_shared_triangle_edges, coplanar_triangle_winding_flips,
        cuboid_corners, cursor_axes, edge_key, height_in_triangle_xz, lighter_axis_color,
        orient_face_color, point_in_triangle, shade_face_for_shadow, shadow_light_direction,
        triangle_back_face_offset, visible_line_fragments,
    };
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
    use meshcraft_core::document::{Document, TriangleId, VertexId};
    use meshcraft_core::geometry::Vec3;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn back_faces_are_darker_and_slightly_bluer_than_front_faces() {
        let front = egui::Color32::from_rgb(220, 220, 220);
        let back = orient_face_color(front, false);
        assert!(back.r() < front.r());
        assert!(back.g() < front.g());
        assert!(back.b() < front.b());
        assert!(back.b() > back.r());
        assert_eq!(front, orient_face_color(front, true));
        assert_eq!(back, shade_face_for_shadow(back, 0.0));
        let translucent = egui::Color32::from_rgba_unmultiplied(220, 180, 140, 96);
        assert_eq!(translucent.a(), orient_face_color(translucent, false).a());
        assert_eq!(translucent.a(), shade_face_for_shadow(translucent, 1.0).a());
    }

    #[test]
    fn perspective_primitives_clip_to_near_and_far_camera_planes() {
        let viewport = Viewport {
            distance: 10.0,
            ..Viewport::default()
        };
        let camera = viewport.camera_position();
        let forward = -viewport.camera_direction();
        let point_at_depth = |depth| camera + forward * depth;

        let clipped = viewport
            .clip_segment_to_camera([point_at_depth(1.0), point_at_depth(900.0)])
            .expect("segment crosses the visible depth range");
        assert!((viewport.camera_depth(clipped[0]) - 1.0).abs() < 1.0e-9);
        assert!((viewport.camera_depth(clipped[1]) - viewport.far_plane).abs() < 1.0e-9);
        assert!(
            viewport
                .clip_segment_to_camera([point_at_depth(600.0), point_at_depth(900.0)])
                .is_none()
        );

        let clipped_triangle = viewport.clip_polygon_to_camera(&[
            point_at_depth(0.01),
            point_at_depth(2.0) + Vec3::X,
            point_at_depth(2.0) - Vec3::X,
        ]);
        assert_eq!(4, clipped_triangle.len());
        assert!(clipped_triangle.iter().all(|point| {
            let depth = viewport.camera_depth(*point);
            depth >= viewport.near_plane - 1.0e-9 && depth <= viewport.far_plane + 1.0e-9
        }));
    }

    #[test]
    fn lines_hidden_behind_faces_are_split_at_the_occlusion_boundary() {
        let occluder = super::ProjectedOccluder {
            points: [
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 0.0),
                Pos2::new(0.0, 10.0),
            ],
            depths: [10.0; 3],
        };
        let depth_buffer = super::ProjectedDepthBuffer::from_occluders(
            Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0)),
            &[occluder],
            false,
        );
        assert!(
            visible_line_fragments(
                Pos2::new(1.0, 2.0),
                Pos2::new(5.0, 2.0),
                20.0,
                20.0,
                &depth_buffer,
                false,
            )
            .is_empty()
        );
        let fragments = visible_line_fragments(
            Pos2::new(1.0, 2.0),
            Pos2::new(5.0, 2.0),
            5.0,
            20.0,
            &depth_buffer,
            false,
        );
        assert_eq!(1, fragments.len());
        assert!(fragments[0][0].x < fragments[0][1].x);
        assert!(fragments[0][1].x < 4.0);
    }

    #[test]
    fn cuboid_face_triangles_receive_consistent_shadowing() {
        let corners = cuboid_corners(Vec3::ZERO, Vec3::new(4.0, 4.0, 4.0));
        let indices = [
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        let triangles = indices.map(|[a, b, c]| WorldTriangle {
            points: [corners[a], corners[b], corners[c]],
        });
        let shadow_map = ShadowMap::build(&triangles).expect("cuboid builds a shadow map");
        for pair in triangles.as_chunks::<2>().0 {
            let normal = (pair[0].points[1] - pair[0].points[0])
                .cross(pair[0].points[2] - pair[0].points[0])
                .try_normalized(1.0e-12)
                .expect("face triangle has a normal");
            let coverage = [pair[0], pair[1]].map(|triangle| {
                let [a, b, c] = triangle.points;
                let center = (a + b + c) / 3.0;
                shadow_map.shadow_coverage(center, normal)
            });
            assert!(
                (coverage[0] - coverage[1]).abs() <= 0.15,
                "coplanar triangles received different shadow coverage: {coverage:?}"
            );
        }
    }

    #[test]
    fn coplanar_triangles_with_reversed_winding_share_one_render_orientation() {
        use meshcraft_core::document::{TriangleId, VertexId};

        let triangles = [
            (
                TriangleId(1),
                [VertexId(1), VertexId(2), VertexId(3)],
                [Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y],
            ),
            (
                TriangleId(2),
                [VertexId(1), VertexId(4), VertexId(3)],
                [Vec3::ZERO, Vec3::Y, Vec3::X + Vec3::Y],
            ),
        ];

        let flips = coplanar_triangle_winding_flips(&triangles);
        assert!(!flips.contains(&TriangleId(1)));
        assert!(flips.contains(&TriangleId(2)));
    }

    #[test]
    fn coplanar_triangles_with_coincident_vertices_share_one_render_orientation() {
        use meshcraft_core::document::{TriangleId, VertexId};

        let triangles = [
            (
                TriangleId(1),
                [VertexId(1), VertexId(2), VertexId(3)],
                [Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y],
            ),
            (
                TriangleId(2),
                [VertexId(4), VertexId(5), VertexId(6)],
                [Vec3::ZERO, Vec3::Y, Vec3::X + Vec3::Y],
            ),
        ];

        let flips = coplanar_triangle_winding_flips(&triangles);
        assert!(!flips.contains(&TriangleId(1)));
        assert!(flips.contains(&TriangleId(2)));
    }

    #[test]
    fn axis_guides_use_red_green_blue_for_tangent_tangent_and_normal_axes() {
        let guide = ConstructionGuide::at_point(ConstructionGuideKind::Axis, Vec3::ZERO, Vec3::Y)
            .expect("horizontal face has an axis guide");
        assert_eq!(Vec3::Y, guide.axis_w);
        assert!(guide.axis_u.x.abs() > 0.999);
        assert!(guide.axis_v.z.abs() > 0.999);

        let colors = axis_guide_colors(false);
        assert!(colors[0].r() > colors[0].g() && colors[0].r() > colors[0].b());
        assert!(colors[1].g() > colors[1].r() && colors[1].g() > colors[1].b());
        assert!(colors[2].b() > colors[2].r() && colors[2].b() > colors[2].g());
        let negative = lighter_axis_color(colors[0]);
        assert!(negative.r() > colors[0].r());
        assert!(negative.g() > colors[0].g());
        assert!(negative.b() > colors[0].b());
        assert_eq!(colors[0].a(), negative.a());
    }

    #[test]
    fn slanted_surface_guide_keeps_red_horizontal_and_blue_on_the_normal() {
        let normal = Vec3::new(1.0, 2.0, 3.0)
            .try_normalized(1.0e-12)
            .expect("slanted surface normal is nonzero");
        let guide = ConstructionGuide::at_point(ConstructionGuideKind::Axis, Vec3::ZERO, normal)
            .expect("slanted surface has a stable axis basis");

        assert!(guide.axis_w.distance(normal) < 1.0e-12);
        assert!(guide.axis_u.y.abs() < 1.0e-12);
        assert!(guide.axis_v.y > 0.0);
        assert!(guide.axis_u.dot(guide.axis_v).abs() < 1.0e-12);
        assert!(guide.axis_u.dot(guide.axis_w).abs() < 1.0e-12);
        assert!(guide.axis_v.dot(guide.axis_w).abs() < 1.0e-12);
    }

    #[test]
    fn segment_cursor_uses_blue_for_segment_and_red_for_ground_tangent() {
        let segment = Vec3::new(1.0, 2.0, 0.0)
            .try_normalized(1.0e-12)
            .expect("segment direction is nonzero");
        let [red, green, blue] =
            cursor_axes(Vec3::Y, Some(segment), Vec3::Y).expect("segment cursor basis is defined");

        assert!(blue.distance(segment) < 1.0e-12);
        assert!(red.y.abs() < 1.0e-12);
        assert!(red.dot(blue).abs() < 1.0e-12);
        assert!(green.dot(red).abs() < 1.0e-12);
        assert!(green.dot(blue).abs() < 1.0e-12);
        assert!((red.length() - 1.0).abs() < 1.0e-12);
        assert!((green.length() - 1.0).abs() < 1.0e-12);

        let [vertical_red, _, vertical_blue] =
            cursor_axes(Vec3::Y, Some(Vec3::Y), Vec3::Y).expect("vertical segment has basis");
        assert!(vertical_red.y.abs() < 1.0e-12);
        assert!(vertical_red.dot(vertical_blue).abs() < 1.0e-12);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_segment_cursor_axis_uses_the_spatial_index() {
        let mut document = Document::new();
        document
            .add_line_segment("axis", Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0))
            .expect("line segment is valid");
        let snap = SnapResult {
            point: Vec3::new(2.0, 0.0, 0.0),
            normal: Vec3::Y,
            kind: SnapKind::Line,
            screen_distance: 0.0,
        };
        let mut viewport = Viewport::default();
        let [red, green, blue] = viewport
            .native_cursor_axes(
                &document,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
                snap,
                Vec3::Y,
            )
            .expect("segment snap has an oriented cursor basis");

        assert!(blue.distance(Vec3::X) < 1.0e-12);
        assert!(red.y.abs() < 1.0e-12);
        assert!(red.dot(green).abs() < 1.0e-12);
        assert!(green.dot(blue).abs() < 1.0e-12);
    }

    #[test]
    fn coplanar_shared_triangle_edges_are_hidden_but_creases_are_retained() {
        let square = [
            (
                TriangleId(1),
                [VertexId(1), VertexId(2), VertexId(3)],
                [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 0.0, 1.0)],
            ),
            (
                TriangleId(2),
                [VertexId(1), VertexId(3), VertexId(4)],
                [Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0), Vec3::Z],
            ),
        ];
        let diagonal = edge_key(VertexId(1), VertexId(3));
        assert_eq!(
            BTreeSet::from([diagonal]),
            coplanar_shared_triangle_edges(&square)
        );

        let folded = [
            square[0],
            (
                TriangleId(2),
                [VertexId(1), VertexId(3), VertexId(4)],
                [Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0), Vec3::Z + Vec3::Y],
            ),
        ];
        assert!(coplanar_shared_triangle_edges(&folded).is_empty());
    }

    #[test]
    fn back_face_is_separated_a_small_distance_along_negative_normal() {
        let points = [Vec3::ZERO, Vec3::X, Vec3::Z];
        let normal = Vec3::Y;
        let offset = triangle_back_face_offset(points, normal);
        let moved_back_point = points[0] - offset;

        assert!(offset.length() > 0.0);
        assert!(offset.length() < 1.0e-3);
        assert!((moved_back_point - points[0]).dot(normal) < 0.0);
    }

    #[test]
    fn shadow_map_occludes_ground_but_does_not_shadow_its_own_horizontal_face() {
        let caster = WorldTriangle {
            points: [
                Vec3::new(-1.0, 4.0, -1.0),
                Vec3::new(1.0, 4.0, -1.0),
                Vec3::new(0.0, 4.0, 1.0),
            ],
        };
        let map = ShadowMap::build(&[caster]).expect("triangle creates a depth map");
        let light = shadow_light_direction();
        let face_center = Vec3::new(0.0, 4.0, -1.0 / 3.0);
        let receiver = face_center - light * (face_center.y / light.y);

        assert!(map.shadow_coverage(receiver, Vec3::Y) > 0.0);
        assert_eq!(0.0, map.shadow_coverage(face_center, Vec3::Y));
        assert_eq!(
            0.0,
            map.shadow_coverage(Vec3::new(100.0, 0.0, 100.0), Vec3::Y)
        );
    }

    #[test]
    fn shadow_map_caster_darkens_a_receiving_entity_face() {
        let caster = WorldTriangle {
            points: [
                Vec3::new(-1.0, 4.0, -1.0),
                Vec3::new(1.0, 4.0, -1.0),
                Vec3::new(0.0, 4.0, 1.0),
            ],
        };
        let light = shadow_light_direction();
        let caster_center = Vec3::new(0.0, 4.0, -1.0 / 3.0);
        let receiver_center = caster_center - light * (3.0 / light.y);
        let receiver = WorldTriangle {
            points: [
                receiver_center + Vec3::new(-0.5, 0.0, -0.5),
                receiver_center + Vec3::new(0.5, 0.0, -0.5),
                receiver_center + Vec3::new(0.0, 0.0, 0.5),
            ],
        };
        let map = ShadowMap::build(&[caster, receiver]).expect("scene creates a depth map");

        assert!(map.shadow_coverage(receiver_center, Vec3::Y) > 0.0);
    }

    fn run_viewport_frame(
        viewport: &mut Viewport,
        context: &egui::Context,
        document: &meshcraft_core::document::Document,
        events: Vec<Event>,
    ) -> super::ViewportInput {
        let mut viewport_input = None;
        let mut output = context.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                events,
                ..Default::default()
            },
            |ui| {
                viewport_input = Some(viewport.show(
                    ui,
                    &super::ViewportScene {
                        size: Vec2::new(800.0, 600.0),
                        document,
                        selection: &[],
                        scope: super::SnapScope::Document,
                        display_policy: &SelectionDisplayPolicy::default(),
                        active_tool: "Select",
                        preview: None,
                        feedback: None,
                        snap: None,
                        guides: &[],
                        guide_preview: None,
                        grid: super::GridStyle {
                            visible: false,
                            spacing: 1.0,
                            extent: 20,
                            normal_axis: super::GridNormalAxis::Y,
                            normal_width: 1.0,
                            thick_width: 3.0,
                        },
                    },
                ));
            },
        );
        output.textures_delta.clear();
        viewport_input.expect("viewport rendered during test frame")
    }

    fn render_selection_drag(start: Pos2, end: Pos2) -> egui::FullOutput {
        let mut viewport = Viewport::default();
        let context = egui::Context::default();
        let document = meshcraft_core::document::Document::new();
        let render_frame = |viewport: &mut Viewport, events: Vec<Event>| {
            context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    viewport.show(
                        ui,
                        &super::ViewportScene {
                            size: Vec2::new(800.0, 600.0),
                            document: &document,
                            selection: &[],
                            scope: super::SnapScope::Document,
                            display_policy: &SelectionDisplayPolicy::default(),
                            active_tool: "Select",
                            preview: None,
                            feedback: None,
                            snap: None,
                            guides: &[],
                            guide_preview: None,
                            grid: super::GridStyle {
                                visible: false,
                                spacing: 1.0,
                                extent: 20,
                                normal_axis: super::GridNormalAxis::Y,
                                normal_width: 1.0,
                                thick_width: 3.0,
                            },
                        },
                    );
                },
            )
        };
        let mut initial_frame = render_frame(&mut viewport, Vec::new());
        initial_frame.textures_delta.clear();
        let mut first_frame = render_frame(
            &mut viewport,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        first_frame.textures_delta.clear();
        let mut output = render_frame(&mut viewport, vec![Event::PointerMoved(end)]);
        output.textures_delta.clear();
        output
    }

    #[test]
    fn selection_drag_draws_direction_coded_window_and_crossing_marquees() {
        let cases = [
            (
                Pos2::new(180.0, 160.0),
                Pos2::new(340.0, 300.0),
                egui::Color32::from_rgb(37, 111, 220),
            ),
            (
                Pos2::new(340.0, 300.0),
                Pos2::new(180.0, 160.0),
                egui::Color32::from_rgb(32, 164, 105),
            ),
        ];
        for (start, end, expected_stroke) in cases {
            let output = render_selection_drag(start, end);
            let region = Rect::from_two_pos(start, end);
            assert!(output.shapes.iter().any(|clipped| {
                matches!(
                    &clipped.shape,
                    egui::epaint::Shape::Rect(shape)
                        if shape.rect == region
                            && shape.stroke.width == 1.5
                            && shape.stroke.color == expected_stroke
                )
            }));
        }
    }

    #[test]
    fn released_selection_drag_emits_a_marquee_gesture() {
        let mut viewport = Viewport::default();
        let context = egui::Context::default();
        let document = meshcraft_core::document::Document::new();
        let start = Pos2::new(180.0, 160.0);
        let end = Pos2::new(340.0, 300.0);
        run_viewport_frame(&mut viewport, &context, &document, Vec::new());
        run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![Event::PointerMoved(end)],
        );
        let release = run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![
                Event::PointerMoved(end),
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        assert_eq!(
            Some(start),
            release.selection_gesture.map(|gesture| gesture.start)
        );
        assert_eq!(
            Some(end),
            release.selection_gesture.map(|gesture| gesture.end)
        );
    }

    #[test]
    fn adaptive_grid_appears_near_the_permanent_grid_edge_and_reanchors_at_its_boundary() {
        let mut viewport = Viewport::default();
        let style = super::GridStyle {
            visible: false,
            spacing: 1.0,
            extent: 20,
            normal_axis: GridNormalAxis::Y,
            normal_width: 1.0,
            thick_width: 3.0,
        };
        let snap_at_center = SnapResult {
            point: Vec3::ZERO,
            normal: Vec3::Y,
            kind: SnapKind::Grid,
            screen_distance: 0.0,
        };
        viewport.update_adaptive_grid(Some(snap_at_center), style, &[]);
        assert_eq!(viewport.adaptive_grid_cloud, None);

        let near_edge = SnapResult {
            point: Vec3::new(16.0, 0.0, 0.0),
            ..snap_at_center
        };
        viewport.update_adaptive_grid(Some(near_edge), style, &[]);
        let cloud = viewport.adaptive_grid_cloud.expect("near-edge grid cloud");
        assert_eq!(cloud.center, Vec3::new(16.0, 0.0, 0.0));

        let within_cloud = SnapResult {
            point: Vec3::new(20.0, 0.0, 0.0),
            ..near_edge
        };
        viewport.update_adaptive_grid(Some(within_cloud), style, &[]);
        assert_eq!(viewport.adaptive_grid_cloud, Some(cloud));

        let beyond_cloud = SnapResult {
            point: Vec3::new(24.0, 0.0, 0.0),
            ..near_edge
        };
        viewport.update_adaptive_grid(Some(beyond_cloud), style, &[]);
        assert_eq!(
            viewport.adaptive_grid_cloud.map(|cloud| cloud.center),
            Some(Vec3::new(24.0, 0.0, 0.0))
        );
        viewport.update_adaptive_grid(None, style, &[]);
        assert_eq!(viewport.adaptive_grid_cloud, None);
    }

    #[test]
    fn adaptive_grid_activates_for_grid_guides_and_follows_the_snapped_surface_plane() {
        let mut viewport = Viewport::default();
        let style = super::GridStyle {
            visible: false,
            spacing: 1.0,
            extent: 20,
            normal_axis: GridNormalAxis::Y,
            normal_width: 1.0,
            thick_width: 3.0,
        };
        let grid_guide =
            ConstructionGuide::at_point(ConstructionGuideKind::Grid, Vec3::ZERO, Vec3::Z)
                .expect("grid guide basis");
        let snap = SnapResult {
            point: Vec3::new(2.4, 3.2, 0.0),
            normal: Vec3::Z,
            kind: SnapKind::Face,
            screen_distance: 0.0,
        };
        viewport.update_adaptive_grid(Some(snap), style, &[grid_guide]);
        let cloud = viewport
            .adaptive_grid_cloud
            .expect("guide activates grid cloud");
        assert_eq!(cloud.normal, Vec3::Z);
        assert_eq!(cloud.center, Vec3::new(2.0, 3.0, 0.0));
        assert_eq!(cloud.normal.dot(cloud.axis_u), 0.0);
        assert_eq!(cloud.normal.dot(cloud.axis_v), 0.0);
    }

    #[test]
    fn camera_reset_restores_the_default_projection_and_clears_pending_frame() {
        let mut viewport = Viewport {
            zoom: 91.0,
            pan: Vec2::new(22.0, -17.0),
            yaw: 1.2,
            elevation: 1.0,
            frame_pending: true,
            ..Viewport::default()
        };

        viewport.reset_camera();

        assert_eq!(28.0, viewport.zoom);
        assert_eq!(Vec2::ZERO, viewport.pan);
        assert_eq!(0.0, viewport.yaw);
        assert_eq!(DEFAULT_ELEVATION, viewport.elevation);
        assert_eq!(Vec3::ZERO, viewport.target);
        assert_eq!(DEFAULT_PERSPECTIVE_DISTANCE, viewport.distance);
        assert_eq!(super::DEFAULT_FIELD_OF_VIEW, viewport.field_of_view);
        assert_eq!(super::DEFAULT_NEAR_PLANE, viewport.near_plane);
        assert_eq!(super::DEFAULT_FAR_PLANE, viewport.far_plane);
        assert_eq!(None, viewport.orthographic_view());
        assert!(!viewport.frame_pending);
    }

    #[test]
    fn walkthrough_uses_highest_surface_moves_and_applies_jump_gravity() {
        let mut document = meshcraft_core::document::Document::new();
        document
            .add_cuboid("ground", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        document
            .add_cuboid(
                "platform",
                Vec3::new(1.0, 3.0, 1.0),
                Vec3::new(0.5, 2.0, 0.5),
            )
            .unwrap();
        let mut viewport = Viewport {
            walkthrough: Some(WalkthroughCamera {
                position: Vec3::new(0.75, 12.0, 0.75),
                yaw: 0.0,
                pitch: 0.0,
                eye_offset: 0.0,
                jump_offset: 0.0,
                vertical_velocity: 0.0,
                jump_queued: false,
                grounded: true,
            }),
            walkthrough_tuning: WalkthroughTuning {
                move_speed: 2.0,
                ..WalkthroughTuning::default()
            },
            ..Viewport::default()
        };

        viewport.update_walkthrough(
            &document, 0.0, false, false, false, false, false, false, false,
        );
        assert!((viewport.walkthrough.unwrap().position.y - 11.0).abs() < 1.0e-12);
        viewport.update_walkthrough(
            &document, 0.1, true, false, false, false, false, false, true,
        );
        let jumping = viewport.walkthrough.unwrap();
        assert!((jumping.position.x - 0.95).abs() < 1.0e-12);
        assert!(jumping.position.y > 11.0);
        assert!(!jumping.grounded);
        viewport.update_walkthrough(
            &document, 0.1, false, false, false, false, false, false, true,
        );
        assert!(viewport.walkthrough.unwrap().jump_queued);
        viewport.walkthrough.as_mut().unwrap().jump_queued = false;

        for _ in 0..20 {
            viewport.update_walkthrough(
                &document, 0.1, false, false, false, false, false, false, false,
            );
        }
        let landed = viewport.walkthrough.unwrap();
        assert!(landed.grounded);
        assert!((landed.position.y - 11.0).abs() < 1.0e-12);
    }

    #[test]
    fn walkthrough_travel_keeps_its_height_when_crossing_ground_supports() {
        let mut document = Document::new();
        document
            .add_cuboid("platform", Vec3::new(2., 2., 2.), Vec3::new(2., 0., 0.))
            .unwrap();
        let mut viewport = Viewport::default();
        let eye = Vec3::new(0., super::WALK_EYE_HEIGHT, 0.5);
        let direction = Vec3::new(1., -0.5, 0.).try_normalized(1e-12).unwrap();
        viewport.native_walkthrough_pose(eye, direction);
        let delta = Vec3::new(3., -1., 0.);
        viewport.travel_walkthrough(&document, delta);
        let (position, forward) = viewport.native_walkthrough_step(&document, 0.05, [false; 7]);
        assert!(position.distance(eye + delta) < 1e-9);
        assert!(forward.distance(direction) < 1e-9);
        viewport.travel_walkthrough(&document, -delta);
        let (position, _) = viewport.native_walkthrough_step(&document, 0.05, [false; 7]);
        assert!(position.distance(eye) < 1e-9);
    }

    #[test]
    fn walkthrough_projection_uses_its_pose_but_model_camera_state_stays_perspective() {
        let mut viewport = Viewport::default();
        let state = serde_json::json!({
            "position": {"x": 8.0, "y": 7.0, "z": 6.0},
            "target": {"x": 2.0, "y": 3.0, "z": 4.0},
            "fieldOfView": 51.0
        });
        assert!(viewport.restore_legacy_camera_state(&state));
        let model_camera = viewport.legacy_camera_state();
        viewport.set_walkthrough_view();
        viewport.walkthrough.as_mut().unwrap().position += Vec3::new(3.0, 2.0, -5.0);

        assert!(viewport.walkthrough_active());
        assert_eq!(model_camera, viewport.legacy_camera_state());
        let walk = viewport.walkthrough.unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let point_ahead = walk.position + walk.forward() * 10.0;
        assert!(
            viewport
                .world_to_screen(rect, point_ahead)
                .distance(rect.center())
                < 1.0e-4
        );
        assert_eq!(None, viewport.orthographic_view());
    }

    #[test]
    fn walkthrough_center_ray_picks_mesh_in_front_of_the_camera() {
        let mut document = meshcraft_core::document::Document::new();
        let occurrence = document
            .add_cuboid("target", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let mut viewport = Viewport {
            walkthrough: Some(WalkthroughCamera {
                position: Vec3::new(5.0, 1.0, 1.0),
                yaw: std::f64::consts::PI,
                pitch: 0.0,
                eye_offset: 0.0,
                jump_offset: 0.0,
                vertical_velocity: 0.0,
                jump_queued: false,
                grounded: true,
            }),
            ..Viewport::default()
        };
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

        assert_eq!(
            Some(occurrence),
            viewport.pick(rect, &document, rect.center())
        );
    }

    #[test]
    fn walkthrough_keyboard_movement_is_routed_only_over_the_viewport() {
        let mut viewport = Viewport::default();
        viewport.set_walkthrough_view();
        let document = meshcraft_core::document::Document::new();
        let context = egui::Context::default();
        let start = viewport.walkthrough.unwrap().position;
        let center = Pos2::new(400.0, 300.0);

        run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![
                Event::PointerMoved(center),
                Event::Key {
                    key: egui::Key::W,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        let after_key_press = viewport.walkthrough.unwrap().position;
        assert!(after_key_press.distance(start) > 0.0);

        run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![Event::PointerMoved(Pos2::new(900.0, 700.0))],
        );
        let after_releasing_pointer = viewport.walkthrough.unwrap().position;
        assert!((after_releasing_pointer.x - after_key_press.x).abs() < 1.0e-12);
        assert!((after_releasing_pointer.z - after_key_press.z).abs() < 1.0e-12);
    }

    #[test]
    fn focused_text_field_keeps_wheel_input_from_zooming_the_viewport() {
        let mut viewport = Viewport::default();
        let document = meshcraft_core::document::Document::new();
        let context = egui::Context::default();
        let field_id = egui::Id::new("wheel-focus-field");
        let mut text = String::new();

        let mut run_frame = |events: Vec<Event>| {
            let mut viewport_input = None;
            let mut field_rect = None;
            let mut output = context.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 700.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    field_rect = Some(
                        ui.add(egui::TextEdit::singleline(&mut text).id(field_id))
                            .rect,
                    );
                    viewport_input = Some(viewport.show(
                        ui,
                        &super::ViewportScene {
                            size: Vec2::new(800.0, 600.0),
                            document: &document,
                            selection: &[],
                            scope: super::SnapScope::Document,
                            display_policy: &SelectionDisplayPolicy::default(),
                            active_tool: "Select",
                            preview: None,
                            feedback: None,
                            snap: None,
                            guides: &[],
                            guide_preview: None,
                            grid: super::GridStyle {
                                visible: false,
                                spacing: 1.0,
                                extent: 20,
                                normal_axis: super::GridNormalAxis::Y,
                                normal_width: 1.0,
                                thick_width: 3.0,
                            },
                        },
                    ));
                },
            );
            output.textures_delta.clear();
            (
                viewport_input.expect("viewport rendered in test frame"),
                field_rect.expect("text field rendered in test frame"),
            )
        };

        let (initial_viewport, field_rect) = run_frame(Vec::new());
        run_frame(vec![
            Event::PointerMoved(field_rect.center()),
            Event::PointerButton {
                pos: field_rect.center(),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
            Event::PointerButton {
                pos: field_rect.center(),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ]);
        assert!(context.egui_wants_keyboard_input());

        let (viewport_input, _) = run_frame(vec![
            Event::PointerMoved(initial_viewport.rect.center()),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::default(),
            },
        ]);
        assert!(viewport_input.pointer_local.is_some());
        assert_eq!(28.0, viewport.zoom);
    }

    #[test]
    fn walkthrough_height_sampling_interpolates_slopes_and_rejects_vertical_faces() {
        let first = Vec3::new(0.0, 0.0, 0.0);
        let second = Vec3::new(1.0, 1.0, 0.0);
        let third = Vec3::new(0.0, 0.0, 1.0);
        assert!(
            (height_in_triangle_xz(0.25, 0.25, first, second, third).unwrap() - 0.25).abs()
                < 1.0e-12
        );
        assert_eq!(
            None,
            height_in_triangle_xz(
                0.0,
                0.0,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            )
        );
        assert_eq!(None, height_in_triangle_xz(2.0, 2.0, first, second, third));
    }

    #[test]
    fn legacy_camera_pose_restores_and_round_trips_position_and_projection_parameters() {
        let state = serde_json::json!({
            "position": {"x": 8.0, "y": 7.0, "z": 6.0},
            "direction": {"x": 0.0, "y": 1.0, "z": 0.0},
            "up": {"x": 1.0, "y": 0.0, "z": 0.0},
            "target": {"x": 2.0, "y": 3.0, "z": 4.0},
            "near": 0.25,
            "far": 240.0,
            "fieldOfView": 51.0
        });
        let mut viewport = Viewport::default();

        assert!(viewport.restore_legacy_camera_state(&state));
        assert!((viewport.target.distance(Vec3::new(2.0, 3.0, 4.0))) < 1.0e-12);
        assert!((viewport.distance - 56.0_f64.sqrt()).abs() < 1.0e-12);
        assert_eq!(51.0, viewport.field_of_view);
        assert_eq!(0.25, viewport.near_plane);
        assert_eq!(240.0, viewport.far_plane);
        assert!(!viewport.frame_pending);

        let written = viewport.legacy_camera_state();
        let mut reopened = Viewport::default();
        assert!(reopened.restore_legacy_camera_state(&written));
        assert!(reopened.target.distance(viewport.target) < 1.0e-12);
        assert!((reopened.distance - viewport.distance).abs() < 1.0e-12);
        assert!((reopened.yaw - viewport.yaw).abs() < 1.0e-12);
        assert!((reopened.elevation - viewport.elevation).abs() < 1.0e-12);
    }

    #[test]
    fn malformed_or_degenerate_camera_states_do_not_replace_the_current_pose() {
        let mut viewport = Viewport::default();
        let original = viewport.legacy_camera_state();
        let invalid = serde_json::json!({
            "position": {"x": 1.0, "y": 1.0, "z": 1.0},
            "target": {"x": 1.0, "y": 1.0, "z": 1.0}
        });

        assert!(!viewport.restore_legacy_camera_state(&invalid));
        assert_eq!(original, viewport.legacy_camera_state());
    }

    #[test]
    fn saving_from_an_orthographic_view_keeps_the_model_perspective_pose() {
        let mut viewport = Viewport::default();
        let camera_state = serde_json::json!({
            "position": {"x": 8.0, "y": 7.0, "z": 6.0},
            "target": {"x": 2.0, "y": 3.0, "z": 4.0},
            "fieldOfView": 51.0
        });
        assert!(viewport.restore_legacy_camera_state(&camera_state));
        let perspective_pose = viewport.legacy_camera_state();

        viewport.set_orthographic_view(OrthoView::Top);

        let saved_pose = viewport.legacy_camera_state();
        assert_eq!(perspective_pose["position"], saved_pose["position"]);
        assert_eq!(perspective_pose["direction"], saved_pose["direction"]);
        assert_eq!(perspective_pose["up"], saved_pose["up"]);
        assert_eq!(perspective_pose["target"], saved_pose["target"]);
        assert_eq!(perspective_pose["fieldOfView"], saved_pose["fieldOfView"]);
    }

    #[test]
    fn all_orthographic_views_project_their_screen_axes_without_depth_scaling() {
        let rect = Rect::from_min_size(Pos2::new(60.0, 30.0), Vec2::new(1000.0, 700.0));
        let views = [
            (
                OrthoView::Top,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            (
                OrthoView::Bottom,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, -1.0, 0.0),
            ),
            (
                OrthoView::Left,
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(-1.0, 0.0, 0.0),
            ),
            (
                OrthoView::Right,
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            ),
            (
                OrthoView::Front,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            (
                OrthoView::Back,
                Vec3::new(-1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
            ),
        ];

        for (view, right, up, depth) in views {
            let mut viewport = Viewport::default();
            viewport.set_orthographic_view(view);
            let origin = Vec3::new(2.0, -3.0, 5.0);
            let screen_origin = viewport.world_to_screen(rect, origin);
            let screen_right = viewport.world_to_screen(rect, origin + right);
            let screen_up = viewport.world_to_screen(rect, origin + up);
            let screen_depth = viewport.world_to_screen(rect, origin + depth * 100.0);
            assert!((screen_right.x - screen_origin.x - viewport.zoom).abs() < 1.0e-4);
            assert!((screen_right.y - screen_origin.y).abs() < 1.0e-4);
            assert!((screen_up.x - screen_origin.x).abs() < 1.0e-4);
            assert!((screen_up.y - screen_origin.y + viewport.zoom).abs() < 1.0e-4);
            assert!(screen_depth.distance(screen_origin) < 1.0e-4);
            assert_eq!(view, viewport.orthographic_view().unwrap());
        }
    }

    #[test]
    fn all_orthographic_views_round_trip_their_construction_planes() {
        let rect = Rect::from_min_size(Pos2::new(40.0, 30.0), Vec2::new(960.0, 720.0));
        let mut viewport = Viewport {
            target: Vec3::new(4.0, 7.0, -3.0),
            pan: Vec2::new(23.0, -19.0),
            zoom: 31.0,
            ..Viewport::default()
        };
        for view in [
            OrthoView::Top,
            OrthoView::Bottom,
            OrthoView::Left,
            OrthoView::Right,
            OrthoView::Front,
            OrthoView::Back,
        ] {
            viewport.orthographic = Some(view);
            let screen = Pos2::new(340.0, 290.0);
            let world = viewport.screen_to_construction_plane(rect, screen).unwrap();
            let projected = viewport.world_to_screen(rect, world);
            assert!(projected.distance(screen) < 1.0e-4, "view={view:?}");
            assert!(
                view.screen_axes().2.dot(world).abs() < 1.0e-10,
                "view={view:?}"
            );

            let opposite_screen = Pos2::new(470.0, 390.0);
            let opposite = viewport
                .screen_to_construction_plane(rect, opposite_screen)
                .unwrap();
            let corners = meshcraft_core::tools::shapes::rectangle_corners(
                world,
                opposite,
                view.screen_axes().2,
            )
            .unwrap();
            assert!(
                viewport
                    .world_to_screen(rect, corners[2])
                    .distance(opposite_screen)
                    < 1.0e-4
            );
        }
    }

    #[test]
    fn perspective_construction_plane_uses_configured_grid_normal() {
        let rect = Rect::from_min_size(Pos2::new(45.0, 28.0), Vec2::new(840.0, 620.0));
        let mut viewport = Viewport::default();
        viewport.set_grid_normal_axis(GridNormalAxis::Z);
        let pointer = rect.center() + Vec2::new(24.0, -18.0);
        let point = viewport
            .screen_to_construction_plane(rect, pointer)
            .expect("perspective pointer intersects the selected construction plane");
        assert!(point.z.abs() < 1.0e-9);

        let empty = meshcraft_core::document::Document::new();
        let snapped = viewport
            .snap_point(rect, &empty, pointer, 32.0, 1.0)
            .expect("virtual construction grid provides a snap");
        assert_eq!(GridNormalAxis::Z.vector(), snapped.normal);
        assert!(snapped.point.z.abs() < 1.0e-9);
    }

    #[test]
    fn only_top_and_bottom_orthographic_views_intersect_the_ground_for_drawing() {
        let rect = Rect::from_min_size(Pos2::new(60.0, 30.0), Vec2::new(1000.0, 700.0));
        for view in [OrthoView::Top, OrthoView::Bottom] {
            let mut viewport = Viewport::default();
            viewport.set_orthographic_view(view);
            viewport.target = Vec3::new(4.0, 2.0, -6.0);
            viewport.pan = Vec2::new(13.0, -21.0);
            let ground = Vec3::new(-7.25, 0.0, 11.5);
            let projected = viewport.world_to_screen(rect, ground);
            let recovered = viewport.screen_to_ground(rect, projected).unwrap();
            assert!(recovered.distance(ground) < 1.0e-5);
        }
        for view in [
            OrthoView::Left,
            OrthoView::Right,
            OrthoView::Front,
            OrthoView::Back,
        ] {
            let mut viewport = Viewport::default();
            viewport.set_orthographic_view(view);
            assert!(viewport.screen_to_ground(rect, rect.center()).is_none());
        }
    }

    #[test]
    fn right_drag_orbits_and_shift_right_drag_pans() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut viewport = Viewport::default();
        let drag = Vec2::new(20.0, -10.0);
        viewport.drag_camera(rect, egui::PointerButton::Secondary, false, drag, false);
        assert_eq!(0.16, viewport.yaw);
        assert!((viewport.elevation - (DEFAULT_ELEVATION - 0.08)).abs() < 1.0e-12);
        assert_eq!(Vec2::ZERO, viewport.pan);

        let orientation = (viewport.yaw, viewport.elevation);
        let (right, up, _) = viewport.camera_axes();
        let scale = viewport.distance / viewport.focal_length(rect.height());
        let delta = Vec2::new(-4.0, 8.0);
        let expected_target =
            right * (-f64::from(delta.x) * scale) + up * (f64::from(delta.y) * scale);
        viewport.drag_camera(rect, egui::PointerButton::Secondary, true, delta, false);
        assert_eq!(orientation, (viewport.yaw, viewport.elevation));
        assert!(viewport.target.distance(expected_target) < 1.0e-10);
        assert_eq!(Vec2::ZERO, viewport.pan);
    }

    #[test]
    fn orbit_pitch_allows_near_vertical_views_and_clamps_at_eighty_nine_degrees() {
        let mut viewport = Viewport::default();

        viewport.orbit_by(Vec2::new(0.0, -10_000.0));
        assert!((viewport.elevation - MIN_ELEVATION).abs() < 1.0e-12);
        assert!(viewport.camera_direction().length().is_finite());

        viewport.orbit_by(Vec2::new(0.0, 20_000.0));
        assert!((viewport.elevation - MAX_ELEVATION).abs() < 1.0e-12);
        assert!(viewport.camera_direction().length().is_finite());
    }

    #[test]
    fn restored_camera_pitch_is_clamped_to_the_expanded_orbit_range() {
        let mut viewport = Viewport::default();
        for (height, expected) in [(10.0, MAX_ELEVATION), (-10.0, MIN_ELEVATION)] {
            assert!(viewport.restore_legacy_camera_state(&json!({
                "position": {"x": 0.0, "y": height, "z": 0.0},
                "target": {"x": 0.0, "y": 0.0, "z": 0.0}
            })));
            assert!((viewport.elevation - expected).abs() < 1.0e-12);
        }
    }

    #[test]
    fn viewport_routes_secondary_pointer_drag_to_orbit_and_shift_drag_to_pan() {
        let mut viewport = Viewport::default();
        let context = egui::Context::default();
        let document = meshcraft_core::document::Document::new();
        let start = Pos2::new(400.0, 300.0);
        let first_frame = run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Secondary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
        );
        assert!(
            first_frame.rect.contains(start),
            "viewport rect was {}",
            first_frame.rect
        );

        let second_frame = run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![Event::PointerMoved(Pos2::new(420.0, 290.0))],
        );
        assert!(
            second_frame.pointer_local.is_some(),
            "pointer was not hovered: rect={}, pointer={:?}",
            second_frame.rect,
            context.input(|input| input.pointer.latest_pos())
        );
        let pointer_state = context.input(|input| {
            (
                input.pointer.secondary_down(),
                input.pointer.delta(),
                input.pointer.is_decidedly_dragging(),
            )
        });
        assert!(
            (viewport.yaw - 0.16).abs() < 1.0e-6,
            "yaw={}, pointer state={pointer_state:?}",
            viewport.yaw,
        );
        assert!((viewport.elevation - (DEFAULT_ELEVATION - 0.08)).abs() < 1.0e-6);
        assert_eq!(Vec2::ZERO, viewport.pan);

        let orientation = (viewport.yaw, viewport.elevation);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        run_viewport_frame(
            &mut viewport,
            &context,
            &document,
            vec![
                Event::ModifiersChanged(shift),
                Event::PointerMoved(Pos2::new(440.0, 300.0)),
            ],
        );
        assert_eq!(orientation, (viewport.yaw, viewport.elevation));
        assert!(viewport.target.distance(Vec3::ZERO) > 0.0);
        assert_eq!(Vec2::ZERO, viewport.pan);
    }

    #[test]
    fn perspective_projection_and_ground_picking_round_trip_across_orbits() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let mut viewport = Viewport {
            pan: Vec2::new(22.0, -17.0),
            ..Viewport::default()
        };
        for yaw in [0.0, 0.8, 2.4] {
            for elevation in [MIN_ELEVATION, DEFAULT_ELEVATION, MAX_ELEVATION] {
                viewport.yaw = yaw;
                viewport.elevation = elevation;
                let ground = Vec3::new(-2.25, 0.0, 3.5);
                let projected = viewport.world_to_screen(rect, ground);
                let recovered = viewport.screen_to_ground(rect, projected).unwrap();
                assert!(recovered.distance(ground) < 1.0e-5);
            }
        }
    }

    #[test]
    fn camera_mapping_uses_the_actual_viewport_rect_across_layout_sizes() {
        let rects = [
            Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0)),
            Rect::from_min_size(Pos2::new(324.0, 28.0), Vec2::new(940.0, 820.0)),
            Rect::from_min_size(Pos2::new(480.0, 32.0), Vec2::new(1400.0, 900.0)),
        ];

        for rect in rects {
            let perspective = Viewport {
                yaw: 0.72,
                elevation: 0.91,
                pan: Vec2::new(18.0, -12.0),
                ..Viewport::default()
            };
            for ground_point in [Vec3::new(-8.5, 0.0, 4.25), Vec3::new(3.0, 0.0, -11.0)] {
                let screen = perspective.world_to_screen(rect, ground_point);
                let recovered = perspective.screen_to_ground(rect, screen).unwrap();
                assert!(recovered.distance(ground_point) < 1.0e-5, "rect={rect:?}");
            }

            for view in [
                OrthoView::Top,
                OrthoView::Bottom,
                OrthoView::Left,
                OrthoView::Right,
                OrthoView::Front,
                OrthoView::Back,
            ] {
                let mut orthographic = Viewport {
                    pan: Vec2::new(-21.0, 14.0),
                    zoom: 28.0,
                    ..Viewport::default()
                };
                orthographic.set_orthographic_view(view);
                let (right, up, _) = view.screen_axes();
                let point = right * 3.5 + up * -2.25;
                let screen = orthographic.world_to_screen(rect, point);
                let recovered = orthographic
                    .screen_to_construction_plane(rect, screen)
                    .unwrap();
                assert!(
                    recovered.distance(point) < 1.0e-5,
                    "view={view:?}, rect={rect:?}"
                );
            }
        }
    }

    #[test]
    fn snapping_prefers_transformed_endpoints_and_segment_midpoints() {
        let rect = Rect::from_min_size(Pos2::new(72.0, 34.0), Vec2::new(920.0, 680.0));
        let mut document = meshcraft_core::document::Document::new();
        let line = document
            .add_line_segment(
                "source",
                Vec3::new(-2.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
            )
            .unwrap();
        let copy = document
            .copy_root_occurrence(&line, Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        let mut viewport = Viewport {
            target: Vec3::new(10.0, 0.0, 0.0),
            ..Viewport::default()
        };
        let expected_endpoint = Vec3::new(8.0, 0.0, 0.0);
        let pointer = viewport.world_to_screen(rect, expected_endpoint) + Vec2::new(3.0, -2.0);
        let endpoint = viewport
            .snap_point(rect, &document, pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Endpoint, endpoint.kind);
        assert_eq!(expected_endpoint, endpoint.point);
        assert!(document.resolve_occurrence(&copy).is_some());

        let mut midpoint_document = meshcraft_core::document::Document::new();
        midpoint_document
            .add_line_segment(
                "midpoint",
                Vec3::new(-3.0, 0.0, 4.0),
                Vec3::new(3.0, 0.0, 4.0),
            )
            .unwrap();
        let mut midpoint_viewport = Viewport {
            target: Vec3::new(0.0, 0.0, 4.0),
            ..Viewport::default()
        };
        let midpoint = Vec3::new(0.0, 0.0, 4.0);
        let midpoint_screen = midpoint_viewport.world_to_screen(rect, midpoint);
        let snapped_midpoint = midpoint_viewport
            .snap_point(rect, &midpoint_document, midpoint_screen, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Midpoint, snapped_midpoint.kind);
        assert_eq!(midpoint, snapped_midpoint.point);

        let mut line_document = meshcraft_core::document::Document::new();
        line_document
            .add_line_segment(
                "line",
                Vec3::new(-3.2, 0.0, 0.37),
                Vec3::new(2.8, 0.0, 0.37),
            )
            .unwrap();
        let mut line_viewport = Viewport {
            target: Vec3::new(-1.7, 0.0, 0.37),
            ..Viewport::default()
        };
        let line_point = Vec3::new(-1.7, 0.0, 0.37);
        let line_screen = line_viewport.world_to_screen(rect, line_point);
        let line = line_viewport
            .snap_point(rect, &line_document, line_screen, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Line, line.kind);
        assert!(line.point.distance(line_point) < 1.0e-5);
    }

    #[test]
    fn snapping_finds_triangle_edge_midpoints_and_nearest_edge_points() {
        let rect = Rect::from_min_size(Pos2::new(56.0, 30.0), Vec2::new(920.0, 680.0));
        let mut document = meshcraft_core::document::Document::new();
        document
            .add_ground_rectangle(
                "face-edge",
                Vec3::new(-4.0, 0.0, -4.0),
                Vec3::new(4.0, 0.0, 4.0),
            )
            .unwrap();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);

        let edge_midpoint = Vec3::new(4.0, 0.0, 0.0);
        let midpoint_pointer = viewport.world_to_screen(rect, edge_midpoint);
        let midpoint = viewport
            .snap_point(rect, &document, midpoint_pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Midpoint, midpoint.kind);
        assert!(midpoint.point.distance(edge_midpoint) < 1.0e-6);

        let pointer_world = Vec3::new(3.8, 0.0, 2.0);
        let nearest_edge_pointer = viewport.world_to_screen(rect, pointer_world);
        let nearest_edge = viewport
            .snap_point(rect, &document, nearest_edge_pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Line, nearest_edge.kind);
        assert!(nearest_edge.point.distance(Vec3::new(4.0, 0.0, 2.0)) < 1.0e-6);
    }

    #[test]
    fn snapping_uses_grid_lines_and_prefers_endpoints_at_face_ties() {
        let rect = Rect::from_min_size(Pos2::new(48.0, 28.0), Vec2::new(900.0, 640.0));
        let empty = meshcraft_core::document::Document::new();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);
        let between_grid_points = Vec3::new(2.25, 0.0, -3.5);
        let grid_line_pointer = viewport.world_to_screen(rect, between_grid_points);
        let grid_line = viewport
            .snap_point(rect, &empty, grid_line_pointer, 8.0, 1.0)
            .unwrap();
        assert_eq!(SnapKind::GridLine, grid_line.kind);
        assert!(grid_line.point.distance(Vec3::new(2.0, 0.0, -3.5)) < 1.0e-6);

        let mut document = meshcraft_core::document::Document::new();
        document
            .add_ground_rectangle(
                "tie-face",
                Vec3::new(-5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, 5.0),
            )
            .unwrap();
        let endpoint = Vec3::new(1.0, 0.0, 2.0);
        document
            .add_line_segment("tie-endpoint", endpoint, Vec3::new(3.0, 0.0, 2.0))
            .unwrap();
        let pointer = viewport.world_to_screen(rect, endpoint);
        let tied = viewport
            .snap_point(rect, &document, pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Endpoint, tied.kind);
        assert_eq!(endpoint, tied.point);
    }

    #[test]
    fn snapping_ignores_model_edges_occluded_by_a_visible_face() {
        let rect = Rect::from_min_size(Pos2::new(48.0, 28.0), Vec2::new(900.0, 640.0));
        let mut document = meshcraft_core::document::Document::new();
        let endpoint = Vec3::new(1.0, 0.0, 2.0);
        document
            .add_line_segment("hidden-line", endpoint, Vec3::new(3.0, 0.0, 2.0))
            .unwrap();
        document
            .add_cuboid(
                "occluding-solid",
                Vec3::new(10.0, 1.0, 10.0),
                Vec3::new(-5.0, 0.5, -5.0),
            )
            .unwrap();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);
        let pointer = viewport.world_to_screen(rect, endpoint);
        let snapped = viewport
            .snap_point(rect, &document, pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Face, snapped.kind);
        assert!((snapped.point.y - 1.5).abs() < 1.0e-6);
    }

    #[test]
    fn snapping_respects_hidden_geometry_and_active_occurrence_scope() {
        let rect = Rect::from_min_size(Pos2::new(40.0, 28.0), Vec2::new(900.0, 640.0));
        let mut document = meshcraft_core::document::Document::new();
        document
            .add_line_segment(
                "active-context-line",
                Vec3::new(2.25, 0.0, 2.25),
                Vec3::new(4.25, 0.0, 2.25),
            )
            .unwrap();
        document
            .add_line_segment(
                "other-context-line",
                Vec3::new(12.25, 0.0, 2.25),
                Vec3::new(14.25, 0.0, 2.25),
            )
            .unwrap();
        let occurrences = document.occurrence_refs().unwrap();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);
        let active_point = Vec3::new(2.25, 0.0, 2.25);
        let active_pointer = viewport.world_to_screen(rect, active_point);
        let active_snap = viewport
            .snap_point_with_options(
                rect,
                &document,
                active_pointer,
                SnapOptions {
                    snap_radius: 8.0,
                    grid_spacing: 1_000.0,
                    guides: &[],
                    scope: SnapScope::Occurrence(&occurrences[0]),
                    visibility: SnapVisibility::default(),
                },
            )
            .unwrap();
        assert_eq!(SnapKind::Endpoint, active_snap.kind);
        assert_eq!(active_point, active_snap.point);

        let other_point = Vec3::new(12.25, 0.0, 2.25);
        let other_pointer = viewport.world_to_screen(rect, other_point);
        let outside_scope = viewport
            .snap_point_with_options(
                rect,
                &document,
                other_pointer,
                SnapOptions {
                    snap_radius: 8.0,
                    grid_spacing: 1_000.0,
                    guides: &[],
                    scope: SnapScope::Occurrence(&occurrences[0]),
                    visibility: SnapVisibility::default(),
                },
            )
            .unwrap();
        assert_eq!(SnapKind::Free, outside_scope.kind);

        let hidden_edges = viewport
            .snap_point_with_options(
                rect,
                &document,
                active_pointer,
                SnapOptions {
                    snap_radius: 8.0,
                    grid_spacing: 1_000.0,
                    guides: &[],
                    scope: SnapScope::Document,
                    visibility: SnapVisibility {
                        edges_visible: false,
                        ..SnapVisibility::default()
                    },
                },
            )
            .unwrap();
        assert_eq!(SnapKind::Free, hidden_edges.kind);

        let mut faces = meshcraft_core::document::Document::new();
        faces
            .add_ground_rectangle(
                "hidden-face",
                Vec3::new(-5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, 5.0),
            )
            .unwrap();
        let face_point = Vec3::new(-2.0, 0.0, 1.0);
        let face_pointer = viewport.world_to_screen(rect, face_point);
        for visibility in [
            SnapVisibility {
                faces_visible: false,
                ..SnapVisibility::default()
            },
            SnapVisibility {
                faces_wireframe: true,
                ..SnapVisibility::default()
            },
        ] {
            let snap = viewport
                .snap_point_with_options(
                    rect,
                    &faces,
                    face_pointer,
                    SnapOptions {
                        snap_radius: 8.0,
                        grid_spacing: 1_000.0,
                        guides: &[],
                        scope: SnapScope::Document,
                        visibility,
                    },
                )
                .unwrap();
            assert_eq!(SnapKind::Free, snap.kind);
        }
    }

    #[test]
    fn snapping_uses_oriented_grid_and_axis_guides() {
        let rect = Rect::from_min_size(Pos2::new(60.0, 30.0), Vec2::new(900.0, 700.0));
        let document = meshcraft_core::document::Document::new();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);
        let grid_guide = ConstructionGuide::at_point(
            ConstructionGuideKind::Grid,
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::Y,
        )
        .unwrap();
        let grid_point = Vec3::new(2.0, 4.0, -3.0);
        let grid_pointer = viewport.world_to_screen(rect, grid_point) + Vec2::new(2.0, -1.0);
        let grid_snap = viewport
            .snap_point_with_guides(rect, &document, grid_pointer, 8.0, 1.0, &[grid_guide])
            .unwrap();
        assert_eq!(SnapKind::GridGuide, grid_snap.kind);
        assert_eq!(grid_point, grid_snap.point);
        assert_eq!(Vec3::Y, grid_snap.normal);

        let axis_guide = ConstructionGuide::at_point(
            ConstructionGuideKind::Axis,
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::Y,
        )
        .unwrap();
        let axis_point = axis_guide.origin + axis_guide.axis_u * 5.0;
        let axis_pointer = viewport.world_to_screen(rect, axis_point) + Vec2::new(2.0, 1.0);
        let axis_snap = viewport
            .snap_point_with_guides(rect, &document, axis_pointer, 8.0, 1.0, &[axis_guide])
            .unwrap();
        assert_eq!(SnapKind::AxisGuide, axis_snap.kind);
        assert!(axis_snap.point.distance(axis_point) < 1.0e-6);
    }

    #[test]
    fn snapping_uses_face_hits_grid_points_and_pixel_radius() {
        let rect = Rect::from_min_size(Pos2::new(48.0, 28.0), Vec2::new(900.0, 640.0));
        let mut document = meshcraft_core::document::Document::new();
        let min = Vec3::new(-4.6, 0.0, -4.4);
        let max = Vec3::new(5.4, 0.0, 5.6);
        document.add_ground_rectangle("surface", min, max).unwrap();
        let face_center = (min + max) * 0.5;
        let mut viewport = Viewport {
            target: face_center,
            ..Viewport::default()
        };
        let face_pointer = viewport.world_to_screen(rect, face_center);
        let triangulation_midpoint = viewport
            .snap_point(rect, &document, face_pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Midpoint, triangulation_midpoint.kind);
        assert!(triangulation_midpoint.point.distance(face_center) < 1.0e-6);

        let face_interior = Vec3::new(min.x + 3.0, 0.0, min.z + 5.0);
        let face_interior_pointer = viewport.world_to_screen(rect, face_interior);
        let face = viewport
            .snap_point(rect, &document, face_interior_pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Face, face.kind);
        assert!(face.point.distance(face_interior) < 1.0e-6);

        let mut side_document = meshcraft_core::document::Document::new();
        side_document
            .add_cuboid(
                "side-face",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(2.0, 3.0, 5.0),
            )
            .unwrap();
        let center = Vec3::new(3.0, 4.0, 6.0);
        let mut front = Viewport {
            target: center,
            ..Viewport::default()
        };
        front.set_orthographic_view(OrthoView::Front);
        let front_pointer = front.world_to_screen(rect, center);
        let front_face = front
            .snap_point(rect, &side_document, front_pointer, 8.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Midpoint, front_face.kind);
        assert!((front_face.point.z - 7.0).abs() < 1.0e-6);
        let front_grid = front
            .snap_point(rect, &side_document, front_pointer, 8.0, 1.0)
            .unwrap();
        assert_eq!(SnapKind::Midpoint, front_grid.kind);
        assert!(front_grid.point.distance(Vec3::new(3.0, 4.0, 7.0)) < 1.0e-6);

        let empty = meshcraft_core::document::Document::new();
        let mut top = Viewport::default();
        top.set_orthographic_view(OrthoView::Top);
        let grid_point = Vec3::new(2.0, 0.0, -3.0);
        let grid_pointer = top.world_to_screen(rect, grid_point) + Vec2::new(3.0, 0.0);
        let grid = top
            .snap_point(rect, &empty, grid_pointer, 8.0, 1.0)
            .unwrap();
        assert_eq!(SnapKind::Grid, grid.kind);
        assert_eq!(grid_point, grid.point);

        let line = document
            .add_line_segment(
                "radius",
                Vec3::new(-2.0, 0.0, 8.0),
                Vec3::new(2.0, 0.0, 8.0),
            )
            .unwrap();
        let mut line_viewport = Viewport {
            target: Vec3::new(0.0, 0.0, 8.0),
            ..Viewport::default()
        };
        let start_screen = line_viewport.world_to_screen(rect, Vec3::new(-2.0, 0.0, 8.0));
        let end_screen = line_viewport.world_to_screen(rect, Vec3::new(2.0, 0.0, 8.0));
        let line_direction = (end_screen - start_screen).normalized();
        let outside_radius = start_screen + Vec2::new(-line_direction.y, line_direction.x) * 13.0;
        let free = line_viewport
            .snap_point(rect, &document, outside_radius, 12.0, 1_000.0)
            .unwrap();
        assert_eq!(SnapKind::Free, free.kind);
        assert!(document.resolve_occurrence(&line).is_some());
    }

    #[test]
    fn viewport_pointer_mapping_uses_logical_points_at_multiple_dpi_scales() {
        let document = meshcraft_core::document::Document::new();
        let physical_window = Vec2::new(1920.0, 1080.0);
        let panel_offset = Vec2::new(84.0, 32.0);
        let right_panel_width = 360.0;

        for pixels_per_point in [1.0, 1.5, 2.0] {
            let logical_window = physical_window / pixels_per_point;
            let viewport_size = Vec2::new(
                logical_window.x - panel_offset.x - right_panel_width,
                logical_window.y - panel_offset.y,
            );
            let target_local = viewport_size * Vec2::new(0.63, 0.42);
            let pointer_in_points = panel_offset + target_local;
            let pointer_in_pixels = pointer_in_points * pixels_per_point;
            let pointer_from_backend = pointer_in_pixels / pixels_per_point;
            let context = egui::Context::default();
            let mut raw_input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, logical_window)),
                events: vec![Event::PointerMoved(Pos2::new(
                    pointer_from_backend.x,
                    pointer_from_backend.y,
                ))],
                ..Default::default()
            };
            raw_input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .expect("root viewport exists")
                .native_pixels_per_point = Some(pixels_per_point);

            let mut viewport = Viewport::default();
            let mut mapped = None;
            let mut output = context.run_ui(raw_input, |ui| {
                ui.add_space(panel_offset.y);
                ui.horizontal(|ui| {
                    ui.add_space(panel_offset.x);
                    mapped = Some(viewport.show(
                        ui,
                        &super::ViewportScene {
                            size: viewport_size,
                            document: &document,
                            selection: &[],
                            scope: super::SnapScope::Document,
                            display_policy: &SelectionDisplayPolicy::default(),
                            active_tool: "Select",
                            preview: None,
                            feedback: None,
                            snap: None,
                            guides: &[],
                            guide_preview: None,
                            grid: super::GridStyle {
                                visible: false,
                                spacing: 1.0,
                                extent: 20,
                                normal_axis: super::GridNormalAxis::Y,
                                normal_width: 1.0,
                                thick_width: 3.0,
                            },
                        },
                    ));
                });
            });
            output.textures_delta.clear();

            let mapped = mapped.expect("viewport rendered during DPI test frame");
            assert_eq!(pixels_per_point, context.pixels_per_point());
            assert_eq!(panel_offset, mapped.rect.min.to_vec2());
            assert_eq!(viewport_size, mapped.rect.size());
            assert_eq!(Some(target_local), mapped.pointer_local);

            let world = Vec3::new(3.25, 0.0, -7.5);
            let screen = viewport.world_to_screen(mapped.rect, world);
            let recovered = viewport.screen_to_ground(mapped.rect, screen).unwrap();
            assert!(
                recovered.distance(world) < 1.0e-5,
                "scale={pixels_per_point}"
            );
        }
    }

    #[test]
    fn perspective_projection_scales_with_camera_depth() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0));
        let viewport = Viewport::default();
        let (right, _, eye) = viewport.camera_axes();
        let target_screen = viewport.world_to_screen(rect, viewport.target);
        let near_screen = viewport.world_to_screen(rect, viewport.target + right);
        let farther_screen = viewport.world_to_screen(rect, viewport.target + right - eye * 10.0);
        let near_offset = (near_screen.x - target_screen.x).abs();
        let far_offset = (farther_screen.x - target_screen.x).abs();
        assert!(near_offset > far_offset);
        let expected_ratio = ((viewport.distance + 10.0) / viewport.distance) as f32;
        assert!((near_offset / far_offset - expected_ratio).abs() < 1.0e-4);
    }

    #[test]
    fn perspective_frame_centers_and_fits_geometry_at_large_world_offsets() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 20.0), Vec2::new(1000.0, 700.0));
        let mut document = meshcraft_core::document::Document::new();
        document
            .add_cuboid(
                "far-box",
                Vec3::new(8.0, 6.0, 4.0),
                Vec3::new(100.0, -2.0, -50.0),
            )
            .unwrap();
        let mut viewport = Viewport::default();

        viewport.frame_document(rect, &document);

        assert!(viewport.target.distance(Vec3::new(104.0, 1.0, -48.0)) < 1.0e-10);
        for corner in cuboid_corners(Vec3::new(100.0, -2.0, -50.0), Vec3::new(8.0, 6.0, 4.0)) {
            let screen = viewport.world_to_screen(rect, corner);
            assert!(screen.x >= rect.left() + 35.0 && screen.x <= rect.right() - 35.0);
            assert!(screen.y >= rect.top() + 35.0 && screen.y <= rect.bottom() - 35.0);
        }
    }

    #[test]
    fn wheel_zoom_keeps_the_ground_point_under_the_pointer() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let pointer = Pos2::new(275.0, 510.0);
        let mut viewport = Viewport {
            yaw: 0.7,
            elevation: 0.9,
            ..Viewport::default()
        };
        let ground_anchor = viewport.screen_to_ground(rect, pointer).unwrap();

        viewport.zoom_at(rect, pointer, 450.0);

        assert!(viewport.distance < DEFAULT_PERSPECTIVE_DISTANCE);
        assert!(
            viewport
                .screen_to_ground(rect, pointer)
                .unwrap()
                .distance(ground_anchor)
                < 1.0e-5
        );
    }

    #[test]
    fn triangle_picker_includes_edges_but_rejects_points_outside() {
        let a = Pos2::new(1.0, 1.0);
        let b = Pos2::new(9.0, 1.0);
        let c = Pos2::new(1.0, 9.0);
        assert!(point_in_triangle(Pos2::new(2.0, 2.0), a, b, c));
        assert!(point_in_triangle(Pos2::new(1.0, 5.0), a, b, c));
        assert!(!point_in_triangle(Pos2::new(9.0, 9.0), a, b, c));
    }

    #[test]
    fn native_ray_picker_returns_the_nearest_face_and_its_world_snap() {
        let mut viewport = Viewport::default();
        let mut document = meshcraft_core::document::Document::new();
        let occurrence = document
            .add_cuboid("cube", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let origin = Vec3::new(1.0, 1.0, 10.0);
        let direction = -Vec3::Z;
        let target = viewport
            .pick_selection_target_on_ray(
                &document,
                origin,
                direction,
                0.05,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .expect("the ray hits the top face");

        assert_eq!(occurrence, target.occurrence);
        assert!(matches!(target.entity, SelectedEntity::Triangle(_)));
        let snap = viewport
            .snap_selection_target_on_ray(&document, &target, origin, direction)
            .expect("the hit face supplies a snap point");
        assert_eq!(SnapKind::Face, snap.kind);
        assert!((snap.point.z - 2.0).abs() < 1.0e-10);

        let mut locked_faces = SelectionDisplayPolicy::default();
        locked_faces.set_unlocked(SelectionKind::Face, false);
        let locked_pick = viewport
            .pick_selection_target_on_ray(
                &document,
                origin,
                direction,
                0.05,
                SnapScope::Document,
                &locked_faces,
            )
            .expect("the enclosing object remains selectable");
        assert_eq!(SelectedEntity::Occurrence, locked_pick.entity);
        assert_eq!(
            Some(SnapKind::Midpoint),
            viewport
                .snap_point_on_ray(
                    &document,
                    origin,
                    direction,
                    0.05,
                    SnapScope::Document,
                    &locked_faces,
                )
                .map(|snap| snap.kind)
        );

        let mut hidden_objects = SelectionDisplayPolicy::default();
        hidden_objects.set_visible(SelectionKind::Object, false);
        assert!(
            viewport
                .pick_selection_target_on_ray(
                    &document,
                    origin,
                    direction,
                    0.05,
                    SnapScope::Document,
                    &hidden_objects,
                )
                .is_none()
        );
        assert!(
            viewport
                .snap_point_on_ray(
                    &document,
                    origin,
                    direction,
                    0.05,
                    SnapScope::Document,
                    &hidden_objects,
                )
                .is_none()
        );

        let second_occurrence = document
            .add_cuboid(
                "second cube",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(4.0, 0.0, 0.0),
            )
            .unwrap();
        let second_target = viewport
            .pick_selection_target_on_ray(
                &document,
                Vec3::new(5.0, 1.0, 10.0),
                direction,
                0.05,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .expect("the updated index finds the newly added object");
        assert_eq!(second_occurrence, second_target.occurrence);
        assert_eq!(
            Some(document.visual_revision()),
            viewport.native_pick_index.as_ref().map(|i| i.revision())
        );
    }

    #[test]
    fn semantic_picker_returns_entity_types_and_applies_display_policy() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 50.0), Vec2::new(800.0, 600.0));
        let mut viewport = Viewport::default();
        let mut document = meshcraft_core::document::Document::new();
        let occurrence = document
            .add_cuboid("cube", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let face_point = Vec3::new(0.4, 2.0, 0.6);
        let pointer = viewport.world_to_screen(rect, face_point);
        let mut policy = SelectionDisplayPolicy::default();

        let face = viewport
            .pick_selection_target(rect, &document, pointer, SnapScope::Document, &policy)
            .unwrap();
        assert_eq!(occurrence, face.occurrence);
        assert!(matches!(face.entity, SelectedEntity::Triangle(_)));

        let geometry = document.effective_geometry(&occurrence).unwrap();
        let resolved = document.resolve_occurrence(&occurrence).unwrap();
        let vertex_id = *geometry.vertices.keys().next().unwrap();
        let vertex_point = resolved
            .world_transform
            .transform_point(geometry.vertices[&vertex_id].position);
        let vertex_pointer = viewport.world_to_screen(rect, vertex_point);
        let vertex = viewport
            .pick_selection_target(
                rect,
                &document,
                vertex_pointer,
                SnapScope::Document,
                &policy,
            )
            .unwrap();
        let SelectedEntity::Vertex(picked_vertex) = vertex.entity else {
            panic!(
                "expected a visible vertex candidate, got {:?}",
                vertex.entity
            );
        };
        assert!(geometry.vertices.contains_key(&picked_vertex));
        assert!(
            viewport
                .world_to_screen(
                    rect,
                    resolved
                        .world_transform
                        .transform_point(geometry.vertices[&picked_vertex].position),
                )
                .distance(vertex_pointer)
                <= 5.0
        );

        let (edge_id, edge) = geometry
            .segments
            .values()
            .find_map(|segment| {
                let start = geometry.vertex_position(segment.endpoints[0])?;
                let end = geometry.vertex_position(segment.endpoints[1])?;
                let edge_point = resolved
                    .world_transform
                    .transform_point((start + end) * 0.5);
                let edge_pointer = viewport.world_to_screen(rect, edge_point);
                viewport
                    .pick_selection_target(
                        rect,
                        &document,
                        edge_pointer,
                        SnapScope::Document,
                        &policy,
                    )
                    .filter(|target| target.entity == SelectedEntity::Segment(segment.id))
                    .map(|target| (segment.id, target))
            })
            .expect("the cube has a visible edge that can be picked");
        assert_eq!(SelectedEntity::Segment(edge_id), edge.entity);

        let object = viewport
            .pick_selection_target_for_kind(
                rect,
                &document,
                pointer,
                SnapScope::Document,
                &policy,
                Some(SelectionKind::Object),
            )
            .unwrap();
        assert_eq!(SelectedEntity::Occurrence, object.entity);

        policy.set_unlocked(SelectionKind::Face, false);
        let locked_face = viewport
            .pick_selection_target(rect, &document, pointer, SnapScope::Document, &policy)
            .unwrap();
        assert_eq!(SelectedEntity::Occurrence, locked_face.entity);

        policy.set_visible(SelectionKind::Object, false);
        assert_eq!(
            None,
            viewport.pick_selection_target(rect, &document, pointer, SnapScope::Document, &policy,)
        );
    }

    #[test]
    fn typed_picker_uses_transformed_instance_geometry_and_rejects_occluded_edges() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 50.0), Vec2::new(800.0, 600.0));
        let mut document = meshcraft_core::document::Document::new();
        let source = document
            .add_cuboid("source", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let instance = document
            .copy_root_occurrence(&source, Vec3::new(10.0, 0.0, 0.0))
            .unwrap();
        document
            .add_line_segment(
                "occluded-line",
                Vec3::new(10.25, 0.0, 1.0),
                Vec3::new(11.75, 0.0, 1.0),
            )
            .unwrap();
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Top);
        let pointer = viewport.world_to_screen(rect, Vec3::new(11.0, 2.0, 1.0));
        let target = viewport
            .pick_selection_target(
                rect,
                &document,
                pointer,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .unwrap();

        assert_eq!(
            instance,
            target.occurrence,
            "source={source:?}, target={target:?}, pointer={pointer:?}, source_transform={:?}, instance_transform={:?}",
            document
                .resolve_occurrence(&source)
                .unwrap()
                .world_transform,
            document
                .resolve_occurrence(&instance)
                .unwrap()
                .world_transform
        );
        assert!(matches!(target.entity, SelectedEntity::Triangle(_)));
    }

    #[test]
    fn segment_picker_selects_standalone_lines_with_a_small_screen_tolerance() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 50.0), Vec2::new(800.0, 600.0));
        let mut viewport = Viewport::default();
        let mut document = meshcraft_core::document::Document::new();
        let line = document
            .add_line_segment("line", Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let midpoint = viewport.world_to_screen(rect, Vec3::ZERO);
        assert_eq!(viewport.pick(rect, &document, midpoint), Some(line.clone()));
        assert_eq!(
            viewport.pick(
                rect,
                &document,
                midpoint + Vec2::new(-3.5, 7.0 * 0.866_025_4)
            ),
            None
        );
    }

    #[test]
    fn crossing_window_and_world_volume_selection_use_distinct_geometry_tests() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut document = meshcraft_core::document::Document::new();
        let line = document
            .add_line_segment(
                "crossing-line",
                Vec3::new(-3.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            )
            .unwrap();
        let mut viewport = Viewport::default();
        let center = viewport.world_to_screen(rect, Vec3::ZERO);
        let region = Rect::from_center_size(center, Vec2::new(12.0, 12.0));
        let policy = SelectionDisplayPolicy::default();
        let window = viewport.select_screen_region(
            rect,
            &document,
            region,
            SnapScope::Document,
            &policy,
            false,
        );
        let crossing = viewport.select_screen_region(
            rect,
            &document,
            region,
            SnapScope::Document,
            &policy,
            true,
        );
        assert!(!window.iter().any(|target| {
            target.occurrence == line
                && target.entity
                    == SelectedEntity::Segment(
                        document
                            .effective_geometry(&line)
                            .unwrap()
                            .segments
                            .keys()
                            .next()
                            .copied()
                            .unwrap(),
                    )
        }));
        assert!(crossing.iter().any(|target| {
            target.occurrence == line && matches!(target.entity, SelectedEntity::Segment(_))
        }));

        let mut filtered_policy = SelectionDisplayPolicy::default();
        filtered_policy.set_visible(SelectionKind::Object, false);
        filtered_policy.set_visible(SelectionKind::Edge, false);
        assert!(
            viewport
                .select_screen_region(
                    rect,
                    &document,
                    region,
                    SnapScope::Document,
                    &filtered_policy,
                    true,
                )
                .is_empty()
        );
        assert!(
            viewport
                .select_world_volume(
                    &document,
                    Vec3::new(-0.25, -0.25, -0.25),
                    Vec3::new(0.25, 0.25, 0.25),
                    SnapScope::Document,
                    &filtered_policy,
                )
                .is_empty()
        );

        let cube = document
            .add_cuboid(
                "volume-cube",
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(10.0, 0.0, 0.0),
            )
            .unwrap();
        let inside = viewport.select_world_volume(
            &document,
            Vec3::new(10.5, 0.5, 0.5),
            Vec3::new(11.5, 1.5, 1.5),
            SnapScope::Document,
            &policy,
        );
        assert!(inside.iter().any(|target| {
            target.occurrence == cube && target.entity == SelectedEntity::Occurrence
        }));
        let outside = viewport.select_world_volume(
            &document,
            Vec3::new(20.0, 20.0, 20.0),
            Vec3::new(21.0, 21.0, 21.0),
            SnapScope::Document,
            &policy,
        );
        assert!(outside.is_empty());
    }

    #[test]
    fn orthographic_picker_selects_geometry_at_constant_screen_scale() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 50.0), Vec2::new(800.0, 600.0));
        let mut viewport = Viewport::default();
        viewport.set_orthographic_view(OrthoView::Front);
        viewport.pan = Vec2::ZERO;
        let mut document = meshcraft_core::document::Document::new();
        let line = document
            .add_line_segment(
                "front-line",
                Vec3::new(-2.0, 1.0, 4.0),
                Vec3::new(2.0, 1.0, 4.0),
            )
            .unwrap();
        let midpoint = viewport.world_to_screen(rect, Vec3::new(0.0, 1.0, 4.0));
        assert_eq!(viewport.pick(rect, &document, midpoint), Some(line));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_midpoint_tests {
    use super::*;
    #[test]
    fn native_midpoints_capture_nearby_rays_and_beat_nearest_line_points() {
        let mut doc = Document::new();
        doc.append_planar_mesh_to_scene_root(
            vec![Vec3::new(-2., 0., 0.), Vec3::new(2., 0., 0.)],
            vec![[0, 1]],
            vec![],
        )
        .unwrap();
        let mut viewport = Viewport::default();
        let snap = viewport
            .snap_point_on_ray(
                &doc,
                Vec3::new(0.04, 5., 0.),
                -Vec3::Y,
                0.1,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .unwrap();
        assert_eq!(SnapKind::Midpoint, snap.kind);
        assert!(snap.point.distance(Vec3::ZERO) < 1e-12);
        let ordinary = viewport
            .snap_point_on_ray(
                &doc,
                Vec3::new(0.4, 5., 0.),
                -Vec3::Y,
                0.1,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .unwrap();
        assert_eq!(SnapKind::Line, ordinary.kind);
        viewport.invalidate_document_caches();
        let mut face = Document::new();
        face.append_planar_mesh_to_scene_root(
            vec![
                Vec3::new(-2., 0., 0.),
                Vec3::new(2., 0., 0.),
                Vec3::new(0., 0., 3.),
            ],
            vec![[0, 1]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let snap = viewport
            .snap_point_on_ray(
                &face,
                Vec3::new(-0.98, 5., 1.52),
                -Vec3::Y,
                0.1,
                SnapScope::Document,
                &SelectionDisplayPolicy::default(),
            )
            .unwrap();
        assert_eq!(SnapKind::Midpoint, snap.kind);
        assert!(snap.point.distance(Vec3::new(-1., 0., 1.5)) < 1e-12);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_snap_radius_is_in_pixels_at_every_depth_zoom_and_projection() {
        use crate::native_snap::{SnapTolerance, SnapView};
        for ortho in [None, Some(0.01), Some(1.), Some(100.)] {
            for fov in [30., 90.] {
                for depth in [1., 10., 100.] {
                    for off_center in [false, true] {
                        let eye = Vec3::ZERO;
                        let view =
                            SnapView::new(eye, -Vec3::Z, Vec3::Y, fov, 1000., ortho).unwrap();
                        let focal = 1000. / (2. * (fov.to_radians() * 0.5).tan());
                        let (origin, direction, base_x) = if ortho.is_some() {
                            let x = if off_center { 0.5 } else { 0. };
                            (Vec3::X * x, -Vec3::Z, x)
                        } else {
                            let x = if off_center { 0.5 } else { 0. };
                            (
                                eye,
                                Vec3::new(x, 0., -1.).try_normalized(1e-12).unwrap(),
                                x * depth,
                            )
                        };
                        for pixel_offset in [9., 13.] {
                            let p = Vec3::new(
                                base_x + pixel_offset * ortho.unwrap_or(depth) / focal,
                                0.,
                                -depth,
                            );
                            let mut doc = Document::new();
                            doc.append_line_segment_to_scene_root(
                                p,
                                p + Vec3::Y * ortho.unwrap_or(depth),
                            )
                            .unwrap();
                            let snap = Viewport::default().snap_geometry_on_ray(
                                &doc,
                                origin,
                                direction,
                                SnapTolerance::pixels(view, origin, direction, 12.).unwrap(),
                                SnapScope::Document,
                                &SelectionDisplayPolicy::default(),
                                None,
                            );
                            if pixel_offset < 12. {
                                let snap = snap.expect("9px candidate must snap at any depth/zoom");
                                assert_eq!(SnapKind::Endpoint, snap.kind);
                                assert!((snap.screen_distance - 9.).abs() < 1e-4);
                            } else {
                                assert!(snap.is_none(), "13px candidate exceeded 12px: {snap:?}");
                            }
                        }
                    }
                }
            }
        }
    }
}
