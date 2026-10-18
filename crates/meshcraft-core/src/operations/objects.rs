//! Selection-first object grouping, placement, duplication and materialization.

use crate::document::{
    ChildLayout, ChildLayoutId, Definition, DefinitionId, DefinitionKind, Document, GeometryBundle,
    GeometryBundleId, InstanceEdits, LayoutEntry, LayoutEntryId, OccurrenceRef, Segment, SegmentId,
    Triangle, TriangleId, Vertex, VertexId, VertexRef,
};
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use crate::selection::{EditContext, SelectedEntity, SelectionSession, SelectionTarget};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn groupable_domain_extension_key(kind: crate::selection::SelectionKind) -> Option<&'static str> {
    match kind {
        crate::selection::SelectionKind::Dimension => Some("k3d.dimensions"),
        crate::selection::SelectionKind::Text => Some("k3d.texts"),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplodeReport {
    pub exploded_occurrences: usize,
    pub vertices: usize,
    pub segments: usize,
    pub triangles: usize,
    pub selected_results: Vec<SelectionTarget>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeleteReport {
    pub occurrences: usize,
    pub vertices: usize,
    pub segments: usize,
    pub triangles: usize,
}

/// Describes path changes made when a parent occurrence receives a private child layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OccurrencePathRemap {
    parent: OccurrenceRef,
    child_index: usize,
    entry_ids: BTreeMap<LayoutEntryId, LayoutEntryId>,
}

impl OccurrencePathRemap {
    /// Returns the rekeyed path when `occurrence` is below the forked parent's child layout.
    pub fn remap_path(&self, occurrence: &OccurrenceRef) -> Option<OccurrenceRef> {
        if !occurrence.path.starts_with(&self.parent.path)
            || occurrence.path.len() <= self.child_index
        {
            return None;
        }
        let new_id = self.entry_ids.get(&occurrence.path[self.child_index])?;
        let mut path = occurrence.path.clone();
        path[self.child_index] = *new_id;
        Some(OccurrenceRef { path })
    }
}

impl DeleteReport {
    pub fn deleted_count(&self) -> usize {
        self.occurrences + self.vertices + self.segments + self.triangles
    }
}

#[derive(Clone, Debug, Default)]
struct MeshDeleteSet {
    vertices: BTreeSet<VertexId>,
    segments: BTreeSet<SegmentId>,
    triangles: BTreeSet<TriangleId>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MeshDeleteOwner {
    Definition(DefinitionId),
    Occurrence(OccurrenceRef),
}

fn mesh_delete_owner(
    selection: &SelectionSession,
    occurrence: &OccurrenceRef,
    definition: DefinitionId,
) -> MeshDeleteOwner {
    let prototype_edit = match &selection.context {
        EditContext::Definition(active) => *active == definition,
        EditContext::Occurrence(active) => active == occurrence,
        EditContext::Document => occurrence.is_scene_root(),
    };
    if prototype_edit {
        MeshDeleteOwner::Definition(definition)
    } else {
        MeshDeleteOwner::Occurrence(occurrence.clone())
    }
}

/// Returns whether the current selection contains only existing targets supported by Delete.
pub fn can_delete_selection(document: &Document, selection: &SelectionSession) -> bool {
    !selection.selected().is_empty()
        && selection.selected().iter().all(|target| {
            let Some(geometry) = document.effective_geometry(&target.occurrence) else {
                return false;
            };
            match target.entity {
                SelectedEntity::Occurrence => {
                    !target.occurrence.is_scene_root()
                        && document.resolve_occurrence(&target.occurrence).is_some()
                }
                SelectedEntity::Vertex(id) => geometry.vertices.contains_key(&id),
                SelectedEntity::Segment(id) => geometry.segments.contains_key(&id),
                SelectedEntity::Triangle(id) => geometry.triangles.contains_key(&id),
                SelectedEntity::SemanticHandle { .. } => false,
            }
        })
}

/// Deletes selected instances and mesh entities in one atomic edit, isolating ordinary instance
/// geometry edits while preserving prototype edits in the active object context.
pub fn delete_selection(
    document: &mut Document,
    selection: &SelectionSession,
) -> Result<DeleteReport, String> {
    if selection.selected().is_empty() {
        return Err("select objects or mesh entities to delete".to_owned());
    }
    for target in selection.selected() {
        if matches!(target.entity, SelectedEntity::SemanticHandle { .. }) {
            return Err(
                "delete this semantic selection through its domain-specific command".to_owned(),
            );
        }
        if document.resolve_occurrence(&target.occurrence).is_none() {
            return Err("a selected object or mesh owner no longer exists".to_owned());
        }
    }

    let selected_occurrences = selection
        .selected()
        .iter()
        .filter(|target| target.entity == SelectedEntity::Occurrence)
        .map(|target| target.occurrence.clone())
        .collect::<BTreeSet<_>>();
    if selected_occurrences
        .iter()
        .any(OccurrenceRef::is_scene_root)
    {
        return Err("the scene root cannot be deleted as an object instance".to_owned());
    }
    let occurrence_roots = selected_occurrences
        .iter()
        .filter(|occurrence| {
            !selected_occurrences.iter().any(|parent| {
                parent.path.len() < occurrence.path.len()
                    && occurrence.path.starts_with(&parent.path)
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut mesh_selections = BTreeMap::<MeshDeleteOwner, MeshDeleteSet>::new();
    for target in selection.selected() {
        if target.entity == SelectedEntity::Occurrence
            || occurrence_roots
                .iter()
                .any(|occurrence| target.occurrence.path.starts_with(&occurrence.path))
        {
            continue;
        }
        let resolved = document
            .resolve_occurrence(&target.occurrence)
            .expect("selected occurrence was validated above");
        let geometry = document
            .effective_geometry(&target.occurrence)
            .expect("resolved mesh owner has effective geometry");
        let owner = match target.entity {
            SelectedEntity::Vertex(id) => {
                if !geometry.vertices.contains_key(&id) {
                    return Err(format!("selected vertex {} no longer exists", id.0));
                }
                mesh_delete_owner(selection, &target.occurrence, resolved.definition.id)
            }
            SelectedEntity::Segment(id) => {
                if !geometry.segments.contains_key(&id) {
                    return Err(format!("selected segment {} no longer exists", id.0));
                }
                mesh_delete_owner(selection, &target.occurrence, resolved.definition.id)
            }
            SelectedEntity::Triangle(id) => {
                if !geometry.triangles.contains_key(&id) {
                    return Err(format!("selected face {} no longer exists", id.0));
                }
                mesh_delete_owner(selection, &target.occurrence, resolved.definition.id)
            }
            SelectedEntity::Occurrence | SelectedEntity::SemanticHandle { .. } => unreachable!(),
        };
        let entities = mesh_selections.entry(owner).or_default();
        match target.entity {
            SelectedEntity::Vertex(id) => {
                entities.vertices.insert(id);
            }
            SelectedEntity::Segment(id) => {
                entities.segments.insert(id);
            }
            SelectedEntity::Triangle(id) => {
                entities.triangles.insert(id);
            }
            SelectedEntity::Occurrence | SelectedEntity::SemanticHandle { .. } => unreachable!(),
        }
    }

    if occurrence_roots.is_empty() && mesh_selections.is_empty() {
        return Err("the selection contains no deletable objects or mesh entities".to_owned());
    }

    let mut candidate = document.clone();
    let mut report = DeleteReport::default();
    for (owner, selected) in mesh_selections {
        let mut geometry = match &owner {
            MeshDeleteOwner::Definition(id) => candidate
                .definitions
                .get(id)
                .ok_or_else(|| format!("mesh definition {} no longer exists", id.0))?
                .geometry
                .clone(),
            MeshDeleteOwner::Occurrence(occurrence) => candidate
                .effective_geometry(occurrence)
                .cloned()
                .ok_or_else(|| "the selected mesh owner no longer exists".to_owned())?,
        };
        for id in &selected.vertices {
            if !geometry.vertices.contains_key(id) {
                return Err(format!(
                    "selected vertex {} is not in the active mesh graph",
                    id.0
                ));
            }
        }
        for id in &selected.segments {
            if !geometry.segments.contains_key(id) {
                return Err(format!(
                    "selected edge {} is not in the active mesh graph",
                    id.0
                ));
            }
        }
        for id in &selected.triangles {
            if !geometry.triangles.contains_key(id) {
                return Err(format!(
                    "selected face {} is not in the active mesh graph",
                    id.0
                ));
            }
        }

        for vertex in &selected.vertices {
            geometry.vertices.remove(vertex);
        }
        let mut incident_segments = BTreeSet::new();
        for (id, segment) in &geometry.segments {
            if segment
                .endpoints
                .iter()
                .any(|reference| selected.vertices.contains(&reference.id))
            {
                incident_segments.insert(*id);
            }
        }
        let mut incident_triangles = BTreeSet::new();
        for (id, triangle) in &geometry.triangles {
            if triangle
                .vertices
                .iter()
                .any(|reference| selected.vertices.contains(&reference.id))
            {
                incident_triangles.insert(*id);
            }
        }
        for id in selected.segments.union(&incident_segments) {
            report.segments += if geometry.segments.remove(id).is_some() {
                1
            } else {
                0
            };
        }
        for id in selected.triangles.union(&incident_triangles) {
            report.triangles += if geometry.triangles.remove(id).is_some() {
                1
            } else {
                0
            };
        }
        report.vertices += selected.vertices.len();

        match owner {
            MeshDeleteOwner::Definition(id) => {
                candidate
                    .definitions
                    .get_mut(&id)
                    .expect("definition was validated above")
                    .geometry = geometry;
            }
            MeshDeleteOwner::Occurrence(occurrence) => {
                let mut edits = candidate
                    .occurrence_edits
                    .get(&occurrence)
                    .cloned()
                    .unwrap_or_else(InstanceEdits::default);
                edits.geometry_replacement = Some(geometry);
                if !candidate.set_occurrence_edits(occurrence, edits) {
                    return Err("could not store the selected instance's mesh edit".to_owned());
                }
            }
        }
    }

    if !occurrence_roots.is_empty() {
        report.occurrences = candidate.remove_selected_occurrences(&occurrence_roots)?;
    }
    candidate.revision = document
        .revision
        .checked_add(1)
        .ok_or_else(|| "document revision counter is exhausted".to_owned())?;
    candidate.validate()?;
    *document = candidate;
    Ok(report)
}

/// Domain implementations can append their resolved mesh output during explode.
pub trait ObjectExplodeAdapter {
    fn append_resolved_content(
        &self,
        document: &Document,
        occurrence: &OccurrenceRef,
        transform_to_parent: Affine3,
        output: &mut GeometryBundle,
    ) -> Result<(), String>;
}

/// Mesh-only adapter for the capabilities currently implemented by Meshcraft.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeshOnlyExplodeAdapter;

impl ObjectExplodeAdapter for MeshOnlyExplodeAdapter {
    fn append_resolved_content(
        &self,
        document: &Document,
        occurrence: &OccurrenceRef,
        _transform_to_parent: Affine3,
        _output: &mut GeometryBundle,
    ) -> Result<(), String> {
        let resolved = document
            .resolve_occurrence(occurrence)
            .ok_or_else(|| "an object occurrence no longer resolves".to_owned())?;
        if !resolved.definition.domain_data.is_empty()
            || resolved.edits.is_some_and(|edits| {
                !edits.values.is_empty() || !edits.hotspot_positions.is_empty()
            })
        {
            return Err(format!(
                "object {:?} contains domain content without an explode adapter",
                resolved.definition.name
            ));
        }
        Ok(())
    }
}

/// Groups selected object instances into one reusable definition while preserving their local
/// placements and occurrence edits. The selected objects must be direct siblings.
pub fn group_selection(
    document: &mut Document,
    selection: &SelectionSession,
    name: &str,
) -> Result<OccurrenceRef, String> {
    if selection.selected().is_empty() {
        return Err("select one or more objects to create an object definition".to_owned());
    }
    if selection
        .selected()
        .iter()
        .all(|target| target.entity == SelectedEntity::Occurrence)
    {
        let occurrences = selection
            .selected()
            .iter()
            .map(|target| target.occurrence.clone())
            .collect::<Vec<_>>();
        return group_occurrences(document, &occurrences, name);
    }
    if selection
        .selected()
        .iter()
        .all(|target| match &target.entity {
            SelectedEntity::Vertex(_)
            | SelectedEntity::Segment(_)
            | SelectedEntity::Triangle(_) => true,
            SelectedEntity::SemanticHandle { kind, .. } => {
                groupable_domain_extension_key(*kind).is_some()
            }
            SelectedEntity::Occurrence => false,
        })
    {
        return group_mesh_selection(document, selection, name);
    }
    Err(
        "group object instances or supported mesh, dimension and text selections separately"
            .to_owned(),
    )
}

/// Deletes an unused object definition. The root definition and definitions referenced by any
/// stored layout are retained; callers must remove placements before retrying.
pub fn delete_unused_definition(
    document: &mut Document,
    definition_id: DefinitionId,
) -> Result<(), String> {
    if definition_id == Document::SCENE_ROOT_DEFINITION_ID {
        return Err("the scene root definition cannot be deleted".to_owned());
    }
    let Some(definition) = document.definitions.get(&definition_id) else {
        return Err("the object definition does not exist".to_owned());
    };
    let reference_count = document
        .child_layouts
        .values()
        .flat_map(|layout| layout.entries.iter())
        .filter(|entry| entry.definition == definition_id)
        .count();
    if reference_count > 0 {
        return Err(format!(
            "cannot delete definition {:?}: {reference_count} object instance(s) still use it",
            definition.name
        ));
    }

    let mut candidate = document.clone();
    let child_layout = definition.default_child_layout;
    candidate.definitions.remove(&definition_id);
    if let Some(layout_id) = child_layout {
        let shared = candidate
            .definitions
            .values()
            .any(|item| item.default_child_layout == Some(layout_id))
            || candidate
                .occurrence_edits
                .values()
                .any(|edits| edits.child_layout == Some(layout_id));
        if layout_id != candidate.root_layout && !shared {
            candidate.child_layouts.remove(&layout_id);
        }
    }
    candidate.revision = document
        .revision
        .checked_add(1)
        .ok_or_else(|| "document revision counter is exhausted".to_owned())?;
    candidate.validate()?;
    *document = candidate;
    Ok(())
}

/// Moves selected mesh entities and supported annotations into a new nested object. Unselected
/// content remains in place; mesh edits on an ordinary instance create an occurrence-local
/// geometry override. Inherited annotations can only move while editing their definition.
pub fn group_mesh_selection(
    document: &mut Document,
    selection: &SelectionSession,
    name: &str,
) -> Result<OccurrenceRef, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("object name cannot be empty".to_owned());
    }
    let targets = selection.selected().iter().collect::<Vec<_>>();
    let occurrence = targets
        .first()
        .ok_or_else(|| "select mesh entities to create an object definition".to_owned())?
        .occurrence
        .clone();
    if targets.iter().any(|target| target.occurrence != occurrence) {
        return Err("group mesh entities from one geometry owner at a time".to_owned());
    }
    if occurrence.path.is_empty() && document.scene_root_definition().is_none() {
        return Err("the scene root geometry graph is missing".to_owned());
    }

    let mut candidate = document.clone();
    let resolved = candidate
        .resolve_occurrence(&occurrence)
        .ok_or_else(|| "the selected geometry owner no longer exists".to_owned())?;
    let definition_id = resolved.definition.id;
    let source = candidate
        .effective_geometry(&occurrence)
        .cloned()
        .ok_or_else(|| "the selected geometry owner has no mesh".to_owned())?;
    let prototype_edit = match &selection.context {
        EditContext::Definition(active) => *active == definition_id,
        EditContext::Occurrence(active) => active == &occurrence,
        EditContext::Document => occurrence.is_scene_root(),
    };
    let mut selected_domain_ids = BTreeMap::<&'static str, BTreeSet<String>>::new();
    let mut selected_vertices = BTreeSet::new();
    let mut selected_segments = BTreeSet::new();
    let mut selected_triangles = BTreeSet::new();
    for target in targets {
        match &target.entity {
            SelectedEntity::Vertex(id) if source.vertices.contains_key(id) => {
                selected_vertices.insert(*id);
            }
            SelectedEntity::Segment(id) => {
                let segment = source
                    .segments
                    .get(id)
                    .ok_or_else(|| format!("selected segment {} no longer exists", id.0))?;
                selected_segments.insert(*id);
                selected_vertices.extend(segment.endpoints.map(|reference| reference.id));
            }
            SelectedEntity::Triangle(id) => {
                let triangle = source
                    .triangles
                    .get(id)
                    .ok_or_else(|| format!("selected face {} no longer exists", id.0))?;
                selected_triangles.insert(*id);
                selected_vertices.extend(triangle.vertices.map(|reference| reference.id));
            }
            SelectedEntity::SemanticHandle { kind, id } => {
                let Some(field) = groupable_domain_extension_key(*kind) else {
                    return Err(format!(
                        "grouping {:?} selections requires a domain adapter",
                        kind
                    ));
                };
                selected_domain_ids
                    .entry(field)
                    .or_default()
                    .insert(id.clone());
            }
            SelectedEntity::Occurrence => {
                return Err(
                    "group raw mesh and supported domain entities separately from object instances"
                        .to_owned(),
                );
            }
            SelectedEntity::Vertex(_) => {
                return Err("a selected vertex no longer exists".to_owned());
            }
        }
    }

    if !selected_domain_ids.is_empty() && !prototype_edit {
        return Err(
            "group inherited dimensions or text while editing their object definition; occurrence overrides are not supported yet"
                .to_owned(),
        );
    }

    let mut moved_extensions = BTreeMap::new();
    for (field, selected_ids) in &selected_domain_ids {
        let values = candidate
            .definitions
            .get(&definition_id)
            .and_then(|definition| definition.extensions.get(*field))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("selected domain data {field:?} is not available"))?;
        let mut moved = Vec::new();
        let mut remaining = Vec::new();
        let mut counts = BTreeMap::<&str, usize>::new();
        for value in values {
            let id = value
                .as_object()
                .and_then(|record| record.get("id"))
                .and_then(Value::as_str);
            if let Some(id) = id.filter(|id| selected_ids.contains(*id)) {
                *counts.entry(id).or_default() += 1;
                moved.push(value.clone());
            } else {
                remaining.push(value.clone());
            }
        }
        for id in selected_ids {
            match counts.get(id.as_str()).copied() {
                Some(1) => {}
                Some(_) => return Err(format!("selected {field} id {id:?} is ambiguous")),
                None => return Err(format!("selected {field} id {id:?} no longer exists")),
            }
        }
        moved_extensions.insert((*field).to_owned(), Value::Array(moved));
        candidate
            .definitions
            .get_mut(&definition_id)
            .expect("selected definition was resolved")
            .extensions
            .insert((*field).to_owned(), Value::Array(remaining));
    }

    let mut moved_geometry = GeometryBundle::default();
    let mut vertex_map = BTreeMap::new();
    for source_id in &selected_vertices {
        let source_vertex = source.vertices.get(source_id).ok_or_else(|| {
            format!(
                "a selected primitive references missing vertex {}",
                source_id.0
            )
        })?;
        let id = VertexId(candidate.allocate_id());
        vertex_map.insert(*source_id, id);
        moved_geometry.vertices.insert(
            id,
            Vertex {
                id,
                position: source_vertex.position,
                legacy_aliases: source_vertex.legacy_aliases.clone(),
            },
        );
    }
    for source_id in &selected_segments {
        let source_segment = source.segments.get(source_id).expect("selected above");
        let id = SegmentId(candidate.allocate_id());
        moved_geometry.segments.insert(
            id,
            Segment {
                id,
                endpoints: source_segment.endpoints.map(|reference| VertexRef {
                    id: vertex_map[&reference.id],
                    position_offset: reference.position_offset,
                }),
                color: source_segment.color,
                legacy_aliases: source_segment.legacy_aliases.clone(),
                extensions: source_segment.extensions.clone(),
            },
        );
    }
    for source_id in &selected_triangles {
        let source_triangle = source.triangles.get(source_id).expect("selected above");
        let id = TriangleId(candidate.allocate_id());
        moved_geometry.triangles.insert(
            id,
            Triangle {
                id,
                vertices: source_triangle.vertices.map(|reference| VertexRef {
                    id: vertex_map[&reference.id],
                    position_offset: reference.position_offset,
                }),
                color: source_triangle.color,
                legacy_aliases: source_triangle.legacy_aliases.clone(),
                extensions: source_triangle.extensions.clone(),
            },
        );
    }
    if moved_geometry.is_empty() && moved_extensions.is_empty() {
        return Err(
            "select at least one edge, face, vertex, dimension or text to create an object"
                .to_owned(),
        );
    }

    let mut remaining = source.clone();
    for id in &selected_segments {
        remaining.segments.remove(id);
    }
    for id in &selected_triangles {
        remaining.triangles.remove(id);
    }
    let referenced = remaining
        .segments
        .values()
        .flat_map(|segment| segment.endpoints.map(|reference| reference.id))
        .chain(
            remaining
                .triangles
                .values()
                .flat_map(|triangle| triangle.vertices.map(|reference| reference.id)),
        )
        .collect::<BTreeSet<_>>();
    for id in &selected_vertices {
        if !referenced.contains(id) {
            remaining.vertices.remove(id);
        }
    }
    for (key, value) in &source.extensions {
        moved_geometry.extensions.insert(
            format!("source_geometry.{}.{key}", source.id.0),
            value.clone(),
        );
    }

    if prototype_edit || occurrence.is_scene_root() {
        let definition = candidate
            .definitions
            .get_mut(&definition_id)
            .ok_or_else(|| "the selected mesh definition is missing".to_owned())?;
        definition.geometry = remaining;
    } else {
        let mut edits = candidate
            .occurrence_edits
            .get(&occurrence)
            .cloned()
            .unwrap_or_default();
        edits.geometry_replacement = Some(remaining);
        if !candidate.set_occurrence_edits(occurrence.clone(), edits) {
            return Err("could not store the selected instance's mesh override".to_owned());
        }
    }

    moved_geometry.id = GeometryBundleId(candidate.allocate_id());
    let object_definition = DefinitionId(candidate.allocate_id());
    candidate.definitions.insert(
        object_definition,
        Definition {
            id: object_definition,
            name: name.to_owned(),
            kind: if moved_extensions.is_empty() {
                DefinitionKind::Mesh
            } else {
                DefinitionKind::Hybrid
            },
            frame: Affine3::IDENTITY,
            geometry: moved_geometry,
            default_child_layout: None,
            domain_data: BTreeMap::new(),
            extensions: moved_extensions,
            legacy_aliases: Vec::new(),
        },
    );
    let (target_parent, target_layout) = if occurrence.is_scene_root() {
        (OccurrenceRef::scene_root(), candidate.root_layout)
    } else if prototype_edit {
        let layout_id = match candidate
            .definitions
            .get(&definition_id)
            .and_then(|definition| definition.default_child_layout)
        {
            Some(layout_id) => layout_id,
            None => {
                let layout_id = ChildLayoutId(candidate.allocate_id());
                candidate.child_layouts.insert(
                    layout_id,
                    ChildLayout {
                        id: layout_id,
                        entries: Vec::new(),
                        extensions: BTreeMap::new(),
                    },
                );
                candidate
                    .definitions
                    .get_mut(&definition_id)
                    .expect("definition exists")
                    .default_child_layout = Some(layout_id);
                layout_id
            }
        };
        (occurrence.clone(), layout_id)
    } else {
        let layout_id = fork_occurrence_child_layout(&mut candidate, &occurrence)?;
        (occurrence.clone(), layout_id)
    };
    let entry_id = candidate
        .add_layout_entry(target_layout, object_definition, Affine3::IDENTITY)
        .ok_or_else(|| "could not insert the new object into the active graph".to_owned())?;
    candidate.validate()?;
    let grouped = if target_parent.is_scene_root() {
        OccurrenceRef::root(entry_id)
    } else {
        target_parent.child(entry_id)
    };
    *document = candidate;
    Ok(grouped)
}

pub fn group_occurrences(
    document: &mut Document,
    occurrences: &[OccurrenceRef],
    name: &str,
) -> Result<OccurrenceRef, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("object name cannot be empty".to_owned());
    }
    if occurrences.is_empty() || occurrences.iter().any(|item| item.path.is_empty()) {
        return Err("select one or more placed objects to group".to_owned());
    }
    let parent_path = occurrences[0].path[..occurrences[0].path.len() - 1].to_vec();
    if occurrences.iter().any(|item| {
        item.path.len() != parent_path.len() + 1 || item.path[..parent_path.len()] != parent_path
    }) {
        return Err("grouping requires selected objects from one parent context".to_owned());
    }

    let mut candidate = document.clone();
    let parent = OccurrenceRef {
        path: parent_path.clone(),
    };
    let layout_id = child_layout_for_parent(&candidate, &parent)?;
    let selected_ids = occurrences
        .iter()
        .map(|item| *item.path.last().expect("non-empty occurrence"))
        .collect::<BTreeSet<_>>();
    if selected_ids.len() != occurrences.len() {
        return Err("selected object paths must be distinct".to_owned());
    }
    let source_layout = candidate
        .child_layouts
        .get(&layout_id)
        .ok_or_else(|| "the active object layout is missing".to_owned())?;
    let selected_entries = source_layout
        .entries
        .iter()
        .filter(|entry| selected_ids.contains(&entry.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected_entries.len() != selected_ids.len() {
        return Err("one or more selected objects are no longer present".to_owned());
    }

    let origin = selected_entries
        .iter()
        .map(|entry| entry.transform.translation)
        .fold(Vec3::ZERO, |sum, position| sum + position)
        * (1.0 / selected_entries.len() as f64);
    if !origin.is_finite() {
        return Err("selected object origins are not finite".to_owned());
    }

    let mut child_id_map = BTreeMap::new();
    let child_layout_id = ChildLayoutId(candidate.allocate_id());
    let mut child_layout = ChildLayout {
        id: child_layout_id,
        entries: Vec::with_capacity(selected_entries.len()),
        extensions: BTreeMap::new(),
    };
    for mut entry in selected_entries {
        let previous = entry.id;
        entry.id = LayoutEntryId(candidate.allocate_id());
        entry.transform.translation -= origin;
        child_id_map.insert(previous, entry.id);
        child_layout.entries.push(entry);
    }

    let definition_id = DefinitionId(candidate.allocate_id());
    let geometry_id = GeometryBundleId(candidate.allocate_id());
    candidate.definitions.insert(
        definition_id,
        Definition {
            id: definition_id,
            name: name.to_owned(),
            kind: DefinitionKind::Hybrid,
            frame: Affine3::IDENTITY,
            geometry: GeometryBundle {
                id: geometry_id,
                ..GeometryBundle::default()
            },
            default_child_layout: Some(child_layout_id),
            domain_data: BTreeMap::new(),
            extensions: BTreeMap::new(),
            legacy_aliases: Vec::new(),
        },
    );
    candidate
        .child_layouts
        .insert(child_layout_id, child_layout);

    let group_entry_id = LayoutEntryId(candidate.allocate_id());
    let group_entry = LayoutEntry {
        id: group_entry_id,
        definition: definition_id,
        transform: Affine3::from_translation(origin),
        legacy_aliases: Vec::new(),
    };
    let source_layout = candidate
        .child_layouts
        .get_mut(&layout_id)
        .ok_or_else(|| "the active object layout is missing".to_owned())?;
    source_layout
        .entries
        .retain(|entry| !selected_ids.contains(&entry.id));
    source_layout.entries.push(group_entry);

    let mut remapped = BTreeMap::new();
    for (path, mut edits) in std::mem::take(&mut candidate.occurrence_edits) {
        let child_index = parent_path.len();
        let Some(old_child_id) = path.path.get(child_index).copied() else {
            remapped.insert(path, edits);
            continue;
        };
        let Some(new_child_id) = child_id_map.get(&old_child_id).copied() else {
            remapped.insert(path, edits);
            continue;
        };
        let mut new_path = path.path[..child_index].to_vec();
        new_path.push(group_entry_id);
        new_path.push(new_child_id);
        new_path.extend(path.path.iter().skip(child_index + 1).copied());
        if path.path.len() == child_index + 1
            && let Some(transform) = edits.transform.as_mut()
        {
            transform.translation -= origin;
        }
        remapped.insert(OccurrenceRef { path: new_path }, edits);
    }
    candidate.occurrence_edits = remapped;
    candidate.revision = document
        .revision
        .checked_add(1)
        .ok_or_else(|| "document revision counter is exhausted".to_owned())?;
    candidate.validate()?;

    let grouped = parent.child(group_entry_id);
    *document = candidate;
    Ok(grouped)
}

/// Copies a placement under its current parent. Nested parents receive a private child layout,
/// so duplication never changes other instances of the parent definition.
pub fn duplicate_occurrence(
    document: &mut Document,
    source: &OccurrenceRef,
    offset_in_parent: Vec3,
) -> Result<OccurrenceRef, String> {
    duplicate_occurrence_with_remap(document, source, offset_in_parent).map(|(copy, _)| copy)
}

/// Copies a placement and reports occurrence paths that were rekeyed while isolating its parent.
/// Callers holding other selection paths can apply the returned map after a nested copy.
pub fn duplicate_occurrence_with_remap(
    document: &mut Document,
    source: &OccurrenceRef,
    offset_in_parent: Vec3,
) -> Result<(OccurrenceRef, Option<OccurrencePathRemap>), String> {
    if source.path.is_empty() || !offset_in_parent.is_finite() {
        return Err("copy requires a placed object and a finite offset".to_owned());
    }
    let mut candidate = document.clone();
    if candidate.resolve_occurrence(source).is_none() {
        return Err("the selected object no longer exists".to_owned());
    }
    if source.path.len() == 1 {
        let copy = candidate
            .copy_root_occurrence(source, offset_in_parent)
            .ok_or_else(|| "could not copy the selected object instance".to_owned())?;
        candidate.validate()?;
        *document = candidate;
        return Ok((copy, None));
    }

    let parent = OccurrenceRef {
        path: source.path[..source.path.len() - 1].to_vec(),
    };
    let child_index = parent.path.len();
    let source_entry_id = *source.path.last().expect("non-empty path");
    let layout_id = child_layout_for_parent(&candidate, &parent)?;
    let layout = candidate
        .child_layouts
        .get(&layout_id)
        .ok_or_else(|| "the selected object's parent layout is missing".to_owned())?
        .clone();
    let mut entry_map = BTreeMap::new();
    let mut entries = Vec::with_capacity(layout.entries.len());
    for mut entry in layout.entries {
        let old_id = entry.id;
        entry.id = LayoutEntryId(candidate.allocate_id());
        entry_map.insert(old_id, entry.id);
        entries.push(entry);
    }
    let private_layout_id = ChildLayoutId(candidate.allocate_id());
    candidate.child_layouts.insert(
        private_layout_id,
        ChildLayout {
            id: private_layout_id,
            entries,
            extensions: layout.extensions,
        },
    );

    let parent_edits = candidate
        .occurrence_edits
        .get(&parent)
        .cloned()
        .unwrap_or_default();
    let mut parent_edits = parent_edits;
    parent_edits.child_layout = Some(private_layout_id);
    if !candidate.set_occurrence_edits(parent.clone(), parent_edits) {
        return Err("could not create a private parent layout for the copy".to_owned());
    }

    let mut remapped_edits = BTreeMap::new();
    for (path, edits) in std::mem::take(&mut candidate.occurrence_edits) {
        if path.path.starts_with(&parent.path) && path.path.len() > child_index {
            let old_id = path.path[child_index];
            if let Some(new_id) = entry_map.get(&old_id) {
                let mut new_path = path.path.clone();
                new_path[child_index] = *new_id;
                remapped_edits.insert(OccurrenceRef { path: new_path }, edits);
                continue;
            }
        }
        remapped_edits.insert(path, edits);
    }
    candidate.occurrence_edits = remapped_edits;

    let mut source_path = parent.path.clone();
    source_path.push(
        *entry_map
            .get(&source_entry_id)
            .ok_or_else(|| "the selected object disappeared while copying".to_owned())?,
    );
    let source_copy = OccurrenceRef { path: source_path };
    let source_resolved = candidate
        .resolve_occurrence(&source_copy)
        .ok_or_else(|| "the selected object no longer resolves in its parent".to_owned())?;
    let mut copy_entry = source_resolved
        .entry
        .cloned()
        .ok_or_else(|| "the scene root cannot be copied as an object".to_owned())?;
    let source_instance_edits = candidate
        .occurrence_edits
        .get(&source_copy)
        .cloned()
        .unwrap_or_default();
    let mut copy_instance_edits = source_instance_edits.clone();
    let base_transform = source_instance_edits
        .transform
        .unwrap_or(copy_entry.transform);
    let moved_transform = Affine3::from_linear_translation(
        base_transform.linear,
        base_transform.translation + offset_in_parent,
    );
    if source_instance_edits.transform.is_some() {
        copy_instance_edits.transform = Some(moved_transform);
    } else {
        copy_entry.transform = moved_transform;
        copy_instance_edits.transform = None;
    }
    copy_entry.id = LayoutEntryId(candidate.allocate_id());
    let copy_id = copy_entry.id;
    candidate
        .child_layouts
        .get_mut(&private_layout_id)
        .ok_or_else(|| "the private parent layout disappeared".to_owned())?
        .entries
        .push(copy_entry);
    let copy = parent.child(copy_id);

    let subtree_edits = candidate
        .occurrence_edits
        .iter()
        .filter(|(path, _)| path.path.starts_with(&source_copy.path))
        .map(|(path, edits)| (path.clone(), edits.clone()))
        .collect::<Vec<_>>();
    for (old_path, mut edits) in subtree_edits {
        let mut new_path = copy.path.clone();
        new_path.extend(old_path.path.iter().skip(source_copy.path.len()).copied());
        if old_path == source_copy && source_instance_edits.transform.is_none() {
            edits.transform = None;
        } else if old_path == source_copy {
            edits.transform = Some(moved_transform);
        }
        if !instance_edits_empty(&edits)
            && !candidate.set_occurrence_edits(OccurrenceRef { path: new_path }, edits)
        {
            return Err("could not copy object occurrence edits".to_owned());
        }
    }
    candidate.validate()?;
    let remap = OccurrencePathRemap {
        parent,
        child_index,
        entry_ids: entry_map,
    };
    *document = candidate;
    Ok((copy, Some(remap)))
}

/// Adds a definition to the active object graph at a world-space point. If a surface normal is
/// supplied, the definition's local up axis is aligned to it before placement.
pub fn place_definition(
    document: &mut Document,
    context: &EditContext,
    definition_id: DefinitionId,
    world_position: Vec3,
    world_surface_normal: Option<Vec3>,
    glue_to_surface: bool,
) -> Result<OccurrenceRef, String> {
    if !world_position.is_finite() {
        return Err("object placement position must be finite".to_owned());
    }
    let mut candidate = document.clone();
    let definition = candidate
        .definitions
        .get(&definition_id)
        .cloned()
        .ok_or_else(|| "the selected object definition does not exist".to_owned())?;

    let (parent_path, parent_world, parent_definition) = match context {
        EditContext::Document => (Vec::new(), Affine3::IDENTITY, None),
        EditContext::Occurrence(occurrence) => {
            let resolved = candidate
                .resolve_occurrence(occurrence)
                .ok_or_else(|| "the active object context no longer exists".to_owned())?;
            (
                occurrence.path.clone(),
                resolved.world_transform,
                Some(resolved.definition.id),
            )
        }
        EditContext::Definition(id) => {
            if *id == Document::SCENE_ROOT_DEFINITION_ID {
                (Vec::new(), Affine3::IDENTITY, None)
            } else {
                let instance = candidate
                    .occurrence_refs()?
                    .into_iter()
                    .find(|occurrence| {
                        candidate
                            .resolve_occurrence(occurrence)
                            .is_some_and(|resolved| resolved.definition.id == *id)
                    })
                    .ok_or_else(|| "the active definition has no placed instance".to_owned())?;
                let resolved = candidate
                    .resolve_occurrence(&instance)
                    .expect("found above");
                (instance.path, resolved.world_transform, Some(*id))
            }
        }
    };

    let parent_inverse = parent_world
        .inverse(GeometryTolerance::default())
        .ok_or_else(|| "the active object transform is singular".to_owned())?;
    let local_position = parent_inverse.transform_point(world_position);
    let mut placement_linear = Affine3::IDENTITY;
    if glue_to_surface && let Some(world_normal) = world_surface_normal {
        let local_normal = parent_inverse
            .transform_normal(world_normal, GeometryTolerance::default())
            .ok_or_else(|| "the picked surface normal is invalid".to_owned())?;
        let source_up = definition
            .frame
            .transform_normal(Vec3::Y, GeometryTolerance::default())
            .ok_or_else(|| "the object definition frame is singular".to_owned())?;
        placement_linear = rotation_between(source_up, local_normal)?;
    }
    placement_linear.translation =
        local_position - placement_linear.transform_vector(definition.frame.translation);

    let layout_id = if let Some(parent_definition) = parent_definition {
        match candidate
            .definitions
            .get(&parent_definition)
            .and_then(|parent| parent.default_child_layout)
        {
            Some(layout_id) => layout_id,
            None => {
                let layout_id = ChildLayoutId(candidate.allocate_id());
                candidate.child_layouts.insert(
                    layout_id,
                    ChildLayout {
                        id: layout_id,
                        entries: Vec::new(),
                        extensions: BTreeMap::new(),
                    },
                );
                let parent = candidate
                    .definitions
                    .get_mut(&parent_definition)
                    .ok_or_else(|| "the active object definition is missing".to_owned())?;
                parent.default_child_layout = Some(layout_id);
                layout_id
            }
        }
    } else {
        candidate.root_layout
    };
    let entry_id = candidate
        .add_layout_entry(layout_id, definition_id, placement_linear)
        .ok_or_else(|| "could not add the object to the active graph".to_owned())?;
    candidate.validate()?;
    let placed = OccurrenceRef {
        path: [parent_path, vec![entry_id]].concat(),
    };
    // A definition can be edited through one of its placed instances. A recursive placement is
    // rejected by validation before the original document is replaced.
    *document = candidate;
    Ok(placed)
}

/// Explodes selected instances into ordinary mesh geometry in their parent graph.
pub fn explode_selection(
    document: &mut Document,
    selection: &SelectionSession,
    adapter: &impl ObjectExplodeAdapter,
) -> Result<ExplodeReport, String> {
    if selection.selected().is_empty() {
        return Err("select one or more objects to explode".to_owned());
    }
    let mut occurrences = Vec::with_capacity(selection.selected().len());
    for target in selection.selected() {
        if target.entity != SelectedEntity::Occurrence {
            return Err("explode currently requires selected object instances".to_owned());
        }
        occurrences.push(target.occurrence.clone());
    }
    explode_occurrences(document, &occurrences, adapter)
}

pub fn explode_occurrences(
    document: &mut Document,
    occurrences: &[OccurrenceRef],
    adapter: &impl ObjectExplodeAdapter,
) -> Result<ExplodeReport, String> {
    if occurrences.is_empty() || occurrences.iter().any(|item| item.path.is_empty()) {
        return Err("select one or more placed objects to explode".to_owned());
    }
    let unique = occurrences.iter().cloned().collect::<BTreeSet<_>>();
    if unique.len() != occurrences.len() {
        return Err("selected object paths must be distinct".to_owned());
    }
    if unique.iter().any(|path| {
        unique
            .iter()
            .any(|other| path.path.len() < other.path.len() && other.path.starts_with(&path.path))
    }) {
        return Err("explode a parent or its descendants in separate operations".to_owned());
    }

    let mut candidate = document.clone();
    let mut materialized = Vec::with_capacity(occurrences.len());
    for occurrence in occurrences {
        let resolved = candidate
            .resolve_occurrence(occurrence)
            .ok_or_else(|| "a selected object no longer exists".to_owned())?;
        let definition_name = resolved.definition.name.clone();
        let parent = OccurrenceRef {
            path: occurrence.path[..occurrence.path.len() - 1].to_vec(),
        };
        let parent_world = if parent.path.is_empty() {
            Affine3::IDENTITY
        } else {
            candidate
                .resolve_occurrence(&parent)
                .ok_or_else(|| "a selected object's parent no longer exists".to_owned())?
                .world_transform
        };
        let parent_inverse = parent_world
            .inverse(GeometryTolerance::default())
            .ok_or_else(|| "a selected object's parent transform is singular".to_owned())?;
        let mut geometry = GeometryBundle::default();
        append_occurrence_tree(
            &mut candidate,
            occurrence,
            parent_inverse,
            &mut geometry,
            adapter,
        )?;
        geometry.validate()?;
        materialized.push((parent, definition_name, geometry));
    }

    candidate.remove_selected_occurrences(occurrences)?;
    let mut report = ExplodeReport {
        exploded_occurrences: materialized.len(),
        ..ExplodeReport::default()
    };
    for (parent, definition_name, mut geometry) in materialized {
        let layout_id = child_layout_for_parent(&candidate, &parent)?;
        geometry.id = GeometryBundleId(candidate.allocate_id());
        let definition_id = DefinitionId(candidate.allocate_id());
        let definition_name = format!("{definition_name} exploded");
        report.vertices += geometry.vertices.len();
        report.segments += geometry.segments.len();
        report.triangles += geometry.triangles.len();
        let vertex_ids = geometry.vertices.keys().copied().collect::<Vec<_>>();
        let segment_ids = geometry.segments.keys().copied().collect::<Vec<_>>();
        let triangle_ids = geometry.triangles.keys().copied().collect::<Vec<_>>();
        candidate.definitions.insert(
            definition_id,
            Definition {
                id: definition_id,
                name: definition_name,
                kind: DefinitionKind::Mesh,
                frame: Affine3::IDENTITY,
                geometry,
                default_child_layout: None,
                domain_data: BTreeMap::new(),
                extensions: BTreeMap::new(),
                legacy_aliases: Vec::new(),
            },
        );
        let entry_id = candidate
            .add_layout_entry(layout_id, definition_id, Affine3::IDENTITY)
            .ok_or_else(|| "could not insert exploded geometry in the parent graph".to_owned())?;
        let result = parent.child(entry_id);
        if !triangle_ids.is_empty() || !segment_ids.is_empty() {
            report
                .selected_results
                .extend(segment_ids.into_iter().map(|id| SelectionTarget {
                    occurrence: result.clone(),
                    entity: SelectedEntity::Segment(id),
                }));
            report
                .selected_results
                .extend(triangle_ids.into_iter().map(|id| SelectionTarget {
                    occurrence: result.clone(),
                    entity: SelectedEntity::Triangle(id),
                }));
        } else if !vertex_ids.is_empty() {
            report
                .selected_results
                .extend(vertex_ids.into_iter().map(|id| SelectionTarget {
                    occurrence: result.clone(),
                    entity: SelectedEntity::Vertex(id),
                }));
        } else {
            report.selected_results.push(SelectionTarget {
                occurrence: result,
                entity: SelectedEntity::Occurrence,
            });
        }
    }
    candidate.validate()?;
    *document = candidate;
    Ok(report)
}

fn append_occurrence_tree(
    document: &mut Document,
    occurrence: &OccurrenceRef,
    parent_inverse: Affine3,
    output: &mut GeometryBundle,
    adapter: &impl ObjectExplodeAdapter,
) -> Result<(), String> {
    let (relative, source, child_ids) = {
        let resolved = document
            .resolve_occurrence(occurrence)
            .ok_or_else(|| "an object occurrence no longer resolves".to_owned())?;
        let relative = parent_inverse.compose(resolved.world_transform);
        let source = document.effective_geometry(occurrence).cloned();
        let child_layout = resolved
            .edits
            .and_then(|edits| edits.child_layout)
            .or(resolved.definition.default_child_layout);
        let child_ids: Vec<LayoutEntryId> = child_layout
            .map(|layout_id| {
                document
                    .child_layouts
                    .get(&layout_id)
                    .map(|layout| layout.entries.iter().map(|entry| entry.id).collect())
                    .ok_or_else(|| "an object child layout is missing".to_owned())
            })
            .transpose()?
            .unwrap_or_default();
        (relative, source, child_ids)
    };
    if let Some(source) = source {
        append_transformed_bundle(document, &source, relative, output);
    }
    adapter.append_resolved_content(document, occurrence, relative, output)?;
    for entry_id in child_ids {
        append_occurrence_tree(
            document,
            &occurrence.child(entry_id),
            parent_inverse,
            output,
            adapter,
        )?;
    }
    Ok(())
}

fn append_transformed_bundle(
    document: &mut Document,
    source: &GeometryBundle,
    transform: Affine3,
    output: &mut GeometryBundle,
) {
    let mut vertex_ids = BTreeMap::new();
    for vertex in source.vertices.values() {
        let id = VertexId(document.allocate_id());
        vertex_ids.insert(vertex.id, id);
        output.vertices.insert(
            id,
            Vertex {
                id,
                position: transform.transform_point(vertex.position),
                legacy_aliases: vertex.legacy_aliases.clone(),
            },
        );
    }
    for segment in source.segments.values() {
        let id = SegmentId(document.allocate_id());
        let endpoints = segment.endpoints.map(|reference| VertexRef {
            id: vertex_ids[&reference.id],
            position_offset: transform.transform_vector(reference.position_offset),
        });
        output.segments.insert(
            id,
            Segment {
                id,
                endpoints,
                color: segment.color,
                legacy_aliases: segment.legacy_aliases.clone(),
                extensions: segment.extensions.clone(),
            },
        );
    }
    for triangle in source.triangles.values() {
        let id = TriangleId(document.allocate_id());
        let vertices = triangle.vertices.map(|reference| VertexRef {
            id: vertex_ids[&reference.id],
            position_offset: transform.transform_vector(reference.position_offset),
        });
        output.triangles.insert(
            id,
            Triangle {
                id,
                vertices,
                color: triangle.color,
                legacy_aliases: triangle.legacy_aliases.clone(),
                extensions: triangle.extensions.clone(),
            },
        );
    }
    for (key, value) in &source.extensions {
        let key = format!("exploded.{}.{}", source.id.0, key);
        output.extensions.insert(key, value.clone());
    }
}

fn instance_edits_empty(edits: &crate::document::InstanceEdits) -> bool {
    edits.legacy_instance_id.is_none()
        && edits.transform.is_none()
        && edits.geometry_replacement.is_none()
        && edits.child_layout.is_none()
        && edits.hotspot_positions.is_empty()
        && edits.values.is_empty()
        && edits.extensions.is_empty()
}

fn child_layout_for_parent(
    document: &Document,
    parent: &OccurrenceRef,
) -> Result<ChildLayoutId, String> {
    if parent.path.is_empty() {
        return Ok(document.root_layout);
    }
    let resolved = document
        .resolve_occurrence(parent)
        .ok_or_else(|| "the active parent object no longer exists".to_owned())?;
    resolved
        .edits
        .and_then(|edits| edits.child_layout)
        .or(resolved.definition.default_child_layout)
        .ok_or_else(|| "the active parent has no child object layout".to_owned())
}

fn fork_occurrence_child_layout(
    document: &mut Document,
    parent: &OccurrenceRef,
) -> Result<ChildLayoutId, String> {
    let resolved = document
        .resolve_occurrence(parent)
        .ok_or_else(|| "the active parent object no longer exists".to_owned())?;
    let source_layout = resolved
        .edits
        .and_then(|edits| edits.child_layout)
        .or(resolved.definition.default_child_layout)
        .and_then(|id| document.child_layouts.get(&id).cloned())
        .unwrap_or(ChildLayout {
            id: ChildLayoutId(0),
            entries: Vec::new(),
            extensions: BTreeMap::new(),
        });
    let mut entry_map = BTreeMap::new();
    let mut entries = Vec::with_capacity(source_layout.entries.len());
    for mut entry in source_layout.entries {
        let old_id = entry.id;
        entry.id = LayoutEntryId(document.allocate_id());
        entry_map.insert(old_id, entry.id);
        entries.push(entry);
    }
    let layout_id = ChildLayoutId(document.allocate_id());
    document.child_layouts.insert(
        layout_id,
        ChildLayout {
            id: layout_id,
            entries,
            extensions: source_layout.extensions,
        },
    );

    let child_index = parent.path.len();
    let mut remapped = BTreeMap::new();
    for (path, edits) in std::mem::take(&mut document.occurrence_edits) {
        if path.path.starts_with(&parent.path)
            && path.path.len() > child_index
            && let Some(new_id) = entry_map.get(&path.path[child_index])
        {
            let mut new_path = path.path.clone();
            new_path[child_index] = *new_id;
            remapped.insert(OccurrenceRef { path: new_path }, edits);
            continue;
        }
        remapped.insert(path, edits);
    }
    document.occurrence_edits = remapped;
    let mut parent_edits = document
        .occurrence_edits
        .get(parent)
        .cloned()
        .unwrap_or_default();
    parent_edits.child_layout = Some(layout_id);
    if !document.set_occurrence_edits(parent.clone(), parent_edits) {
        return Err("could not create a private child layout".to_owned());
    }
    Ok(layout_id)
}

fn rotation_between(from: Vec3, to: Vec3) -> Result<Affine3, String> {
    let tolerance = GeometryTolerance::default();
    let from = from
        .try_normalized(tolerance.absolute)
        .ok_or_else(|| "the object up axis is invalid".to_owned())?;
    let to = to
        .try_normalized(tolerance.absolute)
        .ok_or_else(|| "the picked surface normal is invalid".to_owned())?;
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot >= 1.0 - tolerance.relative {
        return Ok(Affine3::IDENTITY);
    }
    let axis = if dot <= -1.0 + tolerance.relative {
        let seed = if from.x.abs() < 0.8 { Vec3::X } else { Vec3::Z };
        from.cross(seed)
            .try_normalized(tolerance.absolute)
            .ok_or_else(|| "could not align the object to the picked surface".to_owned())?
    } else {
        from.cross(to)
            .try_normalized(tolerance.absolute)
            .ok_or_else(|| "could not align the object to the picked surface".to_owned())?
    };
    let angle = dot.acos();
    let (sin, cos) = angle.sin_cos();
    let one_minus_cos = 1.0 - cos;
    let x = axis.x;
    let y = axis.y;
    let z = axis.z;
    Ok(Affine3::from_linear_translation(
        [
            [
                cos + x * x * one_minus_cos,
                x * y * one_minus_cos - z * sin,
                x * z * one_minus_cos + y * sin,
            ],
            [
                y * x * one_minus_cos + z * sin,
                cos + y * y * one_minus_cos,
                y * z * one_minus_cos - x * sin,
            ],
            [
                z * x * one_minus_cos - y * sin,
                z * y * one_minus_cos + x * sin,
                cos + z * z * one_minus_cos,
            ],
        ],
        Vec3::ZERO,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        MeshOnlyExplodeAdapter, can_delete_selection, delete_selection, delete_unused_definition,
        duplicate_occurrence, explode_occurrences, group_occurrences, group_selection,
        place_definition,
    };
    use crate::document::{Document, OccurrenceRef};
    use crate::geometry::{Affine3, Vec3};
    use crate::selection::{
        EditContext, SelectedEntity, SelectionKind, SelectionMode, SelectionSession,
        SelectionTarget,
    };

    fn cube(document: &mut Document, name: &str, offset: Vec3) -> OccurrenceRef {
        document
            .add_cuboid(name, Vec3::new(1.0, 1.0, 1.0), offset)
            .unwrap()
    }

    #[test]
    fn deleting_a_face_from_one_instance_keeps_other_shared_instances_unchanged() {
        let mut document = Document::new();
        let first = cube(&mut document, "first", Vec3::ZERO);
        let second = document
            .copy_root_occurrence(&first, Vec3::new(3.0, 0.0, 0.0))
            .unwrap();
        let first_geometry = document.effective_geometry(&first).unwrap();
        let face = *first_geometry.triangles.keys().next().unwrap();
        let target = SelectionTarget {
            occurrence: first.clone(),
            entity: SelectedEntity::Triangle(face),
        };
        let mut selection = SelectionSession::default();
        selection.select(target);

        assert!(can_delete_selection(&document, &selection));
        let report = delete_selection(&mut document, &selection).unwrap();

        assert_eq!(1, report.triangles);
        assert_eq!(
            11,
            document.effective_geometry(&first).unwrap().triangles.len()
        );
        assert_eq!(
            12,
            document
                .effective_geometry(&second)
                .unwrap()
                .triangles
                .len()
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn deleting_a_vertex_cascades_to_incident_edges_and_faces() {
        let mut document = Document::new();
        let cube = cube(&mut document, "cube", Vec3::ZERO);
        let vertex = *document
            .effective_geometry(&cube)
            .unwrap()
            .vertices
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: cube.clone(),
            entity: SelectedEntity::Vertex(vertex),
        });

        let report = delete_selection(&mut document, &selection).unwrap();

        assert_eq!(1, report.vertices);
        assert!(report.segments > 0);
        assert!(report.triangles > 0);
        let geometry = document.effective_geometry(&cube).unwrap();
        assert!(!geometry.vertices.contains_key(&vertex));
        assert!(geometry.validate().is_ok());
    }

    #[test]
    fn deleting_a_selected_parent_and_child_removes_the_parent_once() {
        let mut document = Document::new();
        let parent_mesh = cube(&mut document, "parent mesh", Vec3::ZERO);
        let parent = group_occurrences(&mut document, &[parent_mesh], "parent").unwrap();
        let child = document
            .occurrence_refs()
            .unwrap()
            .into_iter()
            .find(|occurrence| occurrence.path.len() == 2)
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.apply(
            [parent.clone(), child]
                .into_iter()
                .map(|occurrence| SelectionTarget {
                    occurrence,
                    entity: SelectedEntity::Occurrence,
                }),
            SelectionMode::Replace,
        );

        let report = delete_selection(&mut document, &selection).unwrap();

        assert_eq!(1, report.occurrences);
        assert!(document.occurrence_refs().unwrap().is_empty());
    }

    #[test]
    fn unsupported_semantic_selection_does_not_partially_delete_objects() {
        let mut document = Document::new();
        let occurrence = cube(&mut document, "selected", Vec3::ZERO);
        let before = document.clone();
        let mut selection = SelectionSession::default();
        selection.apply(
            [
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Occurrence,
                },
                SelectionTarget {
                    occurrence,
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Hotspot,
                        id: "handle-1".to_owned(),
                    },
                },
            ],
            SelectionMode::Replace,
        );

        assert!(!can_delete_selection(&document, &selection));
        assert!(delete_selection(&mut document, &selection).is_err());
        assert_eq!(before, document);
    }

    #[test]
    fn grouping_nested_siblings_keeps_world_geometry_and_shares_definition() {
        let mut document = Document::new();
        let parent_geometry = cube(&mut document, "parent", Vec3::ZERO);
        let parent = group_occurrences(&mut document, &[parent_geometry], "parent group").unwrap();
        let source_a = cube(&mut document, "a", Vec3::ZERO);
        let source_b = cube(&mut document, "b", Vec3::ZERO);
        let source_definition_a = document
            .resolve_occurrence(&source_a)
            .unwrap()
            .definition
            .id;
        let source_definition_b = document
            .resolve_occurrence(&source_b)
            .unwrap()
            .definition
            .id;
        let first = document
            .add_layout_entry(
                document
                    .resolve_occurrence(&parent)
                    .unwrap()
                    .definition
                    .default_child_layout
                    .unwrap(),
                source_definition_a,
                Affine3::from_translation(Vec3::new(4.0, 0.0, 0.0)),
            )
            .unwrap();
        let second = document
            .add_layout_entry(
                document
                    .resolve_occurrence(&parent)
                    .unwrap()
                    .definition
                    .default_child_layout
                    .unwrap(),
                source_definition_b,
                Affine3::from_translation(Vec3::new(8.0, 0.0, 0.0)),
            )
            .unwrap();
        let before = document
            .resolve_occurrence(&parent.child(first))
            .unwrap()
            .world_transform
            .transform_point(Vec3::ZERO);
        let source_path_a = parent.child(first);
        let source_path_b = parent.child(second);
        let grouped = group_occurrences(
            &mut document,
            &[source_path_a.clone(), source_path_b],
            "nested pair",
        )
        .unwrap();
        let grouped_definition = document.resolve_occurrence(&grouped).unwrap().definition;
        let grouped_child_entry = document
            .child_layout(grouped_definition.default_child_layout.unwrap())
            .unwrap()
            .entries[0]
            .id;
        let after = document
            .resolve_occurrence(&grouped.child(grouped_child_entry))
            .unwrap()
            .world_transform
            .transform_point(Vec3::ZERO);
        assert!(before.distance(after) < 1.0e-10);
        let copies = document
            .array_root_occurrence(&parent, 1, Vec3::new(20.0, 0.0, 0.0))
            .unwrap();
        let instance_a = copies[0].child(grouped.path[1]);
        assert_eq!(
            document.resolve_occurrence(&grouped).unwrap().definition.id,
            document
                .resolve_occurrence(&instance_a)
                .unwrap()
                .definition
                .id
        );
    }

    #[test]
    fn nested_duplicate_is_private_to_the_parent_instance_and_keeps_children() {
        let mut document = Document::new();
        let child = cube(&mut document, "child", Vec3::ZERO);
        let group = group_occurrences(&mut document, &[child], "group").unwrap();
        let child_entry = document
            .resolve_occurrence(&group)
            .unwrap()
            .definition
            .default_child_layout
            .and_then(|layout| document.child_layout(layout))
            .unwrap()
            .entries[0]
            .id;
        let nested_child = group.child(child_entry);
        let duplicate =
            duplicate_occurrence(&mut document, &nested_child, Vec3::new(5.0, 0.0, 0.0)).unwrap();
        let second_parent = document
            .array_root_occurrence(
                &OccurrenceRef::root(group.path[0]),
                1,
                Vec3::new(20.0, 0.0, 0.0),
            )
            .unwrap();
        assert!(document.resolve_occurrence(&duplicate).is_some());
        assert!(document.resolve_occurrence(&second_parent[0]).is_some());
        assert_ne!(duplicate.path[0], second_parent[0].path[0]);
        assert_eq!(6, document.occurrence_refs().unwrap().len());
    }

    #[test]
    fn explode_materializes_transformed_mesh_and_selects_the_new_result() {
        let mut document = Document::new();
        let mesh = cube(&mut document, "mesh", Vec3::ZERO);
        let mesh_definition = document.resolve_occurrence(&mesh).unwrap().definition.id;
        let placed = document
            .add_layout_entry(
                document.root_layout(),
                mesh_definition,
                Affine3::from_translation(Vec3::new(7.0, 2.0, -3.0)),
            )
            .unwrap();
        document.remove_selected_occurrences(&[mesh]).unwrap();
        let instance = OccurrenceRef::root(placed);
        let before = document
            .effective_geometry(&instance)
            .unwrap()
            .vertices
            .values()
            .map(|vertex| {
                document
                    .resolve_occurrence(&instance)
                    .unwrap()
                    .world_transform
                    .transform_point(vertex.position)
            })
            .collect::<Vec<_>>();
        let report = explode_occurrences(
            &mut document,
            std::slice::from_ref(&instance),
            &MeshOnlyExplodeAdapter,
        )
        .unwrap();
        assert_eq!(1, report.exploded_occurrences);
        assert_eq!(8, report.vertices);
        assert_eq!(12, report.triangles);
        assert_eq!(24, report.selected_results.len());
        let result = report.selected_results[0].occurrence.clone();
        let after = document
            .effective_geometry(&result)
            .unwrap()
            .vertices
            .values()
            .map(|vertex| {
                document
                    .resolve_occurrence(&result)
                    .unwrap()
                    .world_transform
                    .transform_point(vertex.position)
            })
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        assert!(document.resolve_occurrence(&instance).is_none());
    }

    #[test]
    fn placement_glues_the_local_up_axis_to_a_tilted_world_normal() {
        let mut document = Document::new();
        let source = cube(&mut document, "prototype", Vec3::ZERO);
        let definition = document.resolve_occurrence(&source).unwrap().definition.id;
        let placement = place_definition(
            &mut document,
            &EditContext::Document,
            definition,
            Vec3::new(3.0, 4.0, 5.0),
            Some(Vec3::new(1.0, 2.0, 3.0)),
            true,
        )
        .unwrap();
        let world = document
            .resolve_occurrence(&placement)
            .unwrap()
            .world_transform;
        let up = world
            .transform_normal(Vec3::Y, crate::geometry::GeometryTolerance::default())
            .unwrap();
        let expected = Vec3::new(1.0, 2.0, 3.0).try_normalized(1.0e-12).unwrap();
        assert!(up.distance(expected) < 1.0e-10);
        let definition_frame = document.definition(definition).unwrap().frame;
        assert!(
            world
                .transform_point(definition_frame.translation)
                .distance(Vec3::new(3.0, 4.0, 5.0))
                < 1.0e-10
        );
    }

    #[test]
    fn placement_in_nested_context_respects_parent_and_definition_frames() {
        let mut document = Document::new();
        let parent = cube(&mut document, "parent", Vec3::ZERO);
        let parent_entry = *parent.path.last().unwrap();
        document
            .child_layouts
            .get_mut(&document.root_layout)
            .unwrap()
            .entries
            .iter_mut()
            .find(|entry| entry.id == parent_entry)
            .unwrap()
            .transform = Affine3::from_linear_translation(
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            Vec3::new(10.0, 4.0, 5.0),
        );
        let source = cube(&mut document, "prototype", Vec3::ZERO);
        let definition = document.resolve_occurrence(&source).unwrap().definition.id;
        let definition_origin = Vec3::new(0.5, 1.0, -0.25);
        document
            .definitions
            .get_mut(&definition)
            .unwrap()
            .frame
            .translation = definition_origin;
        let world_position = Vec3::new(14.0, -2.0, 8.0);
        let world_normal = Vec3::new(1.0, 2.0, -1.0);

        let placed = place_definition(
            &mut document,
            &EditContext::Occurrence(parent.clone()),
            definition,
            world_position,
            Some(world_normal),
            true,
        )
        .unwrap();

        assert_eq!(parent.path.len() + 1, placed.path.len());
        let resolved = document.resolve_occurrence(&placed).unwrap();
        assert!(
            resolved
                .world_transform
                .transform_point(Vec3::ZERO)
                .distance(world_position)
                < 1.0e-10
        );
        let aligned_up = resolved
            .world_transform
            .transform_normal(Vec3::Y, crate::geometry::GeometryTolerance::default())
            .unwrap();
        assert!(aligned_up.distance(world_normal.try_normalized(1.0e-12).unwrap()) < 1.0e-10);
        document.validate().unwrap();
    }

    #[test]
    fn grouping_selected_face_moves_geometry_into_instance_local_object() {
        let mut document = Document::new();
        let first = cube(&mut document, "shared", Vec3::ZERO);
        let definition = document.resolve_occurrence(&first).unwrap().definition.id;
        let second_entry = document
            .add_layout_entry(
                document.root_layout(),
                definition,
                Affine3::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            )
            .unwrap();
        let second = OccurrenceRef::root(second_entry);
        let first_resolved = document.resolve_occurrence(&first).unwrap();
        let source_geometry = document.effective_geometry(&first).unwrap();
        let face_id = *source_geometry.triangles.keys().next().unwrap();
        let source_face = source_geometry.triangles[&face_id].clone();
        let expected = source_face.vertices.map(|reference| {
            first_resolved
                .world_transform
                .transform_point(source_geometry.vertex_position(reference).unwrap())
        });
        let mut selection = SelectionSession::default();
        selection.apply(
            [SelectionTarget {
                occurrence: first.clone(),
                entity: SelectedEntity::Triangle(face_id),
            }],
            SelectionMode::Replace,
        );

        let grouped = group_selection(&mut document, &selection, "Selected Face").unwrap();
        let selected_instance_geometry = document.effective_geometry(&first).unwrap();
        assert_eq!(11, selected_instance_geometry.triangles.len());
        assert_eq!(
            12,
            document
                .definition(definition)
                .unwrap()
                .geometry
                .triangles
                .len()
        );
        assert_eq!(
            12,
            document
                .effective_geometry(&second)
                .unwrap()
                .triangles
                .len()
        );
        let grouped_geometry = document.effective_geometry(&grouped).unwrap();
        assert_eq!(1, grouped_geometry.triangles.len());
        let grouped_resolved = document.resolve_occurrence(&grouped).unwrap();
        let grouped_face = grouped_geometry.triangles.values().next().unwrap();
        let actual = grouped_face.vertices.map(|reference| {
            grouped_resolved
                .world_transform
                .transform_point(grouped_geometry.vertex_position(reference).unwrap())
        });
        assert_eq!(expected, actual);
        assert!(grouped.path.starts_with(&first.path));
        document.validate().unwrap();
    }

    #[test]
    fn grouping_selected_dimensions_and_text_moves_legacy_payload_with_mesh() {
        let mut document = Document::new();
        let instance = cube(&mut document, "annotated", Vec3::ZERO);
        let definition_id = document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        let second_entry = document
            .add_layout_entry(
                document.root_layout(),
                definition_id,
                Affine3::from_translation(Vec3::new(5.0, 0.0, 0.0)),
            )
            .unwrap();
        let second = OccurrenceRef::root(second_entry);
        let face_id = *document
            .effective_geometry(&instance)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let definition = document.definitions.get_mut(&definition_id).unwrap();
        definition.extensions.insert(
            "k3d.dimensions".to_owned(),
            serde_json::json!([
                {"id": "dimension-selected", "start": {"x": 1, "y": 2, "z": 3}},
                {"id": "dimension-retained", "offset": 0.25}
            ]),
        );
        definition.extensions.insert(
            "k3d.texts".to_owned(),
            serde_json::json!([
                {"id": "text-selected", "text": "Door", "position": {"x": 4, "y": 5, "z": 6}}
            ]),
        );

        let mut selection = SelectionSession::default();
        selection.context = EditContext::Definition(definition_id);
        selection.apply(
            [
                SelectionTarget {
                    occurrence: instance.clone(),
                    entity: SelectedEntity::Triangle(face_id),
                },
                SelectionTarget {
                    occurrence: instance.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Dimension,
                        id: "dimension-selected".to_owned(),
                    },
                },
                SelectionTarget {
                    occurrence: instance.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Text,
                        id: "text-selected".to_owned(),
                    },
                },
            ],
            SelectionMode::Replace,
        );

        let grouped = group_selection(&mut document, &selection, "Selected annotations").unwrap();

        let source = document.definition(definition_id).unwrap();
        assert_eq!(11, source.geometry.triangles.len());
        assert_eq!(
            serde_json::json!([{"id": "dimension-retained", "offset": 0.25}]),
            source.extensions["k3d.dimensions"]
        );
        assert_eq!(serde_json::json!([]), source.extensions["k3d.texts"]);
        let grouped_definition = document.resolve_occurrence(&grouped).unwrap().definition;
        assert_eq!(
            crate::document::DefinitionKind::Hybrid,
            grouped_definition.kind
        );
        assert_eq!(1, grouped_definition.geometry.triangles.len());
        assert_eq!(
            serde_json::json!([{
                "id": "dimension-selected",
                "start": {"x": 1, "y": 2, "z": 3}
            }]),
            grouped_definition.extensions["k3d.dimensions"]
        );
        assert_eq!(
            serde_json::json!([{
                "id": "text-selected",
                "text": "Door",
                "position": {"x": 4, "y": 5, "z": 6}
            }]),
            grouped_definition.extensions["k3d.texts"]
        );
        let grouped_child_id = *grouped.path.last().unwrap();
        assert!(
            document
                .resolve_occurrence(&second.child(grouped_child_id))
                .is_some()
        );
        let exported = crate::legacy::export_current_value(&document).unwrap();
        let prototypes = exported["prototypes"].as_array().unwrap();
        let source_prototype = prototypes
            .iter()
            .find(|prototype| prototype["name"] == "annotated")
            .unwrap();
        let grouped_prototype = prototypes
            .iter()
            .find(|prototype| prototype["name"] == "Selected annotations")
            .unwrap();
        assert_eq!(
            serde_json::json!([{"id": "dimension-retained", "offset": 0.25}]),
            source_prototype["dimensions"]
        );
        assert_eq!(serde_json::json!([]), source_prototype["texts"]);
        assert_eq!(
            serde_json::json!([{
                "id": "dimension-selected",
                "start": {"x": 1, "y": 2, "z": 3}
            }]),
            grouped_prototype["dimensions"]
        );
        assert_eq!(
            serde_json::json!([{
                "id": "text-selected",
                "text": "Door",
                "position": {"x": 4, "y": 5, "z": 6}
            }]),
            grouped_prototype["texts"]
        );
        document.validate().unwrap();
    }

    #[test]
    fn grouping_inherited_domain_data_from_one_instance_is_rejected_atomically() {
        let mut document = Document::new();
        let instance = cube(&mut document, "shared", Vec3::ZERO);
        let definition_id = document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        document
            .definitions
            .get_mut(&definition_id)
            .unwrap()
            .extensions
            .insert(
                "k3d.dimensions".to_owned(),
                serde_json::json!([{"id": "dimension-1", "text": "length"}]),
            );
        let before = document.clone();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: instance,
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Dimension,
                id: "dimension-1".to_owned(),
            },
        });

        let error = group_selection(&mut document, &selection, "Moved annotation").unwrap_err();

        assert!(error.contains("editing their object definition"));
        assert_eq!(before, document);
    }

    #[test]
    fn prototype_deletion_refuses_live_instances_and_removes_unused_definition() {
        let mut document = Document::new();
        let instance = cube(&mut document, "prototype", Vec3::ZERO);
        let definition = document
            .resolve_occurrence(&instance)
            .unwrap()
            .definition
            .id;
        let before = document.clone();

        let error = delete_unused_definition(&mut document, definition).unwrap_err();

        assert!(error.contains("1 object instance(s) still use it"));
        assert_eq!(before, document);
        document.remove_selected_occurrences(&[instance]).unwrap();
        let revision = document.revision();
        delete_unused_definition(&mut document, definition).unwrap();
        assert!(document.definition(definition).is_none());
        assert_eq!(revision + 1, document.revision());
        document.validate().unwrap();
    }
}
