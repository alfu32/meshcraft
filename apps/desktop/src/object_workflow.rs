//! Object authoring and hotspot commands shared by the desktop shell and automation.
use super::*;
use meshcraft_core::document::hotspots::{
    Hotspot, HotspotColor, Operation, SegmentAttachment, Shape, TriangleAttachment,
};

impl MeshcraftApp {
    pub fn native_pointer_tool_active(&self) -> bool {
        self.active_tool != "Select"
            || self.tool_cycle_state.hotspot_placement_active()
            || self.hotspot_reference_pick
    }

    fn hotspot_owner(&self) -> Result<(OccurrenceRef, DefinitionId, Affine3), String> {
        let occurrence = match &self.selection.context {
            EditContext::Occurrence(active) => active.clone(),
            EditContext::Document => {
                if self.document.scene_root_definition().is_some() {
                    OccurrenceRef::scene_root()
                } else {
                    self.document
                        .occurrence_refs()?
                        .into_iter()
                        .find(|path| {
                            path.path.len() == 1
                                && self
                                    .document
                                    .resolve_occurrence(path)
                                    .is_some_and(|r| r.definition.name == "Root")
                        })
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
            .or_else(|| self.hotspot_inspector.clone());
        if let Some((path, id)) = selected.as_ref() {
            if let Some(r) = self.document.resolve_occurrence(path) {
                if let Ok(hotspots) = self.document.hotspots(r.definition.id) {
                    if let Some(h) = hotspots.into_iter().find(|h| &h.id == id) {
                        return (selected, h);
                    }
                }
            }
        }
        (None, self.hotspot_defaults.clone())
    }

    pub fn native_hotspot_authoring_allowed(&self, occurrence: &OccurrenceRef) -> bool {
        match &self.selection.context {
            EditContext::Document => {
                occurrence.is_scene_root()
                    || (occurrence.path.len() == 1
                        && self
                            .document
                            .resolve_occurrence(occurrence)
                            .is_some_and(|r| r.definition.name == "Root"))
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

    pub fn commit_native_hotspot_drag(
        &mut self,
        occurrence: &OccurrenceRef,
        id: &str,
        target_local: Vec3,
    ) -> Result<(), String> {
        if self.native_hotspot_authoring_allowed(occurrence) && !occurrence.is_scene_root() {
            return Err("finish prototype editing before operating an instance hotspot".into());
        }
        self.commit_document_edit("Operate instance hotspot", |d| {
            d.set_hotspot_target(occurrence, id, target_local)
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
        self.select_native_hotspot(path, id.clone(), SelectionMode::Replace);
        self.tool_cycle_state.cancel_hotspot_placement();
        self.active_tool = "Select".into();
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

    pub fn update_native_hotspot_field(&mut self, field: &str, text: &str) -> Result<(), String> {
        let (selected, mut h) = self.native_hotspot_inspector();
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
        self.store_hotspot(selected, h)
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
            for field in ["name", "operation", "shape", "color"] {
                if let Some(text) = payload
                    .values
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                {
                    self.update_native_hotspot_field(field, text)?;
                }
            }
            return Ok(());
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
