//! Selection-based region extrusion mesh generation.

use crate::document::Rgba8;
use crate::geometry::Vec3;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrudeSourceTriangle {
    pub vertices: [Vec3; 3],
    pub color: Rgba8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrudedSegment {
    pub endpoints: [usize; 2],
    pub color: Rgba8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrudedTriangle {
    pub vertices: [usize; 3],
    pub color: Rgba8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtrudedRegionMesh {
    pub vertices: Vec<Vec3>,
    /// New top boundary and vertical edges. Existing base edges are left in place.
    pub segments: Vec<ExtrudedSegment>,
    /// Translated top faces and two side faces for each selected-region boundary edge.
    pub triangles: Vec<ExtrudedTriangle>,
    pub boundary_edges: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PointKey([i64; 3]);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EdgeKey(PointKey, PointKey);

#[derive(Clone, Copy, Debug)]
struct BoundaryEdge {
    from: PointKey,
    to: PointKey,
    color: Rgba8,
    normal: Vec3,
}

/// Builds the added top and side surfaces for a selected face region.
///
/// Edges shared by two selected triangles are interior and receive no side wall. Edges used
/// more than twice are rejected as non-manifold. Holes remain open because only source boundary
/// edges receive side faces; no cap is synthesized across a boundary loop.
pub fn extrude_region(
    selected: &[ExtrudeSourceTriangle],
    displacement: Vec3,
    merge_tolerance: f64,
) -> Result<ExtrudedRegionMesh, &'static str> {
    if selected.is_empty() {
        return Err("Push/Pull requires selected faces.");
    }
    if !displacement.is_finite() {
        return Err("Push/Pull displacement must be finite.");
    }
    if !merge_tolerance.is_finite() || merge_tolerance <= 0.0 {
        return Err("Push/Pull tolerance must be positive and finite.");
    }
    if displacement.length() <= merge_tolerance {
        return Err("Push/Pull distance is too small.");
    }

    let mut points = BTreeMap::<PointKey, Vec3>::new();
    let mut boundary_candidates = BTreeMap::<EdgeKey, Vec<BoundaryEdge>>::new();
    let mut seen_faces = BTreeSet::new();
    let mut keyed_faces = Vec::with_capacity(selected.len());

    for face in selected {
        if face.vertices.iter().any(|point| !point.is_finite()) {
            return Err("Push/Pull face contains a non-finite point.");
        }
        let keys = face
            .vertices
            .map(|point| point_key(point, merge_tolerance))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let [a, b, c] = [keys[0], keys[1], keys[2]];
        let normal =
            (face.vertices[1] - face.vertices[0]).cross(face.vertices[2] - face.vertices[0]);
        if a == b || b == c || c == a || normal.length() <= merge_tolerance * merge_tolerance {
            return Err("Push/Pull selection contains a degenerate face.");
        }
        let mut face_key = keys.clone();
        face_key.sort_unstable();
        if !seen_faces.insert(face_key) {
            return Err("Push/Pull selection contains a duplicate face.");
        }
        for (key, point) in keys.iter().copied().zip(face.vertices) {
            points.entry(key).or_insert(point);
        }
        for [from, to] in [[a, b], [b, c], [c, a]] {
            let edge_key = if from <= to {
                EdgeKey(from, to)
            } else {
                EdgeKey(to, from)
            };
            let uses = boundary_candidates.entry(edge_key).or_default();
            uses.push(BoundaryEdge {
                from,
                to,
                color: face.color,
                normal,
            });
            if uses.len() > 2 {
                return Err("Push/Pull selection contains a non-manifold edge.");
            }
        }
        keyed_faces.push((keys, face.color, normal.dot(displacement) < 0.0));
    }

    let mut mesh = ExtrudedRegionMesh::default();
    let mut indexed_points = BTreeMap::<(bool, PointKey), usize>::new();
    let intern = |is_top: bool,
                  key: PointKey,
                  mesh: &mut ExtrudedRegionMesh,
                  indexed_points: &mut BTreeMap<(bool, PointKey), usize>| {
        if let Some(index) = indexed_points.get(&(is_top, key)) {
            return *index;
        }
        let point = points[&key] + if is_top { displacement } else { Vec3::ZERO };
        let index = mesh.vertices.len();
        mesh.vertices.push(point);
        indexed_points.insert((is_top, key), index);
        index
    };

    for (keys, color, reverse_top) in keyed_faces {
        let [a, b, c] = [keys[0], keys[1], keys[2]];
        let mut top = [a, b, c].map(|key| intern(true, key, &mut mesh, &mut indexed_points));
        if reverse_top {
            top.swap(1, 2);
        }
        mesh.triangles.push(ExtrudedTriangle {
            vertices: top,
            color,
        });
    }

    let mut verticals = BTreeSet::new();
    for uses in boundary_candidates.into_values() {
        let Some(edge) = (uses.len() == 1).then(|| uses[0]) else {
            continue;
        };
        let base_a = intern(false, edge.from, &mut mesh, &mut indexed_points);
        let base_b = intern(false, edge.to, &mut mesh, &mut indexed_points);
        let top_a = intern(true, edge.from, &mut mesh, &mut indexed_points);
        let top_b = intern(true, edge.to, &mut mesh, &mut indexed_points);
        mesh.segments.push(ExtrudedSegment {
            endpoints: [top_a, top_b],
            color: edge.color,
        });
        for key in [edge.from, edge.to] {
            if verticals.insert(key) {
                let base = intern(false, key, &mut mesh, &mut indexed_points);
                let top = intern(true, key, &mut mesh, &mut indexed_points);
                mesh.segments.push(ExtrudedSegment {
                    endpoints: [base, top],
                    color: edge.color,
                });
            }
        }
        let reverse = edge.normal.dot(displacement) < 0.0;
        let sides = if reverse {
            [[base_a, top_b, base_b], [base_a, top_a, top_b]]
        } else {
            [[base_a, base_b, top_b], [base_a, top_b, top_a]]
        };
        for vertices in sides {
            let [a, b, c] = vertices.map(|index| mesh.vertices[index]);
            if (b - a).cross(c - a).length() <= merge_tolerance * merge_tolerance {
                return Err("Push/Pull displacement collapses a boundary wall.");
            }
            mesh.triangles.push(ExtrudedTriangle {
                vertices,
                color: edge.color,
            });
        }
        mesh.boundary_edges += 1;
    }

    Ok(mesh)
}

fn point_key(point: Vec3, tolerance: f64) -> Result<PointKey, &'static str> {
    let mut coordinates = [0_i64; 3];
    for (index, value) in [point.x, point.y, point.z].into_iter().enumerate() {
        let quantized = (value / tolerance).round();
        if !quantized.is_finite() || quantized < i64::MIN as f64 || quantized > i64::MAX as f64 {
            return Err("Push/Pull coordinates exceed the supported range.");
        }
        coordinates[index] = quantized as i64;
    }
    Ok(PointKey(coordinates))
}

#[cfg(test)]
mod tests {
    use super::{ExtrudeSourceTriangle, extrude_region};
    use crate::document::Rgba8;
    use crate::geometry::Vec3;
    use std::collections::BTreeMap;

    const TOLERANCE: f64 = 1.0e-6;

    fn face(vertices: [Vec3; 3], color: Rgba8) -> ExtrudeSourceTriangle {
        ExtrudeSourceTriangle { vertices, color }
    }

    fn square(color: Rgba8) -> [ExtrudeSourceTriangle; 2] {
        [
            face([Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Z], color),
            face([Vec3::ZERO, Vec3::X + Vec3::Z, Vec3::Z], color),
        ]
    }

    #[test]
    fn extrudes_a_selected_patch_without_an_interior_wall_and_keeps_colors() {
        let color = Rgba8::new(20, 80, 140, 200);
        let mesh = extrude_region(&square(color), Vec3::Y * 2.0, TOLERANCE).unwrap();

        assert_eq!(4, mesh.boundary_edges);
        assert_eq!(8, mesh.segments.len());
        assert_eq!(10, mesh.triangles.len());
        assert_eq!(8, mesh.vertices.len());
        assert!(
            mesh.triangles
                .iter()
                .all(|triangle| triangle.color == color)
        );
        assert!(mesh.segments.iter().all(|segment| segment.color == color));
        assert!(!mesh.triangles.iter().any(|triangle| {
            let points = triangle.vertices.map(|index| mesh.vertices[index]);
            points.iter().all(|point| point.y == 0.0)
        }));
    }

    #[test]
    fn square_extrusion_forms_a_closed_prism_with_expected_volume() {
        let source = square(Rgba8::WHITE);
        for displacement in [Vec3::Y * 2.0, -Vec3::Y * 2.0] {
            let mesh = extrude_region(&source, displacement, TOLERANCE).unwrap();
            let mut triangles = source
                .iter()
                .map(|face| {
                    let mut vertices = face.vertices;
                    let [a, b, c] = vertices;
                    if (b - a).cross(c - a).dot(displacement) > 0.0 {
                        vertices.swap(1, 2);
                    }
                    vertices
                })
                .collect::<Vec<_>>();
            triangles.extend(
                mesh.triangles
                    .iter()
                    .map(|triangle| triangle.vertices.map(|index| mesh.vertices[index])),
            );

            let point_key = |point: Vec3| {
                [point.x, point.y, point.z]
                    .map(|coordinate| (coordinate / TOLERANCE).round() as i64)
            };
            let mut edge_uses = BTreeMap::new();
            let mut signed_volume = 0.0;
            for [a, b, c] in triangles {
                signed_volume += a.dot(b.cross(c)) / 6.0;
                for [start, end] in [[a, b], [b, c], [c, a]] {
                    let [start, end] = [point_key(start), point_key(end)];
                    let edge = if start <= end {
                        [start, end]
                    } else {
                        [end, start]
                    };
                    *edge_uses.entry(edge).or_insert(0_usize) += 1;
                }
            }

            assert!(edge_uses.values().all(|uses| *uses == 2));
            assert!(
                (signed_volume.abs() - 2.0).abs() < 1.0e-12,
                "expected prism volume 2, got {signed_volume}"
            );
        }
    }

    #[test]
    fn negative_extrusion_reverses_side_winding_and_preserves_disconnected_regions() {
        let triangles = [
            face([Vec3::ZERO, Vec3::X, Vec3::Z], Rgba8::WHITE),
            face(
                [
                    Vec3::new(4.0, 0.0, 0.0),
                    Vec3::new(5.0, 0.0, 0.0),
                    Vec3::new(4.0, 0.0, 1.0),
                ],
                Rgba8::WHITE,
            ),
        ];
        let mesh = extrude_region(&triangles, -Vec3::Y, TOLERANCE).unwrap();
        assert_eq!(6, mesh.boundary_edges);
        assert_eq!(14, mesh.triangles.len());

        let positive = extrude_region(&triangles, Vec3::Y, TOLERANCE).unwrap();
        let side_normal = |mesh: &super::ExtrudedRegionMesh| {
            let [a, b, c] = mesh.triangles[2].vertices.map(|index| mesh.vertices[index]);
            (b - a).cross(c - a)
        };
        assert!(side_normal(&positive).dot(side_normal(&mesh)) > 0.0);
    }

    #[test]
    fn extrusion_keeps_a_hole_open_by_siding_both_boundary_loops() {
        let outer = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 4.0),
            Vec3::new(0.0, 0.0, 4.0),
        ];
        let inner = [
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(3.0, 0.0, 1.0),
            Vec3::new(3.0, 0.0, 3.0),
            Vec3::new(1.0, 0.0, 3.0),
        ];
        let mut patch = Vec::new();
        for index in 0..4 {
            let next = (index + 1) % 4;
            patch.push(face([outer[index], outer[next], inner[next]], Rgba8::WHITE));
            patch.push(face(
                [outer[index], inner[next], inner[index]],
                Rgba8::WHITE,
            ));
        }

        let mesh = extrude_region(&patch, Vec3::Y, TOLERANCE).unwrap();
        assert_eq!(8, mesh.boundary_edges);
        assert_eq!(24, mesh.triangles.len());
        assert_eq!(16, mesh.segments.len());
        assert!(
            !mesh
                .vertices
                .iter()
                .any(|point| { point.x == 2.0 && point.z == 2.0 })
        );
    }

    #[test]
    fn rejects_empty_zero_non_finite_degenerate_and_non_manifold_regions() {
        let valid = square(Rgba8::WHITE);
        assert_eq!(
            Err("Push/Pull requires selected faces."),
            extrude_region(&[], Vec3::Y, TOLERANCE)
        );
        assert_eq!(
            Err("Push/Pull distance is too small."),
            extrude_region(&valid, Vec3::ZERO, TOLERANCE)
        );
        assert!(extrude_region(&valid, Vec3::new(f64::NAN, 0.0, 0.0), TOLERANCE).is_err());
        assert!(
            extrude_region(
                &[face([Vec3::ZERO, Vec3::X, Vec3::X * 2.0], Rgba8::WHITE)],
                Vec3::Y,
                TOLERANCE
            )
            .is_err()
        );
        let non_manifold = [
            face([Vec3::ZERO, Vec3::X, Vec3::Z], Rgba8::WHITE),
            face([Vec3::X, Vec3::ZERO, -Vec3::Z], Rgba8::WHITE),
            face([Vec3::ZERO, Vec3::X, Vec3::Y], Rgba8::WHITE),
        ];
        assert_eq!(
            Err("Push/Pull selection contains a non-manifold edge."),
            extrude_region(&non_manifold, Vec3::Y, TOLERANCE)
        );
    }
}
