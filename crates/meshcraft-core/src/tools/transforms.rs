//! Point-picking state for selection transforms.

use crate::geometry::{Affine3, GeometryTolerance, Vec3};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformTool {
    Move,
    Rotate,
    Rotate2,
    Scale,
    Squash,
    Stretch,
    StretchScale,
    RotateStretch,
}

impl TransformTool {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Move" => Some(Self::Move),
            "Rotate" => Some(Self::Rotate),
            "Rotate 2" => Some(Self::Rotate2),
            "Scale" => Some(Self::Scale),
            "Squash" => Some(Self::Squash),
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
            Self::Rotate2 => "Rotate 2",
            Self::Scale => "Scale",
            Self::Squash => "Squash",
            Self::Stretch => "Stretch",
            Self::StretchScale => "Stretch Scale",
            Self::RotateStretch => "Rotate Stretch",
        }
    }

    pub fn required_picks(self) -> usize {
        match self {
            Self::Move | Self::Stretch => 2,
            Self::Rotate2 => 3,
            Self::Scale | Self::Squash | Self::StretchScale => 3,
            Self::Rotate | Self::RotateStretch => 4,
        }
    }

    pub fn supports_copy_mode(self) -> bool {
        matches!(
            self,
            Self::Move | Self::Rotate | Self::Rotate2 | Self::Scale | Self::Squash
        )
    }

    pub fn next_pick_prompt(self, placed: usize) -> &'static str {
        match (self, placed) {
            (Self::Move | Self::Stretch, 0) => "Pick the reference point.",
            (Self::Move | Self::Stretch, _) => "Pick the destination point.",
            (Self::Rotate | Self::RotateStretch, 0) => "Pick the rotation center.",
            (Self::Rotate | Self::RotateStretch, 1) => "Pick a point on the rotation axis.",
            (Self::Rotate | Self::RotateStretch, 2) => "Pick the reference point.",
            (Self::Rotate | Self::RotateStretch, _) => "Pick the final point.",
            (Self::Rotate2, 0) => "Pick the rotation center (C).",
            (Self::Rotate2, 1) => "Pick the first reference point (A).",
            (Self::Rotate2, _) => "Pick the end angle point (B).",
            (Self::Squash, 0) => "Pick the squash center.",
            (Self::Squash, 1) => "Pick the reference point defining the squash axis.",
            (Self::Squash, _) => "Pick the final axis distance (or enter a signed scale factor).",
            (Self::Scale | Self::StretchScale, 0) => "Pick the scale center.",
            (Self::Scale | Self::StretchScale, 1) => "Pick the reference point.",
            (Self::Scale | Self::StretchScale, _) => "Pick the final point.",
        }
    }
}

