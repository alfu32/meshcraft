//! Bounded geometry interchange. Import never executes scripts or fetches network resources.
mod cad;
mod gltf;
mod text;
mod xml;

use meshcraft_core::document::{Document, OccurrenceRef, Rgba8};
use meshcraft_core::geometry::{Affine3, Vec3};
use meshcraft_core::selection::{SelectedEntity, SelectionSession};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_PRIMITIVES: usize = 2_000_000;
pub const EXTENSIONS: &[&str] = &[
    "obj", "stl", "stla", "stlb", "fbx", "gltf", "glb", "dae", "dxf", "3mf", "amf", "ifc",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Obj,
    Stl,
    StlAscii,
    StlBinary,
    Fbx,
    Gltf,
    Glb,
    Dae,
    Dxf,
    ThreeMf,
    Amf,
    Ifc,
}
impl Format {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "obj" => Ok(Self::Obj),
            "stl" => Ok(Self::Stl),
            "stla" => Ok(Self::StlAscii),
            "stlb" => Ok(Self::StlBinary),
            "fbx" => Ok(Self::Fbx),
            "gltf" => Ok(Self::Gltf),
            "glb" => Ok(Self::Glb),
            "dae" => Ok(Self::Dae),
            "dxf" => Ok(Self::Dxf),
            "3mf" => Ok(Self::ThreeMf),
            "amf" => Ok(Self::Amf),
            "ifc" => Ok(Self::Ifc),
            _ => Err(format!(
                "unsupported mesh extension; choose {}",
                EXTENSIONS.join(", ")
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub name: &'static str,
    pub meters: f64,
    pub dxf: u32,
}
impl Unit {
    pub const METER: Self = Self {
        name: "meter",
        meters: 1.,
        dxf: 6,
    };
    pub const MILLIMETER: Self = Self {
        name: "millimeter",
        meters: 0.001,
        dxf: 4,
    };
    pub fn parse(name: &str) -> Option<Self> {
        let (name, meters, dxf) = match name.to_ascii_lowercase().as_str() {
            "unitless" => ("unitless", 1., 0),
            "um" | "micron" | "microns" => ("micron", 1e-6, 13),
            "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => {
                ("millimeter", 0.001, 4)
            }
            "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => {
                ("centimeter", 0.01, 5)
            }
            "m" | "meter" | "meters" | "metre" | "metres" => ("meter", 1., 6),
            "in" | "inch" | "inches" => ("inch", 0.0254, 1),
            "ft" | "foot" | "feet" => ("foot", 0.3048, 2),
            _ => return None,
        };
        Some(Self { name, meters, dxf })
    }
    pub fn from_filename(path: &Path) -> Option<Self> {
        Self::parse(path.file_stem()?.to_str()?.rsplit(['_', '-', ' ']).next()?)
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Physical size of one internal coordinate; independent of the displayed unit label.
    pub meters_per_model_unit: f64,
    /// Explicit import/output unit. Otherwise use embedded metadata, filename suffix or legacy policy.
    pub unit: Option<Unit>,
    /// Additional dimensionless scale, applied exactly once.
    pub scale: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            meters_per_model_unit: 1.,
            unit: None,
            scale: 1.,
        }
    }
}
impl Options {
    fn validate(&self) -> Result<(), String> {
        if !self.meters_per_model_unit.is_finite()
            || self.meters_per_model_unit <= 0.
            || !self.scale.is_finite()
            || self.scale <= 0.
        {
            return Err("unit size and coordinate scale must be positive finite numbers".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub triangles: Vec<([Vec3; 3], Rgba8)>,
    pub segments: Vec<([Vec3; 2], Rgba8)>,
    pub warnings: Vec<String>,
    /// Stable world-space occurrence chunks, used for named IFC geometry proxies.
    pub parts: Vec<MeshPart>,
}
#[derive(Clone, Debug)]
pub struct MeshPart {
    pub name: String,
    pub triangles: std::ops::Range<usize>,
    pub segments: std::ops::Range<usize>,
}
impl Mesh {
    pub fn warn(&mut self, s: impl Into<String>) {
        let s = s.into();
        if self.warnings.len() < 32 && !self.warnings.contains(&s) {
            self.warnings.push(s);
        }
    }
    pub(crate) fn check_size(&self) -> Result<(), String> {
        if self.triangles.len() + self.segments.len() > MAX_PRIMITIVES {
            Err("mesh exceeds the two million primitive limit".into())
        } else {
            Ok(())
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.check_size()?;
        if !self.parts.is_empty() {
            let (mut t, mut s) = (0, 0);
            for part in &self.parts {
                if part.triangles.start != t
                    || part.segments.start != s
                    || part.triangles.end < t
                    || part.segments.end < s
                {
                    return Err("mesh chunks must form an ordered partition".into());
                }
                t = part.triangles.end;
                s = part.segments.end;
            }
            if t != self.triangles.len() || s != self.segments.len() {
                return Err("mesh chunk ranges do not cover the geometry".into());
            }
        }
        if self
            .triangles
            .iter()
            .flat_map(|(p, _)| p)
            .chain(self.segments.iter().flat_map(|(p, _)| p))
            .any(|p| !p.is_finite())
        {
            return Err("mesh contains non-finite coordinates".into());
        }
        Ok(())
    }
    pub fn transform(&mut self, t: Affine3) {
        for (p, _) in &mut self.triangles {
            *p = p.map(|p| t.transform_point(p));
            if t.determinant() < 0. {
                p.swap(1, 2);
            }
        }
        for (p, _) in &mut self.segments {
            *p = p.map(|p| t.transform_point(p));
        }
    }
    fn scale(&mut self, s: f64) {
        self.transform(Affine3 {
            linear: [[s, 0., 0.], [0., s, 0.], [0., 0., s]],
            translation: Vec3::ZERO,
        });
    }
    pub fn into_document(self) -> Result<Document, String> {
        use meshcraft_core::document::{
            RawGeometrySegment, RawGeometryTriangle, RawGeometryVertex, RawVertexRef,
        };
        self.validate()?;
        if self.triangles.is_empty() && self.segments.is_empty() {
            return Err("file contains no supported line or face geometry".into());
        }
        // Exact-coordinate indexing avoids both quadratic welding and tolerance-dependent geometry loss.
        let mut points = Vec::new();
        let mut index = BTreeMap::new();
        let mut vertex = |p: Vec3| {
            let key = [p.x, p.y, p.z].map(|v| if v == 0. { 0 } else { v.to_bits() });
            let i = *index.entry(key).or_insert_with(|| {
                let i = points.len();
                points.push(RawGeometryVertex {
                    position: p,
                    legacy_aliases: vec![],
                });
                i
            });
            RawVertexRef {
                vertex: i,
                position_offset: Vec3::ZERO,
            }
        };
        let segments = self
            .segments
            .into_iter()
            .map(|(p, color)| RawGeometrySegment {
                endpoints: p.map(&mut vertex),
                color,
                legacy_aliases: vec![],
                extensions: BTreeMap::new(),
            })
            .collect();
        let triangles = self
            .triangles
            .into_iter()
            .map(|(p, color)| RawGeometryTriangle {
                vertices: p.map(&mut vertex),
                color,
                legacy_aliases: vec![],
                extensions: BTreeMap::new(),
            })
            .collect();
        let mut doc = Document::new();
        doc.ensure_scene_root();
        doc.replace_definition_geometry_raw(
            Document::SCENE_ROOT_DEFINITION_ID,
            points,
            segments,
            triangles,
        )?;
        doc.validate()?;
        Ok(doc)
    }
    pub fn from_document(
        doc: &Document,
        selection: Option<&SelectionSession>,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let mut out = Self::default();
        let mut explicit = BTreeMap::<OccurrenceRef, BTreeSet<SelectedEntity>>::new();
        let mut objects = BTreeSet::<Vec<meshcraft_core::document::LayoutEntryId>>::new();
        let mut owners = BTreeMap::<OccurrenceRef, BTreeSet<String>>::new();
        if let Some(selection) = selection {
            for target in selection.selected() {
                if target.entity == SelectedEntity::Occurrence {
                    objects.insert(target.occurrence.path.clone());
                }
                if let SelectedEntity::SemanticHandle { id, .. } = &target.entity {
                    owners
                        .entry(target.occurrence.clone())
                        .or_default()
                        .insert(id.clone());
                }
                explicit
                    .entry(target.occurrence.clone())
                    .or_default()
                    .insert(target.entity.clone());
            }
        }
        let whole = |path: &OccurrenceRef| {
            selection.is_none()
                || (0..=path.path.len()).any(|depth| objects.contains(&path.path[..depth]))
        };
        let selected =
            |path: &OccurrenceRef,
             entity: &SelectedEntity,
             extensions: Option<&BTreeMap<String, serde_json::Value>>| {
                whole(path)
                    || explicit
                        .get(path)
                        .is_some_and(|entities| entities.contains(entity))
                    || extensions
                        .and_then(|e| e.get(meshcraft_core::document::architecture::GENERATED))
                        .and_then(|v| v.get("id"))
                        .and_then(|v| v.as_str())
                        .is_some_and(|id| {
                            owners.get(path).is_some_and(|owners| owners.contains(id))
                        })
            };
        for path in doc.scene_geometry_refs()? {
            check(cancel)?;
            let r = doc
                .resolve_occurrence(&path)
                .ok_or("export occurrence is missing")?;
            let Some(g) = selection.map_or_else(
                || doc.effective_geometry(&path),
                |s| doc.geometry_in_context(&path, &s.context),
            ) else {
                continue;
            };
            let start_triangles = out.triangles.len();
            let start_segments = out.segments.len();
            let p = |v| {
                g.vertex_position(v)
                    .map(|p| r.world_transform.transform_point(p))
                    .ok_or("invalid mesh reference")
            };
            for t in g.triangles.values() {
                check(cancel)?;
                if selected(&path, &SelectedEntity::Triangle(t.id), Some(&t.extensions)) {
                    let mut points = [p(t.vertices[0])?, p(t.vertices[1])?, p(t.vertices[2])?];
                    if r.world_transform.determinant() < 0. {
                        points.swap(1, 2);
                    }
                    out.triangles.push((points, t.color));
                }
            }
            for s in g.segments.values() {
                check(cancel)?;
                if selected(&path, &SelectedEntity::Segment(s.id), Some(&s.extensions)) {
                    out.segments
                        .push(([p(s.endpoints[0])?, p(s.endpoints[1])?], s.color));
                }
            }
            if out.triangles.len() > start_triangles || out.segments.len() > start_segments {
                out.parts.push(MeshPart {
                    name: r.definition.name.clone(),
                    triangles: start_triangles..out.triangles.len(),
                    segments: start_segments..out.segments.len(),
                });
            }
            out.check_size()?;
        }
        for a in doc.annotations() {
            if selected(&a.target.occurrence, &a.target.entity, None) {
                let start = out.segments.len();
                out.segments
                    .extend(a.edges.into_iter().map(|p| (p, Rgba8::WHITE)));
                if out.segments.len() > start {
                    out.parts.push(MeshPart {
                        name: a.label.clone(),
                        triangles: out.triangles.len()..out.triangles.len(),
                        segments: start..out.segments.len(),
                    });
                }
                out.warn(
                    "Annotation labels are omitted; dimension and leader linework is exported.",
                );
            }
        }
        out.warn("Interchange bakes the current geometry; prototypes, scripts, hotspots and editable architectural parameters are not preserved. Use .octd to retain them.");
        if doc.materials().iter().any(|m| m.texture_path.is_some()) {
            out.warn("Textures and shading models are omitted; supported formats retain base colors only.");
        }
        out.validate()?;
        Ok(out)
    }
}

pub(crate) fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("interchange canceled".into())
    } else {
        Ok(())
    }
}
pub(crate) fn utf8(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes)
        .map(|s| s.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("invalid UTF-8 text: {e}"))
}
pub(crate) fn number(s: &str) -> Result<f64, String> {
    let n = s
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid number: {s}"))?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err("non-finite coordinate".into())
    }
}
pub(crate) fn numbers(s: &str) -> Result<Vec<f64>, String> {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(number)
        .collect()
}
pub(crate) fn color(v: [f64; 4]) -> Rgba8 {
    let [r, g, b, a] = v.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
    Rgba8::new(r, g, b, a)
}
pub(crate) fn normal(p: [Vec3; 3]) -> Vec3 {
    (p[1] - p[0])
        .cross(p[2] - p[0])
        .try_normalized(1e-20)
        .unwrap_or(Vec3::ZERO)
}

