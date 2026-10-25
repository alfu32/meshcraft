//! Authored material catalog; texture coordinates are sizes in model units.
use super::*;

pub const CATALOG_KEY: &str = "k3d.meshcraftMaterials";
const PALETTE_KEY: &str = "k3d.meshcraftPaletteVersion";
pub const DEFAULT_LINE_MATERIAL: &str = "ansi-black";
pub const DEFAULT_FACE_MATERIAL: &str = "ansi-grey";
pub const MATERIAL_KEY: &str = "k3d.meshcraftMaterialId";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadingType {
    Unlit,
    Flat,
    Gouraud,
    #[default]
    Phong,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pbr {
    pub roughness: f64,
    pub metallic: f64,
}
impl Default for Pbr {
    fn default() -> Self {
        Self {
            roughness: 0.6,
            metallic: 0.0,
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Material {
    pub id: String,
    pub name: String,
    pub color: Rgba8,
    pub shading_type: ShadingType,
    pub texture_path: Option<String>,
    pub texture_scale_u: f64,
    pub texture_scale_v: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procedural: Option<super::procedural::ProceduralMaterial>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pbr: Option<Pbr>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Default".into(),
            color: Rgba8::WHITE,
            shading_type: ShadingType::Phong,
            texture_path: None,
            texture_scale_u: 1.,
            texture_scale_v: 1.,
            procedural: None,
            pbr: None,
        }
    }
}

/// Conventional 16-color ANSI palette plus the architecture surface defaults.
/// ANSI defines color slots; these RGB values use the classic VGA palette.
pub fn default_palette() -> Vec<Material> {
    [
        ("ansi-black", 0x000000ff),
        ("ansi-red", 0x800000ff),
        ("ansi-green", 0x008000ff),
        ("ansi-yellow", 0x808000ff),
        ("ansi-blue", 0x000080ff),
        ("ansi-magenta", 0x800080ff),
        ("ansi-cyan", 0x008080ff),
        ("ansi-grey", 0xc0c0c0ff),
        ("ansi-dark-grey", 0x808080ff),
        ("ansi-bright-red", 0xff0000ff),
        ("ansi-bright-green", 0x00ff00ff),
        ("ansi-bright-yellow", 0xffff00ff),
        ("ansi-bright-blue", 0x0000ffff),
        ("ansi-bright-magenta", 0xff00ffff),
        ("ansi-bright-cyan", 0x00ffffff),
        ("ansi-white", 0xffffffff),
        ("smoke-white", 0xedededff),
        ("smoke-grey", 0xd6d6d6ff),
        ("side-grey", 0xe0e0e0ff),
        ("glass", 0xb8d6f266),
    ]
    .into_iter()
    .map(|(name, rgba): (&str, u32)| {
        let [red, green, blue, alpha] = rgba.to_be_bytes();
        Material {
            id: name.into(),
            name: name.into(),
            color: Rgba8::new(red, green, blue, alpha),
            ..Material::default()
        }
    })
    .collect()
}

pub(super) fn initial_extensions() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            CATALOG_KEY.into(),
            serde_json::to_value(default_palette()).expect("palette serialization"),
        ),
        (PALETTE_KEY.into(), Value::from(1)),
    ])
}

impl Material {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("material ID and name cannot be empty".into());
        }
        if [self.texture_scale_u, self.texture_scale_v]
            .iter()
            .any(|size| !size.is_finite() || *size <= 0.)
        {
            return Err("texture U/V sizes must be positive model-unit distances".into());
        }
        if self.pbr.as_ref().is_some_and(|p| {
            [p.roughness, p.metallic]
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        }) {
            return Err("PBR roughness and metallic must be between zero and one".into());
        }
        if let Some(p) = &self.procedural {
            p.validate()?;
        }
        Ok(())
    }
}

pub fn material_id(extensions: &BTreeMap<String, Value>) -> Option<&str> {
    extensions.get(MATERIAL_KEY)?.as_str()
}

pub(super) fn default_binding() -> BTreeMap<String, Value> {
    BTreeMap::from([(
        MATERIAL_KEY.into(),
        Value::String(DEFAULT_FACE_MATERIAL.into()),
    )])
}

