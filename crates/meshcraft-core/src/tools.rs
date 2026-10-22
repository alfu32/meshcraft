//! Framework-independent tool input state and preview contracts.

use crate::geometry::Vec3;

pub mod arrays;
pub mod object_place;
pub mod paths;
pub mod primitives;
pub mod revolve;
pub mod shapes;
pub mod transforms;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DrawingTool {
    #[default]
    Select,
    Line,
    ConstructionLine,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToolEvent<'a> {
    Activate(DrawingTool),
    PointerMoved(Vec3),
    Pick(Vec3),
    Text(&'a str),
    FocusDistance,
    Backspace,
    Enter,
    Escape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToolEffect {
    None,
    AnchorPlaced(Vec3),
    CommitSegment {
        start: Vec3,
        end: Vec3,
        chain_continues: bool,
    },
    FinishChain,
    Cancel,
    InvalidInput,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolPreview {
    pub segment: Option<[Vec3; 2]>,
    pub measurement: Option<f64>,
}

/// Legacy keyboard groups that activate the next tool in a fixed cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCycle {
    PlanarConstruction,
    Annotation,
    Operations,
    Modification,
}

/// A result of advancing a legacy tool cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCycleEffect {
    ActivateTool(&'static str),
    BeginHotspotPlacement,
}

/// Session state for the legacy hotspot placement action, which is not a ToolId.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ToolCycleState {
    hotspot_placement_active: bool,
}

impl ToolCycleState {
    pub fn hotspot_placement_active(&self) -> bool {
        self.hotspot_placement_active
    }

    pub fn begin_hotspot_placement(&mut self) {
        self.hotspot_placement_active = true;
    }

    pub fn cancel_hotspot_placement(&mut self) -> bool {
        std::mem::replace(&mut self.hotspot_placement_active, false)
    }

    /// Advance a shortcut cycle using the active tool's legacy display name.
    pub fn advance(&mut self, cycle: ToolCycle, active_tool: &str) -> ToolCycleEffect {
        if cycle == ToolCycle::PlanarConstruction {
            if self.cancel_hotspot_placement() {
                return ToolCycleEffect::ActivateTool("Construction Line");
            }
            return match active_tool {
                "Construction Line" => ToolCycleEffect::ActivateTool("Line"),
                "Line" => ToolCycleEffect::ActivateTool("Polyline"),
                "Polyline" => ToolCycleEffect::ActivateTool("Double Line"),
                "Double Line" => ToolCycleEffect::ActivateTool("Rectangle"),
                "Rectangle" => ToolCycleEffect::ActivateTool("Surface Rect"),
                "Surface Rect" => ToolCycleEffect::ActivateTool("Mesh"),
                "Mesh" => ToolCycleEffect::ActivateTool("Quad"),
                "Quad" => ToolCycleEffect::ActivateTool("Circle"),
                _ => {
                    self.hotspot_placement_active = true;
                    ToolCycleEffect::BeginHotspotPlacement
                }
            };
        }

        self.cancel_hotspot_placement();
        let tools = match cycle {
            ToolCycle::PlanarConstruction => unreachable!("handled above"),
            ToolCycle::Annotation => &["Linear Dimension", "Text", "Vector Text"][..],
            ToolCycle::Operations => &[
                "Line Offset",
                "Push/Pull",
                "Extrude Swipe",
                "Extrude Swipe 3D",
                "Plane Section",
            ][..],
            ToolCycle::Modification => &[
                "Move",
                "Rotate",
                "Rotate 2",
                "Scale",
                "Squash",
                "Stretch",
                "Stretch Scale",
                "Rotate Stretch",
                "Copy Multiple",
                "Planar Translate Multiple",
                "Volumetric Translate Multiple",
                "Planar Rotate Multiple",
                "Helicoidal Rotate Multiple",
                "Rotational Array",
                "Helical Array",
                "Rotational Array Even",
                "Helical Array Even",
            ][..],
        };
        let next = if active_tool == "Select" {
            0
        } else {
            tools
                .iter()
                .position(|tool| *tool == active_tool)
                .map_or(0, |index| (index + 1) % tools.len())
        };
        ToolCycleEffect::ActivateTool(tools[next])
    }
}

/// Transient state shared by Line and Construction Line.
///
/// A completed Line segment advances the anchor and remains undoable as its own transaction.
/// Enter ends the current chain while retaining the active tool; Escape discards only transient
/// input and leaves already committed segments intact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawingToolState {
    active: DrawingTool,
    anchor: Option<Vec3>,
    pointer: Option<Vec3>,
    numeric_input: String,
    distance_focused: bool,
}

