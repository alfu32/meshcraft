//! Sampled planar polyline paths and their staged point-input state.

use crate::geometry::polygon::{PolygonSet, PolygonShape};
use crate::geometry::{GeometryTolerance, Vec2, Vec3};
use crate::tools::shapes::PlanarMesh;
use std::collections::BTreeMap;

const DEFAULT_CLOSE_DISTANCE: f64 = 0.15;
const PATH_EPSILON: f64 = 1.0e-9;
const MESH_PICK_EPSILON: f64 = 1.0e-9;
pub const FACE_OUTLINE_MERGE_TOLERANCE: f64 = 1.0e-4;
pub const POLYLINE_MESH_MERGE_TOLERANCE: f64 = 1.0e-3;
const MAX_ARC_SAMPLES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PolylineSegmentMode {
    #[default]
    Line,
    ArcThrough,
    ArcCenter,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolylinePath {
    pub points: Vec<Vec3>,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolylinePickEffect {
    Started,
    Added,
    NeedArcEndpoint,
    Closed,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolylineToolState {
    active: bool,
    mode: PolylineSegmentMode,
    points: Vec<Vec3>,
    arc_control: Option<Vec3>,
}

/// Geometry emitted when the Mesh tool accepts one world-space point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshPickResult {
    pub segment: Option<[Vec3; 2]>,
    /// Points are ordered as nearest earlier point, previous pick, current pick.
    pub triangle: Option<[Vec3; 3]>,
}

/// Staged point input for the legacy incremental triangle-mesh construction tool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshToolState {
    active: bool,
    points: Vec<Vec3>,
    pointer: Option<Vec3>,
}

impl MeshToolState {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn activate(&mut self, active: bool) {
        self.active = active;
        self.clear_points();
    }

    /// Accepts a point and returns the connecting segment and optional nearest-neighbor triangle.
    pub fn pick(&mut self, point: Vec3) -> Result<MeshPickResult, &'static str> {
        if !self.active {
            return Err("Mesh tool is not active.");
        }
        if !point.is_finite() {
            return Err("Mesh points must be finite.");
        }
        if self
            .points
            .last()
            .is_some_and(|previous| previous.distance(point) <= MESH_PICK_EPSILON)
        {
            return Err("Mesh points must be distinct.");
        }

        let previous = self.points.last().copied();
        let segment = previous.map(|previous| [previous, point]);
        self.points.push(point);
        self.pointer = Some(point);

        let triangle = if self.points.len() >= 3 {
            let current_index = self.points.len() - 1;
            let previous_index = current_index - 1;
            let nearest = self.points[..previous_index]
                .iter()
                .copied()
                .min_by(|left, right| left.distance(point).total_cmp(&right.distance(point)));
            nearest.and_then(|nearest| {
                let previous = self.points[previous_index];
                let area = (previous - nearest).cross(point - nearest).length();
                (area > MESH_PICK_EPSILON).then_some([nearest, previous, point])
            })
        } else {
            None
        };

        Ok(MeshPickResult { segment, triangle })
    }

    pub fn set_pointer(&mut self, point: Option<Vec3>) {
        self.pointer = point.filter(|point| point.is_finite());
    }

    /// Clears pending picks after Enter while keeping Mesh active.
    pub fn finish_chain(&mut self) {
        self.clear_points();
    }

    pub fn cancel(&mut self) {
        self.clear_points();
        self.active = false;
    }

    /// Returns staged edges and the current nearest-neighbor triangle preview.
    pub fn preview(&self) -> Vec<[Vec3; 2]> {
        if !self.active {
            return Vec::new();
        }
        let mut edges = self
            .points
            .windows(2)
            .map(|pair| [pair[0], pair[1]])
            .collect::<Vec<_>>();
        let Some(pointer) = self.pointer else {
            return edges;
        };
        let Some(previous) = self.points.last().copied() else {
            return edges;
        };
        edges.push([previous, pointer]);
        if self.points.len() >= 2 {
            let nearest = self.points[..self.points.len() - 1]
                .iter()
                .copied()
                .min_by(|left, right| left.distance(pointer).total_cmp(&right.distance(pointer)));
            if let Some(nearest) = nearest {
                edges.push([nearest, previous]);
                edges.push([nearest, pointer]);
            }
        }
        edges
    }

    fn clear_points(&mut self) {
        self.points.clear();
        self.pointer = None;
    }
}

impl PolylineToolState {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn mode(&self) -> PolylineSegmentMode {
        self.mode
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn arc_control(&self) -> Option<Vec3> {
        self.arc_control
    }

    pub fn activate(&mut self, active: bool) {
        self.clear();
        self.active = active;
    }

    pub fn set_mode(&mut self, mode: PolylineSegmentMode) {
        self.mode = mode;
        self.arc_control = None;
    }

