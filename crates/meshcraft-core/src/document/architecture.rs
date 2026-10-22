//! Canonical architecture parameters and rebuildable, attributed mesh geometry.
use super::*;
use crate::{
    geometry::{
        Vec2,
        polygon::{PolygonOperation, PolygonSet, PolygonShape},
    },
    selection::{SelectedEntity, SelectionKind, SelectionTarget},
    tools::paths::PolylinePath,
};

pub const GENERATED: &str = "meshcraft.architecture";
pub const WALLS: &str = "architectureWalls";
pub const SLABS: &str = "architectureSlabs";
pub const FRAMES: &str = "architectureFrames";
const EPS: f64 = 1e-7;
#[path = "architecture_controls.rs"]
mod controls;
#[path = "architecture_joins.rs"]
mod joins;
#[path = "architecture_slabs.rs"]
mod slabs;
pub use controls::{Control, ControlPoint};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}
impl Default for Color {
    fn default() -> Self {
        Self {
            r: 0.93,
            g: 0.93,
            b: 0.93,
            a: 1.0,
        }
    }
}
impl Color {
    pub fn rgba(self) -> Rgba8 {
        let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba8::new(c(self.r), c(self.g), c(self.b), c(self.a))
    }
    fn validate(self) -> Result<(), String> {
        if [self.r, self.g, self.b, self.a]
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            Ok(())
        } else {
            Err("Architecture colors must have finite channels between zero and one.".into())
        }
    }
}

/// Apply a representable affine edit to canonical parameters, retaining owner-local holes.
/// Sheared extrusions must be exploded before editing as an unrestricted mesh.
pub fn transform_record(field: &str, record: &Value, transform: Affine3) -> Result<Value, String> {
    let mut element = Element::from_value(field, record.clone())?;
    let plane = element.plane()?;
    let u = transform.transform_vector(plane.u);
    let v = transform.transform_vector(plane.v);
    let n = transform.transform_vector(plane.normal);
    let (su, sv, sn) = (u.length(), v.length(), n.length());
    if ![su, sv, sn].into_iter().all(|x| x.is_finite() && x > EPS)
        || u.dot(n).abs() > EPS * su * sn
        || v.dot(n).abs() > EPS * sv * sn
    {
        return Err("Explode architecture before applying a sheared or singular transform.".into());
    }
    let normal = transform
        .transform_normal(plane.normal, crate::geometry::GeometryTolerance::default())
        .ok_or("Singular architecture transform.")?;
    if (!element.holes().is_empty() || matches!(element, Element::Wall(_)))
        && u.dot(v).abs() > EPS * su * sv
    {
        return Err("Explode architecture before skewing its rectangular openings.".into());
    }
    let old_holes = element.holes().to_vec();
    match &mut element {
        Element::Wall(w) => {
            w.start = transform.transform_point(w.start);
            w.end = transform.transform_point(w.end);
            w.up = v / sv;
            w.height *= sv;
            w.end_height = w.end_height.map(|h| h * sv);
            w.thickness *= sn;
            let new_normal = (w.end - w.start)
                .cross(w.up)
                .try_normalized(EPS)
                .ok_or("Invalid wall frame.")?;
            w.offset *= n.dot(new_normal);
            w.inclination_deg = (w.inclination_deg.to_radians().tan() * n.dot(new_normal) / sv)
                .atan()
                .to_degrees();
        }
        Element::Slab(s) => {
            for p in &mut s.profile.points {
                *p = transform.transform_point(*p);
            }
            s.origin = Some(transform.transform_point(plane.origin));
            s.axis_u = Some(u / su);
            s.normal = normal;
            s.thickness *= sn;
        }
        Element::Frame(f) => {
            if (su - sv).abs() > EPS * su.max(sv) || u.dot(v).abs() > EPS * su * sv {
                return Err("Explode a window or door before scaling its frame unevenly.".into());
            }
            for p in &mut f.contour {
                *p = transform.transform_point(*p);
            }
            f.normal = normal;
            f.depth *= sn;
            f.frame_width *= su;
        }
    }
    if !old_holes.is_empty() {
        let next_plane = element.plane()?;
        let holes = element.holes_mut()?;
        for (hole, old) in holes.iter_mut().zip(old_holes) {
            let a = next_plane
                .project(transform.transform_point(plane.point(Vec2::new(old.u0, old.v0))));
            let b = next_plane
                .project(transform.transform_point(plane.point(Vec2::new(old.u1, old.v1))));
            hole.u0 = a.x.min(b.x);
            hole.u1 = a.x.max(b.x);
            hole.v0 = a.y.min(b.y);
            hole.v1 = a.y.max(b.y);
        }
    }
    element.mesh()?;
    let mut result = record.clone();
    result
        .as_object_mut()
        .ok_or("Architecture record must be an object.")?
        .extend(element.value().as_object().unwrap().clone());
    Ok(result)
}

