use super::*;
use meshcraft_core::document::clipboard::ClipboardSelection;
use serde_json::Value;

#[derive(Default)]
pub(super) struct ClipboardState {
    pub text: String,
    pending: Option<String>,
    local: Option<(u64, String)>,
}
impl MeshcraftApp {
    pub(crate) fn native_set_clipboard(&mut self, text: String) {
        self.clipboard.text = text;
    }
    pub(crate) fn native_take_clipboard_write(&mut self) -> Option<String> {
        self.clipboard.pending.take()
    }

    pub(super) fn clipboard_action(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        if action == "paste" {
            let text = payload
                .values
                .get("clipboard")
                .and_then(Value::as_str)
                .unwrap_or(&self.clipboard.text)
                .to_owned();
            if text.starts_with("// Meshcraft Object Script") {
                let source = meshcraft_core::document::scripts::ScriptDefinition {
                    source: text,
                    ..Default::default()
                };
                return match self.create_native_scripted_object(source) {
                    Ok(_) => CommandResult::completed(),
                    Err(e) => CommandResult::failed(e),
                };
            }
            let snapshot: ClipboardSelection = match serde_json::from_str(&text) {
                Ok(s) => s,
                Err(_) => {
                    return CommandResult::failed(
                        "clipboard does not contain a Meshcraft selection",
                    );
                }
            };
            let reuse = self
                .clipboard
                .local
                .as_ref()
                .is_some_and(|(generation, original)| {
                    *generation == self.document_generation && original == &text
                });
            let delta = match payload_vec3(payload, "offset", Vec3::ZERO) {
                Ok(v) => v,
                Err(e) => return CommandResult::failed(e),
            };
            let context = self.selection.context.clone();
            return match self
                .commit_document_edit("Paste", |doc| snapshot.paste(doc, &context, delta, reuse))
            {
                Ok(targets) => {
                    self.selection.apply(targets, SelectionMode::Replace);
                    self.status = "Pasted selection. Use Move to position it.".into();
                    CommandResult::completed()
                }
                Err(e) => CommandResult::failed(e),
            };
        }
        let snapshot = match ClipboardSelection::capture(&self.document, &self.selection) {
            Ok(s) => s,
            Err(e) => return CommandResult::failed(e),
        };
        let text = if action == "script" {
            match selection_script(&self.document, &self.selection) {
                Ok(t) => t,
                Err(e) => return CommandResult::failed(e),
            }
        } else {
            match serde_json::to_string(&snapshot) {
                Ok(t) => t,
                Err(e) => return CommandResult::failed(e.to_string()),
            }
        };
        if action == "cut" {
            let result =
                self.execute_command(ShellAction::DeleteSelection, &CommandPayload::default());
            if result.completion != CommandCompletion::Completed {
                return result;
            }
        }
        self.clipboard.local = Some((self.document_generation, text.clone()));
        self.clipboard.pending = Some(text.clone());
        self.clipboard.text = text;
        self.status = if action == "script" {
            "Copied selection as an object script."
        } else if action == "cut" {
            "Cut selection to clipboard."
        } else {
            "Copied selection to clipboard."
        }
        .into();
        CommandResult::completed()
    }
}

fn selection_script(document: &Document, selection: &SelectionSession) -> Result<String, String> {
    let mut items = Vec::new();
    for occurrence in document.scene_geometry_refs()? {
        let whole = selection.selected().iter().any(|t| {
            t.entity == SelectedEntity::Occurrence
                && occurrence.path.starts_with(&t.occurrence.path)
        });
        let Some(r) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        let prefix = format!("copy:{:?}:", occurrence.path);
        let material_colors = document.material_colors();
        for mut drawable in meshcraft_core::document::scripts::geometry_drawables(geometry) {
            let id = drawable["id"].as_str().unwrap_or_default();
            let selected = whole
                || selection.selected().iter().any(|t| {
                    t.occurrence == occurrence
                        && match t.entity {
                            SelectedEntity::Segment(s) => id == format!("authored:segment:{}", s.0),
                            SelectedEntity::Triangle(f) => id == format!("authored:face:{}", f.0),
                            _ => false,
                        }
                });
            if !selected {
                continue;
            }
            // Use the currently evaluated/deformed geometry, with world placement baked in.
            if let Some(points) = drawable["points"].as_array_mut() {
                for point in points {
                    let p: Vec3 =
                        serde_json::from_value(point.clone()).map_err(|e| e.to_string())?;
                    let position = r.world_transform.transform_point(p);
                    let pid = format!("{prefix}{}", point["id"].as_str().unwrap_or_default());
                    *point = json!({"x": position.x, "y": position.y, "z": position.z, "id": pid});
                }
            }
            let material = if let Some(id) = drawable["id"]
                .as_str()
                .and_then(|s| s.strip_prefix("authored:segment:"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                geometry
                    .segments
                    .get(&meshcraft_core::document::SegmentId(id))
                    .and_then(|s| meshcraft_core::document::materials::material_id(&s.extensions))
            } else {
                drawable["id"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("authored:face:"))
                    .and_then(|s| s.parse::<u64>().ok())
                    .and_then(|id| {
                        geometry
                            .triangles
                            .get(&meshcraft_core::document::TriangleId(id))
                    })
                    .and_then(|f| meshcraft_core::document::materials::material_id(&f.extensions))
            };
            if let Some(color) = material.and_then(|id| material_colors.get(id)) {
                drawable["color"] = json!(color);
            }
            drawable["id"] = json!(format!(
                "{prefix}{}",
                drawable["id"].as_str().unwrap_or_default()
            ));
            items.push(drawable);
        }
    }
    if items.is_empty() {
        return Err("Copy as Script requires selected segments, faces or object geometry".into());
    }
    let geometry = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    Ok(format!(
        "// Meshcraft Object Script — selection snapshot\nfunction onCreate(instance, model) {{}}\n\nfunction onAnimationFrame(instance, model) {{\n  return {geometry};\n}}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_cut_paste_are_independent_undoable_and_export_executable_script() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        let original = app.selected_occurrences()[0].clone();
        let prototype = app
            .document
            .resolve_occurrence(&original)
            .unwrap()
            .definition
            .id;
        app.run_command("edit.copy_selection");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.paste");
        let copy = app.selected_occurrences()[0].clone();
        assert_ne!(original, copy);
        assert_eq!(
            prototype,
            app.document
                .resolve_occurrence(&copy)
                .unwrap()
                .definition
                .id
        );
        app.run_command("edit.cut_selection");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.paste");
        assert_eq!(2, app.document.occurrence_refs().unwrap().len());
        app.run_command("edit.undo");
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.set_selection([original]);
        app.run_command("edit.copy_as_script");
        let source = app.clipboard.text.clone();
        assert!(source.starts_with("// Meshcraft Object Script"));
        let engine = crate::script_engine::Session::new(&json!(null)).unwrap();
        let evaluated = engine.evaluate(&json!({"id":"copied","source":{"source":source}, "geometry":[],"hotspots":{},"time":0.,"deltaTime":0.016})).unwrap();
        let items: Vec<meshcraft_core::document::scripts::Drawable> =
            serde_json::from_value(evaluated["items"].clone()).unwrap();
        let frame = meshcraft_core::document::scripts::ScriptFrame::build(
            &meshcraft_core::document::GeometryBundle::default(),
            items,
            Default::default(),
        )
        .unwrap();
        assert_eq!(12, frame.geometry.triangles.len());
    }
}