    pub fn pick(
        &mut self,
        point: Vec3,
        arc_max_length: f64,
    ) -> Result<PolylinePickEffect, &'static str> {
        if !self.active {
            return Err("Polyline is not active.");
        }
        if !point.is_finite() {
            return Err("Polyline points must be finite.");
        }
        if self.points.is_empty() {
            self.points.push(point);
            return Ok(PolylinePickEffect::Started);
        }
        if self.points.len() >= 3 && self.points[0].distance(point) <= DEFAULT_CLOSE_DISTANCE {
            self.arc_control = None;
            return Ok(PolylinePickEffect::Closed);
        }
        match self.mode {
            PolylineSegmentMode::Line => {
                self.append_if_distinct(point);
                Ok(PolylinePickEffect::Added)
            }
            PolylineSegmentMode::ArcThrough | PolylineSegmentMode::ArcCenter => {
                if let Some(control) = self.arc_control.take() {
                    let start = *self.points.last().expect("a staged path has an endpoint");
                    let arc = match self.mode {
                        PolylineSegmentMode::ArcThrough => {
                            arc_through(start, control, point, arc_max_length)
                        }
                        PolylineSegmentMode::ArcCenter => {
                            arc_from_center(start, point, control, arc_max_length)
                        }
                        PolylineSegmentMode::Line => unreachable!(),
                    };
                    for sample in arc.into_iter().skip(1) {
                        self.append_if_distinct(sample);
                    }
                    Ok(PolylinePickEffect::Added)
                } else {
                    self.arc_control = Some(point);
                    Ok(PolylinePickEffect::NeedArcEndpoint)
                }
            }
        }
    }

    pub fn backspace(&mut self) -> bool {
        if self.arc_control.take().is_some() {
            return true;
        }
        self.points.pop().is_some()
    }

    pub fn finish(&mut self) -> Option<PolylinePath> {
        let path = (self.points.len() >= 2).then(|| PolylinePath {
            points: std::mem::take(&mut self.points),
            closed: false,
        });
        self.clear();
        path
    }

    pub fn close(&mut self) -> Option<PolylinePath> {
        let path = (self.points.len() >= 3).then(|| PolylinePath {
            points: std::mem::take(&mut self.points),
            closed: true,
        });
        self.clear();
        path
    }

    pub fn cancel(&mut self) {
        self.clear();
        self.active = false;
    }

    pub fn preview(&self, pointer: Vec3, arc_max_length: f64) -> Option<PolylinePath> {
        if !self.active || self.points.is_empty() || !pointer.is_finite() {
            return None;
        }
        if self.points.len() >= 3 && self.points[0].distance(pointer) <= DEFAULT_CLOSE_DISTANCE {
            return Some(PolylinePath {
                points: self.points.clone(),
                closed: true,
            });
        }
        let mut points = self.points.clone();
        let start = *points.last()?;
        let preview = match (self.mode, self.arc_control) {
            (PolylineSegmentMode::ArcThrough, Some(control)) => {
                arc_through(start, control, pointer, arc_max_length)
            }
            (PolylineSegmentMode::ArcCenter, Some(center)) => {
                arc_from_center(start, pointer, center, arc_max_length)
            }
            _ => vec![start, pointer],
        };
        points.extend(preview.into_iter().skip(1));
        Some(PolylinePath {
            points,
            closed: false,
        })
    }

    fn append_if_distinct(&mut self, point: Vec3) {
        if self
            .points
            .last()
            .is_none_or(|last| last.distance(point) > PATH_EPSILON)
        {
            self.points.push(point);
        }
    }

    fn clear(&mut self) {
        self.mode = PolylineSegmentMode::Line;
        self.points.clear();
        self.arc_control = None;
    }
}

/// Tessellates path arcs so each segment is no longer than `max_length`.
/// Invalid or tiny limits use the legacy 16-sample fallback.
pub fn arc_from_center(start: Vec3, end: Vec3, center: Vec3, max_length: f64) -> Vec<Vec3> {
    if !start.is_finite() || !end.is_finite() || !center.is_finite() {
        return Vec::new();
    }
    let start_vector = start - center;
    let end_vector = end - center;
    let radius = start_vector.length();
    if radius <= PATH_EPSILON {
        return Vec::new();
    }
    let Some(normal) = start_vector.cross(end_vector).try_normalized(PATH_EPSILON) else {
        return straight_fallback(start, end);
    };
    let u = start_vector / radius;
    let Some(v) = normal.cross(u).try_normalized(PATH_EPSILON) else {
        return straight_fallback(start, end);
    };
    let end_angle = end_vector.dot(v).atan2(end_vector.dot(u));
    let delta = end_angle.rem_euclid(std::f64::consts::TAU);
    arc_samples(center, u, v, radius, delta, max_length)
}