pub(super) fn default_line_binding() -> BTreeMap<String, Value> {
    BTreeMap::from([(
        MATERIAL_KEY.into(),
        Value::String(DEFAULT_LINE_MATERIAL.into()),
    )])
}

impl Document {
    pub fn materials(&self) -> Vec<Material> {
        self.read_materials().unwrap_or_else(|_| default_palette())
    }

    fn read_materials(&self) -> Result<Vec<Material>, String> {
        let mut materials: Vec<Material> = match self.extensions.get(CATALOG_KEY) {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid material catalog: {e}"))?,
            // Preserve the original white "default" binding in older drawings.
            None => vec![Material::default()],
        };
        if !self.extensions.contains_key(PALETTE_KEY) {
            for material in default_palette() {
                if !materials.iter().any(|m| m.id == material.id) {
                    materials.push(material);
                }
            }
        }
        Ok(materials)
    }

    pub(super) fn validate_materials(&self) -> Result<(), String> {
        let materials = self.read_materials()?;
        if materials.is_empty() {
            return Err("the material catalog must contain a default material".into());
        }
        let mut ids = BTreeSet::new();
        for material in materials {
            material.validate()?;
            if !ids.insert(material.id) {
                return Err("material IDs must be unique".into());
            }
        }
        Ok(())
    }

    /// Save a material under a stable ID. Existing geometry retains that reference.
    pub fn put_material(&mut self, mut material: Material) -> Result<(), String> {
        material.name = material.name.trim().to_owned();
        material.texture_path = material.texture_path.filter(|p| !p.trim().is_empty());
        material.validate()?;
        if let Some(p) = &material.procedural {
            if let Some(program) = self
                .procedural_programs()
                .into_iter()
                .find(|v| v.id == p.program)
            {
                program.resolve(&p.parameters, &BTreeMap::new())?;
            }
            if let Some(base) = &p.base_material
                && (base == &material.id
                    || !self
                        .materials()
                        .iter()
                        .any(|m| &m.id == base && m.procedural.is_none()))
            {
                return Err("Base shading material must reference a conventional material".into());
            }
        }
        if material.procedural.is_some()
            && self
                .procedural_object_settings()
                .iter()
                .chain(self.materials().iter())
                .any(|m| {
                    m.id != material.id
                        && m.procedural
                            .as_ref()
                            .and_then(|p| p.base_material.as_deref())
                            == Some(material.id.as_str())
                })
        {
            return Err("A referenced base material must remain conventional".into());
        }
        let id = material.id.clone();
        let color = material.color;
        let mut materials = self.read_materials()?;
        if let Some(old) = materials.iter_mut().find(|m| m.id == material.id) {
            if *old == material {
                return Ok(());
            }
            *old = material;
        } else {
            materials.push(material);
        }
        self.extensions.insert(
            CATALOG_KEY.into(),
            serde_json::to_value(materials).map_err(|e| e.to_string())?,
        );
        self.extensions.insert(PALETTE_KEY.into(), Value::from(1));
        // Keep a current RGBA fallback for legacy readers and color-based modeling tools.
        let sync = |geometry: &mut GeometryBundle| {
            for segment in geometry.segments.values_mut() {
                if material_id(&segment.extensions) == Some(id.as_str()) {
                    segment.color = color;
                }
            }
            for face in geometry.triangles.values_mut() {
                if material_id(&face.extensions) == Some(id.as_str()) {
                    face.color = color;
                }
            }
        };
        for definition in self.definitions.values_mut() {
            sync(&mut definition.geometry);
        }
        for edits in self.occurrence_edits.values_mut() {
            if let Some(geometry) = edits.geometry_replacement.as_mut() {
                sync(geometry);
            }
        }
        for layout in self.child_layouts.values_mut() {
            for entry in &mut layout.entries {
                if let Some(geometry) = entry.default_edits.geometry_replacement.as_mut() {
                    sync(geometry);
                }
            }
        }
        fn sync_record(value: &mut Value, id: &str, color: Rgba8) {
            if let Some(map) = value.as_object_mut() {
                if let Some(bindings) = map.get("materials").and_then(Value::as_object).cloned() {
                    for (slot, field) in [
                        ("exterior", "exteriorColor"),
                        ("interior", "interiorColor"),
                        ("top", "topColor"),
                        ("bottom", "bottomColor"),
                        ("side", "sideColor"),
                        ("frame", "color"),
                        ("glazing", "glazingColor"),
                        ("tread", "treadColor"),
                        ("support", "supportColor"),
                    ] {
                        if bindings.get(slot).and_then(Value::as_str) == Some(id)
                            && map.contains_key(field)
                        {
                            map.insert(field.into(),serde_json::json!({"r":color.red as f64/255.,"g":color.green as f64/255.,"b":color.blue as f64/255.,"a":color.alpha as f64/255.}));
                        }
                    }
                }
                for value in map.values_mut() {
                    sync_record(value, id, color);
                }
            } else if let Some(values) = value.as_array_mut() {
                for value in values {
                    sync_record(value, id, color);
                }
            }
        }
        for d in self.definitions.values_mut() {
            for field in [
                architecture::WALLS,
                architecture::SLABS,
                architecture::FRAMES,
                architecture::STAIRS,
            ] {
                if let Some(value) = d.domain_data.get_mut(field) {
                    sync_record(value, &id, color);
                }
                if let Some(value) = d.extensions.get_mut(&format!("k3d.{field}")) {
                    sync_record(value, &id, color);
                }
            }
        }
        if let Some(value) = self.extensions.get_mut("k3d.meshcraftArchitectureDefaults") {
            sync_record(value, &id, color);
        }
        self.bump_revision();
        Ok(())
    }

