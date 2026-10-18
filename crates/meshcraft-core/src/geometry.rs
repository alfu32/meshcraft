use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

pub mod polygon;

/// `DraftLineStore.epsilon`; do not reuse as a face or screen-space tolerance.
pub const LEGACY_LINE_STORE_EPSILON: f64 = 1.0e-3;
/// `DraftFaceStore.epsilon`, distinct from its projected cut tolerance.
pub const LEGACY_FACE_STORE_EPSILON: f64 = 1.0e-4;
/// `HotspotStore.keyEps`, used only when forming quantized attachment keys.
pub const LEGACY_HOTSPOT_KEY_STEP: f64 = 1.0e-3;
/// `Snapper.epsilon`, used for ray/plane and parameter tie checks.
pub const LEGACY_SNAP_INTERNAL_EPSILON: f64 = 1.0e-1;
/// `planeBasisFromNormal` reference-axis branch threshold from the legacy UI.
pub const LEGACY_PLANE_BASIS_Y_THRESHOLD: f64 = 0.9;
/// Scale applied when projecting faces to the legacy clipping library.
pub const LEGACY_FACE_PROJECTION_SCALE: f64 = 10_000.0;
/// `DraftFaceStore.cutEps2d`, in projected face coordinates.
pub const LEGACY_FACE_CUT_2D_EPSILON: f64 = 1.0e-2;
/// `MeshIo.quantizeCoordinate` step for indexed mesh output vertices.
pub const LEGACY_MESH_VERTEX_KEY_STEP: f64 = 1.0e-5;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum QuantizationTieRule {
    /// Matches Kotlin `round`: halfway values go to the nearest even integer.
    ToEven,
    /// Matches Kotlin `roundToInt`: halfway values go toward positive infinity.
    TowardPositiveInfinity,
    /// Rounds halfway values away from zero.
    AwayFromZero,
}

/// Quantizes one coordinate using a named step and explicit tie behavior.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct QuantizationPolicy {
    pub step: f64,
    pub tie_rule: QuantizationTieRule,
}

impl QuantizationPolicy {
    pub const fn new(step: f64, tie_rule: QuantizationTieRule) -> Self {
        Self { step, tie_rule }
    }

    pub fn quantize(self, value: f64) -> Option<i64> {
        if !value.is_finite() || !self.step.is_finite() || self.step <= 0.0 {
            return None;
        }
        let scaled = value / self.step;
        if !scaled.is_finite() {
            return None;
        }
        let rounded = match self.tie_rule {
            QuantizationTieRule::ToEven => scaled.round_ties_even(),
            QuantizationTieRule::TowardPositiveInfinity => (scaled + 0.5).floor(),
            QuantizationTieRule::AwayFromZero => scaled.round(),
        };
        // `i64::MAX as f64` rounds up to 2^63, so use an exclusive upper bound.
        if rounded < i64::MIN as f64 || rounded >= -(i64::MIN as f64) {
            return None;
        }
        Some(rounded as i64)
    }
}

pub const LEGACY_LINE_KEY_QUANTIZATION: QuantizationPolicy =
    QuantizationPolicy::new(LEGACY_LINE_STORE_EPSILON, QuantizationTieRule::ToEven);
pub const LEGACY_FACE_KEY_QUANTIZATION: QuantizationPolicy =
    QuantizationPolicy::new(LEGACY_FACE_STORE_EPSILON, QuantizationTieRule::ToEven);
pub const LEGACY_HOTSPOT_KEY_QUANTIZATION: QuantizationPolicy = QuantizationPolicy::new(
    LEGACY_HOTSPOT_KEY_STEP,
    QuantizationTieRule::TowardPositiveInfinity,
);
pub const LEGACY_MESH_VERTEX_KEY_QUANTIZATION: QuantizationPolicy =
    QuantizationPolicy::new(LEGACY_MESH_VERTEX_KEY_STEP, QuantizationTieRule::ToEven);

/// Converts model coordinates to physical output units. One model unit is
/// `meters_per_model_unit` meters; it does not rescale stored geometry.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModelUnitScale {
    pub meters_per_model_unit: f64,
}

impl ModelUnitScale {
    pub fn new(meters_per_model_unit: f64) -> Option<Self> {
        let scale = Self {
            meters_per_model_unit,
        };
        scale.is_valid().then_some(scale)
    }

    pub fn is_valid(self) -> bool {
        self.meters_per_model_unit.is_finite() && self.meters_per_model_unit > 0.0
    }

    pub fn displayed_length(self, model_length: f64) -> Option<f64> {
        if !self.is_valid() || !model_length.is_finite() {
            return None;
        }
        let value = model_length * self.meters_per_model_unit;
        value.is_finite().then_some(value)
    }

    pub fn model_length_from_meters(self, meters: f64) -> Option<f64> {
        if !self.is_valid() || !meters.is_finite() {
            return None;
        }
        let value = meters / self.meters_per_model_unit;
        value.is_finite().then_some(value)
    }

