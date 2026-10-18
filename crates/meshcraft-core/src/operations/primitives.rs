//! Closed indexed mesh generators for the legacy solid primitive tools.

use std::f64::consts::{PI, TAU};

use crate::geometry::Vec3;
use crate::tools::primitives::PrimitiveKind;
use crate::tools::shapes::PlanarMesh;

const PRIMITIVE_EPSILON: f64 = 1.0e-3;

/// Builds a wireframe and consistently wound, closed triangle mesh from legacy picks.
/// Sphere picks are center/radius; axis primitives are base center/radius point/height center.
pub fn build_primitive(
    kind: PrimitiveKind,
    picks: &[Vec3],
    circle_segments: u32,
) -> Result<PlanarMesh, &'static str> {
    if picks.len() != kind.pick_count() {
        return Err("Primitive has the wrong number of pick points.");
    }
    if picks.iter().any(|point| !point.is_finite()) {
        return Err("Primitive points must be finite.");
    }
    let segments = circle_segments.clamp(3, 256) as usize;
    match kind {
        PrimitiveKind::Sphere => sphere(picks[0], picks[1], segments),
        PrimitiveKind::Cylinder | PrimitiveKind::Cone | PrimitiveKind::Pill => {
            axis_primitive(kind, picks[0], picks[1], picks[2], segments)
        }
    }
}

fn sphere(center: Vec3, radius_point: Vec3, segments: usize) -> Result<PlanarMesh, &'static str> {
    let radius = center.distance(radius_point);
    if !radius.is_finite() || radius <= PRIMITIVE_EPSILON {
        return Err("Sphere canceled: radius is too small.");
    }
    let stacks = (segments / 2).max(2);
    let axis = Vec3::Y;
    let (axis_u, axis_v) = basis_from_axis(axis, Vec3::X)?;
    let mut mesh = PlanarMesh {
        vertices: vec![center - axis * radius],
        segments: Vec::new(),
        triangles: Vec::new(),
    };
    let mut rings = Vec::with_capacity(stacks - 1);
    for stack in 1..stacks {
        let phi = -PI * 0.5 + PI * stack as f64 / stacks as f64;
        let ring_radius = phi.cos() * radius;
        let ring_center = center + axis * (phi.sin() * radius);
        rings.push(push_ring(
            &mut mesh.vertices,
            ring_center,
            axis_u,
            axis_v,
            ring_radius,
            segments,
        ));
    }
    let north = mesh.vertices.len();
    mesh.vertices.push(center + axis * radius);

    add_ring_edges(&mut mesh, &rings);
    add_meridian_edges(&mut mesh, 0, &rings, north);
    let first = &rings[0];
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles.push([0, first[next], first[index]]);
    }
    for pair in rings.windows(2) {
        add_ring_faces(&mut mesh, &pair[0], &pair[1], segments);
    }
    let last = rings.last().expect("sphere always has an interior ring");
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles.push([north, last[index], last[next]]);
    }
    validate_generated_mesh(mesh)
}

fn axis_primitive(
    kind: PrimitiveKind,
    base: Vec3,
    radius_point: Vec3,
    height: Vec3,
    segments: usize,
) -> Result<PlanarMesh, &'static str> {
    let axis = height - base;
    let axis_length = axis.length();
    if !axis_length.is_finite() || axis_length <= PRIMITIVE_EPSILON {
        return Err("Primitive canceled: height is too short.");
    }
    let axis_unit = axis / axis_length;
    let radial = radius_point - base;
    let radial = radial - axis_unit * radial.dot(axis_unit);
    let radius = radial.length();
    if !radius.is_finite() || radius <= PRIMITIVE_EPSILON {
        return Err("Primitive canceled: radius is too small or lies on the height axis.");
    }
    let (axis_u, axis_v) = basis_from_axis(axis_unit, radial / radius)?;
    match kind {
        PrimitiveKind::Cylinder => cylinder(base, height, axis_u, axis_v, radius, segments),
        PrimitiveKind::Cone => cone(base, height, axis_u, axis_v, radius, segments),
        PrimitiveKind::Pill => pill(base, height, axis_u, axis_v, axis_unit, radius, segments),
        PrimitiveKind::Sphere => Err("Sphere does not use an axis."),
    }
}

fn cylinder(
    base: Vec3,
    height: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    radius: f64,
    segments: usize,
) -> Result<PlanarMesh, &'static str> {
    let mut mesh = PlanarMesh {
        vertices: vec![base, height],
        // The cap centers occupy indices zero and one.
        segments: Vec::with_capacity(3 * segments),
        triangles: Vec::with_capacity(4 * segments),
    };
    let bottom = push_ring(&mut mesh.vertices, base, axis_u, axis_v, radius, segments);
    let top = push_ring(&mut mesh.vertices, height, axis_u, axis_v, radius, segments);
    add_ring_edges(&mut mesh, &[bottom.clone(), top.clone()]);
    for index in 0..segments {
        mesh.segments.push([bottom[index], top[index]]);
    }
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles
            .push([bottom[index], bottom[next], top[next]]);
        mesh.triangles.push([bottom[index], top[next], top[index]]);
        mesh.triangles.push([0, bottom[next], bottom[index]]);
        mesh.triangles.push([1, top[index], top[next]]);
    }
    validate_generated_mesh(mesh)
}

