//! Selection-first geometric transforms and appearance edits.

use crate::document::{
    DefinitionId, Document, GeometryBundle, OccurrenceRef, Segment, SegmentId, Triangle,
    TriangleId, VertexId, VertexRef,
};
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use crate::selection::{
    EditContext, SelectedEntity, SelectionKind, SelectionMode, SelectionSession, SelectionTarget,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransformReport {
    pub transformed_vertices: usize,
    pub copied_vertices: usize,
    pub copied_segments: usize,
    pub copied_triangles: usize,
    pub transformed_occurrences: usize,
    pub copied_occurrences: usize,
    pub transformed_domain_entities: usize,
    pub copied_domain_entities: usize,
}

impl TransformReport {
    pub fn changed(&self) -> bool {
        self.transformed_vertices > 0
            || self.copied_vertices > 0
            || self.copied_segments > 0
            || self.copied_triangles > 0
            || self.transformed_occurrences > 0
            || self.copied_occurrences > 0
            || self.transformed_domain_entities > 0
            || self.copied_domain_entities > 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PaintReport {
    pub painted_segments: usize,
    pub painted_triangles: usize,
}

impl PaintReport {
    pub fn changed(&self) -> bool {
        self.painted_segments > 0 || self.painted_triangles > 0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum GeometryOwner {
    Definition(DefinitionId),
    Occurrence(OccurrenceRef),
}

#[derive(Clone, Debug, Default)]
struct GeometrySelection {
    occurrence_targets: BTreeSet<OccurrenceRef>,
    vertices: BTreeSet<VertexId>,
    explicit_vertices: BTreeSet<VertexId>,
    segments: BTreeSet<SegmentId>,
    triangles: BTreeSet<TriangleId>,
    transform_occurrence: Option<OccurrenceRef>,
}

#[derive(Clone, Debug, Default)]
struct DomainSelection {
    occurrence_targets: BTreeSet<OccurrenceRef>,
    ids_by_field: BTreeMap<&'static str, BTreeSet<String>>,
}

/// Applies one world-space affine transform to the selected mesh entities and object instances.
/// Previews should call this with a cloned document; committing should wrap it in document history.
/// A copy leaves source data untouched and selects the newly created entities/instances.
pub fn transform_selection(
    document: &mut Document,
    selection: &mut SelectionSession,
    world_transform: Affine3,
    copy: bool,
) -> Result<TransformReport, String> {
    transform_selection_with_path_remaps(document, selection, world_transform, copy)
        .map(|(report, _)| report)
}

/// Applies a transform and returns nested-layout path remaps in operation order.
/// Array operations use them to keep their source selection and prior results valid.
pub(crate) fn transform_selection_with_path_remaps(
    document: &mut Document,
    selection: &mut SelectionSession,
    world_transform: Affine3,
    copy: bool,
) -> Result<
    (
        TransformReport,
        Vec<crate::operations::objects::OccurrencePathRemap>,
    ),
    String,
> {
    if !world_transform.is_finite()
        || world_transform
            .inverse(GeometryTolerance::default())
            .is_none()
    {
        return Err("transform must be finite and invertible".to_owned());
    }
    if is_identity(world_transform) {
        return Ok((TransformReport::default(), Vec::new()));
    }

    let selected = selection
        .selected()
        .iter()
        .filter(|target| target_in_context(document, &selection.context, target))
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("transform needs a supported selection in the active context".to_owned());
    }

    let selected_objects = selected
        .iter()
        .filter_map(|target| {
            matches!(target.entity, SelectedEntity::Occurrence).then_some(target.occurrence.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut selected_objects = selected_objects
        .iter()
        .filter(|occurrence| {
            !selected_objects.iter().any(|ancestor| {
                ancestor.path.len() < occurrence.path.len()
                    && occurrence.path.starts_with(&ancestor.path)
            })
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut geometry = BTreeMap::<GeometryOwner, GeometrySelection>::new();
    let mut domain = BTreeMap::<DefinitionId, DomainSelection>::new();
    for target in &selected {
        if matches!(target.entity, SelectedEntity::Occurrence)
            || selected_objects
                .iter()
                .any(|occurrence| target.occurrence.path.starts_with(&occurrence.path))
        {
            continue;
        }
        let Some(resolved) = document.resolve_occurrence(&target.occurrence) else {
            continue;
        };
        if let SelectedEntity::SemanticHandle { kind, id } = &target.entity {
            let field = match kind {
                SelectionKind::Dimension => "dimensions",
                SelectionKind::Text => "texts",
                _ => {
                    return Err(format!(
                        "transform does not support semantic selection kind {kind:?}"
                    ));
                }
            };
            let owner = geometry_owner(&selection.context, target, resolved.definition.id);
            let GeometryOwner::Definition(definition) = owner else {
                return Err(format!(
                    "transforming {field} on an inherited object instance needs occurrence-level domain overrides"
                ));
            };
            let group = domain.entry(definition).or_default();
            group.occurrence_targets.insert(target.occurrence.clone());
            group
                .ids_by_field
                .entry(field)
                .or_default()
                .insert(id.clone());
            continue;
        }
        let owner = geometry_owner(&selection.context, target, resolved.definition.id);
        let group = geometry.entry(owner).or_default();
        group.occurrence_targets.insert(target.occurrence.clone());
        group
            .transform_occurrence
            .get_or_insert_with(|| target.occurrence.clone());
        let bundle = document.effective_geometry(&target.occurrence);
        let Some(bundle) = bundle else {
            continue;
        };
        match target.entity {
            SelectedEntity::Vertex(id) if bundle.vertices.contains_key(&id) => {
                group.vertices.insert(id);
                group.explicit_vertices.insert(id);
            }
            SelectedEntity::Segment(id) => {
                let Some(segment) = bundle.segments.get(&id) else {
                    continue;
                };
                group.segments.insert(id);
                group
                    .vertices
                    .extend(segment.endpoints.map(|reference| reference.id));
            }
            SelectedEntity::Triangle(id) => {
                let Some(triangle) = bundle.triangles.get(&id) else {
                    continue;
                };
                group.triangles.insert(id);
                group
                    .vertices
                    .extend(triangle.vertices.map(|reference| reference.id));
            }
            SelectedEntity::Occurrence
            | SelectedEntity::SemanticHandle { .. }
            | SelectedEntity::Vertex(_) => {}
        }
    }
    if selected_objects.is_empty()
        && !geometry.values().any(|group| !group.vertices.is_empty())
        && domain.is_empty()
    {
        return Err(
            "transform needs selected mesh geometry, annotation, or object instance".to_owned(),
        );
    }

    let mut candidate = document.clone();
    let mut candidate_selection = selection.clone();
    let mut report = TransformReport::default();
    let mut copy_targets = BTreeSet::new();
    let mut path_remaps = Vec::new();

    while let Some(occurrence) = selected_objects.pop_first() {
        if copy {
            let (duplicate, remap) = crate::operations::objects::duplicate_occurrence_with_remap(
                &mut candidate,
                &occurrence,
                Vec3::ZERO,
            )?;
            if let Some(remap) = remap {
                remap_occurrence_set(&mut selected_objects, &remap);
                remap_geometry_selections(&mut geometry, &remap);
                remap_domain_selections(&mut domain, &remap);
                copy_targets = remap_selection_targets(copy_targets, &remap);
                path_remaps.push(remap);
            }
            transform_occurrence(&mut candidate, &duplicate, world_transform)?;
            copy_targets.insert(SelectionTarget {
                occurrence: duplicate,
                entity: SelectedEntity::Occurrence,
            });
            report.copied_occurrences += 1;
        } else if transform_occurrence(&mut candidate, &occurrence, world_transform)? {
            report.transformed_occurrences += 1;
        }
    }

    transform_domain_selections(
        &mut candidate,
        &domain,
        world_transform,
        copy,
        &mut report,
        &mut copy_targets,
    )?;

    for (owner, group) in geometry {
        let Some(occurrence) = group.transform_occurrence.as_ref() else {
            continue;
        };
        let Some(resolved) = candidate.resolve_occurrence(occurrence) else {
            return Err("selected geometry occurrence is no longer resolvable".to_owned());
        };
        let inverse = resolved
            .world_transform
            .inverse(GeometryTolerance::default())
            .ok_or_else(|| "selected geometry has a singular world transform".to_owned())?;
        let local_transform = inverse
            .compose(world_transform)
            .compose(resolved.world_transform);
        let mut bundle = match &owner {
            GeometryOwner::Definition(id) => candidate
                .definitions
                .get(id)
                .map(|definition| definition.geometry.clone())
                .ok_or_else(|| format!("selected definition {} is missing", id.0))?,
            GeometryOwner::Occurrence(instance) => candidate
                .effective_geometry(instance)
                .cloned()
                .ok_or_else(|| "selected occurrence geometry is missing".to_owned())?,
        };
        let result =
            transform_geometry_bundle(&mut candidate, &mut bundle, &group, local_transform, copy)?;
        report.transformed_vertices += result.transformed_vertices;
        report.copied_vertices += result.copied_vertices;
        report.copied_segments += result.copied_segments;
        report.copied_triangles += result.copied_triangles;
        if copy {
            for target_occurrence in &group.occurrence_targets {
                copy_targets.extend(
                    result
                        .targets
                        .iter()
                        .cloned()
                        .map(|entity| SelectionTarget {
                            occurrence: target_occurrence.clone(),
                            entity,
                        }),
                );
            }
        }
        match owner {
            GeometryOwner::Definition(id) => {
                let definition = candidate
                    .definitions
                    .get_mut(&id)
                    .ok_or_else(|| format!("selected definition {} is missing", id.0))?;
                definition.geometry = bundle;
                candidate.revision = candidate.revision.saturating_add(1);
            }
            GeometryOwner::Occurrence(instance) => {
                let mut edits = candidate
                    .occurrence_edits(&instance)
                    .cloned()
                    .unwrap_or_default();
                edits.geometry_replacement = Some(bundle);
                if !candidate.set_occurrence_edits(instance, edits) {
                    return Err("could not store transformed instance geometry".to_owned());
                }
            }
        }
    }

    if !report.changed() {
        return Ok((report, path_remaps));
    }
    if copy {
        if copy_targets.is_empty() {
            return Err("copy mode needs a selected entity or object".to_owned());
        }
        candidate_selection.apply(copy_targets, SelectionMode::Replace);
    }
    candidate.validate()?;
    candidate.revision = document
        .revision()
        .checked_add(1)
        .ok_or_else(|| "document revision counter is exhausted".to_owned())?;
    *document = candidate;
    *selection = candidate_selection;
    Ok((report, path_remaps))
}

fn remapped_occurrence(
    occurrence: &OccurrenceRef,
    remap: &crate::operations::objects::OccurrencePathRemap,
) -> OccurrenceRef {
    remap
        .remap_path(occurrence)
        .unwrap_or_else(|| occurrence.clone())
}

fn remap_occurrence_set(
    occurrences: &mut BTreeSet<OccurrenceRef>,
    remap: &crate::operations::objects::OccurrencePathRemap,
) {
    *occurrences = std::mem::take(occurrences)
        .into_iter()
        .map(|occurrence| remapped_occurrence(&occurrence, remap))
        .collect();
}

fn remap_selection_targets(
    targets: BTreeSet<SelectionTarget>,
    remap: &crate::operations::objects::OccurrencePathRemap,
) -> BTreeSet<SelectionTarget> {
    targets
        .into_iter()
        .map(|mut target| {
            target.occurrence = remapped_occurrence(&target.occurrence, remap);
            target
        })
        .collect()
}

fn remap_geometry_selections(
    groups: &mut BTreeMap<GeometryOwner, GeometrySelection>,
    remap: &crate::operations::objects::OccurrencePathRemap,
) {
    let previous = std::mem::take(groups);
    for (owner, mut group) in previous {
        let owner = match owner {
            GeometryOwner::Definition(id) => GeometryOwner::Definition(id),
            GeometryOwner::Occurrence(occurrence) => {
                GeometryOwner::Occurrence(remapped_occurrence(&occurrence, remap))
            }
        };
        remap_occurrence_set(&mut group.occurrence_targets, remap);
        group.transform_occurrence = group
            .transform_occurrence
            .as_ref()
            .map(|occurrence| remapped_occurrence(occurrence, remap));
        let destination = groups.entry(owner).or_default();
        destination
            .occurrence_targets
            .extend(group.occurrence_targets);
        destination.vertices.extend(group.vertices);
        destination
            .explicit_vertices
            .extend(group.explicit_vertices);
        destination.segments.extend(group.segments);
        destination.triangles.extend(group.triangles);
        if destination.transform_occurrence.is_none() {
            destination.transform_occurrence = group.transform_occurrence;
        }
    }
}

fn remap_domain_selections(
    groups: &mut BTreeMap<DefinitionId, DomainSelection>,
    remap: &crate::operations::objects::OccurrencePathRemap,
) {
    for group in groups.values_mut() {
        remap_occurrence_set(&mut group.occurrence_targets, remap);
    }
}

fn transform_domain_selections(
    document: &mut Document,
    groups: &BTreeMap<DefinitionId, DomainSelection>,
    world_transform: Affine3,
    copy: bool,
    report: &mut TransformReport,
    copy_targets: &mut BTreeSet<SelectionTarget>,
) -> Result<(), String> {
    for (definition_id, group) in groups {
        let mut local_transform = None;
        for occurrence in &group.occurrence_targets {
            let resolved = document.resolve_occurrence(occurrence).ok_or_else(|| {
                "selected annotation occurrence is no longer resolvable".to_owned()
            })?;
            if resolved.definition.id != *definition_id {
                return Err("selected annotation no longer belongs to its definition".to_owned());
            }
            let inverse = resolved
                .world_transform
                .inverse(GeometryTolerance::default())
                .ok_or_else(|| "selected annotation has a singular world transform".to_owned())?;
            let occurrence_transform = inverse
                .compose(world_transform)
                .compose(resolved.world_transform);
            if local_transform.is_some_and(|existing| existing != occurrence_transform) {
                return Err(
                    "shared annotations selected through different instances need different local transforms"
                        .to_owned(),
                );
            }
            local_transform = Some(occurrence_transform);
        }
        let local_transform = local_transform
            .ok_or_else(|| "selected annotation has no resolvable occurrence".to_owned())?;

        let field_records = {
            let definition = document
                .definitions
                .get(definition_id)
                .ok_or_else(|| format!("selected definition {} is missing", definition_id.0))?;
            group
                .ids_by_field
                .iter()
                .map(|(field, selected_ids)| {
                    let key = format!("k3d.{field}");
                    let records = definition
                        .extensions
                        .get(&key)
                        .and_then(serde_json::Value::as_array)
                        .cloned()
                        .ok_or_else(|| format!("selected {field} records are missing"))?;
                    let identities = definition
                        .domain_entity_ids
                        .get(*field)
                        .cloned()
                        .ok_or_else(|| format!("selected {field} identities are missing"))?;
                    if identities.len() != records.len() {
                        return Err(format!(
                            "selected {field} records and identities do not align"
                        ));
                    }
                    Ok((*field, selected_ids, records, identities))
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        let mut updates = Vec::new();
        let mut generated = Vec::new();
        for (field, selected_ids, records, identities) in field_records {
            let key = format!("k3d.{field}");
            let mut next_records = records.clone();
            let mut next_identities = identities.clone();
            let mut changed = false;
            for selected_id in selected_ids {
                let mut matches = identities
                    .iter()
                    .enumerate()
                    .filter(|(_, identity)| *identity == selected_id);
                let Some((index, _)) = matches.next() else {
                    return Err(format!(
                        "selected {field} identity {selected_id:?} is missing"
                    ));
                };
                if matches.next().is_some() {
                    return Err(format!(
                        "selected {field} identity {selected_id:?} is ambiguous"
                    ));
                }
                let transformed = crate::operations::objects::transform_annotation_record(
                    field,
                    &records[index],
                    local_transform,
                )?;
                if copy {
                    let fresh = document.allocate_id();
                    let new_id = format!("meshcraft:transform:{}:{field}:{fresh}", definition_id.0);
                    next_records.push(transformed);
                    next_identities.push(new_id.clone());
                    generated.push((field, new_id));
                    report.copied_domain_entities += 1;
                    changed = true;
                } else if transformed != records[index] {
                    next_records[index] = transformed;
                    report.transformed_domain_entities += 1;
                    changed = true;
                }
            }
            if changed {
                updates.push((key, field, next_records, next_identities));
            }
        }

        if !updates.is_empty() {
            let definition = document
                .definitions
                .get_mut(definition_id)
                .ok_or_else(|| format!("selected definition {} is missing", definition_id.0))?;
            for (key, field, records, identities) in updates {
                definition
                    .extensions
                    .insert(key, serde_json::Value::Array(records));
                definition
                    .domain_entity_ids
                    .insert(field.to_owned(), identities);
            }
        }
        if copy {
            for occurrence in &group.occurrence_targets {
                for (field, id) in &generated {
                    let kind = match *field {
                        "dimensions" => SelectionKind::Dimension,
                        "texts" => SelectionKind::Text,
                        _ => unreachable!("domain transform fields are validated at collection"),
                    };
                    copy_targets.insert(SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::SemanticHandle {
                            kind,
                            id: id.clone(),
                        },
                    });
                }
            }
        }
    }
    Ok(())
}

/// Applies the current RGBA value to selected faces and edges in the active edit context.
pub fn paint_selection(
    document: &mut Document,
    selection: &SelectionSession,
    color: crate::document::Rgba8,
) -> Result<PaintReport, String> {
    let selected = selection
        .selected()
        .iter()
        .filter(|target| target_in_context(document, &selection.context, target))
        .cloned()
        .collect::<Vec<_>>();
    let mut groups = BTreeMap::<GeometryOwner, (BTreeSet<SegmentId>, BTreeSet<TriangleId>)>::new();
    for target in selected {
        let Some(resolved) = document.resolve_occurrence(&target.occurrence) else {
            continue;
        };
        let owner = geometry_owner(&selection.context, &target, resolved.definition.id);
        let group = groups.entry(owner).or_default();
        match target.entity {
            SelectedEntity::Segment(id) => {
                group.0.insert(id);
            }
            SelectedEntity::Triangle(id) => {
                group.1.insert(id);
            }
            _ => {}
        }
    }
    if groups.is_empty()
        || groups
            .values()
            .all(|(edges, faces)| edges.is_empty() && faces.is_empty())
    {
        return Err("paint needs a selected edge or face".to_owned());
    }

    let mut candidate = document.clone();
    let mut report = PaintReport::default();
    for (owner, (edges, faces)) in groups {
        let mut geometry = match &owner {
            GeometryOwner::Definition(id) => candidate
                .definitions
                .get(id)
                .map(|definition| definition.geometry.clone())
                .ok_or_else(|| format!("selected definition {} is missing", id.0))?,
            GeometryOwner::Occurrence(instance) => candidate
                .effective_geometry(instance)
                .cloned()
                .ok_or_else(|| "selected occurrence geometry is missing".to_owned())?,
        };
        let mut group_changed = false;
        for id in &edges {
            if let Some(segment) = geometry.segments.get_mut(id)
                && segment.color != color
            {
                segment.color = color;
                report.painted_segments += 1;
                group_changed = true;
            }
        }
        for id in &faces {
            if let Some(triangle) = geometry.triangles.get_mut(id)
                && triangle.color != color
            {
                triangle.color = color;
                report.painted_triangles += 1;
                group_changed = true;
            }
        }
        if !group_changed {
            continue;
        }
        match owner {
            GeometryOwner::Definition(id) => {
                candidate
                    .definitions
                    .get_mut(&id)
                    .ok_or_else(|| format!("selected definition {} is missing", id.0))?
                    .geometry = geometry;
            }
            GeometryOwner::Occurrence(instance) => {
                let mut edits = candidate
                    .occurrence_edits(&instance)
                    .cloned()
                    .unwrap_or_default();
                edits.geometry_replacement = Some(geometry);
                if !candidate.set_occurrence_edits(instance, edits) {
                    return Err("could not store painted instance geometry".to_owned());
                }
            }
        }
    }
    if !report.changed() {
        return Ok(report);
    }
    candidate.validate()?;
    candidate.revision = document
        .revision()
        .checked_add(1)
        .ok_or_else(|| "document revision counter is exhausted".to_owned())?;
    *document = candidate;
    Ok(report)
}

/// Returns world-space wire edges for a transform preview without changing document or selection.
pub fn preview_selection_transform(
    document: &Document,
    selection: &SelectionSession,
    world_transform: Affine3,
) -> Result<Vec<[Vec3; 2]>, String> {
    if !world_transform.is_finite()
        || world_transform
            .inverse(GeometryTolerance::default())
            .is_none()
    {
        return Err("preview transform must be finite and invertible".to_owned());
    }
    let selected = selection
        .selected()
        .iter()
        .filter(|target| target_in_context(document, &selection.context, target))
        .collect::<Vec<_>>();
    let object_occurrences = selected
        .iter()
        .filter_map(|target| {
            matches!(target.entity, SelectedEntity::Occurrence).then_some(target.occurrence.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut edges = Vec::new();
    for object in &object_occurrences {
        for occurrence in document
            .occurrence_refs()?
            .into_iter()
            .filter(|candidate| candidate.path.starts_with(&object.path))
        {
            let Some(resolved) = document.resolve_occurrence(&occurrence) else {
                continue;
            };
            if let Some(geometry) = document.effective_geometry(&occurrence) {
                let vertex_ids = geometry.vertices.keys().copied().collect::<BTreeSet<_>>();
                let Some(inverse) = resolved
                    .world_transform
                    .inverse(GeometryTolerance::default())
                else {
                    continue;
                };
                append_preview_geometry(
                    geometry,
                    &vertex_ids,
                    resolved.world_transform,
                    inverse
                        .compose(world_transform)
                        .compose(resolved.world_transform),
                    &mut edges,
                );
            }
        }
    }

    let mut selected_by_occurrence = BTreeMap::<OccurrenceRef, BTreeSet<VertexId>>::new();
    for target in selected {
        if matches!(target.entity, SelectedEntity::Occurrence)
            || object_occurrences
                .iter()
                .any(|object| target.occurrence.path.starts_with(&object.path))
        {
            continue;
        }
        let Some(geometry) = document.effective_geometry(&target.occurrence) else {
            continue;
        };
        let vertices = selected_by_occurrence
            .entry(target.occurrence.clone())
            .or_default();
        match target.entity {
            SelectedEntity::Vertex(id) => {
                if geometry.vertices.contains_key(&id) {
                    vertices.insert(id);
                }
            }
            SelectedEntity::Segment(id) => {
                if let Some(segment) = geometry.segments.get(&id) {
                    vertices.extend(segment.endpoints.map(|reference| reference.id));
                }
            }
            SelectedEntity::Triangle(id) => {
                if let Some(triangle) = geometry.triangles.get(&id) {
                    vertices.extend(triangle.vertices.map(|reference| reference.id));
                }
            }
            SelectedEntity::Occurrence | SelectedEntity::SemanticHandle { .. } => {}
        }
    }
    for (occurrence, vertices) in selected_by_occurrence {
        let Some(geometry) = document.effective_geometry(&occurrence) else {
            continue;
        };
        let Some(resolved) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let inverse = resolved
            .world_transform
            .inverse(GeometryTolerance::default())
            .ok_or_else(|| "selected geometry has a singular world transform".to_owned())?;
        let local_transform = inverse
            .compose(world_transform)
            .compose(resolved.world_transform);
        append_preview_geometry(
            geometry,
            &vertices,
            resolved.world_transform,
            local_transform,
            &mut edges,
        );
    }
    Ok(edges)
}

fn append_preview_geometry(
    geometry: &GeometryBundle,
    transformed_vertices: &BTreeSet<VertexId>,
    world: Affine3,
    local_transform: Affine3,
    output: &mut Vec<[Vec3; 2]>,
) {
    let point = |reference: VertexRef| {
        let position = geometry.vertex_position(reference)?;
        let position = if transformed_vertices.contains(&reference.id) {
            local_transform.transform_point(position)
        } else {
            position
        };
        Some(world.transform_point(position))
    };
    for segment in geometry.segments.values() {
        if !segment
            .endpoints
            .iter()
            .any(|reference| transformed_vertices.contains(&reference.id))
        {
            continue;
        }
        if let [Some(start), Some(end)] = segment.endpoints.map(point) {
            output.push([start, end]);
        }
    }
    for triangle in geometry.triangles.values() {
        if !triangle
            .vertices
            .iter()
            .any(|reference| transformed_vertices.contains(&reference.id))
        {
            continue;
        }
        let [Some(a), Some(b), Some(c)] = triangle.vertices.map(point) else {
            continue;
        };
        output.extend([[a, b], [b, c], [c, a]]);
    }
}

#[derive(Clone, Debug, Default)]
struct GeometryTransformResult {
    transformed_vertices: usize,
    copied_vertices: usize,
    copied_segments: usize,
    copied_triangles: usize,
    targets: Vec<SelectedEntity>,
}

fn transform_geometry_bundle(
    document: &mut Document,
    geometry: &mut GeometryBundle,
    selected: &GeometrySelection,
    transform: Affine3,
    copy: bool,
) -> Result<GeometryTransformResult, String> {
    let mut vertex_ids = selected.vertices.clone();
    for id in &selected.segments {
        let segment = geometry
            .segments
            .get(id)
            .ok_or_else(|| format!("selected segment {} is missing", id.0))?;
        vertex_ids.extend(segment.endpoints.map(|reference| reference.id));
    }
    for id in &selected.triangles {
        let triangle = geometry
            .triangles
            .get(id)
            .ok_or_else(|| format!("selected triangle {} is missing", id.0))?;
        vertex_ids.extend(triangle.vertices.map(|reference| reference.id));
    }
    if vertex_ids.is_empty() {
        return Ok(GeometryTransformResult::default());
    }
    if vertex_ids
        .iter()
        .any(|id| !geometry.vertices.contains_key(id))
    {
        return Err("selection references a missing geometry vertex".to_owned());
    }

    if copy {
        let mut vertex_map = BTreeMap::new();
        for id in &vertex_ids {
            let source = geometry
                .vertices
                .get(id)
                .ok_or_else(|| format!("selected vertex {} is missing", id.0))?;
            let new_id = VertexId(document.allocate_id());
            let mut vertex = source.clone();
            vertex.id = new_id;
            vertex.position = transform.transform_point(vertex.position);
            vertex.legacy_aliases.clear();
            geometry.vertices.insert(new_id, vertex);
            vertex_map.insert(*id, new_id);
        }
        let mut result = GeometryTransformResult {
            copied_vertices: vertex_map.len(),
            ..GeometryTransformResult::default()
        };
        result.targets.extend(
            selected
                .explicit_vertices
                .iter()
                .filter_map(|id| vertex_map.get(id).copied())
                .map(SelectedEntity::Vertex),
        );
        for id in &selected.segments {
            let source = geometry
                .segments
                .get(id)
                .ok_or_else(|| format!("selected segment {} is missing", id.0))?
                .clone();
            let new_id = SegmentId(document.allocate_id());
            let copy = Segment {
                id: new_id,
                endpoints: source.endpoints.map(|reference| VertexRef {
                    id: vertex_map[&reference.id],
                    position_offset: transform.transform_vector(reference.position_offset),
                }),
                legacy_aliases: Vec::new(),
                ..source
            };
            geometry.segments.insert(new_id, copy);
            result.copied_segments += 1;
            result.targets.push(SelectedEntity::Segment(new_id));
        }
        for id in &selected.triangles {
            let source = geometry
                .triangles
                .get(id)
                .ok_or_else(|| format!("selected triangle {} is missing", id.0))?
                .clone();
            let new_id = TriangleId(document.allocate_id());
            let copy = Triangle {
                id: new_id,
                vertices: source.vertices.map(|reference| VertexRef {
                    id: vertex_map[&reference.id],
                    position_offset: transform.transform_vector(reference.position_offset),
                }),
                legacy_aliases: Vec::new(),
                ..source
            };
            geometry.triangles.insert(new_id, copy);
            result.copied_triangles += 1;
            result.targets.push(SelectedEntity::Triangle(new_id));
        }
        return Ok(result);
    }

    let mut changed_vertices = BTreeSet::new();
    for id in &vertex_ids {
        let vertex = geometry
            .vertices
            .get_mut(id)
            .ok_or_else(|| format!("selected vertex {} is missing", id.0))?;
        let transformed = transform.transform_point(vertex.position);
        if transformed != vertex.position {
            vertex.position = transformed;
            changed_vertices.insert(*id);
        }
    }
    for segment in geometry.segments.values_mut() {
        for reference in &mut segment.endpoints {
            if vertex_ids.contains(&reference.id) {
                reference.position_offset = transform.transform_vector(reference.position_offset);
            }
        }
    }
    for triangle in geometry.triangles.values_mut() {
        for reference in &mut triangle.vertices {
            if vertex_ids.contains(&reference.id) {
                reference.position_offset = transform.transform_vector(reference.position_offset);
            }
        }
    }
    Ok(GeometryTransformResult {
        transformed_vertices: changed_vertices.len(),
        ..GeometryTransformResult::default()
    })
}

fn transform_occurrence(
    document: &mut Document,
    occurrence: &OccurrenceRef,
    world_transform: Affine3,
) -> Result<bool, String> {
    let (placement, mut edits) = {
        let Some(resolved) = document.resolve_occurrence(occurrence) else {
            return Err("selected object instance is missing".to_owned());
        };
        let Some(entry) = resolved.entry else {
            return Err("the scene root cannot be transformed as an object instance".to_owned());
        };
        (
            resolved
                .edits
                .and_then(|edits| edits.transform)
                .unwrap_or(entry.transform),
            resolved.edits.cloned().unwrap_or_default(),
        )
    };
    let parent_world = if occurrence.path.len() == 1 {
        Affine3::IDENTITY
    } else {
        let parent = OccurrenceRef {
            path: occurrence.path[..occurrence.path.len() - 1].to_vec(),
        };
        document
            .resolve_occurrence(&parent)
            .map(|resolved| resolved.world_transform)
            .ok_or_else(|| "selected object's parent instance is missing".to_owned())?
    };
    let parent_inverse = parent_world
        .inverse(GeometryTolerance::default())
        .ok_or_else(|| "selected object's parent transform is singular".to_owned())?;
    let adjusted = parent_inverse
        .compose(world_transform)
        .compose(parent_world)
        .compose(placement);
    if edits.transform == Some(adjusted) {
        return Ok(false);
    }
    edits.transform = Some(adjusted);
    if !document.set_occurrence_edits(occurrence.clone(), edits) {
        return Err("could not store the transformed object instance".to_owned());
    }
    Ok(true)
}

fn target_in_context(document: &Document, context: &EditContext, target: &SelectionTarget) -> bool {
    match context {
        EditContext::Document => true,
        EditContext::Definition(definition) => document
            .resolve_occurrence(&target.occurrence)
            .is_some_and(|resolved| resolved.definition.id == *definition),
        EditContext::Occurrence(occurrence) => target.occurrence.path.starts_with(&occurrence.path),
    }
}

fn geometry_owner(
    context: &EditContext,
    target: &SelectionTarget,
    definition: DefinitionId,
) -> GeometryOwner {
    match context {
        EditContext::Definition(id) if *id == definition => GeometryOwner::Definition(*id),
        EditContext::Occurrence(occurrence) if *occurrence == target.occurrence => {
            GeometryOwner::Definition(definition)
        }
        _ if target.occurrence.is_scene_root() => GeometryOwner::Definition(definition),
        _ => GeometryOwner::Occurrence(target.occurrence.clone()),
    }
}

fn is_identity(transform: Affine3) -> bool {
    transform == Affine3::IDENTITY
}

// Kept as a small helper so ownership is explicit at the call site when looking up geometry.
#[cfg(test)]
mod tests {
    use super::{paint_selection, transform_selection};
    use crate::document::{Document, OccurrenceRef, Rgba8};
    use crate::geometry::{Affine3, Vec3};
    use crate::selection::{
        EditContext, SelectedEntity, SelectionKind, SelectionSession, SelectionTarget,
    };

    fn selected(occurrence: OccurrenceRef, entity: SelectedEntity) -> SelectionTarget {
        SelectionTarget { occurrence, entity }
    }

    fn affine_translation(delta: Vec3) -> Affine3 {
        Affine3::from_translation(delta)
    }

    fn add_annotations(
        document: &mut Document,
        occurrence: &OccurrenceRef,
    ) -> (crate::document::DefinitionId, String, String) {
        let definition_id = document
            .resolve_occurrence(occurrence)
            .unwrap()
            .definition
            .id;
        let definition = document.definitions.get_mut(&definition_id).unwrap();
        definition.extensions.insert(
            "k3d.dimensions".to_owned(),
            serde_json::json!([{
                "start": {"x": 0.0, "y": 0.0, "z": 0.0},
                "end": {"x": 1.0, "y": 0.0, "z": 0.0},
                "offset": {"x": 0.0, "y": 1.0, "z": 0.0},
                "label": "length"
            }]),
        );
        definition.extensions.insert(
            "k3d.texts".to_owned(),
            serde_json::json!([{
                "position": {"x": 2.0, "y": 0.0, "z": 1.0},
                "text": "Door",
                "size": 0.5,
                "normal": {"x": 0.0, "y": 1.0, "z": 0.0},
                "axisU": {"x": 1.0, "y": 0.0, "z": 0.0}
            }]),
        );
        definition
            .domain_entity_ids
            .insert("dimensions".to_owned(), vec!["dimension-1".to_owned()]);
        definition
            .domain_entity_ids
            .insert("texts".to_owned(), vec!["text-1".to_owned()]);
        (definition_id, "dimension-1".to_owned(), "text-1".to_owned())
    }

    #[test]
    fn moving_one_instance_face_changes_shared_vertices_once_and_keeps_siblings() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .copy_root_occurrence(&first, Vec3::new(5.0, 0.0, 0.0))
            .unwrap();
        let geometry = document.effective_geometry(&first).unwrap();
        let triangle = *geometry.triangles.keys().next().unwrap();
        let referenced = geometry.triangles[&triangle].vertices[0].id;
        let before = geometry.vertices[&referenced].position;
        let sibling_before =
            document.effective_geometry(&second).unwrap().vertices[&referenced].position;
        let mut selection = SelectionSession::default();
        selection.select(selected(first.clone(), SelectedEntity::Triangle(triangle)));

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(0.0, 3.0, 0.0)),
            false,
        )
        .unwrap();

        assert_eq!(3, report.transformed_vertices);
        let moved = document.effective_geometry(&first).unwrap();
        assert_eq!(
            before + Vec3::new(0.0, 3.0, 0.0),
            moved.vertices[&referenced].position
        );
        assert_eq!(
            sibling_before,
            document.effective_geometry(&second).unwrap().vertices[&referenced].position
        );
        assert_eq!(
            8,
            document
                .resolve_occurrence(&first)
                .unwrap()
                .definition
                .geometry
                .vertices
                .len()
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn copy_mixed_edge_and_face_preserves_payload_and_selects_one_shared_vertex_copy() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let resolved = document.resolve_occurrence(&occurrence).unwrap();
        let geometry = &resolved.definition.geometry;
        let segment = *geometry.segments.keys().next().unwrap();
        let triangle = *geometry.triangles.keys().next().unwrap();
        let source_vertices = geometry.segments[&segment]
            .endpoints
            .map(|reference| reference.id);
        let source_color = geometry.triangles[&triangle].color;
        let mut selection = SelectionSession::default();
        selection.apply(
            [
                selected(occurrence.clone(), SelectedEntity::Segment(segment)),
                selected(occurrence.clone(), SelectedEntity::Triangle(triangle)),
            ],
            crate::selection::SelectionMode::Replace,
        );

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(4.0, 0.0, 0.0)),
            true,
        )
        .unwrap();

        assert_eq!(3, report.copied_vertices);
        assert_eq!(1, report.copied_segments);
        assert_eq!(1, report.copied_triangles);
        assert_eq!(2, selection.selected().len());
        let copied = document.effective_geometry(&occurrence).unwrap();
        let copied_segment = selection
            .selected()
            .iter()
            .find_map(|target| match target.entity {
                SelectedEntity::Segment(id) => Some(&copied.segments[&id]),
                _ => None,
            })
            .unwrap();
        let copied_triangle = selection
            .selected()
            .iter()
            .find_map(|target| match target.entity {
                SelectedEntity::Triangle(id) => Some(&copied.triangles[&id]),
                _ => None,
            })
            .unwrap();
        assert_eq!(source_color, copied_triangle.color);
        assert!(copied_segment.endpoints.iter().all(|reference| {
            copied_triangle
                .vertices
                .iter()
                .any(|face_vertex| face_vertex.id == reference.id)
                || !source_vertices.contains(&reference.id)
        }));
        assert!(
            copied_segment
                .endpoints
                .iter()
                .all(|reference| !source_vertices.contains(&reference.id))
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn object_move_copy_updates_the_selected_instance_only() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .copy_root_occurrence(&first, Vec3::new(6.0, 0.0, 0.0))
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(selected(first.clone(), SelectedEntity::Occurrence));
        let second_before = document
            .resolve_occurrence(&second)
            .unwrap()
            .world_transform;

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(0.0, 0.0, 2.0)),
            true,
        )
        .unwrap();

        assert_eq!(1, report.copied_occurrences);
        assert_eq!(1, selection.selected().len());
        assert_eq!(
            second_before,
            document
                .resolve_occurrence(&second)
                .unwrap()
                .world_transform
        );
        let copied = &selection.selected().first().unwrap().occurrence;
        assert_eq!(
            Vec3::new(0.0, 0.0, 2.0),
            document
                .resolve_occurrence(copied)
                .unwrap()
                .world_transform
                .translation
        );
    }

    #[test]
    fn nested_object_copies_remap_siblings_and_remain_selected_in_parent_context() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("first child", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .add_cuboid("second child", Vec3::new(1.0, 1.0, 1.0), Vec3::X * 3.0)
            .unwrap();
        let parent = crate::operations::objects::group_occurrences(
            &mut document,
            &[first, second],
            "parent",
        )
        .unwrap();
        let child_layout = document
            .resolve_occurrence(&parent)
            .unwrap()
            .definition
            .default_child_layout
            .unwrap();
        let children = document.child_layout(child_layout).unwrap().entries.clone();
        let first_child = parent.child(children[0].id);
        let second_child = parent.child(children[1].id);
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Occurrence(parent.clone()));
        selection.apply(
            [
                selected(first_child, SelectedEntity::Occurrence),
                selected(second_child, SelectedEntity::Occurrence),
            ],
            crate::selection::SelectionMode::Replace,
        );

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(0.0, 2.0, 0.0)),
            true,
        )
        .unwrap();

        assert_eq!(2, report.copied_occurrences);
        assert_eq!(2, selection.selected().len());
        assert!(selection.selected().iter().all(|target| {
            target.occurrence.path.starts_with(&parent.path)
                && document.resolve_occurrence(&target.occurrence).is_some()
        }));
        let resolved_parent = document.resolve_occurrence(&parent).unwrap();
        let private_layout = resolved_parent.edits.unwrap().child_layout.unwrap();
        assert_eq!(
            4,
            document.child_layout(private_layout).unwrap().entries.len()
        );
        assert!(!selection.retain_valid(&document));
        assert_eq!(2, selection.selected().len());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn paint_applies_rgba_only_to_selected_face_and_preserves_sibling_instance() {
        let mut document = Document::new();
        let first = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let second = document
            .copy_root_occurrence(&first, Vec3::new(5.0, 0.0, 0.0))
            .unwrap();
        let triangle = *document
            .effective_geometry(&first)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(selected(first.clone(), SelectedEntity::Triangle(triangle)));
        let color = Rgba8::new(18, 80, 220, 96);

        let report = paint_selection(&mut document, &selection, color).unwrap();

        assert_eq!(1, report.painted_triangles);
        assert_eq!(
            color,
            document.effective_geometry(&first).unwrap().triangles[&triangle].color
        );
        assert_eq!(
            Rgba8::WHITE,
            document.effective_geometry(&second).unwrap().triangles[&triangle].color
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn definition_edit_context_paints_shared_prototype_geometry() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let definition = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let triangle = *document
            .effective_geometry(&occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Definition(definition));
        selection.select(selected(
            occurrence.clone(),
            SelectedEntity::Triangle(triangle),
        ));
        let color = Rgba8::new(220, 40, 80, 255);

        paint_selection(&mut document, &selection, color).unwrap();

        assert_eq!(
            color,
            document.effective_geometry(&occurrence).unwrap().triangles[&triangle].color
        );
        assert!(document.occurrence_edits(&occurrence).is_none());
        assert!(document.validate().is_ok());
    }

    #[test]
    fn transform_rejects_semantic_kinds_without_domain_adapters_atomically() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("box", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let revision = document.revision();
        let mut selection = SelectionSession::default();
        selection.select(selected(
            occurrence,
            SelectedEntity::SemanticHandle {
                kind: SelectionKind::Wall,
                id: "wall-endpoint".to_owned(),
            },
        ));

        let error = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::X),
            false,
        )
        .unwrap_err();

        assert!(error.contains("semantic selection kind Wall"));
        assert_eq!(revision, document.revision());
    }

    #[test]
    fn move_transforms_selected_dimensions_and_text_in_definition_context() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("annotated", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let (definition_id, dimension_id, text_id) = add_annotations(&mut document, &occurrence);
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Definition(definition_id));
        selection.apply(
            [
                selected(
                    occurrence.clone(),
                    SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Dimension,
                        id: dimension_id,
                    },
                ),
                selected(
                    occurrence.clone(),
                    SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Text,
                        id: text_id,
                    },
                ),
            ],
            crate::selection::SelectionMode::Replace,
        );

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(2.0, 3.0, 4.0)),
            false,
        )
        .unwrap();

        assert_eq!(2, report.transformed_domain_entities);
        let definition = document.definition(definition_id).unwrap();
        let dimension = &definition.extensions["k3d.dimensions"][0];
        let text = &definition.extensions["k3d.texts"][0];
        let start: Vec3 = serde_json::from_value(dimension["start"].clone()).unwrap();
        let position: Vec3 = serde_json::from_value(text["position"].clone()).unwrap();
        assert_eq!(Vec3::new(2.0, 3.0, 4.0), start);
        assert_eq!(Vec3::new(4.0, 3.0, 5.0), position);
        assert_eq!("length", dimension["label"]);
        assert_eq!("Door", text["text"]);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn copy_duplicates_only_selected_annotation_and_selects_the_new_handle() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("annotated", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let (definition_id, _, text_id) = add_annotations(&mut document, &occurrence);
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Definition(definition_id));
        selection.select(selected(
            occurrence.clone(),
            SelectedEntity::SemanticHandle {
                kind: SelectionKind::Text,
                id: text_id.clone(),
            },
        ));

        let report = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::new(4.0, 0.0, 0.0)),
            true,
        )
        .unwrap();

        assert_eq!(1, report.copied_domain_entities);
        assert_eq!(1, selection.selected().len());
        let SelectedEntity::SemanticHandle {
            kind: SelectionKind::Text,
            id: copied_id,
        } = &selection.selected().first().unwrap().entity
        else {
            panic!("copy should select its new text handle");
        };
        assert_ne!(text_id.as_str(), copied_id.as_str());
        let definition = document.definition(definition_id).unwrap();
        assert_eq!(
            2,
            definition.extensions["k3d.texts"].as_array().unwrap().len()
        );
        assert_eq!(2, definition.domain_entity_ids["texts"].len());
        let original_position: Vec3 =
            serde_json::from_value(definition.extensions["k3d.texts"][0]["position"].clone())
                .unwrap();
        let copied_position: Vec3 =
            serde_json::from_value(definition.extensions["k3d.texts"][1]["position"].clone())
                .unwrap();
        assert_eq!(Vec3::new(2.0, 0.0, 1.0), original_position);
        assert_eq!(Vec3::new(6.0, 0.0, 1.0), copied_position);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn inherited_instance_annotation_transform_is_rejected_without_mutation() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("annotated", Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO)
            .unwrap();
        let (_, dimension_id, _) = add_annotations(&mut document, &occurrence);
        let before = document.clone();
        let mut selection = SelectionSession::default();
        selection.select(selected(
            occurrence,
            SelectedEntity::SemanticHandle {
                kind: SelectionKind::Dimension,
                id: dimension_id,
            },
        ));

        let error = transform_selection(
            &mut document,
            &mut selection,
            affine_translation(Vec3::X),
            false,
        )
        .unwrap_err();

        assert!(error.contains("occurrence-level domain overrides"));
        assert_eq!(before, document);
        assert!(document.validate().is_ok());
    }
}
