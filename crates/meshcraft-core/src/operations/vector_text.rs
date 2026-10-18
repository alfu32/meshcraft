use crate::document::{
    Document, GeometryBundle, LegacyAlias, Rgba8, Segment, SegmentId, Vertex, VertexId, VertexRef,
};
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

// Compact strokes from the pinned legacy alphabet asset; duplicate byte glyphs keep the richer form.
const EMBEDDED_GLYPHS: &str = include_str!("../../assets/vector_glyphs_alphabet.json");
const GLYPH_VERTEX_EPSILON: f64 = 1.0e-3;

#[derive(Clone, Debug, Deserialize)]
struct GlyphRecord {
    c: u32,
    #[serde(rename = "s")]
    #[serde(default)]
    segments: Vec<[[f64; 3]; 2]>,
}

#[derive(Clone, Debug)]
struct Glyph {
    segments: Vec<[[f64; 3]; 2]>,
    min_x: f64,
    max_x: f64,
    min_z: f64,
    max_z: f64,
}

impl Glyph {
    fn from_record(record: GlyphRecord) -> Option<Self> {
        if record.segments.is_empty() {
            return None;
        }
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_z = f64::INFINITY;
        let mut max_z = f64::NEG_INFINITY;
        for point in record.segments.iter().flatten() {
            min_x = min_x.min(point[0]);
            max_x = max_x.max(point[0]);
            min_z = min_z.min(point[2]);
            max_z = max_z.max(point[2]);
        }
        [min_x, max_x, min_z, max_z]
            .into_iter()
            .all(f64::is_finite)
            .then_some(Self {
                segments: record.segments,
                min_x,
                max_x,
                min_z,
                max_z,
            })
    }

    fn width(&self) -> f64 {
        (self.max_x - self.min_x).max(0.0)
    }

    fn height(&self) -> f64 {
        (self.max_z - self.min_z).max(0.0)
    }
}

fn embedded_glyphs() -> &'static HashMap<u32, Glyph> {
    static GLYPHS: OnceLock<HashMap<u32, Glyph>> = OnceLock::new();
    GLYPHS.get_or_init(|| {
        let records: Vec<GlyphRecord> = serde_json::from_str(EMBEDDED_GLYPHS)
            .expect("the embedded alphabet glyph catalog is valid JSON");
        records
            .into_iter()
            .filter_map(|record| {
                let code = record.c;
                Glyph::from_record(record).map(|glyph| (code, glyph))
            })
            .collect()
    })
}

