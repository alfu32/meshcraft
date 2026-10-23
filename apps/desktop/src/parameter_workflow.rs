//! Headless parameter inspection and transactional, selection-wide typed edits.
use super::*;
use meshcraft_core::document::scripts::parameters::{Declaration, Kind, Scope};
use serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ParameterRow {
    #[serde(rename = "definition_id")]
    pub definition: DefinitionId,
    #[serde(flatten)]
    pub declaration: Declaration,
    #[serde(serialize_with = "serialize_paths")]
    pub paths: Vec<OccurrenceRef>,
    pub value: Option<Value>,
}
fn serialize_paths<S: serde::Serializer>(
    paths: &[OccurrenceRef],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(
        &paths.iter().map(|p| &p.path).collect::<Vec<_>>(),
        serializer,
    )
}
impl MeshcraftApp {
    pub fn native_script_parameters(&self) -> Vec<ParameterRow> {
        let Some((definition, _, editing)) = self.native_active_object() else {
            return vec![];
        };
        let paths = self
            .selected_occurrences()
            .into_iter()
            .filter(|p| {
                self.document
                    .resolve_occurrence(p)
                    .is_some_and(|r| r.definition.id == definition)
            })
            .collect::<Vec<_>>();
        let mut result = vec![];
        for d in self
            .document
            .script_parameter_declarations(definition, paths.first())
        {
            if d.scope == Scope::Instance && (editing || paths.is_empty()) {
                continue;
            }
            let targets = if d.scope == Scope::Instance {
                paths.clone()
            } else {
                vec![]
            };
            let read = |path: Option<&OccurrenceRef>| {
                if d.kind == Kind::Hotspot {
                    path.and_then(|p| self.document.occurrence_edits(p))
                        .and_then(|e| e.hotspot_positions.get(&d.id))
                        .map(|p| json!(p))
                        .unwrap_or_else(|| d.default_value.clone())
                } else {
                    self.document
                        .script_parameter_values(definition, path)
                        .get(&d.id)
                        .filter(|v| d.validate_value(v).is_ok())
                        .cloned()
                        .or_else(|| {
                            path.and_then(|p| self.document.initialized_script_parameter(p, &d.id))
                                .filter(|v| d.validate_value(v).is_ok())
                        })
                        .unwrap_or_else(|| d.default_value.clone())
                }
            };
            let value = if targets.is_empty() {
                Some(read(None))
            } else {
                let first = read(targets.first());
                targets
                    .iter()
                    .all(|p| d.same_value(&read(Some(p)), &first))
                    .then_some(first)
            };
            result.push(ParameterRow {
                definition,
                declaration: d,
                paths: targets,
                value,
            });
        }
        result
    }
    pub fn set_native_script_parameter(
        &mut self,
        definition: DefinitionId,
        paths: Vec<OccurrenceRef>,
        id: &str,
        value: Value,
    ) -> Result<(), String> {
        if paths.iter().any(|p| {
            !self.native_target_editable(&SelectionTarget {
                occurrence: p.clone(),
                entity: SelectedEntity::Occurrence,
            })
        }) {
            return Err(
                "A selected parameter target is read-only or outside the edit context".into(),
            );
        }
        self.commit_global_document_edit("Edit script parameter", |document| {
            if paths.is_empty() {
                document.set_script_parameter(definition, None, id, value)?;
            } else {
                for path in &paths {
                    document.set_script_parameter(definition, Some(path), id, value.clone())?;
                }
            }
            Ok(())
        })?;
        self.status = format!("Updated parameter {id}.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parameter_forms_apply_to_all_matching_instances_and_undo_atomically() {
        let mut app = super::super::tests::test_app();
        let first = app
            .document
            .add_cuboid("prototype", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let id = app
            .document
            .resolve_occurrence(&first)
            .unwrap()
            .definition
            .id;
        let d = Declaration {
            id: "width".into(),
            label: "Width".into(),
            scope: Scope::Instance,
            kind: Kind::Number,
            default_value: json!(3),
            options: Default::default(),
            hotspot_style: None,
        };
        app.document
            .set_script_parameter_declarations(id, vec![d])
            .unwrap();
        let second = app
            .document
            .copy_root_occurrence(&first, Vec3::X * 3.)
            .unwrap();
        app.set_selection([first.clone(), second.clone()]);
        let rows = app.native_script_parameters();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].paths.len(), 2);
        app.set_native_script_parameter(id, rows[0].paths.clone(), "width", json!(7))
            .unwrap();
        for p in [&first, &second] {
            assert_eq!(
                app.document.script_parameter_values(id, Some(p))["width"],
                7
            );
        }
        let cursor = app.history.cursor();
        assert!(
            app.set_native_script_parameter(id, rows[0].paths.clone(), "width", json!("bad"))
                .is_err()
        );
        assert_eq!(cursor, app.history.cursor());
        app.run_command("edit.undo");
        assert!(
            app.document
                .script_parameter_values(id, Some(&first))
                .is_empty()
        );
        assert!(
            app.document
                .script_parameter_values(id, Some(&second))
                .is_empty()
        );
    }
}
