//! Authored material catalog; texture coordinates are sizes in model units.
use super::*;

pub const CATALOG_KEY: &str = "k3d.meshcraftMaterials";
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
pub struct Material {
    pub id: String,
    pub name: String,
    pub color: Rgba8,
    pub shading_type: ShadingType,
    pub texture_path: Option<String>,
    pub texture_scale_u: f64,
    pub texture_scale_v: f64,
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
        }
    }
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
        Ok(())
    }
}

pub fn material_id(extensions: &BTreeMap<String, Value>) -> Option<&str> {
    extensions.get(MATERIAL_KEY)?.as_str()
}

pub(super) fn default_binding() -> BTreeMap<String, Value> {
    BTreeMap::from([(MATERIAL_KEY.into(), Value::String("default".into()))])
}

impl Document {
    pub fn materials(&self) -> Vec<Material> {
        self.read_materials()
            .unwrap_or_else(|_| vec![Material::default()])
    }

    fn read_materials(&self) -> Result<Vec<Material>, String> {
        match self.extensions.get(CATALOG_KEY) {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid material catalog: {e}")),
            None => Ok(vec![Material::default()]),
        }
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
        if !ids.contains("default") {
            return Err("the Default material is missing".into());
        }
        Ok(())
    }

    /// Save a material under a stable ID. Existing geometry retains that reference.
    pub fn put_material(&mut self, mut material: Material) -> Result<(), String> {
        material.name = material.name.trim().to_owned();
        material.texture_path = material.texture_path.filter(|p| !p.trim().is_empty());
        material.validate()?;
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
        self.bump_revision();
        Ok(())
    }

    pub(super) fn merge_materials(
        &mut self,
        source: &Document,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut ids = BTreeMap::new();
        for mut material in source.read_materials()? {
            let original = material.id.clone();
            if let Some(existing) = self.materials().into_iter().find(|m| m.id == material.id) {
                if existing == material {
                    ids.insert(original, existing.id);
                    continue;
                }
                material.id = self.next_material().id;
                material.name.push_str(" (imported)");
            }
            ids.insert(original, material.id.clone());
            self.put_material(material)?;
        }
        Ok(ids)
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

#[cfg(test)]
mod tests {
    use super::*;

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
            .find(|id| *id != "default")
            .unwrap();
        assert_ne!(id, blue.id);
        assert_eq!(red.color, host.material_colors()[id]);
        host.validate().unwrap();
    }

    #[test]
    fn catalog_is_shared_persistent_and_invalid_edits_are_atomic() {
        let mut document = Document::new();
        assert_eq!(vec![Material::default()], document.materials());
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