/// Adds the embedded legacy alphabet's vector strokes to the exploded geometry.
pub(crate) fn append_vector_text(
    document: &mut Document,
    geometry: &mut GeometryBundle,
    record: &serde_json::Value,
    transform_to_parent: Affine3,
) -> Result<(), String> {
    let text = record
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let glyph_source = record
        .get("glyphSourcePath")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("embedded:alphabet")
        .trim();
    if !glyph_source.eq_ignore_ascii_case("embedded:alphabet") {
        return Err(format!(
            "vector text glyph source {glyph_source:?} is unavailable to the explode adapter"
        ));
    }

    let position = vector_field(record, "position", Vec3::ZERO)?;
    let normal = vector_field(record, "normal", Vec3::ZERO)?;
    let axis_u = vector_field(record, "axisU", Vec3::ZERO)?;
    let Some(axis_w) = axis_u
        .cross(normal)
        .try_normalized(GeometryTolerance::default().absolute)
    else {
        return Err("vector text has a degenerate local basis".to_owned());
    };
    let size = record
        .get("size")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.1);
    let tracking = record
        .get("tracking")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.1)
        .max(0.0);
    let line_spacing = record
        .get("lineSpacing")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.25)
        .max(0.1);
    if !size.is_finite() || !tracking.is_finite() || !line_spacing.is_finite() {
        return Err("vector text sizing values must be finite".to_owned());
    }
    let target_height = size.max(1.0e-3);
    let line_step = line_spacing * target_height;
    let space_advance = target_height * 0.6;
    let glyphs = embedded_glyphs();
    let fallback = fallback_square_glyph();
    let mut pen = 0.0;
    let mut line_offset = 0.0;

    for character in text.chars() {
        match character {
            '\n' => {
                pen = 0.0;
                line_offset += line_step;
                continue;
            }
            '\r' => continue,
            ' ' | '\t' => {
                pen += space_advance;
                continue;
            }
            _ => {}
        }
        let glyph = glyphs.get(&(character as u32)).unwrap_or(&fallback);
        let glyph_height = glyph.height();
        if glyph_height <= 1.0e-6 {
            pen += space_advance;
            continue;
        }
        let scale = target_height / glyph_height;
        let glyph_origin = position
            + axis_w * -line_offset
            + axis_u * (pen - glyph.min_x * scale)
            + axis_w * (-glyph.min_z * scale);
        let world_u = axis_u * scale;
        let world_v = normal * scale;
        let world_w = axis_w * scale;
        let mut vertices = BTreeMap::new();
        for segment in &glyph.segments {
            let mut endpoints = [VertexRef::new(VertexId(0)); 2];
            for (endpoint_index, point) in segment.iter().enumerate() {
                let local = Vec3::new(point[0], point[1], point[2]);
                let glyph_point =
                    glyph_origin + world_u * local.x + world_v * local.y + world_w * local.z;
                let parent_point = transform_to_parent.transform_point(glyph_point);
                if !parent_point.is_finite() {
                    return Err("vector text produced non-finite geometry".to_owned());
                }
                let key = quantized_glyph_point(local)?;
                let id = if let Some(id) = vertices.get(&key) {
                    *id
                } else {
                    let id = VertexId(document.allocate_id());
                    geometry.vertices.insert(
                        id,
                        Vertex {
                            id,
                            position: parent_point,
                            legacy_aliases: Vec::new(),
                        },
                    );
                    vertices.insert(key, id);
                    id
                };
                let base = geometry
                    .vertices
                    .get(&id)
                    .expect("glyph vertex was inserted above")
                    .position;
                endpoints[endpoint_index] = VertexRef {
                    id,
                    position_offset: parent_point - base,
                };
            }
            let id = SegmentId(document.allocate_id());
            geometry.segments.insert(
                id,
                Segment {
                    id,
                    endpoints,
                    color: Rgba8::WHITE,
                    legacy_aliases: Vec::<LegacyAlias>::new(),
                    extensions: BTreeMap::new(),
                },
            );
        }
        pen += glyph.width() * scale + tracking * target_height;
    }
    Ok(())
}

fn vector_field(record: &serde_json::Value, name: &str, default: Vec3) -> Result<Vec3, String> {
    let Some(value) = record.get(name) else {
        return Ok(default);
    };
    let vector: Vec3 = serde_json::from_value(value.clone())
        .map_err(|_| format!("vector text field {name:?} is not a 3D vector"))?;
    vector
        .is_finite()
        .then_some(vector)
        .ok_or_else(|| format!("vector text field {name:?} is non-finite"))
}

fn quantized_glyph_point(point: Vec3) -> Result<[i64; 3], String> {
    let mut quantized = [0_i64; 3];
    for (index, value) in [point.x, point.y, point.z].into_iter().enumerate() {
        let scaled = (value / GLYPH_VERTEX_EPSILON).round();
        if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
            return Err("vector glyph coordinate exceeds the supported range".to_owned());
        }
        quantized[index] = scaled as i64;
    }
    Ok(quantized)
}

fn fallback_square_glyph() -> Glyph {
    Glyph {
        segments: vec![
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            [[1.0, 0.0, 0.0], [1.0, 0.0, 1.0]],
            [[1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            [[0.0, 0.0, 1.0], [0.0, 0.0, 0.0]],
        ],
        min_x: 0.0,
        max_x: 1.0,
        min_z: 0.0,
        max_z: 1.0,
    }
}