/// Samples the circular arc through start, middle and end in their plane.
pub fn arc_through(start: Vec3, middle: Vec3, end: Vec3, max_length: f64) -> Vec<Vec3> {
    if !start.is_finite() || !middle.is_finite() || !end.is_finite() {
        return Vec::new();
    }
    let ab = middle - start;
    let ac = end - start;
    let Some(normal) = ab.cross(ac).try_normalized(PATH_EPSILON) else {
        return straight_fallback(start, end);
    };
    let Some(u) = ab.try_normalized(PATH_EPSILON) else {
        return straight_fallback(start, end);
    };
    let v = normal.cross(u);
    let bx = ab.length();
    let cx = ac.dot(u);
    let cy = ac.dot(v);
    if cy.abs() <= PATH_EPSILON {
        return straight_fallback(start, end);
    }
    let center = start
        + u * (bx * bx * cy / (2.0 * bx * cy))
        + v * ((cx * cx + cy * cy - bx * bx) / (2.0 * cy));
    let start_vector = start - center;
    let middle_vector = middle - center;
    let end_vector = end - center;
    let radius = start_vector.length();
    if radius <= PATH_EPSILON {
        return Vec::new();
    }
    let u = start_vector / radius;
    let v = normal.cross(u);
    let middle_angle = middle_vector.dot(v).atan2(middle_vector.dot(u));
    let end_angle = end_vector.dot(v).atan2(end_vector.dot(u));
    let middle_ccw = middle_angle.rem_euclid(std::f64::consts::TAU);
    let end_ccw = end_angle.rem_euclid(std::f64::consts::TAU);
    let delta = if middle_ccw <= end_ccw {
        end_ccw
    } else {
        end_ccw - std::f64::consts::TAU
    };
    arc_samples(center, u, v, radius, delta, max_length)
}

/// Converts an open or closed path into authored segment and face geometry.
pub fn build_polyline_mesh(path: &PolylinePath) -> Result<PlanarMesh, &'static str> {
    build_polyline_mesh_with_planarity(path, true)
}

fn build_polyline_mesh_with_planarity(
    path: &PolylinePath,
    require_planar: bool,
) -> Result<PlanarMesh, &'static str> {
    let points = sanitized_path_points(path)?;
    if points.len() < 2 {
        return Err("Polyline needs at least two distinct points.");
    }
    let mut segments = (0..points.len().saturating_sub(1))
        .map(|index| [index, index + 1])
        .collect::<Vec<_>>();
    if path.closed {
        if points.len() < 3 {
            return Err("A closed polyline needs at least three distinct points.");
        }
        segments.push([points.len() - 1, 0]);
        let tolerance = GeometryTolerance::default();
        let scale = points
            .iter()
            .map(|point| point.length())
            .fold(0.0, f64::max);
        let origin = points[0];
        let normal = (1..points.len() - 1)
            .map(|index| (points[index] - origin).cross(points[index + 1] - origin))
            .find_map(|candidate| candidate.try_normalized(tolerance.linear_at(scale)))
            .ok_or("Closed polyline points are collinear.")?;
        if require_planar
            && points
                .iter()
                .any(|point| (*point - origin).dot(normal).abs() > tolerance.linear_at(scale))
        {
            return Err("Closed polyline points must lie in one plane.");
        }
        let axis_u = (points[1] - origin)
            .try_normalized(tolerance.linear_at(scale))
            .ok_or("Closed polyline has a zero-length first edge.")?;
        let axis_v = normal.cross(axis_u);
        let projected = points
            .iter()
            .map(|point| {
                let delta = *point - origin;
                Vec2::new(delta.dot(axis_u), delta.dot(axis_v))
            })
            .collect::<Vec<_>>();
        let polygon = PolygonSet {
            shapes: vec![PolygonShape {
                contours: vec![projected.clone()],
            }],
        };
        let triangulated = polygon
            .triangulate()
            .map_err(|diagnostic| diagnostic.message)?;
        let projected_tolerance = tolerance.linear_at(scale);
        let mut vertices = points.clone();
        let triangulation_remap = triangulated
            .points
            .iter()
            .map(|point| {
                let projected_point = Vec2::new(point.x, point.y);
                if let Some(index) = projected
                    .iter()
                    .position(|source| source.distance(projected_point) <= projected_tolerance)
                {
                    index
                } else {
                    let index = vertices.len();
                    vertices.push(origin + axis_u * point.x + axis_v * point.y);
                    index
                }
            })
            .collect::<Vec<_>>();
        let triangles = triangulated
            .triangles
            .into_iter()
            .map(|triangle| triangle.map(|index| triangulation_remap[index as usize]))
            .collect();
        return Ok(PlanarMesh {
            vertices,
            segments,
            triangles,
        });
    }
    Ok(PlanarMesh {
        vertices: points,
        segments,
        triangles: Vec::new(),
    })
}