    /// Returns the output-coordinate scale; `None` means a unitless format.
    pub fn export_coordinate_scale(self, meters_per_output_unit: Option<f64>) -> Option<f64> {
        if !self.is_valid() {
            return None;
        }
        let value = match meters_per_output_unit {
            Some(output_unit) if output_unit.is_finite() && output_unit > 0.0 => {
                self.meters_per_model_unit / output_unit
            }
            Some(_) => return None,
            None => self.meters_per_model_unit,
        };
        value.is_finite().then_some(value)
    }

    /// Number of model units in one millimeter, as expected by the legacy 3MF importer.
    pub fn model_units_per_millimeter(self) -> Option<f64> {
        if !self.is_valid() {
            return None;
        }
        let value = 0.001 / self.meters_per_model_unit;
        value.is_finite().then_some(value)
    }
}

/// Absolute and relative tolerances used for geometric predicates.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeometryTolerance {
    pub absolute: f64,
    pub relative: f64,
}

impl Default for GeometryTolerance {
    fn default() -> Self {
        Self {
            absolute: 1.0e-9,
            relative: 1.0e-12,
        }
    }
}

impl GeometryTolerance {
    pub fn new(absolute: f64, relative: f64) -> Option<Self> {
        (absolute.is_finite() && absolute >= 0.0 && relative.is_finite() && relative >= 0.0)
            .then_some(Self { absolute, relative })
    }

    pub fn is_valid(self) -> bool {
        self.absolute.is_finite()
            && self.absolute >= 0.0
            && self.relative.is_finite()
            && self.relative >= 0.0
    }

    /// Returns the linear tolerance at a characteristic model scale.
    pub fn linear_at(self, scale: f64) -> f64 {
        self.absolute.max(self.relative * scale.abs())
    }
}

