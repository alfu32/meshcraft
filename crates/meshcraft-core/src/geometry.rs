use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

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
    /// Returns the linear tolerance at a characteristic model scale.
    pub fn linear_at(self, scale: f64) -> f64 {
        self.absolute.max(self.relative * scale.abs())
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
        self.length_squared().sqrt()
    }

    pub fn try_normalized(self, minimum_length: f64) -> Option<Self> {
        let length = self.length();
        (length.is_finite() && length > minimum_length).then(|| self * (1.0 / length))
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
        if !origin.is_finite() {
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
        if !origin.is_finite() {
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
        if !self.is_finite() {
            return None;
        }
        let m = self.linear;
        let determinant = self.determinant();
        let scale = m.into_iter().flatten().map(f64::abs).fold(0.0, f64::max);
        if determinant.abs() <= tolerance.linear_at(scale).powi(3) {
            return None;
        }
        let inverse_determinant = 1.0 / determinant;
        let inverse = [
            [
                m[1][1] * m[2][2] - m[1][2] * m[2][1],
                m[0][2] * m[2][1] - m[0][1] * m[2][2],
                m[0][1] * m[1][2] - m[0][2] * m[1][1],
            ],
            [
                m[1][2] * m[2][0] - m[1][0] * m[2][2],
                m[0][0] * m[2][2] - m[0][2] * m[2][0],
                m[0][2] * m[1][0] - m[0][0] * m[1][2],
            ],
            [
                m[1][0] * m[2][1] - m[1][1] * m[2][0],
                m[0][1] * m[2][0] - m[0][0] * m[2][1],
                m[0][0] * m[1][1] - m[0][1] * m[1][0],
            ],
        ]
        .map(|row| row.map(|value| value * inverse_determinant));
        let inverse = Self::from_linear_translation(inverse, Vec3::ZERO);
        Some(Self::from_linear_translation(
            inverse.linear,
            -inverse.transform_vector(self.translation),
        ))
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
    let normal = normal.try_normalized(tolerance.absolute)?;
    let reference = [Vec3::X, Vec3::Y, Vec3::Z]
        .into_iter()
        .min_by(|left, right| left.dot(normal).abs().total_cmp(&right.dot(normal).abs()))?;
    let u = reference.cross(normal).try_normalized(tolerance.absolute)?;
    let v = normal.cross(u).try_normalized(tolerance.absolute)?;
    Some((u, v, normal))
}

#[cfg(test)]
mod tests {
    use super::{Affine3, GeometryTolerance, Plane3, Vec3, plane_basis_from_normal};

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
    }
}