/// Extracts the outer boundary of a selected triangle set, omitting shared interior edges.
pub fn build_face_outline_mesh(
    triangles: &[[Vec3; 3]],
    merge_tolerance: f64,
) -> Result<PlanarMesh, &'static str> {
    if !merge_tolerance.is_finite() || merge_tolerance <= 0.0 {
        return Err("Face-outline tolerance must be finite and positive.");
    }
    let mut points = BTreeMap::<[i64; 3], Vec3>::new();
    let mut edge_counts = BTreeMap::<([i64; 3], [i64; 3]), usize>::new();
    for triangle in triangles {
        if triangle.iter().any(|point| !point.is_finite()) {
            return Err("Face-outline points must be finite.");
        }
        let keys = [
            quantized_point_key(triangle[0], merge_tolerance)?,
            quantized_point_key(triangle[1], merge_tolerance)?,
            quantized_point_key(triangle[2], merge_tolerance)?,
        ];
        for (point, key) in triangle.iter().zip(keys) {
            points.entry(key).or_insert(*point);
        }
        for [first, second] in [[0, 1], [1, 2], [2, 0]] {
            let first = keys[first];
            let second = keys[second];
            if first == second {
                continue;
            }
            let edge = if first <= second {
                (first, second)
            } else {
                (second, first)
            };
            *edge_counts.entry(edge).or_default() += 1;
        }
    }

    let mut vertex_indices = BTreeMap::<[i64; 3], usize>::new();
    let mut vertices = Vec::new();
    let mut segments = Vec::new();
    for ((first, second), edge_count) in edge_counts {
        if edge_count != 1 {
            continue;
        }
        let first_index = *vertex_indices.entry(first).or_insert_with(|| {
            let index = vertices.len();
            vertices.push(points[&first]);
            index
        });
        let second_index = *vertex_indices.entry(second).or_insert_with(|| {
            let index = vertices.len();
            vertices.push(points[&second]);
            index
        });
        segments.push([first_index, second_index]);
    }

    Ok(PlanarMesh {
        vertices,
        segments,
        triangles: Vec::new(),
    })
}

fn quantized_point_key(point: Vec3, merge_tolerance: f64) -> Result<[i64; 3], &'static str> {
    let mut key = [0; 3];
    for (index, coordinate) in [point.x, point.y, point.z].into_iter().enumerate() {
        let quantized = coordinate / merge_tolerance;
        let rounded = quantized.round();
        if !rounded.is_finite() || rounded < i64::MIN as f64 || rounded >= -(i64::MIN as f64) {
            return Err("Geometry coordinates exceed the supported tolerance range.");
        }
        key[index] = rounded as i64;
    }
    Ok(key)
}

/// Orders selected segments into connected chains and triangulates each chain as a polygon.
/// Open chains are closed by a final polygon edge, matching the legacy Polyline Mesh operation.
pub fn build_polyline_mesh_from_segments(
    segments: &[[Vec3; 2]],
    merge_tolerance: f64,
) -> Result<PlanarMesh, &'static str> {
    if !merge_tolerance.is_finite() || merge_tolerance <= 0.0 {
        return Err("Polyline-mesh tolerance must be finite and positive.");
    }
    let mut point_by_key = BTreeMap::<[i64; 3], Vec3>::new();
    let mut segment_keys = Vec::with_capacity(segments.len());
    let mut adjacency = BTreeMap::<[i64; 3], Vec<usize>>::new();
    for (index, segment) in segments.iter().enumerate() {
        if segment.iter().any(|point| !point.is_finite()) {
            return Err("Polyline-mesh points must be finite.");
        }
        let start = quantized_point_key(segment[0], merge_tolerance)?;
        let end = quantized_point_key(segment[1], merge_tolerance)?;
        point_by_key.entry(start).or_insert(segment[0]);
        point_by_key.entry(end).or_insert(segment[1]);
        segment_keys.push((start, end));
        adjacency.entry(start).or_default().push(index);
        adjacency.entry(end).or_default().push(index);
    }

    let mut start_order = (0..segment_keys.len()).collect::<Vec<_>>();
    start_order.sort_by_key(|index| {
        let (start, end) = segment_keys[*index];
        adjacency[&start].len().min(adjacency[&end].len())
    });
    let mut visited = vec![false; segment_keys.len()];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for seed in start_order {
        if visited[seed] {
            continue;
        }
        let (start, end) = segment_keys[seed];
        let start_degree = adjacency[&start].len();
        let end_degree = adjacency[&end].len();
        let chain_start = if start_degree <= end_degree {
            start
        } else {
            end
        };
        let mut current = chain_start;
        let mut chain_keys = vec![chain_start];
        while let Some(next_segment) = adjacency[&current]
            .iter()
            .copied()
            .find(|index| !visited[*index])
        {
            visited[next_segment] = true;
            let (segment_start, segment_end) = segment_keys[next_segment];
            current = if segment_start == current {
                segment_end
            } else {
                segment_start
            };
            chain_keys.push(current);
            if current == chain_start {
                break;
            }
        }
        if chain_keys.len() > 1 && chain_keys.first() == chain_keys.last() {
            chain_keys.pop();
        }
        if chain_keys.len() < 3 {
            continue;
        }
        let polygon = PolylinePath {
            points: chain_keys.iter().map(|key| point_by_key[key]).collect(),
            closed: true,
        };
        let Ok(mesh) = build_polyline_mesh_with_planarity(&polygon, false) else {
            continue;
        };
        if mesh.triangles.is_empty() {
            continue;
        }
        let vertex_offset = vertices.len();
        vertices.extend(mesh.vertices);
        triangles.extend(
            mesh.triangles
                .into_iter()
                .map(|triangle| triangle.map(|index| index + vertex_offset)),
        );
    }
    if triangles.is_empty() {
        return Err("Selected segments do not form a valid planar polygon.");
    }
    Ok(PlanarMesh {
        vertices,
        segments: Vec::new(),
        triangles,
    })
}