/// A two-dimensional vector or point in model-space coordinates.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }

    pub fn cross(self, rhs: Self) -> f64 {
        self.x * rhs.y - self.y * rhs.x
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn try_normalized(self, minimum_length: f64) -> Option<Self> {
        if !self.is_finite() || !minimum_length.is_finite() || minimum_length < 0.0 {
            return None;
        }
        let scale = self.x.abs().max(self.y.abs());
        if scale == 0.0 {
            return None;
        }
        let scaled = Self::new(self.x / scale, self.y / scale);
        let length = scaled.length();
        (scale > minimum_length / length).then(|| scaled * (1.0 / length))
    }

    pub fn distance(self, rhs: Self) -> f64 {
        (self - rhs).length()
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        self * -1.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Segment2 {
    pub start: Vec2,
    pub end: Vec2,
}

impl Segment2 {
    pub const fn new(start: Vec2, end: Vec2) -> Self {
        Self { start, end }
    }

    pub fn length(self) -> f64 {
        (self.end - self.start).length()
    }

    pub fn point_at(self, parameter: f64) -> Vec2 {
        self.start + (self.end - self.start) * parameter
    }

    pub fn intersect(
        self,
        other: Self,
        tolerance: GeometryTolerance,
    ) -> Option<SegmentIntersection2> {
        if !tolerance.is_valid()
            || !self.start.is_finite()
            || !self.end.is_finite()
            || !other.start.is_finite()
            || !other.end.is_finite()
        {
            return None;
        }

        let first_direction = self.end - self.start;
        let second_direction = other.end - other.start;
        let first_length = first_direction.length();
        let second_length = second_direction.length();
        let epsilon = tolerance.linear_at(first_length.max(second_length));
        if !first_length.is_finite() || !second_length.is_finite() || !epsilon.is_finite() {
            return None;
        }

        if first_length <= epsilon {
            if second_length <= epsilon {
                return Some(if self.start.distance(other.start) <= epsilon {
                    SegmentIntersection2::Point {
                        position: (self.start + other.start) * 0.5,
                        first_parameter: 0.0,
                        second_parameter: 0.0,
                    }
                } else {
                    SegmentIntersection2::Disjoint
                });
            }
            return Some(match other.parameter_of_point(self.start, epsilon)? {
                Some(parameter) => SegmentIntersection2::Point {
                    position: self.start,
                    first_parameter: 0.0,
                    second_parameter: parameter,
                },
                None => SegmentIntersection2::Disjoint,
            });
        }
        if second_length <= epsilon {
            return Some(match self.parameter_of_point(other.start, epsilon)? {
                Some(parameter) => SegmentIntersection2::Point {
                    position: other.start,
                    first_parameter: parameter,
                    second_parameter: 0.0,
                },
                None => SegmentIntersection2::Disjoint,
            });
        }

        let first_axis = first_direction / first_length;
        let second_axis = second_direction / second_length;
        let between_origins = other.start - self.start;
        if !between_origins.is_finite() {
            return None;
        }
        let axis_cross = first_axis.cross(second_axis);
        let angle_tolerance = (epsilon / first_length.min(second_length)).min(1.0);

        if axis_cross.abs() <= angle_tolerance {
            if between_origins.cross(first_axis).abs() > epsilon {
                return Some(SegmentIntersection2::Disjoint);
            }

            let other_start_distance = between_origins.dot(first_axis);
            let other_end_distance = (other.end - self.start).dot(first_axis);
            if !other_start_distance.is_finite() || !other_end_distance.is_finite() {
                return None;
            }
            let low = 0.0_f64.max(other_start_distance.min(other_end_distance));
            let high = first_length.min(other_start_distance.max(other_end_distance));
            if high < low - epsilon {
                return Some(SegmentIntersection2::Disjoint);
            }

            let first_distance = ((low + high) * 0.5).clamp(0.0, first_length);
            let position = self.start + first_axis * first_distance;
            let first_parameter = first_distance / first_length;
            let second_parameter =
                ((position - other.start).dot(second_axis) / second_length).clamp(0.0, 1.0);
            if high - low <= epsilon {
                return Some(SegmentIntersection2::Point {
                    position,
                    first_parameter,
                    second_parameter,
                });
            }
            let overlap_start = self.start + first_axis * low;
            let overlap_end = self.start + first_axis * high;
            return Some(SegmentIntersection2::Overlap {
                start: overlap_start,
                end: overlap_end,
            });
        }

        let first_parameter = between_origins.cross(second_axis) / (axis_cross * first_length);
        let second_parameter = between_origins.cross(first_axis) / (axis_cross * second_length);
        if !first_parameter.is_finite() || !second_parameter.is_finite() {
            return None;
        }
        let first_slack = epsilon / first_length;
        let second_slack = epsilon / second_length;
        if first_parameter < -first_slack
            || first_parameter > 1.0 + first_slack
            || second_parameter < -second_slack
            || second_parameter > 1.0 + second_slack
        {
            return Some(SegmentIntersection2::Disjoint);
        }

        let first_parameter = first_parameter.clamp(0.0, 1.0);
        let second_parameter = second_parameter.clamp(0.0, 1.0);
        let first_position = self.point_at(first_parameter);
        let second_position = other.point_at(second_parameter);
        Some(SegmentIntersection2::Point {
            position: (first_position + second_position) * 0.5,
            first_parameter,
            second_parameter,
        })
    }

    fn parameter_of_point(self, point: Vec2, epsilon: f64) -> Option<Option<f64>> {
        let direction = self.end - self.start;
        let length = direction.length();
        if length <= epsilon {
            return Some((point.distance(self.start) <= epsilon).then_some(0.0));
        }
        let axis = direction / length;
        let relative = point - self.start;
        if !relative.is_finite() {
            return None;
        }
        let along = relative.dot(axis);
        let perpendicular = relative.cross(axis).abs();
        if perpendicular > epsilon || along < -epsilon || along > length + epsilon {
            return Some(None);
        }
        Some(Some((along / length).clamp(0.0, 1.0)))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum SegmentIntersection2 {
    Disjoint,
    Point {
        position: Vec2,
        first_parameter: f64,
        second_parameter: f64,
    },
    Overlap {
        start: Vec2,
        end: Vec2,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Triangle3 {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
}

impl Triangle3 {
    pub const fn new(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self { a, b, c }
    }

    pub fn normal(self, tolerance: GeometryTolerance) -> Option<Vec3> {
        let (ab, ac, scale, epsilon) = self.scaled_edges(tolerance)?;
        ab.cross(ac).try_normalized(epsilon / scale)
    }

    pub fn area(self, tolerance: GeometryTolerance) -> Option<f64> {
        let (ab, ac, scale, epsilon) = self.scaled_edges(tolerance)?;
        let normalized_cross_length = ab.cross(ac).length();
        if normalized_cross_length <= epsilon / scale {
            return None;
        }
        let area = scale * (normalized_cross_length * 0.5) * scale;
        (area.is_finite() && area > 0.0).then_some(area)
    }

    /// Returns barycentric weights `(a, b, c)` for a point in the triangle plane.
    /// Weights outside `[0, 1]` are retained for projection and edge queries.
    pub fn barycentric_coordinates(
        self,
        point: Vec3,
        tolerance: GeometryTolerance,
    ) -> Option<[f64; 3]> {
        if !point.is_finite() {
            return None;
        }
        let (ab, ac, scale, epsilon) = self.scaled_edges(tolerance)?;
        let relative = (point - self.a) / scale;
        if !relative.is_finite() {
            return None;
        }
        let denominator = ab.cross(ac).length_squared();
        let area_tolerance = (epsilon / scale).powi(2);
        if !denominator.is_finite() || denominator <= area_tolerance {
            return None;
        }
        let d00 = ab.dot(ab);
        let d01 = ab.dot(ac);
        let d11 = ac.dot(ac);
        let d20 = relative.dot(ab);
        let d21 = relative.dot(ac);
        let b = (d11 * d20 - d01 * d21) / denominator;
        let c = (d00 * d21 - d01 * d20) / denominator;
        let a = 1.0 - b - c;
        [a, b, c]
            .into_iter()
            .all(f64::is_finite)
            .then_some([a, b, c])
    }

    pub fn contains_point(self, point: Vec3, tolerance: GeometryTolerance) -> Option<bool> {
        let weights = self.barycentric_coordinates(point, tolerance)?;
        let (ab, ac, scale, epsilon) = self.scaled_edges(tolerance)?;
        let normal = ab.cross(ac).try_normalized(epsilon / scale)?;
        let relative = (point - self.a) / scale;
        let plane_distance = relative.dot(normal).abs();
        let min_edge = ab.length().min(ac.length()).min((ac - ab).length());
        let slack = (epsilon / scale) / min_edge;
        Some(
            plane_distance <= epsilon / scale
                && weights
                    .iter()
                    .all(|weight| *weight >= -slack && *weight <= 1.0 + slack),
        )
    }

    fn scaled_edges(self, tolerance: GeometryTolerance) -> Option<(Vec3, Vec3, f64, f64)> {
        if !tolerance.is_valid()
            || !self.a.is_finite()
            || !self.b.is_finite()
            || !self.c.is_finite()
        {
            return None;
        }
        let ab = self.b - self.a;
        let ac = self.c - self.a;
        let bc = self.c - self.b;
        if !ab.is_finite() || !ac.is_finite() || !bc.is_finite() {
            return None;
        }
        let scale = ab.length().max(ac.length()).max(bc.length());
        let epsilon = tolerance.linear_at(scale);
        if !scale.is_finite() || !epsilon.is_finite() || scale <= epsilon {
            return None;
        }
        Some((ab / scale, ac / scale, scale, epsilon))
    }
}

/// A three-dimensional vector or point in model units.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y).hypot(self.z)
    }

    pub fn try_normalized(self, minimum_length: f64) -> Option<Self> {
        if !self.is_finite() || !minimum_length.is_finite() || minimum_length < 0.0 {
            return None;
        }
        let scale = self.x.abs().max(self.y.abs()).max(self.z.abs());
        if scale == 0.0 {
            return None;
        }
        let scaled = Self::new(self.x / scale, self.y / scale, self.z / scale);
        let length = scaled.length();
        (scale > minimum_length / length).then(|| scaled * (1.0 / length))
    }

    pub fn distance(self, rhs: Self) -> f64 {
        (self - rhs).length()
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        self * -1.0
    }
}

/// A normalized plane equation `normal · (point - origin) = 0`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Plane3 {
    pub origin: Vec3,
    pub normal: Vec3,
}

impl Plane3 {
    pub fn from_normal(origin: Vec3, normal: Vec3, tolerance: GeometryTolerance) -> Option<Self> {
        if !origin.is_finite() || !tolerance.is_valid() {
            return None;
        }
        Some(Self {
            origin,
            normal: normal.try_normalized(tolerance.absolute)?,
        })
    }

    pub fn signed_distance(self, point: Vec3) -> f64 {
        self.normal.dot(point - self.origin)
    }

    pub fn project(self, point: Vec3) -> Vec3 {
        point - self.normal * self.signed_distance(point)
    }
}

/// A ray with a normalized direction and a nonnegative parameter range.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Ray3 {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray3 {
    pub fn from_direction(
        origin: Vec3,
        direction: Vec3,
        tolerance: GeometryTolerance,
    ) -> Option<Self> {
        if !origin.is_finite() || !tolerance.is_valid() {
            return None;
        }
        Some(Self {
            origin,
            direction: direction.try_normalized(tolerance.absolute)?,
        })
    }

    pub fn at(self, distance: f64) -> Vec3 {
        self.origin + self.direction * distance
    }
}

/// Column-vector affine transform. Composition `a * b` applies `b` first.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Affine3 {
    pub linear: [[f64; 3]; 3],
    pub translation: Vec3,
}

impl Default for Affine3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine3 {
    pub const IDENTITY: Self = Self {
        linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: Vec3::ZERO,
    };

    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub const fn from_linear_translation(linear: [[f64; 3]; 3], translation: Vec3) -> Self {
        Self {
            linear,
            translation,
        }
    }

    pub fn is_finite(self) -> bool {
        self.translation.is_finite() && self.linear.into_iter().flatten().all(f64::is_finite)
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.transform_vector(point) + self.translation
    }

    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        Vec3::new(
            self.linear[0][0] * vector.x
                + self.linear[0][1] * vector.y
                + self.linear[0][2] * vector.z,
            self.linear[1][0] * vector.x
                + self.linear[1][1] * vector.y
                + self.linear[1][2] * vector.z,
            self.linear[2][0] * vector.x
                + self.linear[2][1] * vector.y
                + self.linear[2][2] * vector.z,
        )
    }

    /// Transforms normals by the inverse transpose, then renormalizes them.
    pub fn transform_normal(self, normal: Vec3, tolerance: GeometryTolerance) -> Option<Vec3> {
        let inverse_transpose = self.inverse(tolerance)?.transpose();
        inverse_transpose
            .transform_vector(normal)
            .try_normalized(tolerance.absolute)
    }

    pub fn determinant(self) -> f64 {
        let m = self.linear;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    pub fn inverse(self, tolerance: GeometryTolerance) -> Option<Self> {
        if !self.is_finite() || !tolerance.is_valid() {
            return None;
        }
        let m = self.linear;
        let scale = m.into_iter().flatten().map(f64::abs).fold(0.0, f64::max);
        if scale == 0.0 {
            return None;
        }
        // Normalize before taking the determinant so uniformly tiny or huge
        // transforms do not underflow or overflow during inversion.
        let normalized = m.map(|row| row.map(|value| value / scale));
        let determinant = determinant_3x3(normalized);
        let singularity_tolerance = tolerance.relative.max(f64::EPSILON * 8.0);
        if !determinant.is_finite() || determinant.abs() <= singularity_tolerance {
            return None;
        }
        let inverse_determinant = 1.0 / determinant;
        let inverse = [
            [
                normalized[1][1] * normalized[2][2] - normalized[1][2] * normalized[2][1],
                normalized[0][2] * normalized[2][1] - normalized[0][1] * normalized[2][2],
                normalized[0][1] * normalized[1][2] - normalized[0][2] * normalized[1][1],
            ],
            [
                normalized[1][2] * normalized[2][0] - normalized[1][0] * normalized[2][2],
                normalized[0][0] * normalized[2][2] - normalized[0][2] * normalized[2][0],
                normalized[0][2] * normalized[1][0] - normalized[0][0] * normalized[1][2],
            ],
            [
                normalized[1][0] * normalized[2][1] - normalized[1][1] * normalized[2][0],
                normalized[0][1] * normalized[2][0] - normalized[0][0] * normalized[2][1],
                normalized[0][0] * normalized[1][1] - normalized[0][1] * normalized[1][0],
            ],
        ]
        .map(|row| row.map(|value| value * inverse_determinant / scale));
        let inverse = Self::from_linear_translation(inverse, Vec3::ZERO);
        let result = Self::from_linear_translation(
            inverse.linear,
            -inverse.transform_vector(self.translation),
        );
        result.is_finite().then_some(result)
    }

    pub fn transpose(self) -> Self {
        Self::from_linear_translation(
            [
                [self.linear[0][0], self.linear[1][0], self.linear[2][0]],
                [self.linear[0][1], self.linear[1][1], self.linear[2][1]],
                [self.linear[0][2], self.linear[1][2], self.linear[2][2]],
            ],
            Vec3::ZERO,
        )
    }

    pub fn compose(self, rhs: Self) -> Self {
        let mut linear = [[0.0; 3]; 3];
        for (row, output_row) in linear.iter_mut().enumerate() {
            for (column, output) in output_row.iter_mut().enumerate() {
                *output = (0..3)
                    .map(|index| self.linear[row][index] * rhs.linear[index][column])
                    .sum();
            }
        }
        Self::from_linear_translation(
            linear,
            self.transform_vector(rhs.translation) + self.translation,
        )
    }
}

fn determinant_3x3(matrix: [[f64; 3]; 3]) -> f64 {
    matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
}

/// An axis-aligned bounding box. `None` represents an empty point set.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Aabb3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb3 {
    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        if !first.is_finite() {
            return None;
        }
        let mut bounds = Self {
            min: first,
            max: first,
        };
        for point in points {
            if !point.is_finite() {
                return None;
            }
            bounds.min.x = bounds.min.x.min(point.x);
            bounds.min.y = bounds.min.y.min(point.y);
            bounds.min.z = bounds.min.z.min(point.z);
            bounds.max.x = bounds.max.x.max(point.x);
            bounds.max.y = bounds.max.y.max(point.y);
            bounds.max.z = bounds.max.z.max(point.z);
        }
        Some(bounds)
    }

    pub fn include(&mut self, point: Vec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
        true
    }
}

