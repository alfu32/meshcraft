//! Selected-profile surface-of-revolution mesh generation.

use super::sweep::{SweepMesh, SweepProfileSegment, SweepTriangle};
use crate::geometry::{LEGACY_LINE_STORE_EPSILON, Vec3};
use std::collections::{BTreeMap, BTreeSet};

const MAX_REVOLUTION_SEGMENTS: usize = 4096;
const FULL_TURN_EPSILON: f64 = 1.0e-10;

type PointKey = [i64; 3];
type EdgeKey = [usize; 2];

#[derive(Clone, Debug)]
struct ProfileChain {
    points: Vec<Vec3>,
    closed: bool,
}

/// Revolves selected profile edges about an axis and returns a welded triangle surface.
///
/// A full turn shares its first and last angular ring. Partial turns keep both end rings open,
/// matching the legacy revolve tool's uncapped surface output.
pub fn revolve(
    profile: &[SweepProfileSegment],
    axis_origin: Vec3,
    axis_direction: Vec3,
    angle_radians: f64,
    segments: usize,
    tolerance: f64,
) -> Result<SweepMesh, &'static str> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("Revolve tolerance must be positive and finite.");
    }
    if !axis_origin.is_finite() || !axis_direction.is_finite() {
        return Err("Revolve axis points must be finite.");
    }
    let Some(axis) = axis_direction.try_normalized(tolerance) else {
        return Err("Revolve axis direction is too short.");
    };
    if !angle_radians.is_finite()
        || angle_radians.abs() <= FULL_TURN_EPSILON
        || angle_radians.abs() > std::f64::consts::TAU + FULL_TURN_EPSILON
    {
        return Err("Revolve angle must be non-zero and no greater than 360 degrees.");
    }
    if !(3..=MAX_REVOLUTION_SEGMENTS).contains(&segments) {
        return Err("Revolve needs between 3 and 4096 angular segments.");
    }
    if profile.is_empty()
        || profile
            .iter()
            .flat_map(|segment| segment.endpoints)
            .any(|point| !point.is_finite())
    {
        return Err("Revolve needs finite selected profile edges.");
    }

    let chains = ordered_profile_chains(
        profile,
        axis_origin,
        axis,
        angle_radians.signum(),
        tolerance,
    )?;
    let full_turn = (angle_radians.abs() - std::f64::consts::TAU).abs() <= FULL_TURN_EPSILON;
    let angle = if full_turn {
        angle_radians.signum() * std::f64::consts::TAU
    } else {
        angle_radians
    };
    let ring_count = if full_turn { segments } else { segments + 1 };
    let mut mesh = SweepMesh::default();
    let mut point_indices = BTreeMap::<PointKey, usize>::new();
    let mut edge_indices = BTreeSet::<EdgeKey>::new();

    for chain in chains {
        let edge_count = if chain.closed {
            chain.points.len()
        } else {
            chain.points.len() - 1
        };
        let rings = (0..ring_count)
            .map(|step| {
                let theta = angle * step as f64 / segments as f64;
                chain
                    .points
                    .iter()
                    .map(|point| rotate_point(*point, axis_origin, axis, theta))
                    .map(|point| {
                        intern_vertex(point, axis_origin, tolerance, &mut mesh, &mut point_indices)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;

        for step in 0..segments {
            let next_step = if full_turn {
                (step + 1) % ring_count
            } else {
                step + 1
            };
            for edge in 0..edge_count {
                let next_edge = (edge + 1) % chain.points.len();
                let a0 = rings[step][edge];
                let b0 = rings[step][next_edge];
                let b1 = rings[next_step][next_edge];
                let a1 = rings[next_step][edge];
                let profile_tangent = mesh.vertices[b0] - mesh.vertices[a0];
                let angular_tangent = mesh.vertices[a1] - mesh.vertices[a0];
                let preferred_normal = angular_tangent.cross(profile_tangent);
                add_oriented_triangle([a0, a1, b1], preferred_normal, tolerance, &mut mesh);
                add_oriented_triangle([a0, b1, b0], preferred_normal, tolerance, &mut mesh);
                for [start, end] in [[a0, b0], [b0, b1], [b1, a1], [a1, a0]] {
                    if start != end {
                        edge_indices.insert(ordered_edge(start, end));
                    }
                }
            }
        }
    }

    if mesh.triangles.is_empty() {
        return Err("Revolve produced no non-degenerate faces.");
    }
    mesh.segments = edge_indices.into_iter().collect();
    Ok(mesh)
}

fn ordered_profile_chains(
    profile: &[SweepProfileSegment],
    origin: Vec3,
    axis: Vec3,
    angle_sign: f64,
    tolerance: f64,
) -> Result<Vec<ProfileChain>, &'static str> {
    let key_step = LEGACY_LINE_STORE_EPSILON.max(tolerance);
    let mut key_to_point = BTreeMap::<PointKey, Vec3>::new();
    let mut segment_keys = Vec::new();
    let mut adjacency = BTreeMap::<PointKey, Vec<usize>>::new();
    for segment in profile {
        let [start, end] = segment.endpoints;
        if start.distance(end) <= tolerance {
            continue;
        }
        let start_key = point_key(start, origin, key_step)?;
        let end_key = point_key(end, origin, key_step)?;
        key_to_point.entry(start_key).or_insert(start);
        key_to_point.entry(end_key).or_insert(end);
        let segment_index = segment_keys.len();
        segment_keys.push([start_key, end_key]);
        adjacency.entry(start_key).or_default().push(segment_index);
        adjacency.entry(end_key).or_default().push(segment_index);
    }
    if segment_keys.is_empty() {
        return Err("Revolve profile edges are degenerate.");
    }

    let mut seeds = (0..segment_keys.len()).collect::<Vec<_>>();
    seeds.sort_by_key(|index| {
        let [start, end] = segment_keys[*index];
        (adjacency[&start].len().min(adjacency[&end].len()), *index)
    });
    let mut visited = BTreeSet::new();
    let mut chains = Vec::new();
    for seed in seeds {
        if visited.contains(&seed) {
            continue;
        }
        let [start, end] = segment_keys[seed];
        let start_key = if adjacency[&start].len() <= adjacency[&end].len() {
            start
        } else {
            end
        };
        let mut current = start_key;
        let mut points = vec![key_to_point[&start_key]];
        let mut closed = false;
        while let Some(next_segment) = adjacency[&current]
            .iter()
            .copied()
            .find(|segment| !visited.contains(segment))
        {
            visited.insert(next_segment);
            let [segment_start, segment_end] = segment_keys[next_segment];
            current = if segment_start == current {
                segment_end
            } else {
                segment_start
            };
            points.push(key_to_point[&current]);
            if current == start_key {
                closed = true;
                break;
            }
        }
        if closed {
            points.pop();
            let area = projected_profile_area(&points, origin, axis);
            // The selected profile may be drawn in either direction. Match its winding to the
            // sweep sign so a negative full turn still produces outward-facing solid normals.
            if area * angle_sign < 0.0 {
                points.reverse();
            }
        }
        if points.len() >= 2 {
            chains.push(ProfileChain { points, closed });
        }
    }
    if chains.is_empty() {
        return Err("Revolve could not order the selected profile edges.");
    }
    Ok(chains)
}

fn projected_profile_area(points: &[Vec3], origin: Vec3, axis: Vec3) -> f64 {
    let coordinates = points
        .iter()
        .map(|point| {
            let relative = *point - origin;
            let height = relative.dot(axis);
            let radius = (relative - axis * height).length();
            [radius, height]
        })
        .collect::<Vec<_>>();
    (0..coordinates.len())
        .map(|index| {
            let [x0, y0] = coordinates[index];
            let [x1, y1] = coordinates[(index + 1) % coordinates.len()];
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
        * 0.5
}

fn point_key(point: Vec3, origin: Vec3, step: f64) -> Result<PointKey, &'static str> {
    [point.x - origin.x, point.y - origin.y, point.z - origin.z]
        .map(|coordinate| {
            let key = (coordinate / step).round_ties_even();
            (key.is_finite() && key.abs() < i64::MAX as f64)
                .then_some(key as i64)
                .ok_or("Revolve profile exceeds the supported coordinate range.")
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "Revolve profile could not be indexed safely.")
}

fn rotate_point(point: Vec3, origin: Vec3, axis: Vec3, angle: f64) -> Vec3 {
    let relative = point - origin;
    relative * angle.cos()
        + axis.cross(relative) * angle.sin()
        + axis * (axis.dot(relative) * (1.0 - angle.cos()))
        + origin
}

fn intern_vertex(
    point: Vec3,
    origin: Vec3,
    tolerance: f64,
    mesh: &mut SweepMesh,
    indices: &mut BTreeMap<PointKey, usize>,
) -> Result<usize, &'static str> {
    let key = point_key(point, origin, (tolerance * 0.5).max(1.0e-12))?;
    if let Some(index) = indices.get(&key) {
        if mesh.vertices[*index].distance(point) > tolerance {
            return Err("Revolve output exceeds the supported point tolerance.");
        }
        return Ok(*index);
    }
    let index = mesh.vertices.len();
    mesh.vertices.push(point);
    indices.insert(key, index);
    Ok(index)
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

fn ordered_edge(a: usize, b: usize) -> EdgeKey {
    if a < b { [a, b] } else { [b, a] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const TOLERANCE: f64 = 1.0e-8;

    fn segment(start: Vec3, end: Vec3) -> SweepProfileSegment {
        SweepProfileSegment {
            endpoints: [start, end],
        }
    }

    fn closed_axis_profile() -> [SweepProfileSegment; 4] {
        [
            segment(Vec3::X, Vec3::X + Vec3::Y),
            segment(Vec3::ZERO, Vec3::X),
            segment(Vec3::Y, Vec3::ZERO),
            segment(Vec3::X + Vec3::Y, Vec3::Y),
        ]
    }

    fn edge_uses(mesh: &SweepMesh) -> BTreeMap<[usize; 2], usize> {
        let mut uses = BTreeMap::new();
        for triangle in &mesh.triangles {
            for [a, b] in [
                [triangle.vertices[0], triangle.vertices[1]],
                [triangle.vertices[1], triangle.vertices[2]],
                [triangle.vertices[2], triangle.vertices[0]],
            ] {
                *uses.entry(ordered_edge(a, b)).or_insert(0) += 1;
            }
        }
        uses
    }

    fn signed_volume(mesh: &SweepMesh) -> f64 {
        mesh.triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.vertices.map(|index| mesh.vertices[index]);
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    #[test]
    fn full_revolution_welds_the_seam_and_builds_an_outward_closed_solid() {
        let resolution = 24.0;
        let expected_volume = resolution * (std::f64::consts::TAU / resolution).sin() * 0.5;
        for angle in [std::f64::consts::TAU, -std::f64::consts::TAU] {
            let mesh = revolve(
                &closed_axis_profile(),
                Vec3::ZERO,
                Vec3::Y,
                angle,
                resolution as usize,
                TOLERANCE,
            )
            .unwrap();
            assert_eq!(50, mesh.vertices.len());
            assert_eq!(96, mesh.triangles.len());
            assert!(edge_uses(&mesh).values().all(|uses| *uses == 2));
            assert!(
                (signed_volume(&mesh) - expected_volume).abs() < 1.0e-8,
                "volume was {}",
                signed_volume(&mesh)
            );
        }
    }

    #[test]
    fn full_revolution_welds_a_non_axis_profile_seam() {
        let profile = [segment(Vec3::X, Vec3::X + Vec3::Y)];
        let mesh = revolve(
            &profile,
            Vec3::ZERO,
            Vec3::Y,
            std::f64::consts::TAU,
            12,
            TOLERANCE,
        )
        .unwrap();
        assert_eq!(24, mesh.vertices.len());
        assert_eq!(24, mesh.triangles.len());
        assert!(edge_uses(&mesh).values().filter(|uses| **uses == 1).count() == 24);
        assert!(edge_uses(&mesh).values().all(|uses| *uses <= 2));
    }

    #[test]
    fn typed_partial_and_negative_angles_keep_distinct_end_rings() {
        for angle in [std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
            let mesh = revolve(
                &[segment(Vec3::X, Vec3::X + Vec3::Y)],
                Vec3::ZERO,
                Vec3::Y,
                angle,
                8,
                TOLERANCE,
            )
            .unwrap();
            assert_eq!(18, mesh.vertices.len());
            assert_eq!(16, mesh.triangles.len());
            assert!(mesh.vertices.iter().any(|point| point.z.abs() > 0.5));
        }
    }

    #[test]
    fn revolve_rejects_invalid_axis_angle_profile_and_resolution() {
        let profile = [segment(Vec3::X, Vec3::X + Vec3::Y)];
        assert!(revolve(&profile, Vec3::ZERO, Vec3::ZERO, 1.0, 12, TOLERANCE).is_err());
        assert!(revolve(&profile, Vec3::ZERO, Vec3::Y, 0.0, 12, TOLERANCE).is_err());
        assert!(
            revolve(
                &profile,
                Vec3::ZERO,
                Vec3::Y,
                std::f64::consts::TAU + 0.1,
                12,
                TOLERANCE,
            )
            .is_err()
        );
        assert!(revolve(&profile, Vec3::ZERO, Vec3::Y, 1.0, 2, TOLERANCE).is_err());
        assert!(revolve(&[], Vec3::ZERO, Vec3::Y, 1.0, 12, TOLERANCE).is_err());
    }
}