/// Ear clipping retains concave polygon boundaries; large or degenerate faces fail explicitly.
pub(crate) fn polygon(out: &mut Mesh, p: &[Vec3], color: Rgba8) -> Result<(), String> {
    if p.len() < 3 || p.len() > 4096 {
        return Err("polygon needs 3..4096 vertices".into());
    }
    if p.iter().any(|v| !v.is_finite()) {
        return Err("non-finite polygon".into());
    }
    if p.len() == 3 {
        out.triangles.push(([p[0], p[1], p[2]], color));
        return Ok(());
    }
    let local = p.iter().map(|point| *point - p[0]).collect::<Vec<_>>();
    let n = (0..p.len()).fold(Vec3::ZERO, |n, i| {
        n + local[i].cross(local[(i + 1) % p.len()])
    });
    let drop = if n.x.abs() > n.y.abs() && n.x.abs() > n.z.abs() {
        0
    } else if n.y.abs() > n.z.abs() {
        1
    } else {
        2
    };
    let mut q = local
        .iter()
        .map(|p| match drop {
            0 => [p.y, p.z],
            1 => [p.z, p.x],
            _ => [p.x, p.y],
        })
        .collect::<Vec<_>>();
    let extent = q.iter().flatten().fold(0.0f64, |a, b| a.max(b.abs()));
    if !extent.is_finite() || extent == 0. {
        return Err("degenerate or out-of-range polygon".into());
    }
    for p in &mut q {
        p[0] /= extent;
        p[1] /= extent;
    }
    let cross = |a: usize, b: usize, c: usize| {
        (q[b][0] - q[a][0]) * (q[c][1] - q[a][1]) - (q[b][1] - q[a][1]) * (q[c][0] - q[a][0])
    };
    let area = (0..p.len())
        .map(|i| q[i][0] * q[(i + 1) % p.len()][1] - q[(i + 1) % p.len()][0] * q[i][1])
        .sum::<f64>();
    if area.abs() < 1e-20 {
        return Err("degenerate polygon".into());
    }
    let sign = area.signum();
    let mut ids = (0..p.len()).collect::<Vec<_>>();
    while ids.len() > 3 {
        let ear = (0..ids.len())
            .find(|&i| {
                let a = ids[(i + ids.len() - 1) % ids.len()];
                let b = ids[i];
                let c = ids[(i + 1) % ids.len()];
                cross(a, b, c) * sign > 1e-16
                    && !ids.iter().any(|&v| {
                        v != a
                            && v != b
                            && v != c
                            && cross(a, b, v) * sign >= -1e-16
                            && cross(b, c, v) * sign >= -1e-16
                            && cross(c, a, v) * sign >= -1e-16
                    })
            })
            .ok_or("polygon is self-intersecting or degenerate")?;
        let i = ear;
        out.triangles.push((
            [
                p[ids[(i + ids.len() - 1) % ids.len()]],
                p[ids[i]],
                p[ids[(i + 1) % ids.len()]],
            ],
            color,
        ));
        ids.remove(i);
    }
    out.triangles
        .push(([p[ids[0]], p[ids[1]], p[ids[2]]], color));
    Ok(())
}

