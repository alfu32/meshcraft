use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use meshcraft_core::document::derived::DerivedIndexCache;
use meshcraft_core::document::{Document, OccurrenceRef, Rgba8, VertexId};
use meshcraft_core::geometry::Vec3;
use std::collections::BTreeSet;

const ISO_X: f64 = std::f64::consts::FRAC_1_SQRT_2;
const DEFAULT_ELEVATION: f64 = 0.615_479_708_670_387_4;
const MIN_ELEVATION: f64 = 0.261_799_387_799_149_4;
const MAX_ELEVATION: f64 = 1.308_996_938_995_747_2;

#[derive(Clone, Copy, Debug)]
pub struct CuboidPreview {
    pub origin: Vec3,
    pub dimensions: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub enum ToolPreview {
    Cuboid(CuboidPreview),
    Line { start: Vec3, end: Vec3 },
    GroundRectangle { first: Vec3, opposite: Vec3 },
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
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            Self::Bottom => (
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
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

#[derive(Clone, Copy, Debug)]
pub struct GridStyle {
    pub visible: bool,
    pub spacing: f64,
    pub extent: u32,
    pub normal_width: f32,
    pub thick_width: f32,
}

pub struct ViewportScene<'a> {
    pub size: Vec2,
    pub document: &'a Document,
    pub selection: &'a [OccurrenceRef],
    pub active_tool: &'a str,
    pub preview: Option<ToolPreview>,
    pub grid: GridStyle,
}

pub struct Viewport {
    zoom: f32,
    pan: Vec2,
    yaw: f64,
    elevation: f64,
    orthographic: Option<OrthoView>,
    camera_drag: Option<egui::PointerButton>,
    frame_pending: bool,
    derived: DerivedIndexCache,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 28.0,
            pan: Vec2::ZERO,
            yaw: 0.0,
            elevation: DEFAULT_ELEVATION,
            orthographic: None,
            camera_drag: None,
            frame_pending: false,
            derived: DerivedIndexCache::default(),
        }
    }
}

