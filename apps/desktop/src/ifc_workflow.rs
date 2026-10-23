//! IFC edits share document transactions with geometry and scripted parameters.
use super::*;
use meshcraft_core::document::ifc::{Properties, Property};
impl MeshcraftApp {
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
