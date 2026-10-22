//! Object authoring and hotspot commands shared by the desktop shell and automation.
use super::*;
use meshcraft_core::document::hotspots::{
    Hotspot, HotspotColor, Operation, SegmentAttachment, Shape, TriangleAttachment,
};

impl MeshcraftApp {
    pub(super) fn delete_selection_with_hotspots(&mut self) -> CommandResult {
        let selected = self.selection.selected().clone();
        let mut handles = Vec::new();
        let mut geometry = SelectionSession::default();
        geometry.set_context(self.selection.context.clone());
        for target in selected {
            if let SelectedEntity::SemanticHandle {
                kind: SelectionKind::Hotspot,
                id,
            } = &target.entity
            {
                if !self.native_hotspot_authoring_allowed(&target.occurrence) {
                    return CommandResult::failed(
                        "open the prototype before deleting its hotspot definitions",
                    );
                }
                let Some(r) = self.document.resolve_occurrence(&target.occurrence) else {
                    return CommandResult::failed("hotspot instance no longer exists");
                };
                handles.push((r.definition.id, id.clone()));
            } else {
                geometry.select(target);
            }
        }
        let result = self.commit_document_edit("Delete selected geometry and hotspots", |d| {
            for (definition, id) in &handles {
                d.delete_hotspot(*definition, id)?;
            }
            if !geometry.selected().is_empty() {
                modeling_objects::delete_selection(d, &geometry)?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                self.hotspot_inspector = None;
                self.selection.clear();
                self.status = "Deleted the selected geometry and hotspot definitions.".into();
                CommandResult::completed()
            }
            Err(e) => CommandResult::failed(e),
        }
    }

    pub fn native_pointer_tool_active(&self) -> bool {
        self.active_tool != "Select"
            || self.tool_cycle_state.hotspot_placement_active()
            || self.hotspot_reference_pick
    }

    pub(super) fn hotspot_owner(&self) -> Result<(OccurrenceRef, DefinitionId, Affine3), String> {
        let occurrence = match &self.selection.context {
            EditContext::Occurrence(active) => active.clone(),
            EditContext::Document => {
                if self.document.scene_root_definition().is_some() {
                    OccurrenceRef::scene_root()
                } else {
                    self.document
                        .scene_container_occurrence()
                        .ok_or("scene root is missing")?
                }
            }
            EditContext::Definition(_) => {
                return Err("open a placed instance to author its prototype hotspots".into());
            }
        };
        let r = self
            .document
            .resolve_occurrence(&occurrence)
            .ok_or("active object no longer exists")?;
        Ok((occurrence.clone(), r.definition.id, r.world_transform))
    }

    pub fn native_hotspot_inspector(&self) -> (Option<(OccurrenceRef, String)>, Hotspot) {
        let selected = self
            .selection
            .selected()
            .iter()
            .find_map(|target| match &target.entity {
                SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id,
                } => Some((target.occurrence.clone(), id.clone())),
                _ => None,
            })
            .or_else(|| {
                self.hotspot_inspector
                    .clone()
                    .filter(|(path, _)| self.native_hotspot_authoring_allowed(path))
            });
        if let Some((path, id)) = selected.as_ref()
            && let Ok(hotspots) = self.document.instance_hotspots(path)
            && let Some(h) = hotspots.into_iter().find(|h| &h.id == id)
        {
            return (selected, h);
        }
        (None, self.hotspot_defaults.clone())
    }