/// Constructs a deterministic, right-handed orthonormal frame for a plane.
pub fn plane_basis_from_normal(
    normal: Vec3,
    tolerance: GeometryTolerance,
) -> Option<(Vec3, Vec3, Vec3)> {
    if !tolerance.is_valid() {
        return None;
    }
    let normal = normal.try_normalized(tolerance.absolute)?;
    // Keep the legacy basis orientation: use Y except near the Y axis, then X.
    // Rotating this choice changes every projected polygon coordinate.
    let reference = if normal.y.abs() < LEGACY_PLANE_BASIS_Y_THRESHOLD {
        Vec3::Y
    } else {
        Vec3::X
    };
    let u = normal.cross(reference).try_normalized(tolerance.absolute)?;
    let v = normal.cross(u).try_normalized(tolerance.absolute)?;
    Some((u, v, normal))
}

#[cfg(test)]
mod tests {
    use super::{
        Affine3, GeometryTolerance, LEGACY_FACE_KEY_QUANTIZATION, LEGACY_HOTSPOT_KEY_QUANTIZATION,
        LEGACY_LINE_KEY_QUANTIZATION, ModelUnitScale, Plane3, QuantizationPolicy,
        QuantizationTieRule, Ray3, Segment2, SegmentIntersection2, Triangle3, Vec2, Vec3,
        plane_basis_from_normal,
    };