/// Builds two offset rails from a path. The returned paths retain matching point indices,
/// which lets sweep and domain tools reuse the same corner joins.
pub fn build_double_line_paths(
    path: &PolylinePath,
    line_size: f64,
    center_offset: f64,
) -> Result<(Vec<Vec3>, Vec<Vec3>), &'static str> {
    build_double_line_paths_in_plane(path, line_size, center_offset, Vec3::Y)
}

/// Builds two offset rails using `preferred_normal` for straight paths whose plane cannot be
/// inferred from their points alone.
pub fn build_double_line_paths_in_plane(
    path: &PolylinePath,
    line_size: f64,
    center_offset: f64,
    preferred_normal: Vec3,
) -> Result<(Vec<Vec3>, Vec<Vec3>), &'static str> {
    if !line_size.is_finite() || line_size <= 0.0 {
        return Err("Double-line size must be finite and positive.");
    }
    if !center_offset.is_finite() {
        return Err("Double-line offset must be finite.");
    }
    let points = sanitized_path_points(path)?;
    if points.len() < 2 {
        return Err("Double line needs at least two distinct points.");
    }
    if path.closed && points.len() < 3 {
        return Err("A closed double line needs at least three distinct points.");
    }
    let plane_normal = path_plane_normal(&points, preferred_normal);
    let left_offset = center_offset - line_size * 0.5;
    let right_offset = center_offset + line_size * 0.5;
    Ok((
        offset_polyline(&points, left_offset, plane_normal, path.closed),
        offset_polyline(&points, right_offset, plane_normal, path.closed),
    ))
}

/// Creates the boundary segments and triangulated strip between two offset path rails.
pub fn build_double_line_mesh(
    path: &PolylinePath,
    line_size: f64,
    center_offset: f64,
) -> Result<PlanarMesh, &'static str> {
    build_double_line_mesh_in_plane(path, line_size, center_offset, Vec3::Y)
}

/// Creates the double-line ribbon using the active construction plane for straight paths.
pub fn build_double_line_mesh_in_plane(
    path: &PolylinePath,
    line_size: f64,
    center_offset: f64,
    preferred_normal: Vec3,
) -> Result<PlanarMesh, &'static str> {
    let (left, right) =
        build_double_line_paths_in_plane(path, line_size, center_offset, preferred_normal)?;
    let count = left.len();
    let mut vertices = left;
    vertices.extend(right);
    let mut segments = Vec::new();
    let mut triangles = Vec::new();
    let step_count = if path.closed { count } else { count - 1 };
    for index in 0..step_count {
        let next = (index + 1) % count;
        segments.push([index, next]);
        segments.push([count + index, count + next]);
        triangles.push([index, next, count + next]);
        triangles.push([index, count + next, count + index]);
    }
    if !path.closed {
        segments.push([count - 1, count * 2 - 1]);
        segments.push([count, 0]);
    }
    Ok(PlanarMesh {
        vertices,
        segments,
        triangles,
    })
}

fn sanitized_path_points(path: &PolylinePath) -> Result<Vec<Vec3>, &'static str> {
    if path.points.iter().any(|point| !point.is_finite()) {
        return Err("Polyline points must be finite.");
    }
    let mut points = Vec::with_capacity(path.points.len());
    for point in &path.points {
        if points
            .last()
            .is_none_or(|previous: &Vec3| previous.distance(*point) > PATH_EPSILON)
        {
            points.push(*point);
        }
    }
    if path.closed
        && points.len() > 1
        && points[0].distance(*points.last().unwrap()) <= PATH_EPSILON
    {
        points.pop();
    }
    Ok(points)
}

fn path_plane_normal(points: &[Vec3], preferred_normal: Vec3) -> Vec3 {
    if points.len() >= 3 {
        let origin = points[0];
        for index in 1..points.len() - 1 {
            if let Some(normal) = (points[index] - origin)
                .cross(points[index + 1] - origin)
                .try_normalized(PATH_EPSILON)
            {
                return normal;
            }
        }
    }
    let direction = points[1] - points[0];
    if let Some(normal) = preferred_normal.try_normalized(PATH_EPSILON)
        && direction
            .try_normalized(PATH_EPSILON)
            .is_some_and(|direction| direction.dot(normal).abs() < 0.95)
    {
        return normal;
    }
    if direction.y.abs() > direction.length() * 0.95 {
        Vec3::X
    } else {
        Vec3::Y
    }
}