    /// References include unplaced prototypes, per-instance replacements, scripted
    /// frames, architecture owners and architecture creation defaults.
    pub fn material_is_used(&self, id: &str) -> bool {
        // These materials are the implicit creation defaults, even in an empty drawing.
        if [
            DEFAULT_LINE_MATERIAL,
            DEFAULT_FACE_MATERIAL,
            "smoke-white",
            "smoke-grey",
            "side-grey",
            "glass",
        ]
        .contains(&id)
        {
            return true;
        }
        if self
            .procedural_object_settings()
            .iter()
            .chain(self.materials().iter())
            .any(|m| {
                m.procedural
                    .as_ref()
                    .and_then(|p| p.base_material.as_deref())
                    == Some(id)
            })
        {
            return true;
        }
        let color = self.material_colors().get(id).copied();
        let geometry_uses = |g: &GeometryBundle| {
            g.segments
                .values()
                .map(|s| (&s.extensions, s.color))
                .chain(g.triangles.values().map(|f| (&f.extensions, f.color)))
                .any(|(ext, rgba)| {
                    material_id(ext)
                        .or_else(|| {
                            ext.get("k3d.meshcraftScriptMaterial")
                                .and_then(|v| v.get("id"))
                                .and_then(Value::as_str)
                        })
                        .map_or(color == Some(rgba), |bound| bound == id)
                })
        };
        fn record_uses(value: &Value, id: &str) -> bool {
            match value {
                Value::Object(map) => {
                    map.get("materials")
                        .and_then(Value::as_object)
                        .is_some_and(|bindings| bindings.values().any(|v| v.as_str() == Some(id)))
                        || map.values().any(|v| record_uses(v, id))
                }
                Value::Array(values) => values.iter().any(|v| record_uses(v, id)),
                _ => false,
            }
        }
        self.definitions.values().any(|d| {
            geometry_uses(&d.geometry)
                || [
                    architecture::WALLS,
                    architecture::SLABS,
                    architecture::FRAMES,
                    architecture::STAIRS,
                ]
                .iter()
                .any(|key| {
                    d.domain_data.get(*key).is_some_and(|v| record_uses(v, id))
                        || d.extensions
                            .get(&format!("k3d.{key}"))
                            .is_some_and(|v| record_uses(v, id))
                })
        }) || self
            .occurrence_edits
            .values()
            .chain(
                self.child_layouts
                    .values()
                    .flat_map(|l| l.entries.iter().map(|e| &e.default_edits)),
            )
            .any(|e| e.geometry_replacement.as_ref().is_some_and(geometry_uses))
            || self
                .script_cache
                .frames
                .values()
                .any(|frame| geometry_uses(&frame.geometry))
            || self
                .extensions
                .get("k3d.meshcraftArchitectureDefaults")
                .is_some_and(|v| record_uses(v, id))
    }