fn cone(
    base: Vec3,
    apex: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    radius: f64,
    segments: usize,
) -> Result<PlanarMesh, &'static str> {
    let mut mesh = PlanarMesh {
        vertices: vec![base],
        segments: Vec::with_capacity(2 * segments),
        triangles: Vec::with_capacity(2 * segments),
    };
    let ring = push_ring(&mut mesh.vertices, base, axis_u, axis_v, radius, segments);
    let apex_index = mesh.vertices.len();
    mesh.vertices.push(apex);
    add_ring_edges(&mut mesh, std::slice::from_ref(&ring));
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.segments.push([ring[index], apex_index]);
        mesh.triangles.push([ring[index], ring[next], apex_index]);
        mesh.triangles.push([0, ring[next], ring[index]]);
    }
    validate_generated_mesh(mesh)
}

fn pill(
    base: Vec3,
    height: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    axis: Vec3,
    radius: f64,
    segments: usize,
) -> Result<PlanarMesh, &'static str> {
    let half_stacks = (segments / 4).max(2);
    let mut mesh = PlanarMesh {
        vertices: vec![base - axis * radius],
        segments: Vec::new(),
        triangles: Vec::new(),
    };
    let mut rings = Vec::with_capacity(half_stacks * 2);
    for stack in 1..=half_stacks {
        let angle = -PI * 0.5 + (PI * 0.5) * stack as f64 / half_stacks as f64;
        rings.push(push_ring(
            &mut mesh.vertices,
            base + axis * (angle.sin() * radius),
            axis_u,
            axis_v,
            angle.cos() * radius,
            segments,
        ));
    }
    rings.push(push_ring(
        &mut mesh.vertices,
        height,
        axis_u,
        axis_v,
        radius,
        segments,
    ));
    for stack in 1..half_stacks {
        let angle = (PI * 0.5) * stack as f64 / half_stacks as f64;
        rings.push(push_ring(
            &mut mesh.vertices,
            height + axis * (angle.sin() * radius),
            axis_u,
            axis_v,
            angle.cos() * radius,
            segments,
        ));
    }
    let north = mesh.vertices.len();
    mesh.vertices.push(height + axis * radius);

    add_ring_edges(&mut mesh, &rings);
    add_meridian_edges(&mut mesh, 0, &rings, north);
    let first = &rings[0];
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles.push([0, first[next], first[index]]);
    }
    for pair in rings.windows(2) {
        add_ring_faces(&mut mesh, &pair[0], &pair[1], segments);
    }
    let last = rings.last().expect("pill has a lower hemisphere ring");
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles.push([north, last[index], last[next]]);
    }
    validate_generated_mesh(mesh)
}

fn basis_from_axis(axis: Vec3, preferred_u: Vec3) -> Result<(Vec3, Vec3), &'static str> {
    let axis = axis
        .try_normalized(PRIMITIVE_EPSILON)
        .ok_or("Primitive axis is degenerate.")?;
    let projected = preferred_u - axis * preferred_u.dot(axis);
    let axis_u = projected
        .try_normalized(PRIMITIVE_EPSILON)
        .or_else(|| {
            let reference = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
            (reference.cross(axis)).try_normalized(PRIMITIVE_EPSILON)
        })
        .ok_or("Primitive orientation is degenerate.")?;
    let axis_v = axis
        .cross(axis_u)
        .try_normalized(PRIMITIVE_EPSILON)
        .ok_or("Primitive orientation is degenerate.")?;
    Ok((axis_u, axis_v))
}

fn push_ring(
    vertices: &mut Vec<Vec3>,
    center: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    radius: f64,
    segments: usize,
) -> Vec<usize> {
    let start = vertices.len();
    for index in 0..segments {
        let angle = TAU * index as f64 / segments as f64;
        vertices.push(center + axis_u * (angle.cos() * radius) + axis_v * (angle.sin() * radius));
    }
    (start..start + segments).collect()
}

fn add_ring_edges(mesh: &mut PlanarMesh, rings: &[Vec<usize>]) {
    let segments = rings.first().map_or(0, Vec::len);
    for ring in rings {
        for index in 0..segments {
            mesh.segments
                .push([ring[index], ring[(index + 1) % segments]]);
        }
    }
}

fn add_meridian_edges(mesh: &mut PlanarMesh, south: usize, rings: &[Vec<usize>], north: usize) {
    if let Some(first) = rings.first() {
        for &vertex in first {
            mesh.segments.push([south, vertex]);
        }
    }
    for pair in rings.windows(2) {
        for (&lower, &upper) in pair[0].iter().zip(&pair[1]) {
            mesh.segments.push([lower, upper]);
        }
    }
    if let Some(last) = rings.last() {
        for &vertex in last {
            mesh.segments.push([vertex, north]);
        }
    }
}

