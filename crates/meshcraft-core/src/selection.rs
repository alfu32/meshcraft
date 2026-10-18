//! Non-persistent editor selection and edit-context state.

use crate::document::{DefinitionId, Document, OccurrenceRef, SegmentId, TriangleId, VertexId};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SelectedEntity {
    Occurrence,
    Vertex(VertexId),
    Segment(SegmentId),
    Triangle(TriangleId),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SelectionTarget {
    pub occurrence: OccurrenceRef,
    pub entity: SelectedEntity,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum EditContext {
    #[default]
    Document,
    Definition(DefinitionId),
    Occurrence(OccurrenceRef),
}

/// Session-only state. It intentionally has no persistence or UI framework types.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionSession {
    pub context: EditContext,
    selected: BTreeSet<SelectionTarget>,
    revision: u64,
}

impl SelectionSession {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selected(&self) -> &BTreeSet<SelectionTarget> {
        &self.selected
    }

    pub fn set_context(&mut self, context: EditContext) -> bool {
        if self.context == context {
            return false;
        }
        self.context = context;
        self.selected.clear();
        self.bump_revision();
        true
    }

    pub fn select(&mut self, target: SelectionTarget) -> bool {
        if !self.selected.insert(target) {
            return false;
        }
        self.bump_revision();
        true
    }

    pub fn deselect(&mut self, target: &SelectionTarget) -> bool {
        if !self.selected.remove(target) {
            return false;
        }
        self.bump_revision();
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        self.selected.clear();
        self.bump_revision();
        true
    }

    /// Drops stale paths and entity IDs after a document edit without changing authored data.
    pub fn retain_valid(&mut self, document: &Document) -> bool {
        let context_valid = match &self.context {
            EditContext::Document => true,
            EditContext::Definition(id) => document.definition(*id).is_some(),
            EditContext::Occurrence(occurrence) => {
                document.resolve_occurrence(occurrence).is_some()
            }
        };
        if !context_valid {
            self.context = EditContext::Document;
            self.selected.clear();
            self.bump_revision();
            return true;
        }
        let before = self.selected.len();
        self.selected.retain(|target| {
            let Some(resolved) = document.resolve_occurrence(&target.occurrence) else {
                return false;
            };
            let Some(geometry) = document.effective_geometry(&target.occurrence) else {
                return false;
            };
            let entity_exists = match target.entity {
                SelectedEntity::Occurrence => true,
                SelectedEntity::Vertex(id) => geometry.vertices.contains_key(&id),
                SelectedEntity::Segment(id) => geometry.segments.contains_key(&id),
                SelectedEntity::Triangle(id) => geometry.triangles.contains_key(&id),
            };
            let context_matches = match &self.context {
                EditContext::Definition(id) => resolved.definition.id == *id,
                EditContext::Occurrence(occurrence) => occurrence == &target.occurrence,
                EditContext::Document => true,
            };
            entity_exists && context_matches
        });
        if self.selected.len() == before {
            return false;
        }
        self.bump_revision();
        true
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{EditContext, SelectedEntity, SelectionSession, SelectionTarget};
    use crate::document::{DefinitionKind, Document};
    use crate::geometry::{Affine3, Vec3};

    #[test]
    fn selection_is_occurrence_scoped_and_prunes_deleted_entities() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("shared", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition = document
            .resolve_occurrence(&occurrence)
            .unwrap()
            .definition
            .id;
        let copy = document
            .copy_root_occurrence(&occurrence, Vec3::new(2.0, 0.0, 0.0))
            .unwrap();
        let vertex = *document
            .effective_geometry(&occurrence)
            .unwrap()
            .vertices
            .keys()
            .next()
            .unwrap();

        let first_target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Vertex(vertex),
        };
        let second_target = SelectionTarget {
            occurrence: copy.clone(),
            entity: SelectedEntity::Vertex(vertex),
        };
        let mut session = SelectionSession::default();
        assert!(session.select(first_target.clone()));
        assert!(!session.selected().contains(&second_target));
        assert!(session.set_context(EditContext::Definition(definition)));
        assert!(session.selected().is_empty());
        assert!(session.select(first_target));

        let empty = crate::document::InstanceEdits {
            geometry_replacement: Some(crate::document::GeometryBundle::default()),
            ..crate::document::InstanceEdits::default()
        };
        assert!(document.set_occurrence_edits(occurrence.clone(), empty));
        assert!(session.retain_valid(&document));
        assert!(session.selected().is_empty());
    }

    #[test]
    fn session_context_and_selection_are_not_document_persistence() {
        let mut document = Document::new();
        let definition = document.add_definition("plain", DefinitionKind::Mesh);
        let entry = document
            .add_layout_entry(document.root_layout(), definition, Affine3::IDENTITY)
            .unwrap();
        let mut session = SelectionSession::default();
        session.set_context(EditContext::Occurrence(
            crate::document::OccurrenceRef::root(entry),
        ));
        let before = serde_json::to_string(&document).unwrap();
        session.select(SelectionTarget {
            occurrence: crate::document::OccurrenceRef::root(entry),
            entity: SelectedEntity::Occurrence,
        });
        let after = serde_json::to_string(&document).unwrap();
        assert_eq!(before, after);
    }
}
