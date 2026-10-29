//! Contour-bounded stairs, with sampled walking stations and editable rail guides.
use super::*;
#[path = "architecture_stair_path.rs"]
mod walking;
use walking::WalkingLine;

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
// Preserve boundary coordinates exactly. Per-cell float/integer polygon
// normalization can round an endpoint differently on its two neighboring treads.
fn triangulate_cell(points: &[Vec2]) -> Result<Vec<[usize; 3]>, String> {
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut triangles = vec![];
    while remaining.len() > 3 {
        let ear = (0..remaining.len())
            .find(|&i| {
                let a = remaining[(i + remaining.len() - 1) % remaining.len()];
                let b = remaining[i];
                let c = remaining[(i + 1) % remaining.len()];
                if cross(points[b] - points[a], points[c] - points[b]) <= EPS * EPS {
                    return false;
                }
                !remaining.iter().any(|&j| {
                    j != a
                        && j != b
                        && j != c
                        && cross(points[b] - points[a], points[j] - points[a]) >= -EPS * EPS
                        && cross(points[c] - points[b], points[j] - points[b]) >= -EPS * EPS
                        && cross(points[a] - points[c], points[j] - points[c]) >= -EPS * EPS
                })
            })
            .ok_or("Could not triangulate a perpendicular stair section.")?;
        triangles.push([
            remaining[(ear + remaining.len() - 1) % remaining.len()],
            remaining[ear],
            remaining[(ear + 1) % remaining.len()],
        ]);
        remaining.remove(ear);
    }
    triangles.push([remaining[0], remaining[1], remaining[2]]);
    Ok(triangles)
}

// Split only the contour interval containing this walking station. An infinite
// half-plane would cut unrelated later flights of a U/S-shaped stair as well.
fn split_cross_section(
    ring: &[Vec2],
    point: Vec2,
    tangent: Vec2,
) -> Result<(Vec<Vec2>, Vec<Vec2>), String> {
    let across = Vec2::new(-tangent.y, tangent.x);
    let mut hits = vec![];
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let da = dot(a - point, tangent);
        let db = dot(b - point, tangent);
        if da.abs() <= EPS {
            hits.push(a);
        }
        if (da < -EPS && db > EPS) || (da > EPS && db < -EPS) {
            hits.push(a + (b - a) * (da / (da - db)));
        }
    }
    hits.sort_by(|a, b| dot(*a - point, across).total_cmp(&dot(*b - point, across)));
    hits.dedup_by(|a, b| length(*a - *b) <= EPS);
    // A walking line may follow a contour edge. Its station is then one end
    // of the cross-section, rather than having width on both sides.
    let on_station = hits.iter().find(|p| dot(**p - point, across).abs() <= EPS);
    let left = hits
        .iter()
        .rev()
        .find(|p| dot(**p - point, across) < -EPS)
        .or(on_station);
    let right = hits
        .iter()
        .find(|p| dot(**p - point, across) > EPS)
        .or(on_station);
    let (Some(&left), Some(&right)) = (left, right) else {
        return Err(
            "The walking path needs room for a perpendicular tread line at every step.".into(),
        );
    };
    if length(right - left) <= EPS {
        return Err("A perpendicular tread section has no width.".into());
    }
    let mut boundary = vec![];
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        boundary.push(a);
        let delta = b - a;
        let mut inserted = [left, right]
            .into_iter()
            .filter_map(|p| {
                let t = dot(p - a, delta) / dot(delta, delta);
                (t > EPS
                    && t < 1. - EPS
                    && cross(delta, p - a).abs() <= EPS * length(delta).max(1.))
                .then_some((t, p))
            })
            .collect::<Vec<_>>();
        inserted.sort_by(|a, b| a.0.total_cmp(&b.0));
        boundary.extend(inserted.into_iter().map(|(_, p)| p));
    }
    let index = |p| {
        boundary
            .iter()
            .position(|v| length(*v - p) <= EPS)
            .ok_or("Could not connect the tread line to its contour.")
    };
    let a = index(left)?;
    let b = index(right)?;
    let arc = |start: usize, end: usize| {
        (0..=(end + boundary.len() - start) % boundary.len())
            .map(|i| boundary[(start + i) % boundary.len()])
            .collect::<Vec<_>>()
    };
    let first = arc(a, b);
    let second = arc(b, a);
    let forward = point + tangent * (EPS * 100.);
    let first_forward = station(forward, &first).is_ok();
    let second_forward = station(forward, &second).is_ok();
    let (tread, remaining) = match (first_forward, second_forward) {
        (true, false) => (second, first),
        (false, true) => (first, second),
        _ => {
            return Err(
                "A tread line cannot separate this walking path; adjust the contour or path."
                    .into(),
            );
        }
    };
    if tread.len() < 3
        || remaining.len() < 3
        || area(&tread) <= EPS * EPS
        || area(&remaining) <= EPS * EPS
    {
        return Err("A perpendicular tread section has zero area.".into());
    }
    Ok((tread, remaining))
}