fn add_ring_faces(mesh: &mut PlanarMesh, lower: &[usize], upper: &[usize], segments: usize) {
    for index in 0..segments {
        let next = (index + 1) % segments;
        mesh.triangles
            .push([lower[index], lower[next], upper[next]]);
        mesh.triangles
            .push([lower[index], upper[next], upper[index]]);
    }
}

fn validate_generated_mesh(mesh: PlanarMesh) -> Result<PlanarMesh, &'static str> {
    if mesh.vertices.iter().any(|vertex| !vertex.is_finite())
        || mesh.triangles.iter().any(|face| {
            let [a, b, c] = face.map(|index| mesh.vertices[index]);
            (b - a).cross(c - a).length() <= PRIMITIVE_EPSILON * PRIMITIVE_EPSILON
        })
    {
        return Err("Primitive dimensions exceed the supported geometry range.");
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn closed_oriented_mesh(mesh: &PlanarMesh) {
        let mut edges = BTreeMap::<(usize, usize), (usize, isize)>::new();
        let mut volume = 0.0;
        for [a, b, c] in &mesh.triangles {
            volume += mesh.vertices[*a].dot(mesh.vertices[*b].cross(mesh.vertices[*c])) / 6.0;
            for (start, end) in [(*a, *b), (*b, *c), (*c, *a)] {
                let (key, direction) = if start < end {
                    ((start, end), 1)
                } else {
                    ((end, start), -1)
                };
                let edge = edges.entry(key).or_default();
                edge.0 += 1;
                edge.1 += direction;
            }
        }
        assert!(
            edges
                .values()
                .all(|(count, direction)| *count == 2 && *direction == 0)
        );
        assert!(
            volume > 0.0,
            "outward winding should enclose positive volume"
        );
        assert!(mesh.segments.iter().all(|[a, b]| a != b));
    }

    #[test]
    fn all_primitives_produce_closed_exportable_manifolds() {
        let picks = [
            Vec3::ZERO,
            Vec3::new(2.0, 0.25, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        ];
        for kind in [
            PrimitiveKind::Sphere,
            PrimitiveKind::Cylinder,
            PrimitiveKind::Cone,
            PrimitiveKind::Pill,
        ] {
            let mesh = build_primitive(kind, &picks[..kind.pick_count()], 12).unwrap();
            assert!(!mesh.vertices.is_empty());
            assert!(!mesh.segments.is_empty());
            assert!(!mesh.triangles.is_empty());
            closed_oriented_mesh(&mesh);
        }
    }

    #[test]
    fn primitive_resolution_is_clamped_to_supported_bounds() {
        let picks = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let low = build_primitive(PrimitiveKind::Cylinder, &picks, 1).unwrap();
        let high = build_primitive(PrimitiveKind::Cylinder, &picks, 1000).unwrap();
        assert_eq!(3, low.triangles.len() / 4);
        assert_eq!(256 * 4, high.triangles.len());
    }

    #[test]
    fn invalid_dimensions_and_pick_counts_are_rejected() {
        assert!(build_primitive(PrimitiveKind::Sphere, &[Vec3::ZERO, Vec3::ZERO], 12).is_err());
        assert!(
            build_primitive(
                PrimitiveKind::Cylinder,
                &[Vec3::ZERO, Vec3::X, Vec3::ZERO],
                12
            )
            .is_err()
        );
        assert!(
            build_primitive(
                PrimitiveKind::Cylinder,
                &[Vec3::ZERO, Vec3::Y, Vec3::Y * 2.0],
                12
            )
            .is_err()
        );
        assert!(build_primitive(PrimitiveKind::Cone, &[Vec3::ZERO, Vec3::X], 12).is_err());
    }

    #[test]
    fn negative_axis_height_is_preserved_with_outward_faces() {
        let mesh = build_primitive(
            PrimitiveKind::Cylinder,
            &[Vec3::ZERO, Vec3::new(2.0, -1.0, 0.0), Vec3::Y * -3.0],
            12,
        )
        .unwrap();
        closed_oriented_mesh(&mesh);
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
        assert!((min_y + 3.0).abs() < 1.0e-9);
        assert!(max_y.abs() < 1.0e-9);
        assert!(
            mesh.vertices
                .iter()
                .any(|point| point.distance(Vec3::new(2.0, 0.0, 0.0)) < 1.0e-9)
        );
    }

    #[test]
    fn pill_height_point_is_the_top_hemisphere_center() {
        let mesh = build_primitive(
            PrimitiveKind::Pill,
            &[Vec3::ZERO, Vec3::X, Vec3::Y * 3.0],
            12,
        )
        .unwrap();
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
        assert!((min_y + 1.0).abs() < 1.0e-9);
        assert!((max_y - 4.0).abs() < 1.0e-9);
    }
}
