use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use meshcraft_core::document::derived::DerivedIndexCache;
use meshcraft_core::document::{Document, OccurrenceRef, Rgba8, VertexId};
use meshcraft_core::geometry::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const ISO_X: f64 = std::f64::consts::FRAC_1_SQRT_2;
const DEFAULT_ELEVATION: f64 = 0.615_479_708_670_387_4;
const MIN_ELEVATION: f64 = 0.261_799_387_799_149_4;
const MAX_ELEVATION: f64 = 1.308_996_938_995_747_2;
const DEFAULT_PERSPECTIVE_DISTANCE: f64 = 10.392_304_845_413_264;
const DEFAULT_FIELD_OF_VIEW: f64 = 67.0;
const DEFAULT_NEAR_PLANE: f64 = 0.1;
const DEFAULT_FAR_PLANE: f64 = 500.0;
const WALK_EYE_HEIGHT: f64 = 6.0;
const WALK_LOOK_RADIANS_PER_PIXEL: f64 = 0.25_f64.to_radians();
const WALK_PAN_UNITS_PER_PIXEL: f64 = 0.05;
const WALK_SCROLL_MOVE_SCALE: f64 = 0.2;

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

#[derive(Clone, Copy, Debug)]
pub struct CuboidPreview {
    pub origin: Vec3,
    pub dimensions: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub enum ToolPreview {
    Cuboid(CuboidPreview),
    Line { start: Vec3, end: Vec3 },
    Rectangle { corners: [Vec3; 4] },
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

    fn screen_axes(self) -> (Vec3, Vec3, Vec3) {
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

#[derive(Clone, Copy, Debug)]
pub struct ViewportInput {
    pub rect: Rect,
    pub click: Option<Pos2>,
    pub pointer_local: Option<Vec2>,
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

    fn vector(self) -> Vec3 {
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
    pub selection: &'a [OccurrenceRef],
    pub active_tool: &'a str,
    pub preview: Option<ToolPreview>,
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
    frame_pending: bool,
    adaptive_grid_cloud: Option<AdaptiveGridCloud>,
    derived: DerivedIndexCache,
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
            frame_pending: false,
            adaptive_grid_cloud: None,
            derived: DerivedIndexCache::default(),
        }
    }
}

impl Viewport {
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

        let pointer_position = ui.input(|input| input.pointer.interact_pos());
        let pointer_over_viewport = pointer_position.is_some_and(|position| {
            rect.contains(position) && ui.ctx().layer_id_at(position) == Some(ui.layer_id())
        });
        if pointer_over_viewport {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                if let Some(walkthrough) = &mut self.walkthrough {
                    let scroll_steps = f64::from(scroll) / 40.0;
                    walkthrough.position += walkthrough.forward()
                        * (scroll_steps
                            * self.walkthrough_tuning.move_speed
                            * WALK_SCROLL_MOVE_SCALE);
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

        self.update_adaptive_grid(scene.snap, scene.grid, scene.guides);
        self.draw_grid(&painter, rect, scene.grid);
        self.draw_document(&painter, rect, scene.document, scene.selection);
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
        if let Some(preview) = scene.preview {
            match preview {
                ToolPreview::Cuboid(cuboid) => self.draw_cuboid_preview(&painter, rect, cuboid),
                ToolPreview::Line { start, end } => {
                    self.draw_line_preview(&painter, rect, start, end)
                }
                ToolPreview::Rectangle { corners } => {
                    self.draw_rectangle_preview(&painter, rect, corners);
                }
            }
        }

        if let Some(snap) = scene.snap.filter(|snap| snap.kind != SnapKind::Free)
            && let Some(position) = self.project_if_visible(rect, snap.point)
        {
            let color = match snap.kind {
                SnapKind::Free => Color32::WHITE,
                SnapKind::Face => Color32::from_rgb(255, 218, 77),
                SnapKind::Line => Color32::from_rgb(87, 192, 255),
                SnapKind::Grid => Color32::from_rgb(105, 226, 177),
                SnapKind::GridLine => Color32::from_rgb(68, 211, 183),
                SnapKind::GridGuide => Color32::from_rgb(79, 225, 212),
                SnapKind::AxisGuide => Color32::from_rgb(103, 201, 255),
                SnapKind::Endpoint => Color32::from_rgb(255, 113, 138),
                SnapKind::Midpoint => Color32::from_rgb(203, 149, 255),
            };
            painter.circle_stroke(position, 6.0, Stroke::new(2.0, Color32::BLACK));
            painter.circle_stroke(position, 4.0, Stroke::new(2.0, color));
        }

        let pointer = pointer_position.filter(|_| pointer_over_viewport);
        let click = (scene.active_tool != "Orbit"
            && response.clicked_by(egui::PointerButton::Primary))
        .then(|| response.interact_pointer_pos())
        .flatten();
        ViewportInput {
            rect,
            click,
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
        self.snap_point_with_guides(rect, document, position, snap_radius, grid_spacing, &[])
    }

    pub fn snap_point_with_guides(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
        snap_radius: f32,
        grid_spacing: f64,
        guides: &[ConstructionGuide],
    ) -> Option<SnapResult> {
        let construction_point = self.screen_to_construction_plane(rect, position);
        let ray = self.ray_for_screen(rect, position);
        let mut best = None;
        let radius = snap_radius.max(0.0);
        let mut base_point = construction_point;
        let mut base_normal = self.construction_plane_normal();
        let mut base_depth = f64::INFINITY;

        if let (Some((ray_origin, _ray_direction)), Ok(occurrences)) =
            (ray, document.occurrence_refs())
        {
            for occurrence in occurrences {
                if let Some(bounds) = self.derived.world_bounds(document, &occurrence) {
                    let projected = bounds_corners(bounds)
                        .into_iter()
                        .filter_map(|corner| self.project_if_visible(rect, corner))
                        .collect::<Vec<_>>();
                    if !projected.is_empty() {
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
                        if position.x < min_x - radius
                            || position.x > max_x + radius
                            || position.y < min_y - radius
                            || position.y > max_y + radius
                        {
                            continue;
                        }
                    }
                }

                let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                    continue;
                };
                let Some(geometry) = document.effective_geometry(&occurrence) else {
                    continue;
                };

                for segment in geometry.segments.values() {
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
                            self.consider_snap(
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
                            );
                        }
                    }

                    let midpoint = (start + end) * 0.5;
                    if let Some(screen) = self.project_if_visible(rect, midpoint) {
                        let distance = screen.distance(position);
                        self.consider_snap(
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
                        self.consider_snap(
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
                        );
                    }
                }

                if let Some((ray_origin, ray_direction)) = ray {
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
                        self.consider_snap(
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
    ) {
        for [start, end] in [
            [vertices[0], vertices[1]],
            [vertices[1], vertices[2]],
            [vertices[2], vertices[0]],
        ] {
            let midpoint = (start + end) * 0.5;
            if let Some(screen) = self.project_if_visible(rect, midpoint) {
                self.consider_snap(
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
                );
            }

            let closest = closest_point_on_segment(face_hit, start, end);
            if let Some(screen) = self.project_if_visible(rect, closest) {
                self.consider_snap(
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
                );
            }
        }
    }

    fn construction_plane_normal(&self) -> Vec3 {
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

    /// Returns a rectangle's four corners in the active construction plane.
    pub fn construction_rectangle_corners(&self, first: Vec3, opposite: Vec3) -> [Vec3; 4] {
        let (axis_u, axis_v) = self.orthographic.map_or((Vec3::X, -Vec3::Z), |view| {
            let (right, up, _) = view.screen_axes();
            (right, up)
        });
        let delta = opposite - first;
        let along_u = delta.dot(axis_u);
        let along_v = delta.dot(axis_v);
        let p1 = first + axis_u * along_u;
        let p3 = first + axis_v * along_v;
        [first, p1, p1 + axis_v * along_v, p3]
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
            walkthrough.position += right * (-f64::from(delta.x) * WALK_PAN_UNITS_PER_PIXEL);
            walkthrough.position += screen_up * (f64::from(delta.y) * WALK_PAN_UNITS_PER_PIXEL);
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
        let Ok(occurrences) = document.occurrence_refs() else {
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
        let Ok(occurrences) = document.occurrence_refs() else {
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

    fn point_in_front_of_camera(&self, point: Vec3) -> bool {
        self.orthographic.is_some() || {
            let depth = (-self.camera_direction()).dot(point - self.camera_position());
            depth >= self.near_plane && depth <= self.far_plane
        }
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

    pub fn height_from_screen_position(
        &self,
        rect: Rect,
        base_center: Vec3,
        position: Pos2,
    ) -> Option<f64> {
        let base_screen = self.world_to_screen(rect, base_center);
        if self.orthographic.is_some() {
            let vertical =
                self.world_to_screen(rect, base_center + Vec3::new(0.0, 1.0, 0.0)) - base_screen;
            let length_squared = vertical.length_sq();
            if length_squared <= f32::EPSILON {
                return None;
            }
            let delta = position - base_screen;
            return Some(f64::from(delta.dot(vertical) / length_squared));
        }
        let (_, up, eye) = self.camera_axes();
        let forward = -eye;
        let relative = base_center - self.camera_position();
        let depth = forward.dot(relative);
        let up_offset = up.dot(relative);
        let screen_up = -f64::from(position.y - rect.center().y - self.pan.y)
            / self.focal_length(rect.height());
        let denominator = up.y - screen_up * forward.y;
        (denominator.abs() > 1.0e-12).then(|| (screen_up * depth - up_offset) / denominator)
    }

    pub fn pick(
        &mut self,
        rect: Rect,
        document: &Document,
        position: Pos2,
    ) -> Option<OccurrenceRef> {
        let occurrences = document.occurrence_refs().ok()?;
        let mut nearest: Option<(f32, f64, OccurrenceRef)> = None;
        for occurrence in occurrences {
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
                if position.x < min_x - 5.0
                    || position.x > max_x + 5.0
                    || position.y < min_y - 5.0
                    || position.y > max_y + 5.0
                {
                    continue;
                }
            }
            let resolved = document.resolve_occurrence(&occurrence)?;
            let geometry = document.effective_geometry(&occurrence)?;
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
                    if nearest.as_ref().is_none_or(|(distance, current_depth, _)| {
                        0.0 < *distance || (0.0 == *distance && depth > *current_depth)
                    }) {
                        nearest = Some((0.0, depth, occurrence.clone()));
                    }
                }
            }
            for segment in geometry.segments.values() {
                let (Some(start_local), Some(end_local)) = (
                    geometry.vertex_position(segment.endpoints[0]),
                    geometry.vertex_position(segment.endpoints[1]),
                ) else {
                    continue;
                };
                let start = resolved.world_transform.transform_point(start_local);
                let end = resolved.world_transform.transform_point(end_local);
                if !self.point_in_front_of_camera(start) || !self.point_in_front_of_camera(end) {
                    continue;
                }
                let a = self.world_to_screen(rect, start);
                let b = self.world_to_screen(rect, end);
                let distance = distance_to_segment_squared(position, a, b);
                if distance > 25.0 {
                    continue;
                }
                let depth = (self.view_depth(start) + self.view_depth(end)) * 0.5;
                if nearest
                    .as_ref()
                    .is_none_or(|(current_distance, current_depth, _)| {
                        distance < *current_distance
                            || (distance == *current_distance && depth > *current_depth)
                    })
                {
                    nearest = Some((distance, depth, occurrence.clone()));
                }
            }
        }
        nearest.map(|(_, _, occurrence)| occurrence)
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
            if let (Some(a), Some(b)) = (
                self.project_if_visible(rect, parallel_v_start),
                self.project_if_visible(rect, parallel_v_end),
            ) {
                painter.line_segment([a, b], Stroke::new(width, color));
            }
            if let (Some(a), Some(b)) = (
                self.project_if_visible(rect, parallel_u_start),
                self.project_if_visible(rect, parallel_u_end),
            ) {
                painter.line_segment([a, b], Stroke::new(width, color));
            }
        }
        let origin = self.project_if_visible(rect, Vec3::ZERO);
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
            if let (Some(origin), Some(end)) = (origin, self.project_if_visible(rect, axis)) {
                painter.line_segment([origin, end], Stroke::new(style.thick_width, color));
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
                    if let (Some(start), Some(end)) = (
                        self.project_if_visible(rect, start),
                        self.project_if_visible(rect, end),
                    ) {
                        painter.line_segment([start, end], stroke);
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
                                if let (Some(start), Some(end)) = (
                                    self.project_if_visible(rect, start),
                                    self.project_if_visible(rect, end),
                                ) {
                                    painter.line_segment(
                                        [start, end],
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
                    let color = if preview {
                        Color32::from_rgb(120, 219, 255)
                    } else {
                        Color32::from_rgb(91, 177, 218)
                    };
                    for (axis, axis_color) in [
                        (guide.axis_u, color),
                        (guide.axis_v, Color32::from_rgb(104, 218, 150)),
                        (guide.axis_w, Color32::from_rgb(227, 126, 126)),
                    ] {
                        let start = guide.origin - axis * extent;
                        let end = guide.origin + axis * extent;
                        if let (Some(start), Some(end)) = (
                            self.project_if_visible(rect, start),
                            self.project_if_visible(rect, end),
                        ) {
                            painter.line_segment(
                                [start, end],
                                Stroke::new(style.thick_width.max(1.4), axis_color),
                            );
                        }
                    }
                    if let Some(origin) = self.project_if_visible(rect, guide.origin) {
                        painter.circle_stroke(origin, 5.0, Stroke::new(1.5, color));
                    }
                }
            }
        }
    }

    fn draw_document(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        document: &Document,
        selection: &[OccurrenceRef],
    ) {
        let Ok(occurrences) = document.occurrence_refs() else {
            return;
        };
        let mut faces = Vec::new();
        let mut segments = Vec::new();
        let view_direction = self.camera_direction();
        for occurrence in occurrences {
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            let Some(geometry) = document.effective_geometry(&occurrence) else {
                continue;
            };
            let selected = selection.contains(&occurrence);
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
                if world_points
                    .iter()
                    .any(|point| !self.point_in_front_of_camera(*point))
                {
                    continue;
                }
                let points = world_points.map(|point| self.world_to_screen(rect, point));
                let facing = (world_points[1] - world_points[0])
                    .cross(world_points[2] - world_points[0])
                    .dot(view_direction);
                let depth = world_points
                    .iter()
                    .map(|point| self.view_depth(*point))
                    .sum::<f64>()
                    / 3.0;
                for [a, b] in [[0, 1], [1, 2], [2, 0]] {
                    let edge = edge_key(triangle.vertices[a].id, triangle.vertices[b].id);
                    surface_edges.insert(edge);
                    if facing > 0.0 {
                        visible_surface_edges.insert(edge);
                    }
                }
                let color = color32(triangle.color);
                faces.push((depth, points, color));
            }
            for segment in geometry.segments.values() {
                if surface_edges
                    .contains(&edge_key(segment.endpoints[0].id, segment.endpoints[1].id))
                    && !visible_surface_edges
                        .contains(&edge_key(segment.endpoints[0].id, segment.endpoints[1].id))
                {
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
                if !self.point_in_front_of_camera(start) || !self.point_in_front_of_camera(end) {
                    continue;
                }
                let depth = (self.view_depth(start) + self.view_depth(end)) * 0.5;
                let edge_color = if selected {
                    Color32::from_rgb(244, 166, 77)
                } else if segment.color == Rgba8::WHITE {
                    Color32::from_rgb(62, 66, 72)
                } else {
                    color32(segment.color)
                };
                segments.push((
                    depth,
                    [
                        self.world_to_screen(rect, start),
                        self.world_to_screen(rect, end),
                    ],
                    edge_color,
                ));
            }
        }
        faces.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (_, points, color) in faces {
            painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));
        }
        segments.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (_, points, color) in segments {
            painter.line_segment(points, Stroke::new(1.4, color));
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

    fn draw_rectangle_preview(&self, painter: &egui::Painter, rect: Rect, corners: [Vec3; 4]) {
        if corners
            .iter()
            .any(|corner| !self.point_in_front_of_camera(*corner))
        {
            return;
        }
        let points = corners.map(|point| self.world_to_screen(rect, point));
        painter.add(Shape::convex_polygon(
            points.to_vec(),
            Color32::from_rgba_unmultiplied(235, 105, 70, 36),
            Stroke::NONE,
        ));
        for index in 0..points.len() {
            painter.line_segment(
                [points[index], points[(index + 1) % points.len()]],
                Stroke::new(1.8, Color32::from_rgb(235, 105, 70)),
            );
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

fn edge_key(first: VertexId, second: VertexId) -> (VertexId, VertexId) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
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
        GridNormalAxis, MAX_ELEVATION, MIN_ELEVATION, OrthoView, SnapKind, SnapResult, Viewport,
        WalkthroughCamera, WalkthroughTuning, cuboid_corners, height_in_triangle_xz,
        point_in_triangle,
    };
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
    use meshcraft_core::geometry::Vec3;

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
                        active_tool: "Select",
                        preview: None,
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
            let corners = viewport.construction_rectangle_corners(world, opposite);
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
        viewport.drag_camera(
            rect,
            eframe::egui::PointerButton::Secondary,
            false,
            drag,
            false,
        );
        assert_eq!(0.16, viewport.yaw);
        assert!((viewport.elevation - (DEFAULT_ELEVATION - 0.08)).abs() < 1.0e-12);
        assert_eq!(Vec2::ZERO, viewport.pan);

        let orientation = (viewport.yaw, viewport.elevation);
        let (right, up, _) = viewport.camera_axes();
        let scale = viewport.distance / viewport.focal_length(rect.height());
        let delta = Vec2::new(-4.0, 8.0);
        let expected_target =
            right * (-f64::from(delta.x) * scale) + up * (f64::from(delta.y) * scale);
        viewport.drag_camera(
            rect,
            eframe::egui::PointerButton::Secondary,
            true,
            delta,
            false,
        );
        assert_eq!(orientation, (viewport.yaw, viewport.elevation));
        assert!(viewport.target.distance(expected_target) < 1.0e-10);
        assert_eq!(Vec2::ZERO, viewport.pan);
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
                            active_tool: "Select",
                            preview: None,
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
    fn height_input_inverts_the_camera_elevation_projection() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let viewport = Viewport {
            elevation: 0.9,
            ..Viewport::default()
        };
        let base = Vec3::new(2.0, 0.0, -4.0);
        let height = 3.25;
        let screen = viewport.world_to_screen(rect, base + Vec3::new(0.0, height, 0.0));

        assert!(
            (viewport
                .height_from_screen_position(rect, base, screen)
                .unwrap()
                - height)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn orthographic_height_input_uses_a_visible_vertical_axis_only() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let base = Vec3::new(2.0, 0.0, -4.0);
        let mut front = Viewport::default();
        front.set_orthographic_view(OrthoView::Front);
        let height = 3.25;
        let screen = front.world_to_screen(rect, base + Vec3::new(0.0, height, 0.0));
        assert!(
            (front
                .height_from_screen_position(rect, base, screen)
                .unwrap()
                - height)
                .abs()
                < 1.0e-6
        );

        let mut top = Viewport::default();
        top.set_orthographic_view(OrthoView::Top);
        assert!(
            top.height_from_screen_position(rect, base, rect.center())
                .is_none()
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
