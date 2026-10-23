//! IFC edits share document transactions with geometry and scripted parameters.
use super::*;
use meshcraft_core::document::ifc::{Properties, Property};
impl MeshcraftApp {
    pub fn apply_native_ifc_preset(
        &mut self,
        definition: DefinitionId,
        paths: Vec<OccurrenceRef>,
        preset: &str,
    ) -> Result<usize, String> {
        use meshcraft_core::document::ifc::presets;
        let additions = presets::properties(preset)?;
        for path in &paths {
            if !self.native_target_editable(&SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Occurrence,
            }) {
                return Err("IFC instance is read-only".into());
            }
        }
        let added = self.commit_global_document_edit("Add IFC common properties", |d| {
            let mut count = 0;
            if paths.is_empty() {
                let mut properties = d.prototype_ifc(definition);
                count += presets::merge_missing(&mut properties, &additions);
                d.set_prototype_ifc(definition, properties)?;
            } else {
                for path in &paths {
                    let mut properties = d.instance_ifc(path);
                    count += presets::merge_missing(&mut properties, &additions);
                    d.set_instance_ifc(path, properties)?;
                }
            }
            Ok(count)
        })?;
        self.status = if added == 0 {
            "All IFC preset properties are already present.".into()
        } else {
            format!("Added {added} IFC properties with unset values.")
        };
        Ok(added)
    }
    pub fn native_ifc_selection(
        &self,
        prototype: bool,
    ) -> Option<(DefinitionId, Vec<OccurrenceRef>, Properties)> {
        let (id, _, _) = self.native_active_object()?;
        if prototype {
            return Some((id, vec![], self.document.prototype_ifc(id)));
        }
        let paths = self.selected_occurrences();
        let path = paths.first()?;
        Some((id, paths.clone(), self.document.instance_ifc(path)))
    }
    pub fn edit_native_ifc(
        &mut self,
        definition: DefinitionId,
        paths: Vec<OccurrenceRef>,
        old: Option<(String, String)>,
        set: String,
        name: String,
        property: Option<Property>,
    ) -> Result<(), String> {
        for path in &paths {
            if !self.native_target_editable(&SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::Occurrence,
            }) {
                return Err("IFC instance is read-only".into());
            }
        }
        self.commit_global_document_edit("Edit IFC properties", |d| {
            let patch = |mut props: Properties| -> Result<Properties, String> {
                if let Some((set, name)) = &old
                    && let Some(group) = props.get_mut(set)
                {
                    group.remove(name);
                    if group.is_empty() {
                        props.remove(set);
                    }
                }
                if let Some(property) = &property {
                    let group = props.entry(set.clone()).or_default();
                    if group.contains_key(&name)
                        && old.as_ref() != Some(&(set.clone(), name.clone()))
                    {
                        return Err("IFC property already exists".into());
                    }
                    group.insert(name.clone(), property.clone());
                }
                Ok(props)
            };
            if paths.is_empty() {
                d.set_prototype_ifc(definition, patch(d.prototype_ifc(definition))?)?;
            } else {
                for p in &paths {
                    d.set_instance_ifc(p, patch(d.instance_ifc(p))?)?;
                }
            }
            Ok(())
        })?;
        self.status = "Updated IFC properties.".into();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_preserve_values_copy_defaults_and_batch_instances_in_one_undo_step() {
        let mut app = super::super::tests::test_app();
        let p = app
            .document
            .add_cuboid("IFC presets", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = app.document.resolve_occurrence(&p).unwrap().definition.id;
        let q = app.document.copy_root_occurrence(&p, Vec3::X * 2.).unwrap();
        let seed: Properties = serde_json::from_value(
            json!({"Pset_WallCommon":{"FireRating":{"value":"EI60","description":"Authored"}}}),
        )
        .unwrap();
        app.document.set_prototype_ifc(id, seed.clone()).unwrap();
        assert_eq!(5, app.apply_native_ifc_preset(id, vec![], "wall").unwrap());
        let defaults = app.document.prototype_ifc(id);
        assert_eq!(
            seed["Pset_WallCommon"]["FireRating"],
            defaults["Pset_WallCommon"]["FireRating"]
        );
        assert!(defaults["Pset_WallCommon"]["IsExternal"].value.is_null());
        assert!(app.document.instance_ifc(&p).is_empty());
        let placed = modeling_objects::place_definition(
            &mut app.document,
            &EditContext::Document,
            id,
            Vec3::Z,
            None,
            false,
        )
        .unwrap();
        assert_eq!(defaults, app.document.instance_ifc(&placed));

        app.document.set_instance_ifc(&p, seed.clone()).unwrap();
        // The fixture above changes the document directly; begin history at this baseline.
        app.history = Default::default();
        let before = app.history.entries().len();
        assert_eq!(
            14,
            app.apply_native_ifc_preset(id, vec![p.clone(), q.clone()], "window")
                .unwrap()
        );
        assert_eq!(before + 1, app.history.entries().len());
        assert_eq!(
            seed["Pset_WallCommon"],
            app.document.instance_ifc(&p)["Pset_WallCommon"]
        );
        for path in [&p, &q] {
            assert_eq!(
                7,
                app.document.instance_ifc(path)["Pset_WindowCommon"].len()
            );
        }
        assert_eq!(defaults, app.document.prototype_ifc(id));
        assert_eq!(
            0,
            app.apply_native_ifc_preset(id, vec![p.clone(), q.clone()], "window")
                .unwrap()
        );
        assert_eq!(
            before + 1,
            app.history.entries().len(),
            "reapplying adds no history entry"
        );
        let saved = meshcraft_core::legacy::export_current_value(&app.document).unwrap();
        let restored = meshcraft_core::legacy::import_current_value(&saved).unwrap();
        // Project import remaps internal IDs, so locate records by their retained metadata.
        let expected = app.document.instance_ifc(&p);
        assert!(
            restored
                .occurrence_refs()
                .unwrap()
                .iter()
                .any(|path| restored.instance_ifc(path) == expected)
        );
        assert!(
            restored
                .definitions()
                .any(|definition| restored.prototype_ifc(definition.id) == defaults)
        );
        app.undo_document();
        assert_eq!(seed, app.document.instance_ifc(&p));
        assert!(app.document.instance_ifc(&q).is_empty());
        assert!(app.apply_native_ifc_preset(id, vec![], "unknown").is_err());
    }
    #[test]
    fn instance_ifc_batches_are_undoable_and_do_not_modify_prototype_defaults() {
        let mut app = super::super::tests::test_app();
        let p = app
            .document
            .add_cuboid("IFC", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = app.document.resolve_occurrence(&p).unwrap().definition.id;
        let q = app.document.copy_root_occurrence(&p, Vec3::X * 2.).unwrap();
        let property = Property {
            value: json!("EI60"),
            ..Default::default()
        };
        app.edit_native_ifc(
            id,
            vec![p.clone(), q.clone()],
            None,
            "Pset_WallCommon".into(),
            "FireRating".into(),
            Some(property),
        )
        .unwrap();
        assert!(app.document.prototype_ifc(id).is_empty());
        for p in [&p, &q] {
            assert_eq!(
                app.document.instance_ifc(p)["Pset_WallCommon"]["FireRating"].value,
                "EI60"
            );
        }
        app.undo_document();
        for p in [&p, &q] {
            assert!(app.document.instance_ifc(p).is_empty());
        }
    }
}
