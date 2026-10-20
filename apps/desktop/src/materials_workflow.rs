//! Material operations use the same document history as modeling operations.
use super::*;
use meshcraft_core::document::materials::Material;

impl MeshcraftApp {
    pub(super) fn commit_global_document_edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut pending = self.history.begin(&self.document, label);
        let value = pending.stage(operation).map_err(|e| e.to_string())?;
        self.history
            .commit(&mut self.document, pending)
            .map_err(|e| e.to_string())?;
        let view = self.native_document_for_view();
        self.selection.retain_valid(&view);
        Ok(value)
    }

    pub fn save_native_material(&mut self, material: Material) -> Result<(), String> {
        let name = material.name.clone();
        self.commit_global_document_edit("Save material", |document| {
            document.put_material(material)
        })?;
        self.status = format!("Saved material {name}.");
        Ok(())
    }

    pub fn assign_native_material(&mut self, id: &str) -> Result<(), String> {
        let mut selection = SelectionSession::default();
        selection.set_context(self.selection.context.clone());
        for target in self.selection.selected() {
            if matches!(target.entity, SelectedEntity::Occurrence) {
                for occurrence in self.document.scene_geometry_refs()? {
                    if !occurrence.path.starts_with(&target.occurrence.path) {
                        continue;
                    }
                    let Some(geometry) = self.document.effective_geometry(&occurrence) else {
                        continue;
                    };
                    for entity in geometry
                        .segments
                        .keys()
                        .copied()
                        .map(SelectedEntity::Segment)
                        .chain(
                            geometry
                                .triangles
                                .keys()
                                .copied()
                                .map(SelectedEntity::Triangle),
                        )
                    {
                        let target = SelectionTarget {
                            occurrence: occurrence.clone(),
                            entity,
                        };
                        if self
                            .selection_display_policy
                            .can_edit_target(&self.document, &target)
                        {
                            selection.select(target);
                        }
                    }
                }
            } else if self
                .selection_display_policy
                .can_edit_target(&self.document, target)
            {
                selection.select(target.clone());
            }
        }
        self.commit_document_edit("Assign material", |document| {
            modeling_transforms::assign_material(document, &selection, id)
        })?;
        self.status = "Assigned the material to the selected geometry.".into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_commands_are_undoable_and_apply_to_only_the_selected_instance() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let second = document.copy_root_occurrence(&first, Vec3::X * 3.).unwrap();
        let mut app = MeshcraftApp::new_native_session(document, "Ready".into(), None);
        let mut material = app.document.next_material();
        material.color = Rgba8::new(180, 80, 25, 110);
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("material".into(), serde_json::to_value(&material).unwrap());
        assert!(app.dispatch_command("material.save", &payload).accepted);
        app.selection.select(SelectionTarget {
            occurrence: first.clone(),
            entity: SelectedEntity::Occurrence,
        });
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("id".into(), material.id.clone().into());
        assert!(app.dispatch_command("material.apply", &payload).accepted);
        assert!(
            app.document
                .effective_geometry(&first)
                .unwrap()
                .triangles
                .values()
                .all(
                    |f| meshcraft_core::document::materials::material_id(&f.extensions)
                        == Some(material.id.as_str())
                )
        );
        assert!(
            app.document
                .effective_geometry(&second)
                .unwrap()
                .triangles
                .values()
                .all(
                    |f| meshcraft_core::document::materials::material_id(&f.extensions)
                        == Some("default")
                )
        );
        app.undo_document();
        assert!(
            app.document
                .effective_geometry(&first)
                .unwrap()
                .triangles
                .values()
                .all(
                    |f| meshcraft_core::document::materials::material_id(&f.extensions)
                        == Some("default")
                )
        );
        app.undo_document();
        assert_eq!(1, app.document.materials().len());
    }
}