impl Viewport {
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
                self.zoom_at(
                    rect,
                    pointer_position.expect("viewport pointer was checked"),
                    scroll,
                );
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
                self.drag_camera(
                    button,
                    shift_held,
                    pointer_delta,
                    scene.active_tool == "Orbit",
                );
            } else {
                self.camera_drag = None;
            }
        }

        self.draw_grid(&painter, rect, scene.grid);
        self.draw_document(&painter, rect, scene.document, scene.selection);
        if let Some(preview) = scene.preview {
            match preview {
                ToolPreview::Cuboid(cuboid) => self.draw_cuboid_preview(&painter, rect, cuboid),
                ToolPreview::Line { start, end } => {
                    self.draw_line_preview(&painter, rect, start, end)
                }
                ToolPreview::GroundRectangle { first, opposite } => {
                    self.draw_rectangle_preview(&painter, rect, first, opposite);
                }
            }
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
            let world = right * f64::from(screen.x) + up * f64::from(screen.y);
            return Some(Vec3::new(world.x, 0.0, world.z));
        }
        let screen_x = f64::from((position.x - rect.center().x - self.pan.x) / self.zoom);
        let screen_y = f64::from((position.y - rect.center().y - self.pan.y) / self.zoom);
        let rotated_x_minus_z = screen_x / ISO_X;
        let rotated_x_plus_z = screen_y / (ISO_X * self.elevation.sin());
        let rotated_x = (rotated_x_minus_z + rotated_x_plus_z) * 0.5;
        let rotated_z = (rotated_x_plus_z - rotated_x_minus_z) * 0.5;
        let cosine = self.yaw.cos();
        let sine = self.yaw.sin();
        Some(Vec3::new(
            cosine * rotated_x - sine * rotated_z,
            0.0,
            sine * rotated_x + cosine * rotated_z,
        ))
    }

    pub fn request_frame(&mut self) {
        self.frame_pending = true;
    }

    pub fn reset_camera(&mut self) {
        self.zoom = 28.0;
        self.pan = Vec2::ZERO;
        self.yaw = 0.0;
        self.elevation = DEFAULT_ELEVATION;
        self.orthographic = None;
        self.camera_drag = None;
        self.frame_pending = false;
    }

    pub fn set_orthographic_view(&mut self, view: OrthoView) {
        self.orthographic = Some(view);
        self.camera_drag = None;
        self.frame_pending = true;
    }

    pub fn orthographic_view(&self) -> Option<OrthoView> {
        self.orthographic
    }

    fn orbit_by(&mut self, delta: Vec2) {
        self.yaw += f64::from(delta.x) * 0.008;
        self.elevation =
            (self.elevation + f64::from(delta.y) * 0.008).clamp(MIN_ELEVATION, MAX_ELEVATION);
    }

    fn drag_camera(
        &mut self,
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
            self.pan += delta;
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
        let Some(ground_anchor) = self.screen_to_ground(rect, position) else {
            return;
        };
        self.zoom = (self.zoom * (scroll * 0.001).exp()).clamp(5.0, 160.0);
        self.pan = position - rect.center() - self.projected_offset(ground_anchor) * self.zoom;
    }

    fn frame_document(&mut self, rect: Rect, document: &Document) {
        let Ok(occurrences) = document.occurrence_refs() else {
            return;
        };
        let mut bounds: Option<[f64; 4]> = None;
        for occurrence in occurrences {
            let Some(world_bounds) = self.derived.world_bounds(document, &occurrence) else {
                continue;
            };
            for point in bounds_corners(world_bounds) {
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
        rect.center() + self.pan + self.projected_offset(point) * self.zoom
    }

    fn projected_position(&self, point: Vec3) -> (f64, f64) {
        if let Some(view) = self.orthographic {
            let (right, up, _) = view.screen_axes();
            return (right.dot(point), -up.dot(point));
        }
        let cosine = self.yaw.cos();
        let sine = self.yaw.sin();
        let rotated_x = cosine * point.x + sine * point.z;
        let rotated_z = -sine * point.x + cosine * point.z;
        let projected_x = (rotated_x - rotated_z) * ISO_X;
        let projected_y =
            (rotated_x + rotated_z) * ISO_X * self.elevation.sin() - point.y * self.elevation.cos();
        (projected_x, projected_y)
    }

    fn projected_offset(&self, point: Vec3) -> Vec2 {
        let (x, y) = self.projected_position(point);
        Vec2::new(x as f32, y as f32)
    }

    fn camera_direction(&self) -> Vec3 {
        if let Some(view) = self.orthographic {
            return view.screen_axes().2;
        }
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
        Some(f64::from((base_screen.y - position.y) / self.zoom) / self.elevation.cos())
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
                let points = bounds_corners(bounds).map(|point| self.world_to_screen(rect, point));
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
        if !style.visible {
            return;
        }
        let extent = style.extent.min(256) as i32;
        let world_extent = f64::from(extent) * style.spacing;
        for step in -extent..=extent {
            let coordinate = f64::from(step) * style.spacing;
            let parallel_z_a =
                self.world_to_screen(rect, Vec3::new(coordinate, 0.0, -world_extent));
            let parallel_z_b = self.world_to_screen(rect, Vec3::new(coordinate, 0.0, world_extent));
            let parallel_x_a =
                self.world_to_screen(rect, Vec3::new(-world_extent, 0.0, coordinate));
            let parallel_x_b = self.world_to_screen(rect, Vec3::new(world_extent, 0.0, coordinate));
            let color = if coordinate == 0.0 {
                Color32::from_rgb(129, 171, 127)
            } else {
                Color32::from_rgb(169, 186, 199)
            };
            let width = if coordinate == 0.0 {
                style.thick_width
            } else {
                style.normal_width
            };
            painter.line_segment([parallel_z_a, parallel_z_b], Stroke::new(width, color));
            painter.line_segment([parallel_x_a, parallel_x_b], Stroke::new(width, color));
        }
        let origin = self.world_to_screen(rect, Vec3::ZERO);
        let x_axis = self.world_to_screen(rect, Vec3::new(world_extent, 0.0, 0.0));
        let z_axis = self.world_to_screen(rect, Vec3::new(0.0, 0.0, world_extent));
        let y_axis = self.world_to_screen(rect, Vec3::new(0.0, world_extent, 0.0));
        painter.line_segment(
            [origin, x_axis],
            Stroke::new(style.thick_width, Color32::from_rgb(202, 105, 105)),
        );
        painter.line_segment(
            [origin, z_axis],
            Stroke::new(style.thick_width, Color32::from_rgb(100, 160, 115)),
        );
        painter.line_segment(
            [origin, y_axis],
            Stroke::new(style.thick_width, Color32::from_rgb(100, 130, 190)),
        );
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
        painter.line_segment(
            [
                self.world_to_screen(rect, start),
                self.world_to_screen(rect, end),
            ],
            Stroke::new(1.8, Color32::from_rgb(235, 105, 70)),
        );
    }

    fn draw_rectangle_preview(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        first: Vec3,
        opposite: Vec3,
    ) {
        let points = [
            Vec3::new(first.x, 0.0, first.z),
            Vec3::new(first.x, 0.0, opposite.z),
            Vec3::new(opposite.x, 0.0, opposite.z),
            Vec3::new(opposite.x, 0.0, first.z),
        ]
        .map(|point| self.world_to_screen(rect, point));
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

fn distance_to_segment_squared(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let direction = end - start;
    let length_squared = direction.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance_sq(start);
    }
    let t = ((point - start).dot(direction) / length_squared).clamp(0.0, 1.0);
    point.distance_sq(start + direction * t)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_ELEVATION, MAX_ELEVATION, MIN_ELEVATION, OrthoView, Viewport, point_in_triangle,
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
                        grid: super::GridStyle {
                            visible: false,
                            spacing: 1.0,
                            extent: 20,
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
        assert_eq!(None, viewport.orthographic_view());
        assert!(!viewport.frame_pending);
    }

    #[test]
    fn all_orthographic_views_project_their_screen_axes_without_depth_scaling() {
        let rect = Rect::from_min_size(Pos2::new(60.0, 30.0), Vec2::new(1000.0, 700.0));
        let views = [
            (
                OrthoView::Top,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ),
            (
                OrthoView::Bottom,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
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
    fn only_top_and_bottom_orthographic_views_intersect_the_ground_for_drawing() {
        let rect = Rect::from_min_size(Pos2::new(60.0, 30.0), Vec2::new(1000.0, 700.0));
        for view in [OrthoView::Top, OrthoView::Bottom] {
            let mut viewport = Viewport::default();
            viewport.set_orthographic_view(view);
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
        let mut viewport = Viewport::default();
        let drag = Vec2::new(20.0, -10.0);
        viewport.drag_camera(eframe::egui::PointerButton::Secondary, false, drag, false);
        assert_eq!(0.16, viewport.yaw);
        assert!((viewport.elevation - (DEFAULT_ELEVATION - 0.08)).abs() < 1.0e-12);
        assert_eq!(Vec2::ZERO, viewport.pan);

        let orientation = (viewport.yaw, viewport.elevation);
        viewport.drag_camera(
            eframe::egui::PointerButton::Secondary,
            true,
            Vec2::new(-4.0, 8.0),
            false,
        );
        assert_eq!(orientation, (viewport.yaw, viewport.elevation));
        assert_eq!(Vec2::new(-4.0, 8.0), viewport.pan);
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
        assert_eq!(Vec2::new(20.0, 10.0), viewport.pan);
    }

    #[test]
    fn isometric_projection_and_ground_picking_round_trip_across_orbits() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let mut viewport = Viewport {
            pan: Vec2::new(22.0, -17.0),
            ..Viewport::default()
        };
        for yaw in [0.0, 0.8, 2.4] {
            for elevation in [MIN_ELEVATION, DEFAULT_ELEVATION, MAX_ELEVATION] {
                viewport.yaw = yaw;
                viewport.elevation = elevation;
                let ground = Vec3::new(-7.25, 0.0, 11.5);
                let projected = viewport.world_to_screen(rect, ground);
                let recovered = viewport.screen_to_ground(rect, projected).unwrap();
                assert!(recovered.distance(ground) < 1.0e-5);
            }
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

        assert!(viewport.zoom > 28.0);
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