    fn assert_vec_near(actual: Vec3, expected: Vec3, tolerance: f64) {
        assert!(
            actual.distance(expected) <= tolerance,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn affine_points_vectors_and_normals_have_distinct_transforms() {
        let transform = Affine3::from_linear_translation(
            [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.5]],
            Vec3::new(7.0, -3.0, 2.0),
        );
        assert_vec_near(
            transform.transform_point(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(9.0, -1.0, 3.5),
            1.0e-12,
        );
        assert_vec_near(
            transform.transform_vector(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(2.0, 2.0, 1.5),
            1.0e-12,
        );
        let normal = transform
            .transform_normal(Vec3::new(1.0, 1.0, 0.0), GeometryTolerance::default())
            .unwrap();
        assert_vec_near(
            normal,
            Vec3::new(1.0, 2.0, 0.0) * (1.0 / 5.0_f64.sqrt()),
            1.0e-12,
        );
    }

    #[test]
    fn affine_inverse_round_trips_points_and_composition_order() {
        let parent = Affine3::from_translation(Vec3::new(3.0, 0.0, 0.0));
        let child = Affine3::from_linear_translation(
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            Vec3::new(2.0, 0.0, 0.0),
        );
        let point = Vec3::new(1.0, 0.0, 0.0);
        let world = parent.compose(child);
        assert_vec_near(
            world.transform_point(point),
            Vec3::new(5.0, 1.0, 0.0),
            1.0e-12,
        );
        assert_vec_near(
            world
                .inverse(GeometryTolerance::default())
                .unwrap()
                .transform_point(world.transform_point(point)),
            point,
            1.0e-12,
        );
    }

    #[test]
    fn nested_nonuniform_frames_round_trip_at_large_world_offsets() {
        let tolerance = GeometryTolerance::default();
        let parent = Affine3::from_linear_translation(
            [[0.0, -2.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.0, 1.5]],
            Vec3::new(1.0e9, -2.0e9, 3.0e9),
        );
        let child = Affine3::from_linear_translation(
            [[1.25, 0.0, 0.0], [0.0, 0.0, -0.5], [0.0, 0.75, 0.0]],
            Vec3::new(0.003, -0.002, 0.004),
        );
        let grandchild = Affine3::from_linear_translation(
            [[0.0, 0.0, 0.4], [0.0, 0.75, 0.0], [-1.5, 0.0, 0.0]],
            Vec3::new(-0.001, 0.002, 0.003),
        );
        let world = parent.compose(child).compose(grandchild);
        let local = Vec3::new(0.001, -0.002, 0.003);
        let world_point = world.transform_point(local);
        let round_trip = world
            .inverse(tolerance)
            .unwrap()
            .transform_point(world_point);

        assert_vec_near(round_trip, local, 2.0e-6);
        assert!(world_point.x > 1.0e8 && world_point.y < -1.0e8);
        assert!(world_point.z > 1.0e8);
    }

    #[test]
    fn affine_inverse_rejects_singular_matrices() {
        let singular = Affine3::from_linear_translation(
            [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            Vec3::ZERO,
        );
        assert!(singular.inverse(GeometryTolerance::default()).is_none());
        assert!(
            singular
                .transform_normal(Vec3::Y, GeometryTolerance::default())
                .is_none()
        );
    }

    #[test]
    fn primitive_constructors_and_queries_reject_nonfinite_inputs() {
        let tolerance = GeometryTolerance::default();
        assert!(Plane3::from_normal(Vec3::new(f64::NAN, 0.0, 0.0), Vec3::Z, tolerance).is_none());
        assert!(
            Plane3::from_normal(
                Vec3::ZERO,
                Vec3::Z,
                GeometryTolerance {
                    absolute: 1.0e-9,
                    relative: f64::INFINITY,
                }
            )
            .is_none()
        );
        assert!(
            Ray3::from_direction(Vec3::ZERO, Vec3::new(f64::INFINITY, 0.0, 0.0), tolerance)
                .is_none()
        );
        assert!(
            Segment2::new(Vec2::ZERO, Vec2::new(f64::NAN, 1.0))
                .intersect(Segment2::new(Vec2::ZERO, Vec2::new(1.0, 0.0)), tolerance)
                .is_none()
        );
        assert!(
            Triangle3::new(Vec3::ZERO, Vec3::X, Vec3::new(f64::NAN, 1.0, 0.0))
                .area(tolerance)
                .is_none()
        );
        let nonfinite_transform = Affine3::from_translation(Vec3::new(f64::NAN, 0.0, 0.0));
        assert!(nonfinite_transform.inverse(tolerance).is_none());
    }

    #[test]
    fn plane_projection_and_basis_are_stable_and_right_handed() {
        let normal = Vec3::new(1.0, 2.0, 3.0);
        let plane = Plane3::from_normal(
            Vec3::new(4.0, 0.0, -1.0),
            normal,
            GeometryTolerance::default(),
        )
        .unwrap();
        let point = Vec3::new(5.0, 8.0, 9.0);
        assert!(plane.signed_distance(plane.project(point)).abs() < 1.0e-12);
        let (u, v, n) = plane_basis_from_normal(normal, GeometryTolerance::default()).unwrap();
        assert!((u.length() - 1.0).abs() < 1.0e-12);
        assert!((v.length() - 1.0).abs() < 1.0e-12);
        assert!((u.dot(v)).abs() < 1.0e-12);
        assert_vec_near(u.cross(v), n, 1.0e-12);
        assert!(plane_basis_from_normal(Vec3::ZERO, GeometryTolerance::default()).is_none());

        let (z_axis_u, z_axis_v, _) =
            plane_basis_from_normal(Vec3::Z, GeometryTolerance::default()).unwrap();
        assert_eq!(z_axis_u, -Vec3::X);
        assert_eq!(z_axis_v, -Vec3::Y);
        let (_, y_axis_v, _) =
            plane_basis_from_normal(Vec3::Y, GeometryTolerance::default()).unwrap();
        assert_eq!(y_axis_v, -Vec3::X);
        assert!(
            plane_basis_from_normal(
                Vec3::Z,
                GeometryTolerance {
                    absolute: 1.0,
                    relative: f64::NAN,
                }
            )
            .is_none()
        );
    }

    #[test]
    fn vector_lengths_and_normalization_remain_stable_at_extreme_scales() {
        let large = Vec3::new(1.0e200, -1.0e200, 0.0);
        let small = Vec3::new(1.0e-200, -1.0e-200, 0.0);
        assert!((large.length() / (std::f64::consts::SQRT_2 * 1.0e200) - 1.0).abs() < 1.0e-15);
        assert!((small.length() / (std::f64::consts::SQRT_2 * 1.0e-200) - 1.0).abs() < 1.0e-15);
        assert!((large.try_normalized(0.0).unwrap().length() - 1.0).abs() < 1.0e-15);
        assert!(Vec3::ZERO.try_normalized(-1.0).is_none());
        assert!(Vec2::new(1.0e-200, 1.0e-200).try_normalized(0.0).is_some());
        assert!(
            Vec3::new(f64::MAX, f64::MAX, f64::MAX)
                .try_normalized(0.0)
                .is_some()
        );
        assert!(Vec3::X.try_normalized(1.0).is_none());
        assert!(Vec3::X.try_normalized(f64::NAN).is_none());
    }

    #[test]
    fn affine_inverse_handles_uniformly_tiny_and_huge_scales() {
        for scale in [1.0e-200, 1.0e200] {
            let transform = Affine3::from_linear_translation(
                [[scale, 0.0, 0.0], [0.0, scale, 0.0], [0.0, 0.0, scale]],
                Vec3::ZERO,
            );
            let point = Vec3::new(1.0, -2.0, 3.0);
            let world = transform.transform_point(point);
            let local = transform
                .inverse(GeometryTolerance::default())
                .unwrap()
                .transform_point(world);
            assert_vec_near(local, point, 1.0e-12);
        }
        assert!(GeometryTolerance::new(-1.0, 0.0).is_none());
    }

    #[test]
    fn model_unit_scale_preserves_geometry_and_matches_legacy_conversion_direction() {
        let millimeter_model = ModelUnitScale::new(0.001).unwrap();
        assert_eq!(Some(0.012), millimeter_model.displayed_length(12.0));
        assert_eq!(
            Some(12_000.0),
            millimeter_model.model_length_from_meters(12.0)
        );
        assert_eq!(Some(0.001), millimeter_model.export_coordinate_scale(None));
        assert_eq!(
            Some(1.0),
            millimeter_model.export_coordinate_scale(Some(0.001))
        );
        assert_eq!(Some(1.0), millimeter_model.model_units_per_millimeter());
        assert_eq!(None, ModelUnitScale::new(0.0));
        assert_eq!(None, millimeter_model.export_coordinate_scale(Some(0.0)));
    }

    #[test]
    fn named_legacy_quantizers_preserve_distinct_tie_rules() {
        assert_eq!(Some(0), LEGACY_LINE_KEY_QUANTIZATION.quantize(0.0005));
        assert_eq!(Some(2), LEGACY_FACE_KEY_QUANTIZATION.quantize(0.00025));
        assert_eq!(Some(4), LEGACY_FACE_KEY_QUANTIZATION.quantize(0.00035));
        assert_eq!(Some(1), LEGACY_HOTSPOT_KEY_QUANTIZATION.quantize(0.0005));
        assert_eq!(Some(0), LEGACY_HOTSPOT_KEY_QUANTIZATION.quantize(-0.0005));
        assert_eq!(
            None,
            QuantizationPolicy::new(0.0, QuantizationTieRule::ToEven).quantize(1.0)
        );
        assert_eq!(None, LEGACY_LINE_KEY_QUANTIZATION.quantize(f64::MAX));
    }

    #[test]
    fn segment_predicates_cover_crossing_touching_overlap_and_disjoint_cases() {
        let tolerance = GeometryTolerance::default();
        let crossing = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 2.0))
            .intersect(
                Segment2::new(Vec2::new(0.0, 2.0), Vec2::new(2.0, 0.0)),
                tolerance,
            )
            .unwrap();
        assert_eq!(
            SegmentIntersection2::Point {
                position: Vec2::new(1.0, 1.0),
                first_parameter: 0.5,
                second_parameter: 0.5,
            },
            crossing
        );

        let touching = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0))
            .intersect(
                Segment2::new(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)),
                tolerance,
            )
            .unwrap();
        assert!(
            matches!(touching, SegmentIntersection2::Point { position, .. }
            if position.distance(Vec2::new(1.0, 0.0)) < 1.0e-12)
        );