fn offset_polyline(points: &[Vec3], offset: f64, plane_normal: Vec3, closed: bool) -> Vec<Vec3> {
    let mut result = Vec::with_capacity(points.len());
    let count = points.len();
    for index in 0..count {
        let current = points[index];
        let previous = if index > 0 {
            Some(points[index - 1])
        } else if closed {
            Some(points[count - 1])
        } else {
            None
        };
        let next = if index + 1 < count {
            Some(points[index + 1])
        } else if closed {
            Some(points[0])
        } else {
            None
        };
        let perpendicular = |start: Vec3, end: Vec3| {
            (end - start)
                .try_normalized(PATH_EPSILON)
                .and_then(|direction| direction.cross(plane_normal).try_normalized(PATH_EPSILON))
        };
        let previous_perpendicular = previous.and_then(|point| perpendicular(point, current));
        let next_perpendicular = next.and_then(|point| perpendicular(current, point));
        let miter = previous_perpendicular
            .zip(next_perpendicular)
            .and_then(|(previous, next)| {
                let miter = (previous + next).try_normalized(PATH_EPSILON)?;
                let denominator = miter.dot(previous);
                if denominator.abs() <= 0.01 {
                    return None;
                }
                let scale = offset / denominator;
                let miter_limit = offset.abs() * 10.0 + 1.0;
                (scale.abs() <= miter_limit).then_some(miter * scale)
            });
        let fallback = previous_perpendicular.or(next_perpendicular);
        let offset_vector = miter.or_else(|| fallback.map(|direction| direction * offset));
        result.push(current + offset_vector.unwrap_or(Vec3::ZERO));
    }
    result
}

fn straight_fallback(start: Vec3, end: Vec3) -> Vec<Vec3> {
    if start.distance(end) <= PATH_EPSILON {
        vec![start]
    } else {
        vec![start, end]
    }
}

