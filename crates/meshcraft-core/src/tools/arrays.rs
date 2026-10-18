//! Ordered point input for legacy linear, planar and volumetric arrays.

use crate::geometry::Vec3;
use crate::operations::copy::{
    MAX_MULTIPLE_ARRAY_PREVIEW_COPIES, copy_multiple_offsets, planar_translation_offsets_limited,
    volumetric_translation_offsets_limited,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrayToolKind {
    CopyMultiple,
    Planar,
    Volumetric,
}

impl ArrayToolKind {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Copy Multiple" => Some(Self::CopyMultiple),
            "Planar Translate Multiple" => Some(Self::Planar),
            "Volumetric Translate Multiple" => Some(Self::Volumetric),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CopyMultiple => "Copy Multiple",
            Self::Planar => "Planar Translate Multiple",
            Self::Volumetric => "Volumetric Translate Multiple",
        }
    }

    fn offsets(self, origin: Vec3, measure: Vec3, extent: Vec3, limit: usize) -> (Vec<Vec3>, bool) {
        match self {
            Self::CopyMultiple => copy_multiple_offsets(origin, measure, extent, limit),
            Self::Planar => planar_translation_offsets_limited(origin, measure, extent, limit),
            Self::Volumetric => {
                volumetric_translation_offsets_limited(origin, measure, extent, limit)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrayToolPick {
    NeedMeasure,
    NeedExtent,
    Complete { offsets: Vec<Vec3>, capped: bool },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrayToolState {
    kind: Option<ArrayToolKind>,
    points: Vec<Vec3>,
}

impl ArrayToolState {
    pub fn activate(&mut self, kind: Option<ArrayToolKind>) {
        self.kind = kind;
        self.points.clear();
    }

    pub fn cancel(&mut self) {
        self.activate(None);
    }

    pub fn kind(&self) -> Option<ArrayToolKind> {
        self.kind
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn backspace(&mut self) -> bool {
        self.points.pop().is_some()
    }

    pub fn active(&self) -> bool {
        self.kind.is_some()
    }

    pub fn pick(&mut self, point: Vec3) -> Result<ArrayToolPick, &'static str> {
        if !point.is_finite() {
            return Err("array points must be finite");
        }
        let Some(kind) = self.kind else {
            return Err("no array tool is active");
        };
        match self.points.len() {
            0 => {
                self.points.push(point);
                Ok(ArrayToolPick::NeedMeasure)
            }
            1 => {
                self.points.push(point);
                Ok(ArrayToolPick::NeedExtent)
            }
            _ => {
                let origin = self.points[0];
                let measure = self.points[1];
                let (offsets, capped) = kind.offsets(
                    origin,
                    measure,
                    point,
                    crate::operations::copy::MAX_MULTIPLE_ARRAY_COPIES,
                );
                self.points.clear();
                Ok(ArrayToolPick::Complete { offsets, capped })
            }
        }
    }

    pub fn preview_offsets(&self, point: Vec3) -> Option<(Vec<Vec3>, bool)> {
        let kind = self.kind?;
        if self.points.len() != 2 || !point.is_finite() {
            return None;
        }
        Some(kind.offsets(
            self.points[0],
            self.points[1],
            point,
            MAX_MULTIPLE_ARRAY_PREVIEW_COPIES,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ArrayToolKind, ArrayToolPick, ArrayToolState};
    use crate::geometry::Vec3;

    #[test]
    fn planar_tool_advances_three_picks_and_reports_generated_offsets() {
        let mut state = ArrayToolState::default();
        state.activate(Some(ArrayToolKind::Planar));

        assert_eq!(ArrayToolPick::NeedMeasure, state.pick(Vec3::ZERO).unwrap());
        assert_eq!(
            ArrayToolPick::NeedExtent,
            state.pick(Vec3::new(2.0, 8.0, 3.0)).unwrap()
        );
        let ArrayToolPick::Complete { offsets, capped } =
            state.pick(Vec3::new(4.0, 0.0, 6.0)).unwrap()
        else {
            panic!("third pick completes the array")
        };

        assert!(!capped);
        assert_eq!(8, offsets.len());
        assert!(state.points().is_empty());
        assert!(state.active());
    }

    #[test]
    fn volumetric_preview_uses_current_pointer_before_commit() {
        let mut state = ArrayToolState::default();
        state.activate(Some(ArrayToolKind::Volumetric));
        state.pick(Vec3::ZERO).unwrap();
        state.pick(Vec3::new(1.0, 2.0, 3.0)).unwrap();

        let (preview, capped) = state.preview_offsets(Vec3::new(1.0, 4.0, 3.0)).unwrap();

        assert!(!capped);
        assert_eq!(11, preview.len());
        assert!(preview.contains(&Vec3::new(0.0, 2.0, 0.0)));
        assert!(preview.contains(&Vec3::new(1.0, 2.0, 0.0)));
    }

    #[test]
    fn copy_multiple_tool_previews_copies_that_fit_along_the_measured_vector() {
        let mut state = ArrayToolState::default();
        state.activate(Some(ArrayToolKind::CopyMultiple));
        state.pick(Vec3::ZERO).unwrap();
        state.pick(Vec3::X * 2.0).unwrap();

        let (preview, capped) = state.preview_offsets(Vec3::new(5.5, 0.0, 3.0)).unwrap();

        assert!(!capped);
        assert_eq!(vec![Vec3::X * 2.0, Vec3::X * 4.0], preview);
    }

    #[test]
    fn cancel_clears_picks_and_deactivates_tool() {
        let mut state = ArrayToolState::default();
        state.activate(Some(ArrayToolKind::Planar));
        state.pick(Vec3::ZERO).unwrap();
        state.cancel();

        assert!(!state.active());
        assert!(state.points().is_empty());
    }

    #[test]
    fn backspace_removes_only_the_latest_array_input() {
        let mut state = ArrayToolState::default();
        state.activate(Some(ArrayToolKind::Planar));
        state.pick(Vec3::ZERO).unwrap();
        state.pick(Vec3::X).unwrap();

        assert!(state.backspace());
        assert_eq!(&[Vec3::ZERO], state.points());
        assert!(state.backspace());
        assert!(state.points().is_empty());
        assert!(!state.backspace());
        assert!(state.active());
    }
}
