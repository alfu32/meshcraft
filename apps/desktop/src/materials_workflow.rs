//! Material operations use the same document history as modeling operations.
use super::*;
use meshcraft_core::document::materials::Material;
use serde_json::Value;

impl MeshcraftApp {
    pub(super) fn current_face_color(&self) -> Rgba8 {
        if self.current_paint_material.is_none() && self.current_paint_color == Rgba8::ANSI_GREY {
            self.document
                .material_colors()
                .get("ansi-grey")
                .copied()
                .unwrap_or(Rgba8::ANSI_GREY)
        } else {
            self.current_paint_color
        }
    }
    pub(super) fn current_outline_color(&self) -> Rgba8 {
        if self.current_paint_material.is_none() && self.current_paint_color == Rgba8::ANSI_GREY {
            self.document
                .material_colors()
                .get("ansi-black")
                .copied()
                .unwrap_or(Rgba8::BLACK)
        } else {
            self.current_paint_color
        }
    }
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

    pub fn delete_native_material(&mut self, id: &str) -> Result<(), String> {
        self.commit_global_document_edit("Delete unused material", |document| {
            document.remove_material(id)
        })?;
        if self.current_paint_material.as_deref() == Some(id) {
            self.current_paint_material = None;
            self.current_paint_color = Rgba8::ANSI_GREY;
        }
        self.status = "Deleted unused material.".into();
        Ok(())
    }

    pub fn save_native_procedural_program(
        &mut self,
        program: meshcraft_core::document::procedural::Program,
    ) -> Result<(), String> {
        self.commit_global_document_edit("Save procedural program", |d| {
            d.put_procedural_program(program)
        })
    }
    pub fn assign_native_material(&mut self, id: &str) -> Result<(), String> {
        self.assign_native_material_parameters(id, None)
    }
    pub fn assign_native_material_parameters(
        &mut self,
        id: &str,
        parameters: Option<BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        if let Some(values) = &parameters {
            let material = self
                .document
                .materials()
                .into_iter()
                .find(|m| m.id == id)
                .ok_or("Material missing")?;
            let p = material.procedural.ok_or("Material is conventional")?;
            self.document
                .procedural_programs()
                .into_iter()
                .find(|v| v.id == p.program)
                .ok_or("Program unavailable")?
                .resolve(&p.parameters, values)?;
        }
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
                    meshcraft_core::document::architecture::Element::Stair(_) => {
                        &["tread", "support"][..]
                    }
                    meshcraft_core::document::architecture::Element::Frame(_) => {
                        &["frame", "glazing"][..]
                    }
                };
                for slot in slots {
                    element
                        .surface_materials_mut()
                        .insert((*slot).into(), id.into());
                }
                if let Some(values) = &parameters {
                    element
                        .procedural_parameters_mut()
                        .insert(id.into(), values.clone());
                } else {
                    element.procedural_parameters_mut().remove(id);
                }
                let SelectedEntity::SemanticHandle { id: owner_id, .. } = &owner.target.entity
                else {
                    return Err(
                        "Architecture material target must identify its semantic owner".into(),
                    );
                };
                document.put_architecture(owner.definition, Some(owner_id), element)?;
            }
            if !selection.selected().is_empty() {
                if let Some(values) = &parameters {
                    modeling_transforms::assign_procedural_material(
                        document, &selection, id, values,
                    )?;
                } else {
                    modeling_transforms::assign_material(document, &selection, id)?;
                }
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
    fn deleting_unused_materials_is_undoable_and_defaults_remain_available() {
        let mut app = MeshcraftApp::new_native_session(Document::new(), "Ready".into(), None);
        let mut payload = CommandPayload::default();
        payload.values.insert("id".into(), "ansi-red".into());
        assert!(app.dispatch_command("material.delete", &payload).accepted);
        assert!(!app.document.material_colors().contains_key("ansi-red"));
        app.undo_document();
        assert!(app.document.material_colors().contains_key("ansi-red"));
        let before = app.document.clone();
        payload.values.insert("id".into(), "ansi-grey".into());
        assert_eq!(
            CommandCompletion::Failed,
            app.dispatch_command("material.delete", &payload)
                .result
                .completion
        );
        assert_eq!(before, app.document);
    }

    #[test]
    fn material_commands_are_undoable_and_apply_to_only_the_selected_instance() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let second = document.copy_root_occurrence(&first, Vec3::X * 3.).unwrap();
        let mut app = MeshcraftApp::new_native_session(document, "Ready".into(), None);
        let initial_materials = app.document.materials();
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
                        == Some("ansi-grey")
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
                        == Some("ansi-grey")
                )
        );
        app.undo_document();
        assert_eq!(initial_materials, app.document.materials());
    }
}

