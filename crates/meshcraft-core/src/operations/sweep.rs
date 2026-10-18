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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepFrame {
    pub origin: Vec3,
    pub normal: Vec3,
    pub u: Vec3,
    pub v: Vec3,
}

type PointKey = [i64; 3];
type EdgeKey = [usize; 2];

/// Builds rotation-minimizing frames along an open or closed 3D path.
pub fn build_sweep_frames(
    path: &[Vec3],
    closed: bool,
    up_axis: Vec3,
    tolerance: f64,
) -> Result<Vec<SweepFrame>, &'static str> {
    let path = clean_spatial_path(path, closed, tolerance)?;
    let up = up_axis
        .try_normalized(tolerance)
        .ok_or("A spatial sweep needs a valid local vertical axis.")?;
    let span_count = if closed { path.len() } else { path.len() - 1 };
    let tangents = (0..span_count)
        .map(|index| {
            let next = (index + 1) % path.len();
            (path[next] - path[index])
                .try_normalized(tolerance)
                .ok_or("A spatial sweep path contains a degenerate span.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let normals = if closed {
        (0..path.len())
            .map(|index| {
                let previous = tangents[(index + tangents.len() - 1) % tangents.len()];
                let next = tangents[index];
                (previous + next).try_normalized(tolerance).unwrap_or(next)
            })
            .collect::<Vec<_>>()
    } else {
        let mut normals = Vec::with_capacity(path.len());
        normals.push(tangents[0]);
        for pair in tangents.windows(2) {
            normals.push(
                (pair[0] + pair[1])
                    .try_normalized(tolerance)
                    .unwrap_or(pair[1]),
            );
        }
        normals.push(*tangents.last().expect("open paths have a span"));
        normals
    };

    let mut frames = Vec::with_capacity(path.len());
    let mut u = initial_frame_u(normals[0], up, tolerance);
    let mut v = normals[0]
        .cross(u)
        .try_normalized(tolerance)
        .ok_or("A spatial sweep could not build its initial frame.")?;
    frames.push(SweepFrame {
        origin: path[0],
        normal: normals[0],
        u,
        v,
    });
    for index in 1..path.len() {
        u = transport_frame_vector(u, normals[index - 1], normals[index], tolerance);
        u = reject_from_axis(u, normals[index]);
        u = u
            .try_normalized(tolerance)
            .unwrap_or_else(|| initial_frame_u(normals[index], up, tolerance));
        v = normals[index]
            .cross(u)
            .try_normalized(tolerance)
            .ok_or("A spatial sweep could not transport its frame.")?;
        frames.push(SweepFrame {
            origin: path[index],
            normal: normals[index],
            u,
            v,
        });
    }

    if closed {
        let closing_u = transport_frame_vector(
            frames.last().expect("closed paths have frames").u,
            normals[path.len() - 1],
            normals[0],
            tolerance,
        );
        let twist = signed_angle_around_axis(closing_u, frames[0].u, normals[0], tolerance);
        for (index, frame) in frames.iter_mut().enumerate().skip(1) {
            let correction = twist * index as f64 / path.len() as f64;
            frame.u = rotate_about_axis(frame.u, frame.normal, correction);
            frame.u = reject_from_axis(frame.u, frame.normal)
                .try_normalized(tolerance)
                .unwrap_or(frame.u);
            frame.v = frame
                .normal
                .cross(frame.u)
                .try_normalized(tolerance)
                .ok_or("A closed spatial sweep could not correct its frame seam.")?;
        }
    }
    Ok(frames)
}

/// Sweeps selected 3D line profiles through rotation-minimizing path frames.
pub fn extrude_swipe_3d(
    profile: &[SweepProfileSegment],
    path: &[Vec3],
    closed: bool,
    up_axis: Vec3,
    tolerance: f64,
) -> Result<SweepMesh, &'static str> {
    if profile.is_empty() {
        return Err("Extrude Swipe 3D needs selected profile edges.");
    }
    if profile
        .iter()
        .flat_map(|segment| segment.endpoints)
        .any(|point| !point.is_finite())
    {
        return Err("Extrude Swipe 3D profiles must contain finite points.");
    }
    let path = clean_spatial_path(path, closed, tolerance)?;
    let frames = build_sweep_frames(&path, closed, up_axis, tolerance)?;
    let profile = profile
        .iter()
        .filter_map(|segment| {
            let coordinates = segment.endpoints.map(|point| {
                let relative = point - path[0];
                [relative.dot(frames[0].u), relative.dot(frames[0].v)]
            });
            (distance_2d(coordinates[0], coordinates[1]) > tolerance).then_some(coordinates)
        })
        .collect::<Vec<_>>();
    build_frame_sweep(&profile, &path, &frames, closed, tolerance)
}

