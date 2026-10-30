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

    pub(crate) fn native_selection_materials(&self) -> std::collections::BTreeSet<String> {
        use std::collections::{BTreeSet, HashMap};
        let mut result = BTreeSet::new();
        let mut groups = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut primitives = BTreeMap::<OccurrenceRef, Vec<&SelectedEntity>>::new();
        for target in self.selection.selected() {
            if target.entity == SelectedEntity::Occurrence {
                let Some(r) = self.document.resolve_occurrence(&target.occurrence) else {
                    continue;
                };
                let children = r
                    .edits
                    .and_then(|e| e.child_layout)
                    .or(r.definition.default_child_layout)
                    .and_then(|l| self.document.child_layout(l))
                    .is_some_and(|l| !l.entries.is_empty());
                if children {
                    groups.insert(target.occurrence.path.clone());
                } else if let Some(binding) = self.document.instance_material(&target.occurrence) {
                    result.insert(binding.id);
                } else {
                    paths.insert(target.occurrence.clone());
                }
            } else if matches!(target.entity, SelectedEntity::SemanticHandle { .. }) {
                if let Some(owner) = self.document.architecture_element(target) {
                    result.extend(owner.element.surface_materials().values().cloned());
                }
            } else {
                primitives
                    .entry(target.occurrence.clone())
                    .or_default()
                    .push(&target.entity);
            }
        }
        if !groups.is_empty() {
            for path in self.document.scene_geometry_refs().unwrap_or_default() {
                if (0..=path.path.len()).any(|depth| groups.contains(&path.path[..depth])) {
                    paths.insert(path);
                }
            }
        }
        // Shared prototypes are scanned once, and selected primitive IDs are
        // grouped by owner. Never decode a script or walk the scene per face.
        let mut sources = HashMap::new();
        for path in paths {
            if let Some(g) = self.document.effective_geometry(&path) {
                let ids = sources.entry(g as *const _ as usize).or_insert_with(|| {
                    g.segments
                        .values()
                        .map(|e| &e.extensions)
                        .chain(g.triangles.values().map(|f| &f.extensions))
                        .filter_map(meshcraft_core::document::materials::material_id)
                        .map(str::to_owned)
                        .collect::<BTreeSet<_>>()
                });
                result.extend(ids.iter().cloned());
            }
        }
        for (path, entities) in primitives {
            let Some(g) = self.document.effective_geometry(&path) else {
                continue;
            };
            for entity in entities {
                let ext = match entity {
                    SelectedEntity::Segment(id) => g.segments.get(id).map(|e| &e.extensions),
                    SelectedEntity::Triangle(id) => g.triangles.get(id).map(|e| &e.extensions),
                    _ => None,
                };
                if let Some(id) = ext.and_then(meshcraft_core::document::materials::material_id) {
                    result.insert(id.to_owned());
                }
            }
        }
        result
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
        self.assign_material_targets(
            id,
            parameters,
            self.selection.selected().iter().cloned().collect(),
        )
    }

    pub(super) fn paint_material_targets(
        &mut self,
        targets: Vec<SelectionTarget>,
    ) -> Result<(), String> {
        let color = self.current_face_color();
        if let Some(id) = self.current_paint_material.clone().or_else(|| {
            self.document
                .materials()
                .into_iter()
                .find(|m| m.procedural.is_none() && m.color == color)
                .map(|m| m.id)
        }) {
            return self.assign_material_targets(&id, None, targets);
        }
        let mut material = self.document.next_material();
        material.color = color;
        material.name = format!(
            "Color #{:02x}{:02x}{:02x}{:02x}",
            color.red, color.green, color.blue, color.alpha
        );
        let id = material.id.clone();
        self.assign_material_targets_with_new(&id, None, targets, Some(material))
    }

    pub(super) fn assign_material_targets(
        &mut self,
        id: &str,
        parameters: Option<BTreeMap<String, Value>>,
        targets: Vec<SelectionTarget>,
    ) -> Result<(), String> {
        self.assign_material_targets_with_new(id, parameters, targets, None)
    }
    fn assign_material_targets_with_new(
        &mut self,
        id: &str,
        parameters: Option<BTreeMap<String, Value>>,
        targets: Vec<SelectionTarget>,
        new_material: Option<Material>,
    ) -> Result<(), String> {
        if new_material.is_none() && !self.document.materials().iter().any(|m| m.id == id) {
            return Err("Material missing".into());
        }
        let mut selection = SelectionSession::default();
        selection.set_context(self.selection.context.clone());
        let mut architecture = Vec::new();
        let mut instances = std::collections::BTreeSet::new();
        for target in targets {
            if !self.native_target_editable(&target) {
                continue;
            }
            if matches!(
                &target.entity,
                SelectedEntity::SemanticHandle {
                    kind: meshcraft_core::selection::SelectionKind::Hotspot,
                    ..
                }
            ) {
                continue;
            }
            if let Some(element) = self.document.architecture_element(&target) {
                if self.native_architecture_editable(&target) {
                    architecture.push(element);
                }
            } else if target.entity == SelectedEntity::Occurrence
                && self.document.is_object_occurrence(&target.occurrence)
            {
                instances.insert(target.occurrence);
            } else if matches!(
                target.entity,
                SelectedEntity::Segment(_) | SelectedEntity::Triangle(_)
            ) {
                selection.select(target);
            }
        }
        if selection.selected().is_empty() && architecture.is_empty() && instances.is_empty() {
            return Err(
                "Select editable geometry or object instances; hotspots use their own colors"
                    .into(),
            );
        }
        self.commit_document_edit("Assign material", |document| {
            if let Some(material) = new_material {
                document.put_material(material)?;
            }
            for instance in instances {
                document.set_instance_material(
                    &instance,
                    id,
                    parameters.clone().unwrap_or_default(),
                )?;
            }
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
    fn paint_promotes_picked_object_facets_but_prototype_editing_keeps_facet_scope() {
        let mut d = Document::new();
        let a = d
            .add_cuboid("box", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let b = d.copy_root_occurrence(&a, Vec3::X * 3.).unwrap();
        let face = *d
            .effective_geometry(&a)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut app = MeshcraftApp::new_native_session(d, "Ready".into(), None);
        app.choose_native_material("ansi-red").unwrap();
        app.paint_native_target(Some(SelectionTarget {
            occurrence: a.clone(),
            entity: SelectedEntity::Triangle(face),
        }));
        assert_eq!(app.document.instance_material(&a).unwrap().id, "ansi-red");
        assert!(app.document.instance_material(&b).is_none());
        assert!(
            app.document
                .occurrence_edits(&a)
                .unwrap()
                .geometry_replacement
                .is_none()
        );
        app.undo_document();
        assert!(app.document.instance_material(&a).is_none());
        app.selection
            .set_context(EditContext::Occurrence(a.clone()));
        app.paint_native_target(Some(SelectionTarget {
            occurrence: a.clone(),
            entity: SelectedEntity::Triangle(face),
        }));
        assert!(app.document.instance_material(&a).is_none());
        assert_eq!(
            1,
            app.document
                .resolve_occurrence(&a)
                .unwrap()
                .definition
                .geometry
                .triangles
                .values()
                .filter(|f| f.color == Rgba8::new(128, 0, 0, 255))
                .count()
        );
    }
    #[test]
    fn mixed_mesh_instance_and_architecture_material_assignment_is_one_undo_step() {
        use meshcraft_core::document::architecture::{Element, Wall};
        let mut d = Document::new();
        let a = d
            .add_cuboid("box", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let line =
            d.append_line_segment_to_scene_root(Vec3::new(5., 0., 0.), Vec3::new(6., 0., 0.));
        let root = d.ensure_scene_root();
        let wall = d
            .put_architecture(
                root,
                None,
                Element::Wall(Wall {
                    end: Vec3::X * 5.,
                    ..Default::default()
                }),
            )
            .unwrap();
        let mut app = MeshcraftApp::new_native_session(d, "Ready".into(), None);
        app.selection.select(SelectionTarget {
            occurrence: a.clone(),
            entity: SelectedEntity::Occurrence,
        });
        for id in line.unwrap().affected_segments {
            app.selection.select(SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::Segment(id),
            });
        }
        app.selection.select(SelectionTarget {
            occurrence: OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle {
                kind: meshcraft_core::selection::SelectionKind::Wall,
                id: wall,
            },
        });
        let before = app.document.clone();
        assert!(app.native_selection_materials().len() > 1);
        let mut targets = app.selection.selected().iter().cloned().collect::<Vec<_>>();
        let vertex = *app
            .document
            .effective_geometry(&a)
            .unwrap()
            .vertices
            .keys()
            .next()
            .unwrap();
        targets.push(SelectionTarget {
            occurrence: a.clone(),
            entity: SelectedEntity::Vertex(vertex),
        });
        app.assign_material_targets("ansi-blue", None, targets)
            .unwrap();
        assert_eq!(
            app.native_selection_materials(),
            std::collections::BTreeSet::from(["ansi-blue".into()])
        );
        assert_eq!(app.history.entries().len(), 1);
        app.undo_document();
        assert_eq!(
            app.document.scene_root_definition(),
            before.scene_root_definition()
        );
        assert!(app.document.instance_material(&a).is_none());
        assert_eq!(app.document.materials(), before.materials());
    }
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
