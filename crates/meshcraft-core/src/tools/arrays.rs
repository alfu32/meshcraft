//! Ordered point input for legacy linear, planar and volumetric arrays.

use crate::geometry::{Affine3, Vec3};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotationalArrayKind {
    Planar,
    Helicoidal,
    RotationalArray,
    HelicalArray,
}

impl RotationalArrayKind {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Planar Rotate Multiple" => Some(Self::Planar),
            "Helicoidal Rotate Multiple" => Some(Self::Helicoidal),
            "Rotational Array" => Some(Self::RotationalArray),
            "Helical Array" => Some(Self::HelicalArray),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Planar => "Planar Rotate Multiple",
            Self::Helicoidal => "Helicoidal Rotate Multiple",
            Self::RotationalArray => "Rotational Array",
            Self::HelicalArray => "Helical Array",
        }
    }

    fn helicoidal(self) -> bool {
        self == Self::Helicoidal
    }

    fn transforms(
        self,
        center: Vec3,
        reference: Vec3,
        increment: Vec3,
        extent: Vec3,
        limit: usize,
    ) -> (Vec<Affine3>, bool) {
        if self == Self::HelicalArray {
            helical_array_transforms(center, reference, increment, extent, limit)
        } else {
            rotational_array_transforms(
                center,
                reference,
                increment,
                extent,
                self.helicoidal(),
                limit,
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RotationalArrayPick {
    NeedReference,
    NeedIncrement,
    NeedExtent,
    Complete {
        transforms: Vec<Affine3>,
        capped: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RotationalArrayToolState {
    kind: Option<RotationalArrayKind>,
    points: Vec<Vec3>,
}

impl RotationalArrayToolState {
    pub fn activate(&mut self, kind: Option<RotationalArrayKind>) {
        self.kind = kind;
        self.points.clear();
    }

    pub fn cancel(&mut self) {
        self.activate(None);
    }

    pub fn kind(&self) -> Option<RotationalArrayKind> {
        self.kind
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn active(&self) -> bool {
        self.kind.is_some()
    }

    pub fn backspace(&mut self) -> bool {
        self.points.pop().is_some()
    }

    pub fn pick(&mut self, point: Vec3) -> Result<RotationalArrayPick, &'static str> {
        if !point.is_finite() {
            return Err("array points must be finite");
        }
        let Some(kind) = self.kind else {
            return Err("no rotational array tool is active");
        };
        match self.points.len() {
            0 => {
                self.points.push(point);
                Ok(RotationalArrayPick::NeedReference)
            }
            1 => {
                self.points.push(point);
                Ok(RotationalArrayPick::NeedIncrement)
            }
            2 => {
                self.points.push(point);
                Ok(RotationalArrayPick::NeedExtent)
            }
            _ => {
                let (transforms, capped) = kind.transforms(
                    self.points[0],
                    self.points[1],
                    self.points[2],
                    point,
                    crate::operations::copy::MAX_MULTIPLE_ARRAY_COPIES,
                );
                self.points.clear();
                Ok(RotationalArrayPick::Complete { transforms, capped })
            }
        }
    }

    pub fn preview_transforms(&self, point: Vec3) -> Option<(Vec<Affine3>, bool)> {
        let kind = self.kind?;
        if self.points.len() != 3 || !point.is_finite() {
            return None;
        }
        Some(kind.transforms(
            self.points[0],
            self.points[1],
            self.points[2],
            point,
            MAX_MULTIPLE_ARRAY_PREVIEW_COPIES,
        ))
    }
}

/// Resolves fixed-step planar or helicoidal rotations from center/reference/increment/extent picks.
pub fn rotational_array_transforms(
    center: Vec3,
    reference: Vec3,
    increment: Vec3,
    extent: Vec3,
    helicoidal: bool,
    limit: usize,
) -> (Vec<Affine3>, bool) {
    if !center.is_finite()
        || !reference.is_finite()
        || !increment.is_finite()
        || !extent.is_finite()
    {
        return (Vec::new(), false);
    }
    let reference_vector = reference - center;
    let increment_vector = increment - center;
    let extent_vector = extent - center;
    let axis = reference_vector
        .cross(increment_vector)
        .try_normalized(ARRAY_ROTATION_EPSILON);
    let Some(axis) = axis else {
        return (Vec::new(), false);
    };
    let reference_projected = project_to_rotation_plane(reference_vector, axis);
    let increment_projected = project_to_rotation_plane(increment_vector, axis);
    let extent_projected = project_to_rotation_plane(extent_vector, axis);
    if reference_projected.length_squared() <= ARRAY_ROTATION_EPSILON
        || increment_projected.length_squared() <= ARRAY_ROTATION_EPSILON
        || extent_projected.length_squared() <= ARRAY_ROTATION_EPSILON
    {
        return (Vec::new(), false);
    }
    let step_angle = signed_angle(reference_projected, increment_projected, axis);
    if step_angle.abs() <= 1.0e-4 {
        return (Vec::new(), false);
    }
    let direction = step_angle.signum();
    let raw_sweep = signed_angle(reference_projected, extent_projected, axis);
    let mut sweep = normalize_sweep(raw_sweep, direction);
    if sweep.abs() <= 1.0e-4 {
        sweep = std::f64::consts::TAU * direction;
    }
    if sweep * direction <= 0.0 {
        return (Vec::new(), false);
    }
    let raw_count = (sweep.abs() / step_angle.abs()).floor();
    if !raw_count.is_finite() || raw_count < 1.0 {
        return (Vec::new(), false);
    }
    let capped = raw_count > limit as f64;
    let mut copies = raw_count.min(limit as f64) as usize;
    let total_lift = if helicoidal {
        extent_vector.dot(axis) - reference_vector.dot(axis)
    } else {
        0.0
    };
    let lift_per_step = if helicoidal && copies > 0 {
        total_lift / copies as f64
    } else {
        0.0
    };
    if (sweep.abs() - std::f64::consts::TAU).abs() <= 1.0e-3
        && lift_per_step.abs() <= 1.0e-6
        && copies > 1
    {
        copies -= 1;
    }
    let transforms = (1..=copies)
        .map(|index| {
            rotation_about_axis(
                center,
                axis,
                step_angle * index as f64,
                lift_per_step * index as f64,
            )
        })
        .collect();
    (transforms, capped)
}

/// Resolves the legacy Y-axis fixed-step helical array from center, reference, unit, and end.
pub fn helical_array_transforms(
    center: Vec3,
    reference: Vec3,
    unit: Vec3,
    extent: Vec3,
    limit: usize,
) -> (Vec<Affine3>, bool) {
    if !center.is_finite() || !reference.is_finite() || !unit.is_finite() || !extent.is_finite() {
        return (Vec::new(), false);
    }
    let axis = Vec3::Y;
    let reference_vector = reference - center;
    let unit_vector = unit - center;
    let extent_vector = extent - center;
    let reference_projected = project_to_rotation_plane(reference_vector, axis);
    let unit_projected = project_to_rotation_plane(unit_vector, axis);
    let extent_projected = project_to_rotation_plane(extent_vector, axis);
    if reference_projected.length_squared() <= ARRAY_ROTATION_EPSILON
        || unit_projected.length_squared() <= ARRAY_ROTATION_EPSILON
        || extent_projected.length_squared() <= ARRAY_ROTATION_EPSILON
    {
        return (Vec::new(), false);
    }
    let step_angle = signed_angle(reference_projected, unit_projected, axis);
    if step_angle.abs() <= 1.0e-4 {
        return (Vec::new(), false);
    }
    let step_lift = unit.y - reference.y;
    let raw_sweep = signed_angle(reference_projected, extent_projected, axis);
    let raw_copies = if step_lift.abs() <= 1.0e-4 {
        let direction = step_angle.signum();
        let sweep = normalize_sweep(raw_sweep, direction);
        let mut copies = (sweep.abs() / step_angle.abs()).floor();
        if (sweep.abs() - std::f64::consts::TAU).abs() <= 1.0e-3 && copies > 1.0 {
            copies -= 1.0;
        }
        copies
    } else {
        let height_estimate = (extent.y - reference.y) / step_lift;
        if !height_estimate.is_finite() || height_estimate <= 0.0 {
            return (Vec::new(), false);
        }
        let approximate_sweep = (step_angle * height_estimate).abs();
        let estimated_turns = if approximate_sweep.is_finite() {
            (approximate_sweep / std::f64::consts::TAU)
                .floor()
                .min(limit as f64) as usize
        } else {
            limit
        };
        let max_turns = estimated_turns.saturating_add(8).min(limit);
        let direction = step_angle.signum();
        let mut best_copies = 0.0;
        let mut best_score = f64::INFINITY;
        for turns in 0..=max_turns {
            let mut candidate_sweep = raw_sweep + std::f64::consts::TAU * direction * turns as f64;
            if candidate_sweep * direction <= 0.0 {
                candidate_sweep += std::f64::consts::TAU * direction;
            }
            if candidate_sweep * direction <= 0.0 {
                continue;
            }
            let candidate_count = candidate_sweep / step_angle;
            if candidate_count <= 0.0 || !candidate_count.is_finite() {
                continue;
            }
            let rounded_count = candidate_count.round().max(1.0);
            let score = (rounded_count - height_estimate).abs()
                + (candidate_count - rounded_count).abs() * 0.25;
            if score < best_score {
                best_score = score;
                best_copies = rounded_count;
            }
        }
        if best_copies == 0.0 {
            height_estimate.round().max(1.0)
        } else {
            best_copies
        }
    };
    let capped = raw_copies > limit as f64;
    let copies = raw_copies.min(limit as f64) as usize;
    if copies == 0 {
        return (Vec::new(), capped);
    }
    let transforms = (1..=copies)
        .map(|index| {
            rotation_about_axis(
                center,
                axis,
                step_angle * index as f64,
                step_lift * index as f64,
            )
        })
        .collect();
    (transforms, capped)
}

const ARRAY_ROTATION_EPSILON: f64 = 1.0e-5;

fn project_to_rotation_plane(vector: Vec3, normal: Vec3) -> Vec3 {
    vector - normal * vector.dot(normal)
}

fn signed_angle(from: Vec3, to: Vec3, axis: Vec3) -> f64 {
    axis.dot(from.cross(to)).atan2(from.dot(to))
}

fn normalize_sweep(angle: f64, direction: f64) -> f64 {
    if direction > 0.0 {
        if angle <= 0.0 {
            angle + std::f64::consts::TAU
        } else {
            angle
        }
    } else if angle >= 0.0 {
        angle - std::f64::consts::TAU
    } else {
        angle
    }
}

fn rotation_about_axis(center: Vec3, axis: Vec3, radians: f64, lift: f64) -> Affine3 {
    let (sin, cos) = radians.sin_cos();
    let one_minus_cos = 1.0 - cos;
    let [x, y, z] = [axis.x, axis.y, axis.z];
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
    let linear_transform = Affine3::from_linear_translation(linear, Vec3::ZERO);
    let translation = center - linear_transform.transform_vector(center) + axis * lift;
    Affine3::from_linear_translation(linear, translation)
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
    use super::{
        ArrayToolKind, ArrayToolPick, ArrayToolState, RotationalArrayKind, RotationalArrayPick,
        RotationalArrayToolState,
    };
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

    #[test]
    fn planar_rotation_omits_the_duplicate_source_on_full_circle() {
        let mut state = RotationalArrayToolState::default();
        state.activate(Some(RotationalArrayKind::Planar));
        for point in [Vec3::ZERO, Vec3::X, Vec3::Y] {
            state.pick(point).unwrap();
        }
        let RotationalArrayPick::Complete { transforms, capped } = state.pick(Vec3::X).unwrap()
        else {
            panic!("fourth pick completes the rotational array")
        };

        assert!(!capped);
        assert_eq!(3, transforms.len());
        let quarter_turn = transforms[0].transform_point(Vec3::X);
        assert!(quarter_turn.distance(Vec3::Y) < 1.0e-9);
        let last_turn = transforms[2].transform_point(Vec3::X);
        assert!(last_turn.distance(Vec3::new(0.0, -1.0, 0.0)) < 1.0e-9);
    }

    #[test]
    fn helicoidal_rotation_distributes_axial_lift_across_the_sweep() {
        let (transforms, capped) = super::rotational_array_transforms(
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::new(-1.0, 0.0, 4.0),
            true,
            2_048,
        );

        assert!(!capped);
        assert_eq!(2, transforms.len());
        assert!(
            transforms[0]
                .transform_point(Vec3::X)
                .distance(Vec3::new(0.0, 1.0, 2.0))
                < 1.0e-9
        );
        assert!(
            transforms[1]
                .transform_point(Vec3::X)
                .distance(Vec3::new(-1.0, 0.0, 4.0))
                < 1.0e-9
        );
    }

    #[test]
    fn helical_array_uses_the_world_y_axis_and_the_measured_unit_lift() {
        let (transforms, capped) = super::helical_array_transforms(
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(0.0, 2.0, -1.0),
            Vec3::new(-1.0, 4.0, 0.0),
            2_048,
        );

        assert!(!capped);
        assert_eq!(2, transforms.len());
        assert!(
            transforms[0]
                .transform_point(Vec3::X)
                .distance(Vec3::new(0.0, 2.0, -1.0))
                < 1.0e-9
        );
        assert!(
            transforms[1]
                .transform_point(Vec3::X)
                .distance(Vec3::new(-1.0, 4.0, 0.0))
                < 1.0e-9
        );
    }
}
