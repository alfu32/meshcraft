//! Independent procedural objects carry settings and bounds, never generated mesh data (#31).
use super::super::materials::Material;
use super::*;
use crate::geometry::{Aabb3, Vec3};
pub const OBJECT_KEY: &str = "k3d.meshcraftProceduralObject";
impl Document {
    pub fn procedural_object_definition(&self, id: DefinitionId) -> Option<Material> {
        serde_json::from_value(self.definition(id)?.extensions.get(OBJECT_KEY)?.clone()).ok()
    }
    pub fn procedural_object(&self, path: &OccurrenceRef) -> Option<Material> {
        let r = self.resolve_occurrence(path)?;
        let inherited = self.procedural_object_definition(r.definition.id)?;
        self.occurrence_edits(path)
            .and_then(|e| e.extensions.get(OBJECT_KEY))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .or(Some(inherited))
    }
    pub fn create_procedural_tree(&mut self) -> Result<DefinitionId, String> {
        let mut material = Material::default();
        apply_preset(&mut material, TREE);
        let id = self.add_definition("Tree", DefinitionKind::Mesh);
        self.set_procedural_object(id, None, material)?;
        Ok(id)
    }
    pub fn set_procedural_object(
        &mut self,
        id: DefinitionId,
        path: Option<&OccurrenceRef>,
        mut settings: Material,
    ) -> Result<(), String> {
        settings.validate()?;
        let config = settings
            .procedural
            .as_ref()
            .ok_or("Procedural object requires a program")?;
        config.validate()?;
        let program = self
            .procedural_programs()
            .into_iter()
            .find(|p| p.id == config.program)
            .ok_or("Unknown procedural object program")?;
        if program.generation != "tree" {
            return Err("Choose an object-generation program".into());
        }
        program.resolve(&config.parameters, &BTreeMap::new())?;
        let bounds = object_bounds(&settings, &self.procedural_programs());
        settings.procedural.as_mut().unwrap().extensions.insert(
            "fallbackBounds".into(),
            serde_json::to_value(bounds).map_err(|e| e.to_string())?,
        );
        if let Some(base) = settings
            .procedural
            .as_ref()
            .and_then(|c| c.base_material.as_ref())
            && !self
                .materials()
                .iter()
                .any(|m| &m.id == base && m.procedural.is_none())
        {
            return Err("Object base material must reference a conventional material".into());
        }
        let definition = self.definition(id).ok_or("Prototype no longer exists")?;
        if id == Self::SCENE_ROOT_DEFINITION_ID
            || !definition.geometry.vertices.is_empty()
            || definition
                .default_child_layout
                .and_then(|id| self.child_layout(id))
                .is_some_and(|l| !l.entries.is_empty())
            || self.script(id).is_some()
        {
            return Err("Procedural owners require an empty, independent prototype".into());
        }
        if let Some(path) = path {
            if !self
                .resolve_occurrence(path)
                .is_some_and(|r| r.definition.id == id)
            {
                return Err("Instance no longer belongs to this prototype".into());
            }
            let mut edits = self.occurrence_edits(path).cloned().unwrap_or_default();
            edits.values.remove(OBJECT_KEY);
            self.occurrence_edits.insert(path.clone(), edits);
            let edits = self.occurrence_edits.get_mut(path).unwrap();
            edits.extensions.insert(
                OBJECT_KEY.into(),
                serde_json::to_value(settings).map_err(|e| e.to_string())?,
            );
        } else {
            self.rename_definition(id, &settings.name)?;
            settings.id = format!("procedural-object-{}", id.0);
            self.definitions.get_mut(&id).unwrap().extensions.insert(
                OBJECT_KEY.into(),
                serde_json::to_value(settings).map_err(|e| e.to_string())?,
            );
        }
        self.bump_revision();
        Ok(())
    }
    pub fn procedural_object_local_bounds(&self, path: &OccurrenceRef) -> Option<Aabb3> {
        let settings = self.procedural_object(path)?;
        Some(object_bounds(&settings, &self.procedural_programs()))
    }
    pub(in crate::document) fn validate_procedural_objects(&self) -> Result<(), String> {
        let programs = self.procedural_programs();
        for settings in self.procedural_object_settings() {
            settings.validate()?;
            let config = settings
                .procedural
                .as_ref()
                .ok_or("Procedural object requires a program")?;
            if let Some(p) = programs.iter().find(|p| p.id == config.program) {
                p.resolve(&config.parameters, &BTreeMap::new())?;
                if p.generation != "tree" {
                    return Err("Procedural object must use an object-generation program".into());
                }
            }
            if let Some(base) = &config.base_material
                && !self
                    .materials()
                    .iter()
                    .any(|m| &m.id == base && m.procedural.is_none())
            {
                return Err("Object base material must reference a conventional material".into());
            }
        }
        Ok(())
    }
    pub fn procedural_object_settings(&self) -> Vec<Material> {
        self.definitions
            .values()
            .map(|d| &d.extensions)
            .chain(self.occurrence_edits.values().map(|e| &e.extensions))
            .chain(
                self.child_layouts
                    .values()
                    .flat_map(|l| l.entries.iter().map(|e| &e.default_edits.extensions)),
            )
            .filter_map(|e| e.get(OBJECT_KEY))
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect()
    }
    pub fn procedural_object_world_bounds(&self, path: &OccurrenceRef) -> Option<Aabb3> {
        let b = self.procedural_object_local_bounds(path)?;
        let m = self.resolve_occurrence(path)?.world_transform;
        Aabb3::from_points(bounds_corners(b).map(|p| m.transform_point(p)))
    }
}
pub fn bounds_corners(b: Aabb3) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        )
    })
}

