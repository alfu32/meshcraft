use super::*;

pub(super) struct AnnotationState {
    pub points: Vec<Vec3>,
    pub text: String,
    pub dimension_label: String,
    pub size: f64,
}
impl Default for AnnotationState {
    fn default() -> Self {
        Self {
            points: vec![],
            text: "Text".into(),
            dimension_label: String::new(),
            size: 0.25,
        }
    }
}
impl MeshcraftApp {
    pub fn native_annotation_defaults(&self) -> (String, f64) {
        (
            if self.active_tool == "Linear Dimension" {
                self.annotations.dimension_label.clone()
            } else {
                self.annotations.text.clone()
            },
            self.annotations.size,
        )
    }
    pub fn native_selected_annotation(
        &self,
    ) -> Option<meshcraft_core::document::annotations::Annotation> {
        if self.active_tool != "Select" {
            return None;
        }
        self.document
            .annotations()
            .into_iter()
            .find(|a| self.selection.selected().contains(&a.target))
    }
    fn annotation_owner(
        &self,
    ) -> Result<
        (
            OccurrenceRef,
            meshcraft_core::document::DefinitionId,
            Affine3,
        ),
        String,
    > {
        let occurrence = match &self.selection.context {
            EditContext::Document => self
                .document
                .scene_container_occurrence()
                .unwrap_or_else(OccurrenceRef::scene_root),
            EditContext::Occurrence(p) => p.clone(),
            EditContext::Definition(id) => self
                .document
                .scene_geometry_refs()?
                .into_iter()
                .find(|p| {
                    self.document
                        .resolve_occurrence(p)
                        .is_some_and(|r| r.definition.id == *id)
                })
                .ok_or("annotation owner has no placement")?,
        };
        let r = self
            .document
            .resolve_occurrence(&occurrence)
            .ok_or("annotation owner is missing")?;
        let inverse = r
            .world_transform
            .inverse(GeometryTolerance::default())
            .ok_or("annotation owner transform is singular")?;
        Ok((occurrence, r.definition.id, inverse))
    }
    pub(super) fn annotation_click(&mut self, point: Vec3) {
        if !point.is_finite() {
            return;
        }
        let required = if self.active_tool == "Text" { 1 } else { 3 };
        self.annotations.points.push(point);
        if self.annotations.points.len() < required {
            self.status = self.native_point_input_hint().into();
            return;
        }
        let result = (|| {
            let (occurrence, definition, inverse) = self.annotation_owner()?;
            let kind = if self.active_tool == "Linear Dimension" {
                SelectionKind::Dimension
            } else {
                SelectionKind::Text
            };
            let target = SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::SemanticHandle {
                    kind,
                    id: String::new(),
                },
            };
            if !self
                .selection_display_policy
                .can_edit_target(&self.document, &target)
            {
                return Err("annotation type is hidden or read-only".into());
            }
            let p = self
                .annotations
                .points
                .iter()
                .map(|p| inverse.transform_point(*p))
                .collect::<Vec<_>>();
            let record = if kind == SelectionKind::Dimension {
                if p[0].distance(p[1]) < 1e-9 {
                    return Err("dimension endpoints must differ".into());
                }
                json!({"start": p[0], "end": p[1], "offset": p[2], "size": self.annotations.size, "text": self.annotations.dimension_label})
            } else {
                let mut record = json!({"position":p[p.len()-1], "text":self.annotations.text, "size":self.annotations.size,
                    "axisU": inverse.transform_vector(Vec3::X), "normal": inverse.transform_vector(Vec3::Y), "screenText":true});
                if self.active_tool == "Leader" {
                    record["leaderPoints"] = json!([p[0], p[1]]);
                }
                record
            };
            let id = self.commit_document_edit("Create annotation", |d| {
                d.add_annotation(
                    definition,
                    if kind == SelectionKind::Dimension {
                        "dimensions"
                    } else {
                        "texts"
                    },
                    record,
                )
            })?;
            self.selection.apply(
                [SelectionTarget {
                    occurrence,
                    entity: SelectedEntity::SemanticHandle { kind, id },
                }],
                SelectionMode::Replace,
            );
            Ok(())
        })();
        self.annotations.points.clear();
        self.status = match result {
            Ok(()) => format!(
                "{} created. Esc selects it for editing in Annotations.",
                self.active_tool
            ),
            Err(e) => e,
        };
    }
    pub(super) fn annotation_settings(&mut self, payload: &CommandPayload) -> CommandResult {
        let text = payload
            .values
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| self.native_annotation_defaults().0);
        if text.len() > 16_384 {
            return CommandResult::failed("annotation text exceeds 16 KB");
        }
        let size = payload
            .values
            .get("size")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(self.annotations.size);
        if !size.is_finite() || size <= 0. || size > 1e6 {
            return CommandResult::failed("text size must be positive and finite");
        }
        let selected = self
            .document
            .annotations()
            .into_iter()
            .filter(|a| {
                self.active_tool == "Select" && self.selection.selected().contains(&a.target)
            })
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            for a in &selected {
                if !self.native_target_editable(&a.target)
                    || self
                        .document
                        .resolve_occurrence(&a.target.occurrence)
                        .is_none_or(|r| match self.selection.context {
                            EditContext::Document => {
                                !a.target.occurrence.is_scene_root()
                                    && self.document.scene_container_occurrence().as_ref()
                                        != Some(&a.target.occurrence)
                            }
                            EditContext::Occurrence(ref p) => p != &a.target.occurrence,
                            EditContext::Definition(id) => id != r.definition.id,
                        })
                {
                    return CommandResult::failed(
                        "open the prototype to edit its inherited annotation",
                    );
                }
            }
            if selected.iter().any(|a| {
                a.record
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    != text
                    && payload.values.contains_key("text")
                    || (payload.values.contains_key("size")
                        && a.record.get("size").and_then(serde_json::Value::as_f64) != Some(size))
            }) && let Err(e) = self.commit_document_edit("Edit annotations", |d| {
                for a in &selected {
                    let mut record = a.record.clone();
                    if payload.values.contains_key("text") {
                        record["text"] = json!(text);
                    }
                    if payload.values.contains_key("size") {
                        record["size"] = json!(size);
                    }
                    d.update_annotation(&a.target, record)?;
                }
                Ok(())
            }) {
                return CommandResult::failed(e);
            }
        }
        if payload.values.contains_key("text") {
            if self.active_tool == "Linear Dimension"
                || selected.first().is_some_and(|a| {
                    matches!(
                        a.target.entity,
                        SelectedEntity::SemanticHandle {
                            kind: SelectionKind::Dimension,
                            ..
                        }
                    )
                })
            {
                self.annotations.dimension_label = text;
            } else {
                self.annotations.text = text;
            }
        }
        if payload.values.contains_key("size") {
            self.annotations.size = size;
        }
        self.status = "Annotation settings updated.".into();
        CommandResult::completed()
    }
    pub(super) fn annotation_preview(&self, point: Vec3) -> Option<ToolPreview> {
        let points = &self.annotations.points;
        if points.is_empty() {
            return Some(ToolPreview::Point { position: point });
        }
        if self.active_tool == "Linear Dimension" && points.len() == 2 {
            let [a, b] = meshcraft_core::document::annotations::dimension_points(
                points[0], points[1], point,
            )?;
            return Some(ToolPreview::Transform {
                edges: vec![[points[0], a], [points[1], b], [a, b]],
            });
        }
        let mut outline = points.clone();
        outline.push(point);
        Some(ToolPreview::Polyline {
            outline,
            closed: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimension_label_converts_units_without_changing_its_definition() {
        let mut app = super::super::tests::test_app();
        let settings = meshcraft_core::document::ModelSettings {
            unit_name: "cm".into(),
            unit_size: 30.,
            grid_size: 0.5,
            ..Default::default()
        };
        app.document.update_model_settings(settings).unwrap();
        app.run_command("tool.builtin.linear_dimension");
        for p in [Vec3::ZERO, Vec3::X * 0.5, Vec3::Z] {
            app.handle_native_world_pick(p, None, None);
        }
        assert_eq!("15.000 cm", app.document.annotations()[0].label);
    }

    #[test]
    fn annotation_tools_commit_preview_edit_copy_delete_and_undo() {
        let mut app = super::super::tests::test_app();
        app.run_command("tool.builtin.linear_dimension");
        assert_eq!("Linear Dimension", app.active_tool);
        for point in [Vec3::ZERO, Vec3::X * 3.] {
            app.handle_native_world_pick(point, None, None);
        }
        assert!(app.document.annotations().is_empty());
        assert!(
            matches!(app.annotation_preview(Vec3::Z), Some(ToolPreview::Transform { edges }) if edges.len()==3)
        );
        app.handle_native_world_pick(Vec3::Z, None, None);
        let dimension = app.document.annotations().pop().unwrap();
        assert_eq!(3, dimension.edges.len());
        assert!(dimension.label.starts_with("3.000"));
        app.handle_native_escape();
        let mut properties = CommandPayload::default();
        properties.values.insert("text".into(), json!("Door width"));
        properties.values.insert("size".into(), json!(0.4));
        assert_eq!(
            CommandCompletion::Completed,
            app.dispatch_command("annotation.update", &properties)
                .result
                .completion
        );
        assert_eq!("Door width", app.document.annotations()[0].label);
        let mut size_only = CommandPayload::default();
        size_only.values.insert("size".into(), json!(0.5));
        assert_eq!(
            CommandCompletion::Completed,
            app.dispatch_command("annotation.update", &size_only)
                .result
                .completion
        );
        assert_eq!("Door width", app.document.annotations()[0].label);
        app.run_command("edit.copy_selection");
        app.run_command("edit.paste_in_place");
        assert_eq!(2, app.document.annotations().len());
        app.run_command("edit.delete_selection");
        assert_eq!(1, app.document.annotations().len());
        app.run_command("edit.undo");
        assert_eq!(2, app.document.annotations().len());
        app.run_command("tool.builtin.text");
        app.handle_native_world_pick(Vec3::Z * 3., None, None);
        assert_eq!(3, app.document.annotations().len());
        app.run_command("tool.leader");
        for point in [Vec3::ZERO, Vec3::Z, Vec3::Z + Vec3::X] {
            app.handle_native_world_pick(point, None, None);
        }
        let leader = app.document.annotations().pop().unwrap();
        assert_eq!(2, leader.edges.len());
        assert!(leader.arrow.is_some());
        let encoded = serde_json::to_string(&app.document).unwrap();
        let restored: Document = serde_json::from_str(&encoded).unwrap();
        assert_eq!(4, restored.annotations().len());
    }
}