#[cfg(test)]
mod procedural_tests {
    use super::*;
    use meshcraft_core::document::{
        architecture::{Element, Wall},
        materials::Material,
        procedural::{OVERRIDES_KEY, ProceduralMaterial},
    };
    #[test]
    fn procedural_architecture_overrides_regenerate_and_undo_without_baking() {
        let mut app = MeshcraftApp::new_native_session(Document::new(), "Ready".into(), None);
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let owner = app
            .document
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let material = Material {
            id: "grass".into(),
            name: "Grass".into(),
            procedural: Some(ProceduralMaterial::default()),
            ..Default::default()
        };
        app.save_native_material(material).unwrap();
        let target = SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle {
                kind: meshcraft_core::selection::SelectionKind::Wall,
                id: owner.clone(),
            },
        };
        app.selection.select(target.clone());
        let before = app.document.clone();
        app.assign_native_material_parameters(
            "grass",
            Some(BTreeMap::from([("height".into(), json!(2.))])),
        )
        .unwrap();
        assert!(
            app.document
                .scene_root_definition()
                .unwrap()
                .geometry
                .triangles
                .values()
                .all(|f| f.extensions[OVERRIDES_KEY]["height"] == 2.)
        );
        let saved = meshcraft_core::legacy::export_current_value(&app.document).unwrap();
        let reopened = meshcraft_core::legacy::import_current_value(&saved).unwrap();
        assert!(reopened.scene_geometry_refs().unwrap().iter().any(|p| {
            reopened.effective_geometry(p).is_some_and(|g| {
                g.triangles.values().any(|f| {
                    f.extensions
                        .get(OVERRIDES_KEY)
                        .is_some_and(|v| v["height"] == 2.)
                })
            })
        }));
        // A legacy reopen synthesizes model identities independently of the
        // original persisted record ID. Apply must use the selected owner ID.
        let mut reopened_app = MeshcraftApp::new_native_session(reopened, "Reopened".into(), None);
        let selected = reopened_app
            .document
            .architecture_elements()
            .into_iter()
            .next()
            .unwrap();
        let SelectedEntity::SemanticHandle { id, .. } = &selected.target.entity else {
            panic!()
        };
        assert_ne!(id, selected.element.id());
        reopened_app.selection.select(selected.target);
        reopened_app
            .assign_native_material_parameters(
                "grass",
                Some(BTreeMap::from([("height".into(), json!(3.))])),
            )
            .unwrap();
        assert_eq!(
            reopened_app.document.architecture_elements()[0]
                .element
                .value()["proceduralParameters"]["grass"]["height"],
            3.
        );
        app.undo_document();
        assert_eq!(
            before.scene_root_definition(),
            app.document.scene_root_definition()
        );
        app.redo_document();
        let Element::Wall(mut wall) = app.document.architecture_element(&target).unwrap().element
        else {
            panic!()
        };
        wall.height = 12.;
        app.document
            .put_architecture(root, Some(&owner), Element::Wall(wall))
            .unwrap();
        assert!(
            app.document
                .scene_root_definition()
                .unwrap()
                .geometry
                .triangles
                .values()
                .all(|f| f.extensions[OVERRIDES_KEY]["height"] == 2.)
        );
    }
}
