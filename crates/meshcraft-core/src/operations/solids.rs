//! Bounded polygon BSP booleans and plane sections of resolved mesh geometry.
use crate::document::{
    DefinitionId, Document, GeometryBundle, Rgba8, Segment, SegmentId, Triangle, TriangleId,
    Vertex, VertexId, VertexRef,
};
use crate::geometry::Vec3;
use serde_json::Value;
use std::collections::BTreeMap;
const EPS: f64 = 1e-7;
#[derive(Clone, Debug)]
pub struct Face {
    pub points: [Vec3; 3],
    pub color: Rgba8,
    pub extensions: BTreeMap<String, Value>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Boolean {
    Union,
    Intersection,
    Difference,
}
#[derive(Clone)]
struct Polygon {
    points: Vec<Vec3>,
    color: Rgba8,
    extensions: BTreeMap<String, Value>,
}
impl Polygon {
    fn flip(&mut self) {
        self.points.reverse();
    }
    fn plane(&self) -> Plane {
        let a = self.points[0];
        let n = (1..self.points.len() - 1)
            .find_map(|i| {
                (self.points[i] - a)
                    .cross(self.points[i + 1] - a)
                    .try_normalized(1e-20)
            })
            .expect("validated nondegenerate polygon");
        Plane { n, w: n.dot(a) }
    }
}
#[derive(Clone, Copy)]
struct Plane {
    n: Vec3,
    w: f64,
}
fn split(
    plane: Plane,
    p: Polygon,
    front: &mut Vec<Polygon>,
    back: &mut Vec<Polygon>,
    coplanar: &mut Vec<Polygon>,
    budget: &mut usize,
) -> Result<(), String> {
    if *budget == 0 {
        return Err("Solid operation exceeded its geometry budget.".into());
    }
    *budget -= 1;
    let ds: Vec<_> = p.points.iter().map(|v| plane.n.dot(*v) - plane.w).collect();
    let f = ds.iter().any(|d| *d > EPS);
    let b = ds.iter().any(|d| *d < -EPS);
    match (f, b) {
        (false, false) => coplanar.push(p),
        (true, false) => front.push(p),
        (false, true) => back.push(p),
        (true, true) => {
            let (mut fp, mut bp) = (vec![], vec![]);
            for i in 0..p.points.len() {
                let j = (i + 1) % p.points.len();
                let a = p.points[i];
                let z = p.points[j];
                if ds[i] >= -EPS {
                    fp.push(a);
                }
                if ds[i] <= EPS {
                    bp.push(a);
                }
                if (ds[i] > EPS && ds[j] < -EPS) || (ds[i] < -EPS && ds[j] > EPS) {
                    let q = a + (z - a) * (ds[i] / (ds[i] - ds[j]));
                    fp.push(q);
                    bp.push(q);
                }
            }
            for (points, out) in [(fp, front), (bp, back)] {
                if points.len() >= 3
                    && (1..points.len() - 1).any(|i| {
                        (points[i] - points[0])
                            .cross(points[i + 1] - points[0])
                            .length()
                            > 1e-14
                    })
                {
                    out.push(Polygon {
                        points,
                        color: p.color,
                        extensions: p.extensions.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}
#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    polygons: Vec<Polygon>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
}
impl Node {
    fn build(&mut self, ps: Vec<Polygon>, depth: usize, budget: &mut usize) -> Result<(), String> {
        if ps.is_empty() {
            return Ok(());
        }
        if depth > 128 {
            return Err("Solid geometry exceeds the supported BSP depth.".into());
        }
        let plane = *self.plane.get_or_insert_with(|| ps[0].plane());
        let (mut f, mut b) = (vec![], vec![]);
        for p in ps {
            split(plane, p, &mut f, &mut b, &mut self.polygons, budget)?;
        }
        if !f.is_empty() {
            self.front
                .get_or_insert_with(Default::default)
                .build(f, depth + 1, budget)?;
        }
        if !b.is_empty() {
            self.back
                .get_or_insert_with(Default::default)
                .build(b, depth + 1, budget)?;
        }
        Ok(())
    }
    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(p) = &mut self.plane {
            p.n = -p.n;
            p.w = -p.w;
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }
    fn all(&self) -> Vec<Polygon> {
        let mut p = self.polygons.clone();
        if let Some(f) = &self.front {
            p.extend(f.all());
        }
        if let Some(b) = &self.back {
            p.extend(b.all());
        }
        p
    }
    fn clip(&self, ps: Vec<Polygon>, budget: &mut usize) -> Result<Vec<Polygon>, String> {
        let Some(plane) = self.plane else {
            return Ok(ps);
        };
        let (mut f, mut b, mut cop) = (vec![], vec![], vec![]);
        for p in ps {
            split(plane, p, &mut f, &mut b, &mut cop, budget)?;
        }
        for p in cop {
            if plane.n.dot(p.plane().n) > 0. {
                f.push(p);
            } else {
                b.push(p);
            }
        }
        if let Some(front) = &self.front {
            f = front.clip(f, budget)?;
        }
        b = if let Some(back) = &self.back {
            back.clip(b, budget)?
        } else {
            vec![]
        };
        f.extend(b);
        Ok(f)
    }
    fn clip_to(&mut self, other: &Self, budget: &mut usize) -> Result<(), String> {
        self.polygons = other.clip(std::mem::take(&mut self.polygons), budget)?;
        if let Some(f) = &mut self.front {
            f.clip_to(other, budget)?;
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other, budget)?;
        }
        Ok(())
    }
}
fn key(p: Vec3) -> [i64; 3] {
    [p.x, p.y, p.z].map(|v| (v / EPS).round() as i64)
}
fn validate_solid(faces: &[Face]) -> Result<Vec<Polygon>, String> {
    if faces.is_empty() {
        return Err("Both solids must contain faces.".into());
    }
    let mut edges = BTreeMap::<([i64; 3], [i64; 3]), (usize, i32)>::new();
    let origin = faces[0].points[0];
    let mut volume = 0.;
    for face in faces {
        let [a, b, c] = face.points;
        if face
            .points
            .iter()
            .any(|p| !p.is_finite() || [p.x, p.y, p.z].iter().any(|v| v.abs() > 1e8))
            || (b - a).cross(c - a).length() < EPS * EPS
        {
            return Err(
                "Solids need finite non-degenerate faces within the supported coordinate range."
                    .into(),
            );
        }
        volume += (a - origin).dot((b - origin).cross(c - origin)) / 6.;
        for [a, b] in [[a, b], [b, c], [c, a]] {
            let (a, b) = (key(a), key(b));
            let e = edges
                .entry(if a < b { (a, b) } else { (b, a) })
                .or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    if edges.values().any(|(count, dir)| *count != 2 || *dir != 0) || volume.abs() < EPS * EPS * EPS
    {
        return Err("Select two closed mesh solids with consistent face directions.".into());
    }
    Ok(faces
        .iter()
        .map(|f| {
            let mut points = f.points.to_vec();
            if volume < 0. {
                points.reverse();
            }
            Polygon {
                points,
                color: f.color,
                extensions: f.extensions.clone(),
            }
        })
        .collect())
}
pub fn boolean(a: &[Face], b: &[Face], op: Boolean) -> Result<Vec<Face>, String> {
    let mut budget = 2_000_000;
    let (mut x, mut y) = (Node::default(), Node::default());
    x.build(validate_solid(a)?, 0, &mut budget)?;
    y.build(validate_solid(b)?, 0, &mut budget)?;
    match op {
        Boolean::Union => {
            x.clip_to(&y, &mut budget)?;
            y.clip_to(&x, &mut budget)?;
            y.invert();
            y.clip_to(&x, &mut budget)?;
            y.invert();
            x.build(y.all(), 0, &mut budget)?;
        }
        Boolean::Difference => {
            x.invert();
            x.clip_to(&y, &mut budget)?;
            y.clip_to(&x, &mut budget)?;
            y.invert();
            y.clip_to(&x, &mut budget)?;
            y.invert();
            x.build(y.all(), 0, &mut budget)?;
            x.invert();
        }
        Boolean::Intersection => {
            x.invert();
            y.clip_to(&x, &mut budget)?;
            y.invert();
            x.clip_to(&y, &mut budget)?;
            y.clip_to(&x, &mut budget)?;
            x.build(y.all(), 0, &mut budget)?;
            x.invert();
        }
    }
    let polygons = x.all();
    let vertices: Vec<_> = polygons
        .iter()
        .flat_map(|p| p.points.iter().copied())
        .collect();
    let mut result = vec![];
    // Insert all coplanar boundary splits before centroid fans, avoiding T junctions.
    for p in polygons {
        let mut contour = vec![];
        for i in 0..p.points.len() {
            let a = p.points[i];
            let b = p.points[(i + 1) % p.points.len()];
            let d = b - a;
            let length = d.dot(d);
            if length < EPS * EPS {
                continue;
            }
            let mut ts = vec![0.];
            for q in &vertices {
                if budget == 0 {
                    return Err("Solid result exceeded its triangulation budget.".into());
                }
                budget -= 1;
                let t = (*q - a).dot(d) / length;
                if t > EPS && t < 1. - EPS && (*q - (a + d * t)).length() <= EPS {
                    ts.push(t);
                }
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|a, b| (*a - *b).abs() < EPS);
            contour.extend(ts.into_iter().map(|t| a + d * t));
        }
        if contour.len() < 3 {
            continue;
        }
        let center =
            contour.iter().copied().fold(Vec3::ZERO, |s, p| s + p) / (contour.len() as f64);
        for i in 0..contour.len() {
            let points = [center, contour[i], contour[(i + 1) % contour.len()]];
            if (points[1] - center).cross(points[2] - center).length() > EPS * EPS {
                result.push(Face {
                    points,
                    color: p.color,
                    extensions: p.extensions.clone(),
                });
            }
        }
    }
    Ok(result)
}
pub fn section(faces: &[Face], origin: Vec3, normal: Vec3) -> Result<Vec<[Vec3; 2]>, String> {
    if !origin.is_finite() {
        return Err("The plane origin must be finite.".into());
    }
    let n = normal
        .try_normalized(EPS)
        .ok_or("The plane normal must be nonzero.")?;
    let mut result = BTreeMap::new();
    let mut coplanar = BTreeMap::<([i64; 3], [i64; 3]), ([Vec3; 2], usize)>::new();
    for f in faces {
        let p = f.points;
        let face_normal = (p[1] - p[0]).cross(p[2] - p[0]);
        let d = p.map(|p| (p - origin).dot(n));
        if d.iter().all(|d| d.abs() <= EPS) {
            for i in 0..3 {
                let mut edge = [p[i], p[(i + 1) % 3]];
                if face_normal.dot(n) < 0. {
                    edge.swap(0, 1);
                }
                let (a, b) = (key(edge[0]), key(edge[1]));
                coplanar
                    .entry(if a < b { (a, b) } else { (b, a) })
                    .or_insert((edge, 0))
                    .1 += 1;
            }
            continue;
        }
        let mut hits = vec![];
        for i in 0..3 {
            let j = (i + 1) % 3;
            if d[i].abs() <= EPS {
                hits.push(p[i]);
            }
            if (d[i] > EPS && d[j] < -EPS) || (d[i] < -EPS && d[j] > EPS) {
                hits.push(p[i] + (p[j] - p[i]) * (d[i] / (d[i] - d[j])));
            }
        }
        if hits.len() >= 2 {
            let a = hits[0];
            if let Some(b) = hits.into_iter().find(|p| (*p - a).length() > EPS) {
                let (ka, kb) = (key(a), key(b));
                let mut edge = [a, b];
                if (b - a).dot(n.cross(face_normal)) < 0. {
                    edge.swap(0, 1);
                }
                result.insert(if ka < kb { (ka, kb) } else { (kb, ka) }, edge);
            }
        }
    }
    for (k, (e, count)) in coplanar {
        if count == 1 {
            result.insert(k, e);
        }
    }
    Ok(result.into_values().collect())
}
/// Append a material-preserving result into the active authored graph.
pub fn append_result(
    document: &mut Document,
    definition: DefinitionId,
    faces: Vec<Face>,
    lines: Vec<[Vec3; 2]>,
) -> Result<(Vec<TriangleId>, Vec<SegmentId>), String> {
    let mut g = GeometryBundle::default();
    let mut vertices = BTreeMap::<[i64; 3], VertexRef>::new();
    let mut vertex = |d: &mut Document, g: &mut GeometryBundle, p: Vec3| {
        *vertices.entry(key(p)).or_insert_with(|| {
            let id = VertexId(d.allocate_id());
            g.vertices.insert(
                id,
                Vertex {
                    id,
                    position: p,
                    legacy_aliases: vec![],
                },
            );
            VertexRef::new(id)
        })
    };
    let mut triangles = vec![];
    let mut segments = vec![];
    let mut edge_faces = BTreeMap::<(VertexId, VertexId), Vec<Vec3>>::new();
    for f in faces {
        let refs = f.points.map(|p| vertex(document, &mut g, p));
        let n = (f.points[1] - f.points[0])
            .cross(f.points[2] - f.points[0])
            .try_normalized(EPS * EPS)
            .ok_or("Result has a degenerate face.")?;
        for i in 0..3 {
            let a = refs[i].id;
            let b = refs[(i + 1) % 3].id;
            edge_faces
                .entry(if a < b { (a, b) } else { (b, a) })
                .or_default()
                .push(n);
        }
        let id = TriangleId(document.allocate_id());
        let mut extensions = f.extensions;
        extensions.remove(crate::document::architecture::GENERATED);
        g.triangles.insert(
            id,
            Triangle {
                id,
                vertices: refs,
                color: f.color,
                extensions,
                legacy_aliases: vec![],
            },
        );
        triangles.push(id);
    }
    let mut line_refs: Vec<_> = lines
        .into_iter()
        .map(|l| l.map(|p| vertex(document, &mut g, p)))
        .collect();
    for ((a, b), ns) in edge_faces {
        if ns.len() != 2 || ns[0].dot(ns[1]) < 1. - 1e-7 {
            line_refs.push([VertexRef::new(a), VertexRef::new(b)]);
        }
    }
    let line_material = if line_refs.is_empty() {
        String::new()
    } else {
        document.material_for_color(Rgba8::BLACK)?
    };
    for endpoints in line_refs {
        let id = SegmentId(document.allocate_id());
        g.segments.insert(
            id,
            Segment {
                id,
                endpoints,
                color: Rgba8::BLACK,
                extensions: BTreeMap::from([(
                    crate::document::materials::MATERIAL_KEY.into(),
                    Value::String(line_material.clone()),
                )]),
                legacy_aliases: vec![],
            },
        );
        segments.push(id);
    }
    if !document.append_geometry_to_definition(definition, g) {
        return Err("Cannot append geometry to the active graph.".into());
    }
    Ok((triangles, segments))
}

/// Intersection curves of two surface meshes. Coplanar pairs have no unique curve.
pub fn intersections(a: &[Face], b: &[Face]) -> Result<Vec<[Vec3; 2]>, String> {
    let mut budget = 2_000_000;
    let mut edges = BTreeMap::new();
    for fa in a {
        let pa = fa.points[0];
        let Some(na) = (fa.points[1] - pa)
            .cross(fa.points[2] - pa)
            .try_normalized(EPS * EPS)
        else {
            continue;
        };
        for fb in b {
            if budget == 0 {
                return Err("Mesh intersection exceeded its face-pair budget.".into());
            }
            budget -= 1;
            let pb = fb.points[0];
            let Some(nb) = (fb.points[1] - pb)
                .cross(fb.points[2] - pb)
                .try_normalized(EPS * EPS)
            else {
                continue;
            };
            let Some(axis) = na.cross(nb).try_normalized(EPS) else {
                continue;
            };
            let x = section(std::slice::from_ref(fa), pb, nb)?;
            let y = section(std::slice::from_ref(fb), pa, na)?;
            let (Some(x), Some(y)) = (x.first(), y.first()) else {
                continue;
            };
            let interval = |s: &[Vec3; 2]| {
                let (a, b) = (s[0].dot(axis), s[1].dot(axis));
                (a.min(b), a.max(b))
            };
            let (x0, x1) = interval(x);
            let (y0, y1) = interval(y);
            let (lo, hi) = (x0.max(y0), x1.min(y1));
            if hi - lo > EPS {
                let p = x[0] + axis * (lo - x[0].dot(axis));
                let q = x[0] + axis * (hi - x[0].dot(axis));
                let (kp, kq) = (key(p), key(q));
                edges.insert(if kp < kq { (kp, kq) } else { (kq, kp) }, [p, q]);
            }
        }
    }
    Ok(edges.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube(origin: Vec3, size: f64) -> Vec<Face> {
        let mut d = Document::new();
        let path = d
            .add_cuboid("box", Vec3::new(size, size, size), origin)
            .unwrap();
        let r = d.resolve_occurrence(&path).unwrap();
        let g = d.effective_geometry(&path).unwrap();
        g.triangles
            .values()
            .map(|f| Face {
                points: f.vertices.map(|v| {
                    r.world_transform
                        .transform_point(g.vertex_position(v).unwrap())
                }),
                color: f.color,
                extensions: f.extensions.clone(),
            })
            .collect()
    }
    fn volume(fs: &[Face]) -> f64 {
        fs.iter()
            .map(|f| {
                let [a, b, c] = f.points;
                a.dot(b.cross(c)) / 6.
            })
            .sum::<f64>()
            .abs()
    }
    #[test]
    fn overlapping_closed_solids_and_materials_survive_all_operations() {
        let a = cube(Vec3::ZERO, 2.);
        let b = cube(Vec3::X, 2.);
        for (op, v) in [
            (Boolean::Union, 12.),
            (Boolean::Intersection, 4.),
            (Boolean::Difference, 4.),
        ] {
            let out = boolean(&a, &b, op).unwrap();
            assert!((volume(&out) - v).abs() < 1e-6, "{op:?}: {}", volume(&out));
            validate_solid(&out).unwrap();
            assert!(out.iter().all(|f| f.color == a[0].color));
        }
    }
    #[test]
    fn contained_disjoint_identical_and_open_shells() {
        let a = cube(Vec3::ZERO, 2.);
        let b = cube(Vec3::new(0.5, 0.5, 0.5), 1.);
        assert!((volume(&boolean(&a, &b, Boolean::Difference).unwrap()) - 7.).abs() < 1e-6);
        assert!((volume(&boolean(&a, &a, Boolean::Union).unwrap()) - 8.).abs() < 1e-6);
        assert!(boolean(&a, &a, Boolean::Difference).unwrap().is_empty());
        let far = cube(Vec3::X * 10., 1.);
        assert!(boolean(&a, &far, Boolean::Intersection).unwrap().is_empty());
        assert!((volume(&boolean(&a, &far, Boolean::Union).unwrap()) - 9.).abs() < 1e-6);
        assert!(boolean(&a[..a.len() - 1], &b, Boolean::Union).is_err());
    }
    #[test]
    fn section_planes_cross_and_touch_without_coplanar_diagonals() {
        let a = cube(Vec3::ZERO, 2.);
        let lines = section(&a, Vec3::X, Vec3::X).unwrap();
        assert!(!lines.is_empty());
        assert!(lines.iter().flatten().all(|p| (p.x - 1.).abs() < 1e-8));
        assert!(
            lines
                .iter()
                .map(|s| s[0].cross(s[1]).dot(Vec3::X))
                .sum::<f64>()
                > 0.
        );
        let opposite = section(&a, Vec3::X, -Vec3::X).unwrap();
        assert!(
            opposite
                .iter()
                .map(|s| s[0].cross(s[1]).dot(Vec3::X))
                .sum::<f64>()
                < 0.
        );
        assert_eq!(section(&a, Vec3::ZERO, Vec3::X).unwrap().len(), 4);
        assert!(section(&a, Vec3::X * 4., Vec3::X).unwrap().is_empty());
        assert!(section(&a, Vec3::ZERO, Vec3::ZERO).is_err());
    }
}
