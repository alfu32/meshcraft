//! Scene-owned hotspot definitions and deterministic, instance-local evaluation.
//!
//! The DTO shape matches k3d's HotspotStore. Runtime meshes are derived from the prototype
//! and absolute handle targets; they are never written into an instance geometry override.
use super::*;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operation {
    #[default]
    Move,
    Scale,
    Stretch,
    Rotate,
    MultiplyLinear,
    MultiplyVolumetric,
    MultiplyRotate2D,
    MultiplyRotate3D,
}
impl Operation {
    pub const ALL: [Self; 8] = [
        Self::Move,
        Self::Scale,
        Self::Stretch,
        Self::Rotate,
        Self::MultiplyLinear,
        Self::MultiplyVolumetric,
        Self::MultiplyRotate2D,
        Self::MultiplyRotate3D,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "MOVE",
            Self::Scale => "SCALE",
            Self::Stretch => "STRETCH",
            Self::Rotate => "ROTATE",
            Self::MultiplyLinear => "MULTIPLY_LINEAR",
            Self::MultiplyVolumetric => "MULTIPLY_VOLUMETRIC",
            Self::MultiplyRotate2D => "MULTIPLY_ROTATE_2D",
            Self::MultiplyRotate3D => "MULTIPLY_ROTATE_3D",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Shape {
    TriangleUp,
    Square,
    #[default]
    Circle,
    Diamond,
    TriangleDown,
}
impl Shape {
    pub const ALL: [Self; 5] = [
        Self::TriangleUp,
        Self::Square,
        Self::Circle,
        Self::Diamond,
        Self::TriangleDown,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::TriangleUp => "TRIANGLE_UP",
            Self::Square => "SQUARE",
            Self::Circle => "CIRCLE",
            Self::Diamond => "DIAMOND",
            Self::TriangleDown => "TRIANGLE_DOWN",
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct HotspotColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}
impl Default for HotspotColor {
    fn default() -> Self {
        Self {
            r: 0.2,
            g: 0.55,
            b: 0.95,
            a: 1.0,
        }
    }
}
impl HotspotColor {
    pub fn rgba(&self) -> Rgba8 {
        let byte = |x: f64| (x.clamp(0., 1.) * 255.).round() as u8;
        Rgba8::new(byte(self.r), byte(self.g), byte(self.b), byte(self.a))
    }
    pub fn from_rgba(color: Rgba8) -> Self {
        Self {
            r: color.red as f64 / 255.,
            g: color.green as f64 / 255.,
            b: color.blue as f64 / 255.,
            a: color.alpha as f64 / 255.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct VertexKey {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}
impl VertexKey {
    pub fn at(p: Vec3) -> Self {
        Self {
            x: (p.x * 1000.).round() as i64,
            y: (p.y * 1000.).round() as i64,
            z: (p.z * 1000.).round() as i64,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SegmentAttachment {
    pub id: String,
    pub a: VertexKey,
    pub b: VertexKey,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct TriangleAttachment {
    pub id: String,
    pub a: VertexKey,
    pub b: VertexKey,
    pub c: VertexKey,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hotspot {
    pub id: String,
    pub name: String,
    pub position: Vec3,
    pub operation: Operation,
    pub shape: Shape,
    pub color: HotspotColor,
    pub reference_position: Option<Vec3>,
    pub attached_hotspot_ids: BTreeSet<String>,
    pub attached_vertex_ids: BTreeSet<String>,
    pub attached_segments: Vec<SegmentAttachment>,
    pub attached_triangles: Vec<TriangleAttachment>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}
impl Default for Hotspot {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            position: Vec3::ZERO,
            operation: Operation::Move,
            shape: Shape::Circle,
            color: HotspotColor::default(),
            reference_position: None,
            attached_hotspot_ids: BTreeSet::new(),
            attached_vertex_ids: BTreeSet::new(),
            attached_segments: Vec::new(),
            attached_triangles: Vec::new(),
            extensions: BTreeMap::new(),
        }
    }
}

/// Cache equality and cloning deliberately ignore derived meshes. History contains authored state.
#[derive(Debug, Default)]
pub(crate) struct EvaluationCache(OnceLock<BTreeMap<OccurrenceRef, Evaluation>>);
impl Clone for EvaluationCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl PartialEq for EvaluationCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
#[derive(Clone, Debug)]
pub struct Evaluation {
    pub geometry: GeometryBundle,
    pub positions: BTreeMap<String, Vec3>,
}
impl EvaluationCache {
    pub fn clear(&mut self) {
        self.0.take();
    }
    pub fn get<'a>(
        &'a self,
        document: &Document,
        occurrence: &OccurrenceRef,
    ) -> Option<&'a Evaluation> {
        self.0
            .get_or_init(|| {
                document
                    .occurrence_edits
                    .iter()
                    .filter_map(|(path, edits)| {
                        if edits.hotspot_positions.is_empty() {
                            return None;
                        }
                        document
                            .evaluate_hotspots(path)
                            .ok()
                            .map(|evaluation| (path.clone(), evaluation))
                    })
                    .collect()
            })
            .get(occurrence)
    }
}

impl Document {
    pub fn hotspots(&self, definition: DefinitionId) -> Result<Vec<Hotspot>, String> {
        let definition = self
            .definition(definition)
            .ok_or("object prototype no longer exists")?;
        definition
            .domain_data
            .get("hotspots")
            .map_or(Ok(Vec::new()), |raw| {
                serde_json::from_value(raw.clone())
                    .map_err(|error| format!("invalid hotspot definition: {error}"))
            })
    }

    pub fn put_hotspot(
        &mut self,
        definition: DefinitionId,
        mut hotspot: Hotspot,
    ) -> Result<String, String> {
        let mut hotspots = self.hotspots(definition)?;
        if !hotspot.position.is_finite()
            || hotspot.reference_position.is_some_and(|p| !p.is_finite())
        {
            return Err("hotspot coordinates must be finite".into());
        }
        if [
            hotspot.color.r,
            hotspot.color.g,
            hotspot.color.b,
            hotspot.color.a,
        ]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("hotspot color channels must be between zero and one".into());
        }
        if hotspot.id.is_empty() {
            hotspot.id = format!("hotspot-{}", self.next_id);
            // Commit allocation only after validating the complete candidate.
        }
        if hotspot.name.trim().is_empty() {
            hotspot.name = format!("HOTSPOT_{}", hotspots.len() + 1);
        }
        let id = hotspot.id.clone();
        if let Some(old) = hotspots.iter_mut().find(|h| h.id == id) {
            *old = hotspot;
        } else {
            hotspots.push(hotspot);
        }
        ordered(&hotspots)?;
        let raw = serde_json::to_value(hotspots).map_err(|e| e.to_string())?;
        if self.definitions[&definition].domain_data.get("hotspots") != Some(&raw) {
            self.allocate_id();
            self.definitions
                .get_mut(&definition)
                .unwrap()
                .domain_data
                .insert("hotspots".into(), raw);
            self.bump_revision();
        }
        Ok(id)
    }

    pub fn delete_hotspot(&mut self, definition: DefinitionId, id: &str) -> Result<(), String> {
        let mut hotspots = self.hotspots(definition)?;
        let before = hotspots.len();
        hotspots.retain(|h| h.id != id);
        if before == hotspots.len() {
            return Err("hotspot no longer exists".into());
        }
        for hotspot in &mut hotspots {
            hotspot.attached_hotspot_ids.remove(id);
        }
        self.definitions
            .get_mut(&definition)
            .unwrap()
            .domain_data
            .insert("hotspots".into(), serde_json::to_value(hotspots).unwrap());
        let paths = self
            .occurrence_edits
            .keys()
            .filter(|path| {
                self.resolve_occurrence(path)
                    .is_some_and(|r| r.definition.id == definition)
            })
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            self.occurrence_edits
                .get_mut(&path)
                .unwrap()
                .hotspot_positions
                .remove(id);
        }
        self.bump_revision();
        Ok(())
    }

    pub fn set_hotspot_target(
        &mut self,
        occurrence: &OccurrenceRef,
        id: &str,
        target: Vec3,
    ) -> Result<(), String> {
        let definition = self
            .resolve_occurrence(occurrence)
            .ok_or("instance no longer exists")?
            .definition
            .id;
        if !target.is_finite() {
            return Err("hotspot target must be finite".into());
        }
        let hotspot = self
            .hotspots(definition)?
            .into_iter()
            .find(|h| h.id == id)
            .ok_or("hotspot no longer exists")?;
        if matches!(
            hotspot.operation,
            Operation::MultiplyLinear
                | Operation::MultiplyVolumetric
                | Operation::MultiplyRotate2D
                | Operation::MultiplyRotate3D
        ) && hotspot.reference_position.is_none()
        {
            return Err("this multiply hotspot needs a reference point".into());
        }
        let mut candidate = self.clone();
        let mut edits = candidate
            .occurrence_edits
            .get(occurrence)
            .cloned()
            .unwrap_or_default();
        if target.distance(hotspot.position) < 1.0e-9 {
            edits.hotspot_positions.remove(id);
        } else {
            edits.hotspot_positions.insert(id.into(), target);
        }
        if !candidate.set_occurrence_edits(occurrence.clone(), edits) {
            return Err("could not store hotspot target".into());
        }
        candidate
            .evaluate_hotspots(occurrence)?
            .geometry
            .validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn evaluated_hotspot_positions(
        &self,
        occurrence: &OccurrenceRef,
    ) -> Result<BTreeMap<String, Vec3>, String> {
        if let Some(result) = self.hotspot_cache.get(self, occurrence) {
            return Ok(result.positions.clone());
        }
        let definition = self
            .resolve_occurrence(occurrence)
            .ok_or("instance no longer exists")?
            .definition
            .id;
        Ok(self
            .hotspots(definition)?
            .into_iter()
            .map(|h| (h.id, h.position))
            .collect())
    }

    pub fn evaluate_hotspots(&self, occurrence: &OccurrenceRef) -> Result<Evaluation, String> {
        let resolved = self
            .resolve_occurrence(occurrence)
            .ok_or("instance no longer exists")?;
        let hotspots = self.hotspots(resolved.definition.id)?;
        let order = ordered(&hotspots)?;
        let mut geometry = resolved
            .edits
            .and_then(|e| e.geometry_replacement.as_ref())
            .unwrap_or(&resolved.definition.geometry)
            .clone();
        let mut positions = hotspots
            .iter()
            .map(|h| {
                (
                    h.id.clone(),
                    resolved
                        .edits
                        .and_then(|e| e.hotspot_positions.get(&h.id))
                        .copied()
                        .unwrap_or(h.position),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for index in order {
            let hotspot = &hotspots[index];
            let target = positions[&hotspot.id];
            if target.distance(hotspot.position) < 1.0e-9 {
                continue;
            }
            apply(&mut geometry, hotspot, target)?;
            for attached in &hotspot.attached_hotspot_ids {
                if let Some(position) = positions.get_mut(attached) {
                    *position = transform_point(hotspot, target, *position);
                }
            }
        }
        geometry.validate()?;
        Ok(Evaluation {
            geometry,
            positions,
        })
    }
}

fn ordered(hotspots: &[Hotspot]) -> Result<Vec<usize>, String> {
    fn visit(
        index: usize,
        hotspots: &[Hotspot],
        active: &mut BTreeSet<usize>,
        done: &mut BTreeSet<usize>,
        order: &mut Vec<usize>,
    ) -> Result<(), String> {
        if done.contains(&index) {
            return Ok(());
        }
        if !active.insert(index) {
            return Err("hotspot attachment cycle is not allowed".into());
        }
        // Dependencies run before their attached handles.
        for (parent, hotspot) in hotspots.iter().enumerate() {
            if hotspot.attached_hotspot_ids.contains(&hotspots[index].id) {
                visit(parent, hotspots, active, done, order)?;
            }
        }
        active.remove(&index);
        done.insert(index);
        order.push(index);
        Ok(())
    }
    let ids = hotspots
        .iter()
        .map(|h| h.id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != hotspots.len() {
        return Err("duplicate hotspot identity".into());
    }
    for h in hotspots {
        for attached in &h.attached_hotspot_ids {
            if !ids.contains(attached.as_str()) {
                return Err(format!("attached hotspot {attached} no longer exists"));
            }
        }
    }
    let mut order = Vec::new();
    let mut done = BTreeSet::new();
    let mut active = BTreeSet::new();
    for i in 0..hotspots.len() {
        visit(i, hotspots, &mut active, &mut done, &mut order)?;
    }
    Ok(order)
}

fn aliases_match(aliases: &[LegacyAlias], id: &str, native: String) -> bool {
    !id.is_empty() && (id == native || aliases.iter().any(|a| a.id == id))
}
impl SegmentAttachment {
    pub fn from_segment(geometry: &GeometryBundle, segment: &Segment) -> Self {
        Self {
            id: segment
                .legacy_aliases
                .first()
                .map(|a| a.id.clone())
                .unwrap_or_else(|| format!("segment-{}", segment.id.0)),
            a: VertexKey::at(geometry.vertex_position(segment.endpoints[0]).unwrap()),
            b: VertexKey::at(geometry.vertex_position(segment.endpoints[1]).unwrap()),
        }
    }
    fn matches(&self, geometry: &GeometryBundle, segment: &Segment) -> bool {
        if aliases_match(
            &segment.legacy_aliases,
            &self.id,
            format!("segment-{}", segment.id.0),
        ) {
            return true;
        }
        let keys = segment
            .endpoints
            .map(|p| geometry.vertex_position(p).map(VertexKey::at));
        keys == [Some(self.a), Some(self.b)] || keys == [Some(self.b), Some(self.a)]
    }
}
impl TriangleAttachment {
    pub fn from_triangle(geometry: &GeometryBundle, triangle: &Triangle) -> Self {
        let keys = triangle
            .vertices
            .map(|p| VertexKey::at(geometry.vertex_position(p).unwrap()));
        Self {
            id: triangle
                .legacy_aliases
                .first()
                .map(|a| a.id.clone())
                .unwrap_or_else(|| format!("triangle-{}", triangle.id.0)),
            a: keys[0],
            b: keys[1],
            c: keys[2],
        }
    }
    fn matches(&self, geometry: &GeometryBundle, triangle: &Triangle) -> bool {
        if aliases_match(
            &triangle.legacy_aliases,
            &self.id,
            format!("triangle-{}", triangle.id.0),
        ) {
            return true;
        }
        let keys = triangle
            .vertices
            .map(|p| geometry.vertex_position(p).map(VertexKey::at));
        [self.a, self.b, self.c]
            .iter()
            .all(|k| keys.contains(&Some(*k)))
    }
}
fn rotate(point: Vec3, axis: Vec3, angle: f64) -> Vec3 {
    point * angle.cos()
        + axis.cross(point) * angle.sin()
        + axis * (axis.dot(point) * (1. - angle.cos()))
}
fn rotation(from: Vec3, to: Vec3) -> Option<(Vec3, f64)> {
    let a = from.try_normalized(1.0e-9)?;
    let b = to.try_normalized(1.0e-9)?;
    let dot = a.dot(b).clamp(-1., 1.);
    let axis = a
        .cross(b)
        .try_normalized(1.0e-9)
        .or_else(|| a.cross(Vec3::Y).try_normalized(1.0e-9))
        .unwrap_or(Vec3::X);
    Some((axis, dot.acos()))
}
fn transform_point(h: &Hotspot, target: Vec3, point: Vec3) -> Vec3 {
    let reference = h.reference_position.unwrap_or(Vec3::ZERO);
    match h.operation {
        Operation::Move | Operation::Stretch => point + target - h.position,
        Operation::Scale => {
            let before = h.position.distance(reference);
            if before < 1.0e-9 {
                return point;
            }
            reference
                + (point - reference) * (target.distance(reference) / before).clamp(0.01, 100.)
        }
        Operation::Rotate => rotation(h.position - reference, target - reference)
            .map_or(point, |(axis, angle)| {
                reference + rotate(point - reference, axis, angle)
            }),
        _ => point,
    }
}
fn apply(geometry: &mut GeometryBundle, h: &Hotspot, target: Vec3) -> Result<(), String> {
    let segments = geometry
        .segments
        .values()
        .filter(|s| h.attached_segments.iter().any(|r| r.matches(geometry, s)))
        .cloned()
        .collect::<Vec<_>>();
    let triangles = geometry
        .triangles
        .values()
        .filter(|t| h.attached_triangles.iter().any(|r| r.matches(geometry, t)))
        .cloned()
        .collect::<Vec<_>>();
    let mut driven = BTreeSet::new();
    for s in &segments {
        driven.extend(s.endpoints.map(|v| v.id));
    }
    for t in &triangles {
        driven.extend(t.vertices.map(|v| v.id));
    }
    for vertex in geometry.vertices.values() {
        if h.attached_vertex_ids.iter().any(|id| {
            aliases_match(
                &vertex.legacy_aliases,
                id,
                format!("vertex-{}", vertex.id.0),
            )
        }) {
            driven.insert(vertex.id);
        }
    }
    if driven.is_empty() {
        driven.extend(
            geometry
                .vertices
                .values()
                .filter(|v| VertexKey::at(v.position) == VertexKey::at(h.position))
                .map(|v| v.id),
        );
    }
    if matches!(
        h.operation,
        Operation::Move | Operation::Scale | Operation::Stretch | Operation::Rotate
    ) {
        for id in driven {
            if let Some(vertex) = geometry.vertices.get_mut(&id) {
                vertex.position = transform_point(h, target, vertex.position);
            }
        }
        return Ok(());
    }
    if segments.is_empty() && triangles.is_empty() {
        return Ok(());
    }
    let reference = h
        .reference_position
        .ok_or("multiply hotspots require a reference point")?;
    let points = driven
        .iter()
        .filter_map(|id| geometry.vertices.get(id).map(|v| v.position))
        .collect::<Vec<_>>();
    let extent = |axis: Vec3| {
        let min = points
            .iter()
            .map(|p| p.dot(axis))
            .fold(f64::INFINITY, f64::min);
        let max = points
            .iter()
            .map(|p| p.dot(axis))
            .fold(f64::NEG_INFINITY, f64::max);
        (max - min).max(0.)
    };
    let mut transforms = Vec::new();
    let step = h.position - reference;
    let span = target - reference;
    const LIMIT: usize = 2048;
    match h.operation {
        Operation::MultiplyLinear => {
            if let Some(dir) = step.try_normalized(1.0e-9) {
                let n = (span.dot(dir) / step.length())
                    .floor()
                    .clamp(0., LIMIT as f64) as usize;
                for i in 1..=n {
                    transforms.push((Vec3::Y, 0., step * i as f64));
                }
            }
        }
        Operation::MultiplyVolumetric => {
            let count = |a: f64, b: f64| {
                if a.abs() > 1.0e-9 {
                    (b / a).floor().clamp(0., LIMIT as f64) as usize
                } else {
                    0
                }
            };
            'outer: for x in 0..=count(step.x, span.x) {
                for y in 0..=count(step.y, span.y) {
                    for z in 0..=count(step.z, span.z) {
                        if x == 0 && y == 0 && z == 0 {
                            continue;
                        }
                        transforms.push((
                            Vec3::Y,
                            0.,
                            Vec3::new(step.x * x as f64, step.y * y as f64, step.z * z as f64),
                        ));
                        if transforms.len() >= LIMIT {
                            break 'outer;
                        }
                    }
                }
            }
        }
        Operation::MultiplyRotate2D | Operation::MultiplyRotate3D => {
            let helix = h.operation == Operation::MultiplyRotate3D;
            let axis = if helix {
                Vec3::Y
            } else {
                step.cross(span).try_normalized(1.0e-9).unwrap_or(Vec3::Y)
            };
            let from = step - axis * step.dot(axis);
            let to = span - axis * span.dot(axis);
            if let (Some(a), Some(b)) = (from.try_normalized(1.0e-9), to.try_normalized(1.0e-9)) {
                let angle = axis.dot(a.cross(b)).atan2(a.dot(b));
                let radius = from.length().max(1.0e-4);
                let tangent = axis.cross(a);
                let height = if helix { span.y } else { 0. };
                let distance = if helix {
                    (angle * angle * radius * radius + height * height).sqrt()
                } else {
                    angle.abs() * radius
                };
                let spacing = if helix {
                    extent(tangent).max(extent(axis)).max(1.0e-3)
                } else {
                    extent(tangent).max(radius * 0.1).max(1.0e-3)
                };
                let n = (distance / spacing).floor().clamp(0., LIMIT as f64) as usize;
                for i in 1..=n {
                    let t = spacing * i as f64 / distance;
                    transforms.push((axis, angle * t, axis * height * t));
                }
            }
        }
        _ => {}
    }
    // Bounded per operation and per evaluation to prevent chained multiply explosions.
    let added = transforms
        .len()
        .saturating_mul(driven.len() + segments.len() + triangles.len());
    if geometry.vertices.len() + geometry.segments.len() + geometry.triangles.len() + added
        > 500_000
    {
        return Err("hotspot result exceeds the 500000 element evaluation budget".into());
    }
    let mut next_vertex = geometry.vertices.keys().map(|id| id.0).max().unwrap_or(0) + 1;
    let mut next_segment = geometry.segments.keys().map(|id| id.0).max().unwrap_or(0) + 1;
    let mut next_triangle = geometry.triangles.keys().map(|id| id.0).max().unwrap_or(0) + 1;
    let sources = driven
        .iter()
        .filter_map(|id| geometry.vertices.get(id).cloned())
        .collect::<Vec<_>>();
    for (axis, angle, offset) in transforms {
        let mut remap = BTreeMap::new();
        for source in &sources {
            let id = VertexId(next_vertex);
            next_vertex += 1;
            let position = reference + rotate(source.position - reference, axis, angle) + offset;
            geometry.vertices.insert(
                id,
                Vertex {
                    id,
                    position,
                    legacy_aliases: Vec::new(),
                },
            );
            remap.insert(source.id, id);
        }
        for source in &segments {
            let id = SegmentId(next_segment);
            next_segment += 1;
            let mut copy = source.clone();
            copy.id = id;
            copy.legacy_aliases.clear();
            for v in &mut copy.endpoints {
                v.id = remap[&v.id];
                v.position_offset = rotate(v.position_offset, axis, angle);
            }
            geometry.segments.insert(id, copy);
        }
        for source in &triangles {
            let id = TriangleId(next_triangle);
            next_triangle += 1;
            let mut copy = source.clone();
            copy.id = id;
            copy.legacy_aliases.clear();
            for v in &mut copy.vertices {
                v.id = remap[&v.id];
                v.position_offset = rotate(v.position_offset, axis, angle);
            }
            geometry.triangles.insert(id, copy);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::GeometryTolerance;
    use crate::operations::objects::{duplicate_occurrence, place_definition};
    use crate::selection::EditContext;
    use crate::transactions::TransactionHistory;

    fn fixture(operation: Operation) -> (Document, OccurrenceRef, Hotspot) {
        let mut d = Document::new();
        let instance = d
            .add_cuboid("Dynamic", Vec3::new(1., 2., 3.), Vec3::ZERO)
            .unwrap();
        let def = d.resolve_occurrence(&instance).unwrap().definition.id;
        let geometry = &d.definition(def).unwrap().geometry;
        let hotspot = Hotspot {
            id: "driver".into(),
            name: "Driver".into(),
            position: Vec3::X,
            operation,
            reference_position: Some(Vec3::ZERO),
            attached_segments: geometry
                .segments
                .values()
                .map(|s| SegmentAttachment::from_segment(geometry, s))
                .collect(),
            attached_triangles: geometry
                .triangles
                .values()
                .map(|t| TriangleAttachment::from_triangle(geometry, t))
                .collect(),
            ..Hotspot::default()
        };
        d.put_hotspot(def, hotspot.clone()).unwrap();
        (d, instance, hotspot)
    }
    #[test]
    fn targets_rebuild_from_prototype_copy_independently_and_survive_roundtrip_undo() {
        let (mut d, instance, _) = fixture(Operation::Move);
        let def = d.resolve_occurrence(&instance).unwrap().definition.id;
        let baseline = d.definition(def).unwrap().geometry.clone();
        let other = place_definition(
            &mut d,
            &EditContext::Document,
            def,
            Vec3::new(8., 0., 0.),
            None,
            false,
        )
        .unwrap();
        d.set_hotspot_target(&instance, "driver", Vec3::new(3., 0., 0.))
            .unwrap();
        let first = d.effective_geometry(&instance).unwrap().clone();
        d.set_hotspot_target(&instance, "driver", Vec3::new(4., 0., 0.))
            .unwrap();
        d.set_hotspot_target(&instance, "driver", Vec3::new(3., 0., 0.))
            .unwrap();
        assert_eq!(first, *d.effective_geometry(&instance).unwrap());
        assert_eq!(baseline, *d.effective_geometry(&other).unwrap());
        assert_eq!(baseline, d.definition(def).unwrap().geometry);
        let copy = duplicate_occurrence(&mut d, &instance, Vec3::new(4., 0., 0.)).unwrap();
        assert_eq!(first, *d.effective_geometry(&copy).unwrap());
        d.set_hotspot_target(&copy, "driver", Vec3::new(7., 0., 0.))
            .unwrap();
        assert_eq!(first, *d.effective_geometry(&instance).unwrap());
        let raw = serde_json::to_value(&d).unwrap();
        assert!(!raw.as_object().unwrap().contains_key("hotspot_cache"));
        let restored: Document = serde_json::from_value(raw).unwrap();
        assert_eq!(d, restored);
        assert_eq!(
            d.effective_geometry(&copy),
            restored.effective_geometry(&copy)
        );
        let mut history = TransactionHistory::default();
        let mut tx = history.begin(&d, "Drag hotspot");
        tx.stage(|d| d.set_hotspot_target(&instance, "driver", Vec3::new(9., 0., 0.)))
            .unwrap();
        history.commit(&mut d, tx).unwrap();
        assert_ne!(first, *d.effective_geometry(&instance).unwrap());
        history.undo(&mut d).unwrap();
        assert_eq!(first, *d.effective_geometry(&instance).unwrap());
    }
    #[test]
    fn stretch_moves_attached_vertices_and_shared_neighbors_not_opposite_end() {
        let (mut d, instance, mut h) = fixture(Operation::Stretch);
        let def = d.resolve_occurrence(&instance).unwrap().definition.id;
        let baseline = d.definition(def).unwrap().geometry.clone();
        let chosen = baseline.vertices.values().next().unwrap();
        h.attached_segments.clear();
        h.attached_triangles.clear();
        h.attached_vertex_ids
            .insert(format!("vertex-{}", chosen.id.0));
        d.put_hotspot(def, h).unwrap();
        d.set_hotspot_target(&instance, "driver", Vec3::new(2., 0., 0.))
            .unwrap();
        let actual = d.effective_geometry(&instance).unwrap();
        for vertex in baseline.vertices.values() {
            assert_eq!(
                vertex.position
                    + if vertex.id == chosen.id {
                        Vec3::X
                    } else {
                        Vec3::ZERO
                    },
                actual.vertices[&vertex.id].position
            );
        }
    }
    #[test]
    fn all_operations_generate_valid_deterministic_geometry_with_bounds() {
        for operation in Operation::ALL {
            let (mut d, instance, mut h) = fixture(operation);
            if operation == Operation::MultiplyRotate2D {
                h.reference_position = Some(Vec3::new(-5., 0., 0.));
                let def = d.resolve_occurrence(&instance).unwrap().definition.id;
                d.put_hotspot(def, h).unwrap();
            }
            let target = match operation {
                Operation::MultiplyRotate2D => Vec3::new(-5., 0., 10.),
                Operation::MultiplyRotate3D => Vec3::new(0., 8., 4.),
                Operation::MultiplyVolumetric => Vec3::new(4., 3., 3.),
                _ => Vec3::new(4., 1., 0.),
            };
            d.set_hotspot_target(&instance, "driver", target).unwrap();
            let a = d.evaluate_hotspots(&instance).unwrap();
            assert_eq!(a.geometry, d.evaluate_hotspots(&instance).unwrap().geometry);
            a.geometry.validate().unwrap();
            assert!(a.geometry.vertices.len() <= 500_000);
            assert_ne!(
                a.geometry,
                d.resolve_occurrence(&instance).unwrap().definition.geometry,
                "{}",
                operation.name()
            );
        }
    }
    #[test]
    fn cyclic_attachment_is_rejected_without_mutation_and_chain_is_applied_once() {
        let (mut d, instance, mut a) = fixture(Operation::Move);
        let def = d.resolve_occurrence(&instance).unwrap().definition.id;
        let mut b = Hotspot {
            id: "child".into(),
            position: Vec3::new(0., 2., 0.),
            ..Hotspot::default()
        };
        d.put_hotspot(def, b.clone()).unwrap();
        a.attached_hotspot_ids.insert(b.id.clone());
        d.put_hotspot(def, a).unwrap();
        d.set_hotspot_target(&instance, "driver", Vec3::new(3., 0., 0.))
            .unwrap();
        assert_eq!(
            Vec3::new(2., 2., 0.),
            d.evaluated_hotspot_positions(&instance).unwrap()["child"]
        );
        b.attached_hotspot_ids.insert("driver".into());
        let before = d.clone();
        assert!(d.put_hotspot(def, b).is_err());
        assert_eq!(before, d);
        assert!(
            d.resolve_occurrence(&instance)
                .unwrap()
                .world_transform
                .inverse(GeometryTolerance::default())
                .is_some()
        );
    }
}
