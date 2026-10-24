//! Shared, backend-independent procedural programs and typed parameter values (#29/#30).
//! Only definitions/settings are authored; generated geometry never enters the mesh graph.
use super::*;

mod objects;
pub use objects::*;
mod presets;
pub use presets::*;
pub const PROGRAMS_KEY: &str = "k3d.meshcraftProceduralPrograms";
pub const OVERRIDES_KEY: &str = "k3d.meshcraftProceduralParameters";
pub const GRASS: &str = "meshcraft.grass.v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub id: String,
    pub kind: String,
    pub default: Value,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}
impl Parameter {
    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        let vector = |n| {
            value.as_array().is_some_and(|a| {
                a.len() == n
                    && a.iter().all(|v| {
                        v.as_f64()
                            .is_some_and(|n| n.is_finite() && n.abs() <= f32::MAX as f64)
                    })
            })
        };
        let valid = match self.kind.as_str() {
            "float" => value
                .as_f64()
                .is_some_and(|n| n.is_finite() && n.abs() <= f32::MAX as f64),
            "integer" => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
            "unsigned" | "seed" => value.as_u64().is_some_and(|n| n <= u32::MAX as u64),
            "boolean" => value.is_boolean(),
            "vec2" => vector(2),
            "vec3" => vector(3),
            "vec4" | "color" => vector(4),
            "matrix" => vector(16),
            "texture" => value.is_string(),
            "enum" => value.as_str().is_some_and(|v| self.options.contains_key(v)),
            // Unknown types belong to unsupported programs and must round-trip.
            _ => true,
        };
        if self.kind == "color"
            && value.as_array().is_some_and(|a| {
                a.iter()
                    .any(|v| v.as_f64().is_some_and(|n| !(0.0..=1.0).contains(&n)))
            })
        {
            return Err(format!(
                "{} color channels must be between zero and one",
                self.id
            ));
        }
        if !valid {
            return Err(format!("{} must be {}", self.id, self.kind));
        }
        for n in value.as_f64().into_iter().chain(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64),
        ) {
            if self.min.is_some_and(|m| n < m) || self.max.is_some_and(|m| n > m) {
                return Err(format!("{} is outside its allowed range", self.id));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Program {
    pub id: String,
    pub name: String,
    /// Surface shading, GPU-subdivided surface_grid, distributed blades/foliage, or an independent tree.
    pub generation: String,
    pub backend: String,
    pub parameters: Vec<Parameter>,
    /// GLSL functions proceduralVertex(...) and proceduralColor(...), not graphics API calls.
    pub vertex: String,
    pub fragment: String,
    pub bounds_expansion: f64,
    pub animated: bool,
    pub transparent: bool,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}
impl Default for Program {
    fn default() -> Self {
        Self {id:String::new(),name:String::new(),generation:"surface".into(),backend:"vertex_instancing_gl33".into(),parameters:vec![],vertex:"void proceduralVertex(inout vec3 position, inout vec3 normal, vec2 uv) {}".into(),fragment:"vec4 proceduralColor(vec4 color, vec3 position, vec3 normal, vec2 uv) { return color; }".into(),bounds_expansion:0.,animated:false,transparent:false,extensions:BTreeMap::new()}
    }
}
impl Program {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || !self.bounds_expansion.is_finite()
            || self.bounds_expansion < 0.
        {
            return Err(
                "Procedural program needs an ID, name and nonnegative bounds expansion".into(),
            );
        }
        if self.vertex.len() + self.fragment.len() > 128 * 1024 {
            return Err("Procedural shader source exceeds 128 KiB".into());
        }
        if self
            .parameters
            .iter()
            .filter(|p| p.kind == "texture")
            .count()
            > 8
        {
            return Err("At most eight texture inputs are supported per program".into());
        }
        let mut ids = BTreeSet::new();
        for p in &self.parameters {
            if p.id.is_empty()
                || p.id.len() > 64
                || p.id.starts_with("gl_")
                || !p.id.bytes().enumerate().all(|(i, c)| {
                    c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                })
                || !ids.insert(&p.id)
            {
                return Err("Parameter identifiers must be unique shader identifiers".into());
            }
            if p.min
                .into_iter()
                .chain(p.max)
                .chain(p.step)
                .any(|v| !v.is_finite())
                || p.min.zip(p.max).is_some_and(|(a, b)| a > b)
                || p.step.is_some_and(|v| v <= 0.)
            {
                return Err(format!("Invalid range for {}", p.id));
            }
            p.validate_value(&p.default)?;
        }
        Ok(())
    }
    pub fn resolve(
        &self,
        configured: &BTreeMap<String, Value>,
        overrides: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let mut out = self
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.default.clone()))
            .collect::<BTreeMap<_, _>>();
        for (k, v) in configured.iter().chain(overrides) {
            let p = self
                .parameters
                .iter()
                .find(|p| &p.id == k)
                .ok_or_else(|| format!("Unknown parameter {k}"))?;
            p.validate_value(v)?;
            out.insert(k.clone(), v.clone());
        }
        Ok(out)
    }
    pub fn supported(&self) -> bool {
        self.backend == "vertex_instancing_gl33"
            && matches!(
                self.generation.as_str(),
                "surface" | "blades" | "foliage" | "surface_grid" | "tree"
            )
            && self.parameters.iter().all(|p| {
                matches!(
                    p.kind.as_str(),
                    "float"
                        | "integer"
                        | "unsigned"
                        | "seed"
                        | "boolean"
                        | "vec2"
                        | "vec3"
                        | "vec4"
                        | "color"
                        | "matrix"
                        | "texture"
                        | "enum"
                )
            })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProceduralMaterial {
    pub program: String,
    pub parameters: BTreeMap<String, Value>,
    pub base_material: Option<String>,
    pub lod_distance: f64,
    pub max_instances: u32,
    pub show_bounds: bool,
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, Value>,
}
impl Default for ProceduralMaterial {
    fn default() -> Self {
        Self {
            program: GRASS.into(),
            parameters: BTreeMap::new(),
            base_material: None,
            lod_distance: 100.,
            max_instances: 50_000,
            show_bounds: false,
            extensions: BTreeMap::new(),
        }
    }
}
impl ProceduralMaterial {
    pub fn validate(&self) -> Result<(), String> {
        if self.program.is_empty()
            || !self.lod_distance.is_finite()
            || self.lod_distance <= 0.
            || self.max_instances == 0
            || self.max_instances > 1_000_000
        {
            return Err("Procedural material needs a program, positive LOD distance and 1–1000000 instance budget".into());
        }
        Ok(())
    }
}

pub fn grass_program() -> Program {
    let scalar =
        |id: &str, label: &str, default: f64, min: f64, max: f64, description: &str| Parameter {
            id: id.into(),
            kind: "float".into(),
            default: Value::from(default),
            label: label.into(),
            description: description.into(),
            min: Some(min),
            max: Some(max),
            unit: String::new(),
            step: None,
            options: BTreeMap::new(),
            extensions: BTreeMap::new(),
        };
    let mut parameters = vec![
        scalar(
            "density",
            "Density",
            30.,
            0.,
            10000.,
            "Blades per square model unit; LOD and the instance budget bound rendering work.",
        ),
        scalar(
            "height",
            "Blade height",
            0.8,
            0.001,
            1000.,
            "Blade length in local model units.",
        ),
        scalar(
            "width",
            "Blade width",
            0.06,
            0.001,
            100.,
            "Base width in local model units.",
        ),
        scalar(
            "curvature",
            "Curvature",
            0.3,
            -2.,
            2.,
            "Tip bend relative to height.",
        ),
        scalar(
            "wind",
            "Wind strength",
            0.15,
            0.,
            2.,
            "Animated tip displacement relative to height; zero pauses wind.",
        ),
        scalar(
            "variation",
            "Color variation",
            0.25,
            0.,
            1.,
            "Seeded variation in blade brightness.",
        ),
    ];
    parameters.push(Parameter {
        id: "seed".into(),
        kind: "seed".into(),
        default: Value::from(1u32),
        label: "Seed".into(),
        description: "Reproducible distribution; shared by repeated instances.".into(),
        unit: String::new(),
        min: None,
        max: None,
        step: None,
        options: BTreeMap::new(),
        extensions: BTreeMap::new(),
    });
    Program {
        id: GRASS.into(),
        name: "Grass".into(),
        generation: "blades".into(),
        parameters,
        animated: true,
        ..Default::default()
    }
}
impl Document {
    pub fn procedural_programs(&self) -> Vec<Program> {
        let mut programs = builtin_programs();
        if let Some(values) = self.extensions.get(PROGRAMS_KEY).and_then(Value::as_array) {
            programs.extend(
                values
                    .iter()
                    .filter_map(|v| serde_json::from_value::<Program>(v.clone()).ok()),
            );
        }
        programs
    }
    pub fn put_procedural_program(&mut self, program: Program) -> Result<(), String> {
        program.validate()?;
        if is_builtin(&program.id) {
            return Err("Built-in programs are read-only; save under a new ID".into());
        }
        let mut candidate = self.clone();
        let mut programs = candidate
            .procedural_programs()
            .into_iter()
            .filter(|p| !is_builtin(&p.id) && p.id != program.id)
            .collect::<Vec<_>>();
        programs.push(program);
        candidate.extensions.insert(
            PROGRAMS_KEY.into(),
            serde_json::to_value(programs).map_err(|e| e.to_string())?,
        );
        candidate.validate_procedural_materials()?;
        candidate.validate_procedural_objects()?;
        candidate.bump_revision();
        *self = candidate;
        Ok(())
    }
    pub(super) fn validate_procedural_materials(&self) -> Result<(), String> {
        let programs = self.procedural_programs();
        let mut ids = BTreeSet::new();
        for p in &programs {
            p.validate()?;
            if !ids.insert(&p.id) {
                return Err("Duplicate procedural program ID".into());
            }
        }
        for m in self.materials() {
            if let Some(p) = &m.procedural {
                p.validate()?;
                if let Some(program) = programs.iter().find(|v| v.id == p.program) {
                    program.resolve(&p.parameters, &BTreeMap::new())?;
                }
                if let Some(base) = &p.base_material
                    && (base == &m.id
                        || !self
                            .materials()
                            .iter()
                            .any(|m| &m.id == base && m.procedural.is_none()))
                {
                    return Err(
                        "Base shading material must reference a conventional material".into(),
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        operations::transforms::{assign_material, assign_procedural_material},
        selection::{SelectedEntity, SelectionSession, SelectionTarget},
    };
    use serde_json::json;
    fn material() -> super::super::materials::Material {
        super::super::materials::Material {
            id: "grass".into(),
            name: "Grass".into(),
            color: Rgba8::new(60, 120, 30, 255),
            procedural: Some(ProceduralMaterial::default()),
            ..Default::default()
        }
    }
    #[test]
    fn parameters_validate_types_ranges_and_three_level_resolution() {
        let p = grass_program();
        p.validate().unwrap();
        let configured = BTreeMap::from([("height".into(), json!(2.0))]);
        let overrides = BTreeMap::from([
            ("height".into(), json!(3.0)),
            ("seed".into(), json!(u32::MAX)),
        ]);
        let values = p.resolve(&configured, &overrides).unwrap();
        assert_eq!(values["height"], 3.0);
        assert_eq!(values["density"], 30.0);
        for invalid in [json!(-1), json!("large"), json!(false)] {
            assert!(
                p.resolve(
                    &BTreeMap::new(),
                    &BTreeMap::from([("height".into(), invalid)])
                )
                .is_err()
            );
        }
        let examples = [
            ("float", json!(1.2)),
            ("integer", json!(-2)),
            ("unsigned", json!(2)),
            ("boolean", json!(true)),
            ("vec2", json!([1, 2])),
            ("vec3", json!([1, 2, 3])),
            ("vec4", json!([1, 2, 3, 4])),
            ("color", json!([0.2, 0.5, 0.4, 0.3])),
            (
                "matrix",
                json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]),
            ),
            ("texture", json!("image.png")),
            ("enum", json!("a")),
        ];
        for (kind, value) in examples {
            let mut param = p.parameters[0].clone();
            param.kind = kind.into();
            param.min = None;
            param.max = None;
            param.options.insert("a".into(), "Choice A".into());
            param.validate_value(&value).unwrap();
            assert!(param.validate_value(&Value::Null).is_err());
        }
    }
    #[test]
    fn instance_assignment_overrides_survive_roundtrip_without_changing_shared_topology() {
        let mut d = Document::new();
        let first = d
            .add_cuboid("box", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let second = d.copy_root_occurrence(&first, Vec3::X * 5.).unwrap();
        d.put_material(material()).unwrap();
        let face = *d
            .effective_geometry(&first)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let source = d.effective_geometry(&first).unwrap().clone();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: first.clone(),
            entity: SelectedEntity::Triangle(face),
        });
        assign_procedural_material(
            &mut d,
            &selection,
            "grass",
            &BTreeMap::from([("height".into(), json!(2.))]),
        )
        .unwrap();
        assert_eq!(source, d.effective_geometry(&second).unwrap().clone());
        let g = d.effective_geometry(&first).unwrap();
        assert_eq!(source.vertices, g.vertices);
        assert_eq!(source.triangles.len(), g.triangles.len());
        assert_eq!(g.triangles[&face].extensions[OVERRIDES_KEY]["height"], 2.);
        let before = d.clone();
        assert!(
            assign_procedural_material(
                &mut d,
                &selection,
                "grass",
                &BTreeMap::from([("height".into(), json!(-2))])
            )
            .is_err()
        );
        assert_eq!(before, d);
        let reopened =
            crate::legacy::import_current_value(&crate::legacy::export_current_value(&d).unwrap())
                .unwrap();
        assert_eq!(d.materials(), reopened.materials());
        assert!(reopened.scene_geometry_refs().unwrap().iter().any(|p| {
            reopened.effective_geometry(p).is_some_and(|g| {
                g.triangles
                    .values()
                    .any(|f| f.extensions.contains_key(OVERRIDES_KEY))
            })
        }));
        assign_material(&mut d, &selection, "grass").unwrap();
        assert!(
            !d.effective_geometry(&first).unwrap().triangles[&face]
                .extensions
                .contains_key(OVERRIDES_KEY)
        );
    }
    #[test]
    fn unknown_programs_roundtrip_and_invalid_updates_are_atomic() {
        let mut d = Document::new();
        let mut m = material();
        m.procedural.as_mut().unwrap().program = "future.compute.v5".into();
        d.put_material(m).unwrap();
        d.validate().unwrap();
        let mut p = Program {
            id: "custom".into(),
            name: "Future".into(),
            backend: "mesh_shader_v9".into(),
            ..Default::default()
        };
        p.extensions
            .insert("future".into(), json!({"preserve":true}));
        d.put_procedural_program(p.clone()).unwrap();
        let reopened =
            crate::legacy::import_current_value(&crate::legacy::export_current_value(&d).unwrap())
                .unwrap();
        assert!(reopened.procedural_programs().contains(&p));
        assert!(!p.supported());
        let before = d.clone();
        let mut invalid = material();
        invalid
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), json!(-1));
        assert!(d.put_material(invalid).is_err());
        assert_eq!(before, d);
    }
    #[test]
    fn import_remaps_program_and_base_material_collisions_and_blocks_used_base_deletion() {
        let mut source = Document::new();
        let mut base = super::super::materials::Material {
            id: "base".into(),
            name: "Base".into(),
            color: Rgba8::new(10, 200, 10, 255),
            ..Default::default()
        };
        source.put_material(base.clone()).unwrap();
        let program = Program {
            id: "custom".into(),
            name: "Custom".into(),
            ..Default::default()
        };
        source.put_procedural_program(program).unwrap();
        let mut m = material();
        m.procedural.as_mut().unwrap().program = "custom".into();
        m.procedural.as_mut().unwrap().base_material = Some("base".into());
        source.put_material(m).unwrap();
        let mut host = Document::new();
        base.color = Rgba8::BLACK;
        host.put_material(base).unwrap();
        host.put_procedural_program(Program {
            id: "custom".into(),
            name: "Different".into(),
            ..Default::default()
        })
        .unwrap();
        let original_grass = source
            .materials()
            .into_iter()
            .find(|m| m.id == "grass")
            .unwrap();
        host.put_material(original_grass.clone()).unwrap();
        let ids = host.merge_materials(&source).unwrap();
        assert_ne!(ids["grass"], "grass");
        assert_eq!(
            host.materials()
                .into_iter()
                .find(|m| m.id == "grass")
                .unwrap(),
            original_grass
        );
        let imported = host
            .materials()
            .into_iter()
            .find(|m| m.id == ids["grass"])
            .unwrap();
        let p = imported.procedural.unwrap();
        assert_ne!(p.program, "custom");
        assert_ne!(p.base_material.as_deref(), Some("base"));
        assert!(
            host.remove_material(p.base_material.as_ref().unwrap())
                .is_err()
        );
        host.validate().unwrap();
        let mut referenced = host
            .materials()
            .into_iter()
            .find(|m| Some(m.id.as_str()) == p.base_material.as_deref())
            .unwrap();
        referenced.procedural = Some(ProceduralMaterial::default());
        let before = host.clone();
        assert!(host.put_material(referenced).is_err());
        assert_eq!(before, host);
    }
}