/// Resource loader is local-only; caller owns path containment and byte limits.
pub fn decode(
    bytes: &[u8],
    format: Format,
    options: &Options,
    resource: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    cancel: &AtomicBool,
) -> Result<Mesh, String> {
    options.validate()?;
    check(cancel)?;
    if matches!(format, Format::Gltf | Format::Glb) && options.unit.is_some_and(|u| u.meters != 1.)
    {
        return Err("glTF coordinates are meters; omit the import unit or select meter".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("mesh exceeds the 512 MiB input limit".into());
    }
    let mut mesh = match format {
        Format::Obj => text::read_obj(utf8(bytes)?, resource, cancel)?,
        Format::Stl | Format::StlAscii | Format::StlBinary => {
            text::read_stl(bytes, format, cancel)?
        }
        Format::Dxf => cad::read_dxf(utf8(bytes)?, options, cancel)?,
        Format::Fbx => cad::read_fbx(bytes, cancel)?,
        Format::Gltf | Format::Glb => gltf::read(bytes, resource, cancel)?,
        Format::Dae | Format::ThreeMf | Format::Amf => xml::read(bytes, format, options, cancel)?,
        Format::Ifc => cad::read_ifc(utf8(bytes)?, options, cancel)?,
    };
    let scale = if format == Format::ThreeMf {
        1.
    } else if matches!(format, Format::Gltf | Format::Glb) && options.unit.is_none() {
        1. / options.meters_per_model_unit
    } else {
        options
            .unit
            .map_or(1., |u| u.meters / options.meters_per_model_unit)
    };
    mesh.scale(scale * options.scale);
    mesh.validate()?;
    check(cancel)?;
    if mesh.triangles.is_empty() && mesh.segments.is_empty() {
        return Err("file contains no supported line or face geometry".into());
    }
    Ok(mesh)
}

pub fn encode(
    mesh: &Mesh,
    format: Format,
    options: &Options,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<String>), String> {
    options.validate()?;
    check(cancel)?;
    mesh.validate()?;
    let mut mesh = mesh.clone();
    let unit = options.unit.unwrap_or(if format == Format::ThreeMf {
        Unit::MILLIMETER
    } else {
        Unit::METER
    });
    // glTF coordinates are always meters; never apply a filename unit suffix to glTF.
    if matches!(format, Format::Gltf | Format::Glb) && options.unit.is_some_and(|u| u.meters != 1.)
    {
        return Err("glTF requires meter coordinates; omit the output unit or select meter".into());
    }
    mesh.scale(options.meters_per_model_unit / unit.meters * options.scale);
    mesh.validate()?;
    if mesh.triangles.is_empty() && mesh.segments.is_empty() {
        return Err("nothing to export".into());
    }
    if matches!(
        format,
        Format::Stl
            | Format::StlAscii
            | Format::StlBinary
            | Format::Fbx
            | Format::ThreeMf
            | Format::Amf
    ) && !mesh.segments.is_empty()
    {
        if mesh.triangles.is_empty() {
            return Err(
                "this format cannot store a line-only model; use OBJ, DXF, glTF, DAE or IFC".into(),
            );
        }
        mesh.warn("This format omits line segments.");
    }
    let bytes = match format {
        Format::Obj => text::write_obj(&mesh, cancel)?,
        Format::Stl | Format::StlAscii | Format::StlBinary => {
            text::write_stl(&mesh, format, cancel)?
        }
        Format::Dxf => cad::write_dxf(&mesh, unit, cancel)?,
        Format::Fbx => cad::write_fbx(&mesh, cancel)?,
        Format::Gltf | Format::Glb => gltf::write(&mesh, format, cancel)?,
        Format::Dae | Format::ThreeMf | Format::Amf => xml::write(&mesh, format, unit, cancel)?,
        Format::Ifc => cad::write_ifc(&mesh, unit, cancel)?,
    };
    if matches!(format, Format::Obj | Format::Dxf)
        && mesh
            .triangles
            .iter()
            .map(|(_, c)| c)
            .chain(mesh.segments.iter().map(|(_, c)| c))
            .any(|c| c.alpha != 255)
    {
        mesh.warn("This exporter does not retain material transparency.");
    }
    if matches!(
        format,
        Format::Stl
            | Format::StlAscii
            | Format::StlBinary
            | Format::Fbx
            | Format::Dae
            | Format::Amf
            | Format::ThreeMf
            | Format::Ifc
    ) && mesh.triangles.iter().any(|(_, c)| *c != Rgba8::WHITE)
    {
        mesh.warn("Base colors are not retained by this exporter.");
    }
    check(cancel)?;
    Ok((bytes, mesh.warnings))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_file(path: &Path, options: &Options, cancel: &AtomicBool) -> Result<Mesh, String> {
    use std::io::Read;
    let base = path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let load = |path: &Path| -> Result<Vec<u8>, String> {
        check(cancel)?;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES as u64 {
            return Err("mesh resource exceeds 512 MiB".into());
        }
        let mut b = vec![];
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut b)
            .map_err(|e| e.to_string())?;
        if b.len() > MAX_BYTES {
            return Err("mesh resource exceeds 512 MiB".into());
        }
        Ok(b)
    };
    let bytes = load(path)?;
    let mut total = bytes.len();
    let mut resource = |uri: &str| {
        if uri.contains(':') || uri.contains('\\') || uri.contains('%') {
            return Err("only plain relative local resource paths are supported".into());
        }
        let resolved = base
            .join(uri)
            .canonicalize()
            .map_err(|e| format!("resource {uri}: {e}"))?;
        if !resolved.starts_with(&base) {
            return Err("mesh resource escapes its source directory".into());
        }
        let bytes = load(&resolved)?;
        total += bytes.len();
        if total > MAX_BYTES {
            return Err("combined mesh resources exceed 512 MiB".into());
        }
        Ok(bytes)
    };
    let mut options = options.clone();
    let format = Format::from_path(path)?;
    if options.unit.is_none() && !matches!(format, Format::ThreeMf | Format::Gltf | Format::Glb) {
        options.unit = Unit::from_filename(path);
    }
    decode(&bytes, format, &options, &mut resource, cancel)
}

#[cfg(test)]
mod tests;
