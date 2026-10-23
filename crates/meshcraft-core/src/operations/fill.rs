//! Planar line arrangements: weld/split a selection, extract bounded loops, then fill them.
use crate::geometry::polygon::{PolygonSet, PolygonShape};
use crate::geometry::{Vec2, Vec3};
use crate::tools::shapes::PlanarMesh;
use std::collections::{BTreeMap, BTreeSet};
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn inside(ring: &[Vec2], p: Vec2) -> bool {
    let mut yes = false;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            yes = !yes;
        }
    }
    yes
}

pub fn fill_segments(segments: &[[Vec3; 2]], epsilon: f64) -> Result<PlanarMesh, String> {
    if segments.len() < 3
        || !epsilon.is_finite()
        || epsilon <= 0.
        || segments.iter().flatten().any(|p| !p.is_finite())
    {
        return Err("Select finite planar lines forming closed loops.".into());
    }
    let origin = segments[0][0];
    let u = (segments[0][1] - origin)
        .try_normalized(epsilon)
        .ok_or("A selected line has zero length.")?;
    let n = segments
        .iter()
        .flatten()
        .filter_map(|p| u.cross(*p - origin).try_normalized(epsilon))
        .next()
        .ok_or("Selected lines are collinear.")?;
    let v = n.cross(u);
    if segments
        .iter()
        .flatten()
        .any(|p| (*p - origin).dot(n).abs() > epsilon)
    {
        return Err(
            "Fill Loops requires coplanar lines. Fill separate planes individually.".into(),
        );
    }
    let projected: Vec<[Vec2; 2]> = segments
        .iter()
        .map(|s| {
            s.map(|p| {
                let d = p - origin;
                Vec2::new(d.dot(u), d.dot(v))
            })
        })
        .collect();
    let mut cuts = vec![vec![0., 1.]; segments.len()];
    // Split crossings, T junctions and collinear overlaps before walking the graph.
    for i in 0..projected.len() {
        for j in i + 1..projected.len() {
            let [a, b] = projected[i];
            let [c, d] = projected[j];
            let r = b - a;
            let s = d - c;
            let den = cross(r, s);
            let rr = r.dot(r);
            let ss = s.dot(s);
            if rr <= epsilon * epsilon || ss <= epsilon * epsilon {
                continue;
            }
            if den.abs() > epsilon * rr.sqrt().max(ss.sqrt()) {
                let t = cross(c - a, s) / den;
                let q = cross(c - a, r) / den;
                if (-epsilon / rr.sqrt()..=1. + epsilon / rr.sqrt()).contains(&t)
                    && (-epsilon / ss.sqrt()..=1. + epsilon / ss.sqrt()).contains(&q)
                {
                    cuts[i].push(t.clamp(0., 1.));
                    cuts[j].push(q.clamp(0., 1.));
                }
            } else if cross(c - a, r).abs() <= epsilon * rr.sqrt() {
                for p in [c, d] {
                    let t = (p - a).dot(r) / rr;
                    if (0.0..=1.0).contains(&t) {
                        cuts[i].push(t);
                    }
                }
                for p in [a, b] {
                    let t = (p - c).dot(s) / ss;
                    if (0.0..=1.0).contains(&t) {
                        cuts[j].push(t);
                    }
                }
            }
        }
    }
    let mut points: Vec<Vec2> = vec![];
    let mut buckets = BTreeMap::<(i64, i64), Vec<usize>>::new();
    let mut edges = BTreeSet::new();
    let mut directions = BTreeMap::<(usize, usize), f64>::new();
    let mut vertex = |p: Vec2| -> Result<usize, String> {
        let x = (p.x / epsilon).floor();
        let y = (p.y / epsilon).floor();
        if x.abs() > 1e15 || y.abs() > 1e15 {
            return Err("Selection coordinates exceed the fill tolerance range.".into());
        }
        let key = (x as i64, y as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(ids) = buckets.get(&(key.0 + dx, key.1 + dy)) {
                    for &id in ids {
                        if (points[id] - p).length() <= epsilon {
                            return Ok(id);
                        }
                    }
                }
            }
        }
        let id = points.len();
        points.push(p);
        buckets.entry(key).or_default().push(id);
        Ok(id)
    };
    for (i, ts) in cuts.iter_mut().enumerate() {
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        let [a, b] = projected[i];
        for pair in ts.windows(2) {
            let p = a + (b - a) * pair[0];
            let q = a + (b - a) * pair[1];
            let ia = vertex(p)?;
            let ib = vertex(q)?;
            if ia != ib {
                edges.insert((ia.min(ib), ia.max(ib)));
                *directions.entry((ia, ib)).or_default() += 1.;
            }
        }
    }
    let mut adjacency = vec![vec![]; points.len()];
    for &(a, b) in &edges {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }
    for (i, neighbors) in adjacency.iter_mut().enumerate() {
        neighbors.sort_by(|&a, &b| {
            let x = points[a] - points[i];
            let y = points[b] - points[i];
            x.y.atan2(x.x).total_cmp(&y.y.atan2(y.x))
        });
    }
    let mut visited = BTreeSet::new();
    let mut rings = vec![];
    let mut orientations = vec![];
    for &(a, b) in &edges {
        for start in [(a, b), (b, a)] {
            if visited.contains(&start) {
                continue;
            }
            let mut current = start;
            let mut ring = vec![];
            let mut vote = 0.;
            for _ in 0..edges.len() * 2 + 1 {
                if !visited.insert(current) {
                    break;
                }
                vote += (directions.get(&current).copied().unwrap_or(0.)
                    - directions
                        .get(&(current.1, current.0))
                        .copied()
                        .unwrap_or(0.))
                    * (points[current.1] - points[current.0]).length();
                ring.push(points[current.0]);
                let neighbors = &adjacency[current.1];
                let back = neighbors.iter().position(|i| *i == current.0).unwrap();
                current = (
                    current.1,
                    neighbors[(back + neighbors.len() - 1) % neighbors.len()],
                );
                if current == start {
                    break;
                }
            }
            let area: f64 = (0..ring.len())
                .map(|i| cross(ring[i], ring[(i + 1) % ring.len()]))
                .sum();
            if current == start && area > epsilon * epsilon {
                orientations.push((ring.clone(), area, vote));
                rings.push(ring);
            }
        }
    }
    if rings.is_empty() {
        return Err("Selected lines contain no closed planar loops.".into());
    }
    let triangulated = PolygonSet {
        shapes: vec![PolygonShape { contours: rings }],
    }
    .triangulate()
    .map_err(|e| e.message.to_owned())?;
    let vertices = triangulated
        .points
        .iter()
        .map(|p| origin + u * p.x + v * p.y)
        .collect();
    let triangles = triangulated
        .triangles
        .into_iter()
        .map(|mut t| {
            let p = (triangulated.points[t[0] as usize]
                + triangulated.points[t[1] as usize]
                + triangulated.points[t[2] as usize])
                / 3.;
            let vote = orientations
                .iter()
                .filter(|(ring, _, _)| inside(ring, p))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map_or(1., |o| o.2);
            if vote < 0. {
                t.swap(1, 2);
            }
            t.map(|i| i as usize)
        })
        .collect();
    Ok(PlanarMesh {
        vertices,
        segments: vec![],
        triangles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn square(a: f64, b: f64) -> Vec<[Vec3; 2]> {
        let p = [
            Vec3::new(a, a, 0.),
            Vec3::new(b, a, 0.),
            Vec3::new(b, b, 0.),
            Vec3::new(a, b, 0.),
        ];
        (0..4).map(|i| [p[i], p[(i + 1) % 4]]).collect()
    }
    #[test]
    fn directions_holes_crossings_and_open_lines() {
        let mut lines = square(0., 4.);
        lines.extend(square(1., 3.));
        lines.rotate_left(3);
        let mesh = fill_segments(&lines, 1e-3).unwrap();
        let area: f64 = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i]);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum();
        assert!((area - 12.).abs() < 1e-8);
        let source = square(0., 2.);
        let a = fill_segments(&source, 1e-3).unwrap();
        let reversed: Vec<_> = source.iter().map(|s| [s[1], s[0]]).collect();
        let b = fill_segments(&reversed, 1e-3).unwrap();
        let normal = |m: &PlanarMesh| {
            let [a, b, c] = m.triangles[0].map(|i| m.vertices[i]);
            (b - a).cross(c - a)
        };
        assert!(normal(&a).dot(normal(&b)) < 0.);
        assert!(fill_segments(&source[..3], 1e-3).is_err());
        let mut split = square(0., 2.);
        split.extend([
            [Vec3::new(0., 1., 0.), Vec3::new(2., 1., 0.)],
            [Vec3::new(1., 0., 0.), Vec3::new(1., 2., 0.)],
        ]);
        let m = fill_segments(&split, 1e-3).unwrap();
        assert!(!m.triangles.is_empty());
    }
}
