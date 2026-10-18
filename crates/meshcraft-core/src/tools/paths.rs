//! Sampled planar polyline paths and their staged point-input state.

use crate::geometry::polygon::{PolygonSet, PolygonShape};
use crate::geometry::{GeometryTolerance, Vec2, Vec3};
use crate::tools::shapes::PlanarMesh;

const DEFAULT_CLOSE_DISTANCE: f64 = 0.15;
const PATH_EPSILON: f64 = 1.0e-9;
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
        if points
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
        PolylinePath, PolylinePickEffect, PolylineSegmentMode, PolylineToolState, arc_from_center,
        arc_through, build_polyline_mesh,
    };
    use crate::geometry::Vec3;

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
}