fn tread_cells(ring: &[Vec2], path: &WalkingLine, count: u32) -> Result<Vec<Vec<Vec2>>, String> {
    let total = path.length;
    let mut remaining = ring.to_vec();
    let mut cells = Vec::with_capacity(count as usize);
    for step in 1..count {
        let (point, tangent) = path.sample(total * f64::from(step) / f64::from(count))?;
        station(point, &remaining)?;
        let (cell, next) = split_cross_section(&remaining, point, tangent)?;
        cells.push(cell);
        remaining = next;
    }
    cells.push(remaining);
    Ok(cells)
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
    let path = WalkingLine::new(&path, ring)?;
    let total = path.length;
    let cells = tread_cells(ring, &path, stair.step_count)?;
    // Shared boundary positions have one underside height. At converging
    // sections the lowest adjoining tread bounds it, preserving support below
    // every tread while keeping a continuous underside rather than floating slabs.
    let mut nodes = BTreeMap::<[i64; 2], (Vec2, f64)>::new();
    for (step, cell) in cells.iter().enumerate() {
        let high = (step + 1) as f64 / f64::from(stair.step_count);
        for &point in cell {
            let value = (path.nearest(point).0 / total).min(high);
            let node = nodes.entry(key(point)).or_insert((point, value));
            node.1 = node.1.min(value);
        }
    }
    let mut mesh = Mesh::default();
    let mut sides = BTreeMap::<[[i64; 2]; 2], Vec<Side>>::new();
    let material = |slot: &str| stair.materials.get(slot).map(String::as_str);
    for (i, contour) in cells.iter().enumerate() {
        let high = (i + 1) as f64 / f64::from(stair.step_count);
        let top = stair.height * high;
        let mut points = vec![];
        for (j, &a) in contour.iter().enumerate() {
            points.push(a);
            let d = contour[(j + 1) % contour.len()] - a;
            let mut inner = nodes
                .values()
                .filter_map(|&(point, _)| {
                    let t = dot(point - a, d) / dot(d, d);
                    (t > EPS
                        && t < 1. - EPS
                        && cross(d, point - a).abs() <= EPS * length(d).max(1.))
                    .then_some((t, point))
                })
                .collect::<Vec<_>>();
            inner.sort_by(|a, b| a.0.total_cmp(&b.0));
            points.extend(inner.into_iter().map(|(_, p)| p));
        }
        points.dedup_by(|a, b| length(*a - *b) <= EPS);
        let triangles = triangulate_cell(&points)?;
        if triangles.len().saturating_mul(stair.step_count as usize) > 2_000_000 {
            return Err("Stair sampling exceeds the construction budget; simplify the path or reduce steps.".into());
        }
        for triangle in &triangles {
            let cell = triangle.map(|j| (points[j], nodes[&key(points[j])].1));
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
        if edges.len() == 2 {
            let step = (edge.top / stair.height * f64::from(stair.step_count)).round() - 1.;
            let (point, tangent) = path.sample(total * step / f64::from(stair.step_count))?;
            if [edge.a, edge.b]
                .iter()
                .any(|p| dot(*p - point, tangent).abs() > EPS * 100.)
            {
                return Err(
                    "The walking path turns too tightly for straight tread fronts; adjust the contour or path."
                        .into(),
                );
            }
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
        let (distance, tangent, projected) = path.nearest(mid);
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
    fn stair_walking_line_can_follow_the_contour_edge() {
        let mut stair = straight();
        stair.walking_path = vec![Vec3::ZERO, Vec3::Z * 12.];
        let generated = mesh(&stair).unwrap();
        assert_closed(&generated);
        assert_straight_tread_fronts(&stair, &generated);
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
    // A tread front is a geometric straight cross-section, independent of the
    // triangulation used to fill either neighboring horizontal surface.
    fn assert_straight_tread_fronts(stair: &Stair, mesh: &Mesh) {
        let plane = stair.plane().unwrap();
        let path = stair
            .walking_path
            .iter()
            .map(|p| plane.project(*p))
            .collect::<Vec<_>>();
        let ring = stair
            .contour
            .iter()
            .map(|p| plane.project(*p))
            .collect::<Vec<_>>();
        let path = WalkingLine::new(&path, &ring).unwrap();
        let total = path.length;
        let mut boundaries = BTreeMap::<[[i64; 2]; 2], (Vec2, Vec2, BTreeMap<u32, usize>)>::new();
        for (face, _) in &mesh.faces {
            let points = face.map(|i| mesh.points[i]);
            let heights = points.map(|p| (p - plane.origin).dot(plane.normal));
            if heights.iter().any(|h| (*h - heights[0]).abs() > EPS) {
                continue;
            }
            let level = (heights[0] / stair.height * f64::from(stair.step_count)).round();
            if level < 1.
                || level > f64::from(stair.step_count)
                || (heights[0] - level * stair.height / f64::from(stair.step_count)).abs() > EPS
            {
                continue;
            }
            for edge in 0..3 {
                let a = plane.project(points[edge]);
                let b = plane.project(points[(edge + 1) % 3]);
                let mut k = [key(a), key(b)];
                k.sort();
                let entry = boundaries
                    .entry(k)
                    .or_insert_with(|| (a, b, BTreeMap::new()));
                *entry.2.entry(level as u32 - 1).or_default() += 1;
            }
        }
        let mut fronts = BTreeSet::new();
        for (_, (a, b, levels)) in boundaries {
            let levels = levels
                .into_iter()
                .filter(|(_, count)| count % 2 == 1)
                .map(|(level, _)| level)
                .collect::<Vec<_>>();
            if levels.len() < 2 {
                continue;
            }
            let cut = *levels.last().unwrap();
            let (point, tangent) = path
                .sample(total * f64::from(cut) / f64::from(stair.step_count))
                .unwrap();
            for endpoint in [a, b] {
                assert!(
                    dot(endpoint - point, tangent).abs() < EPS * 100.,
                    "tread front {cut} bends away from its perpendicular section: {endpoint:?}, station={point:?}, tangent={tangent:?}"
                );
            }
            fronts.insert(cut);
        }
        assert_eq!(
            fronts.len(),
            stair.step_count as usize - 1,
            "Each step must have a shared straight front"
        );
    }

    #[test]
    fn stair_multiple_turns_have_single_perpendicular_tread_fronts() {
        // U-shaped flight: the same infinite cut line crosses both legs, so only
        // the finite contour interval containing the walking station is valid.
        let stair = Stair {
            contour: [
                (0., 0.),
                (8., 0.),
                (8., 10.),
                (5., 10.),
                (5., 3.),
                (3., 3.),
                (3., 10.),
                (0., 10.),
            ]
            .map(|(x, z)| Vec3::new(x, 0., z))
            .to_vec(),
            walking_path: [(1.5, 10.), (1.5, 1.5), (6.5, 1.5), (6.5, 10.)]
                .map(|(x, z)| Vec3::new(x, 0., z))
                .to_vec(),
            rail_left: false,
            rail_right: false,
            step_count: 22,
            ..Default::default()
        };
        let generated = mesh(&stair).unwrap();
        assert_straight_tread_fronts(&stair, &generated);
        assert_closed(&generated);
    }

    #[test]
    fn stair_three_turns_cover_the_contour_with_straight_winder_fronts() {
        let mut stair = Stair {
            contour: [
                (0., 0.),
                (0., 9.5),
                (7.5, 9.5),
                (7.5, 17.5),
                (17.5, 17.5),
                (17.5, 14.5),
                (10.5, 14.5),
                (10.5, 6.5),
                (3., 6.5),
                (3., 0.),
            ]
            .map(|(x, z)| Vec3::new(x, 0., z))
            .to_vec(),
            walking_path: [(1.5, 0.), (1.5, 8.), (9., 8.), (9., 16.), (17.5, 16.)]
                .map(|(x, z)| Vec3::new(x, 0., z))
                .to_vec(),
            rail_left: false,
            rail_right: false,
            step_count: 30,
            ..Default::default()
        };
        for _ in 0..2 {
            let generated = mesh(&stair).unwrap();
            assert_straight_tread_fronts(&stair, &generated);
            assert_closed(&generated);
            let plane = stair.plane().unwrap();
            let footprint = area(
                &stair
                    .contour
                    .iter()
                    .map(|p| plane.project(*p))
                    .collect::<Vec<_>>(),
            )
            .abs();
            let tops: f64 = generated
                .faces
                .iter()
                .filter_map(|(face, _)| {
                    let p = face.map(|i| generated.points[i]);
                    let normal = (p[1] - p[0]).cross(p[2] - p[0]);
                    (normal.dot(plane.normal) > EPS && normal.cross(plane.normal).length() < EPS)
                        .then_some(normal.length() * 0.5)
                })
                .sum();
            assert!(
                (tops - footprint).abs() < EPS * footprint,
                "Treads must tile the whole contour once: {tops} vs {footprint}"
            );
            stair.contour.reverse();
        }
    }

    #[test]
    fn stair_collinear_path_vertices_do_not_change_winder_geometry() {
        let mut stair = Stair {
            contour: [
                (0., 0.),
                (8., 0.),
                (8., 10.),
                (5., 10.),
                (5., 3.),
                (3., 3.),
                (3., 10.),
                (0., 10.),
            ]
            .map(|(x, z)| Vec3::new(x, 0., z))
            .to_vec(),
            walking_path: [(1.5, 10.), (1.5, 1.5), (6.5, 1.5), (6.5, 10.)]
                .map(|(x, z)| Vec3::new(x, 0., z))
                .to_vec(),
            rail_left: false,
            rail_right: false,
            ..Default::default()
        };
        let original = mesh(&stair).unwrap();
        let original_path = stair.walking_path.clone();
        stair.walking_path.clear();
        for pair in original_path.windows(2) {
            stair.walking_path.push(pair[0]);
            stair
                .walking_path
                .push(pair[0] + (pair[1] - pair[0]) * 0.95);
        }
        stair.walking_path.push(*original_path.last().unwrap());
        let refined = mesh(&stair).unwrap();
        assert_eq!(original.points, refined.points);
        assert_eq!(original.faces, refined.faces);
        assert_eq!(original.edges, refined.edges);
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