/// Creates a ribbon with the configured width and lateral offset along a 3D path.
pub fn ribbon_3d(
    path: &[Vec3],
    closed: bool,
    width: f64,
    offset: f64,
    up_axis: Vec3,
    tolerance: f64,
) -> Result<SweepMesh, &'static str> {
    if !width.is_finite() || width <= tolerance || !offset.is_finite() {
        return Err("Ribbon width must be positive and its offset finite.");
    }
    let path = clean_spatial_path(path, closed, tolerance)?;
    let frames = build_sweep_frames(&path, closed, up_axis, tolerance)?;
    let profile = [[[offset - width * 0.5, 0.0], [offset + width * 0.5, 0.0]]];
    build_frame_sweep(&profile, &path, &frames, closed, tolerance)
}

fn clean_spatial_path(
    path: &[Vec3],
    closed: bool,
    tolerance: f64,
) -> Result<Vec<Vec3>, &'static str> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("Spatial sweep tolerance must be positive and finite.");
    }
    if path.iter().any(|point| !point.is_finite()) {
        return Err("Spatial sweep path must contain finite points.");
    }
    let mut path = path.iter().copied().fold(Vec::new(), |mut unique, point| {
        if unique
            .last()
            .is_none_or(|previous: &Vec3| previous.distance(point) > tolerance)
        {
            unique.push(point);
        }
        unique
    });
    if closed && path.len() > 1 && path[0].distance(*path.last().unwrap()) <= tolerance {
        path.pop();
    }
    let minimum = if closed { 3 } else { 2 };
    if path.len() < minimum {
        return Err("A spatial sweep needs at least two distinct path points.");
    }
    Ok(path)
}

fn initial_frame_u(normal: Vec3, up: Vec3, tolerance: f64) -> Vec3 {
    reject_from_axis(up, normal)
        .try_normalized(tolerance)
        .or_else(|| reject_from_axis(Vec3::X, normal).try_normalized(tolerance))
        .or_else(|| reject_from_axis(Vec3::Z, normal).try_normalized(tolerance))
        .expect("a frame normal cannot be parallel to both X and Z")
}

fn transport_frame_vector(vector: Vec3, from: Vec3, to: Vec3, tolerance: f64) -> Vec3 {
    let axis = from.cross(to);
    let axis_length = axis.length();
    if axis_length <= tolerance {
        return vector;
    }
    let axis = axis / axis_length;
    let angle = axis_length.atan2(from.dot(to).clamp(-1.0, 1.0));
    rotate_about_axis(vector, axis, angle)
}

fn reject_from_axis(vector: Vec3, axis: Vec3) -> Vec3 {
    vector - axis * vector.dot(axis)
}

fn rotate_about_axis(vector: Vec3, axis: Vec3, angle: f64) -> Vec3 {
    vector * angle.cos()
        + axis.cross(vector) * angle.sin()
        + axis * (axis.dot(vector) * (1.0 - angle.cos()))
}

fn signed_angle_around_axis(from: Vec3, to: Vec3, axis: Vec3, tolerance: f64) -> f64 {
    let from = reject_from_axis(from, axis).try_normalized(tolerance);
    let to = reject_from_axis(to, axis).try_normalized(tolerance);
    let (Some(from), Some(to)) = (from, to) else {
        return 0.0;
    };
    axis.dot(from.cross(to)).atan2(from.dot(to))
}

fn distance_2d(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

fn build_frame_sweep(
    profile: &[[[f64; 2]; 2]],
    path: &[Vec3],
    frames: &[SweepFrame],
    closed: bool,
    tolerance: f64,
) -> Result<SweepMesh, &'static str> {
    let profile = profile
        .iter()
        .copied()
        .filter(|segment| distance_2d(segment[0], segment[1]) > tolerance)
        .collect::<Vec<_>>();
    if profile.is_empty() {
        return Err("Spatial sweep profiles collapse in the initial path frame.");
    }
    let span_count = if closed {
        frames.len()
    } else {
        frames.len() - 1
    };
    let origin = path[0];
    let mut mesh = SweepMesh::default();
    let mut point_indices = BTreeMap::<PointKey, usize>::new();
    let mut edge_indices = BTreeSet::<EdgeKey>::new();
    for [[ax, ay], [bx, by]] in profile {
        for span in 0..span_count {
            let next = (span + 1) % frames.len();
            let a0 = frame_section_point(frames[span], [ax, ay]);
            let b0 = frame_section_point(frames[span], [bx, by]);
            let a1 = frame_section_point(frames[next], [ax, ay]);
            let b1 = frame_section_point(frames[next], [bx, by]);
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
                edge_indices.insert(if start < end {
                    [start, end]
                } else {
                    [end, start]
                });
            }
        }
    }
    if mesh.triangles.is_empty() {
        return Err("Spatial sweep produced no non-degenerate faces.");
    }
    mesh.segments = edge_indices.into_iter().collect();
    Ok(mesh)
}