/// Center-relative construction rays and the projected rotation angle.
#[derive(Clone, Debug)]
pub struct RotationFeedback {
    pub center: Vec3,
    pub vectors: Vec<Vec3>,
    pub arc: Vec<[Vec3; 2]>,
    pub angle_degrees: Option<f64>,
    pub pair_angles: Vec<(usize, usize, f64)>,
}
impl RotationFeedback {
    pub fn edges(&self) -> Vec<[Vec3; 2]> {
        let mut edges = vec![[self.center, self.center]];
        for v in &self.vectors {
            let p = self.center + *v;
            edges.extend([[self.center, p], [p, p]]);
        }
        edges.extend(self.arc.iter().copied());
        edges
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
    pub fn clear_numeric_text(&mut self) {
        self.numeric_input.clear();
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
    pub fn divided_preview(&self, pointer: Vec3, fraction: f64) -> Result<Affine3, String> {
        build_transform_fraction(
            self.active.ok_or("No transform active")?,
            &self.points,
            Some(pointer),
            &self.numeric_input,
            fraction,
        )
    }

    pub fn rotation_feedback(&self, pointer: Vec3) -> Option<RotationFeedback> {
        let tool = self.active?;
        if !matches!(
            tool,
            TransformTool::Rotate | TransformTool::Rotate2 | TransformTool::RotateStretch
        ) {
            return None;
        }
        let center = *self.points.first()?;
        let mut vectors = self
            .points
            .iter()
            .skip(1)
            .map(|p| *p - center)
            .collect::<Vec<_>>();
        vectors.push(pointer - center);
        let mut pair_angles = vec![];
        for i in 0..vectors.len() {
            for j in i + 1..vectors.len() {
                if let (Some(a), Some(b)) = (
                    vectors[i].try_normalized(1e-12),
                    vectors[j].try_normalized(1e-12),
                ) {
                    pair_angles.push((i + 1, j + 1, a.dot(b).clamp(-1., 1.).acos().to_degrees()));
                }
            }
        }
        let rotation = if tool == TransformTool::Rotate2 && vectors.len() == 2 {
            vectors[0]
                .cross(vectors[1])
                .try_normalized(1e-12)
                .map(|axis| (axis, vectors[0], vectors[1]))
        } else if vectors.len() == 3 {
            vectors[0]
                .try_normalized(1e-12)
                .map(|axis| (axis, vectors[1], vectors[2]))
        } else {
            None
        };
        let mut arc = vec![];
        let mut angle_degrees = None;
        if let Some((axis, a, b)) = rotation {
            let a = a - axis * a.dot(axis);
            let b = b - axis * b.dot(axis);
            if let Some(unit) = a.try_normalized(1e-12)
                && b.length() > 1e-12
            {
                let measured = axis.dot(a.cross(b)).atan2(a.dot(b));
                let angle = self
                    .numeric_input
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(f64::to_radians)
                    .unwrap_or(measured);
                angle_degrees = Some(angle.to_degrees());
                let radius = a.length().min(b.length()) * 0.6;
                let segments = (angle.to_degrees().abs() / 5.).ceil().clamp(1., 72.) as usize;
                let point = |i: usize| {
                    let t = angle * i as f64 / segments as f64;
                    center + (unit * t.cos() + axis.cross(unit) * t.sin()) * radius
                };
                arc.extend((0..segments).map(|i| [point(i), point(i + 1)]));
            }
        }
        Some(RotationFeedback {
            center,
            vectors,
            arc,
            angle_degrees,
            pair_angles,
        })
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
    build_transform_fraction(tool, points, pointer, numeric_input, 1.)
}
fn build_transform_fraction(
    tool: TransformTool,
    points: &[Vec3],
    pointer: Option<Vec3>,
    numeric_input: &str,
    fraction: f64,
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
            Ok(Affine3::from_translation(delta * fraction))
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
            scale_from_points(center, reference, target).map(|t| interpolate_scale(t, fraction))
        }
        TransformTool::Squash => {
            let center = picks[0];
            let reference = picks[1] - center;
            let axis = reference
                .try_normalized(GeometryTolerance::default().absolute)
                .ok_or("Squash reference must differ from its center.")?;
            let factor = numeric.unwrap_or((picks[2] - center).dot(axis) / reference.length());
            if !factor.is_finite() || factor.abs() <= 1e-9 {
                return Err("Squash factor must be finite and nonzero.".into());
            }
            let u = [axis.x, axis.y, axis.z];
            let linear = std::array::from_fn(|i| {
                std::array::from_fn(|j| {
                    (if i == j { 1. } else { 0. }) + (factor - 1.) * u[i] * u[j]
                })
            });
            let transform = Affine3::from_linear_translation(linear, Vec3::ZERO);
            Ok(interpolate_scale(
                Affine3::from_linear_translation(
                    linear,
                    center - transform.transform_vector(center),
                ),
                fraction,
            ))
        }
        TransformTool::Rotate2 => {
            let center = picks[0];
            let reference = picks[1] - center;
            let target = picks[2] - center;
            if reference.length() <= GeometryTolerance::default().absolute
                || target.length() <= GeometryTolerance::default().absolute
            {
                return Err("Rotate 2 reference points must differ from the center".to_owned());
            }
            let axis = reference
                .cross(target)
                .try_normalized(GeometryTolerance::default().absolute)
                .ok_or_else(|| {
                    "Rotate 2 center and reference points must not be collinear".to_owned()
                })?;
            let reference_plane = reference - axis * reference.dot(axis);
            let target_plane = target - axis * target.dot(axis);
            let angle = if let Some(degrees) = numeric {
                degrees.to_radians()
            } else {
                axis.dot(reference_plane.cross(target_plane))
                    .atan2(reference_plane.dot(target_plane))
            };
            if !angle.is_finite() || angle.abs() <= 1.0e-12 {
                return Err("Rotate 2 angle must be nonzero".to_owned());
            }
            rotation_about(center, axis, angle * fraction)
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
            rotation_about(center, axis, angle * fraction)
        }
    }
}
fn interpolate_scale(t: Affine3, fraction: f64) -> Affine3 {
    Affine3::from_linear_translation(
        std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                let identity = if i == j { 1. } else { 0. };
                identity + (t.linear[i][j] - identity) * fraction
            })
        }),
        t.translation * fraction,
    )
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
    use super::{TransformPick, TransformTool, TransformToolState, build_transform};
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
    fn numeric_distance_and_copy_mode_support_move_and_scale_but_not_stretch() {
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
        assert!(state.toggle_copy_mode());
        state.activate(Some(TransformTool::Stretch));
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
    fn rotate_2_uses_three_points_to_define_its_rotation_axis() {
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Rotate2));
        assert!(state.toggle_copy_mode());
        assert!(matches!(
            state.pick(Vec3::ZERO),
            TransformPick::NeedPoint { placed: 1, .. }
        ));
        assert!(matches!(
            state.pick(Vec3::X),
            TransformPick::NeedPoint { placed: 2, .. }
        ));
        let TransformPick::Commit(commit) = state.pick(Vec3::Y) else {
            panic!("Rotate 2 commits on its third point")
        };

        assert!(commit.copy);
        assert!(commit.transform.transform_point(Vec3::X).distance(Vec3::Y) < 1.0e-9);
        assert!(state.points().is_empty());
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
    #[test]
    fn squash_uses_an_arbitrary_axis_and_preserves_perpendicular_components() {
        let center = Vec3::new(3., 4., 5.);
        let axis = Vec3::new(1., 1., 0.).try_normalized(1e-12).unwrap();
        let sideways = Vec3::new(1., -1., 2.);
        let t = build_transform(
            TransformTool::Squash,
            &[center, center + axis * 4., center + axis * 2. + sideways],
            None,
            "",
        )
        .unwrap();
        assert!(t.transform_point(center).distance(center) < 1e-12);
        assert!(t.transform_vector(sideways).distance(sideways) < 1e-12);
        assert!(
            t.transform_point(center + axis * 4. + sideways)
                .distance(center + axis * 2. + sideways)
                < 1e-12
        );
        let mirrored = build_transform(
            TransformTool::Squash,
            &[center, center + axis],
            Some(center + axis),
            "-2",
        )
        .unwrap();
        assert!(mirrored.transform_vector(axis).distance(axis * -2.) < 1e-12);
        assert!(
            build_transform(
                TransformTool::Squash,
                &[center, center + axis, center],
                None,
                ""
            )
            .is_err()
        );
    }
    #[test]
    fn rotation_feedback_keeps_all_rays_on_the_center_and_measures_projected_angle() {
        let center = Vec3::new(7., 8., 9.);
        let mut state = TransformToolState::default();
        state.activate(Some(TransformTool::Rotate));
        for p in [center, center + Vec3::Y * 2., center + Vec3::X + Vec3::Y] {
            state.pick(p);
        }
        let feedback = state
            .rotation_feedback(center + Vec3::Z + Vec3::Y * 3.)
            .unwrap();
        assert_eq!(feedback.center, center);
        assert_eq!(feedback.vectors.len(), 3);
        assert!((feedback.angle_degrees.unwrap() + 90.).abs() < 1e-9);
        for p in state
            .points()
            .iter()
            .skip(1)
            .copied()
            .chain([center + Vec3::Z + Vec3::Y * 3.])
        {
            assert!(feedback.edges().contains(&[center, p]));
        }
        assert!(!feedback.arc.is_empty());
        assert!(
            feedback
                .arc
                .iter()
                .flatten()
                .all(|p| (p.y - center.y).abs() < 1e-12)
        );
    }
}
