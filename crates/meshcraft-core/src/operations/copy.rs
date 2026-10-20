//! Selection-first copy patterns built on the shared transform operation.

use crate::document::Document;
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use crate::operations::transforms::{self, TransformReport};
use crate::selection::{
    EditContext, SelectedEntity, SelectionKind, SelectionMode, SelectionSession, SelectionTarget,
};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_LINEAR_ARRAY_COPIES: usize = 10_000;
pub const MAX_MULTIPLE_ARRAY_COPIES: usize = 2_048;
pub const MAX_MULTIPLE_ARRAY_PREVIEW_COPIES: usize = 10_000;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LinearArrayReport {
    pub copies: usize,
    pub copied_vertices: usize,
    pub copied_segments: usize,
    pub copied_triangles: usize,
    pub copied_occurrences: usize,
    pub copied_domain_entities: usize,
    pub selected_results: Vec<SelectionTarget>,
}

impl LinearArrayReport {
    fn include_transform(&mut self, report: TransformReport) {
        self.copied_vertices += report.copied_vertices;
        self.copied_segments += report.copied_segments;
        self.copied_triangles += report.copied_triangles;
        self.copied_occurrences += report.copied_occurrences;
        self.copied_domain_entities += report.copied_domain_entities;
    }
}

/// Returns whether the current selection contains object, mesh, dimension, or text entities
/// supported by the shared copy transform.
pub fn can_linear_array_selection(document: &Document, selection: &SelectionSession) -> bool {
    !selection.selected().is_empty()
        && selection.selected().iter().all(|target| {
            if !target_in_selection_context(document, selection, target) {
                return false;
            }
            match target.entity {
                SelectedEntity::Occurrence => {
                    document.resolve_occurrence(&target.occurrence).is_some()
                }
                SelectedEntity::Vertex(id) => document
                    .effective_geometry(&target.occurrence)
                    .is_some_and(|geometry| geometry.vertices.contains_key(&id)),
                SelectedEntity::Segment(id) => document
                    .effective_geometry(&target.occurrence)
                    .is_some_and(|geometry| geometry.segments.contains_key(&id)),
                SelectedEntity::Triangle(id) => document
                    .effective_geometry(&target.occurrence)
                    .is_some_and(|geometry| geometry.triangles.contains_key(&id)),
                SelectedEntity::SemanticHandle { kind, .. } => {
                    selected_annotation_record(document, selection, target, kind).is_some()
                }
            }
        })
}

fn target_in_selection_context(
    document: &Document,
    selection: &SelectionSession,
    target: &SelectionTarget,
) -> bool {
    match &selection.context {
        EditContext::Document => true,
        EditContext::Definition(definition) => document
            .resolve_occurrence(&target.occurrence)
            .is_some_and(|resolved| resolved.definition.id == *definition),
        EditContext::Occurrence(occurrence) => target.occurrence.path.starts_with(&occurrence.path),
    }
}