    pub fn native_hotspot_authoring_allowed(&self, occurrence: &OccurrenceRef) -> bool {
        if !self.selection_display_policy.can_edit_target(
            &self.document,
            &SelectionTarget {
                occurrence: occurrence.clone(),
                entity: SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id: String::new(),
                },
            },
        ) {
            return false;
        }
        match &self.selection.context {
            EditContext::Document => {
                occurrence.is_scene_root()
                    || self.document.scene_container_occurrence().as_ref() == Some(occurrence)
            }
            EditContext::Occurrence(active) => occurrence == active,
            EditContext::Definition(id) => self
                .document
                .resolve_occurrence(occurrence)
                .is_some_and(|r| r.definition.id == *id),
        }
    }

    pub fn select_native_hotspot(
        &mut self,
        occurrence: OccurrenceRef,
        id: String,
        mode: SelectionMode,
    ) {
        if !self.native_target_editable(&SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        }) {
            return;
        }
        self.hotspot_inspector = Some((occurrence.clone(), id.clone()));
        self.panel_focus = Some("hotspots".into());
        self.selection.apply(
            [SelectionTarget {
                occurrence,
                entity: SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id,
                },
            }],
            mode,
        );
        self.status =
            "Hotspot selected. Drag an instance handle to operate on this instance only.".into();
    }

    /// Reposition an authored marker without applying its runtime geometry operator.
    pub fn commit_native_hotspot_anchor(
        &mut self,
        occurrence: &OccurrenceRef,
        id: &str,
        position: Vec3,
    ) -> Result<(), String> {
        if !self.native_hotspot_authoring_allowed(occurrence) {
            return Err("open the prototype before repositioning its hotspot marker".into());
        }
        let definition = self
            .document
            .resolve_occurrence(occurrence)
            .ok_or("object no longer exists")?
            .definition
            .id;
        let mut hotspot = self
            .document
            .hotspots(definition)?
            .into_iter()
            .find(|h| h.id == id)
            .ok_or("hotspot no longer exists")?;
        hotspot.position = position;
        self.commit_document_edit("Reposition hotspot marker", |d| {
            d.put_hotspot(definition, hotspot)
        })?;
        self.status = "Repositioned the shared hotspot marker.".into();
        Ok(())
    }

    pub fn commit_native_hotspot_drag(
        &mut self,
        occurrence: &OccurrenceRef,
        id: &str,
        target_local: Vec3,
    ) -> Result<(), String> {
        if !self.native_target_editable(&SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Hotspot,
                id: id.into(),
            },
        }) {
            return Err("this instance hotspot is hidden or read-only".into());
        }
        if self.native_hotspot_authoring_allowed(occurrence) && !occurrence.is_scene_root() {
            return Err("finish prototype editing before operating an instance hotspot".into());
        }
        let context = self.selection.context.clone();
        self.commit_document_edit("Operate instance hotspot", |d| {
            d.set_nested_instance_hotspot_default(&context, occurrence, id, target_local)
        })?;
        self.status = "Updated this instance's hotspot value.".into();
        Ok(())
    }

    fn store_hotspot(
        &mut self,
        selected: Option<(OccurrenceRef, String)>,
        hotspot: Hotspot,
    ) -> Result<(), String> {
        if let Some((path, _)) = selected {
            if !self.native_hotspot_authoring_allowed(&path) {
                return Err("open this instance's prototype to change hotspot properties; dragging changes instance values".into());
            }
            let definition = self
                .document
                .resolve_occurrence(&path)
                .ok_or("object no longer exists")?
                .definition
                .id;
            self.commit_document_edit("Change hotspot properties", |d| {
                d.put_hotspot(definition, hotspot)
            })?;
        } else {
            self.hotspot_defaults = hotspot;
        }
        Ok(())
    }

    pub(super) fn add_hotspot_at_world(&mut self, point: Vec3) -> Result<String, String> {
        let (path, definition, world) = self.hotspot_owner()?;
        let inverse = world
            .inverse(GeometryTolerance::default())
            .ok_or("active object frame is singular")?;
        let mut hotspot = self.hotspot_defaults.clone();
        hotspot.id.clear();
        hotspot.position = inverse.transform_point(point);
        let id = self.commit_document_edit("Add prototype hotspot", |d| {
            d.put_hotspot(definition, hotspot)
        })?;
        self.cancel_active_tool();
        self.select_native_hotspot(path, id.clone(), SelectionMode::Replace);
        Ok(id)
    }

    pub(super) fn pick_hotspot_reference_at_world(&mut self, point: Vec3) -> Result<(), String> {
        let (selected, mut hotspot) = self.native_hotspot_inspector();
        let (path, _) = selected.as_ref().ok_or("select a hotspot first")?;
        let world = self
            .document
            .resolve_occurrence(path)
            .ok_or("object no longer exists")?
            .world_transform;
        hotspot.reference_position = Some(
            world
                .inverse(GeometryTolerance::default())
                .ok_or("object frame is singular")?
                .transform_point(point),
        );
        self.store_hotspot(selected, hotspot)?;
        self.hotspot_reference_pick = false;
        self.active_tool = "Select".into();
        self.status = "Hotspot reference point set in prototype coordinates.".into();
        Ok(())
    }

    fn patch_hotspot_field(h: &mut Hotspot, field: &str, text: &str) -> Result<(), String> {
        match field {
            "name" => h.name = text.to_owned(),
            "operation" => {
                h.operation = Operation::ALL
                    .into_iter()
                    .find(|op| op.name() == text)
                    .ok_or("unknown hotspot operation")?
            }
            "shape" => {
                h.shape = Shape::ALL
                    .into_iter()
                    .find(|shape| shape.name() == text)
                    .ok_or("unknown hotspot shape")?
            }
            "color" => {
                let text = text.trim().trim_start_matches('#');
                if text.len() != 8 {
                    return Err("use eight RGBA hexadecimal digits".into());
                }
                let raw = u32::from_str_radix(text, 16)
                    .map_err(|_| "use eight RGBA hexadecimal digits")?;
                h.color = HotspotColor::from_rgba(Rgba8::new(
                    (raw >> 24) as u8,
                    (raw >> 16) as u8,
                    (raw >> 8) as u8,
                    raw as u8,
                ));
            }
            _ => return Err("unknown hotspot property".into()),
        }
        Ok(())
    }
    pub fn update_native_hotspot_field(&mut self, field: &str, text: &str) -> Result<(), String> {
        self.update_native_hotspot_fields(&[(field, text)])
    }
    fn update_native_hotspot_fields(&mut self, fields: &[(&str, &str)]) -> Result<(), String> {
        if fields.is_empty() {
            return Ok(());
        }
        let mut targets = self
            .selection
            .selected()
            .iter()
            .filter_map(|target| match &target.entity {
                SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id,
                } => Some((target.occurrence.clone(), id.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        if targets.is_empty()
            && let Some(selected) = self.native_hotspot_inspector().0
        {
            targets.push(selected);
        }
        if targets.is_empty() {
            let mut hotspot = self.hotspot_defaults.clone();
            for &(field, text) in fields {
                Self::patch_hotspot_field(&mut hotspot, field, text)?;
            }
            self.hotspot_defaults = hotspot;
            return Ok(());
        }
        let mut edits = BTreeMap::new();
        for (path, id) in targets {
            let target = SelectionTarget {
                occurrence: path.clone(),
                entity: SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id: id.clone(),
                },
            };
            if !self.native_hotspot_authoring_allowed(&path)
                || !self.native_target_editable(&target)
            {
                return Err("Open the prototype or unlock the selected hotspots before changing properties.".into());
            }
            let definition = self
                .document
                .resolve_occurrence(&path)
                .ok_or("object no longer exists")?
                .definition
                .id;
            let mut h = self
                .document
                .instance_hotspots(&path)?
                .into_iter()
                .find(|h| h.id == id)
                .ok_or("hotspot no longer exists")?;
            for &(field, text) in fields {
                Self::patch_hotspot_field(&mut h, field, text)?;
            }
            edits.insert((definition, id), h);
        }
        self.commit_document_edit("Change hotspot properties", |d| {
            for ((definition, _), hotspot) in edits {
                d.put_hotspot(definition, hotspot)?;
            }
            Ok(())
        })?;
        Ok(())
    }

    pub(super) fn execute_hotspot_command(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        let result = self.run_hotspot_command(action, payload);
        match result {
            Ok(()) => CommandResult::completed(),
            Err(error) => {
                self.status = error.clone();
                CommandResult::failed(error)
            }
        }
    }
    fn run_hotspot_command(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> Result<(), String> {
        if action == "add" {
            let point = if let Some(value) = payload.values.get("position") {
                serde_json::from_value::<Vec3>(value.clone()).map_err(|e| e.to_string())?
            } else {
                self.cursor_snap
                    .ok_or("move the cursor to a snapped point first")?
                    .point
            };
            self.add_hotspot_at_world(point)?;
            return Ok(());
        }
        let (selected, mut h) = self.native_hotspot_inspector();
        if action == "update" {
            let fields = ["name", "operation", "shape", "color"]
                .into_iter()
                .filter_map(|field| {
                    payload
                        .values
                        .get(field)
                        .and_then(serde_json::Value::as_str)
                        .map(|text| (field, text))
                })
                .collect::<Vec<_>>();
            return self.update_native_hotspot_fields(&fields);
        }
        let (path, id) = selected.as_ref().ok_or("select a hotspot first")?;
        let resolved = self
            .document
            .resolve_occurrence(path)
            .ok_or("object no longer exists")?;
        let definition = resolved.definition.id;
        if action == "target" {
            let target: Vec3 = serde_json::from_value(
                payload
                    .values
                    .get("target")
                    .ok_or("target coordinates are required")?
                    .clone(),
            )
            .map_err(|e| e.to_string())?;
            return self.commit_native_hotspot_drag(path, id, target);
        }
        if action == "select_attached" {
            let geometry = &resolved.definition.geometry;
            let mut targets = Vec::new();
            for segment in geometry.segments.values() {
                let attachment = SegmentAttachment::from_segment(geometry, segment);
                if h.attached_segments.contains(&attachment) {
                    targets.push(SelectionTarget {
                        occurrence: path.clone(),
                        entity: SelectedEntity::Segment(segment.id),
                    });
                }
            }
            for triangle in geometry.triangles.values() {
                let attachment = TriangleAttachment::from_triangle(geometry, triangle);
                if h.attached_triangles.contains(&attachment) {
                    targets.push(SelectionTarget {
                        occurrence: path.clone(),
                        entity: SelectedEntity::Triangle(triangle.id),
                    });
                }
            }
            for vertex in geometry.vertices.values() {
                if h.attached_vertex_ids
                    .contains(&format!("vertex-{}", vertex.id.0))
                    || vertex
                        .legacy_aliases
                        .iter()
                        .any(|a| h.attached_vertex_ids.contains(&a.id))
                {
                    targets.push(SelectionTarget {
                        occurrence: path.clone(),
                        entity: SelectedEntity::Vertex(vertex.id),
                    });
                }
            }
            for id in &h.attached_hotspot_ids {
                targets.push(SelectionTarget {
                    occurrence: path.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Hotspot,
                        id: id.clone(),
                    },
                });
            }
            self.selection.apply(targets, SelectionMode::Replace);
            return Ok(());
        }
        if !self.native_hotspot_authoring_allowed(path) {
            return Err("open the prototype before editing hotspot definitions".into());
        }
        match action {
            "attach" => {
                let geometry = &resolved.definition.geometry;
                h.attached_vertex_ids.clear();
                h.attached_segments.clear();
                h.attached_triangles.clear();
                h.attached_hotspot_ids.clear();
                for target in self.selection.selected() {
                    if &target.occurrence != path {
                        return Err("hotspot attachments must belong to its prototype".into());
                    }
                    match &target.entity {
                        SelectedEntity::Vertex(id) => {
                            let vertex = geometry
                                .vertices
                                .get(id)
                                .ok_or("attached vertex no longer exists")?;
                            h.attached_vertex_ids.insert(
                                vertex
                                    .legacy_aliases
                                    .first()
                                    .map(|a| a.id.clone())
                                    .unwrap_or_else(|| format!("vertex-{}", id.0)),
                            );
                        }
                        SelectedEntity::Segment(id) => {
                            h.attached_segments.push(SegmentAttachment::from_segment(
                                geometry,
                                geometry.segments.get(id).ok_or("edge no longer exists")?,
                            ))
                        }
                        SelectedEntity::Triangle(id) => {
                            h.attached_triangles.push(TriangleAttachment::from_triangle(
                                geometry,
                                geometry.triangles.get(id).ok_or("face no longer exists")?,
                            ))
                        }
                        SelectedEntity::SemanticHandle {
                            kind: SelectionKind::Hotspot,
                            id,
                        } if id != &h.id => {
                            h.attached_hotspot_ids.insert(id.clone());
                        }
                        SelectedEntity::SemanticHandle {
                            kind: SelectionKind::Hotspot,
                            ..
                        } => {}
                        _ => {
                            return Err(
                                "attach prototype edges, faces, vertices, or other hotspots".into(),
                            );
                        }
                    }
                }
                self.store_hotspot(selected, h)?;
                self.status = "Attached the selected prototype geometry to the hotspot.".into();
            }
            "reference" => {
                self.cancel_active_tool();
                self.hotspot_reference_pick = true;
                self.active_tool = "Hotspot Reference".into();
                self.status =
                    "Pick a hotspot reference point; external geometry can supply snap references."
                        .into();
            }
            "clear_reference" => {
                h.reference_position = None;
                self.store_hotspot(selected, h)?;
            }
            "delete" => {
                self.commit_document_edit("Delete hotspot", |d| d.delete_hotspot(definition, id))?;
                self.hotspot_inspector = None;
                self.selection.retain_valid(&self.document);
            }
            _ => return Err("unknown hotspot command".into()),
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ObjectImportJob {
    job_id: String,
    path: PathBuf,
    external: bool,
    replace: Option<DefinitionId>,
    generation: u64,
    receiver: std::sync::mpsc::Receiver<Result<Document, String>>,
}
impl MeshcraftApp {
    pub fn select_native_prototype(&mut self, id: DefinitionId) {
        if self.document.definition(id).is_some() {
            self.object_browser_selection = Some(id);
            self.selection.clear();
            self.status = "Prototype selected. Double-click to insert an instance.".into();
        }
    }
    pub fn native_selected_prototype(&self) -> Option<DefinitionId> {
        self.object_browser_selection
            .filter(|id| self.document.definition(*id).is_some())
    }
    pub(super) fn execute_object_file_command(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (action, payload);
            CommandResult::failed("external object files require the desktop host")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start_object_import(action, payload)
                .unwrap_or_else(CommandResult::failed)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn start_object_import(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> Result<CommandResult, String> {
        if self.pending_object_import.is_some() {
            return Err("an object scene is already loading".into());
        }
        let replace = if action == "refresh_reference" {
            Some(
                self.native_selected_prototype()
                    .ok_or("select an external prototype in Objects first")?,
            )
        } else {
            None
        };
        let path = if let Some(definition) = replace.and_then(|id| self.document.definition(id)) {
            definition
                .extensions
                .get("k3d.externalReferencePath")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .ok_or("the selected prototype is internal")?
        } else if let Some(path) = payload
            .values
            .get("path")
            .and_then(serde_json::Value::as_str)
        {
            PathBuf::from(path)
        } else {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Object scenes", &["octd", "k3d"])
                .pick_file()
            else {
                return Ok(CommandResult::completed());
            };
            path
        };
        let path = if path.is_absolute() {
            path
        } else {
            self.current_project_path
                .as_ref()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        };
        let path = path.canonicalize().map_err(|e| {
            format!(
                "cannot read {}: {e}; cached external geometry was retained",
                path.display()
            )
        })?;
        if action != "import_scene"
            && self
                .current_project_path
                .as_ref()
                .and_then(|p| p.canonicalize().ok())
                .as_ref()
                == Some(&path)
        {
            return Err("a scene cannot reference itself as an external object".into());
        }
        let external = action != "import_scene";
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker_path = path.clone();
        let job_id = format!("object-import-{}", self.next_job_id);
        self.next_job_id += 1;
        std::thread::Builder::new()
            .name(job_id.clone())
            .spawn(move || {
                let loaded = (|| {
                    let length = std::fs::metadata(&worker_path)
                        .map_err(|e| e.to_string())?
                        .len();
                    if length > 512 * 1024 * 1024 {
                        return Err("external scene exceeds the 512 MiB import limit".into());
                    }
                    let bytes = std::fs::read(&worker_path).map_err(|e| e.to_string())?;
                    meshcraft_io::project::read_project(&bytes)
                        .map(|project| project.document)
                        .map_err(|e| e.to_string())
                })();
                let _ = sender.send(loaded);
            })
            .map_err(|e| format!("could not start object scene loader: {e}"))?;
        self.status = format!("Loading object scene {}…", path.display());
        self.pending_object_import = Some(ObjectImportJob {
            job_id: job_id.clone(),
            path,
            external,
            replace,
            generation: self.document_generation,
            receiver,
        });
        let result = CommandResult::pending(
            job_id.clone(),
            HostEffect::StartJob {
                job_id: job_id.clone(),
                kind: "object.import_scene".into(),
                input_revision: Some(self.document.revision()),
            },
        );
        self.job_results.insert(job_id, result.clone());
        Ok(result)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_object_import(&mut self) {
        let loaded = match self
            .pending_object_import
            .as_ref()
            .map(|job| job.receiver.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                Err("object scene loader stopped unexpectedly".into())
            }
            _ => return,
        };
        let job = self.pending_object_import.take().unwrap();
        let result = if job.generation != self.document_generation {
            Err("object import canceled because the active document changed".into())
        } else {
            loaded.and_then(|source| {
                let name = job
                    .replace
                    .and_then(|id| self.document.definition(id).map(|d| d.name.clone()))
                    .unwrap_or_else(|| {
                        job.path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                let path = job.path.to_string_lossy();
                self.commit_document_edit(
                    if job.replace.is_some() {
                        "Refresh external prototype"
                    } else {
                        "Import scene prototype"
                    },
                    |d| {
                        d.import_subscene(
                            &source,
                            &name,
                            job.external.then_some(path.as_ref()),
                            job.replace,
                        )
                    },
                )
            })
        };
        let final_result = match result {
            Ok(id) => {
                self.object_browser_selection = Some(id);
                self.panel_focus = Some("objects".into());
                self.status = format!(
                    "{} prototype ready. Double-click it in Objects to insert an instance.",
                    if job.external { "External" } else { "Internal" }
                );
                let mut result = CommandResult::completed();
                result.document_revision = Some(self.document.revision());
                result.outputs.insert("definition_id".into(), id.0.into());
                result
            }
            Err(error) => {
                self.status =
                    format!("Object import failed: {error}. Cached geometry was retained.");
                CommandResult::failed(error)
            }
        };
        self.job_results.insert(job.job_id, final_result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotspot_property_patch_updates_all_selected_hotspots_atomically() {
        let mut app = super::super::tests::test_app();
        let a = app.add_hotspot_at_world(Vec3::ZERO).unwrap();
        let b = app.add_hotspot_at_world(Vec3::X * 3.).unwrap();
        let root = Document::SCENE_ROOT_DEFINITION_ID;
        let before = app.document.hotspots(root).unwrap();
        app.selection.apply(
            [a, b].into_iter().map(|id| SelectionTarget {
                occurrence: OccurrenceRef::scene_root(),
                entity: SelectedEntity::SemanticHandle {
                    kind: SelectionKind::Hotspot,
                    id,
                },
            }),
            SelectionMode::Replace,
        );
        let cursor = app.history.cursor();
        app.update_native_hotspot_fields(&[("shape", "SQUARE"), ("color", "ff0000ff")])
            .unwrap();
        assert_eq!(cursor + 1, app.history.cursor());
        for h in app.document.hotspots(root).unwrap() {
            assert_eq!("SQUARE", h.shape.name());
            assert_eq!(Rgba8::new(255, 0, 0, 255), h.color.rgba());
            assert_eq!(
                before.iter().find(|x| x.id == h.id).unwrap().position,
                h.position
            );
        }
        app.run_command("edit.undo");
        assert_eq!(before, app.document.hotspots(root).unwrap());
        assert!(
            app.update_native_hotspot_fields(&[("shape", "SQUARE"), ("color", "invalid")])
                .is_err()
        );
        assert_eq!(before, app.document.hotspots(root).unwrap());
        assert_eq!(cursor, app.history.cursor());
    }

    #[test]
    fn author_prototype_then_operate_closed_instances_copy_values_and_escape_isolates_editing() {
        let mut app = super::super::tests::test_app();
        let instance = app
            .document
            .add_cuboid("Dynamic", Vec3::new(2., 2., 2.), Vec3::ZERO)
            .unwrap();
        let def = app
            .document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        app.selection
            .set_context(EditContext::Occurrence(instance.clone()));
        let id = app.add_hotspot_at_world(Vec3::ZERO).unwrap();
        app.update_native_hotspot_field("operation", "STRETCH")
            .unwrap();
        app.update_native_hotspot_field("shape", "SQUARE").unwrap();
        app.update_native_hotspot_field("color", "338cf2ff")
            .unwrap();
        let face = *app
            .document
            .definition(def)
            .unwrap()
            .geometry
            .triangles
            .keys()
            .next()
            .unwrap();
        app.selection.apply(
            [SelectionTarget {
                occurrence: instance.clone(),
                entity: SelectedEntity::Triangle(face),
            }],
            SelectionMode::Replace,
        );
        assert_eq!(
            CommandCompletion::Completed,
            app.execute_hotspot_command("attach", &CommandPayload::default())
                .completion
        );
        let h = app.document.hotspots(def).unwrap().pop().unwrap();
        assert_eq!(1, h.attached_triangles.len());
        assert_eq!(Shape::Square, h.shape);
        let baseline = app.document.definition(def).unwrap().geometry.clone();
        assert!(
            app.commit_native_hotspot_drag(&instance, &id, Vec3::Y)
                .is_err()
        );
        app.commit_native_hotspot_anchor(&instance, &id, Vec3::X)
            .unwrap();
        assert_eq!(Vec3::X, app.document.hotspots(def).unwrap()[0].position);
        assert_eq!(baseline, app.document.definition(def).unwrap().geometry);
        app.handle_native_escape();
        assert!(
            app.commit_native_hotspot_anchor(&instance, &id, Vec3::ZERO)
                .is_err()
        );
        assert_eq!(EditContext::Document, app.selection.context);
        app.commit_native_hotspot_drag(&instance, &id, Vec3::Y)
            .unwrap();
        assert_eq!(baseline, app.document.definition(def).unwrap().geometry);
        assert_ne!(
            baseline,
            *app.document.effective_geometry(&instance).unwrap()
        );
        let copy =
            modeling_objects::duplicate_occurrence(&mut app.document, &instance, Vec3::X * 5.)
                .unwrap();
        assert_eq!(
            app.document.effective_geometry(&instance),
            app.document.effective_geometry(&copy)
        );
        app.commit_native_hotspot_drag(&copy, &id, Vec3::Y * 3.)
            .unwrap();
        assert_ne!(
            app.document.effective_geometry(&instance),
            app.document.effective_geometry(&copy)
        );
        app.selection
            .set_context(EditContext::Occurrence(instance.clone()));
        assert_eq!(
            baseline,
            *app.native_document_for_view()
                .effective_geometry(&instance)
                .unwrap()
        );
        let outside = SelectionTarget {
            occurrence: copy.clone(),
            entity: SelectedEntity::Occurrence,
        };
        assert!(!app.native_target_editable(&outside));
        app.apply_native_selection_region(vec![outside], false, SelectionMode::Replace);
        assert!(app.selection.selected().is_empty());
        app.select_native_hotspot(instance.clone(), id.clone(), SelectionMode::Replace);
        assert!(app.query_command_state("edit.delete_selection").enabled);
        app.execute_hotspot_command("delete", &CommandPayload::default());
        assert!(app.document.hotspots(def).unwrap().is_empty());
        assert!(app.selection.selected().is_empty());
    }
}

#[cfg(test)]
mod spatial_view_tests {
    use super::*;
    #[test]
    fn finishing_a_top_level_imported_object_skips_the_scene_container() {
        let mut app = super::super::tests::test_app();
        app.document = meshcraft_io::project::read_project(include_bytes!(
            "../../../examples/dynamic-object.octd"
        ))
        .unwrap()
        .document;
        let root = app.document.scene_container_occurrence().unwrap();
        let instance = app
            .document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|p| p.path.len() == root.path.len() + 1)
            .unwrap();
        app.document
            .set_hotspot_target(&instance, "stretch", Vec3::X)
            .unwrap();
        let runtime = app.document.effective_geometry(&instance).unwrap().clone();
        app.selection
            .set_context(EditContext::Occurrence(instance.clone()));
        app.handle_native_escape();
        assert_eq!(EditContext::Document, app.native_edit_context());
        assert_eq!(
            runtime,
            *app.native_document_for_view()
                .effective_geometry(&instance)
                .unwrap()
        );
    }

    #[test]
    fn switching_to_authored_prototype_invalidates_the_runtime_pick_mesh() {
        let mut app = super::super::tests::test_app();
        let instance = app
            .document
            .add_cuboid("Moved by handle", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let def = app
            .document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        let geometry = &app.document.definition(def).unwrap().geometry;
        let h = Hotspot {
            id: "move".into(),
            attached_triangles: geometry
                .triangles
                .values()
                .map(|t| TriangleAttachment::from_triangle(geometry, t))
                .collect(),
            ..Hotspot::default()
        };
        app.document.put_hotspot(def, h).unwrap();
        app.document
            .set_hotspot_target(&instance, "move", Vec3::X * 5.)
            .unwrap();
        let direction = -Vec3::Y;
        assert!(
            app.pick_native_target(Vec3::new(5.5, 10., 0.5), direction, 0.01)
                .is_some()
        );
        app.selection
            .set_context(EditContext::Occurrence(instance.clone()));
        let picked = app
            .pick_native_target(Vec3::new(0.5, 10., 0.5), direction, 0.01)
            .unwrap();
        assert!(matches!(picked.entity, SelectedEntity::Triangle(_)));
        app.handle_native_escape();
        assert_eq!(EditContext::Document, app.selection.context);
        assert!(
            app.pick_native_target(Vec3::new(5.5, 10., 0.5), direction, 0.01)
                .is_some()
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod import_job_tests {
    use super::*;

    #[test]
    fn external_scene_job_finishes_without_input_and_failed_refresh_retains_cache() {
        let mut app = super::super::tests::test_app();
        let path = std::env::temp_dir().join(format!(
            "meshcraft-external-object-{}.octd",
            std::process::id()
        ));
        let mut source = Document::new();
        source
            .add_cuboid("Child", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        std::fs::write(
            &path,
            meshcraft_io::project::write_project(&source, None).unwrap(),
        )
        .unwrap();
        let mut payload = CommandPayload::default();
        payload
            .values
            .insert("path".into(), path.to_string_lossy().into_owned().into());
        let started = app.execute_object_file_command("reference_scene", &payload);
        assert!(matches!(
            started.completion,
            CommandCompletion::Pending { .. }
        ));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.pending_object_import.is_some() {
            app.poll_object_import();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let id = app
            .native_selected_prototype()
            .expect("background import selected its prototype");
        let cached = app.document.clone();
        assert!(
            app.document
                .definition(id)
                .unwrap()
                .extensions
                .contains_key("k3d.externalReferencePath")
        );
        std::fs::remove_file(&path).unwrap();
        let failed =
            app.execute_object_file_command("refresh_reference", &CommandPayload::default());
        assert_eq!(CommandCompletion::Failed, failed.completion);
        assert_eq!(cached, app.document);
    }
}
