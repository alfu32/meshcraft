//! Selection-first copy patterns built on the shared transform operation.

use crate::document::Document;
use crate::geometry::{Affine3, GeometryTolerance, Vec3};
use crate::operations::transforms::{self, TransformReport};
use crate::selection::{SelectedEntity, SelectionMode, SelectionSession, SelectionTarget};
use std::collections::BTreeSet;

pub const MAX_LINEAR_ARRAY_COPIES: usize = 10_000;

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
    if !can_linear_array_selection(document, selection) {
        return Err("select supported object instances or mesh entities to array".to_owned());
    }

    let mut candidate = document.clone();
    let original_selection = selection.clone();
    let mut resulting_selection = original_selection.clone();
    resulting_selection.clear();
    let mut result_targets = BTreeSet::new();
    let mut report = LinearArrayReport {
        copies,
        ..LinearArrayReport::default()
    };

    for index in 1..=copies {
        let distance = index as f64;
        let offset = step * distance;
        if !offset.is_finite() {
            return Err("linear array offsets exceed the supported coordinate range".to_owned());
        }
        let mut copy_selection = original_selection.clone();
        let transform_report = transforms::transform_selection(
            &mut candidate,
            &mut copy_selection,
            Affine3::from_translation(offset),
            true,
        )?;
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

#[cfg(test)]
mod tests {
    use super::{can_linear_array_selection, linear_array};
    use crate::document::{Document, OccurrenceRef};
    use crate::geometry::Vec3;
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
}
