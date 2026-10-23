//! Height-bearing slab profiles. Projection defines topology; heights define the roof.
use super::*;

pub(super) fn surface(s: &Slab) -> Result<(Plane, PolygonSet), String> {
    let n = s.normal.try_normalized(EPS).ok_or("Invalid slab normal.")?;
    let origin = s
        .origin
        .or_else(|| s.profile.points.first().copied())
        .ok_or("Empty slab profile.")?;
    let u = s.axis_u.or_else(|| {
        s.profile.points.iter().skip(1).find_map(|p| {
            let d = *p - origin;
            (d - n * d.dot(n)).try_normalized(EPS)
        })
    });
    let projected = s
        .profile
        .points
        .iter()
        .map(|p| *p - n * (*p - origin).dot(n))
        .collect::<Vec<_>>();
    surface_with_basis(&projected, n, Some(origin), u)
}

struct Roof {
    plane: Plane,
    uv: Vec<Vec2>,
    height: Vec<f64>,
    drop: Vec<f64>,
    triangles: Vec<[usize; 3]>,
}
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn weights(p: Vec2, t: [Vec2; 3]) -> [f64; 3] {
    let d = cross(t[1] - t[0], t[2] - t[0]);
    let b = cross(p - t[0], t[2] - t[0]) / d;
    let c = cross(t[1] - t[0], p - t[0]) / d;
    [1. - b - c, b, c]
}
impl Roof {
    fn new(s: &Slab) -> Result<Self, String> {
        let (plane, shape) = surface(s)?;
        let tri = shape.triangulate().map_err(|e| e.message.to_owned())?;
        let mut uv = tri.points;
        let mut triangles = tri
            .triangles
            .into_iter()
            .map(|t| t.map(|i| i as usize))
            .collect::<Vec<_>>();
        // Polygon kernels may simplify collinear plan vertices. Retain them when they
        // carry a distinct height by splitting their incident boundary triangle(s).
        for p in &s.profile.points {
            let p = plane.project(*p);
            if uv.iter().any(|v| v.distance(p) <= EPS) {
                continue;
            }
            let index = uv.len();
            uv.push(p);
            let mut split = vec![];
            triangles.retain(|t| {
                for i in 0..3 {
                    let (a, b, c) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                    let d = uv[b] - uv[a];
                    let length = d.distance(Vec2::ZERO);
                    if length > EPS && cross(d, p - uv[a]).abs() <= EPS * length {
                        let f = ((p.x - uv[a].x) * d.x + (p.y - uv[a].y) * d.y) / (length * length);
                        if f > EPS && f < 1. - EPS {
                            split.extend([[a, index, c], [index, b, c]]);
                            return false;
                        }
                    }
                }
                true
            });
            triangles.extend(split);
        }
        let height = uv
            .iter()
            .map(|p| {
                s.profile
                    .points
                    .iter()
                    .find(|v| plane.project(**v).distance(*p) <= EPS)
                    .map(|v| (*v - plane.origin).dot(plane.normal))
                    .ok_or_else(|| "Roof triangulation lost a profile height.".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut roof = Self {
            plane,
            drop: vec![s.thickness; uv.len()],
            uv,
            height,
            triangles,
        };
        for t in &mut roof.triangles {
            if cross(roof.uv[t[1]] - roof.uv[t[0]], roof.uv[t[2]] - roof.uv[t[0]]) < 0. {
                t.swap(1, 2);
            }
            let p = t.map(|i| roof.plane.point(roof.uv[i]) + roof.plane.normal * roof.height[i]);
            let n = (p[1] - p[0])
                .cross(p[2] - p[0])
                .try_normalized(EPS * EPS)
                .ok_or("Degenerate roof facet.")?;
            let drop = s.thickness / n.dot(roof.plane.normal).abs();
            if !drop.is_finite() {
                return Err("Roof facet is vertical.".into());
            }
            // Vertical sides and a shared underside at folds. Taking the larger
            // incident drop preserves at least the requested normal thickness;
            // a planar roof has exactly that thickness everywhere.
            for i in t {
                roof.drop[*i] = roof.drop[*i].max(drop);
            }
        }
        Ok(roof)
    }
    fn sample(&self, p: Vec2, t: [usize; 3]) -> (Vec3, Vec3) {
        let w = weights(p, t.map(|i| self.uv[i]));
        let interpolate = |values: &[f64]| (0..3).map(|i| values[t[i]] * w[i]).sum::<f64>();
        let high = self.plane.point(p) + self.plane.normal * interpolate(&self.height);
        (high, high - self.plane.normal * interpolate(&self.drop))
    }
    fn point(&self, p: Vec2) -> Vec3 {
        let t = self
            .triangles
            .iter()
            .min_by(|a, b| {
                let outside = |t: &[usize; 3]| {
                    weights(p, t.map(|i| self.uv[i]))
                        .into_iter()
                        .map(|v| (-v).max(0.))
                        .sum::<f64>()
                };
                outside(a).total_cmp(&outside(b))
            })
            .expect("nonempty roof");
        self.sample(p, *t).0
    }
}
pub(super) fn point(s: &Slab, p: Vec2) -> Result<Vec3, String> {
    Ok(Roof::new(s)?.point(p))
}

pub(super) fn mesh(s: &Slab) -> Result<Mesh, String> {
    positive(s.thickness, "Slab thickness")?;
    if !s.profile.closed {
        return Err("A slab needs a closed polyline profile.".into());
    }
    for color in [s.top_color, s.bottom_color, s.side_color] {
        color.validate()?;
    }
    let (plane, shape) = surface(s)?;
    let shape = cut(shape, &s.holes)?;
    let mut mesh = Mesh::default();
    // Keep the original single-polygon path for level slabs, including concave
    // outlines and many openings; height edits alone need facet clipping.
    if s.profile
        .points
        .iter()
        .all(|p| (*p - plane.origin).dot(plane.normal).abs() <= EPS)
    {
        prism(
            &mut mesh,
            &shape,
            plane,
            -plane.normal * s.thickness,
            Vec3::ZERO,
            [s.top_color, s.bottom_color, s.side_color],
            ["top", "bottom", "side"].map(|key| s.materials.get(key).map(String::as_str)),
        )?;
        return Ok(mesh);
    }
    let roof = Roof::new(s)?;
    let mut bottom = BTreeMap::new();
    let mut triangles = vec![];
    for t in &roof.triangles {
        let mut shape = polygon(t.map(|i| roof.uv[i]).to_vec());
        for hole in &s.holes {
            shape = shape
                .overlay(&hole.polygon()?, PolygonOperation::Difference)
                .map_err(|e| e.message.to_owned())?;
        }
        let tri = shape.triangulate().map_err(|e| e.message.to_owned())?;
        for indices in tri.triangles {
            let mut uv = indices.map(|i| tri.points[i as usize]);
            if cross(uv[1] - uv[0], uv[2] - uv[0]) < 0. {
                uv.swap(1, 2);
            }
            let pairs = uv.map(|p| roof.sample(p, *t));
            let top = pairs.map(|p| p.0);
            let low = pairs.map(|p| p.1);
            let ids = top
                .map(|p| mesh.vertex(p))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            for i in 0..3 {
                bottom.insert(ids[i], low[i]);
            }
            triangles.push([ids[0], ids[1], ids[2]]);
        }
    }
    // Independent Boolean clips can introduce a point on only one side of a
    // shared facet edge. Split its neighbor too, before deciding which edges
    // need reveals. Otherwise a T junction becomes an internal side wall.
    for i in bottom.keys().copied() {
        let p = mesh.points[i];
        let mut split = vec![];
        triangles.retain(|t| {
            if t.contains(&i) {
                return true;
            }
            for j in 0..3 {
                let (a, b, c) = (t[j], t[(j + 1) % 3], t[(j + 2) % 3]);
                let d = mesh.points[b] - mesh.points[a];
                let f = (p - mesh.points[a]).dot(d) / d.length_squared();
                if f > EPS && f < 1. - EPS && p.distance(mesh.points[a] + d * f) <= EPS {
                    split.extend([[a, i, c], [i, b, c]]);
                    return false;
                }
            }
            true
        });
        triangles.extend(split);
    }
    let mut edges = BTreeMap::<[usize; 2], Vec<(usize, usize, Vec3)>>::new();
    for ids in triangles {
        let top = ids.map(|i| mesh.points[i]);
        let low = ids.map(|i| bottom[&i]);
        mesh.face(top, s.top_color, s.materials.get("top").map(String::as_str))?;
        mesh.face(
            [low[2], low[1], low[0]],
            s.bottom_color,
            s.materials.get("bottom").map(String::as_str),
        )?;
        let normal = (top[1] - top[0])
            .cross(top[2] - top[0])
            .try_normalized(EPS * EPS)
            .ok_or("Degenerate clipped roof.")?;
        for i in 0..3 {
            let (a, b) = (ids[i], ids[(i + 1) % 3]);
            edges
                .entry([a.min(b), a.max(b)])
                .or_default()
                .push((a, b, normal));
        }
    }
    for adjacent in edges.into_values() {
        let (a, b, n) = adjacent[0];
        let (a0, b0) = (bottom[&a], bottom[&b]);
        let (a1, b1) = (mesh.points[a], mesh.points[b]);
        if adjacent.len() == 1 {
            mesh.face(
                [a0, b0, b1],
                s.side_color,
                s.materials.get("side").map(String::as_str),
            )?;
            mesh.face(
                [a0, b1, a1],
                s.side_color,
                s.materials.get("side").map(String::as_str),
            )?;
            mesh.edge(a0, b0)?;
            mesh.edge(a1, b1)?;
            mesh.edge(a0, a1)?;
        } else if adjacent
            .iter()
            .any(|(_, _, other)| n.dot(*other) < 1. - 1e-8)
        {
            mesh.edge(a0, b0)?;
            mesh.edge(a1, b1)?;
        }
    }
    Ok(mesh)
}
