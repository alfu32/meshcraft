//! Selected-profile sweep mesh generation.

use crate::geometry::Vec3;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepProfileSegment {
    pub endpoints: [Vec3; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SweepTriangle {
    pub vertices: [usize; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SweepMesh {
    pub vertices: Vec<Vec3>,
    pub segments: Vec<[usize; 2]>,
    pub triangles: Vec<SweepTriangle>,
}

type PointKey = [i64; 3];
type EdgeKey = [usize; 2];

/// Sweeps selected line profiles along a sampled polyline in the active geometry plane.
///
/// The source profiles are projected onto the plane normal to the first path segment. Their
/// orientation follows the horizontal path heading around `up_axis`; path positions supply the
/// translation. Closed paths wrap their final span to the first section.
pub fn extrude_swipe(
    profile: &[SweepProfileSegment],
    path: &[Vec3],
    closed: bool,
    up_axis: Vec3,
    tolerance: f64,
) -> Result<SweepMesh, &'static str> {
    if profile.is_empty() {
        return Err("Extrude Swipe needs selected profile edges.");
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("Extrude Swipe tolerance must be positive and finite.");
    }
    if path.iter().any(|point| !point.is_finite())
        || profile
            .iter()
            .flat_map(|segment| segment.endpoints)
            .any(|point| !point.is_finite())
    {
        return Err("Extrude Swipe geometry must contain finite points.");
    }

    let mut distinct_path = Vec::with_capacity(path.len());
    for point in path {
        if distinct_path
            .last()
            .is_none_or(|previous: &Vec3| previous.distance(*point) > tolerance)
        {
            distinct_path.push(*point);
        }
    }
    let mut path = distinct_path;
    if closed && path.len() > 1 && path[0].distance(*path.last().unwrap()) <= tolerance {
        path.pop();
    }
    if path.len() < if closed { 3 } else { 2 } {
        return Err("Extrude Swipe needs at least two distinct path points.");
    }

    let up = up_axis
        .try_normalized(tolerance)
        .ok_or("Extrude Swipe needs a valid local vertical axis.")?;
    let segment_count = if closed { path.len() } else { path.len() - 1 };
    let first_direction = (path[1] - path[0])
        .try_normalized(tolerance)
        .ok_or("Extrude Swipe path begins with a degenerate segment.")?;
    let directions = (0..segment_count)
        .map(|index| {
            let next = (index + 1) % path.len();
            (path[next] - path[index])
                .try_normalized(tolerance)
                .ok_or("Extrude Swipe path contains a degenerate segment.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let headings = segment_headings(&directions, up, tolerance);
    let nodes = node_headings(&headings, closed, tolerance);
    let base_heading = nodes[0];
    let node_angles = nodes
        .iter()
        .map(|heading| signed_angle_around_axis(base_heading, *heading, up, tolerance))
        .collect::<Vec<_>>();

    let projected_profile = profile
        .iter()
        .filter_map(|segment| {
            let endpoints = segment.endpoints.map(|point| {
                let relative = point - path[0];
                path[0] + relative - first_direction * relative.dot(first_direction)
            });
            (endpoints[0].distance(endpoints[1]) > tolerance).then_some(endpoints)
        })
        .collect::<Vec<_>>();
    if projected_profile.is_empty() {
        return Err("Extrude Swipe profiles collapse when projected onto the path plane.");
    }

    let origin = path[0];
    let mut mesh = SweepMesh::default();
    let mut point_indices = BTreeMap::<PointKey, usize>::new();
    let mut edge_indices = BTreeSet::<EdgeKey>::new();
    for [profile_start, profile_end] in projected_profile {
        for span in 0..segment_count {
            let next = (span + 1) % path.len();
            let a0 =
                transform_profile_point(profile_start, origin, path[span], up, node_angles[span]);
            let b0 =
                transform_profile_point(profile_end, origin, path[span], up, node_angles[span]);
            let a1 =
                transform_profile_point(profile_start, origin, path[next], up, node_angles[next]);
            let b1 =
                transform_profile_point(profile_end, origin, path[next], up, node_angles[next]);
            let preferred_normal = (b0 - a0).cross(path[next] - path[span]);
            if preferred_normal.length() <= tolerance * tolerance {
                continue;
            }
            let quad = [a0, b0, b1, a1]
                .map(|point| intern_vertex(point, origin, tolerance, &mut mesh, &mut point_indices))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            let [a0, b0, b1, a1] = [quad[0], quad[1], quad[2], quad[3]];
            add_oriented_triangle([a0, b0, b1], preferred_normal, tolerance, &mut mesh);
            add_oriented_triangle([a0, b1, a1], preferred_normal, tolerance, &mut mesh);
            for [start, end] in [[a0, b0], [b0, b1], [b1, a1], [a1, a0]] {
                let edge = if start < end {
                    [start, end]
                } else {
                    [end, start]
                };
                edge_indices.insert(edge);
            }
        }
    }
    if mesh.triangles.is_empty() {
        return Err("Extrude Swipe produced no non-degenerate faces.");
    }
    mesh.segments = edge_indices.into_iter().collect();
    Ok(mesh)
}

fn segment_headings(directions: &[Vec3], up: Vec3, tolerance: f64) -> Vec<Vec3> {
    let fallback = default_heading(up, tolerance);
    let mut headings = Vec::with_capacity(directions.len());
    for direction in directions {
        let horizontal = *direction - up * direction.dot(up);
        let heading = horizontal
            .try_normalized(tolerance)
            .or_else(|| headings.last().copied().or(Some(fallback)));
        headings.push(heading.expect("the default heading is valid"));
    }
    headings
}

fn node_headings(segment_headings: &[Vec3], closed: bool, tolerance: f64) -> Vec<Vec3> {
    if closed {
        return (0..segment_headings.len())
            .map(|index| {
                let previous =
                    segment_headings[(index + segment_headings.len() - 1) % segment_headings.len()];
                let next = segment_headings[index];
                (previous + next).try_normalized(tolerance).unwrap_or(next)
            })
            .collect();
    }
    let mut result = Vec::with_capacity(segment_headings.len() + 1);
    result.push(segment_headings[0]);
    for pair in segment_headings.windows(2) {
        result.push(
            (pair[0] + pair[1])
                .try_normalized(tolerance)
                .unwrap_or(pair[1]),
        );
    }
    result.push(*segment_headings.last().expect("open path has a segment"));
    result
}

fn default_heading(up: Vec3, tolerance: f64) -> Vec3 {
    up.cross(Vec3::X)
        .try_normalized(tolerance)
        .or_else(|| up.cross(Vec3::Z).try_normalized(tolerance))
        .expect("a normalized up axis cannot be parallel to both X and Z")
}

fn signed_angle_around_axis(from: Vec3, to: Vec3, axis: Vec3, tolerance: f64) -> f64 {
    let from = (from - axis * from.dot(axis)).try_normalized(tolerance);
    let to = (to - axis * to.dot(axis)).try_normalized(tolerance);
    let (Some(from), Some(to)) = (from, to) else {
        return 0.0;
    };
    axis.dot(from.cross(to)).atan2(from.dot(to))
}

fn transform_profile_point(point: Vec3, origin: Vec3, node: Vec3, up: Vec3, angle: f64) -> Vec3 {
    let relative = point - origin;
    let rotated = relative * angle.cos()
        + up.cross(relative) * angle.sin()
        + up * (up.dot(relative) * (1.0 - angle.cos()));
    node + rotated
}

fn intern_vertex(
    point: Vec3,
    origin: Vec3,
    tolerance: f64,
    mesh: &mut SweepMesh,
    indices: &mut BTreeMap<PointKey, usize>,
) -> Result<usize, &'static str> {
    let key = point_key(point, origin, tolerance)?;
    if let Some(index) = indices.get(&key) {
        if mesh.vertices[*index].distance(point) > tolerance {
            return Err("Extrude Swipe output exceeds the supported point tolerance.");
        }
        return Ok(*index);
    }
    let index = mesh.vertices.len();
    mesh.vertices.push(point);
    indices.insert(key, index);
    Ok(index)
}

fn point_key(point: Vec3, origin: Vec3, tolerance: f64) -> Result<PointKey, &'static str> {
    let scale = tolerance * 0.5;
    [point.x - origin.x, point.y - origin.y, point.z - origin.z]
        .map(|coordinate| {
            let key = (coordinate / scale).round();
            (key.is_finite() && key.abs() < i64::MAX as f64)
                .then_some(key as i64)
                .ok_or("Extrude Swipe output exceeds the supported coordinate range.")
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "Extrude Swipe output could not be indexed safely.")
}

fn add_oriented_triangle(
    mut vertices: [usize; 3],
    preferred_normal: Vec3,
    tolerance: f64,
    mesh: &mut SweepMesh,
) {
    let [a, b, c] = vertices.map(|index| mesh.vertices[index]);
    let normal = (b - a).cross(c - a);
    if normal.length() <= tolerance * tolerance {
        return;
    }
    if normal.dot(preferred_normal) < 0.0 {
        vertices.swap(1, 2);
    }
    mesh.triangles.push(SweepTriangle { vertices });
}

#[cfg(test)]
mod tests {
    use super::{SweepProfileSegment, extrude_swipe};
    use crate::geometry::Vec3;
    use std::collections::BTreeMap;

    const TOLERANCE: f64 = 1.0e-6;

    fn vertical_profile() -> [SweepProfileSegment; 1] {
        [SweepProfileSegment {
            endpoints: [Vec3::ZERO, Vec3::Y],
        }]
    }

    fn edge_uses(mesh: &super::SweepMesh) -> BTreeMap<[usize; 2], usize> {
        let mut uses = BTreeMap::new();
        for triangle in &mesh.triangles {
            let [a, b, c] = triangle.vertices;
            for [start, end] in [[a, b], [b, c], [c, a]] {
                let edge = if start < end {
                    [start, end]
                } else {
                    [end, start]
                };
                *uses.entry(edge).or_insert(0) += 1;
            }
        }
        uses
    }

    #[test]
    fn straight_profile_sweep_builds_two_oriented_faces_and_four_border_edges() {
        let mesh = extrude_swipe(
            &vertical_profile(),
            &[Vec3::ZERO, Vec3::X * 2.0],
            false,
            Vec3::Y,
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(4, mesh.vertices.len());
        assert_eq!(4, mesh.segments.len());
        assert_eq!(2, mesh.triangles.len());
        assert!(mesh.triangles.iter().all(|triangle| {
            let [a, b, c] = triangle.vertices.map(|index| mesh.vertices[index]);
            (b - a).cross(c - a).z < 0.0
        }));
    }

    #[test]
    fn elbow_sweep_shares_the_join_section_and_emits_a_continuous_surface() {
        let mesh = extrude_swipe(
            &vertical_profile(),
            &[Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Z],
            false,
            Vec3::Y,
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(6, mesh.vertices.len());
        assert_eq!(7, mesh.segments.len());
        assert_eq!(4, mesh.triangles.len());
        assert_eq!(
            3,
            edge_uses(&mesh).values().filter(|uses| **uses == 2).count()
        );
    }

    #[test]
    fn closed_path_wraps_the_last_sweep_span_to_the_first_section() {
        let mesh = extrude_swipe(
            &vertical_profile(),
            &[Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Z, Vec3::Z],
            true,
            Vec3::Y,
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(8, mesh.vertices.len());
        assert_eq!(12, mesh.segments.len());
        assert_eq!(8, mesh.triangles.len());
        let uses = edge_uses(&mesh);
        assert!(uses.values().all(|uses| *uses <= 2));
        assert_eq!(8, uses.values().filter(|uses| **uses == 2).count());
        assert_eq!(8, uses.values().filter(|uses| **uses == 1).count());
    }

    #[test]
    fn invalid_paths_and_collapsed_profiles_fail_without_partial_meshes() {
        assert!(
            extrude_swipe(
                &vertical_profile(),
                &[Vec3::ZERO, Vec3::ZERO],
                false,
                Vec3::Y,
                TOLERANCE,
            )
            .is_err()
        );
        assert!(
            extrude_swipe(
                &[SweepProfileSegment {
                    endpoints: [Vec3::ZERO, Vec3::X],
                }],
                &[Vec3::ZERO, Vec3::X],
                false,
                Vec3::Y,
                TOLERANCE,
            )
            .is_err()
        );
    }

    #[test]
    fn profile_projection_preserves_the_path_origin_offset() {
        let origin = Vec3::new(10.0, 0.0, 0.0);
        let profile = [SweepProfileSegment {
            endpoints: [origin, origin + Vec3::Y],
        }];
        let mesh = extrude_swipe(
            &profile,
            &[origin, origin + Vec3::X * 2.0],
            false,
            Vec3::Y,
            TOLERANCE,
        )
        .unwrap();
        assert!(
            mesh.vertices
                .iter()
                .all(|point| point.x >= 10.0 - TOLERANCE)
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|point| point.x <= 12.0 + TOLERANCE)
        );
    }
}