        let endpoint_on_edge = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0))
            .intersect(
                Segment2::new(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)),
                tolerance,
            )
            .unwrap();
        assert!(matches!(
            endpoint_on_edge,
            SegmentIntersection2::Point {
                first_parameter,
                second_parameter,
                ..
            } if (first_parameter - 0.5).abs() < 1.0e-12 && second_parameter == 0.0
        ));

        let overlap = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0))
            .intersect(
                Segment2::new(Vec2::new(1.0, 0.0), Vec2::new(4.0, 0.0)),
                tolerance,
            )
            .unwrap();
        assert_eq!(
            SegmentIntersection2::Overlap {
                start: Vec2::new(1.0, 0.0),
                end: Vec2::new(3.0, 0.0),
            },
            overlap
        );

        let within_tolerance = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0))
            .intersect(
                Segment2::new(Vec2::new(0.0, 0.5e-9), Vec2::new(2.0, 0.5e-9)),
                tolerance,
            )
            .unwrap();
        assert!(matches!(
            within_tolerance,
            SegmentIntersection2::Overlap { .. }
        ));
        let outside_tolerance = Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0))
            .intersect(
                Segment2::new(Vec2::new(0.0, 2.0e-9), Vec2::new(2.0, 2.0e-9)),
                tolerance,
            )
            .unwrap();
        assert_eq!(outside_tolerance, SegmentIntersection2::Disjoint);

        assert_eq!(
            Some(SegmentIntersection2::Disjoint),
            Segment2::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)).intersect(
                Segment2::new(Vec2::new(0.0, 2.0), Vec2::new(1.0, 2.0)),
                tolerance,
            )
        );
        assert_eq!(
            None,
            Segment2::new(Vec2::ZERO, Vec2::new(f64::NAN, 0.0))
                .intersect(Segment2::new(Vec2::ZERO, Vec2::new(1.0, 0.0)), tolerance,)
        );
    }

    #[test]
    fn triangle_queries_return_barycentrics_and_reject_off_plane_points() {
        let triangle = Triangle3::new(
            Vec3::ZERO,
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        );
        let tolerance = GeometryTolerance::default();
        assert_vec_near(triangle.normal(tolerance).unwrap(), Vec3::Z, 1.0e-12);
        assert_eq!(Some(2.0), triangle.area(tolerance));
        let weights = triangle
            .barycentric_coordinates(Vec3::new(0.5, 0.5, 0.0), tolerance)
            .unwrap();
        assert_eq!([0.5, 0.25, 0.25], weights);
        assert_eq!(
            Some(true),
            triangle.contains_point(Vec3::new(1.0, 1.0, 0.0), tolerance)
        );
        assert_eq!(
            Some(false),
            triangle.contains_point(Vec3::new(2.0, 2.0, 0.0), tolerance)
        );
        assert_eq!(
            Some(false),
            triangle.contains_point(Vec3::new(0.5, 0.5, 1.0), tolerance)
        );

        let tiny = Triangle3::new(
            Vec3::ZERO,
            Vec3::new(1.0e-12, 0.0, 0.0),
            Vec3::new(0.0, 1.0e-12, 0.0),
        );
        let tiny_tolerance = GeometryTolerance::new(0.0, 1.0e-12).unwrap();
        assert!(tiny.area(tiny_tolerance).unwrap() > 0.0);
        assert!(tiny.normal(tiny_tolerance).is_some());

        let degenerate = Triangle3::new(Vec3::ZERO, Vec3::X, Vec3::X * 2.0);
        assert!(degenerate.normal(tolerance).is_none());
        assert!(
            degenerate
                .barycentric_coordinates(Vec3::ZERO, tolerance)
                .is_none()
        );
    }
}