fn frame_section_point(frame: SweepFrame, [x, y]: [f64; 2]) -> Vec3 {
    frame.origin + frame.u * x + frame.v * y
}

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
    use super::{
        SweepProfileSegment, build_sweep_frames, extrude_swipe, extrude_swipe_3d, ribbon_3d,
    };
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

    #[test]
    fn transported_frames_stay_orthonormal_on_a_nonplanar_path() {
        let path = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::X + Vec3::Y + Vec3::Z,
            Vec3::X + Vec3::Y + Vec3::Z * 2.0,
        ];
        let frames = build_sweep_frames(&path, false, Vec3::Y, TOLERANCE).unwrap();
        assert_eq!(path.len(), frames.len());
        for frame in frames {
            assert!((frame.normal.length() - 1.0).abs() <= TOLERANCE);
            assert!((frame.u.length() - 1.0).abs() <= TOLERANCE);
            assert!((frame.v.length() - 1.0).abs() <= TOLERANCE);
            assert!(frame.normal.dot(frame.u).abs() <= TOLERANCE);
            assert!(frame.normal.dot(frame.v).abs() <= TOLERANCE);
            assert!(frame.u.dot(frame.v).abs() <= TOLERANCE);
            assert!((frame.normal.cross(frame.u).distance(frame.v)) <= TOLERANCE);
        }
    }

    #[test]
    fn spatial_sweep_closes_both_profile_and_path_seams() {
        let path = [
            Vec3::ZERO,
            Vec3::X * 3.0,
            Vec3::X * 3.0 + Vec3::Z * 3.0,
            Vec3::Z * 3.0,
        ];
        let frames = build_sweep_frames(&path, true, Vec3::Y, TOLERANCE).unwrap();
        let u = frames[0].u * 0.5;
        let v = frames[0].v * 0.5;
        let corners = [
            path[0] - u - v,
            path[0] + u - v,
            path[0] + u + v,
            path[0] - u + v,
        ];
        let profile = (0..corners.len())
            .map(|index| SweepProfileSegment {
                endpoints: [corners[index], corners[(index + 1) % corners.len()]],
            })
            .collect::<Vec<_>>();
        let mesh = extrude_swipe_3d(&profile, &path, true, Vec3::Y, TOLERANCE).unwrap();
        assert_eq!(16, mesh.vertices.len());
        assert_eq!(32, mesh.triangles.len());
        assert!(edge_uses(&mesh).values().all(|uses| *uses == 2));
    }

    #[test]
    fn ribbon_uses_configured_width_and_offset_and_handles_collinear_path_points() {
        let mesh = ribbon_3d(
            &[Vec3::ZERO, Vec3::X, Vec3::X * 2.0, Vec3::X * 2.0 + Vec3::Z],
            false,
            2.0,
            3.0,
            Vec3::Y,
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(8, mesh.vertices.len());
        assert_eq!(10, mesh.segments.len());
        assert_eq!(6, mesh.triangles.len());
        let min_y = mesh
            .vertices
            .iter()
            .map(|point| point.y)
            .fold(f64::INFINITY, f64::min);
        let max_y = mesh
            .vertices
            .iter()
            .map(|point| point.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((min_y - 2.0).abs() <= TOLERANCE);
        assert!((max_y - 4.0).abs() <= TOLERANCE);
    }

    #[test]
    fn spatial_sweeps_reject_degenerate_paths_and_ribbon_dimensions() {
        assert!(build_sweep_frames(&[Vec3::ZERO, Vec3::ZERO], false, Vec3::Y, TOLERANCE).is_err());
        assert!(ribbon_3d(&[Vec3::ZERO, Vec3::X], false, 0.0, 0.0, Vec3::Y, TOLERANCE).is_err());
        assert!(
            ribbon_3d(
                &[Vec3::ZERO, Vec3::X],
                false,
                1.0,
                f64::NAN,
                Vec3::Y,
                TOLERANCE
            )
            .is_err()
        );
    }
}