fn arc_samples(
    center: Vec3,
    u: Vec3,
    v: Vec3,
    radius: f64,
    delta: f64,
    max_length: f64,
) -> Vec<Vec3> {
    if !center.is_finite() || !radius.is_finite() || !delta.is_finite() {
        return Vec::new();
    }
    let arc_length = radius * delta.abs();
    let step = if max_length.is_finite() && max_length > 1.0e-4 {
        max_length
    } else {
        arc_length / 16.0
    };
    let steps = ((arc_length / step).ceil() as usize).clamp(1, MAX_ARC_SAMPLES);
    (0..=steps)
        .map(|index| {
            let angle = delta * index as f64 / steps as f64;
            center + u * (radius * angle.cos()) + v * (radius * angle.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        FACE_OUTLINE_MERGE_TOLERANCE, MeshToolState, POLYLINE_MESH_MERGE_TOLERANCE, PolylinePath,
        PolylinePickEffect, PolylineSegmentMode, PolylineToolState, arc_from_center, arc_through,
        build_double_line_mesh, build_double_line_mesh_in_plane, build_double_line_paths,
        build_double_line_paths_in_plane, build_face_outline_mesh, build_polyline_mesh,
        build_polyline_mesh_from_segments,
    };
    use crate::geometry::Vec3;

    #[test]
    fn mesh_tool_adds_path_edges_and_nearest_neighbor_triangles_then_finishes_chain() {
        let mut state = MeshToolState::default();
        state.activate(true);
        assert_eq!(Ok(Default::default()), state.pick(Vec3::ZERO));
        assert_eq!(
            Some([Vec3::ZERO, Vec3::X * 5.0]),
            state.pick(Vec3::X * 5.0).unwrap().segment
        );
        let third = state.pick(Vec3::new(5.0, 0.0, 5.0)).unwrap();
        assert_eq!(
            Some([Vec3::ZERO, Vec3::X * 5.0, Vec3::new(5.0, 0.0, 5.0)]),
            third.triangle
        );

        let fourth = Vec3::new(4.9, 0.0, 4.9);
        let fourth_pick = state.pick(fourth).unwrap();
        assert_eq!(
            Some([Vec3::X * 5.0, Vec3::new(5.0, 0.0, 5.0), fourth]),
            fourth_pick.triangle
        );
        state.set_pointer(Some(Vec3::new(3.0, 0.0, 3.0)));
        assert_eq!(6, state.preview().len());

        state.finish_chain();
        assert!(state.active());
        assert!(state.points().is_empty());
        assert_eq!(Ok(Default::default()), state.pick(Vec3::Y));
        state.cancel();
        assert!(!state.active());
    }

    #[test]
    fn polyline_modes_stage_points_undo_and_finish_open_paths() {
        let mut state = PolylineToolState::default();
        state.activate(true);
        assert_eq!(Ok(PolylinePickEffect::Started), state.pick(Vec3::ZERO, 0.5));
        state.set_mode(PolylineSegmentMode::ArcThrough);
        assert_eq!(
            Ok(PolylinePickEffect::NeedArcEndpoint),
            state.pick(Vec3::new(1.0, 1.0, 0.0), 0.5)
        );
        assert!(state.backspace());
        assert!(state.arc_control().is_none());
        assert_eq!(
            Ok(PolylinePickEffect::NeedArcEndpoint),
            state.pick(Vec3::new(1.0, 1.0, 0.0), 0.5)
        );
        assert_eq!(
            Ok(PolylinePickEffect::Added),
            state.pick(Vec3::new(2.0, 0.0, 0.0), 0.5)
        );
        let path = state.finish().unwrap();
        assert!(!path.closed);
        assert!(path.points.len() > 2);
        assert!(state.active());
    }

    #[test]
    fn center_and_three_point_arcs_are_sampled_to_length_limit() {
        let center_arc = arc_from_center(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::ZERO,
            0.2,
        );
        let three_point_arc = arc_through(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            0.2,
        );
        for points in [center_arc, three_point_arc] {
            assert!(points.len() > 5);
            assert!(
                points
                    .windows(2)
                    .all(|pair| pair[0].distance(pair[1]) <= 0.201)
            );
        }
    }

    #[test]
    fn center_arc_uses_a_center_pick_and_backspace_discards_it_first() {
        let mut state = PolylineToolState::default();
        state.activate(true);
        state.pick(Vec3::new(1.0, 0.0, 0.0), 0.25).unwrap();
        state.set_mode(PolylineSegmentMode::ArcCenter);
        assert_eq!(
            Ok(PolylinePickEffect::NeedArcEndpoint),
            state.pick(Vec3::ZERO, 0.25)
        );
        assert!(state.backspace());
        assert_eq!(
            Ok(PolylinePickEffect::NeedArcEndpoint),
            state.pick(Vec3::ZERO, 0.25)
        );
        assert_eq!(
            Ok(PolylinePickEffect::Added),
            state.pick(Vec3::new(0.0, 1.0, 0.0), 0.25)
        );
        let path = state.finish().unwrap();
        assert!(path.points.len() > 5);
        assert!(
            path.points
                .last()
                .unwrap()
                .distance(Vec3::new(0.0, 1.0, 0.0))
                < 1.0e-9
        );
    }

    #[test]
    fn closed_concave_polyline_emits_boundary_and_triangulated_faces() {
        let path = PolylinePath {
            points: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 3.0),
                Vec3::new(0.0, 0.0, 3.0),
            ],
            closed: true,
        };
        let mesh = build_polyline_mesh(&path).unwrap();
        assert_eq!(6, mesh.segments.len());
        assert_eq!(4, mesh.triangles.len());
        let area = mesh
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum::<f64>();
        assert!((5.0 - area).abs() < 1.0e-8);
    }

    #[test]
    fn face_outline_omits_shared_triangle_edges_and_merges_nearby_corners() {
        let mesh = build_face_outline_mesh(
            &[
                [
                    Vec3::ZERO,
                    Vec3::new(2.0, 0.0, 0.0),
                    Vec3::new(2.0, 0.0, 2.0),
                ],
                [
                    Vec3::new(0.0, 0.00001, 0.0),
                    Vec3::new(2.0, 0.00001, 2.0),
                    Vec3::new(0.0, 0.0, 2.0),
                ],
            ],
            1.0e-4,
        )
        .unwrap();

        assert_eq!(4, mesh.vertices.len());
        assert_eq!(4, mesh.segments.len());
        assert!(mesh.triangles.is_empty());
        assert!(mesh.segments.iter().all(|[first, second]| {
            (mesh.vertices[*first].distance(mesh.vertices[*second]) - 2.0).abs() < 1.0e-9
        }));
    }

    #[test]
    fn face_outline_keeps_disconnected_boundaries_and_rejects_invalid_input() {
        let triangles = [
            [Vec3::ZERO, Vec3::X, Vec3::Z],
            [
                Vec3::new(4.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 1.0),
            ],
        ];
        let mesh = build_face_outline_mesh(&triangles, FACE_OUTLINE_MERGE_TOLERANCE).unwrap();
        assert_eq!(6, mesh.segments.len());
        assert!(build_face_outline_mesh(&triangles, 0.0).is_err());
        assert!(
            build_face_outline_mesh(
                &[[Vec3::ZERO, Vec3::X, Vec3::new(f64::NAN, 0.0, 0.0)]],
                1.0e-4
            )
            .is_err()
        );
        assert!(
            build_face_outline_mesh(&[[Vec3::new(1.0e20, 0.0, 0.0), Vec3::X, Vec3::Z]], 1.0e-4,)
                .is_err()
        );
    }

    #[test]
    fn polyline_mesh_orders_selected_segments_and_triangulates_concave_loops() {
        let points = [
            Vec3::ZERO,
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 3.0),
            Vec3::new(0.0, 0.0, 3.0),
        ];
        let mesh = build_polyline_mesh_from_segments(
            &[
                [points[2], points[3]],
                [points[5], points[0]],
                [points[1], points[2]],
                [points[3], points[4]],
                [points[0], points[1]],
                [points[4], points[5]],
            ],
            POLYLINE_MESH_MERGE_TOLERANCE,
        )
        .unwrap();
        let area = mesh
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum::<f64>();

        assert_eq!(6, mesh.vertices.len());
        assert!(mesh.segments.is_empty());
        assert_eq!(4, mesh.triangles.len());
        assert!((5.0 - area).abs() < 1.0e-8);
    }

    #[test]
    fn polyline_mesh_closes_open_chains_and_joins_nearby_endpoints() {
        let mesh = build_polyline_mesh_from_segments(
            &[
                [Vec3::ZERO, Vec3::X * 2.0],
                [Vec3::new(2.0, 0.0, 0.0004), Vec3::new(2.0, 0.0, 2.0)],
                [Vec3::new(2.0, 0.0, 2.0), Vec3::Z * 2.0],
            ],
            POLYLINE_MESH_MERGE_TOLERANCE,
        )
        .unwrap();
        let area = mesh
            .triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum::<f64>();

        assert!(mesh.segments.is_empty());
        assert_eq!(2, mesh.triangles.len());
        assert!((4.0 - area).abs() < 1.0e-8);
        assert!(
            build_polyline_mesh_from_segments(
                &[[Vec3::ZERO, Vec3::X], [Vec3::X, Vec3::X * 2.0]],
                POLYLINE_MESH_MERGE_TOLERANCE,
            )
            .is_err()
        );
    }

    #[test]
    fn polyline_mesh_projects_nonplanar_chains_for_triangulation() {
        let mesh = build_polyline_mesh_from_segments(
            &[
                [Vec3::ZERO, Vec3::X * 2.0],
                [Vec3::X * 2.0, Vec3::new(2.0, 0.0, 2.0)],
                [Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 0.5, 2.0)],
            ],
            POLYLINE_MESH_MERGE_TOLERANCE,
        )
        .unwrap();

        assert_eq!(2, mesh.triangles.len());
        assert!(mesh.vertices.iter().any(|point| point.y == 0.5));
    }

    #[test]
    fn open_double_line_is_a_closed_ribbon_with_expected_width_and_offset() {
        let path = PolylinePath {
            points: vec![Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0)],
            closed: false,
        };
        let (left, right) = build_double_line_paths(&path, 1.0, 0.0).unwrap();
        assert_eq!(2, left.len());
        assert_eq!(2, right.len());
        for index in 0..2 {
            assert!((left[index].z.abs() - 0.5).abs() < 1.0e-9);
            assert!((right[index].z.abs() - 0.5).abs() < 1.0e-9);
            assert!((left[index].z + right[index].z).abs() < 1.0e-9);
        }
        let mesh = build_double_line_mesh(&path, 1.0, 0.0).unwrap();
        assert_eq!(4, mesh.segments.len());
        assert_eq!(2, mesh.triangles.len());
        let [a, b, c] = mesh.triangles[0].map(|index| mesh.vertices[index]);
        assert!(((b - a).cross(c - a).length() * 0.5 - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn double_line_miters_stay_finite_and_closed_paths_keep_two_boundary_loops() {
        let open_corner = PolylinePath {
            points: vec![
                Vec3::ZERO,
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(2.001, 0.0, 2.0),
            ],
            closed: false,
        };
        let open_mesh = build_double_line_mesh(&open_corner, 1.0, 0.25).unwrap();
        assert!(open_mesh.vertices.iter().all(|point| point.is_finite()));
        assert_eq!(6, open_mesh.segments.len());
        assert_eq!(4, open_mesh.triangles.len());

        let closed = PolylinePath {
            points: vec![
                Vec3::ZERO,
                Vec3::new(4.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 3.0),
                Vec3::new(0.0, 0.0, 3.0),
            ],
            closed: true,
        };
        let closed_mesh = build_double_line_mesh(&closed, 0.5, 0.0).unwrap();
        assert_eq!(8, closed_mesh.segments.len());
        assert_eq!(8, closed_mesh.triangles.len());
        assert!(closed_mesh.vertices.iter().all(|point| point.is_finite()));
        assert!(
            !closed_mesh
                .segments
                .iter()
                .any(|[a, b]| { (*a < 4 && *b >= 4) || (*b < 4 && *a >= 4) })
        );
    }

    #[test]
    fn double_line_rejects_invalid_width_offsets_and_degenerate_closed_paths() {
        let open = PolylinePath {
            points: vec![Vec3::ZERO, Vec3::X],
            closed: false,
        };
        assert!(build_double_line_mesh(&open, 0.0, 0.0).is_err());
        assert!(build_double_line_mesh(&open, 1.0, f64::NAN).is_err());
        let closed = PolylinePath {
            points: vec![Vec3::ZERO, Vec3::X],
            closed: true,
        };
        assert!(build_double_line_mesh(&closed, 1.0, 0.0).is_err());
    }

    #[test]
    fn straight_double_line_uses_the_active_construction_plane_normal() {
        let path = PolylinePath {
            points: vec![Vec3::ZERO, Vec3::Z * 2.0],
            closed: false,
        };
        let (left, right) = build_double_line_paths_in_plane(&path, 1.0, 0.0, Vec3::X).unwrap();
        assert!(left.iter().all(|point| point.x.abs() < 1.0e-9));
        assert!(right.iter().all(|point| point.x.abs() < 1.0e-9));
        assert!((left[0].y.abs() - 0.5).abs() < 1.0e-9);
        assert!((right[0].y.abs() - 0.5).abs() < 1.0e-9);
        let mesh = build_double_line_mesh_in_plane(&path, 1.0, 0.0, Vec3::X).unwrap();
        assert!(mesh.vertices.iter().all(|point| point.x.abs() < 1.0e-9));
    }
}
