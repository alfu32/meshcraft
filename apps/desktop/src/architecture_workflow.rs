//! Shared point-input, structured commands and transactional architecture editing.
use super::*;
use meshcraft_core::document::{
    DefinitionKind,
    architecture::{self as arch, Element, Frame, FrameKind, PlacedElement, Slab, Wall},
};
use serde_json::Value;

#[derive(Clone, Default)]
pub(super) struct State {
    pub points: Vec<Vec3>,
    pub normal: Option<Vec3>,
    pub owner: Option<SelectionTarget>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Defaults {
    wall: Wall,
    slab: Slab,
    frame: Frame,
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            wall: Wall::default(),
            slab: Slab::default(),
            frame: Frame {
                glazing_enabled: true,
                ..Frame::default()
            },
        }
    }
}
const TOOLS: &[&str] = &[
    "Wall",
    "Rectangular Slab",
    "Freeform Slab",
    "Add Hole",
    "Window",
    "Door",
];
pub(super) fn active(tool: &str) -> bool {
    TOOLS.contains(&tool)
}
fn rectangle(a: Vec3, b: Vec3, normal: Vec3) -> Result<Vec<Vec3>, String> {
    let n = normal
        .try_normalized(1e-9)
        .ok_or("Rectangle normal is invalid.")?;
    let u = Vec3::Y.cross(n).try_normalized(1e-9).unwrap_or(Vec3::X);
    let v = n.cross(u);
    let d = b - a;
    if d.dot(u).abs() < 1e-7 || d.dot(v).abs() < 1e-7 {
        return Err("Pick two distinct opposite corners with nonzero width and height.".into());
    }
    Ok(vec![
        a,
        a + u * d.dot(u),
        a + u * d.dot(u) + v * d.dot(v),
        a + v * d.dot(v),
    ])
}
fn scalar(payload: &CommandPayload, key: &str, default: f64) -> Result<f64, String> {
    match payload.values.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{key} must be a finite number.")),
    }
}
fn color(payload: &CommandPayload, key: &str, default: arch::Color) -> Result<arch::Color, String> {
    payload
        .values
        .get(key)
        .map(|v| {
            serde_json::from_value(v.clone())
                .map_err(|_| format!("{key} must contain RGBA channels between zero and one."))
        })
        .unwrap_or(Ok(default))
}
impl MeshcraftApp {
    fn arch_defaults(&self) -> Defaults {
        self.document
            .architecture_defaults()
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }
    pub fn native_selected_architecture(&self) -> Option<PlacedElement> {
        self.selection.selected().iter().find_map(|t| {
            self.document.architecture_element(t).or_else(|| {
                (t.entity == SelectedEntity::Occurrence)
                    .then(|| {
                        self.document
                            .architecture_elements()
                            .into_iter()
                            .find(|e| e.target.occurrence == t.occurrence)
                    })
                    .flatten()
            })
        })
    }
    pub fn native_architecture_payload(&self) -> CommandPayload {
        let mut d = self.arch_defaults();
        let selected = self
            .native_selected_architecture()
            .filter(|_| self.active_tool == "Select");
        if let Some(selected) = &selected {
            match &selected.element {
                Element::Wall(w) => d.wall = w.clone(),
                Element::Slab(s) => d.slab = s.clone(),
                Element::Frame(f) => d.frame = f.clone(),
            }
        }
        let value = json!({"name":selected.as_ref().map(|e|e.element.name()).unwrap_or(""),
            "wall_thickness":d.wall.thickness,"wall_height":d.wall.height,"wall_inclination":d.wall.inclination_deg,
            "wall_exterior_color":d.wall.exterior_color,"wall_interior_color":d.wall.interior_color,
            "slab_thickness":d.slab.thickness,"slab_top_color":d.slab.top_color,"slab_bottom_color":d.slab.bottom_color,"slab_side_color":d.slab.side_color,
            "frame_depth":d.frame.depth,"frame_width":d.frame.frame_width,"frame_color":d.frame.color,"glazing_enabled":d.frame.glazing_enabled,"glazing_color":d.frame.glazing_color});
        CommandPayload {
            values: value
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..Default::default()
        }
    }
    pub(super) fn arch_editable(&self, e: &PlacedElement) -> bool {
        if !self.native_target_editable(&e.target) {
            return false;
        }
        match &self.selection.context {
            EditContext::Document => {
                e.target.occurrence.is_scene_root()
                    || self.document.scene_container_occurrence().as_ref()
                        == Some(&e.target.occurrence)
                    || matches!(e.element, Element::Frame(_))
                        && (e.target.occurrence.path.len() == 1
                            || self
                                .document
                                .scene_container_occurrence()
                                .is_some_and(|root| {
                                    e.target.occurrence.path.len() == root.path.len() + 1
                                        && e.target.occurrence.path.starts_with(&root.path)
                                }))
            }
            EditContext::Occurrence(p) => {
                e.target.occurrence == *p
                    || matches!(e.element, Element::Frame(_))
                        && e.target.occurrence.path.len() == p.path.len() + 1
                        && e.target.occurrence.path.starts_with(&p.path)
            }
            EditContext::Definition(id) => e.definition == *id,
        }
    }
    pub fn native_architecture_editable(&self, target: &SelectionTarget) -> bool {
        self.document
            .architecture_element(target)
            .is_some_and(|e| self.arch_editable(&e))
    }
    fn arch_owner(&self) -> (DefinitionId, OccurrenceRef, Affine3) {
        let (owner, occurrence, inverse) = self.line_geometry_context();
        (
            match owner {
                LineOwner::SceneRoot => Document::SCENE_ROOT_DEFINITION_ID,
                LineOwner::Definition(id) => id,
            },
            occurrence,
            inverse,
        )
    }
    fn arch_element(
        &self,
        tool: &str,
        points: &[Vec3],
        normal: Vec3,
        payload: &CommandPayload,
    ) -> Result<Element, String> {
        let (_, _, inverse) = self.arch_owner();
        let local_normal = inverse
            .transform_normal(normal, GeometryTolerance::default())
            .ok_or("Architecture normal is invalid.")?;
        let d = self.arch_defaults();
        let local = points
            .iter()
            .map(|p| inverse.transform_point(*p))
            .collect::<Vec<_>>();
        let name = payload.values.get("name").and_then(Value::as_str);
        let mut element = match tool {
            "Wall" => {
                if local.len() != 2 {
                    return Err("Wall needs start and end points.".into());
                }
                Element::Wall(Wall {
                    name: name.unwrap_or("Wall").into(),
                    start: local[0],
                    end: local[1],
                    up: inverse
                        .transform_vector(Vec3::Y)
                        .try_normalized(1e-9)
                        .ok_or("Wall up axis is invalid.")?,
                    thickness: scalar(payload, "wall_thickness", d.wall.thickness)?,
                    height: scalar(payload, "wall_height", d.wall.height)?,
                    inclination_deg: scalar(payload, "wall_inclination", d.wall.inclination_deg)?,
                    exterior_color: color(payload, "wall_exterior_color", d.wall.exterior_color)?,
                    interior_color: color(payload, "wall_interior_color", d.wall.interior_color)?,
                    ..Wall::default()
                })
            }
            "Rectangular Slab" | "Freeform Slab" => Element::Slab(Slab {
                name: name.unwrap_or("Slab").into(),
                profile: PolylinePath {
                    points: local,
                    closed: true,
                },
                normal: local_normal,
                thickness: scalar(payload, "slab_thickness", d.slab.thickness)?,
                top_color: color(payload, "slab_top_color", d.slab.top_color)?,
                bottom_color: color(payload, "slab_bottom_color", d.slab.bottom_color)?,
                side_color: color(payload, "slab_side_color", d.slab.side_color)?,
                ..Slab::default()
            }),
            "Window" | "Door" => Element::Frame(Frame {
                name: name.unwrap_or(tool).into(),
                contour: local,
                normal: local_normal,
                kind: if tool == "Door" {
                    FrameKind::Door
                } else {
                    FrameKind::Window
                },
                depth: scalar(payload, "frame_depth", d.frame.depth)?,
                frame_width: scalar(payload, "frame_width", d.frame.frame_width)?,
                color: color(payload, "frame_color", d.frame.color)?,
                glazing_enabled: payload
                    .values
                    .get("glazing_enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(tool == "Window" && d.frame.glazing_enabled),
                glazing_color: color(payload, "glazing_color", d.frame.glazing_color)?,
                ..Frame::default()
            }),
            _ => return Err("Unknown architecture tool.".into()),
        };
        if let Some(record) = payload.values.get("record").and_then(Value::as_object) {
            let mut value = element.value();
            value.as_object_mut().unwrap().extend(record.clone());
            element = Element::from_value(element.field(), value)?;
        }
        element.mesh()?;
        Ok(element)
    }
    fn commit_arch_element(&mut self, element: Element) -> Result<SelectionTarget, String> {
        let (owner, occurrence, inverse) = self.arch_owner();
        let target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::SemanticHandle {
                kind: element.kind(),
                id: String::new(),
            },
        };
        if !self
            .selection_display_policy
            .can_edit_target(&self.document, &target)
        {
            return Err("This architecture type is hidden or read-only.".into());
        }
        let context = self.selection.context.clone();
        let target = self.commit_document_edit("Create architecture element", |doc| {
            if let Element::Frame(mut frame) = element {
                // Anchor the independent prototype at the first authored corner.
                let anchor = frame.contour[0];
                for p in &mut frame.contour {
                    *p -= anchor;
                }
                let world_anchor = inverse
                    .inverse(GeometryTolerance::default())
                    .ok_or("Architecture context is singular.")?
                    .transform_point(anchor);
                let definition =
                    doc.add_definition(frame.name.clone(), DefinitionKind::Architecture);
                doc.put_architecture(definition, None, Element::Frame(frame))?;
                let path = modeling_objects::place_definition(
                    doc,
                    &context,
                    definition,
                    world_anchor,
                    None,
                    false,
                )?;
                Ok(SelectionTarget {
                    occurrence: path,
                    entity: SelectedEntity::Occurrence,
                })
            } else {
                let kind = element.kind();
                let id = doc.put_architecture(owner, None, element)?;
                Ok(SelectionTarget {
                    occurrence,
                    entity: SelectedEntity::SemanticHandle { kind, id },
                })
            }
        })?;
        self.selection
            .apply([target.clone()], SelectionMode::Replace);
        Ok(target)
    }
    pub(super) fn arch_begin(&mut self) {
        self.architecture = State::default();
        self.architecture.normal =
            Some(if matches!(self.active_tool.as_str(), "Window" | "Door") {
                Vec3::Z
            } else {
                self.preferences.grid_normal_axis.vector()
            });
        self.panel_focus = Some("architecture_walls".into());
        self.status = self.arch_hint().into();
        if matches!(
            self.active_tool.as_str(),
            "Freeform Slab" | "Window" | "Door"
        ) && let Some(points) = self.arch_selected_contour()
        {
            let normal = (points[1] - points[0])
                .cross(points[2] - points[1])
                .try_normalized(1e-9)
                .unwrap_or(Vec3::Y);
            let tool = self.active_tool.clone();
            match self
                .arch_element(&tool, &points, normal, &CommandPayload::default())
                .and_then(|e| self.commit_arch_element(e))
            {
                Ok(_) => {
                    self.activate_tool("Select");
                    self.status = format!("{tool} created from the selected closed polyline.");
                }
                Err(e) => self.status = e,
            }
        }
    }
    fn arch_selected_contour(&self) -> Option<Vec<Vec3>> {
        let mut edges = vec![];
        for target in self.selection.selected() {
            let SelectedEntity::Segment(id) = target.entity else {
                return None;
            };
            let r = self.document.resolve_occurrence(&target.occurrence)?;
            let g = self.document.effective_geometry(&target.occurrence)?;
            let e = g.segments.get(&id)?;
            edges.push(
                e.endpoints
                    .map(|v| {
                        g.vertex_position(v)
                            .map(|p| r.world_transform.transform_point(p))
                    })
                    .into_iter()
                    .collect::<Option<Vec<_>>>()?,
            );
        }
        if edges.len() < 3 {
            return None;
        }
        let first = edges.pop()?;
        let mut points = vec![first[0], first[1]];
        while !edges.is_empty() {
            let end = *points.last()?;
            let i = edges
                .iter()
                .position(|e| e[0].distance(end) < 1e-6 || e[1].distance(end) < 1e-6)?;
            let e = edges.remove(i);
            points.push(if e[0].distance(end) < 1e-6 {
                e[1]
            } else {
                e[0]
            });
        }
        if points[0].distance(*points.last()?) > 1e-6 {
            return None;
        }
        points.pop();
        Some(points)
    }
    pub(super) fn arch_hint(&self) -> &'static str {
        match self.active_tool.as_str() {
            "Wall" if self.architecture.points.is_empty() => {
                "Wall: pick the start of a wall chain · Enter finishes · Esc cancels"
            }
            "Wall" => {
                "Wall: pick the next endpoint · Enter finishes the chain · Esc cancels the pending span"
            }
            "Rectangular Slab" if self.architecture.points.is_empty() => {
                "Rectangular Slab: pick the first corner"
            }
            "Rectangular Slab" => "Rectangular Slab: pick the opposite corner",
            "Freeform Slab" => {
                "Freeform Slab: draw a closed profile · A arc · C circular · L line · Enter closes and creates the slab"
            }
            "Add Hole" if self.architecture.points.is_empty() => {
                "Add Hole: pick the first opening corner on a wall or slab"
            }
            "Add Hole" => "Add Hole: pick the opposite corner on the same owner",
            "Window" | "Door" if self.architecture.points.is_empty() => {
                "Window / Door: pick the first corner, or use a selected closed polyline"
            }
            "Window" | "Door" => {
                "Window / Door: pick the opposite corner · This object does not cut an opening"
            }
            _ => "",
        }
    }
    fn arch_project(&self, p: Vec3) -> Vec3 {
        let Some(a) = self
            .architecture
            .points
            .first()
            .or_else(|| self.polyline_tool_state.points().first())
        else {
            return p;
        };
        let n = self.architecture.normal.unwrap_or(Vec3::Y);
        p - n * (p - *a).dot(n)
    }
    pub fn native_architecture_plane_snap(
        &self,
        origin: Vec3,
        direction: Vec3,
        snap: Option<SnapResult>,
    ) -> Option<SnapResult> {
        if !active(&self.active_tool) {
            return snap;
        }
        let Some(a) = self
            .architecture
            .points
            .first()
            .or_else(|| self.polyline_tool_state.points().first())
        else {
            return snap;
        };
        let n = self.architecture.normal.unwrap_or(Vec3::Y);
        if let Some(s) = snap
            && (s.point - *a).dot(n).abs() < 1e-5
            && matches!(
                s.kind,
                SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Line
            )
        {
            return Some(s);
        }
        let denominator = direction.dot(n);
        if denominator.abs() < 1e-9 {
            return None;
        }
        let distance = (*a - origin).dot(n) / denominator;
        (distance >= 0.0 && distance.is_finite()).then_some(SnapResult {
            point: origin + direction * distance,
            normal: n,
            kind: SnapKind::Face,
            screen_distance: 0.0,
        })
    }
    pub(super) fn arch_click(
        &mut self,
        point: Vec3,
        snap: Option<SnapResult>,
        target: Option<SelectionTarget>,
    ) {
        let tool = self.active_tool.clone();
        if self.architecture.points.is_empty() && self.polyline_tool_state.points().is_empty() {
            if tool == "Add Hole" {
                let owner = target
                    .and_then(|t| self.document.architecture_element(&t))
                    .or_else(|| self.native_selected_architecture())
                    .filter(|e| {
                        matches!(e.element, Element::Wall(_) | Element::Slab(_))
                            && self.arch_editable(e)
                    });
                let Some(owner) = owner else {
                    self.status = "Pick an editable wall or slab surface for the opening.".into();
                    return;
                };
                let plane = owner.element.plane().ok();
                self.architecture.normal = plane.and_then(|p| {
                    owner
                        .transform
                        .transform_vector(p.u)
                        .cross(owner.transform.transform_vector(p.v))
                        .try_normalized(1e-9)
                });
                self.architecture.owner = Some(owner.target.clone());
                self.selection.apply([owner.target], SelectionMode::Replace);
            }
            self.architecture.normal = Some(if tool == "Wall" {
                Vec3::Y
            } else if tool == "Add Hole" {
                self.architecture.normal.unwrap_or(Vec3::Y)
            } else {
                snap.filter(|s| s.kind == SnapKind::Face)
                    .map(|s| s.normal)
                    .unwrap_or(self.architecture.normal.unwrap_or(Vec3::Y))
            });
        }
        let point = self.arch_project(point);
        if tool == "Freeform Slab" {
            match self
                .polyline_tool_state
                .pick(point, self.document.model_settings().arc_max_length)
            {
                Ok(PolylinePickEffect::Closed) => {
                    let result = self.finish_polyline(true);
                    if result.completion != CommandCompletion::Completed {
                        self.status = result.diagnostics.join("; ");
                    }
                }
                Ok(_) => self.status = self.arch_hint().into(),
                Err(e) => self.status = e.into(),
            }
            return;
        }
        if self.architecture.points.is_empty() {
            self.architecture.points.push(point);
            self.status = self.arch_hint().into();
            return;
        }
        let a = self.architecture.points[0];
        let normal = self.architecture.normal.unwrap_or(Vec3::Y);
        let result = if tool == "Add Hole" {
            self.architecture
                .owner
                .clone()
                .ok_or("Opening owner is missing.".into())
                .and_then(|owner| {
                    if !self.native_architecture_editable(&owner) {
                        return Err("Opening owner is read-only.".into());
                    }
                    self.commit_document_edit("Add wall/slab opening", |d| {
                        d.add_architecture_hole(&owner, a, point)
                    })
                    .map(|_| owner)
                })
        } else {
            let points = if tool == "Wall" {
                Ok(vec![a, point])
            } else {
                rectangle(a, point, normal)
            };
            points
                .and_then(|points| {
                    self.arch_element(&tool, &points, normal, &CommandPayload::default())
                })
                .and_then(|e| self.commit_arch_element(e))
        };
        match result {
            Ok(target) => {
                self.selection.apply([target], SelectionMode::Replace);
                if tool == "Wall" {
                    self.architecture.points = vec![point];
                    self.status = self.arch_hint().into();
                } else {
                    self.activate_tool("Select");
                    self.status = format!("{tool} created.");
                }
            }
            Err(e) => self.status = e,
        }
    }
    pub(super) fn commit_arch_slab_path(&mut self, mut path: PolylinePath) -> CommandResult {
        path.closed = true;
        let normal = self.architecture.normal.unwrap_or(Vec3::Y);
        match self
            .arch_element(
                "Freeform Slab",
                &path.points,
                normal,
                &CommandPayload::default(),
            )
            .and_then(|e| self.commit_arch_element(e))
        {
            Ok(_) => CommandResult::completed(),
            Err(e) => CommandResult::failed(e),
        }
    }
    pub(super) fn arch_preview(&self, point: Vec3) -> Option<ToolPreview> {
        let point = self.arch_project(point);
        let tool = &self.active_tool;
        let normal = self.architecture.normal.unwrap_or(Vec3::Y);
        if tool == "Add Hole" {
            let a = *self.architecture.points.first()?;
            return rectangle(a, point, normal)
                .ok()
                .map(|outline| ToolPreview::Polyline {
                    outline,
                    closed: true,
                });
        }
        let points = if tool == "Freeform Slab" {
            self.polyline_tool_state
                .preview(point, self.document.model_settings().arc_max_length)?
                .points
        } else {
            let a = *self.architecture.points.first()?;
            if tool == "Wall" {
                vec![a, point]
            } else {
                rectangle(a, point, normal).ok()?
            }
        };
        let fallback = || ToolPreview::Polyline {
            outline: points.clone(),
            closed: tool != "Wall",
        };
        let Some(mesh) = self
            .arch_element(tool, &points, normal, &CommandPayload::default())
            .and_then(|e| e.mesh())
            .ok()
        else {
            return Some(fallback());
        };
        let (_, _, inverse) = self.arch_owner();
        let world = inverse.inverse(GeometryTolerance::default())?;
        Some(ToolPreview::PushPull {
            edges: mesh
                .edges
                .iter()
                .map(|e| e.map(|i| world.transform_point(mesh.points[i])))
                .collect(),
            triangles: mesh
                .faces
                .iter()
                .map(|(t, c)| (t.map(|i| world.transform_point(mesh.points[i])), *c))
                .collect(),
        })
    }
    pub fn commit_native_slab_control(
        &mut self,
        target: &SelectionTarget,
        index: usize,
        point: Vec3,
    ) -> Result<(), String> {
        if !self.native_architecture_editable(target) {
            return Err(
                "Open the slab's prototype to edit its profile, or unlock its type.".into(),
            );
        }
        self.commit_document_edit("Edit slab profile", |d| {
            d.move_slab_vertex(target, index, point)
        })?;
        self.status = "Slab profile updated.".into();
        Ok(())
    }
    fn arch_payload_target(&self, payload: &CommandPayload) -> Result<PlacedElement, String> {
        if let Some(id) = payload.values.get("id").and_then(Value::as_str) {
            let occurrence = payload
                .values
                .get("occurrence")
                .map(|v| {
                    serde_json::from_value::<OccurrenceRef>(v.clone())
                        .map_err(|_| "occurrence must be {path: [entry IDs]}".to_owned())
                })
                .transpose()?;
            self.document.architecture_elements().into_iter().find(|e| {
                (e.element.id()==id || matches!(&e.target.entity,SelectedEntity::SemanticHandle {id:i,..} if i==id))
                    && occurrence.as_ref().is_none_or(|p|p==&e.target.occurrence)
            }).ok_or_else(||"Architecture element does not exist in the requested occurrence.".into())
        } else {
            self.native_selected_architecture().ok_or_else(|| {
                "Select an architecture element, or provide its id and occurrence.".into()
            })
        }
    }
    fn arch_patch(element: &mut Element, payload: &CommandPayload) -> Result<(), String> {
        if let Some(record) = payload.values.get("record") {
            let patch = record.as_object().ok_or("record must be an object.")?;
            let mut value = element.value();
            value.as_object_mut().unwrap().extend(patch.clone());
            *element = Element::from_value(element.field(), value)?;
        }
        match element {
            Element::Wall(w) => {
                w.thickness = scalar(payload, "wall_thickness", w.thickness)?;
                w.height = scalar(payload, "wall_height", w.height)?;
                w.inclination_deg = scalar(payload, "wall_inclination", w.inclination_deg)?;
                w.exterior_color = color(payload, "wall_exterior_color", w.exterior_color)?;
                w.interior_color = color(payload, "wall_interior_color", w.interior_color)?;
                if let Some(name) = payload.values.get("name").and_then(Value::as_str) {
                    w.name = name.into();
                }
            }
            Element::Slab(s) => {
                s.thickness = scalar(payload, "slab_thickness", s.thickness)?;
                s.top_color = color(payload, "slab_top_color", s.top_color)?;
                s.bottom_color = color(payload, "slab_bottom_color", s.bottom_color)?;
                s.side_color = color(payload, "slab_side_color", s.side_color)?;
                if let Some(name) = payload.values.get("name").and_then(Value::as_str) {
                    s.name = name.into();
                }
            }
            Element::Frame(f) => {
                f.depth = scalar(payload, "frame_depth", f.depth)?;
                f.frame_width = scalar(payload, "frame_width", f.frame_width)?;
                f.color = color(payload, "frame_color", f.color)?;
                f.glazing_color = color(payload, "glazing_color", f.glazing_color)?;
                if let Some(v) = payload.values.get("glazing_enabled") {
                    f.glazing_enabled = v.as_bool().ok_or("glazing_enabled must be boolean.")?;
                }
                if let Some(name) = payload.values.get("name").and_then(Value::as_str) {
                    f.name = name.into();
                }
            }
        }
        Ok(())
    }
    pub(super) fn architecture_action(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        let result = (|| -> Result<Value, String> {
            match action {
                "settings" => {
                    let mut d = self.arch_defaults();
                    let mut w = Element::Wall(d.wall.clone());
                    Self::arch_patch(&mut w, payload)?;
                    let mut s = Element::Slab(d.slab.clone());
                    Self::arch_patch(&mut s, payload)?;
                    let mut f = Element::Frame(d.frame.clone());
                    Self::arch_patch(&mut f, payload)?;
                    if let Element::Wall(v) = w {
                        d.wall = v;
                    }
                    if let Element::Slab(v) = s {
                        d.slab = v;
                    }
                    if let Element::Frame(v) = f {
                        d.frame = v;
                    }
                    let wall = Element::Wall(Wall {
                        start: Vec3::ZERO,
                        end: Vec3::X,
                        ..d.wall.clone()
                    });
                    wall.mesh()?;
                    let slab = Element::Slab(Slab {
                        profile: PolylinePath {
                            points: vec![
                                Vec3::ZERO,
                                Vec3::X * 1e6,
                                (Vec3::X + Vec3::Z) * 1e6,
                                Vec3::Z * 1e6,
                            ],
                            closed: true,
                        },
                        ..d.slab.clone()
                    });
                    slab.mesh()?;
                    let frame = Element::Frame(Frame {
                        contour: vec![
                            Vec3::ZERO,
                            Vec3::X * 1e6,
                            (Vec3::X + Vec3::Y) * 1e6,
                            Vec3::Y * 1e6,
                        ],
                        ..d.frame.clone()
                    });
                    frame.mesh()?;
                    let value = serde_json::to_value(d).map_err(|e| e.to_string())?;
                    self.commit_document_edit("Architecture defaults", |doc| {
                        doc.set_architecture_defaults(value)
                    })?;
                    Ok(json!({"updated":"defaults"}))
                }
                "update" => {
                    let mut e = self.arch_payload_target(payload)?;
                    if !self.arch_editable(&e) {
                        return Err(
                            "Open the prototype to edit its architecture, or unlock the type."
                                .into(),
                        );
                    }
                    Self::arch_patch(&mut e.element, payload)?;
                    let SelectedEntity::SemanticHandle { id, .. } = &e.target.entity else {
                        return Err("Select an architecture element.".into());
                    };
                    let id = id.clone();
                    self.commit_document_edit("Architecture properties", |doc| {
                        doc.put_architecture(e.definition, Some(&id), e.element)
                    })?;
                    Ok(json!({"id":id}))
                }
                "add_hole" => {
                    let e = self.arch_payload_target(payload)?;
                    if !self.arch_editable(&e) {
                        return Err("Opening owner is read-only.".into());
                    }
                    let a = payload_vec3(payload, "start", Vec3::ZERO)?;
                    let b = payload_vec3(payload, "end", Vec3::ZERO)?;
                    let id = self.commit_document_edit("Add wall/slab opening", |doc| {
                        doc.add_architecture_hole(&e.target, a, b)
                    })?;
                    Ok(json!({"hole_id":id}))
                }
                "remove_hole" => {
                    let e = self.arch_payload_target(payload)?;
                    if !self.arch_editable(&e) {
                        return Err("Opening owner is read-only.".into());
                    }
                    let hole = payload
                        .values
                        .get("hole_id")
                        .and_then(Value::as_str)
                        .ok_or("hole_id is required.")?;
                    self.commit_document_edit("Delete owned opening", |doc| {
                        doc.remove_architecture_hole(&e.target, hole)
                    })?;
                    Ok(json!({"removed":hole}))
                }
                "move_slab_vertex" => {
                    let e = self.arch_payload_target(payload)?;
                    let index = payload
                        .values
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or("index must be a nonnegative integer.")?
                        as usize;
                    let point = payload_vec3(payload, "position", Vec3::ZERO)?;
                    self.commit_native_slab_control(&e.target, index, point)?;
                    Ok(json!({"index":index}))
                }
                "create_wall" | "create_rectangular_slab" | "create_slab" | "create_frame" => {
                    let tool = match action {
                        "create_wall" => "Wall",
                        "create_rectangular_slab" => "Rectangular Slab",
                        "create_slab" => "Freeform Slab",
                        _ => {
                            if payload
                                .values
                                .get("kind")
                                .and_then(Value::as_str)
                                .is_some_and(|k| k.eq_ignore_ascii_case("door"))
                            {
                                "Door"
                            } else {
                                "Window"
                            }
                        }
                    };
                    let normal = payload_vec3(
                        payload,
                        "normal",
                        if matches!(tool, "Window" | "Door") {
                            Vec3::Z
                        } else {
                            self.preferences.grid_normal_axis.vector()
                        },
                    )?;
                    let mut points = if let Some(v) = payload.values.get("points") {
                        serde_json::from_value::<Vec<Vec3>>(v.clone())
                            .map_err(|_| "points must be an array of finite XYZ points.")?
                    } else {
                        vec![
                            payload_vec3(payload, "start", Vec3::ZERO)?,
                            payload_vec3(payload, "end", Vec3::ZERO)?,
                        ]
                    };
                    if (tool == "Rectangular Slab" || matches!(tool, "Window" | "Door"))
                        && points.len() == 2
                    {
                        points = rectangle(points[0], points[1], normal)?;
                    }
                    let e = self.arch_element(tool, &points, normal, payload)?;
                    self.commit_arch_element(e)?;
                    let e = self
                        .native_selected_architecture()
                        .ok_or("New architecture element is missing.")?;
                    let SelectedEntity::SemanticHandle { id, .. } = &e.target.entity else {
                        return Err("New architecture identity is missing.".into());
                    };
                    Ok(
                        json!({"id":id,"occurrence":e.target.occurrence,"kind":format!("{:?}",e.element.kind()),"record":e.element.value()}),
                    )
                }
                _ => Err("Unknown architecture command.".into()),
            }
        })();
        match result {
            Ok(value) => {
                self.status = "Architecture updated.".into();
                let mut result = CommandResult::completed();
                result.outputs.insert("architecture".into(), value);
                result
            }
            Err(e) => CommandResult::failed(e),
        }
    }
    pub(super) fn delete_architecture_selection(&mut self) -> CommandResult {
        let mut selected = self.selection.clone();
        selected.clear();
        let mut architecture = vec![];
        for t in self.selection.selected() {
            if let Some(e) = self.document.architecture_element(t) {
                if !self.arch_editable(&e) {
                    return CommandResult::failed("Architecture selection is read-only.");
                }
                if matches!(e.element, Element::Frame(_)) {
                    selected.select(SelectionTarget {
                        occurrence: t.occurrence.clone(),
                        entity: SelectedEntity::Occurrence,
                    });
                } else {
                    architecture.push(t.clone());
                }
            } else {
                selected.select(t.clone());
            }
        }
        match self.commit_document_edit("Delete architecture selection", |doc| {
            for t in &architecture {
                doc.remove_architecture(t)?;
            }
            if !selected.selected().is_empty() {
                modeling_objects::delete_selection(doc, &selected)?;
            }
            Ok(())
        }) {
            Ok(()) => {
                self.selection.clear();
                self.status = "Selection deleted, including owned openings.".into();
                CommandResult::completed()
            }
            Err(e) => CommandResult::failed(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> MeshcraftApp {
        MeshcraftApp::new_native_session(Document::new(), "Architecture test".into(), None)
    }
    fn command(app: &mut MeshcraftApp, id: &str, values: Value) -> CommandResult {
        let payload = CommandPayload {
            values: values
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..Default::default()
        };
        let result = app.dispatch_command(id, &payload).result;
        assert_eq!(
            result.completion,
            CommandCompletion::Completed,
            "{id}: {:?}",
            result.diagnostics
        );
        result
    }
    fn pick(app: &mut MeshcraftApp, p: Vec3) {
        app.handle_native_world_pick(p, None, None);
    }
    fn wall(app: &mut MeshcraftApp) -> SelectionTarget {
        command(
            app,
            "architecture.create_wall",
            json!({"start":[0,0,0],"end":[5,0,0],"wall_height":3}),
        );
        app.native_selected_architecture().unwrap().target
    }
    #[test]
    fn wall_chain_has_solid_feedback_and_commits_each_span_once() {
        let mut app = app();
        command(&mut app, "tool.builtin.arch_wall", json!({}));
        pick(&mut app, Vec3::ZERO);
        let (preview, feedback) = app.native_tool_feedback_at(Vec3::X * 5.0, egui::Pos2::ZERO);
        assert!(
            matches!(preview,Some(ToolPreview::PushPull {triangles,..}) if !triangles.is_empty())
        );
        assert!(feedback.is_some());
        pick(&mut app, Vec3::X * 5.0);
        pick(&mut app, (Vec3::X + Vec3::Z) * 5.0);
        assert_eq!(app.document.architecture_elements().len(), 2);
        assert_eq!(app.history.entries().len(), 2);
        command(&mut app, "tool.finish", json!({}));
        assert_eq!(app.active_tool, "Select");
        command(&mut app, "edit.undo", json!({}));
        assert_eq!(app.document.architecture_elements().len(), 1);
        command(&mut app, "edit.redo", json!({}));
        assert_eq!(app.document.architecture_elements().len(), 2);
    }
    #[test]
    fn rectangular_and_freeform_slabs_share_closed_polyline_parameters() {
        let mut app = app();
        command(&mut app, "tool.builtin.arch_slab", json!({}));
        assert_eq!(app.active_tool, "Rectangular Slab");
        pick(&mut app, Vec3::ZERO);
        pick(&mut app, Vec3::new(5.0, 0.0, 5.0));
        let Element::Slab(rect) = app.native_selected_architecture().unwrap().element else {
            panic!()
        };
        assert!(rect.profile.closed);
        assert_eq!(rect.profile.points.len(), 4);
        command(&mut app, "tool.arch_slab_freeform", json!({}));
        for p in [
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(15.0, 0.0, 0.0),
            Vec3::new(15.0, 0.0, 2.0),
            Vec3::new(12.0, 0.0, 2.0),
            Vec3::new(12.0, 0.0, 5.0),
            Vec3::new(10.0, 0.0, 5.0),
        ] {
            pick(&mut app, p);
        }
        command(&mut app, "tool.finish", json!({}));
        assert_eq!(app.active_tool, "Select");
        let Element::Slab(freeform) = app.native_selected_architecture().unwrap().element else {
            panic!()
        };
        assert!(freeform.profile.closed);
        assert_eq!(freeform.profile.points.len(), 6);
        assert_eq!(app.document.architecture_elements().len(), 2);
    }
    #[test]
    fn windows_and_doors_are_independent_placements_and_do_not_cut_the_wall() {
        let mut app = app();
        let target = wall(&mut app);
        command(
            &mut app,
            "architecture.create_frame",
            json!({"points":[[1,1,0.1],[3,2,0.1]],"normal":[0,0,1],"kind":"window"}),
        );
        let window = app.native_selected_architecture().unwrap();
        assert!(matches!(
            window.element,
            Element::Frame(Frame {
                glazing_enabled: true,
                ..
            })
        ));
        command(
            &mut app,
            "architecture.create_frame",
            json!({"points":[[3.5,0,0.1],[4.5,2.5,0.1]],"normal":[0,0,1],"kind":"door"}),
        );
        let door = app.native_selected_architecture().unwrap();
        assert!(matches!(
            door.element,
            Element::Frame(Frame {
                kind: FrameKind::Door,
                glazing_enabled: false,
                ..
            })
        ));
        assert_ne!(window.definition, door.definition);
        assert_ne!(window.target.occurrence, door.target.occurrence);
        assert!(
            app.document
                .architecture_element(&target)
                .unwrap()
                .element
                .holes()
                .is_empty()
        );
        assert_eq!(app.document.occurrence_refs().unwrap().len(), 2);
    }
    #[test]
    fn interactive_hole_locks_to_the_first_picked_owner_and_is_undoable() {
        let mut app = app();
        let target = wall(&mut app);
        command(&mut app, "tool.builtin.arch_add_hole", json!({}));
        let normal = Vec3::Z;
        let snap = |point| {
            Some(SnapResult {
                point,
                normal,
                kind: SnapKind::Face,
                screen_distance: 0.0,
            })
        };
        let a = Vec3::new(1.0, 0.5, 0.1);
        let b = Vec3::new(2.0, 2.5, 0.1);
        app.handle_native_world_pick(a, snap(a), Some(target.clone()));
        app.handle_native_world_pick(b, snap(b), None);
        assert_eq!(
            app.document
                .architecture_element(&target)
                .unwrap()
                .element
                .holes()
                .len(),
            1
        );
        assert_eq!(app.history.entries().len(), 2);
        command(&mut app, "edit.undo", json!({}));
        assert!(
            app.document
                .architecture_element(&target)
                .unwrap()
                .element
                .holes()
                .is_empty()
        );
        command(&mut app, "edit.redo", json!({}));
        assert_eq!(
            app.document
                .architecture_element(&target)
                .unwrap()
                .element
                .holes()
                .len(),
            1
        );
    }
    #[test]
    fn semantic_ray_picking_respects_architecture_visibility_independently_of_loose_faces() {
        let mut app = app();
        let wall = wall(&mut app);
        app.selection_display_policy
            .set_visible(SelectionKind::Face, false);
        let hit = app
            .pick_native_target(Vec3::new(2.0, 1.0, 3.0), -Vec3::Z, 0.01)
            .unwrap();
        assert_eq!(hit, wall);
        app.selection_display_policy
            .set_visible(SelectionKind::Wall, false);
        assert!(
            app.pick_native_target(Vec3::new(2.0, 1.0, 3.0), -Vec3::Z, 0.01)
                .is_none()
        );
    }
    #[test]
    fn invalid_property_edits_leave_history_and_parameters_unchanged() {
        let mut app = app();
        let target = wall(&mut app);
        let before = app.document.clone();
        let count = app.history.entries().len();
        let result = app.dispatch_command(
            "architecture.update",
            &CommandPayload {
                values: BTreeMap::from([("wall_height".into(), json!(-1))]),
                ..Default::default()
            },
        );
        assert_eq!(result.result.completion, CommandCompletion::Failed);
        assert_eq!(app.document, before);
        assert_eq!(app.history.entries().len(), count);
        command(&mut app, "architecture.update", json!({"wall_height":4.0}));
        assert!(matches!(
            app.document.architecture_element(&target).unwrap().element,
            Element::Wall(Wall { height: 4.0, .. })
        ));
    }
    #[test]
    fn deleting_a_semantic_wall_removes_its_openings_as_one_undoable_edit() {
        let mut app = app();
        let target = wall(&mut app);
        command(
            &mut app,
            "architecture.add_hole",
            json!({"start":[1,0.5,0],"end":[2,2.5,0]}),
        );
        command(&mut app, "edit.delete_selection", json!({}));
        assert!(app.document.architecture_elements().is_empty());
        command(&mut app, "edit.undo", json!({}));
        assert_eq!(
            app.document
                .architecture_element(&target)
                .unwrap()
                .element
                .holes()
                .len(),
            1
        );
    }
    #[test]
    fn drawing_a_slab_on_a_vertical_plane_retains_the_picked_plane() {
        let mut app = app();
        command(&mut app, "tool.builtin.arch_slab", json!({}));
        let a = Vec3::ZERO;
        let b = Vec3::new(3.0, 2.0, 0.0);
        app.handle_native_world_pick(
            a,
            Some(SnapResult {
                point: a,
                normal: Vec3::Z,
                kind: SnapKind::Face,
                screen_distance: 0.0,
            }),
            None,
        );
        app.handle_native_world_pick(b, None, None);
        let Element::Slab(s) = app.native_selected_architecture().unwrap().element else {
            panic!()
        };
        assert_eq!(s.normal, Vec3::Z);
        assert!(s.profile.points.iter().all(|p| p.z.abs() < 1e-9));
    }
    #[test]
    fn opening_started_on_an_edge_uses_its_owner_plane() {
        let mut app = app();
        let target = wall(&mut app);
        command(&mut app, "tool.builtin.arch_add_hole", json!({}));
        let a = Vec3::new(1., 0., 0.1);
        let b = Vec3::new(2., 2., 0.1);
        app.handle_native_world_pick(
            a,
            Some(SnapResult {
                point: a,
                normal: Vec3::Y,
                kind: SnapKind::Endpoint,
                screen_distance: 0.,
            }),
            Some(target.clone()),
        );
        assert!(app.architecture.normal.unwrap().dot(Vec3::Y).abs() < 1e-6);
        app.handle_native_world_pick(b, None, None);
        let wall = app.document.architecture_element(&target).unwrap();
        assert_eq!(wall.element.holes().len(), 1);
        assert!((wall.element.holes()[0].v1 - 2.).abs() < 1e-6);
    }
    #[test]
    fn generated_facets_are_protected_and_mcp_publishes_the_canonical_owner() {
        let mut app = app();
        let target = wall(&mut app);
        let g = app.document.effective_geometry(&target.occurrence).unwrap();
        let facet = SelectionTarget {
            occurrence: target.occurrence.clone(),
            entity: SelectedEntity::Triangle(*g.triangles.keys().next().unwrap()),
        };
        assert!(!app.native_target_editable(&facet));
        assert!(app.native_target_editable(&target));
        let response = app.mcp_geometry(&json!({"path":[]})).unwrap();
        assert_eq!(response["architecture"].as_array().unwrap().len(), 1);
        assert_eq!(
            response["primitives"][0]["target"]["kind"],
            "semantic_handle"
        );
    }
    #[test]
    fn slab_profile_drag_commits_one_undo_and_invalid_release_is_atomic() {
        let mut app = app();
        command(
            &mut app,
            "architecture.create_rectangular_slab",
            json!({"start":[0,0,0],"end":[5,0,5]}),
        );
        let t = app.native_selected_architecture().unwrap().target;
        let before = app.document.architecture_element(&t).unwrap().element;
        let entries = app.history.entries().len();
        app.commit_native_slab_control(&t, 1, Vec3::new(6., 3., 0.))
            .unwrap();
        assert_eq!(app.history.entries().len(), entries + 1);
        let after = app.document.architecture_element(&t).unwrap().element;
        assert_ne!(before, after);
        command(&mut app, "edit.undo", json!({}));
        assert_eq!(
            app.document.architecture_element(&t).unwrap().element,
            before
        );
        command(&mut app, "edit.redo", json!({}));
        assert_eq!(
            app.document.architecture_element(&t).unwrap().element,
            after
        );
        let revision = app.document.revision();
        assert!(
            app.commit_native_slab_control(&t, 0, Vec3::new(5., 0., 5.))
                .is_err()
        );
        assert_eq!(revision, app.document.revision());
    }
}
