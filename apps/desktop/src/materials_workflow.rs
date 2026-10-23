//! Material operations use the same document history as modeling operations.
use super::*;
use meshcraft_core::document::materials::Material;

impl MeshcraftApp {
    pub(super) fn commit_global_document_edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Document) -> Result<T, String>,
    ) -> Result<T, String> {
        self.document.persist_script_states();
        let mut pending = self.history.begin(&self.document, label);
        let value = pending
            .stage(|d| {
                let value = operation(d)?;
                d.normalize_material_bindings()?;
                Ok(value)
            })
            .map_err(|e| e.to_string())?;
        self.history
            .commit(&mut self.document, pending)
            .map_err(|e| e.to_string())?;
        let view = self.native_document_for_view();
        self.selection.retain_valid(&view);
        Ok(value)
    }

    pub fn choose_native_material(&mut self, id: &str) -> Result<(), String> {
        let material = self
            .document
            .materials()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or("Material missing")?;
        self.current_paint_material = Some(id.into());
        self.current_paint_color = material.color;
        self.status = format!("Current material: {}", material.name);
        Ok(())
    }
    pub(super) fn paint_with_material(
        document: &mut Document,
        selection: &SelectionSession,
        color: Rgba8,
        material: Option<&str>,
    ) -> Result<meshcraft_core::operations::transforms::PaintReport, String> {
        let id =
            if let Some(id) = material.filter(|id| document.material_colors().contains_key(*id)) {
                id.to_owned()
            } else {
                document.material_for_color(color)?
            };
        modeling_transforms::assign_material(document, selection, &id)
    }
    pub fn save_native_material(&mut self, material: Material) -> Result<(), String> {
        let name = material.name.clone();
        let current = self.current_paint_material.as_deref() == Some(&material.id);
        let color = material.color;
        self.commit_global_document_edit("Save material", |document| {
            document.put_material(material)
        })?;
        if current {
            self.current_paint_color = color;
        }
        self.status = format!("Saved material {name}.");
        Ok(())
    }

    pub fn assign_native_material(&mut self, id: &str) -> Result<(), String> {
        let mut selection = SelectionSession::default();
        selection.set_context(self.selection.context.clone());
        let mut architecture = Vec::new();
        for target in self.selection.selected() {
            if matches!(
                &target.entity,
                SelectedEntity::SemanticHandle {
                    kind: meshcraft_core::selection::SelectionKind::Hotspot,
                    ..
                }
            ) {
                continue;
            }
            if let Some(element) = self.document.architecture_element(target) {
                if self.native_architecture_editable(target) {
                    architecture.push(element);
                }
                continue;
            }
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
        if selection.selected().is_empty() && architecture.is_empty() {
            return Err("Select editable 3D geometry; hotspots use their own colors".into());
        }
        self.commit_document_edit("Assign material", |document| {
            for owner in architecture {
                let mut element = owner.element;
                let slots = match &element {
                    meshcraft_core::document::architecture::Element::Wall(_) => {
                        &["exterior", "interior"][..]
                    }
                    meshcraft_core::document::architecture::Element::Slab(_) => {
                        &["top", "bottom", "side"][..]
                    }
                    _ => &["frame", "glazing"][..],
                };
                for slot in slots {
                    element
                        .surface_materials_mut()
                        .insert((*slot).into(), id.into());
                }
                let owner_id = element.id().to_owned();
                document.put_architecture(owner.definition, Some(&owner_id), element)?;
            }
            if !selection.selected().is_empty() {
                modeling_transforms::assign_material(document, &selection, id)?;
            }
            Ok(())
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
