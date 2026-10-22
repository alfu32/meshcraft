//! Pixel-accurate native snap tolerance with depth-aware BVH margins.
use meshcraft_core::geometry::{Aabb3, Vec3};

#[derive(Clone, Copy, Debug)]
pub(crate) struct SnapView {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    focal_pixels: f64,
    ortho_distance: Option<f64>,
}
impl SnapView {
    pub(crate) fn new(
        eye: Vec3,
        target: Vec3,
        up: Vec3,
        field_of_view: f64,
        height_pixels: f64,
        ortho_distance: Option<f64>,
    ) -> Option<Self> {
        let forward = (target - eye).try_normalized(1e-12)?;
        let right = forward.cross(up).try_normalized(1e-12)?;
        let up = right.cross(forward);
        let focal_pixels = height_pixels / (2. * (field_of_view.to_radians() * 0.5).tan());
        if !focal_pixels.is_finite()
            || focal_pixels <= 0.
            || ortho_distance.is_some_and(|v| !v.is_finite() || v <= 0.)
        {
            return None;
        }
        Some(Self {
            eye,
            forward,
            right,
            up,
            focal_pixels,
            ortho_distance,
        })
    }
    fn depth(self, p: Vec3) -> f64 {
        (p - self.eye).dot(self.forward)
    }
    fn project(self, p: Vec3) -> Option<[f64; 2]> {
        let depth = self.depth(p);
        if depth <= 1e-9 {
            return None;
        }
        let delta = p - self.eye;
        let scale = self.focal_pixels / self.ortho_distance.unwrap_or(depth);
        Some([delta.dot(self.right) * scale, delta.dot(self.up) * scale])
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SnapTolerance {
    World(f64),
    Pixels {
        view: SnapView,
        pointer: [f64; 2],
        radius: f64,
    },
}
impl SnapTolerance {
    pub(crate) fn pixels(
        view: SnapView,
        origin: Vec3,
        direction: Vec3,
        radius: f64,
    ) -> Option<Self> {
        if !radius.is_finite() || radius < 0. {
            return None;
        }
        Some(Self::Pixels {
            view,
            pointer: view.project(origin + direction)?,
            radius,
        })
    }
    pub(crate) fn radius(self) -> f64 {
        match self {
            Self::World(r) | Self::Pixels { radius: r, .. } => r,
        }
    }
    pub(crate) fn world_radius(self, point: Vec3) -> f64 {
        match self {
            Self::World(r) => r,
            Self::Pixels { view, radius, .. } => {
                radius * view.ortho_distance.unwrap_or(view.depth(point).max(0.))
                    / view.focal_pixels
            }
        }
    }
    /// Expansion grows with each node's farthest depth; distant nodes cannot
    /// inflate the tolerance used for unrelated nearby geometry.
    pub(crate) fn margin(self, bounds: Aabb3) -> f64 {
        match self {
            Self::World(r) => r,
            Self::Pixels { view, .. } => {
                let corner = Vec3::new(
                    if view.forward.x >= 0. {
                        bounds.max.x
                    } else {
                        bounds.min.x
                    },
                    if view.forward.y >= 0. {
                        bounds.max.y
                    } else {
                        bounds.min.y
                    },
                    if view.forward.z >= 0. {
                        bounds.max.z
                    } else {
                        bounds.min.z
                    },
                );
                self.world_radius(corner)
            }
        }
    }
    pub(crate) fn distance(self, p: Vec3, origin: Vec3, direction: Vec3) -> Option<f64> {
        match self {
            Self::World(_) => Some((p - origin - direction * (p - origin).dot(direction)).length()),
            Self::Pixels { view, pointer, .. } => {
                let q = view.project(p)?;
                Some((q[0] - pointer[0]).hypot(q[1] - pointer[1]))
            }
        }
    }
    /// Projected closest segment point, with perspective-correct interpolation.
    pub(crate) fn segment_point(self, mut a: Vec3, mut b: Vec3) -> Option<Vec3> {
        let Self::Pixels { view, pointer, .. } = self else {
            return None;
        };
        let near = 1e-8;
        let (da, db) = (view.depth(a), view.depth(b));
        if da < near && db < near {
            return None;
        }
        if da < near {
            a += (b - a) * ((near - da) / (db - da));
        }
        if db < near {
            b += (a - b) * ((near - db) / (view.depth(a) - db));
        }
        let (pa, pb) = (view.project(a)?, view.project(b)?);
        let delta = [pb[0] - pa[0], pb[1] - pa[1]];
        let length = delta[0] * delta[0] + delta[1] * delta[1];
        let t = if length > 1e-20 {
            ((pointer[0] - pa[0]) * delta[0] + (pointer[1] - pa[1]) * delta[1]) / length
        } else {
            0.
        };
        let t = t.clamp(0., 1.);
        let t = if view.ortho_distance.is_some() {
            t
        } else {
            let (wa, wb) = ((1. - t) / view.depth(a), t / view.depth(b));
            wb / (wa + wb)
        };
        Some(a + (b - a) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_interpolation_minimizes_screen_distance_even_across_large_depth_changes() {
        let view = SnapView::new(Vec3::ZERO, -Vec3::Z, Vec3::Y, 90., 1000., None).unwrap();
        let ray = Vec3::new(0.2, 0., -1.).try_normalized(1e-12).unwrap();
        let tolerance = SnapTolerance::pixels(view, Vec3::ZERO, ray, 12.).unwrap();
        let p = tolerance
            .segment_point(Vec3::new(0., 0., -1.), Vec3::new(100., 0., -100.))
            .unwrap();
        assert!(tolerance.distance(p, Vec3::ZERO, ray).unwrap() < 1e-8);
        assert!((p.x / -p.z - 0.2).abs() < 1e-9);
    }
}
