//! UI-owned automation adapters. Queries and edits use the same document/session as buttons.
use super::*;
use serde_json::{Value, json};

pub(crate) fn target_json(target: &SelectionTarget) -> Value {
    let (kind, id) = match &target.entity {
        SelectedEntity::Occurrence => ("occurrence", Value::Null),
        SelectedEntity::Vertex(id) => ("vertex", json!(id.0.to_string())),
        SelectedEntity::Segment(id) => ("segment", json!(id.0.to_string())),
        SelectedEntity::Triangle(id) => ("triangle", json!(id.0.to_string())),
        SelectedEntity::SemanticHandle { kind, id } => {
            return json!({"path":target.occurrence.path,"kind":"semantic_handle","semantic_kind":format!("{kind:?}").to_lowercase(),"id":id});
        }
    };
    json!({"path":target.occurrence.path,"kind":kind,"id":id})
}
fn path(value: &Value) -> Result<OccurrenceRef, String> {
    let values = value
        .as_array()
        .ok_or("path must be an array of layout-entry IDs")?;
    if values.len() > 128 {
        return Err("path exceeds 128 levels".into());
    }
    Ok(OccurrenceRef {
        path: values
            .iter()
            .map(|v| {
                v.as_u64()
                    .map(LayoutEntryId)
                    .ok_or("path IDs must be unsigned integers".to_owned())
            })
            .collect::<Result<_, _>>()?,
    })
}
fn primitive_id(value: &Value) -> Result<u64, String> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value.as_u64())
        .ok_or("entity id must be an unsigned integer or decimal string".into())
}
fn page(args: &Value, max: u64) -> Result<(usize, usize), String> {
    let integer = |key: &str, default| {
        args.get(key).map_or(Ok(default), |v| {
            v.as_u64()
                .ok_or_else(|| format!("{key} must be a nonnegative integer"))
        })
    };
    let offset = integer("offset", 0)?;
    let limit = integer("limit", 100)?;
    if !(1..=max).contains(&limit) || offset > usize::MAX as u64 {
        return Err(format!(
            "limit must be 1..{max}; offset must fit this platform"
        ));
    }
    Ok((offset as usize, limit as usize))
}
pub(crate) fn command_schema(command: &CommandDescriptor) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for parameter in &command.parameters {
        let mut spec = match parameter.value_type {
            CommandValueType::Boolean => json!({"type":"boolean"}),
            CommandValueType::Integer => json!({"type":"integer"}),
            CommandValueType::Number => json!({"type":"number"}),
            CommandValueType::String => json!({"type":"string"}),
            CommandValueType::Array => json!({"type":"array"}),
            CommandValueType::Object => json!({"type":"object"}),
            CommandValueType::Any => json!({}),
        };
        let name = parameter.name.as_str();
        spec["description"] = json!(match name {
            "path" => "Absolute local file path. Supplying it avoids an interactive dialog.",
            "paths" =>
                "Array of occurrence paths: each path is an array of numeric layout-entry IDs from meshcraft_scene.",
            "definition_id" =>
                "Numeric prototype ID from meshcraft_scene, distinct from occurrence/instance paths.",
            "clipboard" =>
                "Portable Meshcraft selection JSON or Copy as Script source. Use the copy result or OS clipboard when omitted.",
            "source" => "Unified JavaScript object source; see meshcraft_script_api.",
            "text" => "Annotation text; an empty dimension label uses its measured value.",
            "dimensions" | "origin" | "offset" | "position" | "target" =>
                "Three finite numbers [x,y,z], in world model units (Y up, +Z Front).",
            _ =>
                "Typed command argument; omitted optional fields retain command defaults or current settings.",
        });
        if parameter.value_type == CommandValueType::Array
            && matches!(
                name,
                "dimensions" | "origin" | "offset" | "position" | "target"
            )
        {
            spec["items"] = json!({"type":"number"});
            spec["minItems"] = json!(3);
            spec["maxItems"] = json!(3);
        }
        if command.id.as_str() == "selection.update_display" {
            if name == "kind" {
                spec["enum"] = json!([
                    "all",
                    "vertex",
                    "edge",
                    "face",
                    "object",
                    "voxel",
                    "hotspot",
                    "dimension",
                    "text",
                    "wall",
                    "slab",
                    "stair",
                    "frame",
                    "hvac"
                ]);
            }
            if name == "property" {
                spec["enum"] = json!(["visible", "editable", "wireframe"]);
            }
        }
        properties.insert(name.into(), spec);
        if parameter.required {
            required.push(name);
        }
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
impl MeshcraftApp {
    pub(crate) fn mcp_clipboard(&self) -> &str {
        &self.clipboard.text
    }
    pub(crate) fn native_mcp_port(&self) -> u16 {
        self.preferences.mcp_port
    }
    pub(crate) fn native_set_mcp_port(&mut self, port: u16) -> Result<(), String> {
        if port == 0 {
            return Err("Port must be 1–65535".into());
        }
        self.preferences.mcp_port = port;
        if self.persist_native_preferences {
            persist_native_preferences(&self.preferences)?;
        }
        Ok(())
    }
    pub(crate) fn mcp_status(&self) -> Value {
        let context = match &self.selection.context {
            EditContext::Document => json!({"kind":"document"}),
            EditContext::Definition(id) => json!({"kind":"definition","definition_id":id}),
            EditContext::Occurrence(p) => json!({"kind":"occurrence","path":p.path}),
        };
        json!({"version":env!("CARGO_PKG_VERSION"),"revision":self.document.revision(),"generation":self.document_generation,"activeTool":self.active_tool,"toolInstructions":self.native_point_input_hint(),"status":self.status,"editContext":context,"selection":self.selection.selected().iter().map(target_json).collect::<Vec<_>>(),"projectPath":self.current_project_path,"unsavedChanges":self.document.revision()!=self.saved_revision,"clipboard":{"available":!self.clipboard.text.is_empty(),"bytes":self.clipboard.text.len()}})
    }
    pub(crate) fn mcp_commands(&self, args: &Value) -> Value {
        let query = args["query"].as_str().unwrap_or("").to_lowercase();
        let category = args["category"].as_str().unwrap_or("");
        let commands=Self::command_catalog().into_iter().filter(|c| {
            (category.is_empty() || c.category.eq_ignore_ascii_case(category)) &&
            (query.is_empty() || format!("{} {} {:?}",c.id.as_str(),c.label,c.keywords).to_lowercase().contains(&query))
        }).map(|command|json!({"inputSchema":command_schema(&command),"state":self.query_command_state(command.id.as_str()),"command":command})).collect::<Vec<_>>();
        json!({"commands":commands,"revision":self.document.revision()})
    }
    pub(crate) fn mcp_scene(&self, args: &Value) -> Result<Value, String> {
        let (offset, limit) = page(args, 256)?;
        let paths = self.document.scene_geometry_refs()?;
        let objects=paths.iter().skip(offset).take(limit).filter_map(|p| {
            let r=self.document.resolve_occurrence(p)?;
            let g=self.document.effective_geometry(p)?;
            let bounds=meshcraft_core::geometry::Aabb3::from_points(g.vertices.values().map(|v|r.world_transform.transform_point(v.position)));
            Some(json!({"path":p.path,"definition_id":r.definition.id,"name":r.definition.name,"transform":r.world_transform,"bounds":bounds,"counts":{"vertices":g.vertices.len(),"segments":g.segments.len(),"triangles":g.triangles.len()},"target":target_json(&SelectionTarget {occurrence:p.clone(),entity:SelectedEntity::Occurrence})}))
        }).collect::<Vec<_>>();
        let prototypes=self.document.definitions().map(|d|json!({"id":d.id,"name":d.name,"kind":d.kind,"frame":d.frame,"childLayout":d.default_child_layout,"scripted":self.document.script(d.id).is_some()})).collect::<Vec<_>>();
        Ok(
            json!({"objects":objects,"total":paths.len(),"offset":offset,"nextOffset":if offset.saturating_add(limit)<paths.len(){Some(offset+limit)}else{None},"prototypes":prototypes,"materials":self.document.materials(),"settings":self.document.model_settings(),"status":self.mcp_status()}),
        )
    }
    pub(crate) fn mcp_geometry(&self, args: &Value) -> Result<Value, String> {
        let occurrence = path(&args["path"])?;
        let (offset, limit) = page(args, 1000)?;
        let r = self
            .document
            .resolve_occurrence(&occurrence)
            .ok_or("occurrence not found")?;
        let geometry = self
            .document
            .geometry_in_context(&occurrence, &self.selection.context)
            .ok_or("geometry not found")?;
        let mut referenced = BTreeSet::new();
        let mut primitives = Vec::new();
        for (record, entity, vertices) in geometry
            .segments
            .values()
            .map(|s| {
                (
                    json!(s),
                    SelectedEntity::Segment(s.id),
                    s.endpoints.to_vec(),
                )
            })
            .chain(geometry.triangles.values().map(|f| {
                (
                    json!(f),
                    SelectedEntity::Triangle(f.id),
                    f.vertices.to_vec(),
                )
            }))
            .skip(offset)
            .take(limit)
        {
            referenced.extend(vertices.into_iter().map(|v| v.id));
            let target = SelectionTarget {
                occurrence: occurrence.clone(),
                entity,
            };
            primitives.push(json!({"record":record,"target":target_json(&target),"editable":self.native_target_editable(&target)}));
        }
        let vertices=geometry.vertices.values().filter(|v|referenced.contains(&v.id)).map(|v|json!({"id":v.id.0.to_string(),"position":v.position,"target":target_json(&SelectionTarget {occurrence:occurrence.clone(),entity:SelectedEntity::Vertex(v.id)})})).collect::<Vec<_>>();
        let total = geometry.segments.len() + geometry.triangles.len();
        let mut result = json!({"path":occurrence.path,"definition_id":r.definition.id,"transform":r.world_transform,"primitives":primitives,"vertices":vertices,"total":total,"nextOffset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None},"hotspots":self.document.instance_hotspots(&occurrence)?,"scriptState":self.document.script_frame(&occurrence).map(|f|&f.state),"annotations":self.document.annotations().into_iter().filter(|a|a.target.occurrence==occurrence).map(|a|json!({"target":target_json(&a.target),"record":a.record,"editable":self.native_target_editable(&a.target)})).collect::<Vec<_>>()});
        // Stable generated IDs can exceed JavaScript's 53-bit integer precision.
        fn ids(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    for (key, v) in map {
                        if key == "id" && v.is_u64() {
                            *v = json!(v.as_u64().unwrap().to_string());
                        } else {
                            ids(v);
                        }
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        ids(v);
                    }
                }
                _ => {}
            }
        }
        ids(&mut result);
        Ok(result)
    }
    pub(crate) fn mcp_select(&mut self, args: &Value) -> Result<Value, String> {
        let list = args["targets"]
            .as_array()
            .ok_or("targets must be an array")?;
        if list.len() > 100_000 {
            return Err("selection exceeds 100000 targets".into());
        }
        let mode = match args["mode"].as_str().unwrap_or("replace") {
            "replace" => SelectionMode::Replace,
            "add" => SelectionMode::Add,
            "remove" => SelectionMode::Remove,
            _ => return Err("mode must be replace, add or remove".into()),
        };
        let view = self.native_document_for_view();
        let mut targets = Vec::new();
        for target in list {
            let occurrence = path(&target["path"])?;
            let entity = match target["kind"].as_str().ok_or("target kind is missing")? {
                "occurrence" => SelectedEntity::Occurrence,
                "vertex" => SelectedEntity::Vertex(VertexId(primitive_id(&target["id"])?)),
                "segment" => SelectedEntity::Segment(meshcraft_core::document::SegmentId(
                    primitive_id(&target["id"])?,
                )),
                "triangle" => SelectedEntity::Triangle(TriangleId(primitive_id(&target["id"])?)),
                "semantic_handle" => SelectedEntity::SemanticHandle {
                    kind: selection_kind_from_name(
                        target["semantic_kind"]
                            .as_str()
                            .ok_or("semantic_kind required")?,
                    )
                    .ok_or("unknown semantic kind")?,
                    id: target["id"]
                        .as_str()
                        .ok_or("semantic id must be a string")?
                        .into(),
                },
                _ => return Err("unknown selection target kind".into()),
            };
            let target = SelectionTarget { occurrence, entity };
            if !self.native_target_editable(&target) {
                return Err(
                    "target is missing, hidden, outside the edit context or read-only".into(),
                );
            }
            let mut valid = SelectionSession::default();
            valid.select(target.clone());
            valid.retain_valid(&view);
            if valid.selected().is_empty() {
                return Err("target does not exist".into());
            }
            targets.push(target);
        }
        self.selection.apply(targets, mode);
        self.status = format!("MCP selected {} elements.", self.selection.selected().len());
        Ok(self.mcp_status())
    }
    pub(crate) fn mcp_pointer(&mut self, args: &Value) -> Result<Value, String> {
        match args["action"].as_str().ok_or("action required")? {
            "cancel" => self.handle_native_escape(),
            "finish" => {
                let d = self.dispatch_command("tool.finish", &CommandPayload::default());
                if d.result.completion == CommandCompletion::Failed {
                    return Err(d.result.diagnostics.join("; "));
                }
            }
            "text" => {
                let text = args["text"].as_str().ok_or("text required")?;
                if text.len() > 256 {
                    return Err("tool input exceeds 256 characters".into());
                }
                if self.transform_tool_state.active().is_some() {
                    self.transform_tool_state.add_numeric_text(text);
                } else if self.revolve_tool_state.active() {
                    self.revolve_tool_state.add_numeric_text(text);
                } else if self.active_tool == "Push/Pull" {
                    self.push_pull_numeric_input.push_str(text);
                } else if matches!(self.active_tool.as_str(), "Line" | "Construction Line") {
                    self.drawing_tool.handle(ToolEvent::Text(text));
                } else {
                    return Err("current tool does not accept numeric text; annotation text uses annotation.update".into());
                }
            }
            action @ ("click" | "move") => {
                let number = |key| {
                    args.get(key)
                        .and_then(Value::as_f64)
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| format!("{key} must be a finite number"))
                };
                let p = Vec3::new(number("worldX")?, number("worldY")?, number("worldZ")?);
                let snap = SnapResult {
                    point: p,
                    normal: Vec3::Y,
                    kind: SnapKind::Free,
                    screen_distance: 0.,
                };
                self.cursor_snap = Some(snap);
                if action == "click" {
                    if self.active_tool == "Select" {
                        return Err("use meshcraft_select with exact target IDs".into());
                    }
                    self.handle_native_world_pick(p, Some(snap), None);
                } else {
                    self.native_tool_feedback_for_snap(snap, egui::Pos2::ZERO);
                }
            }
            _ => return Err("unknown pointer action".into()),
        }
        Ok(self.mcp_status())
    }
    pub(crate) fn mcp_script(&mut self, args: &Value) -> Result<Value, String> {
        match args["action"].as_str().ok_or("action required")? {
            "get" => {
                let id = DefinitionId(
                    args["definition_id"]
                        .as_u64()
                        .ok_or("definition_id required")?,
                );
                let d = self.document.definition(id).ok_or("prototype not found")?;
                Ok(
                    json!({"definition_id":id,"name":d.name,"script":self.document.script(id),"api":"meshcraft://scripting/api"}),
                )
            }
            action @ ("set" | "create") => {
                let source = args["source"].as_str().ok_or("source required")?;
                if source.len() > 256 * 1024 {
                    return Err("script source exceeds 256 KiB".into());
                }
                let script = meshcraft_core::document::scripts::ScriptDefinition {
                    source: source.into(),
                    ..Default::default()
                };
                let id = if action == "create" {
                    self.create_native_scripted_object(script)?
                } else {
                    let id = DefinitionId(
                        args["definition_id"]
                            .as_u64()
                            .ok_or("definition_id required")?,
                    );
                    self.save_native_script(id, script)?;
                    id
                };
                Ok(
                    json!({"definition_id":id,"revision":self.document.revision(),"status":self.mcp_status(),"evaluation":"The existing asynchronous script worker evaluates the new source; inspect scene/geometry and script errors for generated output."}),
                )
            }
            _ => Err("action must be create, get or set".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_world_picks_queries_selection_and_undo_share_live_model() {
        let mut app = super::super::tests::test_app();
        let catalog = app.mcp_commands(&json!({"query":"line"}));
        assert!(
            catalog["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["command"]["id"] == "tool.builtin.line")
        );
        assert_eq!(
            CommandCompletion::Completed,
            app.dispatch_command("tool.builtin.line", &Default::default())
                .result
                .completion
        );
        for p in [
            json!({"action":"click","worldX":0,"worldY":0,"worldZ":0}),
            json!({"action":"click","worldX":3,"worldY":0,"worldZ":0}),
        ] {
            app.mcp_pointer(&p).unwrap();
        }
        app.mcp_pointer(&json!({"action":"cancel"})).unwrap();
        let scene = app.mcp_scene(&json!({})).unwrap();
        let path = scene["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["counts"]["segments"].as_u64().unwrap() > 0)
            .unwrap()["path"]
            .clone();
        let geometry = app.mcp_geometry(&json!({"path":path})).unwrap();
        let target = geometry["primitives"][0]["target"].clone();
        let state = app.mcp_select(&json!({"targets":[target]})).unwrap();
        assert_eq!(1, state["selection"].as_array().unwrap().len());
        app.dispatch_command("edit.delete_selection", &Default::default());
        assert_eq!(0, app.mcp_geometry(&json!({"path":path})).unwrap()["total"]);
        app.dispatch_command("edit.undo", &Default::default());
        assert_eq!(1, app.mcp_geometry(&json!({"path":path})).unwrap()["total"]);
        assert!(
            app.mcp_select(&json!({"targets":[{"path":[],"kind":"segment","id":"999999999999"}]}))
                .is_err()
        );
        let script=app.mcp_script(&json!({"action":"create","source":"function onAnimationFrame(){return cuboid(new Point(),new Point(2,3,4));}"})).unwrap();
        let source = app
            .mcp_script(&json!({"action":"get","definition_id":script["definition_id"]}))
            .unwrap();
        assert!(
            source["script"]["source"]
                .as_str()
                .unwrap()
                .contains("cuboid")
        );
    }
}