fn selected_annotation_record<'a>(
    document: &'a Document,
    selection: &SelectionSession,
    target: &SelectionTarget,
    kind: SelectionKind,
) -> Option<(&'static str, &'a Value)> {
    let field = match kind {
        SelectionKind::Dimension => "dimensions",
        SelectionKind::Text => "texts",
        _ => return None,
    };
    let SelectedEntity::SemanticHandle { id, .. } = &target.entity else {
        return None;
    };
    let resolved = document.resolve_occurrence(&target.occurrence)?;
    let definition_context = match &selection.context {
        EditContext::Document => target.occurrence.is_scene_root(),
        EditContext::Definition(definition) => resolved.definition.id == *definition,
        EditContext::Occurrence(occurrence) => occurrence == &target.occurrence,
    };
    if !definition_context {
        return None;
    }
    let definition = resolved.definition;
    let records = definition
        .extensions
        .get(&format!("k3d.{field}"))?
        .as_array()?;
    let identities = definition.domain_entity_ids.get(field)?;
    if records.len() != identities.len() {
        return None;
    }
    let mut matches = identities
        .iter()
        .enumerate()
        .filter(|(_, identity)| *identity == id);
    let (index, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let record = &records[index];
    crate::operations::objects::transform_annotation_record(field, record, Affine3::IDENTITY)
        .ok()?;
    Some((field, record))
}

fn selected_annotation_points(record: &Value, field: &str) -> Option<Vec<Vec3>> {
    let read = |key: &str| -> Option<Vec3> {
        let value = record.get(key)?.clone();
        let point: Vec3 = serde_json::from_value(value).ok()?;
        point.is_finite().then_some(point)
    };
    match field {
        "dimensions" => {
            let points = ["start", "end", "offset"]
                .into_iter()
                .filter_map(read)
                .collect::<Vec<_>>();
            (!points.is_empty()).then_some(points)
        }
        "texts" => {
            let position = read("position")?;
            let size = record
                .get("size")
                .and_then(Value::as_f64)
                .filter(|size| size.is_finite() && *size > 0.0)
                .unwrap_or(0.1);
            let text = record
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let width = size * text.chars().count().max(1) as f64 * 0.6;
            if !width.is_finite() {
                return None;
            }
            let (Some(axis_u), Some(normal)) = (read("axisU"), read("normal")) else {
                return Some(vec![position]);
            };
            let Some(axis_u) = axis_u.try_normalized(GeometryTolerance::default().absolute) else {
                return Some(vec![position]);
            };
            let Some(axis_w) = axis_u
                .cross(normal)
                .try_normalized(GeometryTolerance::default().absolute)
            else {
                return Some(vec![position]);
            };
            let width_axis = axis_u * width;
            let height_axis = axis_w * size;
            let corners = [
                position,
                position + width_axis,
                position + width_axis + height_axis,
                position + height_axis,
            ];
            corners
                .iter()
                .all(|point| point.is_finite())
                .then_some(corners.to_vec())
        }
        _ => None,
    }
}

/// Measures the selected payload's projected world-space span along a direction.
/// Root occurrences include realized descendant geometry; mesh selections include only their
/// referenced vertices. Returns `None` for unsupported or invalid selections.
pub fn projected_selection_span(
    document: &Document,
    selection: &SelectionSession,
    direction: Vec3,
) -> Option<f64> {
    if !can_linear_array_selection(document, selection) {
        return None;
    }
    let direction = direction.try_normalized(GeometryTolerance::default().absolute)?;
    let occurrences = selection
        .selected()
        .iter()
        .any(|target| target.entity == SelectedEntity::Occurrence)
        .then(|| document.occurrence_refs().ok())
        .flatten();
    let mut minimum = f64::INFINITY;
    let mut maximum = f64::NEG_INFINITY;

    for target in selection.selected() {
        if target.entity == SelectedEntity::Occurrence {
            for occurrence in occurrences
                .as_ref()?
                .iter()
                .filter(|occurrence| occurrence.path.starts_with(&target.occurrence.path))
            {
                let resolved = document.resolve_occurrence(occurrence)?;
                let geometry = document.effective_geometry(occurrence)?;
                include_geometry_points(
                    geometry,
                    resolved.world_transform,
                    direction,
                    &mut minimum,
                    &mut maximum,
                );
            }
        } else if let SelectedEntity::SemanticHandle { kind, .. } = target.entity {
            let (field, record) = selected_annotation_record(document, selection, target, kind)?;
            let resolved = document.resolve_occurrence(&target.occurrence)?;
            for point in selected_annotation_points(record, field)? {
                include_projected_point(
                    resolved.world_transform.transform_point(point),
                    direction,
                    &mut minimum,
                    &mut maximum,
                );
            }
        } else {
            let resolved = document.resolve_occurrence(&target.occurrence)?;
            let geometry = document.effective_geometry(&target.occurrence)?;
            let points = selected_local_points(geometry, &target.entity)?;
            for point in points {
                include_projected_point(
                    resolved.world_transform.transform_point(point),
                    direction,
                    &mut minimum,
                    &mut maximum,
                );
            }
        }
    }

    (minimum.is_finite() && maximum.is_finite()).then_some(maximum - minimum)
}

fn include_geometry_points(
    geometry: &crate::document::GeometryBundle,
    transform: Affine3,
    direction: Vec3,
    minimum: &mut f64,
    maximum: &mut f64,
) {
    for vertex in geometry.vertices.values() {
        include_projected_point(
            transform.transform_point(vertex.position),
            direction,
            minimum,
            maximum,
        );
    }
    for segment in geometry.segments.values() {
        for endpoint in segment.endpoints {
            if let Some(point) = geometry.vertex_position(endpoint) {
                include_projected_point(
                    transform.transform_point(point),
                    direction,
                    minimum,
                    maximum,
                );
            }
        }
    }
    for triangle in geometry.triangles.values() {
        for vertex in triangle.vertices {
            if let Some(point) = geometry.vertex_position(vertex) {
                include_projected_point(
                    transform.transform_point(point),
                    direction,
                    minimum,
                    maximum,
                );
            }
        }
    }
}

fn selected_local_points(
    geometry: &crate::document::GeometryBundle,
    entity: &SelectedEntity,
) -> Option<Vec<Vec3>> {
    match entity {
        SelectedEntity::Vertex(id) => Some(vec![geometry.vertices.get(id)?.position]),
        SelectedEntity::Segment(id) => geometry
            .segments
            .get(id)?
            .endpoints
            .iter()
            .map(|endpoint| geometry.vertex_position(*endpoint))
            .collect(),
        SelectedEntity::Triangle(id) => geometry
            .triangles
            .get(id)?
            .vertices
            .iter()
            .map(|vertex| geometry.vertex_position(*vertex))
            .collect(),
        SelectedEntity::Occurrence | SelectedEntity::SemanticHandle { .. } => None,
    }
}

fn include_projected_point(point: Vec3, direction: Vec3, minimum: &mut f64, maximum: &mut f64) {
    let projection = point.dot(direction);
    *minimum = minimum.min(projection);
    *maximum = maximum.max(projection);
}

/// Copies the selected objects and mesh entities at each positive multiple of `step`.
/// Every copy is derived from the original selection, so the operation never accumulates offsets.
/// The source remains unchanged and all generated entities become the resulting selection.
pub fn linear_array(
    document: &mut Document,
    selection: &mut SelectionSession,
    copies: usize,
    step: Vec3,
) -> Result<LinearArrayReport, String> {
    if copies == 0 || copies > MAX_LINEAR_ARRAY_COPIES {
        return Err(format!(
            "linear array copy count must be between 1 and {MAX_LINEAR_ARRAY_COPIES}"
        ));
    }
    let zero_step = GeometryTolerance::default().absolute;
    if !step.is_finite() || step.length_squared() <= zero_step * zero_step {
        return Err("linear array step must be finite and nonzero".to_owned());
    }
    let offsets = (1..=copies)
        .map(|index| step * index as f64)
        .collect::<Vec<_>>();
    translated_array(document, selection, &offsets)
}

/// Copies the supported selection once at each supplied world-space translation.
/// The source selection is never transformed, and generated results replace it atomically.
pub fn translated_array(
    document: &mut Document,
    selection: &mut SelectionSession,
    offsets: &[Vec3],
) -> Result<LinearArrayReport, String> {
    if offsets.is_empty() || offsets.len() > MAX_LINEAR_ARRAY_COPIES {
        return Err(format!(
            "array copy count must be between 1 and {MAX_LINEAR_ARRAY_COPIES}"
        ));
    }
    if offsets
        .iter()
        .any(|offset| !offset.is_finite() || offset.length_squared() <= f64::EPSILON.powi(2))
    {
        return Err("array offsets must be finite and nonzero".to_owned());
    }
    let transforms = offsets
        .iter()
        .copied()
        .map(Affine3::from_translation)
        .collect::<Vec<_>>();
    transformed_array(document, selection, &transforms)
}

/// Copies the supported selection once through each supplied world-space transform.
/// Each result derives from the source selection, and publication is atomic.
pub fn transformed_array(
    document: &mut Document,
    selection: &mut SelectionSession,
    affines: &[Affine3],
) -> Result<LinearArrayReport, String> {
    if affines.is_empty() || affines.len() > MAX_LINEAR_ARRAY_COPIES {
        return Err(format!(
            "array copy count must be between 1 and {MAX_LINEAR_ARRAY_COPIES}"
        ));
    }
    if affines.iter().any(|transform| {
        !transform.is_finite() || transform.inverse(GeometryTolerance::default()).is_none()
    }) {
        return Err("array transforms must be finite and invertible".to_owned());
    }
    if !can_linear_array_selection(document, selection) {
        return Err(
            "select supported objects, mesh entities, dimensions, or text to array".to_owned(),
        );
    }

    let mut candidate = document.clone();
    let mut original_selection = selection.clone();
    let mut resulting_selection = original_selection.clone();
    resulting_selection.clear();
    let mut result_targets = BTreeSet::<SelectionTarget>::new();
    let mut report = LinearArrayReport {
        copies: affines.len(),
        ..LinearArrayReport::default()
    };

    for transform in affines {
        let mut copy_selection = original_selection.clone();
        let (transform_report, path_remaps) = transforms::transform_selection_with_path_remaps(
            &mut candidate,
            &mut copy_selection,
            *transform,
            true,
        )?;
        if !transform_report.changed() || copy_selection.selected().is_empty() {
            return Err("linear array could not create a copy of the selection".to_owned());
        }
        report.include_transform(transform_report);
        for remap in path_remaps {
            let remapped_source = original_selection
                .selected()
                .iter()
                .cloned()
                .map(|mut target| {
                    target.occurrence = remap
                        .remap_path(&target.occurrence)
                        .unwrap_or(target.occurrence);
                    target
                })
                .collect::<Vec<_>>();
            original_selection.apply(remapped_source, SelectionMode::Replace);
            result_targets = result_targets
                .into_iter()
                .map(|mut target| {
                    target.occurrence = remap
                        .remap_path(&target.occurrence)
                        .unwrap_or(target.occurrence);
                    target
                })
                .collect();
        }
        result_targets.extend(copy_selection.selected().iter().cloned());
    }

    if result_targets.is_empty() {
        return Err("linear array produced no selected results".to_owned());
    }
    candidate.validate()?;
    resulting_selection.apply(result_targets, SelectionMode::Replace);
    report.selected_results = resulting_selection.selected().iter().cloned().collect();
    *document = candidate;
    *selection = resulting_selection;
    Ok(report)
}

/// Builds the legacy XZ translation grid from origin, one measured step, and a span point.
pub fn planar_translation_offsets(origin: Vec3, measure: Vec3, extent: Vec3) -> (Vec<Vec3>, bool) {
    planar_translation_offsets_limited(origin, measure, extent, MAX_MULTIPLE_ARRAY_COPIES)
}

pub fn planar_translation_offsets_limited(
    origin: Vec3,
    measure: Vec3,
    extent: Vec3,
    limit: usize,
) -> (Vec<Vec3>, bool) {
    grid_translation_offsets(origin, measure, extent, [true, false, true], limit)
}

/// Builds the legacy XYZ translation grid from origin, one measured step, and a span point.
pub fn volumetric_translation_offsets(
    origin: Vec3,
    measure: Vec3,
    extent: Vec3,
) -> (Vec<Vec3>, bool) {
    volumetric_translation_offsets_limited(origin, measure, extent, MAX_MULTIPLE_ARRAY_COPIES)
}

pub fn volumetric_translation_offsets_limited(
    origin: Vec3,
    measure: Vec3,
    extent: Vec3,
    limit: usize,
) -> (Vec<Vec3>, bool) {
    grid_translation_offsets(origin, measure, extent, [true, true, true], limit)
}

/// Builds fixed-vector copy offsets that fit between the reference and projected span points.
pub fn copy_multiple_offsets(
    origin: Vec3,
    measure: Vec3,
    extent: Vec3,
    limit: usize,
) -> (Vec<Vec3>, bool) {
    if !origin.is_finite() || !measure.is_finite() || !extent.is_finite() {
        return (Vec::new(), false);
    }
    let step = measure - origin;
    let step_length = step.length();
    if !step_length.is_finite() || step_length <= GeometryTolerance::default().absolute {
        return (Vec::new(), false);
    }
    let direction = step / step_length;
    let projected_span = (extent - origin).dot(direction);
    if !projected_span.is_finite() || projected_span < step_length {
        return (Vec::new(), false);
    }
    let count = (projected_span / step_length).floor();
    if count.is_nan() || count < 1.0 {
        return (Vec::new(), false);
    }
    let capped = !count.is_finite() || count > limit as f64;
    let copies = if count.is_finite() {
        count.min(limit as f64) as usize
    } else {
        limit
    };
    let offsets = (1..=copies).map(|index| step * index as f64).collect();
    (offsets, capped)
}

fn grid_translation_offsets(
    origin: Vec3,
    measure: Vec3,
    extent: Vec3,
    axes: [bool; 3],
    limit: usize,
) -> (Vec<Vec3>, bool) {
    if !origin.is_finite() || !measure.is_finite() || !extent.is_finite() {
        return (Vec::new(), false);
    }
    let step = measure - origin;
    let span = extent - origin;
    let steps = [step.x, step.y, step.z];
    let spans = [span.x, span.y, span.z];
    let counts: [usize; 3] = std::array::from_fn(|axis| {
        if axes[axis] {
            axis_copy_count(steps[axis], spans[axis], limit)
        } else {
            0
        }
    });
    let mut offsets = Vec::new();
    let mut capped = false;
    for ix in 0..=counts[0] {
        for iy in 0..=counts[1] {
            for iz in 0..=counts[2] {
                if ix == 0 && iy == 0 && iz == 0 {
                    continue;
                }
                if offsets.len() == limit {
                    capped = true;
                    return (offsets, capped);
                }
                offsets.push(Vec3::new(
                    steps[0] * ix as f64,
                    steps[1] * iy as f64,
                    steps[2] * iz as f64,
                ));
            }
        }
    }
    (offsets, capped)
}

fn axis_copy_count(step: f64, span: f64, limit: usize) -> usize {
    if !step.is_finite() || !span.is_finite() || step.abs() <= f64::EPSILON {
        return 0;
    }
    let ratio = span / step;
    if !ratio.is_finite() || ratio < 1.0 {
        return 0;
    }
    ratio.floor().min(limit.saturating_add(1) as f64) as usize
}

#[cfg(test)]
mod tests {
    use super::{
        can_linear_array_selection, copy_multiple_offsets, linear_array,
        planar_translation_offsets, projected_selection_span, translated_array,
        volumetric_translation_offsets,
    };
    use crate::document::{Document, OccurrenceRef};
    use crate::geometry::{Affine3, Vec3};
    use crate::selection::{
        EditContext, SelectedEntity, SelectionKind, SelectionMode, SelectionSession,
        SelectionTarget,
    };
    use serde_json::json;

    fn cube(document: &mut Document, name: &str, origin: Vec3) -> OccurrenceRef {
        document
            .add_cuboid(name, Vec3::new(1.0, 1.0, 1.0), origin)
            .unwrap()
    }

    fn add_dimension_and_text(
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
            json!([{
                "start": {"x": 0.0, "y": 0.0, "z": 0.0},
                "end": {"x": 1.0, "y": 0.0, "z": 0.0},
                "offset": {"x": 0.0, "y": 1.0, "z": 0.0},
                "label": "length"
            }]),
        );
        definition.extensions.insert(
            "k3d.texts".to_owned(),
            json!([{
                "position": {"x": 2.0, "y": 0.0, "z": 0.0},
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
    fn array_copies_selected_mesh_from_original_and_selects_only_the_results() {
        let mut document = Document::new();
        let source = cube(&mut document, "source", Vec3::ZERO);
        let face = *document
            .effective_geometry(&source)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let source_revision = document.revision();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: source.clone(),
            entity: SelectedEntity::Triangle(face),
        });

        let report = linear_array(&mut document, &mut selection, 3, Vec3::X * 2.0).unwrap();

        assert_eq!(3, report.copies);
        assert_eq!(9, report.copied_vertices);
        assert_eq!(3, report.copied_triangles);
        assert_eq!(source_revision + 3, document.revision());
        let geometry = document.effective_geometry(&source).unwrap();
        assert_eq!(15, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&face));
        assert_eq!(3, selection.selected().len());
        assert!(selection.selected().iter().all(|target| {
            target.occurrence == source
                && matches!(target.entity, SelectedEntity::Triangle(id) if id != face)
        }));
        let centers = selection
            .selected()
            .iter()
            .map(|target| {
                let SelectedEntity::Triangle(id) = target.entity else {
                    unreachable!()
                };
                let triangle = &geometry.triangles[&id];
                triangle
                    .vertices
                    .iter()
                    .filter_map(|reference| geometry.vertex_position(*reference))
                    .fold(Vec3::ZERO, |sum, point| sum + point)
                    / 3.0
            })
            .collect::<Vec<_>>();
        assert!(centers.iter().any(|center| center.x > 1.9));
        assert!(centers.iter().any(|center| center.x > 3.9));
        assert!(centers.iter().any(|center| center.x > 5.9));
    }

    #[test]
    fn linear_array_copies_selected_dimension_and_text_payloads_with_new_ids() {
        let mut document = Document::new();
        let occurrence = cube(&mut document, "annotations", Vec3::ZERO);
        let (definition_id, dimension_id, text_id) =
            add_dimension_and_text(&mut document, &occurrence);
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Definition(definition_id));
        selection.apply(
            [
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Dimension,
                        id: dimension_id,
                    },
                },
                SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::SemanticHandle {
                        kind: SelectionKind::Text,
                        id: text_id,
                    },
                },
            ],
            SelectionMode::Replace,
        );

        assert!(can_linear_array_selection(&document, &selection));
        assert_eq!(
            Some(3.2),
            projected_selection_span(&document, &selection, Vec3::X)
        );
        let report = linear_array(&mut document, &mut selection, 2, Vec3::X * 5.0).unwrap();

        assert_eq!(2, report.copies);
        assert_eq!(4, report.copied_domain_entities);
        assert_eq!(4, report.selected_results.len());
        assert!(report.selected_results.iter().all(|target| {
            target.occurrence == occurrence
                && matches!(target.entity, SelectedEntity::SemanticHandle { .. })
        }));
        let definition = document.definition(definition_id).unwrap();
        assert_eq!(
            3,
            definition.extensions["k3d.dimensions"]
                .as_array()
                .unwrap()
                .len()
        );
        assert_eq!(
            3,
            definition.extensions["k3d.texts"].as_array().unwrap().len()
        );
        assert_eq!(3, definition.domain_entity_ids["dimensions"].len());
        assert_eq!(3, definition.domain_entity_ids["texts"].len());
        let text_positions = definition.extensions["k3d.texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| {
                serde_json::from_value::<Vec3>(record["position"].clone())
                    .unwrap()
                    .x
            })
            .collect::<Vec<_>>();
        assert_eq!(vec![2.0, 7.0, 12.0], text_positions);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn invalid_annotation_identity_is_not_arrayable_and_fails_without_mutation() {
        let mut document = Document::new();
        let occurrence = cube(&mut document, "annotations", Vec3::ZERO);
        let (definition_id, _, _) = add_dimension_and_text(&mut document, &occurrence);
        let mut selection = SelectionSession::default();
        selection.set_context(EditContext::Definition(definition_id));
        selection.select(SelectionTarget {
            occurrence,
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Dimension,
                id: "missing-dimension".to_owned(),
            },
        });
        let before = document.clone();
        assert!(!can_linear_array_selection(&document, &selection));

        let error = linear_array(&mut document, &mut selection, 2, Vec3::X * 3.0).unwrap_err();

        assert!(error.contains("supported objects"));
        assert_eq!(before, document);
        assert!(document.validate().is_ok());
    }

    #[test]
    fn projected_selection_span_measures_world_geometry_for_objects_and_faces() {
        let mut document = Document::new();
        let source = cube(&mut document, "source", Vec3::new(10.0, 0.0, 0.0));
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: source.clone(),
            entity: SelectedEntity::Occurrence,
        });
        assert_eq!(
            Some(1.0),
            projected_selection_span(&document, &selection, Vec3::X)
        );

        let geometry = document.effective_geometry(&source).unwrap();
        let face = geometry
            .triangles
            .values()
            .find(|triangle| {
                let x = triangle
                    .vertices
                    .iter()
                    .filter_map(|vertex| geometry.vertex_position(*vertex))
                    .map(|point| point.x)
                    .collect::<Vec<_>>();
                x.iter()
                    .fold(f64::NEG_INFINITY, |max, value| max.max(*value))
                    - x.iter().fold(f64::INFINITY, |min, value| min.min(*value))
                    > 0.5
            })
            .unwrap()
            .id;
        selection.clear();
        selection.select(SelectionTarget {
            occurrence: source,
            entity: SelectedEntity::Triangle(face),
        });
        assert_eq!(
            Some(1.0),
            projected_selection_span(&document, &selection, Vec3::X)
        );
    }

    #[test]
    fn array_copies_selected_objects_and_mesh_entities_together() {
        let mut document = Document::new();
        let object = cube(&mut document, "object", Vec3::ZERO);
        let mesh_owner = cube(&mut document, "mesh owner", Vec3::new(4.0, 0.0, 0.0));
        let face = *document
            .effective_geometry(&mesh_owner)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.apply(
            [
                SelectionTarget {
                    occurrence: object,
                    entity: SelectedEntity::Occurrence,
                },
                SelectionTarget {
                    occurrence: mesh_owner.clone(),
                    entity: SelectedEntity::Triangle(face),
                },
            ],
            SelectionMode::Replace,
        );

        assert!(can_linear_array_selection(&document, &selection));
        let report = linear_array(&mut document, &mut selection, 2, Vec3::X * 3.0).unwrap();

        assert_eq!(2, report.copied_occurrences);
        assert_eq!(2, report.copied_triangles);
        assert_eq!(4, report.selected_results.len());
        assert_eq!(
            2,
            report
                .selected_results
                .iter()
                .filter(|target| { target.entity == SelectedEntity::Occurrence })
                .count()
        );
        assert_eq!(
            2,
            report
                .selected_results
                .iter()
                .filter(|target| { matches!(target.entity, SelectedEntity::Triangle(_)) })
                .count()
        );
    }

    #[test]
    fn nested_object_array_preserves_source_paths_and_prior_results() {
        let mut document = Document::new();
        let child = cube(&mut document, "nested child", Vec3::ZERO);
        let mesh_child = cube(&mut document, "mesh child", Vec3::X * 5.0);
        let parent = crate::operations::objects::group_occurrences(
            &mut document,
            &[child, mesh_child],
            "parent",
        )
        .unwrap();
        let parent_copy = document
            .copy_root_occurrence(&parent, Vec3::X * 20.0)
            .unwrap();
        let child_layout = document
            .resolve_occurrence(&parent)
            .unwrap()
            .definition
            .default_child_layout
            .unwrap();
        let entries = &document.child_layout(child_layout).unwrap().entries;
        let child_entry = entries[0].id;
        let mesh_entry = entries[1].id;
        let nested = parent.child(child_entry);
        let mesh_occurrence = parent.child(mesh_entry);
        let face = *document
            .effective_geometry(&mesh_occurrence)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.set_context(crate::selection::EditContext::Occurrence(parent.clone()));
        selection.apply(
            [
                SelectionTarget {
                    occurrence: nested,
                    entity: SelectedEntity::Occurrence,
                },
                SelectionTarget {
                    occurrence: mesh_occurrence.clone(),
                    entity: SelectedEntity::Triangle(face),
                },
            ],
            SelectionMode::Replace,
        );

        assert!(can_linear_array_selection(&document, &selection));
        let report = linear_array(&mut document, &mut selection, 2, Vec3::X * 2.0).unwrap();

        assert_eq!(2, report.copied_occurrences);
        assert_eq!(2, report.copied_triangles);
        assert_eq!(4, report.selected_results.len());
        assert!(report.selected_results.iter().all(|target| {
            target.occurrence.path.starts_with(&parent.path)
                && document.resolve_occurrence(&target.occurrence).is_some()
        }));
        let copied_positions = report
            .selected_results
            .iter()
            .filter(|target| target.entity == SelectedEntity::Occurrence)
            .map(|target| {
                document
                    .resolve_occurrence(&target.occurrence)
                    .unwrap()
                    .world_transform
                    .translation
                    .x
            })
            .collect::<Vec<_>>();
        assert_eq!(2, copied_positions.len());
        assert!(copied_positions.contains(&2.0));
        assert!(copied_positions.contains(&4.0));
        let private_layout = document
            .resolve_occurrence(&parent)
            .unwrap()
            .definition
            .default_child_layout
            .unwrap();
        assert_eq!(
            4,
            document.child_layout(private_layout).unwrap().entries.len()
        );
        assert_eq!(
            4,
            document.child_layout(child_layout).unwrap().entries.len()
        );
        let peer_child = document
            .resolve_occurrence(&parent_copy)
            .unwrap()
            .definition
            .default_child_layout
            .and_then(|layout| document.child_layout(layout))
            .unwrap()
            .entries[0]
            .id;
        assert!(
            document
                .resolve_occurrence(&parent_copy.child(peer_child))
                .is_some()
        );
        assert_eq!(
            2,
            report
                .selected_results
                .iter()
                .filter(|target| target.entity == SelectedEntity::Occurrence)
                .count()
        );
        assert_eq!(
            2,
            report
                .selected_results
                .iter()
                .filter(|target| matches!(target.entity, SelectedEntity::Triangle(_)))
                .count()
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn invalid_array_requests_leave_document_and_selection_unchanged() {
        let mut document = Document::new();
        let source = cube(&mut document, "source", Vec3::ZERO);
        let face = *document
            .effective_geometry(&source)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: source,
            entity: SelectedEntity::Triangle(face),
        });
        let before_document = document.clone();
        let before_selection = selection.clone();

        assert!(linear_array(&mut document, &mut selection, 0, Vec3::X).is_err());
        assert!(linear_array(&mut document, &mut selection, 1, Vec3::ZERO).is_err());

        assert_eq!(before_document, document);
        assert_eq!(before_selection, selection);
    }

    #[test]
    fn planar_array_uses_signed_xz_counts_and_excludes_the_source_cell() {
        let (offsets, capped) = planar_translation_offsets(
            Vec3::ZERO,
            Vec3::new(-2.0, 7.0, 3.0),
            Vec3::new(-4.5, -10.0, 6.1),
        );

        assert!(!capped);
        assert_eq!(8, offsets.len());
        assert!(offsets.contains(&Vec3::new(-2.0, 0.0, 0.0)));
        assert!(offsets.contains(&Vec3::new(0.0, 0.0, 6.0)));
        assert!(offsets.contains(&Vec3::new(-4.0, 0.0, 6.0)));
        assert!(!offsets.contains(&Vec3::ZERO));
    }

    #[test]
    fn volumetric_array_uses_xyz_steps_and_caps_output() {
        let (offsets, capped) = volumetric_translation_offsets(
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(20.0, 20.0, 20.0),
        );

        assert_eq!(super::MAX_MULTIPLE_ARRAY_COPIES, offsets.len());
        assert!(capped);
        assert!(!offsets.contains(&Vec3::ZERO));
    }

    #[test]
    fn copy_multiple_counts_projected_span_without_overshooting() {
        let (offsets, capped) = copy_multiple_offsets(
            Vec3::ZERO,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(5.5, 0.0, 3.0),
            2_048,
        );

        assert!(!capped);
        assert_eq!(2, offsets.len());
        assert_eq!(vec![Vec3::X * 2.0, Vec3::X * 4.0], offsets);
    }

    #[test]
    fn translated_array_copies_each_selected_target_at_the_supplied_offsets() {
        let mut document = Document::new();
        let source = cube(&mut document, "source", Vec3::ZERO);
        let face = *document
            .effective_geometry(&source)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: source.clone(),
            entity: SelectedEntity::Triangle(face),
        });

        let report = translated_array(
            &mut document,
            &mut selection,
            &[Vec3::X * 2.0, Vec3::Z * 3.0],
        )
        .unwrap();

        assert_eq!(2, report.copies);
        assert_eq!(2, report.selected_results.len());
        assert!(
            document
                .effective_geometry(&source)
                .unwrap()
                .triangles
                .contains_key(&face)
        );
    }

    #[test]
    fn transformed_array_copies_selected_geometry_through_rotation_affines() {
        let mut document = Document::new();
        let source = cube(&mut document, "source", Vec3::ZERO);
        let face = *document
            .effective_geometry(&source)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        let mut selection = SelectionSession::default();
        selection.select(SelectionTarget {
            occurrence: source.clone(),
            entity: SelectedEntity::Triangle(face),
        });
        let quarter_turn = Affine3::from_linear_translation(
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            Vec3::ZERO,
        );

        let report =
            super::transformed_array(&mut document, &mut selection, &[quarter_turn]).unwrap();

        assert_eq!(1, report.copies);
        assert_eq!(1, selection.selected().len());
        let geometry = document.effective_geometry(&source).unwrap();
        assert_eq!(13, geometry.triangles.len());
        assert!(geometry.triangles.contains_key(&face));
    }
}
