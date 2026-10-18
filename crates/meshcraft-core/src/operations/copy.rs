//! Selection-first copy patterns built on the shared transform operation.

use crate::document::Document;
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use crate::operations::transforms::{self, TransformReport};
use crate::selection::{SelectedEntity, SelectionMode, SelectionSession, SelectionTarget};
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
    pub selected_results: Vec<SelectionTarget>,
}

impl LinearArrayReport {
    fn include_transform(&mut self, report: TransformReport) {
        self.copied_vertices += report.copied_vertices;
        self.copied_segments += report.copied_segments;
        self.copied_triangles += report.copied_triangles;
        self.copied_occurrences += report.copied_occurrences;
    }
}

/// Returns whether the current selection contains existing objects or mesh entities supported by
/// the shared copy transform. Nested object occurrences and semantic handles need domain adapters.
pub fn can_linear_array_selection(document: &Document, selection: &SelectionSession) -> bool {
    !selection.selected().is_empty()
        && selection.selected().iter().all(|target| {
            let Some(geometry) = document.effective_geometry(&target.occurrence) else {
                return false;
            };
            match target.entity {
                SelectedEntity::Occurrence => {
                    target.occurrence.path.len() == 1
                        && document.resolve_occurrence(&target.occurrence).is_some()
                }
                SelectedEntity::Vertex(id) => geometry.vertices.contains_key(&id),
                SelectedEntity::Segment(id) => geometry.segments.contains_key(&id),
                SelectedEntity::Triangle(id) => geometry.triangles.contains_key(&id),
                SelectedEntity::SemanticHandle { .. } => false,
            }
        })
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
        return Err("select supported object instances or mesh entities to array".to_owned());
    }

    let mut candidate = document.clone();
    let original_selection = selection.clone();
    let mut resulting_selection = original_selection.clone();
    resulting_selection.clear();
    let mut result_targets = BTreeSet::new();
    let mut report = LinearArrayReport {
        copies: affines.len(),
        ..LinearArrayReport::default()
    };

    for transform in affines {
        let mut copy_selection = original_selection.clone();
        let transform_report =
            transforms::transform_selection(&mut candidate, &mut copy_selection, *transform, true)?;
        if !transform_report.changed() || copy_selection.selected().is_empty() {
            return Err("linear array could not create a copy of the selection".to_owned());
        }
        report.include_transform(transform_report);
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
    use crate::selection::{SelectedEntity, SelectionMode, SelectionSession, SelectionTarget};

    fn cube(document: &mut Document, name: &str, origin: Vec3) -> OccurrenceRef {
        document
            .add_cuboid(name, Vec3::new(1.0, 1.0, 1.0), origin)
            .unwrap()
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
