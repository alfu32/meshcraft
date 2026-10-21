//! Shared-axis wall junctions. Parameters stay separate; derived boundary vertices meet.
use super::*;

pub(super) fn wall_mesh(wall: &Wall, neighbors: &[Wall]) -> Result<Mesh, String> {
    let plane = wall.plane()?;
    positive(wall.height, "Wall start height")?;
    positive(wall.end_height(), "Wall end height")?;
    positive(wall.thickness, "Wall thickness")?;
    if !wall.start.is_finite()
        || !wall.end.is_finite()
        || !wall.offset.is_finite()
        || !wall.inclination_deg.is_finite()
        || wall.inclination_deg.abs() >= 85.0
    {
        return Err("Wall geometry, offset or inclination is invalid.".into());
    }
    let length = (wall.end - wall.start).dot(plane.u);
    let rise = (wall.end - wall.start).dot(plane.v);
    let shape = cut(wall.profile()?, &wall.holes)?;
    let mut mesh = Mesh::default();
    // A split at the axis allows a multiwall junction to meet at one common center,
    // rather than leaving a triangular hole between three independently mitered ends.
    for (low, high) in [(-wall.thickness * 0.5, 0.), (0., wall.thickness * 0.5)] {
        prism(
            &mut mesh,
            &shape,
            plane,
            plane.normal * (wall.offset + low),
            plane.normal * (wall.offset + high),
            [
                wall.exterior_color,
                wall.interior_color,
                wall.exterior_color,
            ],
        )?;
    }
    let side = |p: Vec3| (p - plane.origin).dot(plane.normal) - wall.offset;
    mesh.faces
        .retain(|(t, _)| !t.iter().all(|i| side(mesh.points[*i]).abs() < EPS));
    mesh.edges
        .retain(|e| !e.iter().all(|i| side(mesh.points[*i]).abs() < EPS));
    let shear = wall.inclination_deg.to_radians().tan();
    for p in &mut mesh.points {
        let uv = plane.project(*p);
        *p += plane.normal * ((uv.y - rise * uv.x / length) * shear);
    }
    if !wall.auto_join {
        return Ok(mesh);
    }
    for at_start in [true, false] {
        let joint = if at_start { wall.start } else { wall.end };
        let mut refs = vec![(wall, at_start)];
        for other in neighbors {
            if other.id == wall.id || !other.auto_join {
                continue;
            }
            let Ok(p) = other.plane() else {
                continue;
            };
            if p.v.dot(plane.v) < 1. - 1e-6 {
                continue;
            }
            for (point, start) in [(other.start, true), (other.end, false)] {
                if point.distance(joint) <= 1e-5 {
                    refs.push((other, start));
                }
            }
        }
        if refs.len() < 2 {
            continue;
        }
        let direction = |w: &Wall, start: bool| {
            w.plane()
                .map(|p| p.u * if start { 1. } else { -1. })
                .unwrap_or(Vec3::X)
        };
        let reference = direction(wall, at_start);
        refs.sort_by(|(a, sa), (b, sb)| {
            let angle = |d: Vec3| {
                let v = reference.cross(d).dot(plane.v).atan2(reference.dot(d));
                if v < -1e-10 {
                    v + std::f64::consts::TAU
                } else {
                    v
                }
            };
            angle(direction(a, *sa))
                .total_cmp(&angle(direction(b, *sb)))
                .then_with(|| a.id.cmp(&b.id))
        });
        let own = refs
            .iter()
            .position(|(w, s)| w.id == wall.id && *s == at_start)
            .unwrap();
        // Collinear butt joints already meet, including deliberate width/offset steps.
        if refs.len() == 2
            && direction(refs[0].0, refs[0].1)
                .cross(direction(refs[1].0, refs[1].1))
                .length()
                < 1e-6
        {
            continue;
        }
        let boundary = |i: usize, height: f64| -> Vec3 {
            let (a, sa) = refs[i];
            let (b, sb) = refs[(i + 1) % refs.len()];
            let pa = a.plane().unwrap();
            let pb = b.plane().unwrap();
            let da = direction(a, sa);
            let db = direction(b, sb);
            let base_a = if sa { a.start } else { a.end };
            let base_b = if sb { b.start } else { b.end };
            let h_a = height - (base_a - joint).dot(plane.v);
            let h_b = height - (base_b - joint).dot(plane.v);
            let left_a = plane.v.cross(da);
            let right_b = db.cross(plane.v);
            let a0 = base_a
                + plane.v * h_a
                + pa.normal * (a.offset + h_a * a.inclination_deg.to_radians().tan())
                + left_a * (a.thickness * 0.5);
            let b0 = base_b
                + plane.v * h_b
                + pb.normal * (b.offset + h_b * b.inclination_deg.to_radians().tan())
                + right_b * (b.thickness * 0.5);
            let slope = |w: &Wall, p: Plane, start: bool| {
                let delta = w.end - w.start;
                let d = p.u
                    - p.normal
                        * (delta.dot(p.v) / delta.dot(p.u) * w.inclination_deg.to_radians().tan());
                d * if start { 1. } else { -1. }
            };
            let da = slope(a, pa, sa);
            let db = slope(b, pb, sb);
            let cross = da.cross(db).dot(plane.v);
            let hit = if cross.abs() > 1e-8 {
                a0 + da * ((b0 - a0).cross(db).dot(plane.v) / cross)
            } else {
                (a0 + b0) * 0.5
            };
            // Bound acute miters; both owners use the same bevel point.
            let center = joint + plane.v * height;
            let limit = (8. * (a.thickness.max(b.thickness) + a.offset.abs().max(b.offset.abs())))
                .max(1e-4);
            let span_a = (a.end - a.start).dot(pa.u).abs();
            let span_b = (b.end - b.start).dot(pb.u).abs();
            let limit = limit.min(0.45 * span_a.min(span_b));
            let delta = hit - center;
            if delta.length() > limit {
                center + delta * (limit / delta.length())
            } else {
                hit
            }
        };
        let prev = (own + refs.len() - 1) % refs.len();
        for p in &mut mesh.points {
            let uv = plane.project(*p);
            if (uv.x - if at_start { 0. } else { length }).abs() > EPS * 10. {
                continue;
            }
            let height = (*p - joint).dot(plane.v);
            let original_side = ((*p - plane.point(uv)).dot(plane.normal)
                - wall.offset
                - (uv.y - rise * uv.x / length) * shear)
                / (wall.thickness * 0.5);
            let left = boundary(own, height);
            let right = boundary(prev, height);
            let mid = if refs.len() == 2 {
                (left + right) * 0.5
            } else {
                (0..refs.len())
                    .map(|i| boundary(i, height))
                    .fold(Vec3::ZERO, |a, b| a + b)
                    / (refs.len() as f64)
            };
            let left_fraction = original_side * if at_start { -1. } else { 1. };
            *p = if left_fraction >= 0. {
                mid + (left - mid) * left_fraction
            } else {
                mid + (right - mid) * (-left_fraction)
            };
        }
    }
    Ok(mesh)
}
