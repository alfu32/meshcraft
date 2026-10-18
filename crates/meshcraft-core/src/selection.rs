//! Non-persistent editor selection and edit-context state.

use crate::document::{DefinitionId, Document, OccurrenceRef, SegmentId, TriangleId, VertexId};
use crate::geometry::Vec2;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SelectionKind {
    Vertex,
    Edge,
    Face,
    Object,
    Voxel,
    Hotspot,
    Dimension,
    Text,
    Wall,
    Slab,
    Stair,
    Frame,
    Hvac,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionDisplayState {
    pub visible: bool,
    pub unlocked: bool,
    pub wireframe: bool,
}

impl Default for SelectionDisplayState {
    fn default() -> Self {
        Self {
            visible: true,
            unlocked: true,
            wireframe: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionDisplayPolicy {
    states: BTreeMap<SelectionKind, SelectionDisplayState>,
}

impl Default for SelectionDisplayPolicy {
    fn default() -> Self {
        let mut states = BTreeMap::new();
        for kind in [
            SelectionKind::Vertex,
            SelectionKind::Edge,
            SelectionKind::Face,
            SelectionKind::Object,
            SelectionKind::Voxel,
            SelectionKind::Hotspot,
            SelectionKind::Dimension,
            SelectionKind::Text,
            SelectionKind::Wall,
            SelectionKind::Slab,
            SelectionKind::Stair,
            SelectionKind::Frame,
            SelectionKind::Hvac,
        ] {
            states.insert(kind, SelectionDisplayState::default());
        }
        Self { states }
    }
}

impl SelectionDisplayPolicy {
    pub fn state(&self, kind: SelectionKind) -> SelectionDisplayState {
        self.states.get(&kind).copied().unwrap_or_default()
    }

    pub fn can_draw(&self, kind: SelectionKind) -> bool {
        self.state(kind).visible
    }

    pub fn can_select(&self, kind: SelectionKind) -> bool {
        let state = self.state(kind);
        state.visible && state.unlocked
    }

    pub fn set_visible(&mut self, kind: SelectionKind, visible: bool) -> bool {
        let state = self.states.entry(kind).or_default();
        if state.visible == visible {
            return false;
        }
        state.visible = visible;
        state.unlocked = visible;
        true
    }

    pub fn set_unlocked(&mut self, kind: SelectionKind, unlocked: bool) -> bool {
        let state = self.states.entry(kind).or_default();
        let unlocked = unlocked && state.visible;
        if state.unlocked == unlocked {
            return false;
        }
        state.unlocked = unlocked;
        true
    }

    pub fn set_wireframe(&mut self, kind: SelectionKind, wireframe: bool) -> bool {
        let state = self.states.entry(kind).or_default();
        let wireframe = wireframe
            && matches!(
                kind,
                SelectionKind::Face
                    | SelectionKind::Voxel
                    | SelectionKind::Wall
                    | SelectionKind::Slab
                    | SelectionKind::Stair
                    | SelectionKind::Frame
                    | SelectionKind::Hvac
            );
        if state.wireframe == wireframe {
            return false;
        }
        state.wireframe = wireframe;
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMode {
    Replace,
    Add,
    Remove,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SelectedEntity {
    Occurrence,
    Vertex(VertexId),
    Segment(SegmentId),
    Triangle(TriangleId),
}

impl SelectedEntity {
    pub fn kind(&self) -> SelectionKind {
        match self {
            Self::Occurrence => SelectionKind::Object,
            Self::Vertex(_) => SelectionKind::Vertex,
            Self::Segment(_) => SelectionKind::Edge,
            Self::Triangle(_) => SelectionKind::Face,
        }
    }
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

    /// Applies one replace/add/remove operation and returns the number of changed targets.
    pub fn apply(
        &mut self,
        targets: impl IntoIterator<Item = SelectionTarget>,
        mode: SelectionMode,
    ) -> usize {
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        let changed = match mode {
            SelectionMode::Replace => {
                let changed = self.selected.symmetric_difference(&targets).count();
                if changed > 0 {
                    self.selected = targets;
                }
                changed
            }
            SelectionMode::Add => targets
                .into_iter()
                .filter(|target| self.selected.insert(target.clone()))
                .count(),
            SelectionMode::Remove => targets
                .into_iter()
                .filter(|target| self.selected.remove(target))
                .count(),
        };
        if changed > 0 {
            self.bump_revision();
        }
        changed
    }

    pub fn summary(&self) -> SelectionSummary {
        let mut summary = SelectionSummary::default();
        for target in &self.selected {
            match target.entity {
                SelectedEntity::Occurrence => summary.objects += 1,
                SelectedEntity::Vertex(_) => summary.vertices += 1,
                SelectedEntity::Segment(_) => summary.edges += 1,
                SelectedEntity::Triangle(_) => summary.faces += 1,
            }
        }
        summary
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SelectionSummary {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    pub objects: usize,
}

impl SelectionSummary {
    pub fn total(self) -> usize {
        self.vertices + self.edges + self.faces + self.objects
    }
}

/// Tracks a repeated pointer click using an injected monotonic millisecond timestamp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelectionClickTracker {
    last_click: Option<(u64, Vec2)>,
    click_count: u8,
}

impl SelectionClickTracker {
    pub const MULTI_CLICK_INTERVAL_MS: u64 = 350;
    pub const MULTI_CLICK_DISTANCE: f64 = 6.0;

    pub fn record(&mut self, at_ms: u64, screen_position_px: Vec2) -> u8 {
        let within_sequence = self.last_click.is_some_and(|(last_ms, last_position)| {
            at_ms
                .checked_sub(last_ms)
                .is_some_and(|elapsed| elapsed <= Self::MULTI_CLICK_INTERVAL_MS)
                && (screen_position_px - last_position).length_squared()
                    <= Self::MULTI_CLICK_DISTANCE * Self::MULTI_CLICK_DISTANCE
        });
        self.click_count = if within_sequence {
            self.click_count.saturating_add(1).min(3)
        } else {
            1
        };
        self.last_click = Some((at_ms, screen_position_px));
        self.click_count
    }

    pub fn reset(&mut self) {
        self.last_click = None;
        self.click_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditContext, SelectedEntity, SelectionClickTracker, SelectionDisplayPolicy, SelectionKind,
        SelectionMode, SelectionSession, SelectionTarget,
    };
    use crate::document::{DefinitionKind, Document};
    use crate::geometry::{Affine3, Vec2, Vec3};

    #[test]
    fn mixed_selection_operations_are_atomic_and_report_per_type_counts() {
        let mut document = Document::new();
        let occurrence = document
            .add_cuboid("mixed", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let geometry = document.effective_geometry(&occurrence).unwrap();
        let vertex = *geometry.vertices.keys().next().unwrap();
        let edge = *geometry.segments.keys().next().unwrap();
        let face = *geometry.triangles.keys().next().unwrap();
        let object_target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Occurrence,
        };
        let edge_target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Segment(edge),
        };
        let face_target = SelectionTarget {
            occurrence: occurrence.clone(),
            entity: SelectedEntity::Triangle(face),
        };
        let vertex_target = SelectionTarget {
            occurrence,
            entity: SelectedEntity::Vertex(vertex),
        };

        let mut selection = SelectionSession::default();
        assert_eq!(
            3,
            selection.apply(
                [
                    object_target.clone(),
                    edge_target.clone(),
                    face_target.clone()
                ],
                SelectionMode::Replace
            )
        );
        assert_eq!(1, selection.revision());
        assert_eq!(
            0,
            selection.apply(
                [
                    object_target.clone(),
                    edge_target.clone(),
                    face_target.clone()
                ],
                SelectionMode::Replace
            )
        );
        assert_eq!(1, selection.revision(), "identical replacement is a no-op");
        assert_eq!(
            1,
            selection.apply([vertex_target.clone()], SelectionMode::Add)
        );
        assert_eq!(1, selection.apply([edge_target], SelectionMode::Remove));
        assert_eq!(
            (1, 1, 0, 1, 3),
            (
                selection.summary().vertices,
                selection.summary().faces,
                selection.summary().edges,
                selection.summary().objects,
                selection.summary().total(),
            )
        );
        assert_eq!(3, selection.revision());
    }

    #[test]
    fn display_policy_unifies_visibility_lock_and_wireframe_rules() {
        let mut policy = SelectionDisplayPolicy::default();
        assert!(policy.can_draw(SelectionKind::Edge));
        assert!(policy.can_select(SelectionKind::Edge));
        assert!(!policy.set_visible(SelectionKind::Edge, true));
        assert!(policy.set_visible(SelectionKind::Edge, false));
        assert!(!policy.can_draw(SelectionKind::Edge));
        assert!(!policy.can_select(SelectionKind::Edge));
        assert!(policy.set_visible(SelectionKind::Edge, true));
        assert!(policy.can_select(SelectionKind::Edge));
        assert!(policy.set_unlocked(SelectionKind::Face, false));
        assert!(!policy.can_select(SelectionKind::Face));
        assert!(policy.set_wireframe(SelectionKind::Face, true));
        assert!(policy.state(SelectionKind::Face).wireframe);
        assert!(!policy.set_wireframe(SelectionKind::Edge, true));
        assert!(!policy.state(SelectionKind::Edge).wireframe);
    }

    #[test]
    fn selection_click_tracker_uses_injected_time_distance_and_reset() {
        let mut tracker = SelectionClickTracker::default();
        let start = Vec2::new(24.0, 36.0);
        assert_eq!(1, tracker.record(100, start));
        assert_eq!(2, tracker.record(450, start));
        assert_eq!(3, tracker.record(800, start));
        assert_eq!(1, tracker.record(1_151, start));
        assert_eq!(1, tracker.record(1_200, start + Vec2::new(7.0, 0.0)));
        assert_eq!(1, tracker.record(1_199, start));
        tracker.reset();
        assert_eq!(1, tracker.record(2_000, start));
    }

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