/// Derived geometry becomes authored mesh when an object is exploded.
pub fn materialize(bundle: &mut GeometryBundle) {
    for vertex in bundle.vertices.values_mut() {
        vertex.legacy_aliases.retain(|a| a.source != GENERATED);
    }
    for edge in bundle.segments.values_mut() {
        edge.extensions.remove(GENERATED);
    }
    for face in bundle.triangles.values_mut() {
        face.extensions.remove(GENERATED);
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Hole {
    pub id: String,
    pub name: String,
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
}
impl Hole {
    pub fn from_points(a: Vec2, b: Vec2) -> Self {
        Self {
            u0: a.x.min(b.x),
            u1: a.x.max(b.x),
            v0: a.y.min(b.y),
            v1: a.y.max(b.y),
            ..Default::default()
        }
    }
    fn polygon(&self) -> Result<PolygonSet, String> {
        if ![self.u0, self.u1, self.v0, self.v1]
            .into_iter()
            .all(f64::is_finite)
            || self.u1 - self.u0 <= EPS
            || self.v1 - self.v0 <= EPS
        {
            return Err("Opening corners must define a nonzero rectangle.".into());
        }
        Ok(polygon(vec![
            Vec2::new(self.u0, self.v0),
            Vec2::new(self.u1, self.v0),
            Vec2::new(self.u1, self.v1),
            Vec2::new(self.u0, self.v1),
        ]))
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Wall {
    pub id: String,
    pub name: String,
    pub start: Vec3,
    pub end: Vec3,
    pub thickness: f64,
    pub height: f64,
    pub end_height: Option<f64>,
    pub offset: f64,
    pub auto_join: bool,
    pub up: Vec3,
    pub inclination_deg: f64,
    pub exterior_color: Color,
    pub interior_color: Color,
    pub holes: Vec<Hole>,
}
impl Default for Wall {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Wall".into(),
            start: Vec3::ZERO,
            end: Vec3::X,
            thickness: 0.2,
            height: 2.7,
            end_height: None,
            offset: 0.,
            auto_join: true,
            up: Vec3::Y,
            inclination_deg: 0.0,
            exterior_color: Color::default(),
            interior_color: Color {
                r: 0.84,
                g: 0.84,
                b: 0.84,
                a: 1.0,
            },
            holes: vec![],
        }
    }
}
impl Wall {
    pub fn end_height(&self) -> f64 {
        self.end_height.unwrap_or(self.height)
    }
    pub fn plane(&self) -> Result<Plane, String> {
        let v = self
            .up
            .try_normalized(EPS)
            .ok_or("Wall up must be nonzero.")?;
        let delta = self.end - self.start;
        let u = (delta - v * delta.dot(v))
            .try_normalized(EPS)
            .ok_or("Wall endpoints must differ in plan.")?;
        Ok(Plane {
            origin: self.start,
            u,
            v,
            normal: u.cross(v),
        })
    }
    fn profile(&self) -> Result<PolygonSet, String> {
        let p = self.plane()?;
        let delta = self.end - self.start;
        let length = delta.dot(p.u);
        let rise = delta.dot(p.v);
        Ok(polygon(vec![
            Vec2::new(0., 0.),
            Vec2::new(length, rise),
            Vec2::new(length, rise + self.end_height()),
            Vec2::new(0., self.height),
        ]))
    }
    pub fn point(&self, uv: Vec2) -> Result<Vec3, String> {
        let p = self.plane()?;
        let delta = self.end - self.start;
        Ok(p.point(uv)
            + p.normal
                * (self.offset
                    + (uv.y - delta.dot(p.v) * uv.x / delta.dot(p.u))
                        * self.inclination_deg.to_radians().tan()))
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Slab {
    pub id: String,
    pub name: String,
    pub profile: PolylinePath,
    pub normal: Vec3,
    pub origin: Option<Vec3>,
    pub axis_u: Option<Vec3>,
    pub thickness: f64,
    pub top_color: Color,
    pub bottom_color: Color,
    pub side_color: Color,
    pub holes: Vec<Hole>,
}
impl Default for Slab {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Slab".into(),
            profile: PolylinePath {
                points: vec![],
                closed: true,
            },
            normal: Vec3::Y,
            origin: None,
            axis_u: None,
            thickness: 0.2,
            top_color: Color::default(),
            bottom_color: Color {
                r: 0.84,
                g: 0.84,
                b: 0.84,
                a: 1.0,
            },
            side_color: Color {
                r: 0.88,
                g: 0.88,
                b: 0.88,
                a: 1.0,
            },
            holes: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameKind {
    #[default]
    Window,
    Door,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Frame {
    pub id: String,
    pub name: String,
    pub contour: Vec<Vec3>,
    pub normal: Vec3,
    pub depth: f64,
    pub frame_width: f64,
    pub kind: FrameKind,
    pub color: Color,
    pub glazing_enabled: bool,
    pub glazing_color: Color,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Window".into(),
            contour: vec![],
            normal: Vec3::Z,
            depth: 0.12,
            frame_width: 0.06,
            kind: FrameKind::Window,
            color: Color::default(),
            glazing_enabled: false,
            glazing_color: Color {
                r: 0.72,
                g: 0.84,
                b: 0.95,
                a: 0.4,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Element {
    Wall(Wall),
    Slab(Slab),
    Frame(Frame),
}
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub normal: Vec3,
}
impl Plane {
    pub fn project(self, p: Vec3) -> Vec2 {
        let d = p - self.origin;
        Vec2::new(d.dot(self.u), d.dot(self.v))
    }
    pub fn point(self, p: Vec2) -> Vec3 {
        self.origin + self.u * p.x + self.v * p.y
    }
}
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub points: Vec<Vec3>,
    pub edges: Vec<[usize; 2]>,
    pub faces: Vec<([usize; 3], Rgba8)>,
    index: BTreeMap<[i64; 3], usize>,
    edge_set: BTreeSet<[usize; 2]>,
}
impl Mesh {
    fn vertex(&mut self, p: Vec3) -> Result<usize, String> {
        if !p.is_finite() {
            return Err("Architecture geometry exceeds the supported coordinate range.".into());
        }
        let values = [p.x, p.y, p.z].map(|v| v / EPS);
        if values
            .iter()
            .any(|v| !v.is_finite() || v.abs() > i64::MAX as f64 * 0.5)
        {
            return Err("Architecture coordinates exceed the mesh precision range.".into());
        }
        let key = values.map(|v| v.round() as i64);
        if let Some(i) = self.index.get(&key) {
            return Ok(*i);
        }
        let i = self.points.len();
        self.points.push(p);
        self.index.insert(key, i);
        Ok(i)
    }
    fn face(&mut self, p: [Vec3; 3], color: Color) -> Result<(), String> {
        if (p[1] - p[0]).cross(p[2] - p[0]).length() <= EPS * EPS {
            return Ok(());
        }
        let v = p
            .map(|p| self.vertex(p))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        self.faces.push(([v[0], v[1], v[2]], color.rgba()));
        Ok(())
    }
    fn edge(&mut self, a: Vec3, b: Vec3) -> Result<(), String> {
        if a.distance(b) <= EPS {
            return Ok(());
        }
        let a = self.vertex(a)?;
        let b = self.vertex(b)?;
        let e = [a.min(b), a.max(b)];
        if self.edge_set.insert(e) {
            self.edges.push(e);
        }
        Ok(())
    }
}
fn polygon(points: Vec<Vec2>) -> PolygonSet {
    PolygonSet {
        shapes: vec![PolygonShape {
            contours: vec![points],
        }],
    }
}
fn area(points: &[Vec2]) -> f64 {
    (0..points.len())
        .map(|i| {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        * 0.5
}
fn positive(v: f64, label: &str) -> Result<(), String> {
    if v.is_finite() && v > EPS {
        Ok(())
    } else {
        Err(format!("{label} must be finite and positive."))
    }
}
fn surface(points: &[Vec3], normal: Vec3) -> Result<(Plane, PolygonSet), String> {
    surface_with_basis(points, normal, None, None)
}
fn surface_with_basis(
    points: &[Vec3],
    normal: Vec3,
    origin: Option<Vec3>,
    axis: Option<Vec3>,
) -> Result<(Plane, PolygonSet), String> {
    if points.len() < 3 || points.len() > 8192 || points.iter().any(|p| !p.is_finite()) {
        return Err("A profile needs 3–8192 finite points.".into());
    }
    let n = normal
        .try_normalized(EPS)
        .ok_or("Profile normal must be nonzero.")?;
    let u = axis
        .and_then(|u| u.try_normalized(EPS))
        .or_else(|| {
            points
                .iter()
                .skip(1)
                .find_map(|p| (*p - points[0]).try_normalized(EPS))
        })
        .ok_or("Profile points coincide.")?;
    if u.dot(n).abs() > 1e-5 {
        return Err("Profile normal must be perpendicular to its plane.".into());
    }
    let plane = Plane {
        origin: origin.unwrap_or(points[0]),
        u,
        v: n.cross(u),
        normal: n,
    };
    if points
        .iter()
        .any(|p| (*p - plane.origin).dot(n).abs() > 1e-5)
    {
        return Err("Profile points must lie on one plane.".into());
    }
    let mut ring = points.iter().map(|p| plane.project(*p)).collect::<Vec<_>>();
    if ring
        .first()
        .zip(ring.last())
        .is_some_and(|(a, b)| a.distance(*b) <= EPS)
    {
        ring.pop();
    }
    let signed = area(&ring);
    if signed.abs() <= EPS * EPS {
        return Err("Profile has zero area.".into());
    }
    if signed < 0.0 {
        ring.reverse();
    }
    let original = polygon(ring);
    let clean = original.polygonize().map_err(|e| e.message.to_owned())?;
    if clean.shapes.len() != 1
        || clean.shapes[0].contours.len() != 1
        || (area(&clean.shapes[0].contours[0]).abs() - signed.abs()).abs()
            > EPS * signed.abs().max(1.0)
    {
        return Err("Profile must be a simple closed polyline without self-intersections.".into());
    }
    Ok((plane, clean))
}
fn cut(mut shape: PolygonSet, holes: &[Hole]) -> Result<PolygonSet, String> {
    for hole in holes {
        shape = shape
            .overlay(&hole.polygon()?, PolygonOperation::Difference)
            .map_err(|e| e.message.to_owned())?;
    }
    if shape.shapes.is_empty() {
        return Err("The openings remove the entire element.".into());
    }
    Ok(shape)
}
// Closed prism, including the inner reveal surfaces for every clipped opening.
fn prism(
    mesh: &mut Mesh,
    shape: &PolygonSet,
    plane: Plane,
    low: Vec3,
    high: Vec3,
    colors: [Color; 3],
) -> Result<(), String> {
    for c in colors {
        c.validate()?;
    }
    let tri = shape.triangulate().map_err(|e| e.message.to_owned())?;
    for indices in tri.triangles {
        let p = indices.map(|i| plane.point(tri.points[i as usize]));
        mesh.face([p[0] + high, p[1] + high, p[2] + high], colors[0])?;
        mesh.face([p[2] + low, p[1] + low, p[0] + low], colors[1])?;
    }
    for shape in &shape.shapes {
        for (index, contour) in shape.contours.iter().enumerate() {
            let mut contour = contour.clone();
            if (area(&contour) > 0.0) != (index == 0) {
                contour.reverse();
            }
            for i in 0..contour.len() {
                let a = plane.point(contour[i]);
                let b = plane.point(contour[(i + 1) % contour.len()]);
                let [a0, b0, a1, b1] = [a + low, b + low, a + high, b + high];
                mesh.face([a0, b0, b1], colors[2])?;
                mesh.face([a0, b1, a1], colors[2])?;
                mesh.edge(a0, b0)?;
                mesh.edge(a1, b1)?;
                mesh.edge(a0, a1)?;
            }
        }
    }
    Ok(())
}
impl Element {
    pub fn field(&self) -> &'static str {
        match self {
            Self::Wall(_) => WALLS,
            Self::Slab(_) => SLABS,
            Self::Frame(_) => FRAMES,
        }
    }
    pub fn kind(&self) -> SelectionKind {
        match self {
            Self::Wall(_) => SelectionKind::Wall,
            Self::Slab(_) => SelectionKind::Slab,
            Self::Frame(_) => SelectionKind::Frame,
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Self::Wall(v) => &v.id,
            Self::Slab(v) => &v.id,
            Self::Frame(v) => &v.id,
        }
    }
    pub(super) fn set_id(&mut self, id: String) {
        match self {
            Self::Wall(v) => v.id = id,
            Self::Slab(v) => v.id = id,
            Self::Frame(v) => v.id = id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Wall(v) => &v.name,
            Self::Slab(v) => &v.name,
            Self::Frame(v) => &v.name,
        }
    }
    pub fn holes(&self) -> &[Hole] {
        match self {
            Self::Wall(v) => &v.holes,
            Self::Slab(v) => &v.holes,
            Self::Frame(_) => &[],
        }
    }
    pub fn holes_mut(&mut self) -> Result<&mut Vec<Hole>, String> {
        match self {
            Self::Wall(v) => Ok(&mut v.holes),
            Self::Slab(v) => Ok(&mut v.holes),
            _ => Err("Only walls and slabs own openings.".into()),
        }
    }
    pub fn from_value(field: &str, value: Value) -> Result<Self, String> {
        let point = |name: &str| {
            value
                .get(name)
                .cloned()
                .and_then(|p| serde_json::from_value::<Vec3>(p).ok())
        };
        match field {
            WALLS => {
                let mut wall: Wall =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                // Legacy wall axes lie on the inner face. Native records have an up
                // vector and use a centered axis; preserve both representations.
                if value.get("offset").is_none() && value.get("up").is_none() {
                    wall.offset = wall.thickness * 0.5;
                }
                Ok(Self::Wall(wall))
            }
            SLABS => {
                let mut slab: Slab =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if slab.origin.is_none() {
                    slab.origin = if value.get("profile").is_some() {
                        slab.profile.points.first().copied()
                    } else {
                        point("min")
                    };
                }
                if slab.profile.points.is_empty() {
                    slab.profile.points = value
                        .get("contour")
                        .cloned()
                        .and_then(|p| serde_json::from_value(p).ok())
                        .unwrap_or_default();
                }
                if slab.profile.points.is_empty()
                    && let (Some(a), Some(b)) = (point("min"), point("max"))
                {
                    let u = point("axisU")
                        .unwrap_or(Vec3::X)
                        .try_normalized(EPS)
                        .ok_or("Slab axis is invalid.")?;
                    let v = point("axisV")
                        .unwrap_or(Vec3::Z)
                        .try_normalized(EPS)
                        .ok_or("Slab axis is invalid.")?;
                    let d = b - a;
                    slab.profile.points = vec![a, a + u * d.dot(u), b, a + v * d.dot(v)];
                }
                if value.get("profile").is_none() {
                    // K3d normalizes handedness before storing hole UVs: its slabBasis
                    // already uses normal.cross(axisU), even if stored axisV opposes it.
                    // Its min/max plane is the lower face; our profile is the top face.
                    let lift = slab
                        .normal
                        .try_normalized(EPS)
                        .ok_or("Slab normal is invalid.")?
                        * slab.thickness;
                    for p in &mut slab.profile.points {
                        *p += lift;
                    }
                    slab.origin = slab.origin.map(|p| p + lift);
                }
                Ok(Self::Slab(slab))
            }
            FRAMES => {
                let mut frame: Frame =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if frame.contour.is_empty()
                    && let (Some(a), Some(b)) = (point("cornerA"), point("cornerB"))
                {
                    let n = frame
                        .normal
                        .try_normalized(EPS)
                        .ok_or("Frame normal is invalid.")?;
                    let u = Vec3::Y.cross(n).try_normalized(EPS).unwrap_or(Vec3::X);
                    let v = n.cross(u);
                    let d = b - a;
                    frame.contour = vec![a, a + u * d.dot(u), b, a + v * d.dot(v)];
                }
                Ok(Self::Frame(frame))
            }
            _ => Err("Unknown architecture element type.".into()),
        }
    }
    pub fn value(&self) -> Value {
        let mut value = match self {
            Self::Wall(v) => serde_json::to_value(v),
            Self::Slab(v) => serde_json::to_value(v),
            Self::Frame(v) => serde_json::to_value(v),
        }
        .expect("finite validated architecture record");
        // Compatibility fields describe the same canonical polyline; legacy readers
        // retain the full contour even when they only understand rectangular slabs.
        if let Self::Slab(v) = self
            && let Ok((plane, _)) = slabs::surface(v)
        {
            value["contour"] = serde_json::json!(v.profile.points);
            value["min"] = serde_json::json!(plane.origin - plane.normal * v.thickness);
            value["max"] = serde_json::json!(
                v.profile.points[v.profile.points.len() / 2] - plane.normal * v.thickness
            );
            value["axisU"] = serde_json::json!(plane.u);
            value["axisV"] = serde_json::json!(plane.v);
        }
        if let Self::Frame(v) = self
            && !v.contour.is_empty()
        {
            value["cornerA"] = serde_json::json!(v.contour[0]);
            value["cornerB"] = serde_json::json!(v.contour[v.contour.len() / 2]);
        }
        value
    }
    pub fn plane(&self) -> Result<Plane, String> {
        match self {
            Self::Wall(w) => w.plane(),
            Self::Slab(s) => slabs::surface(s).map(|v| v.0),
            Self::Frame(f) => surface(&f.contour, f.normal).map(|v| v.0),
        }
    }
    pub fn mesh(&self) -> Result<Mesh, String> {
        let mut mesh = Mesh::default();
        match self {
            Self::Wall(w) => return joins::wall_mesh(w, &[]),
            Self::Slab(s) => return slabs::mesh(s),
            Self::Frame(f) => {
                positive(f.depth, "Frame depth")?;
                positive(f.frame_width, "Frame width")?;
                let (plane, outer) = surface(&f.contour, f.normal)?;
                // Build inward strips at each boundary. Polygon unions handle
                // concave contours without relying on intersecting miter offsets.
                let ring = &outer.shapes[0].contours[0];
                let mut strips = PolygonSet::default();
                for i in 0..ring.len() {
                    let a = ring[i];
                    let b = ring[(i + 1) % ring.len()];
                    let d = b - a;
                    let length = (d.x * d.x + d.y * d.y).sqrt();
                    if length <= EPS {
                        continue;
                    }
                    let n = Vec2::new(-d.y / length, d.x / length) * f.frame_width;
                    let strip = polygon(vec![a, b, b + n, a + n]);
                    strips = if strips.shapes.is_empty() {
                        strip
                    } else {
                        strips
                            .overlay(&strip, PolygonOperation::Union)
                            .map_err(|e| e.message.to_owned())?
                    };
                }
                let border = outer
                    .overlay(&strips, PolygonOperation::Intersection)
                    .map_err(|e| e.message.to_owned())?;
                let inside = outer
                    .overlay(&border, PolygonOperation::Difference)
                    .map_err(|e| e.message.to_owned())?;
                if inside.shapes.is_empty() {
                    return Err("Frame width leaves no internal opening.".into());
                }
                prism(
                    &mut mesh,
                    &border,
                    plane,
                    -plane.normal * f.depth * 0.5,
                    plane.normal * f.depth * 0.5,
                    [f.color; 3],
                )?;
                if f.glazing_enabled {
                    let glass = inside.triangulate().map_err(|e| e.message.to_owned())?;
                    f.glazing_color.validate()?;
                    for t in glass.triangles {
                        let p = t.map(|i| plane.point(glass.points[i as usize]));
                        mesh.face(p, f.glazing_color)?;
                    }
                }
            }
        }
        if mesh.faces.is_empty() {
            return Err("Architecture geometry is empty.".into());
        }
        Ok(mesh)
    }
}

pub fn semantic_entity(extensions: &BTreeMap<String, Value>) -> Option<SelectedEntity> {
    let info = extensions.get(GENERATED)?;
    let id = info.get("id")?.as_str()?.to_owned();
    let kind = match info.get("field")?.as_str()? {
        WALLS => SelectionKind::Wall,
        SLABS => SelectionKind::Slab,
        FRAMES => SelectionKind::Frame,
        _ => return None,
    };
    Some(SelectedEntity::SemanticHandle { kind, id })
}
pub fn generated(extensions: &BTreeMap<String, Value>) -> bool {
    extensions.contains_key(GENERATED)
}
pub fn field(kind: SelectionKind) -> Option<&'static str> {
    match kind {
        SelectionKind::Wall => Some(WALLS),
        SelectionKind::Slab => Some(SLABS),
        SelectionKind::Frame => Some(FRAMES),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct PlacedElement {
    pub target: SelectionTarget,
    pub definition: DefinitionId,
    pub element: Element,
    pub transform: Affine3,
}
impl Document {
    pub fn architecture_defaults(&self) -> Option<&Value> {
        self.extensions.get("k3d.meshcraftArchitectureDefaults")
    }
    pub fn set_architecture_defaults(&mut self, value: Value) -> Result<(), String> {
        if !value.is_object() {
            return Err("Architecture defaults must be an object.".into());
        }
        self.extensions
            .insert("k3d.meshcraftArchitectureDefaults".into(), value);
        self.bump_revision();
        Ok(())
    }
    pub fn architecture_elements(&self) -> Vec<PlacedElement> {
        let mut result = vec![];
        for occurrence in self.scene_geometry_refs().unwrap_or_default() {
            let Some(r) = self.resolve_occurrence(&occurrence) else {
                continue;
            };
            for field in [WALLS, SLABS, FRAMES] {
                let Some(records) = r
                    .definition
                    .domain_data
                    .get(field)
                    .or_else(|| r.definition.extensions.get(&format!("k3d.{field}")))
                    .and_then(Value::as_array)
                else {
                    continue;
                };
                for (index, record) in records.iter().enumerate() {
                    let Some(id) = r
                        .definition
                        .domain_entity_ids
                        .get(field)
                        .and_then(|ids| ids.get(index))
                    else {
                        continue;
                    };
                    let Ok(element) = Element::from_value(field, record.clone()) else {
                        continue;
                    };
                    result.push(PlacedElement {
                        target: SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity: SelectedEntity::SemanticHandle {
                                kind: element.kind(),
                                id: id.clone(),
                            },
                        },
                        definition: r.definition.id,
                        element,
                        transform: r.world_transform,
                    });
                }
            }
        }
        result
    }
    pub fn architecture_element(&self, target: &SelectionTarget) -> Option<PlacedElement> {
        let SelectedEntity::SemanticHandle { kind, id } = &target.entity else {
            return None;
        };
        let field = field(*kind)?;
        let r = self.resolve_occurrence(&target.occurrence)?;
        let index = r
            .definition
            .domain_entity_ids
            .get(field)?
            .iter()
            .position(|i| i == id)?;
        let value = r
            .definition
            .domain_data
            .get(field)
            .or_else(|| r.definition.extensions.get(&format!("k3d.{field}")))?
            .as_array()?
            .get(index)?;
        let element = Element::from_value(field, value.clone()).ok()?;
        Some(PlacedElement {
            target: target.clone(),
            definition: r.definition.id,
            element,
            transform: r.world_transform,
        })
    }
    /// A semantic edit validates/tessellates before committing parameters and mesh together.
    pub fn put_architecture(
        &mut self,
        definition: DefinitionId,
        identity: Option<&str>,
        mut element: Element,
    ) -> Result<String, String> {
        if let Element::Slab(s) = &mut element {
            let (plane, _) = slabs::surface(s)?;
            s.origin = Some(plane.origin);
            s.axis_u = Some(plane.u);
        }
        let mesh = element.mesh()?;
        let mut staged = self.clone();
        let id = identity
            .map(str::to_owned)
            .unwrap_or_else(|| format!("meshcraft:architecture:{}", staged.allocate_id()));
        if element.id().is_empty() {
            element.set_id(id.clone());
        }
        let field = element.field();
        let key = format!("k3d.{field}");
        let d = staged
            .definitions
            .get_mut(&definition)
            .ok_or("Architecture owner is missing.")?;
        let mut records = d
            .domain_data
            .get(field)
            .or_else(|| d.extensions.get(&key))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let ids = d.domain_entity_ids.entry(field.into()).or_default();
        if ids.len() != records.len() {
            return Err("Architecture identities and records are inconsistent.".into());
        }
        let index = if identity.is_some() {
            ids.iter()
                .position(|i| i == &id)
                .ok_or("Architecture element no longer exists.")?
        } else {
            ids.push(id.clone());
            records.push(Value::Null);
            records.len() - 1
        };
        let mut value = records[index].as_object().cloned().unwrap_or_default();
        value.extend(
            element
                .value()
                .as_object()
                .cloned()
                .ok_or("Invalid architecture record.")?,
        );
        records[index] = Value::Object(value);
        d.extensions.insert(key, Value::Array(records.clone()));
        if d.domain_data.contains_key(field) {
            d.domain_data.insert(field.into(), Value::Array(records));
        }
        if field == WALLS {
            staged.rebuild_walls(definition);
        } else {
            staged.replace_architecture_mesh(definition, field, &id, mesh)?;
        }
        staged.bump_revision();
        *self = staged;
        Ok(id)
    }
    fn wall_records(&self, definition: DefinitionId) -> Vec<(String, Wall)> {
        let Some(d) = self.definitions.get(&definition) else {
            return vec![];
        };
        let values = d
            .domain_data
            .get(WALLS)
            .or_else(|| d.extensions.get(&format!("k3d.{WALLS}")))
            .and_then(Value::as_array);
        values
            .into_iter()
            .flatten()
            .zip(d.domain_entity_ids.get(WALLS).into_iter().flatten())
            .filter_map(
                |(v, id)| match Element::from_value(WALLS, v.clone()).ok()? {
                    Element::Wall(mut w) => {
                        w.id = id.clone();
                        Some((id.clone(), w))
                    }
                    _ => None,
                },
            )
            .collect()
    }
    fn rebuild_walls(&mut self, definition: DefinitionId) {
        let records = self.wall_records(definition);
        let neighbors = records.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>();
        for (id, w) in records {
            if let Ok(mesh) = joins::wall_mesh(&w, &neighbors) {
                let _ = self.replace_architecture_mesh(definition, WALLS, &id, mesh);
            }
        }
    }
    pub fn architecture_mesh(&self, target: &SelectionTarget) -> Result<Mesh, String> {
        let placed = self
            .architecture_element(target)
            .ok_or("Architecture owner is missing.")?;
        match placed.element {
            Element::Wall(mut w) => {
                if let SelectedEntity::SemanticHandle { id, .. } = &target.entity {
                    w.id = id.clone();
                }
                joins::wall_mesh(
                    &w,
                    &self
                        .wall_records(placed.definition)
                        .into_iter()
                        .map(|(_, w)| w)
                        .collect::<Vec<_>>(),
                )
            }
            e => e.mesh(),
        }
    }
    fn remove_architecture_mesh(&mut self, definition: DefinitionId, id: &str) {
        let Some(d) = self.definitions.get_mut(&definition) else {
            return;
        };
        let owned = |ext: &BTreeMap<String, Value>| {
            ext.get(GENERATED)
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                == Some(id)
        };
        let mut vertices = BTreeSet::new();
        d.geometry.segments.retain(|_, e| {
            let keep = !owned(&e.extensions);
            if !keep {
                vertices.extend(e.endpoints.map(|v| v.id));
            }
            keep
        });
        d.geometry.triangles.retain(|_, e| {
            let keep = !owned(&e.extensions);
            if !keep {
                vertices.extend(e.vertices.map(|v| v.id));
            }
            keep
        });
        for e in d.geometry.segments.values() {
            for v in e.endpoints {
                vertices.remove(&v.id);
            }
        }
        for e in d.geometry.triangles.values() {
            for v in e.vertices {
                vertices.remove(&v.id);
            }
        }
        d.geometry.vertices.retain(|id, _| !vertices.contains(id));
    }
    fn replace_architecture_mesh(
        &mut self,
        definition: DefinitionId,
        field: &str,
        id: &str,
        mesh: Mesh,
    ) -> Result<(), String> {
        self.remove_architecture_mesh(definition, id);
        let ext = BTreeMap::from([(GENERATED.into(), serde_json::json!({"field":field,"id":id}))]);
        let vertices = mesh
            .points
            .into_iter()
            .map(|position| RawGeometryVertex {
                position,
                legacy_aliases: vec![LegacyAlias {
                    source: GENERATED.into(),
                    id: id.into(),
                }],
            })
            .collect();
        let reference = |vertex| RawVertexRef {
            vertex,
            position_offset: Vec3::ZERO,
        };
        let segments = mesh
            .edges
            .into_iter()
            .map(|e| RawGeometrySegment {
                endpoints: e.map(reference),
                color: Rgba8::new(0, 0, 0, 255),
                legacy_aliases: vec![],
                extensions: ext.clone(),
            })
            .collect();
        let faces = mesh
            .faces
            .into_iter()
            .map(|(v, color)| RawGeometryTriangle {
                vertices: v.map(reference),
                color,
                legacy_aliases: vec![],
                extensions: ext.clone(),
            })
            .collect();
        let bundle = self.build_geometry_bundle_raw(vertices, segments, faces)?;
        let geometry = &mut self
            .definitions
            .get_mut(&definition)
            .ok_or("Architecture owner is missing.")?
            .geometry;
        geometry.vertices.extend(bundle.vertices);
        geometry.segments.extend(bundle.segments);
        geometry.triangles.extend(bundle.triangles);
        Ok(())
    }
    /// Remove generated caches from an export snapshot; canonical records remain.
    pub fn strip_architecture_geometry(&mut self) {
        let owners = self
            .definitions
            .values()
            .flat_map(|d| {
                d.geometry
                    .segments
                    .values()
                    .map(|e| &e.extensions)
                    .chain(d.geometry.triangles.values().map(|e| &e.extensions))
                    .filter_map(|e| {
                        e.get(GENERATED)
                            .and_then(|v| v.get("id"))
                            .and_then(Value::as_str)
                            .map(|id| (d.id, id.to_owned()))
                    })
            })
            .collect::<BTreeSet<_>>();
        for (definition, id) in owners {
            self.remove_architecture_mesh(definition, &id);
        }
    }
    /// Rebuild valid imported records. Unknown/invalid legacy records remain intact.
    pub fn regenerate_architecture(&mut self) -> Vec<String> {
        self.strip_architecture_geometry();
        let mut records = vec![];
        for d in self.definitions.values() {
            for field in [WALLS, SLABS, FRAMES] {
                let Some(values) = d
                    .domain_data
                    .get(field)
                    .or_else(|| d.extensions.get(&format!("k3d.{field}")))
                    .and_then(Value::as_array)
                else {
                    continue;
                };
                for (index, value) in values.iter().enumerate() {
                    if let Some(id) = d
                        .domain_entity_ids
                        .get(field)
                        .and_then(|ids| ids.get(index))
                    {
                        records.push((d.id, field, id.clone(), value.clone()));
                    }
                }
            }
        }
        let mut errors = vec![];
        for (definition, field, id, value) in records {
            match Element::from_value(field, value)
                .and_then(|e| match e {
                    Element::Wall(mut w) => {
                        w.id = id.clone();
                        joins::wall_mesh(
                            &w,
                            &self
                                .wall_records(definition)
                                .into_iter()
                                .map(|(_, w)| w)
                                .collect::<Vec<_>>(),
                        )
                    }
                    e => e.mesh(),
                })
                .and_then(|mesh| self.replace_architecture_mesh(definition, field, &id, mesh))
            {
                Ok(()) => {}
                Err(e) => errors.push(format!("{id}: {e}")),
            }
        }
        if !self.definitions.is_empty() {
            self.bump_revision();
        }
        errors
    }
    pub fn remove_architecture(&mut self, target: &SelectionTarget) -> Result<(), String> {
        let element = self
            .architecture_element(target)
            .ok_or("Architecture element is missing.")?;
        let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
            return Err("Select an architecture element.".into());
        };
        let field = element.element.field();
        let key = format!("k3d.{field}");
        let d = self
            .definitions
            .get_mut(&element.definition)
            .ok_or("Architecture owner is missing.")?;
        let ids = d
            .domain_entity_ids
            .get_mut(field)
            .ok_or("Architecture identity is missing.")?;
        let index = ids
            .iter()
            .position(|i| i == id)
            .ok_or("Architecture identity is missing.")?;
        let mut records = d
            .domain_data
            .get(field)
            .or_else(|| d.extensions.get(&key))
            .and_then(Value::as_array)
            .cloned()
            .ok_or("Architecture records are missing.")?;
        records.remove(index);
        ids.remove(index);
        d.extensions.insert(key, Value::Array(records.clone()));
        if d.domain_data.contains_key(field) {
            d.domain_data.insert(field.into(), Value::Array(records));
        }
        self.remove_architecture_mesh(element.definition, id);
        if field == WALLS {
            self.rebuild_walls(element.definition);
        }
        self.bump_revision();
        Ok(())
    }
    pub fn add_architecture_hole(
        &mut self,
        target: &SelectionTarget,
        a: Vec3,
        b: Vec3,
    ) -> Result<String, String> {
        let mut owner = self
            .architecture_element(target)
            .ok_or("Pick a wall or slab for the opening.")?;
        let inverse = owner
            .transform
            .inverse(crate::geometry::GeometryTolerance::default())
            .ok_or("Architecture transform is singular.")?;
        let plane = owner.element.plane()?;
        let mut hole = Hole::from_points(
            plane.project(inverse.transform_point(a)),
            plane.project(inverse.transform_point(b)),
        );
        hole.polygon()?;
        let available = match &owner.element {
            Element::Wall(w) => cut(w.profile()?, &w.holes)?,
            Element::Slab(s) => cut(slabs::surface(s)?.1, &s.holes)?,
            _ => return Err("Only walls and slabs own openings.".into()),
        };
        let overlap = available
            .overlay(&hole.polygon()?, PolygonOperation::Intersection)
            .map_err(|e| e.message.to_owned())?;
        if overlap
            .shapes
            .iter()
            .flat_map(|s| s.contours.iter().enumerate())
            .map(|(i, r)| {
                if i == 0 {
                    area(r).abs()
                } else {
                    -area(r).abs()
                }
            })
            .sum::<f64>()
            <= EPS * EPS
        {
            return Err("The opening does not intersect new material on its owner.".into());
        }
        // IDs only advance on successful edits.
        let id = format!("meshcraft:opening:{}", self.next_id);
        hole.id = id.clone();
        hole.name = format!("Opening {}", owner.element.holes().len() + 1);
        owner.element.holes_mut()?.push(hole);
        let SelectedEntity::SemanticHandle { id: identity, .. } = &target.entity else {
            return Err("Select a wall or slab.".into());
        };
        self.put_architecture(owner.definition, Some(identity), owner.element)?;
        self.allocate_id();
        Ok(id)
    }
    pub fn move_slab_vertex(
        &mut self,
        target: &SelectionTarget,
        index: usize,
        world: Vec3,
    ) -> Result<(), String> {
        self.move_architecture_control(target, &Control::SlabVertex { index }, world)
    }
    pub fn remove_architecture_hole(
        &mut self,
        target: &SelectionTarget,
        hole_id: &str,
    ) -> Result<(), String> {
        let mut owner = self
            .architecture_element(target)
            .ok_or("Opening owner is missing.")?;
        let holes = owner.element.holes_mut()?;
        let before = holes.len();
        holes.retain(|h| h.id != hole_id);
        if before == holes.len() {
            return Err("Opening no longer exists.".into());
        }
        let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
            return Err("Select an opening owner.".into());
        };
        self.put_architecture(owner.definition, Some(id), owner.element)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rectangle() -> Slab {
        Slab {
            profile: PolylinePath {
                points: vec![
                    Vec3::ZERO,
                    Vec3::X * 5.0,
                    (Vec3::X + Vec3::Z) * 5.0,
                    Vec3::Z * 5.0,
                ],
                closed: true,
            },
            ..Default::default()
        }
    }
    fn target(kind: SelectionKind, id: String) -> SelectionTarget {
        SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle { kind, id },
        }
    }
    fn volume(mesh: &Mesh) -> f64 {
        mesh.faces
            .iter()
            .map(|(t, _)| {
                let [a, b, c] = t.map(|i| mesh.points[i]);
                a.dot(b.cross(c)) / 6.0
            })
            .sum::<f64>()
            .abs()
    }
    fn closed(mesh: &Mesh) {
        let mut edges = BTreeMap::<[usize; 2], (usize, i32)>::new();
        for (t, _) in &mesh.faces {
            let [a, b, c] = *t;
            for [a, b] in [[a, b], [b, c], [c, a]] {
                let e = edges.entry([a.min(b), a.max(b)]).or_default();
                e.0 += 1;
                e.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(
            edges.values().all(|e| *e == (2, 0)),
            "unclosed/unoriented mesh: {edges:?}"
        );
    }
    #[test]
    fn wall_openings_generate_closed_reveals_with_the_correct_volume() {
        let wall = Wall {
            end: Vec3::X * 5.0,
            height: 3.0,
            holes: vec![Hole {
                u0: 1.0,
                u1: 3.0,
                v0: 0.5,
                v1: 2.5,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mesh = Element::Wall(wall).mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - (15.0 - 4.0) * 0.2).abs() < 1e-5);
    }
    #[test]
    fn overlapping_openings_subtract_their_union_and_doors_may_touch_the_floor() {
        let wall = Wall {
            end: Vec3::X * 5.0,
            height: 3.0,
            holes: vec![
                Hole {
                    u0: 1.0,
                    u1: 3.0,
                    v0: 0.0,
                    v1: 2.0,
                    ..Default::default()
                },
                Hole {
                    u0: 2.0,
                    u1: 4.0,
                    v0: 0.0,
                    v1: 2.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mesh = Element::Wall(wall).mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - (15.0 - 6.0) * 0.2).abs() < 1e-5);
    }
    #[test]
    fn concave_slab_is_a_closed_polyline_solid_with_clipped_openings() {
        let mut slab = rectangle();
        slab.profile.points = vec![
            Vec3::ZERO,
            Vec3::X * 4.0,
            Vec3::new(4.0, 0.0, 2.0),
            Vec3::new(2.0, 0.0, 2.0),
            Vec3::new(2.0, 0.0, 4.0),
            Vec3::Z * 4.0,
        ];
        // This basis uses negative Z as V. Clip a hole across the slab edge.
        slab.holes = vec![Hole {
            u0: 0.5,
            u1: 1.5,
            v0: -5.0,
            v1: -3.0,
            ..Default::default()
        }];
        let mesh = Element::Slab(slab).mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - 11.0 * 0.2).abs() < 1e-5);
    }
    #[test]
    fn frame_creation_is_independent_of_the_wall_and_has_no_implicit_hole() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.0,
                    ..Default::default()
                }),
            )
            .unwrap();
        let before = d
            .architecture_element(&target(SelectionKind::Wall, id))
            .unwrap()
            .element;
        let definition = d.add_definition("Window", DefinitionKind::Architecture);
        let frame = Frame {
            contour: vec![Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y, Vec3::Y],
            ..Default::default()
        };
        let mesh = Element::Frame(frame.clone()).mesh().unwrap();
        closed(&mesh);
        d.put_architecture(definition, None, Element::Frame(frame))
            .unwrap();
        assert_eq!(before.holes().len(), 0);
        let after = d
            .architecture_elements()
            .into_iter()
            .find(|e| e.element.kind() == SelectionKind::Wall)
            .unwrap()
            .element;
        assert_eq!(before, after);
    }
    #[test]
    fn moving_slab_corner_preserves_opening_coordinates_and_rejects_a_crossed_profile_atomically() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(root, None, Element::Slab(rectangle()))
            .unwrap();
        let t = target(SelectionKind::Slab, id);
        d.add_architecture_hole(&t, Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 2.0))
            .unwrap();
        let original = d.architecture_element(&t).unwrap().element;
        let before = original.plane().unwrap();
        let h = &original.holes()[0];
        let hole_corner = before.point(Vec2::new(h.u0, h.v0));
        d.move_slab_vertex(&t, 0, Vec3::new(-1.0, 0.0, -1.0))
            .unwrap();
        let moved = d.architecture_element(&t).unwrap().element;
        let h = &moved.holes()[0];
        assert!(
            moved
                .plane()
                .unwrap()
                .point(Vec2::new(h.u0, h.v0))
                .distance(hole_corner)
                < EPS
        );
        let snapshot = d.clone();
        assert!(d.move_slab_vertex(&t, 1, Vec3::new(2.0, 0.0, 7.0)).is_err());
        assert_eq!(d, snapshot);
    }
    #[test]
    fn deleting_an_owned_hole_restores_material_and_failed_holes_do_not_change_the_document() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(root, None, Element::Slab(rectangle()))
            .unwrap();
        let t = target(SelectionKind::Slab, id);
        let baseline = d.architecture_element(&t).unwrap().element.mesh().unwrap();
        let snapshot = d.clone();
        assert!(
            d.add_architecture_hole(&t, Vec3::X * 20.0, Vec3::new(22.0, 0.0, 2.0))
                .is_err()
        );
        assert_eq!(d, snapshot);
        let hole = d
            .add_architecture_hole(&t, Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 2.0))
            .unwrap();
        assert!(
            volume(&d.architecture_element(&t).unwrap().element.mesh().unwrap())
                < volume(&baseline)
        );
        d.remove_architecture_hole(&t, &hole).unwrap();
        assert!(
            (volume(&d.architecture_element(&t).unwrap().element.mesh().unwrap())
                - volume(&baseline))
            .abs()
                < EPS
        );
    }
    #[test]
    fn schema17_roundtrip_rebuilds_once_preserves_authored_geometry_and_slab_polyline() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        d.append_colored_indexed_mesh_to_definition(
            root,
            vec![Vec3::new(-2.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)],
            vec![([0, 1], Rgba8::new(0, 0, 0, 255))],
            vec![],
        )
        .unwrap();
        d.put_architecture(root, None, Element::Slab(rectangle()))
            .unwrap();
        let source = crate::legacy::export_current_value(&d).unwrap();
        let prototype = &source["prototypes"][0];
        assert_eq!(prototype["segments"].as_array().unwrap().len(), 1);
        assert!(prototype["faces"].as_array().unwrap().is_empty());
        assert_eq!(
            prototype["architectureSlabs"][0]["profile"]["points"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        let mut loaded = crate::legacy::import_current_value(&source).unwrap();
        loaded.validate().unwrap();
        let count = loaded
            .definitions
            .values()
            .map(|d| d.geometry.triangles.len())
            .sum::<usize>();
        assert!(count > 0);
        assert!(loaded.regenerate_architecture().is_empty());
        assert_eq!(
            count,
            loaded
                .definitions
                .values()
                .map(|d| d.geometry.triangles.len())
                .sum::<usize>()
        );
        assert_eq!(
            source,
            crate::legacy::export_current_value(&loaded).unwrap()
        );
    }
    #[test]
    fn sparse_legacy_rectangle_hole_axes_match_the_source_slab() {
        let value = serde_json::json!({"min":Vec3::ZERO,"max":Vec3::new(5.0,0.0,5.0),"axisU":Vec3::X,"axisV":Vec3::Z,"normal":Vec3::Y,"thickness":0.2,
            "holes":[{"u0":1.0,"u1":2.0,"v0":-2.0,"v1":-1.0}]});
        let e = Element::from_value(SLABS, value).unwrap();
        let mesh = e.mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - 24.0 * 0.2).abs() < 1e-5);
    }
    #[test]
    fn semantic_copy_move_and_preview_preserve_owned_openings() {
        use crate::{
            operations::transforms::{preview_selection_transform, transform_selection},
            selection::SelectionSession,
        };
        let mut d = Document::new();
        let wall = Element::Wall(Wall {
            end: Vec3::X * 5.,
            holes: vec![Hole {
                u0: 1.,
                u1: 2.,
                v0: 0.5,
                v1: 2.,
                ..Default::default()
            }],
            ..Default::default()
        });
        let id = d
            .put_architecture(Document::SCENE_ROOT_DEFINITION_ID, None, wall.clone())
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(target(SelectionKind::Wall, id));
        let delta = Vec3::new(3., 2., 1.);
        assert!(
            !preview_selection_transform(&d, &selection, Affine3::from_translation(delta))
                .unwrap()
                .is_empty()
        );
        transform_selection(
            &mut d,
            &mut selection,
            Affine3::from_translation(delta),
            true,
        )
        .unwrap();
        assert_eq!(d.architecture_elements().len(), 2);
        let copy = d
            .architecture_element(selection.selected().first().unwrap())
            .unwrap();
        assert_ne!(copy.element.id(), wall.id());
        let Element::Wall(w) = copy.element else {
            panic!()
        };
        assert_eq!(w.start, delta);
        assert_eq!(w.holes, wall.holes());
        transform_selection(
            &mut d,
            &mut selection,
            Affine3::from_translation(Vec3::Y),
            false,
        )
        .unwrap();
        let copy = d
            .architecture_element(selection.selected().first().unwrap())
            .unwrap();
        let Element::Wall(w) = copy.element else {
            panic!()
        };
        assert_eq!(w.start, delta + Vec3::Y);
        assert!(d.validate().is_ok());
    }
    #[test]
    fn clipboard_slab_retains_one_canonical_solid_and_fresh_identity() {
        use crate::{
            document::clipboard::ClipboardSelection,
            selection::{EditContext, SelectionSession},
        };
        let mut d = Document::new();
        let id = d
            .put_architecture(
                Document::SCENE_ROOT_DEFINITION_ID,
                None,
                Element::Slab(rectangle()),
            )
            .unwrap();
        let t = target(SelectionKind::Slab, id);
        d.add_architecture_hole(&t, Vec3::new(1., 0., 1.), Vec3::new(2., 0., 2.))
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(t.clone());
        let clipboard = ClipboardSelection::capture(&d, &selection).unwrap();
        assert!(!clipboard.preview_edges.is_empty());
        let pasted = clipboard
            .paste(&mut d, &EditContext::Document, Vec3::X * 10., true)
            .unwrap();
        assert_eq!(pasted.len(), 1);
        assert_ne!(t, pasted[0]);
        let slabs = d.architecture_elements();
        assert_eq!(slabs.len(), 2);
        let copy = d.architecture_element(&pasted[0]).unwrap();
        assert_eq!(copy.element.holes().len(), 1);
        let Element::Slab(s) = copy.element else {
            panic!()
        };
        assert_eq!(s.profile.points[0], Vec3::X * 10.);
        let counts = d.definitions[&Document::SCENE_ROOT_DEFINITION_ID]
            .geometry
            .triangles
            .len();
        d.regenerate_architecture();
        assert_eq!(
            counts,
            d.definitions[&Document::SCENE_ROOT_DEFINITION_ID]
                .geometry
                .triangles
                .len()
        );
    }
    #[test]
    fn grouping_and_exploding_architecture_materializes_mesh_for_export() {
        use crate::{operations::objects, selection::SelectionSession};
        let mut d = Document::new();
        let id = d
            .put_architecture(
                Document::SCENE_ROOT_DEFINITION_ID,
                None,
                Element::Slab(rectangle()),
            )
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(target(SelectionKind::Slab, id));
        let object = objects::group_mesh_selection(&mut d, &selection, "Floor").unwrap();
        assert_eq!(d.architecture_elements().len(), 1);
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: object,
            entity: SelectedEntity::Occurrence,
        });
        let report = objects::explode_selection(
            &mut d,
            &selection,
            &objects::LegacyAnnotationExplodeAdapter,
        )
        .unwrap();
        assert!(report.triangles > 0);
        let count = d
            .definitions
            .values()
            .map(|d| {
                d.geometry
                    .triangles
                    .values()
                    .filter(|f| !generated(&f.extensions))
                    .count()
            })
            .sum::<usize>();
        assert!(count > 0);
        d.strip_architecture_geometry();
        assert_eq!(
            count,
            d.definitions
                .values()
                .map(|d| d.geometry.triangles.len())
                .sum::<usize>()
        );
    }
    #[test]
    fn transformed_wall_uses_its_own_up_axis_and_slab_holes_follow_reflection() {
        let wall = Element::Wall(Wall {
            end: Vec3::X * 4.,
            ..Default::default()
        });
        let turn = Affine3::from_linear_translation(
            [[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]],
            Vec3::ZERO,
        );
        let transformed =
            Element::from_value(WALLS, transform_record(WALLS, &wall.value(), turn).unwrap())
                .unwrap();
        assert!((transformed.plane().unwrap().v - Vec3::Z).length() < 1e-6);
        let mut slab = rectangle();
        slab.holes.push(Hole {
            u0: 1.,
            u1: 2.,
            v0: -2.,
            v1: -1.,
            ..Default::default()
        });
        let original = Element::Slab(slab);
        let reflection =
            Affine3::from_linear_translation([[-2., 0., 0.], [0., 1., 0.], [0., 0., 3.]], Vec3::X);
        let transformed = Element::from_value(
            SLABS,
            transform_record(SLABS, &original.value(), reflection).unwrap(),
        )
        .unwrap();
        assert!(
            (volume(&transformed.mesh().unwrap()) - volume(&original.mesh().unwrap()) * 6.).abs()
                < 1e-5
        );
    }
    #[test]
    fn architecture_only_scene_root_survives_export_with_and_without_child_objects() {
        for child in [false, true] {
            let mut d = Document::new();
            let root = Document::SCENE_ROOT_DEFINITION_ID;
            d.put_architecture(root, None, Element::Slab(rectangle()))
                .unwrap();
            d.put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    ..Default::default()
                }),
            )
            .unwrap();
            if child {
                d.add_cuboid("Independent object", Vec3::new(1., 1., 1.), Vec3::ZERO)
                    .unwrap();
            }
            let source = crate::legacy::export_current_value(&d).unwrap();
            let root = source["prototypes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "Root")
                .unwrap();
            assert_eq!(root[SLABS].as_array().unwrap().len(), 1);
            assert_eq!(root[WALLS].as_array().unwrap().len(), 1);
            let loaded = crate::legacy::import_current_value(&source).unwrap();
            assert_eq!(loaded.architecture_elements().len(), 2);
            let saved = crate::legacy::export_current_value(&loaded).unwrap();
            let loaded_root = saved["prototypes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "Root")
                .unwrap();
            assert_eq!(root[SLABS], loaded_root[SLABS]);
            assert_eq!(root[WALLS], loaded_root[WALLS]);
            assert_eq!(loaded.architecture_elements().len(), 2);
            assert_eq!(
                source["rootInstance"]["children"],
                saved["rootInstance"]["children"]
            );
        }
    }
    #[test]
    fn wall_independent_endpoint_levels_heights_and_axis_offset_generate_closed_solids() {
        let w = Wall {
            start: Vec3::new(0., 1., 0.),
            end: Vec3::new(5., 2., 0.),
            height: 2.,
            end_height: Some(4.),
            offset: 0.75,
            ..Default::default()
        };
        let mesh = Element::Wall(w.clone()).mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - 5. * 3. * w.thickness).abs() < 1e-5);
        assert!(
            mesh.points
                .iter()
                .all(|p| p.z >= 0.65 - EPS && p.z <= 0.85 + EPS)
        );
        assert!(mesh.points.iter().any(|p| (p.y - 6.).abs() < EPS));
        assert_eq!(w.plane().unwrap().v, Vec3::Y);
    }
    #[test]
    fn adjacent_walls_share_miter_vertices_and_updating_or_disabling_a_join_rebuilds_neighbors() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let a = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 4.,
                    thickness: 0.4,
                    ..Default::default()
                }),
            )
            .unwrap();
        let b = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    start: Vec3::X * 4.,
                    end: Vec3::new(4., 0., 4.),
                    thickness: 0.4,
                    ..Default::default()
                }),
            )
            .unwrap();
        let a = target(SelectionKind::Wall, a);
        let b = target(SelectionKind::Wall, b);
        let ma = d.architecture_mesh(&a).unwrap();
        let mb = d.architecture_mesh(&b).unwrap();
        closed(&ma);
        closed(&mb);
        assert!(
            ma.points
                .iter()
                .any(|p| p.distance(Vec3::new(4.2, 0., -0.2)) < 1e-5)
        );
        assert!(
            mb.points
                .iter()
                .any(|p| p.distance(Vec3::new(4.2, 0., -0.2)) < 1e-5)
        );
        let before = d.effective_geometry(&a.occurrence).unwrap().clone();
        let mut wb = d.architecture_element(&b).unwrap().element;
        let Element::Wall(w) = &mut wb else { panic!() };
        w.auto_join = false;
        let SelectedEntity::SemanticHandle { id, .. } = &b.entity else {
            panic!()
        };
        d.put_architecture(root, Some(id), wb).unwrap();
        assert!(
            !d.architecture_mesh(&a)
                .unwrap()
                .points
                .iter()
                .any(|p| p.x > 4. + EPS)
        );
        assert_ne!(before, d.effective_geometry(&a.occurrence).unwrap().clone());
        d.remove_architecture(&b).unwrap();
        closed(&d.architecture_mesh(&a).unwrap());
    }
    #[test]
    fn three_way_wall_junction_has_a_shared_center_without_an_open_triangle() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let mut targets = vec![];
        for end in [Vec3::X * 4., Vec3::Z * 4., -Vec3::X * 4.] {
            let id = d
                .put_architecture(
                    root,
                    None,
                    Element::Wall(Wall {
                        end,
                        thickness: 0.4,
                        ..Default::default()
                    }),
                )
                .unwrap();
            targets.push(target(SelectionKind::Wall, id));
        }
        let meshes = targets
            .iter()
            .map(|t| d.architecture_mesh(t).unwrap())
            .collect::<Vec<_>>();
        for m in &meshes {
            closed(m);
        }
        assert!(meshes[0].points.iter().any(|p| {
            p.y.abs() < EPS
                && p.length() < 0.3
                && meshes
                    .iter()
                    .all(|m| m.points.iter().any(|v| v.distance(*p) < EPS))
        }));
    }
    #[test]
    fn wall_top_and_base_controls_change_independent_levels_and_reject_zero_height() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let t = target(SelectionKind::Wall, id);
        d.move_architecture_control(&t, &Control::WallTop { end: true }, Vec3::new(5., 5., 0.))
            .unwrap();
        d.move_architecture_control(&t, &Control::WallBase { end: true }, Vec3::new(5., 1., 0.))
            .unwrap();
        let Element::Wall(w) = d.architecture_element(&t).unwrap().element else {
            panic!()
        };
        assert_eq!(w.end.y, 1.);
        assert_eq!(w.end_height(), 5.);
        assert_eq!(w.height, 2.7);
        closed(&d.architecture_mesh(&t).unwrap());
        let revision = d.revision();
        assert!(
            d.move_architecture_control(&t, &Control::WallTop { end: true }, Vec3::new(5., 0., 0.))
                .is_err()
        );
        assert_eq!(revision, d.revision());
    }
    #[test]
    fn hole_controls_resize_in_the_owner_plane_and_retain_owner_identity() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    height: 4.,
                    offset: 0.5,
                    inclination_deg: 10.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let t = target(SelectionKind::Wall, id);
        let hole = d
            .add_architecture_hole(&t, Vec3::new(1., 1., 0.5), Vec3::new(2., 2., 0.5))
            .unwrap();
        let control = Control::HoleCorner {
            hole_id: hole.clone(),
            corner: 1,
        };
        d.move_architecture_control(&t, &control, Vec3::new(3., 3., 2.))
            .unwrap();
        let e = d.architecture_element(&t).unwrap().element;
        assert_eq!(e.holes()[0].id, hole);
        assert_eq!(e.holes()[0].u1, 3.);
        assert_eq!(e.holes()[0].v1, 3.);
        assert_eq!(
            e.controls()
                .unwrap()
                .iter()
                .filter(|c| matches!(c.control, Control::HoleCorner { .. }))
                .count(),
            2
        );
        closed(&d.architecture_mesh(&t).unwrap());
    }
    #[test]
    fn frame_opposite_controls_resize_freeform_contours_without_touching_walls() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let wall = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let frame = Frame {
            contour: vec![
                Vec3::ZERO,
                Vec3::X * 2.,
                Vec3::new(2., 2., 0.),
                Vec3::new(1., 3., 0.),
                Vec3::Y * 2.,
            ],
            normal: Vec3::Z,
            ..Default::default()
        };
        let id = d
            .put_architecture(root, None, Element::Frame(frame))
            .unwrap();
        let t = target(SelectionKind::Frame, id);
        d.move_architecture_control(
            &t,
            &Control::FrameCorner { corner: 1 },
            Vec3::new(4., 6., 5.),
        )
        .unwrap();
        let Element::Frame(f) = d.architecture_element(&t).unwrap().element else {
            panic!()
        };
        assert_eq!(f.contour[2], Vec3::new(4., 4., 0.));
        assert_eq!(f.contour[3], Vec3::new(2., 6., 0.));
        assert!(
            d.architecture_element(&target(SelectionKind::Wall, wall))
                .unwrap()
                .element
                .holes()
                .is_empty()
        );
    }
    #[test]
    fn direct_semantic_explode_keeps_realized_join_geometry_and_survives_reopen() {
        use crate::{operations::objects, selection::SelectionSession};
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 4.,
                    ..Default::default()
                }),
            )
            .unwrap();
        d.put_architecture(
            root,
            None,
            Element::Wall(Wall {
                start: Vec3::X * 4.,
                end: Vec3::new(4., 0., 4.),
                ..Default::default()
            }),
        )
        .unwrap();
        let t = target(SelectionKind::Wall, id);
        let mut selection = SelectionSession::default();
        selection.select(t.clone());
        let report = objects::explode_selection(
            &mut d,
            &selection,
            &objects::LegacyAnnotationExplodeAdapter,
        )
        .unwrap();
        assert!(report.segments > 0 && report.triangles > 0);
        assert!(d.architecture_element(&t).is_none());
        assert_eq!(d.architecture_elements().len(), 1);
        let saved = crate::legacy::export_current_value(&d).unwrap();
        let loaded = crate::legacy::import_current_value(&saved).unwrap();
        assert_eq!(loaded.architecture_elements().len(), 1);
        assert_eq!(
            report.triangles,
            loaded
                .definitions
                .values()
                .flat_map(|d| d.geometry.triangles.values())
                .filter(|f| !generated(&f.extensions))
                .count()
        );
    }
    #[test]
    fn clipboard_wireframe_includes_realized_miter_extensions() {
        use crate::{document::clipboard::ClipboardSelection, selection::SelectionSession};
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 6.,
                    thickness: 0.4,
                    ..Default::default()
                }),
            )
            .unwrap();
        d.put_architecture(
            root,
            None,
            Element::Wall(Wall {
                start: Vec3::X * 6.,
                end: Vec3::new(6., 0., 4.),
                thickness: 0.4,
                ..Default::default()
            }),
        )
        .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(target(SelectionKind::Wall, id));
        let clipboard = ClipboardSelection::capture(&d, &selection).unwrap();
        assert!(
            clipboard
                .preview_edges
                .iter()
                .flatten()
                .any(|p| p.x > 6.1 && p.z < -0.1)
        );
    }
    #[test]
    fn legacy_roof_holes_use_normalized_basis_and_controls_lie_on_the_actual_top() {
        let legacy = serde_json::json!({"id":"roof","min":Vec3::new(-12.,9.,-19.),"max":Vec3::new(9.,9.,14.),"axisU":Vec3::X,"axisV":Vec3::Z,"normal":Vec3::Y,"thickness":1.,"holes":[{"id":"opening","u0":6.,"u1":10.,"v0":-28.,"v1":-16.}]});
        let element = Element::from_value(SLABS, legacy.clone()).unwrap();
        let mesh = element.mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - (21. * 33. - 4. * 12.)).abs() < 1e-5);
        assert!(
            mesh.points
                .iter()
                .all(|p| p.y >= 9. - EPS && p.y <= 10. + EPS)
        );
        let controls = element
            .controls()
            .unwrap()
            .into_iter()
            .filter(|c| matches!(c.control, Control::HoleCorner { .. }))
            .collect::<Vec<_>>();
        assert_eq!(controls[0].point, Vec3::new(-6., 10., 9.));
        assert_eq!(controls[1].point, Vec3::new(-2., 10., -3.));
        let canonical = element.value();
        assert_eq!(canonical["min"], legacy["min"]);
        assert_eq!(canonical["holes"][0]["v0"], legacy["holes"][0]["v0"]);
        assert_eq!(canonical["holes"][0]["v1"], legacy["holes"][0]["v1"]);
        let reopened = Element::from_value(SLABS, canonical).unwrap();
        assert_eq!(element, reopened);
    }
    #[test]
    fn legacy_inner_wall_axis_is_normalized_without_shifting_centered_native_records() {
        let mut value =
            serde_json::json!({"start":Vec3::ZERO,"end":Vec3::X*5.,"height":3.,"thickness":1.});
        let legacy = Element::from_value(WALLS, value.clone()).unwrap();
        let mesh = legacy.mesh().unwrap();
        assert!(mesh.points.iter().all(|p| p.z >= -EPS && p.z <= 1. + EPS));
        value["up"] = serde_json::json!(Vec3::Y);
        let native = Element::from_value(WALLS, value).unwrap();
        let mesh = native.mesh().unwrap();
        assert!(
            mesh.points
                .iter()
                .all(|p| p.z >= -0.5 - EPS && p.z <= 0.5 + EPS)
        );
        assert_eq!(Element::from_value(WALLS, legacy.value()).unwrap(), legacy);
    }
    #[test]
    fn wall_controls_stay_on_axis_even_with_offset_and_inclination() {
        let wall = Wall {
            start: Vec3::new(2., 1., 3.),
            end: Vec3::new(7., 2., 3.),
            height: 3.,
            end_height: Some(5.),
            offset: 1.25,
            inclination_deg: 20.,
            ..Default::default()
        };
        let plane = wall.plane().unwrap();
        for c in Element::Wall(wall.clone()).controls().unwrap() {
            assert!((c.point - plane.origin).dot(plane.normal).abs() < EPS);
        }
        let top = Element::Wall(wall.clone())
            .controls()
            .unwrap()
            .into_iter()
            .find(|c| c.control == Control::WallTop { end: true })
            .unwrap();
        assert_eq!(top.point, wall.end + Vec3::Y * 5.);
    }
    #[test]
    fn wall_opening_has_no_rectangular_grid_segments_on_either_face() {
        let wall = Wall {
            end: Vec3::X * 6.,
            height: 4.,
            auto_join: false,
            holes: vec![Hole {
                u0: 2.,
                u1: 4.,
                v0: 1.,
                v1: 3.,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mesh = Element::Wall(wall.clone()).mesh().unwrap();
        closed(&mesh);
        for e in mesh.edges {
            let [a, b] = e.map(|i| mesh.points[i]);
            if (a.z - b.z).abs() < EPS {
                let p = (a + b) * 0.5;
                let on_outer = p.x.abs() < EPS
                    || (p.x - 6.).abs() < EPS
                    || p.y.abs() < EPS
                    || (p.y - 4.).abs() < EPS;
                let on_hole = ((p.x - 2.).abs() < EPS || (p.x - 4.).abs() < EPS)
                    && p.y >= 1. - EPS
                    && p.y <= 3. + EPS
                    || ((p.y - 1.).abs() < EPS || (p.y - 3.).abs() < EPS)
                        && p.x >= 2. - EPS
                        && p.x <= 4. + EPS;
                assert!(
                    on_outer || on_hole,
                    "artificial wall subdivision: {a:?} -> {b:?}"
                );
            }
        }
    }
    #[test]
    fn sloped_roof_retains_normal_thickness_vertical_sides_and_owned_openings() {
        let mut s = rectangle();
        for p in &mut s.profile.points {
            p.y = 2. + p.x * 0.5;
        }
        s.holes = vec![Hole {
            u0: 1.,
            u1: 3.,
            v0: -3.,
            v1: -1.,
            ..Default::default()
        }];
        let mesh = Element::Slab(s.clone()).mesh().unwrap();
        closed(&mesh);
        let normal = Vec3::new(-0.5, 1., 0.).try_normalized(EPS).unwrap();
        let drop = s.thickness * 1.25_f64.sqrt();
        for p in &mesh.points {
            let height = 2. + p.x * 0.5;
            assert!((p.y - height).abs() < EPS || (p.y - height + drop).abs() < EPS);
            let distance = (Vec3::new(p.x, height, p.z) - *p).dot(normal);
            assert!(distance.abs() < EPS || (distance - s.thickness).abs() < EPS);
        }
        for e in &mesh.edges {
            let [a, b] = e.map(|i| mesh.points[i]);
            if (a.y - b.y).abs() > EPS && (a - b).cross(Vec3::Y).length() < EPS {
                assert!((a.distance(b) - drop).abs() < EPS);
            }
        }
        for c in Element::Slab(s).controls().unwrap() {
            if matches!(c.control, Control::HoleCorner { .. }) {
                assert!((c.point.y - 2. - c.point.x * 0.5).abs() < EPS);
            }
        }
    }
    #[test]
    fn warped_roof_with_a_hole_across_its_fold_is_closed_and_keeps_profile_heights() {
        let mut s = rectangle();
        s.profile.points[1].y = 2.;
        s.profile.points[2].y = 4.;
        s.profile.points[3].y = 1.;
        s.holes = vec![Hole {
            u0: 1.,
            u1: 4.,
            v0: -4.,
            v1: -1.,
            ..Default::default()
        }];
        let mesh = Element::Slab(s.clone()).mesh().unwrap();
        closed(&mesh);
        for p in s.profile.points {
            assert!(mesh.points.iter().any(|v| v.distance(p) < EPS));
        }
    }
    #[test]
    fn roof_height_control_roundtrip_and_collinear_plan_vertex_are_preserved() {
        let mut s = rectangle();
        s.profile.points.insert(1, Vec3::new(2.5, 2., 0.));
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d.put_architecture(root, None, Element::Slab(s)).unwrap();
        let t = target(SelectionKind::Slab, id);
        d.move_slab_vertex(&t, 1, Vec3::new(2.5, 3., 0.)).unwrap();
        let e = d.architecture_element(&t).unwrap().element;
        closed(&e.mesh().unwrap());
        let Element::Slab(s) = &e else { panic!() };
        assert_eq!(s.profile.points[1].y, 3.);
        let loaded = Element::from_value(SLABS, e.value()).unwrap();
        assert_eq!(loaded.value(), e.value());
        closed(&loaded.mesh().unwrap());
    }
    #[test]
    fn owner_only_control_preview_is_wireframe_ready_and_does_not_modify_the_document() {
        let mut d = Document::new();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let id = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    auto_join: false,
                    ..Default::default()
                }),
            )
            .unwrap();
        let target = target(SelectionKind::Wall, id);
        let before = d.clone();
        let element = d.architecture_element(&target).unwrap().element;
        let changed = element
            .with_control_at(&Control::WallBase { end: true }, Vec3::new(6., 1., 3.))
            .unwrap();
        let ghost = changed.mesh().unwrap();
        assert!(!ghost.edges.is_empty());
        assert_eq!(d, before);
        d.move_architecture_control(
            &target,
            &Control::WallBase { end: true },
            Vec3::new(6., 1., 3.),
        )
        .unwrap();
        assert_eq!(d.architecture_element(&target).unwrap().element, changed);
    }
}
