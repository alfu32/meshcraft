use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use meshcraft_core::document::{Document, OccurrenceRef, Rgba8, VertexId};
use meshcraft_core::geometry::Vec3;
use std::collections::BTreeSet;

const ISO_X: f64 = std::f64::consts::FRAC_1_SQRT_2;
const ISO_GROUND_Y: f64 = 0.408_248_290_463_863_1;
const ISO_HEIGHT_Y: f64 = 0.816_496_580_927_726_1;

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
    frame_pending: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 28.0,
            pan: Vec2::ZERO,
            yaw: 0.0,
            frame_pending: false,
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

        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                self.zoom = (self.zoom * (scroll * 0.001).exp()).clamp(5.0, 160.0);
            }
            if response.dragged_by(egui::PointerButton::Middle)
                || response.dragged_by(egui::PointerButton::Secondary)
            {
                self.pan += ui.input(|input| input.pointer.delta());
            }
            if scene.active_tool == "Orbit" && response.dragged_by(egui::PointerButton::Primary) {
                self.yaw += f64::from(ui.input(|input| input.pointer.delta().x)) * 0.008;
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

        let pointer = response.hover_pos();
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

    pub fn screen_to_ground(&self, rect: Rect, position: Pos2) -> Vec3 {
        let screen_x = f64::from((position.x - rect.center().x - self.pan.x) / self.zoom);
        let screen_y = f64::from((position.y - rect.center().y - self.pan.y) / self.zoom);
        let rotated_x_minus_z = screen_x / ISO_X;
        let rotated_x_plus_z = screen_y / ISO_GROUND_Y;
        let rotated_x = (rotated_x_minus_z + rotated_x_plus_z) * 0.5;
        let rotated_z = (rotated_x_plus_z - rotated_x_minus_z) * 0.5;
        let cosine = self.yaw.cos();
        let sine = self.yaw.sin();
        Vec3::new(
            cosine * rotated_x - sine * rotated_z,
            0.0,
            sine * rotated_x + cosine * rotated_z,
        )
    }

    pub fn request_frame(&mut self) {
        self.frame_pending = true;
    }

    fn frame_document(&mut self, rect: Rect, document: &Document) {
        let Ok(occurrences) = document.occurrence_refs() else {
            return;
        };
        let mut bounds: Option<[f64; 4]> = None;
        for occurrence in occurrences {
            let (Some(resolved), Some(geometry)) = (
                document.resolve_occurrence(&occurrence),
                document.effective_geometry(&occurrence),
            ) else {
                continue;
            };
            for vertex in geometry.vertices.values() {
                let point = resolved.world_transform.transform_point(vertex.position);
                let rotated_x = self.yaw.cos() * point.x + self.yaw.sin() * point.z;
                let rotated_z = -self.yaw.sin() * point.x + self.yaw.cos() * point.z;
                let projected_x = (rotated_x - rotated_z) * ISO_X;
                let projected_y = (rotated_x + rotated_z) * ISO_GROUND_Y - point.y * ISO_HEIGHT_Y;
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
        let cosine = self.yaw.cos();
        let sine = self.yaw.sin();
        let rotated_x = cosine * point.x + sine * point.z;
        let rotated_z = -sine * point.x + cosine * point.z;
        let projected_x = (rotated_x - rotated_z) * ISO_X;
        let projected_y = (rotated_x + rotated_z) * ISO_GROUND_Y - point.y * ISO_HEIGHT_Y;
        Pos2::new(
            rect.center().x + self.pan.x + projected_x as f32 * self.zoom,
            rect.center().y + self.pan.y + projected_y as f32 * self.zoom,
        )
    }

    pub fn height_from_screen_position(
        &self,
        rect: Rect,
        base_center: Vec3,
        position: Pos2,
    ) -> f64 {
        let base_screen = self.world_to_screen(rect, base_center);
        f64::from((base_screen.y - position.y) / self.zoom) / ISO_HEIGHT_Y
    }

    pub fn pick(&self, rect: Rect, document: &Document, position: Pos2) -> Option<OccurrenceRef> {
        let occurrences = document.occurrence_refs().ok()?;
        let mut nearest: Option<(f32, f64, OccurrenceRef)> = None;
        for occurrence in occurrences {
            let resolved = document.resolve_occurrence(&occurrence)?;
            let geometry = document.effective_geometry(&occurrence)?;
            for triangle in geometry.triangles.values() {
                let a = self.world_to_screen(
                    rect,
                    resolved
                        .world_transform
                        .transform_point(geometry.vertices[&triangle.vertices[0]].position),
                );
                let b = self.world_to_screen(
                    rect,
                    resolved
                        .world_transform
                        .transform_point(geometry.vertices[&triangle.vertices[1]].position),
                );
                let c = self.world_to_screen(
                    rect,
                    resolved
                        .world_transform
                        .transform_point(geometry.vertices[&triangle.vertices[2]].position),
                );
                if point_in_triangle(position, a, b, c) {
                    let depth = triangle
                        .vertices
                        .iter()
                        .map(|vertex| {
                            let point = resolved
                                .world_transform
                                .transform_point(geometry.vertices[vertex].position);
                            point.x + point.z + point.y * 0.5
                        })
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
                let start = resolved
                    .world_transform
                    .transform_point(geometry.vertices[&segment.endpoints[0]].position);
                let end = resolved
                    .world_transform
                    .transform_point(geometry.vertices[&segment.endpoints[1]].position);
                let a = self.world_to_screen(rect, start);
                let b = self.world_to_screen(rect, end);
                let distance = distance_to_segment_squared(position, a, b);
                if distance > 25.0 {
                    continue;
                }
                let depth = (start.x + start.y * 0.5 + start.z + end.x + end.y * 0.5 + end.z) * 0.5;
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
        let view_direction = Vec3::new(
            self.yaw.cos() - self.yaw.sin(),
            1.0,
            self.yaw.sin() + self.yaw.cos(),
        );
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
                let world_points = triangle.vertices.map(|vertex| {
                    resolved
                        .world_transform
                        .transform_point(geometry.vertices[&vertex].position)
                });
                let points = world_points.map(|point| self.world_to_screen(rect, point));
                let facing = (world_points[1] - world_points[0])
                    .cross(world_points[2] - world_points[0])
                    .dot(view_direction);
                let depth = triangle
                    .vertices
                    .iter()
                    .map(|vertex| {
                        let point = resolved
                            .world_transform
                            .transform_point(geometry.vertices[vertex].position);
                        point.x + point.z + point.y * 0.5
                    })
                    .sum::<f64>()
                    / 3.0;
                for [a, b] in [[0, 1], [1, 2], [2, 0]] {
                    let edge = edge_key(triangle.vertices[a], triangle.vertices[b]);
                    surface_edges.insert(edge);
                    if facing > 0.0 {
                        visible_surface_edges.insert(edge);
                    }
                }
                let color = color32(triangle.color);
                faces.push((depth, points, color));
            }
            for segment in geometry.segments.values() {
                if surface_edges.contains(&edge_key(segment.endpoints[0], segment.endpoints[1]))
                    && !visible_surface_edges
                        .contains(&edge_key(segment.endpoints[0], segment.endpoints[1]))
                {
                    continue;
                }
                let start = resolved
                    .world_transform
                    .transform_point(geometry.vertices[&segment.endpoints[0]].position);
                let end = resolved
                    .world_transform
                    .transform_point(geometry.vertices[&segment.endpoints[1]].position);
                let depth = (start.x + start.y * 0.5 + start.z + end.x + end.y * 0.5 + end.z) * 0.5;
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
    use super::{Viewport, point_in_triangle};
    use eframe::egui::{Pos2, Rect, Vec2};
    use meshcraft_core::geometry::Vec3;

    #[test]
    fn isometric_projection_and_ground_picking_round_trip_across_orbits() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0));
        let mut viewport = Viewport {
            pan: Vec2::new(22.0, -17.0),
            ..Viewport::default()
        };
        for yaw in [0.0, 0.8, 2.4] {
            viewport.yaw = yaw;
            let ground = Vec3::new(-7.25, 0.0, 11.5);
            let projected = viewport.world_to_screen(rect, ground);
            let recovered = viewport.screen_to_ground(rect, projected);
            assert!(recovered.distance(ground) < 1.0e-5);
        }
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
        let viewport = Viewport::default();
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
}
