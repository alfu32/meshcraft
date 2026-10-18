//! Point-picking state for selection transforms.

use crate::geometry::{Affine3, GeometryTolerance, Vec3};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformTool {
    Move,
    Rotate,
    Scale,
    Stretch,
    StretchScale,
    RotateStretch,
}

impl TransformTool {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Move" => Some(Self::Move),
            "Rotate" => Some(Self::Rotate),
            "Scale" => Some(Self::Scale),
            "Stretch" => Some(Self::Stretch),
            "Stretch Scale" => Some(Self::StretchScale),
            "Rotate Stretch" => Some(Self::RotateStretch),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Stretch => "Stretch",
            Self::StretchScale => "Stretch Scale",
            Self::RotateStretch => "Rotate Stretch",
        }
    }

    pub fn required_picks(self) -> usize {
        match self {
            Self::Move | Self::Stretch => 2,
            Self::Scale | Self::StretchScale => 3,
            Self::Rotate | Self::RotateStretch => 4,
        }
    }

    pub fn supports_copy_mode(self) -> bool {
        matches!(self, Self::Move | Self::Rotate)
    }

    pub fn next_pick_prompt(self, placed: usize) -> &'static str {
        match (self, placed) {
            (Self::Move | Self::Stretch, 0) => "Pick the reference point.",
            (Self::Move | Self::Stretch, _) => "Pick the destination point.",
            (Self::Rotate | Self::RotateStretch, 0) => "Pick the rotation center.",
            (Self::Rotate | Self::RotateStretch, 1) => "Pick a point on the rotation axis.",
            (Self::Rotate | Self::RotateStretch, 2) => "Pick the reference point.",
            (Self::Rotate | Self::RotateStretch, _) => "Pick the final point.",
            (Self::Scale | Self::StretchScale, 0) => "Pick the scale center.",
            (Self::Scale | Self::StretchScale, 1) => "Pick the reference point.",
            (Self::Scale | Self::StretchScale, _) => "Pick the final point.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformCommit {
    pub transform: Affine3,
    pub copy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransformPick {
    NeedPoint { prompt: &'static str, placed: usize },
    Commit(TransformCommit),
    Invalid(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransformToolState {
    active: Option<TransformTool>,
    points: Vec<Vec3>,
    pointer: Option<Vec3>,
    numeric_input: String,
    copy_mode: bool,
}

impl TransformToolState {
    pub fn active(&self) -> Option<TransformTool> {
        self.active
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn numeric_input(&self) -> &str {
        &self.numeric_input
    }

    pub fn copy_mode(&self) -> bool {
        self.copy_mode
    }

    pub fn activate(&mut self, tool: Option<TransformTool>) {
        self.active = tool;
        self.clear_picks();
        self.copy_mode = false;
    }

    pub fn set_pointer(&mut self, point: Option<Vec3>) {
        self.pointer = point.filter(|point| point.is_finite());
    }

    pub fn toggle_copy_mode(&mut self) -> bool {
        let Some(tool) = self.active else {
            return false;
        };
        if !tool.supports_copy_mode() {
            return false;
        }
        self.copy_mode = !self.copy_mode;
        true
    }

    pub fn set_copy_mode(&mut self, enabled: bool) -> bool {
        if !self.active.is_some_and(TransformTool::supports_copy_mode) {
            return false;
        }
        if self.copy_mode == enabled {
            return false;
        }
        self.copy_mode = enabled;
        true
    }

    pub fn add_numeric_text(&mut self, text: &str) {
        if self.active.is_none() {
            return;
        }
        for character in text.chars() {
            if character.is_ascii_digit() || matches!(character, '.' | '-' | '+' | 'e' | 'E') {
                self.numeric_input.push(character);
            }
        }
    }

    pub fn backspace(&mut self) -> bool {
        if self.numeric_input.pop().is_some() {
            return true;
        }
        if self.points.pop().is_some() {
            self.pointer = None;
            return true;
        }
        false
    }

    pub fn cancel(&mut self) {
        self.clear_picks();
        self.active = None;
        self.copy_mode = false;
    }

    pub fn pick(&mut self, point: Vec3) -> TransformPick {
        let Some(tool) = self.active else {
            return TransformPick::Invalid("no transform tool is active".to_owned());
        };
        if !point.is_finite() {
            return TransformPick::Invalid("pick a finite point".to_owned());
        }
        self.points.push(point);
        if self.points.len() < tool.required_picks() {
            return TransformPick::NeedPoint {
                prompt: tool.next_pick_prompt(self.points.len()),
                placed: self.points.len(),
            };
        }
        match build_transform(tool, &self.points, None, &self.numeric_input) {
            Ok(transform) => {
                let commit = TransformCommit {
                    transform,
                    copy: self.copy_mode,
                };
                self.clear_picks();
                TransformPick::Commit(commit)
            }
            Err(error) => {
                self.clear_picks();
                TransformPick::Invalid(error)
            }
        }
    }

    pub fn finish(&mut self) -> Result<TransformCommit, String> {
        let tool = self
            .active
            .ok_or_else(|| "no transform tool is active".to_owned())?;
        let transform = build_transform(tool, &self.points, self.pointer, &self.numeric_input)?;
        let commit = TransformCommit {
            transform,
            copy: self.copy_mode,
        };
        self.clear_picks();
        Ok(commit)
    }

    pub fn preview(&self, pointer: Option<Vec3>) -> Option<Affine3> {
        let tool = self.active?;
        let pointer = pointer.or(self.pointer)?;
        build_transform(tool, &self.points, Some(pointer), &self.numeric_input).ok()
    }

    fn clear_picks(&mut self) {
        self.points.clear();
        self.pointer = None;
        self.numeric_input.clear();
    }
}

fn build_transform(
    tool: TransformTool,
    points: &[Vec3],
    pointer: Option<Vec3>,
    numeric_input: &str,
) -> Result<Affine3, String> {
    let mut picks = points.to_vec();
    if let Some(pointer) = pointer {
        picks.push(pointer);
    }
    if picks.len() != tool.required_picks() || picks.iter().any(|point| !point.is_finite()) {
        return Err("transform does not have all required pick points".to_owned());
    }
    let numeric = if numeric_input.is_empty() {
        None
    } else {
        let value = numeric_input
            .parse::<f64>()
            .map_err(|_| "enter a valid finite numeric value".to_owned())?;
        if !value.is_finite() || value == 0.0 {
            return Err("numeric transform value must be finite and nonzero".to_owned());
        }
        Some(value)
    };

    match tool {
        TransformTool::Move | TransformTool::Stretch => {
            let start = picks[0];
            let mut end = picks[1];
            if let Some(distance) = numeric {
                let direction = (end - start)
                    .try_normalized(GeometryTolerance::default().absolute)
                    .ok_or_else(|| {
                        "choose a nonzero direction before entering distance".to_owned()
                    })?;
                end = start + direction * distance;
            }
            let delta = end - start;
            if delta.length()
                <= GeometryTolerance::default().linear_at(start.length().max(end.length()))
            {
                return Err("reference and destination points must differ".to_owned());
            }
            Ok(Affine3::from_translation(delta))
        }
        TransformTool::Scale | TransformTool::StretchScale => {
            let center = picks[0];
            let reference = picks[1];
            let mut target = picks[2];
            if let Some(factor) = numeric {
                let reference_vector = reference - center;
                if reference_vector.length() <= GeometryTolerance::default().absolute {
                    return Err("scale reference must differ from its center".to_owned());
                }
                target = center + reference_vector * factor;
            }
            scale_from_points(center, reference, target)
        }
        TransformTool::Rotate | TransformTool::RotateStretch => {
            let center = picks[0];
            let axis = (picks[1] - center)
                .try_normalized(GeometryTolerance::default().absolute)
                .ok_or_else(|| "rotation axis point must differ from its center".to_owned())?;
            let reference = picks[2] - center;
            let target = picks[3] - center;
            let reference_plane = reference - axis * reference.dot(axis);
            let target_plane = target - axis * target.dot(axis);
            if reference_plane.length() <= GeometryTolerance::default().absolute
                || target_plane.length() <= GeometryTolerance::default().absolute
            {
                return Err("rotation reference points must be off the axis".to_owned());
            }
            let angle = if let Some(degrees) = numeric {
                degrees.to_radians()
            } else {
                axis.dot(reference_plane.cross(target_plane))
                    .atan2(reference_plane.dot(target_plane))
            };
            if !angle.is_finite() || angle.abs() <= 1.0e-12 {
                return Err("rotation angle must be nonzero".to_owned());
            }
            rotation_about(center, axis, angle)
        }
    }
}

fn scale_from_points(center: Vec3, reference: Vec3, target: Vec3) -> Result<Affine3, String> {
    let reference_vector = reference - center;
    let target_vector = target - center;
    let reference_length = reference_vector.length();
    let target_length = target_vector.length();
    if reference_length <= GeometryTolerance::default().absolute
        || target_length <= GeometryTolerance::default().absolute
    {
        return Err("scale vectors must have nonzero length".to_owned());
    }
    let constraint = scale_constraint(reference_vector, target_vector);
    let factor = match constraint {
        ScaleConstraint::AxisX => component_scale(reference_vector.x, target_vector.x)?,
        ScaleConstraint::AxisY => component_scale(reference_vector.y, target_vector.y)?,
        ScaleConstraint::AxisZ => component_scale(reference_vector.z, target_vector.z)?,
        _ => target_length / reference_length,
    };
    let factors = match constraint {
        ScaleConstraint::Uniform => Vec3::new(factor, factor, factor),
        ScaleConstraint::PlaneX => Vec3::new(1.0, factor, factor),
        ScaleConstraint::PlaneY => Vec3::new(factor, 1.0, factor),
        ScaleConstraint::PlaneZ => Vec3::new(factor, factor, 1.0),
        ScaleConstraint::AxisX => Vec3::new(factor, 1.0, 1.0),
        ScaleConstraint::AxisY => Vec3::new(1.0, factor, 1.0),
        ScaleConstraint::AxisZ => Vec3::new(1.0, 1.0, factor),
    };
    scale_about(center, factors)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScaleConstraint {
    Uniform,
    PlaneX,
    PlaneY,
    PlaneZ,
    AxisX,
    AxisY,
    AxisZ,
}

fn scale_constraint(reference: Vec3, target: Vec3) -> ScaleConstraint {
    let length = reference.length();
    let tolerance = length * 0.03;
    let zero_x = reference.x.abs() <= tolerance;
    let zero_y = reference.y.abs() <= tolerance;
    let zero_z = reference.z.abs() <= tolerance;
    let zero_count = [zero_x, zero_y, zero_z]
        .into_iter()
        .filter(|value| *value)
        .count();
    if zero_count >= 2 {
        return if !zero_x {
            ScaleConstraint::AxisX
        } else if !zero_y {
            ScaleConstraint::AxisY
        } else {
            ScaleConstraint::AxisZ
        };
    }
    if zero_count == 1 {
        return if zero_x {
            ScaleConstraint::PlaneX
        } else if zero_y {
            ScaleConstraint::PlaneY
        } else {
            ScaleConstraint::PlaneZ
        };
    }
    let normal = reference
        .cross(target)
        .try_normalized(GeometryTolerance::default().absolute);
    if let Some(normal) = normal {
        let components = [normal.x.abs(), normal.y.abs(), normal.z.abs()];
        let largest = components.iter().copied().fold(0.0, f64::max);
        if largest >= 0.98 {
            if components[0] == largest {
                return ScaleConstraint::PlaneX;
            }
            if components[1] == largest {
                return ScaleConstraint::PlaneY;
            }
            return ScaleConstraint::PlaneZ;
        }
    }
    ScaleConstraint::Uniform
}

fn component_scale(reference: f64, target: f64) -> Result<f64, String> {
    if reference.abs() <= GeometryTolerance::default().absolute {
        return Err("scale axis component is too small".to_owned());
    }
    let ratio = target / reference;
    let factor = if ratio < 0.0 {
        ratio.min(-0.1)
    } else {
        ratio.max(0.1)
    };
    factor
        .is_finite()
        .then_some(factor)
        .ok_or_else(|| "scale factor must be finite".to_owned())
}

fn scale_about(center: Vec3, factors: Vec3) -> Result<Affine3, String> {
    if !center.is_finite()
        || !factors.is_finite()
        || factors.x == 0.0
        || factors.y == 0.0
        || factors.z == 0.0
    {
        return Err("scale factors must be finite and nonzero".to_owned());
    }
    let linear = [
        [factors.x, 0.0, 0.0],
        [0.0, factors.y, 0.0],
        [0.0, 0.0, factors.z],
    ];
    let scaled_center = Vec3::new(
        factors.x * center.x,
        factors.y * center.y,
        factors.z * center.z,
    );
    Ok(Affine3::from_linear_translation(
        linear,
        center - scaled_center,
    ))
}

fn rotation_about(center: Vec3, axis: Vec3, angle: f64) -> Result<Affine3, String> {
    if !center.is_finite() || !angle.is_finite() {
        return Err("rotation must use finite coordinates and angle".to_owned());
    }
    let axis = axis
        .try_normalized(GeometryTolerance::default().absolute)
        .ok_or_else(|| "rotation axis must be nonzero".to_owned())?;
    let (sin, cos) = angle.sin_cos();
    let one_minus_cos = 1.0 - cos;
    let (x, y, z) = (axis.x, axis.y, axis.z);
    let linear = [
        [
            cos + x * x * one_minus_cos,
            x * y * one_minus_cos - z * sin,
            x * z * one_minus_cos + y * sin,
        ],
        [
            y * x * one_minus_cos + z * sin,
            cos + y * y * one_minus_cos,
            y * z * one_minus_cos - x * sin,
        ],
        [
            z * x * one_minus_cos - y * sin,
            z * y * one_minus_cos + x * sin,
            cos + z * z * one_minus_cos,
        ],
    ];
    let rotated_center =
        Affine3::from_linear_translation(linear, Vec3::ZERO).transform_vector(center);
    Ok(Affine3::from_linear_translation(
        linear,
        center - rotated_center,
    ))
}

#[cfg(test)]
mod tests {
    use super::{TransformPick, TransformTool, TransformToolState};
    use crate::geometry::Vec3;

    #[test]
    fn move_preview_and_commit_derive_from_picks_without_mutating_source_points() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Move));
        assert!(matches!(
            state.pick(Vec3::ZERO),
            TransformPick::NeedPoint { placed: 1, .. }
        ));
        let source = state.points().to_vec();
        let preview = state.preview(Some(Vec3::new(3.0, 0.0, 0.0))).unwrap();
        assert_eq!(
            Vec3::new(3.0, 0.0, 0.0),
            preview.transform_point(Vec3::ZERO)
        );
        assert_eq!(source, state.points());
        assert!(matches!(
            state.pick(Vec3::new(3.0, 0.0, 0.0)),
            TransformPick::Commit(_)
        ));
        assert!(state.points().is_empty());
    }

    #[test]
    fn move_numeric_distance_and_copy_mode_are_supported_only_when_characterized() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Move));
        assert!(state.toggle_copy_mode());
        assert!(state.copy_mode());
        state.pick(Vec3::ZERO);
        state.add_numeric_text("2.5");
        state.set_pointer(Some(Vec3::new(0.0, 0.0, 1.0)));
        let commit = state.finish().unwrap();
        assert!(commit.copy);
        assert_eq!(Vec3::new(0.0, 0.0, 2.5), commit.transform.translation);
        state.activate(Some(TransformTool::Scale));
        assert!(!state.toggle_copy_mode());
    }

    #[test]
    fn rotate_uses_four_points_and_numeric_angle() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Rotate));
        state.pick(Vec3::ZERO);
        state.pick(Vec3::Y);
        state.pick(Vec3::X);
        state.add_numeric_text("-90");
        let preview = state.preview(Some(Vec3::Z)).unwrap();
        let rotated = preview.transform_point(Vec3::X);
        assert!(rotated.distance(Vec3::Z) < 1.0e-9);
    }

    #[test]
    fn scale_axis_constraint_preserves_perpendicular_components() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Scale));
        state.pick(Vec3::ZERO);
        state.pick(Vec3::new(2.0, 0.0, 0.0));
        let commit = state.pick(Vec3::new(4.0, 0.0, 0.0));
        let TransformPick::Commit(commit) = commit else {
            panic!("scale should commit on the third pick");
        };
        assert_eq!(
            Vec3::new(4.0, 2.0, 3.0),
            commit.transform.transform_point(Vec3::new(2.0, 2.0, 3.0))
        );
    }

    #[test]
    fn cancel_discards_pending_points_and_tool_switch_resets_copy_mode() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Move));
        state.toggle_copy_mode();
        state.pick(Vec3::ZERO);
        state.cancel();
        assert_eq!(None, state.active());
        assert!(state.points().is_empty());
        state.activate(Some(TransformTool::Rotate));
        assert!(!state.copy_mode());
    }
}