fn object_bounds(settings: &Material, programs: &[Program]) -> Aabb3 {
    let fallback = Aabb3 {
        min: Vec3::new(-3., -1., -3.),
        max: Vec3::new(3., 7., 3.),
    };
    let Some(config) = &settings.procedural else {
        return fallback;
    };
    let Some(values) = programs
        .iter()
        .find(|p| p.id == config.program)
        .and_then(|p| p.resolve(&config.parameters, &BTreeMap::new()).ok())
    else {
        return config
            .extensions
            .get("fallbackBounds")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(fallback);
    };
    let n = |id: &str| values.get(id).and_then(Value::as_f64).unwrap_or(0.);
    let h = n("height");
    let pad = n("leafSize") * 3. + h * n("wind") + n("trunkRadius");
    let r = n("radius").max(n("trunkRadius")) + pad;
    Aabb3 {
        min: Vec3::new(-r, -pad, -r),
        max: Vec3::new(r, h + pad, r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::derived::DerivedIndexCache,
        operations::objects::{duplicate_occurrence, place_definition},
        selection::EditContext,
    };
    #[test]
    fn tree_parameters_bounds_copy_and_serialization_are_mesh_free() {
        let mut d = Document::new();
        let id = d.create_procedural_tree().unwrap();
        let a =
            place_definition(&mut d, &EditContext::Document, id, Vec3::ZERO, None, false).unwrap();
        let b = place_definition(
            &mut d,
            &EditContext::Document,
            id,
            Vec3::X * 20.,
            None,
            false,
        )
        .unwrap();
        let mut settings = d.procedural_object(&a).unwrap();
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), 9.0.into());
        d.set_procedural_object(id, Some(&a), settings.clone())
            .unwrap();
        assert_eq!(
            d.procedural_object(&a)
                .unwrap()
                .procedural
                .unwrap()
                .parameters,
            settings.procedural.unwrap().parameters
        );
        assert!(d.procedural_object_world_bounds(&a).unwrap().max.y > 9.);
        assert!(d.procedural_object_world_bounds(&b).unwrap().max.y < 9.);
        let copy = duplicate_occurrence(&mut d, &a, Vec3::Z * 10.).unwrap();
        assert_eq!(d.procedural_object(&a), d.procedural_object(&copy));
        let mut index = DerivedIndexCache::default();
        assert!(index.world_bounds(&d, &a).is_some());
        assert!(d.definition(id).unwrap().geometry.vertices.is_empty());
        assert!(d.definition(id).unwrap().geometry.triangles.is_empty());
        d.validate().unwrap();
        let restored: Document = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
        assert_eq!(restored.procedural_object(&a), d.procedural_object(&a));
        assert_eq!(
            restored.procedural_object_world_bounds(&a),
            d.procedural_object_world_bounds(&a)
        );
    }
    #[test]
    fn tree_clipboard_feedback_and_import_references_survive() {
        use crate::selection::{SelectedEntity, SelectionSession, SelectionTarget};
        let mut d = Document::new();
        let id = d.create_procedural_tree().unwrap();
        let a =
            place_definition(&mut d, &EditContext::Document, id, Vec3::ZERO, None, false).unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: a.clone(),
            entity: SelectedEntity::Occurrence,
        });
        let clipboard =
            crate::document::clipboard::ClipboardSelection::capture(&d, &selection).unwrap();
        assert_eq!(clipboard.preview_edges.len(), 12);
        let edges = crate::operations::transforms::preview_selection_transform(
            &d,
            &selection,
            crate::geometry::Affine3::from_translation(Vec3::X),
        )
        .unwrap();
        assert_eq!(edges.len(), 12);
        let mut base = crate::document::materials::Material {
            id: "tree-base".into(),
            name: "Source base".into(),
            ..Default::default()
        };
        d.put_material(base.clone()).unwrap();
        let mut settings = d.procedural_object_definition(id).unwrap();
        settings.procedural.as_mut().unwrap().base_material = Some(base.id.clone());
        d.set_procedural_object(id, None, settings).unwrap();
        assert!(d.material_is_used("tree-base"));
        assert!(d.remove_material("tree-base").is_err());
        let mut host = Document::new();
        base.name = "Host base".into();
        host.put_material(base).unwrap();
        host.import_subscene(&d, "Imported trees", None, None)
            .unwrap();
        let imported = host
            .definitions()
            .find_map(|d| host.procedural_object_definition(d.id))
            .unwrap();
        let base_id = imported.procedural.unwrap().base_material.unwrap();
        assert_ne!(base_id, "tree-base");
        assert!(
            host.materials()
                .iter()
                .find(|m| m.id == base_id)
                .unwrap()
                .name
                .starts_with("Source base")
        );
        host.validate().unwrap();
        let mut settings = d.procedural_object_definition(id).unwrap();
        settings.procedural.as_mut().unwrap().program = "future.tree.backend".into();
        d.definitions
            .get_mut(&id)
            .unwrap()
            .extensions
            .insert(OBJECT_KEY.into(), serde_json::to_value(settings).unwrap());
        d.validate().unwrap();
        assert!(d.procedural_object_world_bounds(&a).is_some());
    }
    #[test]
    fn builtins_are_immutable_valid_and_unsupported_data_survives() {
        let mut d = Document::new();
        for p in builtin_programs() {
            p.validate().unwrap();
            assert!(p.supported());
            assert!(d.put_procedural_program(p).is_err());
        }
        assert_eq!(d.procedural_programs().len(), 5);
        let id = d.create_procedural_tree().unwrap();
        let before = d.clone();
        let mut bad = d.procedural_object_definition(id).unwrap();
        bad.procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), (-1.).into());
        assert!(d.set_procedural_object(id, None, bad).is_err());
        assert_eq!(d, before);
    }
}
