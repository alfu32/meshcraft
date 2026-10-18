//! Geometry and point-picking state for the first planar construction tools.

use crate::geometry::{GeometryTolerance, Vec3};

const RECTANGLE_AXIS_EPSILON: f64 = 1.0e-2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanarShapeTool {
    Rectangle,
    SurfaceRectangle,
    Quad,
    Circle,
}

impl PlanarShapeTool {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Rectangle" => Some(Self::Rectangle),
            "Surface Rect" | "Surface Rectangle" => Some(Self::SurfaceRectangle),
            "Quad" => Some(Self::Quad),
            "Circle" => Some(Self::Circle),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::SurfaceRectangle => "Surface Rect",
            Self::Quad => "Quad",
            Self::Circle => "Circle",
        }
    }

    fn pick_count(self) -> usize {
        if self == Self::Quad { 3 } else { 2 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanarMesh {
    pub vertices: Vec<Vec3>,
    pub segments: Vec<[usize; 2]>,
    pub triangles: Vec<[usize; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapePreview {
    pub outline: Vec<Vec3>,
    pub closed: bool,
    pub measurements: Vec<ShapeMeasurement>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeMeasurement {
    pub label: &'static str,
    pub value: f64,
    pub unit_power: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeToolState {
    active: Option<PlanarShapeTool>,
    picks: Vec<Vec3>,
    preferred_normal: Option<Vec3>,
}

impl ShapeToolState {
    pub fn active(&self) -> Option<PlanarShapeTool> {
        self.active
    }

    pub fn picks(&self) -> &[Vec3] {
        &self.picks
    }

    pub fn activate(&mut self, tool: Option<PlanarShapeTool>) {
        self.active = tool;
        self.clear_picks();
    }

    pub fn clear_picks(&mut self) {
        self.picks.clear();
        self.preferred_normal = None;
    }

    /// Records one pick. `Ok(None)` means more points are needed; a completed pick returns its mesh.
    pub fn pick(
        &mut self,
        point: Vec3,
        normal: Option<Vec3>,
        fallback_normal: Vec3,
        circle_segments: u32,
    ) -> Result<Option<PlanarMesh>, &'static str> {
        let Some(tool) = self.active else {
            return Err("No planar shape tool is active.");
        };
        if !point.is_finite() {
            return Err("Pick points must be finite.");
        }
        if self.picks.is_empty() {
            self.preferred_normal = normal
                .filter(|candidate| candidate.is_finite())
                .and_then(|candidate| candidate.try_normalized(1.0e-12))
                .or_else(|| fallback_normal.try_normalized(1.0e-12))
                .or(Some(Vec3::Y));
        }
        self.picks.push(point);
        if self.picks.len() < tool.pick_count() {
            return Ok(None);
        }

        let mesh = build_shape(
            tool,
            &self.picks,
            self.preferred_normal.unwrap_or(Vec3::Y),
            circle_segments,
        );
        self.clear_picks();
        mesh.map(Some)
            .ok_or("Shape points are degenerate or invalid.")
    }

    pub fn preview(
        &self,
        pointer: Vec3,
        fallback_normal: Vec3,
        circle_segments: u32,
    ) -> Option<ShapePreview> {
        let tool = self.active?;
        if !pointer.is_finite() || self.picks.is_empty() {
            return None;
        }
        let normal = self
            .preferred_normal
            .or_else(|| fallback_normal.try_normalized(1.0e-12))
            .unwrap_or(Vec3::Y);
        let mut points = self.picks.clone();
        points.push(pointer);
        let (outline, closed) = match tool {
            PlanarShapeTool::Rectangle => {
                let corners = rectangle_corners(points[0], pointer, normal)?;
                (corners.to_vec(), true)
            }
            PlanarShapeTool::SurfaceRectangle => {
                let corners = surface_rectangle_corners(points[0], pointer, normal)?;
                (corners.to_vec(), true)
            }
            PlanarShapeTool::Quad if self.picks.len() == 1 => (vec![self.picks[0], pointer], false),
            PlanarShapeTool::Quad => {
                let corners = quad_corners(self.picks[0], self.picks[1], pointer)?;
                (corners.to_vec(), true)
            }
            PlanarShapeTool::Circle => {
                let mesh = circle_mesh(self.picks[0], pointer, normal, circle_segments)?;
                (mesh.vertices.into_iter().skip(1).collect(), true)
            }
        };
        Some(ShapePreview {
            measurements: preview_measurements(tool, &points, normal)?,
            outline,
            closed,
        })
    }
}

pub fn build_shape(
    tool: PlanarShapeTool,
    picks: &[Vec3],
    preferred_normal: Vec3,
    circle_segments: u32,
) -> Option<PlanarMesh> {
    if picks.len() != tool.pick_count() || picks.iter().any(|point| !point.is_finite()) {
        return None;
    }
    let normal = preferred_normal.try_normalized(1.0e-12)?;
    match tool {
        PlanarShapeTool::Rectangle => {
            let corners = rectangle_corners(picks[0], picks[1], normal)?;
            Some(quad_mesh(corners, normal))
        }
        PlanarShapeTool::SurfaceRectangle => {
            let corners = surface_rectangle_corners(picks[0], picks[1], normal)?;
            Some(quad_mesh(corners, normal))
        }
        PlanarShapeTool::Quad => {
            let corners = quad_corners(picks[0], picks[1], picks[2])?;
            Some(quad_mesh(corners, normal))
        }
        PlanarShapeTool::Circle => circle_mesh(picks[0], picks[1], normal, circle_segments),
    }
}

/// Builds the legacy two-point rectangle basis from the dominant plane of the pick delta.
pub fn rectangle_corners(start: Vec3, end: Vec3, preferred_normal: Vec3) -> Option<[Vec3; 4]> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let delta = end - start;
    let basis = rectangle_basis(delta, preferred_normal)?;
    let along_u = delta.dot(basis.1);
    let along_v = delta.dot(basis.2);
    let corners = [
        start,
        start + basis.1 * along_u,
        start + basis.1 * along_u + basis.2 * along_v,
        start + basis.2 * along_v,
    ];
    valid_quad(&corners).then_some(corners)
}

/// Builds a two-point rectangle aligned to a picked surface's plane basis.
pub fn surface_rectangle_corners(start: Vec3, end: Vec3, normal: Vec3) -> Option<[Vec3; 4]> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let (axis_u, axis_v) = plane_basis(normal)?;
    let delta = end - start;
    let along_u = delta.dot(axis_u);
    let along_v = delta.dot(axis_v);
    let corners = [
        start,
        start + axis_u * along_u,
        start + axis_u * along_u + axis_v * along_v,
        start + axis_v * along_v,
    ];
    valid_quad(&corners).then_some(corners)
}

/// Returns `[A, B, B + (C - A), C]`, the three-pick parallelogram used by Quad.
pub fn quad_corners(a: Vec3, b: Vec3, c: Vec3) -> Option<[Vec3; 4]> {
    if !a.is_finite() || !b.is_finite() || !c.is_finite() {
        return None;
    }
    let corners = [a, b, b + (c - a), c];
    valid_quad(&corners).then_some(corners)
}

pub fn circle_mesh(
    center: Vec3,
    radius_point: Vec3,
    normal: Vec3,
    segments: u32,
) -> Option<PlanarMesh> {
    if !center.is_finite() || !radius_point.is_finite() || !(3..=256).contains(&segments) {
        return None;
    }
    let normal = normal.try_normalized(1.0e-12)?;
    let (axis_u, axis_v) = plane_basis(normal)?;
    let delta = radius_point - center;
    let radius = delta.dot(axis_u).hypot(delta.dot(axis_v));
    if !radius.is_finite() || radius <= GeometryTolerance::default().linear_at(center.length()) {
        return None;
    }
    let segment_count = segments as usize;
    let mut vertices = Vec::with_capacity(segment_count + 1);
    vertices.push(center);
    for index in 0..segment_count {
        let angle = std::f64::consts::TAU * index as f64 / segment_count as f64;
        vertices.push(center + axis_u * (angle.cos() * radius) + axis_v * (angle.sin() * radius));
    }
    let segments = (0..segment_count)
        .map(|index| [index + 1, (index + 1) % segment_count + 1])
        .collect();
    // The legacy circle fan uses center, next, current winding.
    let triangles = (0..segment_count)
        .map(|index| [0, (index + 1) % segment_count + 1, index + 1])
        .collect();
    Some(PlanarMesh {
        vertices,
        segments,
        triangles,
    })
}

fn quad_mesh(corners: [Vec3; 4], preferred_normal: Vec3) -> PlanarMesh {
    let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
    let (first, second) = if normal.dot(preferred_normal) >= 0.0 {
        ([0, 1, 2], [0, 2, 3])
    } else {
        ([0, 2, 1], [0, 3, 2])
    };
    PlanarMesh {
        vertices: corners.to_vec(),
        segments: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
        triangles: vec![first, second],
    }
}

fn preview_measurements(
    tool: PlanarShapeTool,
    points: &[Vec3],
    normal: Vec3,
) -> Option<Vec<ShapeMeasurement>> {
    let measurement = |label: &'static str, value: f64, unit_power: u8| {
        (value.is_finite() && value >= 0.0).then_some(ShapeMeasurement {
            label,
            value,
            unit_power,
        })
    };
    let mut values = Vec::new();
    match tool {
        PlanarShapeTool::Rectangle => {
            let corners = rectangle_corners(points[0], *points.last()?, normal)?;
            add_rectangle_measurements(&mut values, corners, &measurement)?;
        }
        PlanarShapeTool::SurfaceRectangle => {
            let corners = surface_rectangle_corners(points[0], *points.last()?, normal)?;
            add_rectangle_measurements(&mut values, corners, &measurement)?;
        }
        PlanarShapeTool::Quad if points.len() >= 3 => {
            let side_u = points[1] - points[0];
            let side_v = points[2] - points[0];
            let width = side_u.length();
            let height = side_v.length();
            let area = side_u.cross(side_v).length();
            values.extend([
                measurement("Width", width, 1)?,
                measurement("Height", height, 1)?,
                measurement("Area", area, 2)?,
                measurement("Perimeter", 2.0 * (width + height), 1)?,
            ]);
        }
        PlanarShapeTool::Quad => {}
        PlanarShapeTool::Circle => {
            let delta = *points.last()? - points[0];
            let (axis_u, axis_v) = plane_basis(normal)?;
            let radius = delta.dot(axis_u).hypot(delta.dot(axis_v));
            values.extend([
                measurement("Radius", radius, 1)?,
                measurement("Diameter", 2.0 * radius, 1)?,
                measurement("Perimeter", std::f64::consts::TAU * radius, 1)?,
                measurement("Area", std::f64::consts::PI * radius * radius, 2)?,
            ]);
        }
    }
    Some(values)
}

fn add_rectangle_measurements(
    values: &mut Vec<ShapeMeasurement>,
    corners: [Vec3; 4],
    measurement: &impl Fn(&'static str, f64, u8) -> Option<ShapeMeasurement>,
) -> Option<()> {
    let width = corners[0].distance(corners[1]);
    let height = corners[1].distance(corners[2]);
    values.extend([
        measurement("Width", width, 1)?,
        measurement("Height", height, 1)?,
        measurement("Area", width * height, 2)?,
        measurement("Perimeter", 2.0 * (width + height), 1)?,
    ]);
    Some(())
}

fn valid_quad(corners: &[Vec3; 4]) -> bool {
    let scale = corners
        .iter()
        .map(|point| point.length())
        .fold(0.0, f64::max);
    let tolerance = GeometryTolerance::default().linear_at(scale);
    let first = corners[1] - corners[0];
    let second = corners[3] - corners[0];
    first.is_finite()
        && second.is_finite()
        && first.length() > tolerance
        && second.length() > tolerance
        && first.cross(second).length() > tolerance * tolerance
        && (corners[2] - corners[0]).distance(first + second) <= tolerance * 2.0
}

fn rectangle_basis(delta: Vec3, preferred_normal: Vec3) -> Option<(Vec3, Vec3, Vec3)> {
    let abs_x = delta.x.abs();
    let abs_y = delta.y.abs();
    let abs_z = delta.z.abs();
    let (axis_u, axis_v, normal) = if abs_y <= RECTANGLE_AXIS_EPSILON {
        (Vec3::X, Vec3::Z, Vec3::X.cross(Vec3::Z))
    } else if abs_x <= RECTANGLE_AXIS_EPSILON {
        (Vec3::Y, Vec3::Z, Vec3::Y.cross(Vec3::Z))
    } else if abs_z <= RECTANGLE_AXIS_EPSILON {
        (Vec3::X, Vec3::Y, Vec3::X.cross(Vec3::Y))
    } else {
        let direction = Vec3::new(delta.x, 0.0, delta.z).try_normalized(1.0e-12)?;
        (direction, Vec3::Y, direction.cross(Vec3::Y))
    };
    let normal = normal.try_normalized(1.0e-12)?;
    let normal = if normal.dot(preferred_normal) < 0.0 {
        -normal
    } else {
        normal
    };
    Some((normal, axis_u, axis_v))
}

fn plane_basis(normal: Vec3) -> Option<(Vec3, Vec3)> {
    let normal = normal.try_normalized(1.0e-12)?;
    let reference = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let axis_u = normal.cross(reference).try_normalized(1.0e-12)?;
    let axis_v = normal.cross(axis_u).try_normalized(1.0e-12)?;
    Some((axis_u, axis_v))
}

#[cfg(test)]
mod tests {
    use super::{
        PlanarShapeTool, ShapeToolState, build_shape, circle_mesh, quad_corners,
        surface_rectangle_corners,
    };
    use crate::geometry::Vec3;

    #[test]
    fn rectangle_preserves_plane_and_preferred_winding_for_reversed_picks() {
        let normal = Vec3::Y;
        for picks in [
            [Vec3::ZERO, Vec3::new(3.0, 0.0, 2.0)],
            [Vec3::new(3.0, 0.0, 2.0), Vec3::ZERO],
        ] {
            let mesh = build_shape(PlanarShapeTool::Rectangle, &picks, normal, 24).unwrap();
            assert!(mesh.vertices.iter().all(|point| point.y == 0.0));
            let [a, b, c] = mesh.triangles[0].map(|index| mesh.vertices[index]);
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
        }
    }

    #[test]
    fn surface_rectangle_uses_rotated_normal_and_projects_off_plane_picks() {
        let normal = Vec3::new(1.0, 1.0, 0.0).try_normalized(1.0e-12).unwrap();
        let start = Vec3::new(4.0, -2.0, 1.0);
        let end = start + Vec3::new(2.0, 0.0, 4.0) + normal * 8.0;
        let corners = surface_rectangle_corners(start, end, normal).unwrap();
        assert!(
            corners
                .iter()
                .all(|point| ((*point - start).dot(normal)).abs() < 1.0e-10)
        );
        let mesh =
            build_shape(PlanarShapeTool::SurfaceRectangle, &[start, end], normal, 24).unwrap();
        let [a, b, c] = mesh.triangles[0].map(|index| mesh.vertices[index]);
        assert!((b - a).cross(c - a).dot(normal) > 0.0);
    }

    #[test]
    fn quad_is_the_three_pick_parallelogram_and_rejects_collinear_points() {
        let corners = quad_corners(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 2.0, 3.0),
            Vec3::new(1.0, 5.0, 3.0),
        )
        .unwrap();
        assert_eq!(Vec3::new(4.0, 5.0, 3.0), corners[2]);
        assert!(quad_corners(Vec3::ZERO, Vec3::X, Vec3::X * 2.0).is_none());
    }

    #[test]
    fn circle_uses_configured_segment_count_and_legacy_fan_winding() {
        let mesh = circle_mesh(Vec3::ZERO, Vec3::X * 2.0, Vec3::Y, 32).unwrap();
        assert_eq!(33, mesh.vertices.len());
        assert_eq!(32, mesh.segments.len());
        assert_eq!(32, mesh.triangles.len());
        let [center, next, current] = mesh.triangles[0].map(|index| mesh.vertices[index]);
        assert!((next - center).cross(current - center).dot(Vec3::Y) < 0.0);
        assert!(circle_mesh(Vec3::ZERO, Vec3::ZERO, Vec3::Y, 24).is_none());
        assert!(circle_mesh(Vec3::ZERO, Vec3::X, Vec3::Y, 2).is_none());
    }

    #[test]
    fn shape_tool_state_tracks_picks_and_cancels_without_geometry() {
        let mut state = ShapeToolState::default();
        state.activate(Some(PlanarShapeTool::Quad));
        assert!(state.pick(Vec3::ZERO, None, Vec3::Y, 24).unwrap().is_none());
        assert!(state.pick(Vec3::X, None, Vec3::Y, 24).unwrap().is_none());
        assert_eq!(2, state.picks().len());
        let preview = state.preview(Vec3::Y, Vec3::Y, 24).unwrap();
        assert!(preview.closed);
        assert_eq!(4, preview.outline.len());
        assert_eq!(4, preview.measurements.len());
        assert_eq!("Area", preview.measurements[2].label);
        assert_eq!(1.0, preview.measurements[2].value);
        let mesh = state.pick(Vec3::Y, None, Vec3::Y, 24).unwrap().unwrap();
        assert_eq!(2, mesh.triangles.len());
        assert!(state.picks().is_empty());
        state.activate(Some(PlanarShapeTool::Circle));
        state.pick(Vec3::ZERO, None, Vec3::Y, 24).unwrap();
        let preview = state.preview(Vec3::X * 2.0, Vec3::Y, 24).unwrap();
        assert_eq!("Radius", preview.measurements[0].label);
        assert_eq!(2.0, preview.measurements[0].value);
        assert_eq!("Diameter", preview.measurements[1].label);
        assert_eq!(4.0, preview.measurements[1].value);
        state.clear_picks();
        assert!(state.picks().is_empty());
    }

    #[test]
    fn build_shape_requires_the_tool_specific_number_of_points() {
        assert!(build_shape(PlanarShapeTool::Rectangle, &[Vec3::ZERO], Vec3::Y, 24).is_none());
    }
}