impl DrawingToolState {
    pub fn active(&self) -> DrawingTool {
        self.active
    }

    pub fn anchor(&self) -> Option<Vec3> {
        self.anchor
    }

    pub fn numeric_input(&self) -> &str {
        &self.numeric_input
    }

    pub fn handle(&mut self, event: ToolEvent<'_>) -> ToolEffect {
        match event {
            ToolEvent::Activate(tool) => {
                self.active = tool;
                self.clear_transient();
                ToolEffect::None
            }
            ToolEvent::PointerMoved(point) => {
                self.pointer = point.is_finite().then_some(point);
                ToolEffect::None
            }
            ToolEvent::Pick(point) => {
                if !point.is_finite() {
                    return ToolEffect::InvalidInput;
                }
                if self.active == DrawingTool::Select {
                    return ToolEffect::None;
                }
                let Some(start) = self.anchor else {
                    self.anchor = Some(point);
                    self.pointer = Some(point);
                    self.numeric_input.clear();
                    self.distance_focused = false;
                    return ToolEffect::AnchorPlaced(point);
                };
                let Some(end) = self.constrained_endpoint(point) else {
                    return ToolEffect::InvalidInput;
                };
                if start.distance(end) <= 1.0e-9 {
                    return ToolEffect::InvalidInput;
                }
                let chain_continues = self.active == DrawingTool::Line;
                if chain_continues {
                    self.anchor = Some(end);
                    self.pointer = Some(end);
                } else {
                    self.anchor = None;
                    self.pointer = None;
                }
                self.numeric_input.clear();
                self.distance_focused = false;
                ToolEffect::CommitSegment {
                    start,
                    end,
                    chain_continues,
                }
            }
            ToolEvent::Text(text) => {
                if self.active == DrawingTool::Select || self.anchor.is_none() {
                    return ToolEffect::None;
                }
                for character in text.chars() {
                    if character.is_ascii_digit()
                        || matches!(character, '.' | '-' | '+' | 'e' | 'E')
                    {
                        self.numeric_input.push(character);
                    }
                }
                self.distance_focused = !self.numeric_input.is_empty();
                ToolEffect::None
            }
            ToolEvent::FocusDistance => {
                if self.active != DrawingTool::Select && self.anchor.is_some() {
                    self.distance_focused = true;
                }
                ToolEffect::None
            }
            ToolEvent::Backspace => {
                if self.numeric_input.pop().is_none() {
                    self.anchor = None;
                    self.pointer = None;
                }
                self.distance_focused = !self.numeric_input.is_empty();
                ToolEffect::None
            }
            ToolEvent::Enter => {
                if self.active == DrawingTool::Select {
                    return ToolEffect::None;
                }
                if !self.numeric_input.is_empty() {
                    let Some(pointer) = self.pointer else {
                        return ToolEffect::InvalidInput;
                    };
                    let Some(start) = self.anchor else {
                        return ToolEffect::InvalidInput;
                    };
                    let Some(end) = self.constrained_endpoint(pointer) else {
                        return ToolEffect::InvalidInput;
                    };
                    if start.distance(end) <= 1.0e-9 {
                        return ToolEffect::InvalidInput;
                    }
                    let chain_continues = self.active == DrawingTool::Line;
                    if chain_continues {
                        self.anchor = Some(end);
                        self.pointer = Some(end);
                    } else {
                        self.anchor = None;
                        self.pointer = None;
                    }
                    self.numeric_input.clear();
                    self.distance_focused = false;
                    return ToolEffect::CommitSegment {
                        start,
                        end,
                        chain_continues,
                    };
                }
                self.anchor = None;
                self.pointer = None;
                self.distance_focused = false;
                ToolEffect::FinishChain
            }
            ToolEvent::Escape => {
                self.active = DrawingTool::Select;
                self.clear_transient();
                ToolEffect::Cancel
            }
        }
    }

