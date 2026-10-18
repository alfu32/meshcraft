//! Staged point-picking state for built-in solid primitives.

use crate::geometry::Vec3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveKind {
    Sphere,
    Cylinder,
    Cone,
    Pill,
}

impl PrimitiveKind {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Sphere" => Some(Self::Sphere),
            "Cylinder" => Some(Self::Cylinder),
            "Cone" => Some(Self::Cone),
            "Pill" => Some(Self::Pill),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sphere => "Sphere",
            Self::Cylinder => "Cylinder",
            Self::Cone => "Cone",
            Self::Pill => "Pill",
        }
    }

    pub fn pick_count(self) -> usize {
        match self {
            Self::Sphere => 2,
            Self::Cylinder | Self::Cone | Self::Pill => 3,
        }
    }

    pub fn next_pick_prompt(self, picks: usize) -> &'static str {
        match (self, picks) {
            (Self::Sphere, 0) => "Click sphere center.",
            (Self::Sphere, _) => "Click sphere radius point.",
            (Self::Cylinder, 0) => "Click cylinder base center.",
            (Self::Cylinder, 1) => "Click cylinder radius point.",
            (Self::Cylinder, _) => "Click cylinder height point.",
            (Self::Cone, 0) => "Click cone base center.",
            (Self::Cone, 1) => "Click cone radius point.",
            (Self::Cone, _) => "Click cone height point.",
            (Self::Pill, 0) => "Click pill base center.",
            (Self::Pill, 1) => "Click pill radius point.",
            (Self::Pill, _) => "Click pill top hemisphere center.",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrimitiveToolState {
    active: Option<PrimitiveKind>,
    picks: Vec<Vec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PrimitivePick {
    NeedPoint { prompt: &'static str },
    Complete(Vec<Vec3>),
}

impl PrimitiveToolState {
    pub fn active(&self) -> Option<PrimitiveKind> {
        self.active
    }

    pub fn picks(&self) -> &[Vec3] {
        &self.picks
    }

    pub fn activate(&mut self, kind: Option<PrimitiveKind>) {
        self.active = kind;
        self.picks.clear();
    }

    pub fn cancel(&mut self) {
        self.activate(None);
    }

    pub fn pick(&mut self, point: Vec3) -> Result<PrimitivePick, &'static str> {
        let Some(kind) = self.active else {
            return Err("No primitive tool is active.");
        };
        if !point.is_finite() {
            return Err("Primitive points must be finite.");
        }
        self.picks.push(point);
        if self.picks.len() < kind.pick_count() {
            return Ok(PrimitivePick::NeedPoint {
                prompt: kind.next_pick_prompt(self.picks.len()),
            });
        }
        let picks = std::mem::take(&mut self.picks);
        Ok(PrimitivePick::Complete(picks))
    }

    pub fn preview_picks(&self, pointer: Vec3) -> Option<Vec<Vec3>> {
        let kind = self.active?;
        if !pointer.is_finite() || self.picks.is_empty() {
            return None;
        }
        let mut picks = self.picks.clone();
        picks.push(pointer);
        (picks.len() <= kind.pick_count()).then_some(picks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_tools_follow_legacy_pick_stages_and_reset_after_completion() {
        for (kind, expected) in [
            (
                PrimitiveKind::Sphere,
                vec!["Click sphere center.", "Click sphere radius point."],
            ),
            (
                PrimitiveKind::Cylinder,
                vec![
                    "Click cylinder base center.",
                    "Click cylinder radius point.",
                    "Click cylinder height point.",
                ],
            ),
            (
                PrimitiveKind::Cone,
                vec![
                    "Click cone base center.",
                    "Click cone radius point.",
                    "Click cone height point.",
                ],
            ),
            (
                PrimitiveKind::Pill,
                vec![
                    "Click pill base center.",
                    "Click pill radius point.",
                    "Click pill top hemisphere center.",
                ],
            ),
        ] {
            let mut state = PrimitiveToolState::default();
            state.activate(Some(kind));
            for (index, prompt) in expected.iter().enumerate() {
                assert_eq!(kind.next_pick_prompt(index), *prompt);
                let point = Vec3::new(index as f64, 0.0, 0.0);
                let result = state.pick(point).unwrap();
                if index + 1 == expected.len() {
                    assert_eq!(
                        PrimitivePick::Complete(
                            (0..=index).map(|i| Vec3::new(i as f64, 0.0, 0.0)).collect()
                        ),
                        result
                    );
                    assert!(state.picks().is_empty());
                } else {
                    assert!(matches!(result, PrimitivePick::NeedPoint { .. }));
                }
            }
        }
    }

    #[test]
    fn escape_clears_primitive_picks_without_deactivating_reusable_tool_state() {
        let mut state = PrimitiveToolState::default();
        state.activate(Some(PrimitiveKind::Sphere));
        state.pick(Vec3::ZERO).unwrap();
        state.activate(Some(PrimitiveKind::Sphere));
        assert!(state.picks().is_empty());
        assert_eq!(Some(PrimitiveKind::Sphere), state.active());
        state.cancel();
        assert_eq!(None, state.active());
    }
}
