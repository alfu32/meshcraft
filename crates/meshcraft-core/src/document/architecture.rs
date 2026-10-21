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
    fn set_id(&mut self, id: String) {
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
            WALLS => serde_json::from_value(value)
                .map(Self::Wall)
                .map_err(|e| e.to_string()),
            SLABS => {
                let mut slab: Slab =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if slab.origin.is_none() {
                    slab.origin = point("min");
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
                if value.get("profile").is_none()
                    && let Some(v) = point("axisV")
                {
                    let plane = surface_with_basis(
                        &slab.profile.points,
                        slab.normal,
                        slab.origin,
                        slab.axis_u,
                    )?
                    .0;
                    if v.dot(plane.v) < 0.0 {
                        for h in &mut slab.holes {
                            let v0 = -h.v1;
                            h.v1 = -h.v0;
                            h.v0 = v0;
                        }
                    }
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
            && let Ok((plane, _)) =
                surface_with_basis(&v.profile.points, v.normal, v.origin, v.axis_u)
        {
            value["contour"] = serde_json::json!(v.profile.points);
            value["min"] = serde_json::json!(plane.origin);
            value["max"] = serde_json::json!(v.profile.points[v.profile.points.len() / 2]);
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
            Self::Wall(w) => {
                let u = (w.end - w.start)
                    .try_normalized(EPS)
                    .ok_or("Wall endpoints must differ.")?;
                let n = u
                    .cross(Vec3::Y)
                    .try_normalized(EPS)
                    .ok_or("A wall baseline cannot be vertical.")?;
                Ok(Plane {
                    origin: w.start,
                    u,
                    v: n.cross(u),
                    normal: n,
                })
            }
            Self::Slab(s) => {
                surface_with_basis(&s.profile.points, s.normal, s.origin, s.axis_u).map(|v| v.0)
            }
            Self::Frame(f) => surface(&f.contour, f.normal).map(|v| v.0),
        }
    }
    pub fn mesh(&self) -> Result<Mesh, String> {
        let mut mesh = Mesh::default();
        match self {
            Self::Wall(w) => {
                positive(w.height, "Wall height")?;
                positive(w.thickness, "Wall thickness")?;
                if !w.start.is_finite()
                    || !w.end.is_finite()
                    || !w.inclination_deg.is_finite()
                    || w.inclination_deg.abs() >= 85.0
                {
                    return Err("Wall geometry or inclination is invalid.".into());
                }
                let plane = self.plane()?;
                let shape = cut(
                    polygon(vec![
                        Vec2::new(0.0, 0.0),
                        Vec2::new(w.start.distance(w.end), 0.0),
                        Vec2::new(w.start.distance(w.end), w.height),
                        Vec2::new(0.0, w.height),
                    ]),
                    &w.holes,
                )?;
                // Inclination is a shear of the closed perforated prism.
                prism(
                    &mut mesh,
                    &shape,
                    plane,
                    -plane.normal * w.thickness * 0.5,
                    plane.normal * w.thickness * 0.5,
                    [w.exterior_color, w.interior_color, w.exterior_color],
                )?;
                let shear = w.inclination_deg.to_radians().tan();
                for p in &mut mesh.points {
                    *p += plane.normal * (plane.project(*p).y * shear);
                }
            }
            Self::Slab(s) => {
                positive(s.thickness, "Slab thickness")?;
                if !s.profile.closed {
                    return Err("A slab needs a closed polyline profile.".into());
                }
                let (plane, shape) =
                    surface_with_basis(&s.profile.points, s.normal, s.origin, s.axis_u)?;
                let shape = cut(shape, &s.holes)?;
                prism(
                    &mut mesh,
                    &shape,
                    plane,
                    -plane.normal * s.thickness,
                    Vec3::ZERO,
                    [s.top_color, s.bottom_color, s.side_color],
                )?;
            }
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
            let (plane, _) = surface_with_basis(&s.profile.points, s.normal, s.origin, s.axis_u)?;
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
        staged.replace_architecture_mesh(definition, field, &id, mesh)?;
        staged.bump_revision();
        *self = staged;
        Ok(id)
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
                .and_then(|e| e.mesh())
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
            Element::Wall(w) => cut(
                polygon(vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(w.start.distance(w.end), 0.0),
                    Vec2::new(w.start.distance(w.end), w.height),
                    Vec2::new(0.0, w.height),
                ]),
                &w.holes,
            )?,
            Element::Slab(s) => cut(
                surface_with_basis(&s.profile.points, s.normal, s.origin, s.axis_u)?.1,
                &s.holes,
            )?,
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
        let mut owner = self
            .architecture_element(target)
            .ok_or("Slab is missing.")?;
        let plane = owner.element.plane()?;
        let inverse = owner
            .transform
            .inverse(crate::geometry::GeometryTolerance::default())
            .ok_or("Slab transform is singular.")?;
        let p = inverse.transform_point(world);
        let p = plane.point(plane.project(p));
        let Element::Slab(s) = &mut owner.element else {
            return Err("Only slabs have profile controls.".into());
        };
        *s.profile
            .points
            .get_mut(index)
            .ok_or("Slab control no longer exists.")? = p;
        let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
            return Err("Select a slab.".into());
        };
        self.put_architecture(owner.definition, Some(id), owner.element)?;
        Ok(())
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
        assert!(
            d.move_slab_vertex(&t, 1, Vec3::new(2.0, 0.0, 7.0))
                .is_err()
        );
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
            "holes":[{"u0":1.0,"u1":2.0,"v0":1.0,"v1":2.0}]});
        let e = Element::from_value(SLABS, value).unwrap();
        let mesh = e.mesh().unwrap();
        closed(&mesh);
        assert!((volume(&mesh) - 24.0 * 0.2).abs() < 1e-5);
    }
}