    pub fn preview(&self, pointer: Option<Vec3>) -> ToolPreview {
        let Some(start) = self.anchor else {
            return ToolPreview::default();
        };
        let Some(pointer) = pointer.or(self.pointer) else {
            return ToolPreview::default();
        };
        let Some(end) = self.constrained_endpoint(pointer) else {
            return ToolPreview::default();
        };
        let length = start.distance(end);
        if length.is_finite() && length > 0.0 {
            ToolPreview {
                segment: Some([start, end]),
                measurement: Some(length),
            }
        } else {
            ToolPreview::default()
        }
    }

    pub fn distance_focused(&self) -> bool {
        self.distance_focused
    }

    fn constrained_endpoint(&self, pointer: Vec3) -> Option<Vec3> {
        let start = self.anchor?;
        if self.numeric_input.is_empty() {
            return pointer.is_finite().then_some(pointer);
        }
        let length = self.numeric_input.parse::<f64>().ok()?;
        if !length.is_finite() || length == 0.0 {
            return None;
        }
        let direction = (pointer - start).try_normalized(1.0e-12)?;
        let endpoint = start + direction * length;
        endpoint.is_finite().then_some(endpoint)
    }

    fn clear_transient(&mut self) {
        self.anchor = None;
        self.pointer = None;
        self.numeric_input.clear();
        self.distance_focused = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DrawingTool, DrawingToolState, ToolCycle, ToolCycleEffect, ToolCycleState, ToolEffect,
        ToolEvent,
    };
    use crate::geometry::Vec3;

    #[test]
    fn line_chains_construction_line_finishes_pairs_and_escape_discards_pending_only() {
        let mut line = DrawingToolState::default();
        line.handle(ToolEvent::Activate(DrawingTool::Line));
        assert_eq!(
            ToolEffect::AnchorPlaced(Vec3::ZERO),
            line.handle(ToolEvent::Pick(Vec3::ZERO))
        );
        let first = line.handle(ToolEvent::Pick(Vec3::X));
        assert_eq!(
            ToolEffect::CommitSegment {
                start: Vec3::ZERO,
                end: Vec3::X,
                chain_continues: true,
            },
            first
        );
        assert_eq!(Some(Vec3::X), line.anchor());
        assert_eq!(ToolEffect::FinishChain, line.handle(ToolEvent::Enter));
        assert_eq!(DrawingTool::Line, line.active());
        assert_eq!(None, line.anchor());

        let mut construction = DrawingToolState::default();
        construction.handle(ToolEvent::Activate(DrawingTool::ConstructionLine));
        construction.handle(ToolEvent::Pick(Vec3::ZERO));
        assert!(matches!(
            construction.handle(ToolEvent::Pick(Vec3::X)),
            ToolEffect::CommitSegment {
                chain_continues: false,
                ..
            }
        ));
        assert_eq!(DrawingTool::ConstructionLine, construction.active());
        assert_eq!(None, construction.anchor());
        construction.handle(ToolEvent::Pick(Vec3::ZERO));
        assert_eq!(ToolEffect::Cancel, construction.handle(ToolEvent::Escape));
        assert_eq!(DrawingTool::Select, construction.active());
    }

    #[test]
    fn typed_distance_is_used_for_both_preview_and_commit() {
        let mut tool = DrawingToolState::default();
        tool.handle(ToolEvent::Activate(DrawingTool::Line));
        tool.handle(ToolEvent::Pick(Vec3::ZERO));
        tool.handle(ToolEvent::PointerMoved(Vec3::new(0.0, 0.0, 5.0)));
        tool.handle(ToolEvent::Text("2.5"));
        let preview = tool.preview(None);
        assert_eq!(
            Some([Vec3::ZERO, Vec3::new(0.0, 0.0, 2.5)]),
            preview.segment
        );
        assert_eq!(Some(2.5), preview.measurement);
        assert_eq!(
            ToolEffect::CommitSegment {
                start: Vec3::ZERO,
                end: Vec3::new(0.0, 0.0, 2.5),
                chain_continues: true,
            },
            tool.handle(ToolEvent::Enter)
        );
    }

