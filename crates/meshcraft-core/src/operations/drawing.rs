//! Authored line and planar construction operations.

use crate::document::{DefinitionId, Document, LineInsertion};
use crate::geometry::Vec3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineOwner {
    SceneRoot,
    Definition(DefinitionId),
}

/// Inserts a line in one active geometry owner. The operation never visits or cuts faces.
pub fn insert_line(
    document: &mut Document,
    owner: LineOwner,
    start: Vec3,
    end: Vec3,
) -> Result<LineInsertion, String> {
    let result = match owner {
        LineOwner::SceneRoot => document.append_line_segment_to_scene_root(start, end),
        LineOwner::Definition(definition) => {
            document.append_line_segment_to_definition_with_remap(definition, start, end)
        }
    };
    result.ok_or_else(|| "Line endpoints must be distinct finite points.".to_owned())
}