    /// Remove an unused material. Rejection leaves the document untouched.
    pub fn remove_material(&mut self, id: &str) -> Result<(), String> {
        let mut materials = self.read_materials()?;
        let index = materials
            .iter()
            .position(|m| m.id == id)
            .ok_or("Material missing")?;
        if self.material_is_used(id) {
            return Err("Material is used by geometry or creation defaults".into());
        }
        materials.remove(index);
        self.extensions.insert(
            CATALOG_KEY.into(),
            serde_json::to_value(materials).map_err(|e| e.to_string())?,
        );
        self.extensions.insert(PALETTE_KEY.into(), Value::from(1));
        self.bump_revision();
        Ok(())
    }

    pub(super) fn merge_materials(
        &mut self,
        source: &Document,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut programs = BTreeMap::new();
        for mut p in source
            .procedural_programs()
            .into_iter()
            .filter(|p| !super::procedural::is_builtin(&p.id))
        {
            let old = p.id.clone();
            if self
                .procedural_programs()
                .iter()
                .any(|v| v.id == p.id && v != &p)
            {
                let existing = self.procedural_programs();
                let mut n = 1;
                while existing
                    .iter()
                    .any(|v| v.id == format!("{old}.imported{n}"))
                {
                    n += 1;
                }
                p.id = format!("{old}.imported{n}");
            }
            programs.insert(old, p.id.clone());
            self.put_procedural_program(p)?;
        }
        let mut source_materials = source.read_materials()?;
        source_materials.sort_by_key(|m| m.procedural.is_some());
        let mut reserved = self
            .materials()
            .iter()
            .map(|m| m.id.clone())
            .collect::<BTreeSet<_>>();
        let mut ids = BTreeMap::<String, String>::new();
        for m in &source_materials {
            let mut comparable = m.clone();
            if let Some(p) = comparable.procedural.as_mut() {
                if let Some(id) = programs.get(&p.program) {
                    p.program = id.clone();
                }
                if let Some(base) = p.base_material.as_mut()
                    && let Some(id) = ids.get(base)
                {
                    *base = id.clone();
                }
            }
            let id = if self
                .materials()
                .iter()
                .any(|v| v.id == m.id && v != &comparable)
            {
                let n = (1..)
                    .find(|n| !reserved.contains(&format!("material-{n}")))
                    .unwrap();
                format!("material-{n}")
            } else {
                m.id.clone()
            };
            reserved.insert(id.clone());
            ids.insert(m.id.clone(), id);
        }
        // Conventional base materials must exist before their procedural users.
        let mut ordered = source_materials;
        ordered.sort_by_key(|m| m.procedural.is_some());
        for mut m in ordered {
            let old = m.id.clone();
            m.id = ids[&old].clone();
            if m.id != old {
                m.name.push_str(" (imported)");
            }
            if let Some(p) = m.procedural.as_mut() {
                if let Some(id) = programs.get(&p.program) {
                    p.program = id.clone();
                }
                if let Some(base) = p.base_material.as_mut()
                    && let Some(id) = ids.get(base)
                {
                    *base = id.clone();
                }
            }
            self.put_material(m)?;
        }
        Ok(ids)
    }

    pub fn material_for_color(&mut self, color: Rgba8) -> Result<String, String> {
        if let Some(m) = self
            .materials()
            .into_iter()
            .find(|m| m.procedural.is_none() && m.color == color)
        {
            return Ok(m.id);
        }
        let mut material = self.next_material();
        material.name = format!(
            "Color #{:02x}{:02x}{:02x}{:02x}",
            color.red, color.green, color.blue, color.alpha
        );
        material.color = color;
        let id = material.id.clone();
        self.put_material(material)?;
        Ok(id)
    }
    pub fn next_material(&self) -> Material {
        let materials = self.materials();
        let n = (1..)
            .find(|n| !materials.iter().any(|m| m.id == format!("material-{n}")))
            .unwrap_or(1);
        Material {
            id: format!("material-{n}"),
            name: format!("Material {n}"),
            color: Rgba8::new(184, 200, 214, 255),
            ..Material::default()
        }
    }