    #[test]
    fn zero_typed_distance_has_no_preview_and_cannot_commit() {
        let mut tool = DrawingToolState::default();
        tool.handle(ToolEvent::Activate(DrawingTool::Line));
        tool.handle(ToolEvent::Pick(Vec3::ZERO));
        tool.handle(ToolEvent::PointerMoved(Vec3::X));
        tool.handle(ToolEvent::Text("0"));

        assert_eq!(None, tool.preview(None).segment);
        assert_eq!(ToolEffect::InvalidInput, tool.handle(ToolEvent::Enter));
        assert_eq!(Some(Vec3::ZERO), tool.anchor());
        assert_eq!("0", tool.numeric_input());
    }

    #[test]
    fn tool_switch_and_backspace_clear_the_right_transient_state() {
        let mut tool = DrawingToolState::default();
        tool.handle(ToolEvent::Activate(DrawingTool::Line));
        tool.handle(ToolEvent::Pick(Vec3::ZERO));
        tool.handle(ToolEvent::Text("12"));
        tool.handle(ToolEvent::Backspace);
        assert_eq!("1", tool.numeric_input());
        tool.handle(ToolEvent::Backspace);
        assert_eq!(Some(Vec3::ZERO), tool.anchor());
        tool.handle(ToolEvent::Backspace);
        assert_eq!(None, tool.anchor());
        tool.handle(ToolEvent::Pick(Vec3::ZERO));
        tool.handle(ToolEvent::Activate(DrawingTool::ConstructionLine));
        assert_eq!(None, tool.anchor());
        assert_eq!("", tool.numeric_input());
    }

    #[test]
    fn legacy_tool_cycles_preserve_order_and_hotspot_is_a_separate_action() {
        let mut cycles = ToolCycleState::default();
        assert_eq!(
            ToolCycleEffect::BeginHotspotPlacement,
            cycles.advance(ToolCycle::PlanarConstruction, "Select")
        );
        assert!(cycles.hotspot_placement_active());
        assert_eq!(
            ToolCycleEffect::ActivateTool("Construction Line"),
            cycles.advance(ToolCycle::PlanarConstruction, "Select")
        );
        assert!(!cycles.hotspot_placement_active());
        assert_eq!(
            ToolCycleEffect::ActivateTool("Line"),
            cycles.advance(ToolCycle::PlanarConstruction, "Construction Line")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Polyline"),
            cycles.advance(ToolCycle::PlanarConstruction, "Line")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Circle"),
            cycles.advance(ToolCycle::PlanarConstruction, "Quad")
        );
        assert_eq!(
            ToolCycleEffect::BeginHotspotPlacement,
            cycles.advance(ToolCycle::PlanarConstruction, "Circle")
        );

        assert_eq!(
            ToolCycleEffect::ActivateTool("Linear Dimension"),
            cycles.advance(ToolCycle::Annotation, "Select")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Text"),
            cycles.advance(ToolCycle::Annotation, "Linear Dimension")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Line Offset"),
            cycles.advance(ToolCycle::Operations, "Select")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Push/Pull"),
            cycles.advance(ToolCycle::Operations, "Line Offset")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Move"),
            cycles.advance(ToolCycle::Modification, "Select")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Rotate"),
            cycles.advance(ToolCycle::Modification, "Move")
        );
        assert_eq!(
            ToolCycleEffect::ActivateTool("Move"),
            cycles.advance(ToolCycle::Modification, "Helical Array Even")
        );
    }

    #[test]
    fn annotation_and_modification_cycles_cancel_pending_hotspot_action() {
        let mut cycles = ToolCycleState::default();
        cycles.advance(ToolCycle::PlanarConstruction, "Select");

        assert_eq!(
            ToolCycleEffect::ActivateTool("Linear Dimension"),
            cycles.advance(ToolCycle::Annotation, "Select")
        );
        assert!(!cycles.hotspot_placement_active());
        cycles.advance(ToolCycle::PlanarConstruction, "Select");
        assert_eq!(
            ToolCycleEffect::ActivateTool("Move"),
            cycles.advance(ToolCycle::Modification, "Select")
        );
        assert!(!cycles.hotspot_placement_active());
    }
}
