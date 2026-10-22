//! One-pick object placement state with an optional surface-glue frame.

use crate::document::{DefinitionId, Document, OccurrenceRef};
use crate::geometry::{Affine3, Vec3};
use crate::operations::objects::place_definition;
use crate::selection::EditContext;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementHit {
    pub position: Vec3,
    pub surface_normal: Option<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementPreview {
    pub hit: PlacementHit,
    pub world_transform: Affine3,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectPlaceEffect {
    Inactive,
    NeedHit,
    Preview(PlacementPreview),
    Placed(OccurrenceRef),
    Invalid(String),
    Canceled,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectPlaceToolState {
    definition: Option<DefinitionId>,
    glue_to_surface: bool,
    pointer_hit: Option<PlacementHit>,
}

impl ObjectPlaceToolState {
    pub fn active_definition(&self) -> Option<DefinitionId> {
        self.definition
    }

    pub fn glue_to_surface(&self) -> bool {
        self.glue_to_surface
    }

    pub fn activate(&mut self, definition: DefinitionId, glue_to_surface: bool) {
        self.definition = Some(definition);
        self.glue_to_surface = glue_to_surface;
        self.pointer_hit = None;
    }

    pub fn set_glue_to_surface(&mut self, enabled: bool) -> bool {
        if self.definition.is_none() || self.glue_to_surface == enabled {
            return false;
        }
        self.glue_to_surface = enabled;
        true
    }

    pub fn update_pointer(&mut self, hit: Option<PlacementHit>) {
        self.pointer_hit = hit.filter(|hit| {
            hit.position.is_finite()
                && hit
                    .surface_normal
                    .is_none_or(|normal| normal.is_finite() && normal.length() > 1.0e-12)
        });
    }

    pub fn preview(
        &self,
        document: &Document,
        context: &EditContext,
    ) -> Result<Option<PlacementPreview>, String> {
        let (Some(definition), Some(hit)) = (self.definition, self.pointer_hit) else {
            return Ok(None);
        };
        let world_transform = crate::operations::objects::placement_world_transform(
            document,
            context,
            definition,
            hit.position,
            hit.surface_normal,
            self.glue_to_surface,
        )?;
        Ok(Some(PlacementPreview {
            hit,
            world_transform,
        }))
    }

    pub fn pick(
        &mut self,
        document: &mut Document,
        context: &EditContext,
        hit: PlacementHit,
    ) -> ObjectPlaceEffect {
        if self.definition.is_none() {
            return ObjectPlaceEffect::Inactive;
        }
        if !hit.position.is_finite()
            || hit
                .surface_normal
                .is_some_and(|normal| !normal.is_finite() || normal.length() <= 1.0e-12)
        {
            return ObjectPlaceEffect::Invalid(
                "pick a finite position and valid surface normal".to_owned(),
            );
        }
        let Some(definition) = self.definition else {
            return ObjectPlaceEffect::Inactive;
        };
        match place_definition(
            document,
            context,
            definition,
            hit.position,
            hit.surface_normal,
            self.glue_to_surface,
        ) {
            Ok(occurrence) => {
                self.cancel();
                ObjectPlaceEffect::Placed(occurrence)
            }
            Err(error) => ObjectPlaceEffect::Invalid(error),
        }
    }

    pub fn cancel(&mut self) -> bool {
        let changed = self.definition.take().is_some() || self.pointer_hit.take().is_some();
        self.glue_to_surface = false;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectPlaceEffect, ObjectPlaceToolState, PlacementHit};
    use crate::document::Document;
    use crate::geometry::Vec3;
    use crate::selection::EditContext;

    #[test]
    fn placement_waits_for_a_hit_then_commits_once_and_cancels_cleanly() {
        let mut document = Document::new();
        let source = document
            .add_cuboid("prototype", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        let definition = document.resolve_occurrence(&source).unwrap().definition.id;
        let mut state = ObjectPlaceToolState::default();
        state.activate(definition, false);
        assert!(
            state
                .preview(&document, &EditContext::Document)
                .unwrap()
                .is_none()
        );
        let hit = PlacementHit {
            position: Vec3::new(5.0, 2.0, -1.0),
            surface_normal: None,
        };
        state.update_pointer(Some(hit));
        assert!(
            state
                .preview(&document, &EditContext::Document)
                .unwrap()
                .is_some()
        );
        let result = state.pick(&mut document, &EditContext::Document, hit);
        let ObjectPlaceEffect::Placed(occurrence) = result else {
            panic!("valid placement should complete")
        };
        assert!(document.resolve_occurrence(&occurrence).is_some());
        assert_eq!(None, state.active_definition());
        state.activate(definition, true);
        assert!(state.cancel());
        assert!(!state.cancel());
    }
}
