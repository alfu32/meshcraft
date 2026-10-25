//! Contour-bounded stairs, with sampled walking stations and editable rail guides.
use super::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Stair {
    pub id: String,
    pub name: String,
    pub contour: Vec<Vec3>,
    pub walking_path: Vec<Vec3>,
    pub up: Vec3,
    pub height: f64,
    pub step_count: u32,
    pub support_thickness: f64,
    #[serde(rename = "railLeft", alias = "railLeftEnabled")]
    pub rail_left: bool,
    #[serde(rename = "railRight", alias = "railRightEnabled")]
    pub rail_right: bool,
    pub rail_spacing: f64,
    pub rail_count: u32,
    pub tread_color: Color,
    pub support_color: Color,
    pub materials: BTreeMap<String, String>,
    pub procedural_parameters: BTreeMap<String, BTreeMap<String, Value>>,
}
impl Default for Stair {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Stair".into(),
            contour: vec![],
            walking_path: vec![],
            up: Vec3::Y,
            height: 10.,
            step_count: 14,
            support_thickness: 0.2,
            rail_left: true,
            rail_right: true,
            rail_spacing: 1.,
            rail_count: 5,
            tread_color: Color::default(),
            support_color: Color {
                r: 0.82,
                g: 0.82,
                b: 0.82,
                a: 1.,
            },
            materials: BTreeMap::new(),
            procedural_parameters: BTreeMap::new(),
        }
    }
}
impl Stair {
    pub fn validate_settings(&self) -> Result<(), String> {
        positive(self.height, "Stair height")?;
        positive(self.support_thickness, "Stair support thickness")?;
        positive(self.rail_spacing, "Stair guide spacing")?;
        if self.step_count == 0 || self.step_count > 512 || self.rail_count > 128 {
            return Err("Stairs require 1–512 steps and 0–128 guide levels.".into());
        }
        self.tread_color.validate()?;
        self.support_color.validate()?;
        Ok(())
    }
    pub fn plane(&self) -> Result<Plane, String> {
        surface(&self.contour, self.up).map(|p| p.0)
    }
}
fn length(v: Vec2) -> f64 {
    (v.x * v.x + v.y * v.y).sqrt()
}
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}
fn unit(v: Vec2) -> Result<Vec2, String> {
    let l = length(v);
    if l <= EPS {
        Err("Walking path contains an empty segment or a reversal.".into())
    } else {
        Ok(v * (1. / l))
    }
}
fn key(p: Vec2) -> [i64; 2] {
    [(p.x / EPS).round() as i64, (p.y / EPS).round() as i64]
}
fn sample(path: &[Vec2], lengths: &[f64], mut d: f64) -> Result<(Vec2, Vec2), String> {
    for (i, &l) in lengths.iter().enumerate() {
        if d <= l + EPS || i == lengths.len() - 1 {
            let tangent = unit(path[i + 1] - path[i])?;
            let tangent = if (d - l).abs() <= EPS && i + 1 < lengths.len() {
                unit(tangent + unit(path[i + 2] - path[i + 1])?)?
            } else {
                tangent
            };
            return Ok((
                path[i] + (path[i + 1] - path[i]) * (d / l).clamp(0., 1.),
                tangent,
            ));
        }
        d -= l;
    }
    Err("Walking path is empty.".into())
}
fn station(p: Vec2, ring: &[Vec2]) -> Result<(), String> {
    let on_boundary = ring.iter().enumerate().any(|(i, a)| {
        let d = ring[(i + 1) % ring.len()] - *a;
        let v = p - *a;
        cross(d, v).abs() <= EPS * length(d).max(1.)
            && dot(v, d) >= -EPS
            && dot(v, d) <= dot(d, d) + EPS
    });
    let mut inside = false;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    if !inside && !on_boundary {
        return Err("The walking path must remain inside the stair contour.".into());
    }
    Ok(())
}
#[derive(Clone)]
struct Side {
    a: Vec2,
    b: Vec2,
    top: f64,
    low_a: f64,
    low_b: f64,
}
type RailNodes = BTreeMap<[i64; 2], (Vec2, Vec<f64>)>;
fn quad(mesh: &mut Mesh, p: [Vec3; 4], color: Color, material: Option<&str>) -> Result<(), String> {
    mesh.face([p[0], p[1], p[2]], color, material)?;
    mesh.face([p[0], p[2], p[3]], color, material)?;
    for i in 0..4 {
        mesh.edge(p[i], p[(i + 1) % 4])?;
    }
    Ok(())
}
pub(super) fn mesh(stair: &Stair) -> Result<Mesh, String> {
    stair.validate_settings()?;
    if stair.contour.len() > 4096
        || stair.walking_path.len() > 4096
        || stair.contour.len() * stair.step_count as usize > 200_000
    {
        return Err("Stair contour/path exceeds the construction budget.".into());
    }
    let (plane, outer) = surface(&stair.contour, stair.up)?;
    let ring = &outer.shapes[0].contours[0];
    let path = stair
        .walking_path
        .iter()
        .map(|p| {
            if !p.is_finite() || (*p - plane.origin).dot(plane.normal).abs() > 1e-5 {
                return Err("Contour and walking path must share a plane.".to_owned());
            }
            Ok(plane.project(*p))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if path.len() < 2 || length(path[0] - path[path.len() - 1]) <= EPS {
        return Err("Walking path must be an open connected polyline.".into());
    }
    let lengths = path
        .windows(2)
        .map(|p| length(p[1] - p[0]))
        .collect::<Vec<_>>();
    if lengths.iter().any(|l| *l <= EPS) {
        return Err("Walking path contains an empty segment.".into());
    }
    for (i, a) in path.windows(2).enumerate() {
        if i + 2 < path.len() && dot(unit(a[1] - a[0])?, unit(path[i + 2] - a[1])?) < -1. + EPS {
            return Err("Walking path cannot reverse along itself.".into());
        }
        for b in path.windows(2).skip(i + 2) {
            let d = a[1] - a[0];
            let e = b[1] - b[0];
            let den = cross(d, e);
            if den.abs() > EPS {
                let t = cross(b[0] - a[0], e) / den;
                let u = cross(b[0] - a[0], d) / den;
                if (-EPS..=1. + EPS).contains(&t) && (-EPS..=1. + EPS).contains(&u) {
                    return Err("Walking path cannot cross or revisit itself.".into());
                }
            }
        }
    }
    let total: f64 = lengths.iter().sum();
    // Validate intermediate points too: concave contours can contain segment ends
    // while the segment itself crosses outside the footprint.
    for pair in path.windows(2) {
        let mut cuts = vec![0., 1.];
        let d = pair[1] - pair[0];
        for i in 0..ring.len() {
            let a = ring[i];
            let e = ring[(i + 1) % ring.len()] - a;
            let den = cross(d, e);
            if den.abs() > EPS {
                let t = cross(a - pair[0], e) / den;
                let u = cross(a - pair[0], d) / den;
                if (0. ..=1.).contains(&t) && (-EPS..=1. + EPS).contains(&u) {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        for ts in cuts.windows(2) {
            station(pair[0] + d * ((ts[0] + ts[1]) * 0.5), ring)?;
        }
    }
    let mut field = outer.triangulate().map_err(|e| e.message.to_owned())?;
    let progress = |point: Vec2| nearest_progress(point, &path, &lengths).0 / total;
    let mut values = field
        .points
        .iter()
        .copied()
        .map(progress)
        .collect::<Vec<_>>();
    // Constrain the interpolated field along the actual walking path, including
    // its bends. Contour triangles partition the whole footprint exactly once.
    // A straight flight spanning the contour already has an exact affine
    // progress field. Avoid subdividing it at every walking station.
    let affine = path.len() == 2
        && field.points.iter().zip(&values).all(|(p, value)| {
            let unbounded = dot(*p - path[0], path[1] - path[0]) / (total * total);
            (unbounded - *value).abs() <= EPS
        });
    if !affine {
        let mut travelled = 0.;
        for (i, &point) in path.iter().enumerate() {
            insert_field_point(
                &mut field.points,
                &mut field.triangles,
                &mut values,
                point,
                travelled / total,
            );
            if i < lengths.len() {
                travelled += lengths[i];
            }
        }
        for i in 0..=stair.step_count {
            let (point, _) = sample(
                &path,
                &lengths,
                total * f64::from(i) / f64::from(stair.step_count),
            )?;
            insert_field_point(
                &mut field.points,
                &mut field.triangles,
                &mut values,
                point,
                f64::from(i) / f64::from(stair.step_count),
            );
        }
    }
    if field
        .triangles
        .len()
        .saturating_mul(stair.step_count as usize)
        > 2_000_000
    {
        return Err(
            "Stair sampling exceeds the construction budget; simplify the path or reduce steps."
                .into(),
        );
    }
    let mut mesh = Mesh::default();
    let mut sides = BTreeMap::<[[i64; 2]; 2], Vec<Side>>::new();
    let material = |slot: &str| stair.materials.get(slot).map(String::as_str);
    for i in 0..stair.step_count {
        let low = f64::from(i) / f64::from(stair.step_count);
        let high = f64::from(i + 1) / f64::from(stair.step_count);
        let top = stair.height * high;
        for triangle in &field.triangles {
            let vertices = triangle.map(|j| (field.points[j as usize], values[j as usize]));
            let min = vertices.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let max = vertices
                .iter()
                .map(|p| p.1)
                .fold(f64::NEG_INFINITY, f64::max);
            if max - min <= EPS
                && (min * f64::from(stair.step_count))
                    .floor()
                    .clamp(0., f64::from(stair.step_count - 1)) as u32
                    != i
            {
                continue;
            }
            let cell = clip_progress(clip_progress(vertices.to_vec(), low, true), high, false);
            if cell.len() < 3 {
                continue;
            }
            let world = |v: (Vec2, f64), height: f64| plane.point(v.0) + plane.normal * height;
            for j in 1..cell.len() - 1 {
                let tri = [cell[0], cell[j], cell[j + 1]];
                mesh.face(
                    tri.map(|p| world(p, top)),
                    stair.tread_color,
                    material("tread"),
                )?;
                mesh.face(
                    [tri[2], tri[1], tri[0]]
                        .map(|p| world(p, p.1 * stair.height - stair.support_thickness)),
                    stair.support_color,
                    material("support"),
                )?;
            }
            for j in 0..cell.len() {
                let a = cell[j];
                let b = cell[(j + 1) % cell.len()];
                if length(a.0 - b.0) <= EPS {
                    continue;
                }
                let mut k = [key(a.0), key(b.0)];
                k.sort();
                sides.entry(k).or_default().push(Side {
                    a: a.0,
                    b: b.0,
                    low_a: a.1 * stair.height - stair.support_thickness,
                    low_b: b.1 * stair.height - stair.support_thickness,
                    top,
                });
            }
        }
    }
    let mut rails: [RailNodes; 2] = Default::default();
    for edges in sides.values() {
        let edge = edges.iter().max_by(|a, b| a.top.total_cmp(&b.top)).unwrap();
        if edges.len() == 2 && (edges[0].top - edges[1].top).abs() <= EPS {
            continue;
        }
        let (low_a, low_b) = if edges.len() == 2 {
            let other = edges.iter().min_by(|a, b| a.top.total_cmp(&b.top)).unwrap();
            (other.top, other.top)
        } else if edges.len() == 1 {
            (edge.low_a, edge.low_b)
        } else {
            return Err("Stair treads overlap; adjust the walking path.".into());
        };
        let p = |uv, h| plane.point(uv) + plane.normal * h;
        quad(
            &mut mesh,
            [
                p(edge.a, low_a),
                p(edge.b, low_b),
                p(edge.b, edge.top),
                p(edge.a, edge.top),
            ],
            stair.support_color,
            material("support"),
        )?;
        if edges.len() != 1 {
            continue;
        }
        let mid = (edge.a + edge.b) * 0.5;
        let (distance, tangent, projected) = nearest_progress(mid, &path, &lengths);
        let side = dot(mid - projected, Vec2::new(-tangent.y, tangent.x));
        if (distance <= EPS || distance >= total - EPS)
            && dot(edge.b - edge.a, tangent).abs() <= EPS
        {
            continue;
        }
        let left = if side.abs() <= EPS {
            dot(edge.b - edge.a, tangent) < 0.
        } else {
            side > 0.
        };
        if (left && !stair.rail_left) || (!left && !stair.rail_right) {
            continue;
        }
        for level in 1..=stair.rail_count {
            let h = edge.top + f64::from(level) * stair.rail_spacing;
            mesh.edge(p(edge.a, h), p(edge.b, h))?;
        }
        let nodes = &mut rails[usize::from(left)];
        for uv in [edge.a, edge.b] {
            nodes
                .entry(key(uv))
                .or_insert_with(|| (uv, vec![]))
                .1
                .push(edge.top);
        }
    }
    for nodes in rails {
        for (_, (uv, heights)) in nodes {
            let min = heights.iter().copied().fold(f64::INFINITY, f64::min);
            let max = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if max - min > EPS {
                for level in 1..=stair.rail_count {
                    let h = f64::from(level) * stair.rail_spacing;
                    mesh.edge(
                        plane.point(uv) + plane.normal * (min + h),
                        plane.point(uv) + plane.normal * (max + h),
                    )?;
                }
            }
        }
    }
    conform_vertical_edges(&mut mesh, plane)?;
    Ok(mesh)
}

// Riser/side edges meet at different heights. Split collinear vertical triangle
// edges at their shared vertices so the shell has no geometric T-junctions.
fn conform_vertical_edges(mesh: &mut Mesh, plane: Plane) -> Result<(), String> {
    let mut columns = BTreeMap::<[i64; 2], Vec<usize>>::new();
    // Only shell vertices matter; guide endpoints are excluded below by face membership.
    let shell = mesh
        .faces
        .iter()
        .flat_map(|(t, _)| *t)
        .collect::<BTreeSet<_>>();
    for i in shell {
        columns
            .entry(key(plane.project(mesh.points[i])))
            .or_default()
            .push(i);
    }
    let faces = std::mem::take(&mut mesh.faces);
    let materials = std::mem::take(&mut mesh.face_materials);
    for ((face, color), material) in faces.into_iter().zip(materials) {
        let mut boundary = vec![];
        for e in 0..3 {
            let a = face[e];
            let b = face[(e + 1) % 3];
            boundary.push(a);
            if key(plane.project(mesh.points[a])) != key(plane.project(mesh.points[b])) {
                continue;
            }
            let delta = mesh.points[b] - mesh.points[a];
            let l = delta.dot(delta);
            if l <= EPS * EPS {
                continue;
            }
            let mut inner = columns[&key(plane.project(mesh.points[a]))]
                .iter()
                .copied()
                .filter_map(|i| {
                    let t = (mesh.points[i] - mesh.points[a]).dot(delta) / l;
                    (t > EPS && t < 1. - EPS).then_some((t, i))
                })
                .collect::<Vec<_>>();
            inner.sort_by(|a, b| a.0.total_cmp(&b.0));
            boundary.extend(inner.into_iter().map(|(_, i)| i));
        }
        if boundary.len() == 3 {
            mesh.faces.push((face, color));
            mesh.face_materials.push(material);
            continue;
        }
        let center = mesh.vertex(
            (mesh.points[face[0]] + mesh.points[face[1]] + mesh.points[face[2]]) * (1. / 3.),
        )?;
        for i in 0..boundary.len() {
            mesh.faces.push((
                [center, boundary[i], boundary[(i + 1) % boundary.len()]],
                color,
            ));
            mesh.face_materials.push(material.clone());
        }
    }
    Ok(())
}

// Linear clipping of a conforming scalar field keeps adjacent tread boundaries identical.
fn clip_progress(input: Vec<(Vec2, f64)>, cut: f64, above: bool) -> Vec<(Vec2, f64)> {
    let mut out = vec![];
    if input.is_empty() {
        return out;
    }
    for i in 0..input.len() {
        let a = input[i];
        let b = input[(i + 1) % input.len()];
        let inside = |v: f64| {
            if above {
                v >= cut - EPS
            } else {
                v <= cut + EPS
            }
        };
        if inside(a.1) {
            out.push(a);
        }
        if inside(a.1) != inside(b.1) && (b.1 - a.1).abs() > EPS {
            let t = ((cut - a.1) / (b.1 - a.1)).clamp(0., 1.);
            out.push((a.0 + (b.0 - a.0) * t, cut));
        }
    }
    out.dedup_by(|a, b| length(a.0 - b.0) <= EPS);
    if out.len() > 1 && length(out[0].0 - out.last().unwrap().0) <= EPS {
        out.pop();
    }
    out
}
fn nearest_progress(p: Vec2, path: &[Vec2], lengths: &[f64]) -> (f64, Vec2, Vec2) {
    let mut best = f64::INFINITY;
    let mut distance = 0.;
    let mut tangent = Vec2::new(1., 0.);
    let mut projected = p;
    let mut offset = 0.;
    for (i, &l) in lengths.iter().enumerate() {
        let d = path[i + 1] - path[i];
        let t = (dot(p - path[i], d) / (l * l)).clamp(0., 1.);
        let q = path[i] + d * t;
        let error = dot(p - q, p - q);
        if error < best - EPS {
            best = error;
            distance = offset + l * t;
            tangent = d * (1. / l);
            projected = q;
        } else if (error - best).abs() <= EPS {
            distance = (distance + offset + l * t) * 0.5;
            let n = tangent + d * (1. / l);
            if let Ok(n) = unit(n) {
                tangent = n;
            }
            projected = (projected + q) * 0.5;
        }
        offset += l;
    }
    (distance, tangent, projected)
}
fn insert_field_point(
    points: &mut Vec<Vec2>,
    triangles: &mut Vec<[u32; 3]>,
    values: &mut Vec<f64>,
    point: Vec2,
    value: f64,
) {
    if let Some(i) = points.iter().position(|p| length(*p - point) <= EPS) {
        values[i] = value;
        return;
    }
    let mut output = vec![];
    let new = points.len() as u32;
    let mut inserted = false;
    for t in triangles.iter().copied() {
        let p = t.map(|i| points[i as usize]);
        let signs = [
            cross(p[1] - p[0], point - p[0]),
            cross(p[2] - p[1], point - p[1]),
            cross(p[0] - p[2], point - p[2]),
        ];
        if signs.iter().any(|s| *s < -EPS) {
            output.push(t);
            continue;
        }
        inserted = true;
        if let Some(e) = signs.iter().position(|s| s.abs() <= EPS) {
            output.extend([
                [t[e], new, t[(e + 2) % 3]],
                [new, t[(e + 1) % 3], t[(e + 2) % 3]],
            ]);
        } else {
            output.extend([[t[0], t[1], new], [t[1], t[2], new], [t[2], t[0], new]]);
        }
    }
    if inserted {
        points.push(point);
        values.push(value);
        *triangles = output;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn straight() -> Stair {
        Stair {
            contour: vec![
                Vec3::ZERO,
                Vec3::X * 4.,
                Vec3::new(4., 0., 12.),
                Vec3::Z * 12.,
            ],
            walking_path: vec![Vec3::new(2., 0., 0.), Vec3::new(2., 0., 12.)],
            rail_left: false,
            rail_right: false,
            ..Default::default()
        }
    }
    fn assert_closed(mesh: &Mesh) {
        let mut edges = BTreeMap::<[usize; 2], (usize, i32)>::new();
        for (t, _) in &mesh.faces {
            for i in 0..3 {
                let a = t[i];
                let b = t[(i + 1) % 3];
                let entry = edges.entry([a.min(b), a.max(b)]).or_default();
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        let bad = edges
            .iter()
            .filter(|(_, v)| **v != (2, 0))
            .collect::<Vec<_>>();
        assert!(
            bad.is_empty(),
            "{} nonmanifold/unoriented edges: {:?}",
            bad.len(),
            bad.iter()
                .take(8)
                .map(|(e, v)| (e, v, e.map(|i| mesh.points[i])))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn stair_straight_shell_is_closed_and_reaches_requested_height() {
        let s = straight();
        let m = mesh(&s).unwrap();
        assert_closed(&m);
        assert!(
            m.faces.len() < 400,
            "Straight stairs should not subdivide the affine walking field: {} faces",
            m.faces.len()
        );
        let top = m
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 10.).abs() < EPS);
        let bottom = m.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        assert!((bottom + 0.2).abs() < EPS);
        assert!(
            m.faces
                .iter()
                .all(|(t, _)| t.iter().all(|i| m.points[*i].is_finite()))
        );
    }
    #[test]
    fn stair_guides_copy_both_sides_at_exact_spacing_and_count() {
        let mut s = straight();
        s.step_count = 6;
        let base = mesh(&s).unwrap();
        s.rail_count = 5;
        s.rail_spacing = 2.;
        s.rail_left = true;
        let left = mesh(&s).unwrap();
        assert_eq!(left.edges.len() - base.edges.len(), 5 * (2 * 6 - 1));
        let base_positions = base
            .edges
            .iter()
            .map(|e| {
                e.map(|i| {
                    [
                        base.points[i].x.to_bits(),
                        base.points[i].y.to_bits(),
                        base.points[i].z.to_bits(),
                    ]
                })
            })
            .collect::<BTreeSet<_>>();
        for e in &left.edges {
            let p = e.map(|i| left.points[i]);
            let k = p.map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]);
            if !base_positions.contains(&k) {
                assert!(p.iter().all(|p| (p.x - 4.).abs() < EPS));
            }
        }
        s.rail_right = true;
        let both = mesh(&s).unwrap();
        assert_eq!(both.edges.len() - base.edges.len(), 2 * 5 * (2 * 6 - 1));
        assert!((both.points.iter().map(|p| p.y).fold(0., f64::max) - 20.).abs() < EPS);
        s.rail_count = 0;
        assert_eq!(mesh(&s).unwrap().edges.len(), base.edges.len());
    }
    #[test]
    fn stair_turning_contours_keep_treads_inside_and_shells_closed() {
        for (contour, path) in [
            (
                vec![(0., 0.), (8., 0.), (8., 3.), (3., 3.), (3., 8.), (0., 8.)],
                vec![(6.5, 1.5), (1.5, 1.5), (1.5, 8.)],
            ),
            (
                vec![
                    (0., 0.),
                    (8., 0.),
                    (8., 10.),
                    (5., 10.),
                    (5., 3.),
                    (3., 3.),
                    (3., 10.),
                    (0., 10.),
                ],
                vec![(1.5, 10.), (1.5, 1.5), (6.5, 1.5), (6.5, 10.)],
            ),
        ] {
            let s = Stair {
                contour: contour
                    .into_iter()
                    .map(|(x, z)| Vec3::new(x, 0., z))
                    .collect(),
                walking_path: path.into_iter().map(|(x, z)| Vec3::new(x, 0., z)).collect(),
                rail_left: false,
                rail_right: false,
                ..Default::default()
            };
            let m = mesh(&s).unwrap();
            assert_closed(&m);
        }
    }
    #[test]
    fn stair_curved_flight_is_closed_and_preserves_its_outline() {
        let points = |radius: f64, reverse: bool| {
            (0..=12).map(move |i| {
                let i = if reverse { 12 - i } else { i };
                let angle = f64::from(i) * std::f64::consts::FRAC_PI_2 / 12.;
                Vec3::new(radius * angle.cos(), 0., radius * angle.sin())
            })
        };
        let s = Stair {
            contour: points(8., false).chain(points(4., true)).collect(),
            walking_path: points(6., false).collect(),
            rail_left: false,
            rail_right: false,
            ..Default::default()
        };
        let m = mesh(&s).unwrap();
        assert_closed(&m);
        assert!(
            m.points
                .iter()
                .all(|p| p.x >= -EPS && p.z >= -EPS && p.x <= 8. + EPS && p.z <= 8. + EPS)
        );
    }
    #[test]
    fn stair_bad_inputs_are_rejected_before_installing_geometry() {
        for mutate in [
            |s: &mut Stair| s.height = 0.,
            |s: &mut Stair| s.rail_spacing = 0.,
            |s: &mut Stair| s.step_count = 0,
            |s: &mut Stair| s.rail_count = 129,
            |s: &mut Stair| s.walking_path[1].y = 1.,
            |s: &mut Stair| s.walking_path[1].x = 9.,
        ] {
            let mut s = straight();
            mutate(&mut s);
            assert!(mesh(&s).is_err());
        }
    }
    #[test]
    fn stair_owner_materials_regeneration_and_height_control_roundtrip() {
        let mut doc = Document::new();
        doc.ensure_scene_root();
        let id = doc
            .put_architecture(
                Document::SCENE_ROOT_DEFINITION_ID,
                None,
                Element::Stair(straight()),
            )
            .unwrap();
        let owner = doc
            .architecture_elements()
            .into_iter()
            .find(|o| o.element.id() == id)
            .unwrap();
        assert_eq!(owner.element.kind(), SelectionKind::Stair);
        let mut legacy = owner.element.value();
        legacy.as_object_mut().unwrap().remove("up");
        legacy.as_object_mut().unwrap().remove("railCount");
        let imported = Element::from_value(STAIRS, legacy).unwrap();
        assert_eq!(imported.value()["railCount"], 4);
        assert_eq!(
            owner.element.value()["walkingEnd"],
            serde_json::json!(Vec3::new(2., 0., 12.))
        );

        let control = owner
            .element
            .controls()
            .unwrap()
            .into_iter()
            .find(|c| c.control == Control::StairHeight)
            .unwrap();
        let changed = owner
            .element
            .with_control_at(&Control::StairHeight, control.point + Vec3::Y * 3.)
            .unwrap();
        assert_eq!(changed.value()["height"], 13.);
        doc.put_architecture(owner.definition, Some(&id), changed)
            .unwrap();
        let saved = serde_json::to_string(&doc).unwrap();
        let mut restored: Document = serde_json::from_str(&saved).unwrap();
        assert!(restored.regenerate_architecture().is_empty());
        assert_eq!(
            restored
                .architecture_elements()
                .iter()
                .find(|o| o.element.id() == id)
                .unwrap()
                .element
                .value()["height"],
            13.
        );
        assert_closed(&restored.architecture_mesh(&owner.target).unwrap());
        let mut selection = crate::selection::SelectionSession::default();
        selection.select(owner.target.clone());
        crate::operations::transforms::transform_selection(
            &mut restored,
            &mut selection,
            Affine3 {
                translation: Vec3::X * 20.,
                ..Affine3::IDENTITY
            },
            true,
        )
        .unwrap();
        assert_eq!(restored.architecture_elements().len(), 2);
        let copied = restored
            .architecture_element(selection.selected().first().unwrap())
            .unwrap();
        assert!(copied.element.value()["contour"][0]["x"].as_f64().unwrap() >= 20.);
        crate::operations::transforms::transform_selection(
            &mut restored,
            &mut selection,
            Affine3 {
                linear: [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
                translation: Vec3::ZERO,
            },
            false,
        )
        .unwrap();
        assert_eq!(restored.architecture_elements().len(), 2);
        for owner in restored.architecture_elements() {
            assert_closed(&restored.architecture_mesh(&owner.target).unwrap());
            assert_eq!(owner.element.value()["height"], 13.);
        }
    }
}
