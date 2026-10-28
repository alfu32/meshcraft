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
            "unit"
                if matches!(
                    command.id.as_str(),
                    "import.mesh" | "export.mesh" | "export.ifc_model"
                ) =>
                "Physical import/output unit override. Otherwise use embedded metadata or the filename suffix. glTF always uses meter coordinates. See interchange.formats.",
            "scale"
                if matches!(
                    command.id.as_str(),
                    "import.mesh" | "export.mesh" | "export.ifc_model"
                ) =>
                "Positive additional coordinate multiplier, applied once independently of physical unit conversion.",
            "selection_only" =>
                "Export only the selection; defaults to whether anything is selected. Set false for the full scene, including hidden geometry.",
            "definition_id" =>
                "Numeric prototype ID from meshcraft_scene, distinct from occurrence/instance paths.",
            "black_model_lines" =>
                "Application display preference: true (default) renders model lines black; false uses their material colors. Does not modify authored materials or history.",
            "confirmed" =>
                "For object.delete_definition, true explicitly confirms deleting the prototype and ALL its instances. Omit first to inspect reference counts and request confirmation.",
            "clipboard" =>
                "Portable Meshcraft selection JSON or Copy as Script source. Use the copy result or OS clipboard when omitted.",
            "source" => "Unified JavaScript object source; see meshcraft_script_api.",
            "text" => "Annotation text; an empty dimension label uses its measured value.",
            "dimensions" | "origin" | "offset" | "position" | "target" =>
                "Three finite numbers [x,y,z], in world model units (Y up, +Z Front).",
            _ =>
                "Typed command argument; omitted optional fields retain command defaults or current settings.",
        });
        if parameter.value_type == CommandValueType::Object
            && matches!(name, "position" | "target" | "origin" | "offset")
        {
            spec["description"] = json!(if command.id.as_str() == "hotspot.target" {
                "Point object {x,y,z} in the selected instance's LOCAL model coordinates. Finish prototype editing before operating an instance hotspot."
            } else {
                "Point object {x,y,z} in WORLD model units (Y up, +Z Front)."
            });
            spec["properties"] =
                json!({"x":{"type":"number"},"y":{"type":"number"},"z":{"type":"number"}});
            spec["required"] = json!(["x", "y", "z"]);
        }
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
        if command.id.as_str().starts_with("architecture.") {
            if matches!(name, "contour" | "walking_path") {
                spec["description"] = json!(if name == "contour" {
                    "Closed planar stair boundary C in world coordinates; omit a repeated closing point. Source linework is retained."
                } else {
                    "Open planar walking path P in uphill order, on the contour plane. Path direction determines ascent and left/right sides."
                });
            } else if name.ends_with("_material") {
                spec["description"] = json!(
                    "Project material ID, conventional or procedural; bind to this architectural surface and preserve through regeneration."
                );
            }
            match name {
                "stair_height" => {
                    spec["description"] = json!(
                        "Total stair rise in internal model units; new stairs default to 10."
                    );
                    spec["exclusiveMinimum"] = json!(0);
                }
                "stair_steps" => {
                    spec["minimum"] = json!(1);
                    spec["maximum"] = json!(512);
                }
                "stair_levels" => {
                    spec["minimum"] = json!(0);
                    spec["maximum"] = json!(128);
                    spec["description"] =
                        json!("Number of copied guardrail guide levels; 0 hides guides.");
                }
                "stair_spacing" | "stair_support" => {
                    spec["exclusiveMinimum"] = json!(0);
                }
                _ => {}
            }
        }
        if name == "unit"
            && matches!(
                command.id.as_str(),
                "import.mesh" | "export.mesh" | "export.ifc_model"
            )
        {
            spec["enum"] = json!([
                "micron",
                "millimeter",
                "centimeter",
                "meter",
                "inch",
                "foot",
                "unitless"
            ]);
        }
        if name == "scale"
            && matches!(
                command.id.as_str(),
                "import.mesh" | "export.mesh" | "export.ifc_model"
            )
        {
            spec["exclusiveMinimum"] = json!(0);
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
        if command.id.as_str() == "object.configure_procedural" {
            spec["description"] = json!(match name {
                "definition_id" =>
                    "Existing procedural prototype ID; omit to create a tree prototype and start placement.",
                "instance_path" =>
                    "Optional numeric occurrence path. Supplied settings become a private instance override; omit to change prototype defaults.",
                "settings" =>
                    "Full Material-shaped shader settings record (name/color/pbr/procedural). procedural.program=meshcraft.tree.v1, parameters use the tree schema in meshcraft_scene.proceduralPrograms. No mesh triangles are generated on the CPU.",
                _ => "",
            });
        }
        if command.id.as_str() == "material.program" && name == "program" {
            spec["description"] = json!(
                "Procedural Program: id, name, generation=surface|blades|foliage|surface_grid|tree, backend=vertex_instancing_gl33, parameters=[{id,kind,default,label,description,min,max,step,unit,options}], vertex GLSL proceduralVertex(inout vec3 position,inout vec3 normal,vec2 uv), fragment GLSL vec4 proceduralColor(vec4 color,vec3 position,vec3 normal,vec2 uv), boundsExpansion, animated, transparent. Typed uniforms u_<id>, time in seconds; localPosition/sourceNormal varyings anchor material patterns. Optional vec3 proceduralShadingNormal(vec3 normal,vec3 position,vec2 uv) changes shading relief. tree programs belong to independent objects; surface_grid subdivides on the GPU. See meshcraft_scene.proceduralPrograms for builtins."
            );
        }
        if command.id.as_str() == "material.save" && name == "material" {
            spec["description"] = json!(
                "Material: id/name/color RGBA, optional pbr={roughness,metallic}, texturePath/textureScaleU/V, procedural={program,parameters,baseMaterial,lodDistance,maxInstances,showBounds}. Material defaults and per-assignment overrides resolve against program parameter definitions; unsupported programs retain a conventional fallback."
            );
        }
        if command.id.as_str() == "material.parameters" && name == "parameters" {
            spec["description"] = json!(
                "Parameter override object keyed by program parameter IDs, e.g. {height:2,seed:42}. Applies the material and values to editable selected surfaces/instances, preserving siblings and canonical architectural owners. Use material.apply to clear overrides and inherit material defaults."
            );
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
        let pending_dialog=self.pending_replacement.as_ref().map(|p|json!({"job_id":p.job_id,"kind":match &p.target {ReplacementTarget::NewDocument=>"new",ReplacementTarget::OpenProject(_)=>"open"},"choices":["save","discard","cancel"],"instructions":"Use meshcraft_resolve_document explicitly; save needs a path when the document is unnamed."}));
        json!({"prototypeDeletion":self.native_prototype_deletion_prompt().map(|p|json!({"definition_id":p.definition_id.0,"name":p.name,"references":p.references,"instances":p.instances,"message":p.message()})),"pendingDialog":pending_dialog,"history":{"cursor":self.history.cursor(),"entries":self.history.entries().len()},"version":env!("CARGO_PKG_VERSION"),"revision":self.document.revision(),"generation":self.document_generation,"activeTool":self.active_tool,"numericInput":self.tool_numeric_text(),"toolInstructions":self.native_point_input_hint(),"status":self.status,"editContext":context,"selection":self.selection.ordered_selected().iter().map(target_json).collect::<Vec<_>>(),"projectPath":self.current_project_path(),"unsavedChanges":self.document.revision()!=self.saved_revision,"clipboard":{"available":!self.clipboard.text.is_empty(),"bytes":self.clipboard.text.len()}})
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
            let bounds=self.document.procedural_object_world_bounds(p).or_else(||meshcraft_core::geometry::Aabb3::from_points(g.vertices.values().map(|v|r.world_transform.transform_point(v.position))));
            Some(json!({"path":p.path,"is_scene_root":p.is_scene_root()||self.document.scene_container_occurrence().as_ref()==Some(p),"definition_id":r.definition.id,"name":r.definition.name,"transform":r.world_transform,"bounds":bounds,"counts":{"vertices":g.vertices.len(),"segments":g.segments.len(),"triangles":g.triangles.len()},"proceduralObject":self.document.procedural_object(p),"classification":if self.document.is_baked_gpu_object(p){"gpu_mesh"}else if self.document.procedural_object(p).is_some(){"procedural_object"}else if self.document.frame_target_for_occurrence(p).is_some(){"frame"}else if self.document.is_object_occurrence(p){"object"}else{"scene"},"target":target_json(&self.document.frame_target_for_occurrence(p).unwrap_or(SelectionTarget {occurrence:p.clone(),entity:SelectedEntity::Occurrence}))}))
        }).collect::<Vec<_>>();
        let prototypes=self.document.definitions().filter(|d|!self.document.is_frame_definition(d.id)).map(|d|json!({"id":d.id,"name":d.name,"kind":d.kind,"frame":d.frame,"childLayout":d.default_child_layout,"scripted":self.document.script(d.id).is_some(),"proceduralObject":self.document.procedural_object_definition(d.id)})).collect::<Vec<_>>();
        Ok(
            json!({"objects":objects,"total":paths.len(),"offset":offset,"nextOffset":if offset.saturating_add(limit)<paths.len(){Some(offset+limit)}else{None},"prototypes":prototypes,"materials":self.document.materials(),"proceduralPrograms":self.document.procedural_programs(),"settings":self.document.model_settings(),"status":self.mcp_status()}),
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
            let entity = record
                .get("extensions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .and_then(|ext| meshcraft_core::document::architecture::semantic_entity(&ext))
                .unwrap_or(entity);
            let target = self.native_normalize_selection_target(SelectionTarget {
                occurrence: occurrence.clone(),
                entity,
            });
            primitives.push(json!({"record":record,"target":target_json(&target),"editable":self.native_target_editable(&target)}));
        }
        let vertices=geometry.vertices.values().filter(|v|referenced.contains(&v.id)).map(|v|json!({"id":v.id.0.to_string(),"position":v.position,"target":target_json(&SelectionTarget {occurrence:occurrence.clone(),entity:SelectedEntity::Vertex(v.id)})})).collect::<Vec<_>>();
        let total = geometry.segments.len() + geometry.triangles.len();
        let mut result = json!({"path":occurrence.path,"definition_id":r.definition.id,"transform":r.world_transform,"primitives":primitives,"vertices":vertices,"total":total,"nextOffset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None},"hotspots":self.document.instance_hotspots(&occurrence)?,"scriptState":self.document.script_frame(&occurrence).map(|f|&f.state),"annotations":self.document.annotations().into_iter().filter(|a|a.target.occurrence==occurrence).map(|a|json!({"target":target_json(&a.target),"record":a.record,"editable":self.native_target_editable(&a.target)})).collect::<Vec<_>>()});
        result["architecture"] = json!(self.document.architecture_elements().into_iter().filter(|e|e.target.occurrence==occurrence).map(|e|json!({"target":target_json(&e.target),"record":e.element.value(),"controls":e.element.controls().unwrap_or_default(),"editable":self.native_architecture_editable(&e.target)})).collect::<Vec<_>>());
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
            let target = if target.entity == SelectedEntity::Occurrence
                && self
                    .document
                    .frame_target_for_occurrence(&target.occurrence)
                    .is_some()
            {
                self.native_normalize_selection_target(target)
            } else {
                target
            };
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
                if !self.append_tool_numeric_text(text) {
                    return Err("current tool does not accept numeric text; annotation text uses annotation.update".into());
                }
            }
            "backspace" => {
                self.run_command("tool.undo_point");
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
                    if self.active_tool == "Select"
                        && !self.tool_cycle_state.hotspot_placement_active()
                        && !self.hotspot_reference_pick
                    {
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
                let previous_version = args["definition_id"]
                    .as_u64()
                    .and_then(|id| self.document.script(DefinitionId(id)))
                    .map(|script| u64::from(script.api_version))
                    .unwrap_or(1);
                let api_version = args
                    .get("api_version")
                    .and_then(Value::as_u64)
                    .unwrap_or(if action == "set" { previous_version } else { 1 });
                if !matches!(api_version, 1 | 2) {
                    return Err("api_version must be 1 or 2".into());
                }
                let script = meshcraft_core::document::scripts::ScriptDefinition {
                    api_version: api_version as u32,
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
    fn world_points_work_for_hotspot_placement_and_reference_while_select_is_active() {
        let mut app = super::super::tests::test_app();
        let click = json!({"action":"click","worldX":0,"worldY":0,"worldZ":0});
        assert!(app.mcp_pointer(&click).is_err());
        let result = app.dispatch_command("hotspot.begin_placement", &CommandPayload::default());
        assert!(result.accepted);
        app.mcp_pointer(&click).unwrap();
        assert_eq!(app.history.cursor(), 1);
        assert_eq!(app.active_tool, "Select");
        let result = app.dispatch_command("hotspot.reference", &CommandPayload::default());
        assert!(result.accepted);
        app.mcp_pointer(&json!({"action":"click","worldX":1,"worldY":0,"worldZ":0}))
            .unwrap();
        assert!(!app.hotspot_reference_pick);
        assert_eq!(app.history.cursor(), 2);
    }
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
        let id = script["definition_id"].clone();
        app.mcp_script(&json!({"action":"set","definition_id":id,"api_version":2,"source":"function onAnimationFrame(params){return [];}"}))
            .unwrap();
        app.mcp_script(&json!({"action":"set","definition_id":id,"source":"function onAnimationFrame(params){return this.geometry;}"}))
            .unwrap();
        assert_eq!(
            app.mcp_script(&json!({"action":"get","definition_id":id}))
                .unwrap()["script"]["apiVersion"],
            2,
            "omitting api_version when editing preserves the existing contract"
        );
    }
}

#[derive(Default)]
pub(super) struct Jobs {
    pending: Option<ScriptJob>,
    recent: std::collections::VecDeque<String>,
}
struct ScriptJob {
    id: String,
    generation: u64,
    revision: u64,
    context: EditContext,
    name: String,
    position: Vec3,
    commit: bool,
    receiver:
        std::sync::mpsc::Receiver<Result<meshcraft_core::document::scripts::ScriptFrame, String>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Jobs {
    fn drop(&mut self) {
        if let Some(job) = &self.pending {
            job.cancel.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}
impl MeshcraftApp {
    pub(crate) fn mcp_run_script(&mut self, args: &Value) -> Result<Value, String> {
        if self.mcp_jobs.pending.is_some() {
            return Err(
                "a one-shot script is already running; poll its job ID or cancel it".into(),
            );
        }
        let source = args["source"].as_str().ok_or("source required")?;
        if source.len() > 256 * 1024 {
            return Err("script source exceeds 256 KiB".into());
        }
        let api_version = args.get("api_version").and_then(Value::as_u64).unwrap_or(1);
        if !matches!(api_version, 1 | 2) {
            return Err("api_version must be 1 or 2".into());
        }
        let mut definition = meshcraft_core::document::scripts::ScriptDefinition {
            api_version: api_version as u32,
            ..Default::default()
        };
        match args["source_type"].as_str().unwrap_or("body") {
            "body" => definition.on_animation_frame = source.into(),
            "module" => definition.source = source.into(),
            _ => return Err("source_type must be body or module".into()),
        }
        let name = args["name"]
            .as_str()
            .unwrap_or("Agent Geometry")
            .trim()
            .to_owned();
        if name.is_empty() || name.len() > 256 {
            return Err("name must contain 1–256 UTF-8 bytes".into());
        }
        let position = args
            .get("position")
            .map(|v| payload_vec3_value(v, "position"))
            .transpose()?
            .unwrap_or(Vec3::ZERO);
        let properties = args.get("properties").cloned().unwrap_or(json!({}));
        if properties.to_string().len() > 65536 {
            return Err("properties exceed 64 KiB".into());
        }
        let properties = properties
            .as_object()
            .ok_or("properties must be an object")?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let commit = args["commit"].as_bool().unwrap_or(true);
        self.document.persist_script_states();
        let document = std::sync::Arc::new(self.document.clone());
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = std::sync::Arc::clone(&cancel);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let id = format!("mcp-script-{}", self.next_job_id);
        self.next_job_id = self.next_job_id.saturating_add(1);
        std::thread::Builder::new()
            .name(id.clone())
            .spawn(move || {
                let result = crate::script_runtime::evaluate_once(
                    document,
                    definition,
                    properties,
                    worker_cancel,
                );
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        let mut result = CommandResult::completed();
        result.completion = CommandCompletion::Pending { job_id: id.clone() };
        result.document_revision = Some(self.document.revision());
        self.job_results.insert(id.clone(), result.clone());
        self.mcp_jobs.recent.push_back(id.clone());
        while self.mcp_jobs.recent.len() > 128 {
            if let Some(old) = self.mcp_jobs.recent.pop_front() {
                self.job_results.remove(&old);
            }
        }
        self.mcp_jobs.pending = Some(ScriptJob {
            id: id.clone(),
            generation: self.document_generation,
            revision: self.document.revision(),
            context: self.selection.context.clone(),
            name,
            position,
            commit,
            receiver,
            cancel,
        });
        self.status = format!("Running one-shot script {id}.");
        Ok(
            json!({"job_id":id,"result":result,"next":"Poll meshcraft_get_job; do not repeat the script to check completion."}),
        )
    }
    pub(crate) fn cancel_mcp_script(&mut self) -> Result<Value, String> {
        let Some(job) = self.mcp_jobs.pending.take() else {
            return Err("no one-shot script is running".into());
        };
        job.cancel.store(true, std::sync::atomic::Ordering::Release);
        let result = CommandResult::failed("One-shot script canceled; no geometry committed");
        self.job_results.insert(job.id.clone(), result.clone());
        self.status = "One-shot script canceled.".into();
        Ok(json!({"job_id":job.id,"result":result}))
    }
    pub(super) fn poll_mcp_script(&mut self) {
        let Some(job) = self.mcp_jobs.pending.as_ref() else {
            return;
        };
        let frame = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("script worker disconnected".into())
            }
        };
        let job = self.mcp_jobs.pending.take().unwrap();
        let result = if job.generation != self.document_generation
            || job.revision != self.document.revision()
            || job.context != self.selection.context
        {
            Err("Document or edit context changed during script execution; no geometry committed. Inspect current state before rerunning.".into())
        } else {
            frame.and_then(|frame| self.install_mcp_script_frame(&job, frame))
        };
        let result = match result {
            Ok(result) => {
                self.status = if job.commit {
                    "One-shot script geometry committed."
                } else {
                    "One-shot script evaluated without changing the model."
                }
                .into();
                result
            }
            Err(error) => {
                let error = error.chars().take(4096).collect::<String>();
                self.status = error.clone();
                CommandResult::failed(error)
            }
        };
        self.job_results.insert(job.id, result);
    }
    fn install_mcp_script_frame(
        &mut self,
        job: &ScriptJob,
        frame: meshcraft_core::document::scripts::ScriptFrame,
    ) -> Result<CommandResult, String> {
        let mut result = CommandResult::completed();
        result.outputs.insert("counts".into(),json!({"vertices":frame.geometry.vertices.len(),"segments":frame.geometry.segments.len(),"triangles":frame.geometry.triangles.len(),"hotspots":frame.hotspots.len()}));
        result.outputs.insert(
            "bounds".into(),
            json!(meshcraft_core::geometry::Aabb3::from_points(
                frame
                    .geometry
                    .vertices
                    .values()
                    .map(|v| v.position + job.position)
            )),
        );
        if serde_json::to_vec(&frame.state.properties)
            .map_err(|e| e.to_string())?
            .len()
            <= 65536
        {
            result
                .outputs
                .insert("properties".into(), json!(frame.state.properties));
        } else {
            result.outputs.insert("properties".into(), json!({}));
            result
                .outputs
                .insert("propertiesOmitted".into(), json!(true));
            result.diagnostics.push(
                "Final script properties exceed 64 KiB and were omitted from the job summary."
                    .into(),
            );
        }
        result.outputs.insert("committed".into(), json!(job.commit));
        if job.commit {
            let snapshot = baked_snapshot(&job.name, &frame)?;
            let targets = self.commit_document_edit("One-shot script", |document| {
                snapshot.paste(document, &job.context, job.position, false)
            })?;
            result.outputs.insert(
                "targets".into(),
                json!(targets.iter().map(target_json).collect::<Vec<_>>()),
            );
            self.cancel_active_tool();
            self.selection.apply(targets, SelectionMode::Replace);
        }
        result.document_revision = Some(self.document.revision());
        Ok(result)
    }
}
fn baked_snapshot(
    name: &str,
    frame: &meshcraft_core::document::scripts::ScriptFrame,
) -> Result<meshcraft_core::document::clipboard::ClipboardSelection, String> {
    use meshcraft_core::document::{
        DefinitionKind, RawGeometrySegment, RawGeometryTriangle, RawGeometryVertex, RawVertexRef,
    };
    let mut document = Document::new();
    let definition = document.add_definition(name, DefinitionKind::Mesh);
    let indices = frame
        .geometry
        .vertices
        .keys()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect::<BTreeMap<_, _>>();
    let corner = |v: meshcraft_core::document::VertexRef| RawVertexRef {
        vertex: indices[&v.id],
        position_offset: v.position_offset,
    };
    let vertices = frame
        .geometry
        .vertices
        .values()
        .map(|v| RawGeometryVertex {
            position: v.position,
            legacy_aliases: v.legacy_aliases.clone(),
        })
        .collect();
    let segments = frame
        .geometry
        .segments
        .values()
        .map(|s| RawGeometrySegment {
            endpoints: s.endpoints.map(corner),
            color: s.color,
            extensions: s.extensions.clone(),
            legacy_aliases: s.legacy_aliases.clone(),
        })
        .collect();
    let triangles = frame
        .geometry
        .triangles
        .values()
        .map(|f| RawGeometryTriangle {
            vertices: f.vertices.map(corner),
            color: f.color,
            extensions: f.extensions.clone(),
            legacy_aliases: f.legacy_aliases.clone(),
        })
        .collect();
    document.replace_definition_geometry_raw(definition, vertices, segments, triangles)?;
    document.set_prototype_ifc(definition, frame.state.prototype_ifc.clone())?;
    for hotspot in &frame.hotspots {
        document.put_hotspot(definition, hotspot.clone())?;
    }
    let path = modeling_objects::place_definition(
        &mut document,
        &EditContext::Document,
        definition,
        Vec3::ZERO,
        None,
        false,
    )?;
    let mut selection = SelectionSession::default();
    document.set_instance_ifc(&path, frame.state.instance_ifc.clone())?;
    selection.select(SelectionTarget {
        occurrence: path,
        entity: SelectedEntity::Occurrence,
    });
    meshcraft_core::document::clipboard::ClipboardSelection::capture(&document, &selection)
}

impl MeshcraftApp {
    pub(crate) fn mcp_resolve_document(&mut self, args: &Value) -> Result<Value, String> {
        let pending = self
            .pending_replacement
            .as_ref()
            .ok_or("no document replacement is pending")?
            .clone();
        match args["action"].as_str().ok_or("action required")? {
            "cancel" => self.resolve_document_replacement(ReplacementDecision::Cancel),
            "discard" => self.resolve_document_replacement(ReplacementDecision::Discard),
            "save" => {
                if let Some(path) = args.get("path") {
                    let path = path.as_str().ok_or("path must be a string")?;
                    if !std::path::Path::new(path).is_absolute() {
                        return Err("provide an absolute save path".into());
                    }
                    self.pending_replacement = None;
                    self.replacement_after_save = Some(pending.clone());
                    let payload = CommandPayload {
                        values: BTreeMap::from([("path".into(), json!(path))]),
                        ..Default::default()
                    };
                    let result = self.save_project_native(true, &payload);
                    if !matches!(result.completion, CommandCompletion::Pending { .. }) {
                        self.replacement_after_save = None;
                        self.pending_replacement = Some(pending.clone());
                        return Err(result.diagnostics.join("; "));
                    }
                } else {
                    if self.current_project_path.is_none() {
                        return Err("provide an absolute save path for an unnamed document".into());
                    }
                    self.resolve_document_replacement(ReplacementDecision::Save);
                }
            }
            _ => return Err("action must be save, discard or cancel".into()),
        }
        Ok(
            json!({"job_id":pending.job_id,"result":self.job_results.get(&pending.job_id),"status":self.mcp_status()}),
        )
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;
    use meshcraft_core::document::scripts::{Drawable, ScriptFrame};
    fn frame() -> ScriptFrame {
        let items:Vec<Drawable>=serde_json::from_value(json!([
            {"kind":"segment","id":"line","points":[{"x":0,"y":0,"z":0},{"x":3,"y":0,"z":0}],"color":{"red":20,"green":25,"blue":30,"alpha":255}},
            {"kind":"hotspot","id":"handle","position":{"x":3,"y":0,"z":0}}
        ])).unwrap();
        ScriptFrame::build(&Default::default(), items, Default::default()).unwrap()
    }
    fn queued(
        app: &mut MeshcraftApp,
        commit: bool,
    ) -> std::sync::mpsc::SyncSender<Result<ScriptFrame, String>> {
        let (send, receiver) = std::sync::mpsc::sync_channel(1);
        app.mcp_jobs.pending = Some(ScriptJob {
            id: "test-script".into(),
            generation: app.document_generation,
            revision: app.document.revision(),
            context: app.selection.context.clone(),
            name: "Test Geometry".into(),
            position: Vec3::X * 2.,
            commit,
            receiver,
            cancel: Default::default(),
        });
        send
    }
    #[test]
    fn committed_result_bakes_geometry_hotspots_and_is_one_undo_step() {
        let mut app = super::super::tests::test_app();
        let revision = app.document.revision();
        let entries = app.history.entries().len();
        let mut output = frame();
        let metadata =
            serde_json::from_value(json!({"Pset_Custom":{"Code":{"value":"A"}}})).unwrap();
        output.state.prototype_ifc = metadata;
        output.state.instance_ifc = output.state.prototype_ifc.clone();
        queued(&mut app, true).send(Ok(output)).unwrap();
        app.poll_mcp_script();
        let result = app.command_job_result("test-script").unwrap();
        assert_eq!(CommandCompletion::Completed, result.completion);
        assert_eq!(true, result.outputs["committed"]);
        assert!(app.document.revision() != revision);
        assert_eq!(entries + 1, app.history.entries().len());
        let path = app.selected_occurrences()[0].clone();
        let r = app.document.resolve_occurrence(&path).unwrap();
        assert_eq!(
            "A",
            app.document.instance_ifc(&path)["Pset_Custom"]["Code"].value
        );
        assert_eq!(
            "A",
            app.document.prototype_ifc(r.definition.id)["Pset_Custom"]["Code"].value
        );
        assert!(app.document.script(r.definition.id).is_none());
        assert_eq!(1, app.document.hotspots(r.definition.id).unwrap().len());
        assert_eq!(2., r.world_transform.transform_point(Vec3::ZERO).x);
        assert_eq!(
            1,
            app.document
                .effective_geometry(&path)
                .unwrap()
                .segments
                .len()
        );
        app.run_command("edit.undo");
        assert!(app.document.occurrence_refs().unwrap().is_empty());
    }
    #[test]
    fn preview_failure_cancel_and_stale_results_cannot_change_model() {
        let mut app = super::super::tests::test_app();
        let revision = app.document.revision();
        queued(&mut app, false).send(Ok(frame())).unwrap();
        app.poll_mcp_script();
        assert_eq!(revision, app.document.revision());
        assert_eq!(
            false,
            app.command_job_result("test-script").unwrap().outputs["committed"]
        );
        queued(&mut app, true)
            .send(Err("Bad source".into()))
            .unwrap();
        app.poll_mcp_script();
        assert_eq!(revision, app.document.revision());
        assert_eq!(
            CommandCompletion::Failed,
            app.command_job_result("test-script").unwrap().completion
        );
        let send = queued(&mut app, true);
        app.cancel_mcp_script().unwrap();
        assert!(send.send(Ok(frame())).is_err());
        app.poll_mcp_script();
        assert_eq!(revision, app.document.revision());
        let send = queued(&mut app, true);
        app.document
            .add_cuboid("user edit", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let changed = app.document.revision();
        send.send(Ok(frame())).unwrap();
        app.poll_mcp_script();
        assert_eq!(changed, app.document.revision());
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        assert!(app.command_job_result("test-script").unwrap().diagnostics[0].contains("changed"));
    }
    #[test]
    fn oversized_properties_and_errors_do_not_accumulate_in_job_summaries() {
        let mut app = super::super::tests::test_app();
        let mut output = frame();
        output
            .state
            .properties
            .insert("large".into(), json!("x".repeat(70_000)));
        queued(&mut app, false).send(Ok(output)).unwrap();
        app.poll_mcp_script();
        assert_eq!(
            true,
            app.command_job_result("test-script").unwrap().outputs["propertiesOmitted"]
        );
        queued(&mut app, false)
            .send(Err("x".repeat(70_000)))
            .unwrap();
        app.poll_mcp_script();
        assert_eq!(
            4096,
            app.command_job_result("test-script").unwrap().diagnostics[0].len()
        );
    }
    #[test]
    fn pending_document_choices_are_visible_and_explicitly_resolvable() {
        let mut app = super::super::tests::test_app();
        app.run_command("geometry.add_cuboid");
        app.run_command("file.new");
        assert!(!app.mcp_status()["pendingDialog"].is_null());
        app.mcp_resolve_document(&json!({"action":"cancel"}))
            .unwrap();
        assert_eq!(1, app.document.occurrence_refs().unwrap().len());
        app.run_command("file.new");
        app.mcp_resolve_document(&json!({"action":"discard"}))
            .unwrap();
        assert!(app.document.occurrence_refs().unwrap().is_empty());
    }
}