    pub fn material_colors(&self) -> BTreeMap<String, Rgba8> {
        self.materials()
            .into_iter()
            .map(|m| (m.id, m.color))
            .collect()
    }
}

impl Document {
    /// Promote legacy independent RGBA values to catalog references without changing appearance.
    pub fn normalize_material_bindings(&mut self) -> Result<(), String> {
        let mut colors = self.material_colors();
        let mut needed = BTreeMap::<[u8; 4], Rgba8>::new();
        let collect = |g: &GeometryBundle, needed: &mut BTreeMap<[u8; 4], Rgba8>| {
            for (color, ext) in g
                .segments
                .values()
                .map(|s| (s.color, &s.extensions))
                .chain(g.triangles.values().map(|s| (s.color, &s.extensions)))
            {
                if material_id(ext).is_none_or(|id| !colors.contains_key(id)) {
                    needed.insert([color.red, color.green, color.blue, color.alpha], color);
                }
            }
        };
        for d in self.definitions.values() {
            collect(&d.geometry, &mut needed);
        }
        for e in self.occurrence_edits.values().chain(
            self.child_layouts
                .values()
                .flat_map(|l| l.entries.iter().map(|e| &e.default_edits)),
        ) {
            if let Some(g) = &e.geometry_replacement {
                collect(g, &mut needed);
            }
        }
        let mut catalog = self.materials();
        let mut by_color = catalog
            .iter()
            .filter(|m| m.procedural.is_none())
            .map(|m| {
                (
                    [m.color.red, m.color.green, m.color.blue, m.color.alpha],
                    m.id.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut used = catalog
            .iter()
            .map(|m| m.id.clone())
            .collect::<BTreeSet<_>>();
        let mut added = !self.extensions.contains_key(PALETTE_KEY);
        let mut ids = BTreeMap::new();
        for (key, color) in needed {
            let id = if let Some(id) = by_color.get(&key) {
                id.clone()
            } else {
                let base = format!(
                    "rgba-{:02x}{:02x}{:02x}{:02x}",
                    color.red, color.green, color.blue, color.alpha
                );
                let mut id = base.clone();
                let mut n = 1;
                while used.contains(&id) {
                    id = format!("{base}-{n}");
                    n += 1;
                }
                used.insert(id.clone());
                catalog.push(Material {
                    id: id.clone(),
                    name: format!(
                        "Color #{:02x}{:02x}{:02x}{:02x}",
                        color.red, color.green, color.blue, color.alpha
                    ),
                    color,
                    ..Default::default()
                });
                by_color.insert(key, id.clone());
                added = true;
                id
            };
            ids.insert(key, id);
        }
        if added {
            self.extensions.insert(
                CATALOG_KEY.into(),
                serde_json::to_value(catalog).map_err(|e| e.to_string())?,
            );
        }
        self.extensions.insert(PALETTE_KEY.into(), Value::from(1));
        colors = self.material_colors();
        let mut changed = added;
        let mut sync = |g: &mut GeometryBundle| {
            let mut item = |color: &mut Rgba8, ext: &mut BTreeMap<String, Value>| {
                if let Some(value) = material_id(ext).and_then(|id| colors.get(id)) {
                    if color != value {
                        *color = *value;
                        changed = true;
                    }
                } else if let Some(id) = ids.get(&[color.red, color.green, color.blue, color.alpha])
                {
                    ext.insert(MATERIAL_KEY.into(), Value::String(id.clone()));
                    changed = true;
                }
            };
            for e in g.segments.values_mut() {
                item(&mut e.color, &mut e.extensions);
            }
            for e in g.triangles.values_mut() {
                item(&mut e.color, &mut e.extensions);
            }
        };
        for d in self.definitions.values_mut() {
            sync(&mut d.geometry);
        }
        for e in self.occurrence_edits.values_mut() {
            if let Some(g) = e.geometry_replacement.as_mut() {
                sync(g);
            }
        }
        for l in self.child_layouts.values_mut() {
            for e in &mut l.entries {
                if let Some(g) = e.default_edits.geometry_replacement.as_mut() {
                    sync(g);
                }
            }
        }
        if changed {
            self.bump_revision();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_drawing_palette_defaults_and_unused_deletion_survive_roundtrip() {
        let mut d = Document::new();
        assert_eq!(
            16,
            d.materials()
                .iter()
                .filter(|m| m.id.starts_with("ansi-"))
                .count()
        );
        assert_eq!(20, d.materials().len());
        assert_eq!(Rgba8::BLACK, d.material_colors()[DEFAULT_LINE_MATERIAL]);
        assert_eq!(Rgba8::ANSI_GREY, d.material_colors()[DEFAULT_FACE_MATERIAL]);
        assert_eq!(102, d.material_colors()["glass"].alpha);
        let cube = d
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        d.normalize_material_bindings().unwrap();
        let geometry = d.effective_geometry(&cube).unwrap();
        assert!(geometry.segments.values().all(|s| s.color == Rgba8::BLACK
            && material_id(&s.extensions) == Some(DEFAULT_LINE_MATERIAL)));
        assert!(
            geometry
                .triangles
                .values()
                .all(|f| f.color == Rgba8::ANSI_GREY
                    && material_id(&f.extensions) == Some(DEFAULT_FACE_MATERIAL))
        );
        d.remove_material("ansi-red").unwrap();
        let reopened =
            crate::legacy::import_current_value(&crate::legacy::export_current_value(&d).unwrap())
                .unwrap();
        assert_eq!(d.materials(), reopened.materials());
        assert!(!reopened.material_colors().contains_key("ansi-red"));
        for id in [DEFAULT_LINE_MATERIAL, DEFAULT_FACE_MATERIAL, "glass"] {
            let before = d.clone();
            assert!(d.remove_material(id).is_err());
            assert_eq!(before, d);
        }
    }

    #[test]
    fn legacy_palette_migration_keeps_authored_white_faces_and_lines() {
        let mut d = Document::new();
        let cube = d
            .add_cuboid("old", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let definition = d.resolve_occurrence(&cube).unwrap().definition.id;
        let g = &mut d.definitions.get_mut(&definition).unwrap().geometry;
        for s in g.segments.values_mut() {
            s.color = Rgba8::WHITE;
            s.extensions.clear();
        }
        for f in g.triangles.values_mut() {
            f.color = Rgba8::WHITE;
            f.extensions.insert(MATERIAL_KEY.into(), "default".into());
        }
        d.extensions.remove(CATALOG_KEY);
        d.extensions.remove(PALETTE_KEY);
        d = crate::legacy::import_current_value(&crate::legacy::export_current_value(&d).unwrap())
            .unwrap();
        d.normalize_material_bindings().unwrap();
        let g = &d
            .definitions
            .values()
            .find(|definition| definition.name == "old")
            .unwrap()
            .geometry;
        assert!(g.segments.values().all(|s| s.color == Rgba8::WHITE));
        assert!(
            g.triangles
                .values()
                .all(|f| f.color == Rgba8::WHITE && material_id(&f.extensions) == Some("default"))
        );
        assert!(d.material_colors().contains_key("glass"));
        d.validate().unwrap();
    }

    #[test]
    fn deletion_checks_unplaced_definitions_instance_overrides_and_architecture_defaults() {
        let mut d = Document::new();
        let cube = d
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let definition = d.resolve_occurrence(&cube).unwrap().definition.id;
        let m = d.next_material();
        d.put_material(m.clone()).unwrap();
        let mut replacement = d.effective_geometry(&cube).unwrap().clone();
        replacement
            .triangles
            .values_mut()
            .next()
            .unwrap()
            .extensions
            .insert(MATERIAL_KEY.into(), m.id.clone().into());
        d.child_layouts
            .get_mut(&d.root_layout)
            .unwrap()
            .entries
            .clear();
        d.definitions.get_mut(&definition).unwrap().geometry = replacement.clone();
        let before = d.clone();
        assert!(d.remove_material(&m.id).is_err());
        assert_eq!(before, d);
        d.definitions.get_mut(&definition).unwrap().geometry = GeometryBundle::default();
        assert!(!d.material_is_used(&m.id));
        d.child_layouts
            .get_mut(&d.root_layout)
            .unwrap()
            .entries
            .push(LayoutEntry {
                id: LayoutEntryId(999),
                definition,
                transform: Affine3::IDENTITY,
                default_edits: InstanceEdits {
                    geometry_replacement: Some(replacement.clone()),
                    ..Default::default()
                },
                legacy_aliases: vec![],
            });
        assert!(d.material_is_used(&m.id));
        d.child_layouts
            .get_mut(&d.root_layout)
            .unwrap()
            .entries
            .clear();
        d.occurrence_edits.insert(
            cube.clone(),
            InstanceEdits {
                geometry_replacement: Some(replacement),
                ..Default::default()
            },
        );
        assert!(d.material_is_used(&m.id));
        d.occurrence_edits.clear();
        d.extensions.insert(
            "k3d.meshcraftArchitectureDefaults".into(),
            serde_json::json!({"wall":{"materials":{"exterior":m.id}}}),
        );
        assert!(d.remove_material(&m.id).is_err());
        d.extensions.remove("k3d.meshcraftArchitectureDefaults");
        d.remove_material(&m.id).unwrap();
    }

    #[test]
    fn assigning_and_repainting_preserve_instance_scope_and_catalog_reference() {
        use crate::operations::transforms::{assign_material, paint_selection};
        use crate::selection::{SelectedEntity, SelectionSession, SelectionTarget};
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let face = *document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut material = document.next_material();
        material.color = Rgba8::new(44, 88, 120, 125);
        document.put_material(material.clone()).unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        });
        assign_material(&mut document, &selection, &material.id).unwrap();
        let triangle = &document.effective_geometry(&occurrence).unwrap().triangles[&face];
        assert_eq!(
            Some(material.id.as_str()),
            material_id(&triangle.extensions)
        );
        material.color = Rgba8::new(80, 120, 200, 40);
        document.put_material(material.clone()).unwrap();
        assert_eq!(material.color, document.material_colors()[&material.id]);
        paint_selection(&mut document, &selection, Rgba8::WHITE).unwrap();
        assert_eq!(
            None,
            material_id(
                &document.effective_geometry(&occurrence).unwrap().triangles[&face].extensions
            )
        );
    }

    #[test]
    fn imported_scene_material_collisions_are_remapped_without_changing_the_host() {
        use crate::operations::transforms::assign_material;
        use crate::selection::{SelectedEntity, SelectionSession, SelectionTarget};
        let mut source = Document::new();
        let cube = source
            .add_cuboid("imported cube", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let face = *source
            .effective_geometry(&cube)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut red = source.next_material();
        red.color = Rgba8::new(240, 20, 20, 255);
        source.put_material(red.clone()).unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: cube,
            entity: SelectedEntity::Triangle(face),
        });
        assign_material(&mut source, &selection, &red.id).unwrap();
        let mut host = Document::new();
        let mut blue = host.next_material();
        blue.color = Rgba8::new(20, 20, 240, 255);
        host.put_material(blue.clone()).unwrap();
        host.import_subscene(&source, "External", Some("source.octd"), None)
            .unwrap();
        assert_eq!(blue.color, host.material_colors()[&blue.id]);
        let imported_geometry = host
            .child_layouts
            .values()
            .flat_map(|layout| &layout.entries)
            .find_map(|entry| entry.default_edits.geometry_replacement.as_ref())
            .unwrap();
        let id = imported_geometry
            .triangles
            .values()
            .filter_map(|face| material_id(&face.extensions))
            .find(|id| *id != DEFAULT_FACE_MATERIAL)
            .unwrap();
        assert_ne!(id, blue.id);
        assert_eq!(red.color, host.material_colors()[id]);
        host.validate().unwrap();
    }

    #[test]
    fn catalog_is_shared_persistent_and_invalid_edits_are_atomic() {
        let mut document = Document::new();
        assert_eq!(default_palette(), document.materials());
        let mut material = document.next_material();
        material.color = Rgba8::new(22, 44, 66, 90);
        material.texture_path = Some("textures/brick.png".into());
        material.texture_scale_u = 2.;
        document.put_material(material.clone()).unwrap();
        let reopened = crate::legacy::import_current_value(
            &crate::legacy::export_current_value(&document).unwrap(),
        )
        .unwrap();
        assert_eq!(document.materials(), reopened.materials());
        let before = document.clone();
        material.texture_scale_v = 0.;
        assert!(document.put_material(material).is_err());
        assert_eq!(before, document);
    }
}
