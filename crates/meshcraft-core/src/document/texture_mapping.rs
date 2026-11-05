//! Planar texture placement shared by the viewport shader and the texture edit mode.
//!
//! A material's image covers one tile of `texture_scale_u` × `texture_scale_v` model units and
//! repeats across every face that uses it. Each face projects onto its own plane: the U axis runs
//! horizontally along the face (to the right as you look at it) and V runs up it. Horizontal faces
//! use X for U. Coordinates are in the owning prototype's local space, so a texture moves with its
//! object. `assets/shaders/scene.fs` evaluates the same formulas; keep them in step.
use super::materials::Material;
use crate::geometry::Vec3;

/// Faces steeper than this use the horizontal-face basis.
const HORIZONTAL: f64 = 0.999;
/// The smallest tile a drag can produce, so a collapsed corner never divides by zero.
pub const MIN_TILE: f64 = 1.0e-3;

/// The U and V directions on a face with this normal, or `None` for a degenerate normal.
pub fn axes(normal: Vec3) -> Option<(Vec3, Vec3)> {
    let n = normal.try_normalized(1.0e-12)?;
    let u = if n.y.abs() > HORIZONTAL {
        Vec3::X
    } else {
        Vec3::Y.cross(n).try_normalized(1.0e-12)?
    };
    let v = n.cross(u).try_normalized(1.0e-12)?;
    Some((u, v))
}

/// Placement of one tile, in model units along a face's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub offset: [f64; 2],
    pub size: [f64; 2],
}

impl Placement {
    pub fn of(material: &Material) -> Self {
        Self {
            offset: [material.texture_offset_u, material.texture_offset_v],
            size: [material.texture_scale_u, material.texture_scale_v],
        }
    }

    /// Writes the placement back, keeping each offset within one tile so values stay small.
    pub fn apply(self, material: &mut Material) {
        material.texture_scale_u = self.size[0];
        material.texture_scale_v = self.size[1];
        material.texture_offset_u = self.offset[0].rem_euclid(self.size[0]);
        material.texture_offset_v = self.offset[1].rem_euclid(self.size[1]);
    }

    /// Image coordinates of a point: whole numbers fall on tile borders.
    pub fn uv(self, point: Vec3, normal: Vec3) -> Option<[f64; 2]> {
        let (u, v) = axes(normal)?;
        Some([
            (point.dot(u) - self.offset[0]) / self.size[0],
            (point.dot(v) - self.offset[1]) / self.size[1],
        ])
    }

    /// The tile under a point, on that point's face plane.
    pub fn tile_at(self, point: Vec3, normal: Vec3) -> Option<Tile> {
        let [i, j] = self.uv(point, normal)?.map(f64::floor);
        let (u, v) = axes(normal)?;
        Some(Tile {
            u,
            v,
            depth: point - u * point.dot(u) - v * point.dot(v),
            start: [
                self.offset[0] + i * self.size[0],
                self.offset[1] + j * self.size[1],
            ],
            size: self.size,
        })
    }
}

/// One repeat of the image on a face plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub u: Vec3,
    pub v: Vec3,
    /// The plane's offset along its normal, so plane coordinates map back to 3D.
    pub depth: Vec3,
    /// Plane coordinates of the image's lower-left corner.
    pub start: [f64; 2],
    pub size: [f64; 2],
}

/// What a drag in the texture edit mode holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Grip {
    /// A corner, counter-clockwise from the image's lower-left: the opposite corner stays put.
    Corner(usize),
    /// The tile's middle: the image slides without resizing.
    Move,
}

impl Tile {
    pub fn point(&self, s: f64, t: f64) -> Vec3 {
        self.depth + self.u * s + self.v * t
    }

    /// Plane coordinates of a corner, counter-clockwise from the lower-left.
    fn corner(&self, index: usize) -> [f64; 2] {
        let [a, b] = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]][index % 4];
        [
            self.start[0] + a * self.size[0],
            self.start[1] + b * self.size[1],
        ]
    }

    pub fn corners(&self) -> [Vec3; 4] {
        std::array::from_fn(|i| {
            let [s, t] = self.corner(i);
            self.point(s, t)
        })
    }

    pub fn center(&self) -> Vec3 {
        self.point(
            self.start[0] + self.size[0] * 0.5,
            self.start[1] + self.size[1] * 0.5,
        )
    }

    /// The tile after dragging `grip` from `from` to `to`. Points are projected onto the plane.
    pub fn dragged(&self, grip: Grip, from: Vec3, to: Vec3) -> Tile {
        let plane = |p: Vec3| [p.dot(self.u), p.dot(self.v)];
        let [s, t] = plane(to);
        let mut next = *self;
        match grip {
            Grip::Move => {
                let [s0, t0] = plane(from);
                next.start = [self.start[0] + s - s0, self.start[1] + t - t0];
            }
            Grip::Corner(index) => {
                // The moved corner may cross the fixed one; the tile then spans the other way,
                // without mirroring the image.
                let [fs, ft] = self.corner(index + 2);
                next.start = [fs.min(s), ft.min(t)];
                next.size = [(s - fs).abs().max(MIN_TILE), (t - ft).abs().max(MIN_TILE)];
                if s < fs && (fs - s) < MIN_TILE {
                    next.start[0] = fs - MIN_TILE;
                }
                if t < ft && (ft - t) < MIN_TILE {
                    next.start[1] = ft - MIN_TILE;
                }
            }
        }
        next
    }

    /// The material placement that shows this tile.
    pub fn placement(&self) -> Placement {
        Placement {
            offset: self.start,
            size: self.size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) {
        assert!(a.distance(b) < 1.0e-9, "{a:?} != {b:?}");
    }

    #[test]
    fn walls_run_u_to_the_right_and_v_up_and_floors_use_x() {
        // Looking at a wall that faces +Z, right is +X.
        let (u, v) = axes(Vec3::Z).unwrap();
        close(u, Vec3::X);
        close(v, Vec3::Y);
        // The far side of the same wall reads right-to-left in X, still upright.
        let (u, v) = axes(-Vec3::Z).unwrap();
        close(u, -Vec3::X);
        close(v, Vec3::Y);
        let (u, v) = axes(Vec3::Y).unwrap();
        close(u, Vec3::X);
        close(v, -Vec3::Z);
        // A roof slope keeps U level, so courses stay horizontal.
        let (u, v) = axes(Vec3::new(0., 1., 1.)).unwrap();
        assert!(u.y.abs() < 1.0e-12);
        assert!(v.y > 0.);
        assert!(axes(Vec3::ZERO).is_none());
    }

    #[test]
    fn tiles_repeat_from_the_offset_at_the_material_size() {
        let placement = Placement {
            offset: [0.5, 0.],
            size: [2., 3.],
        };
        assert_eq!(
            placement.uv(Vec3::new(4.5, 6., 0.), Vec3::Z),
            Some([2., 2.])
        );
        let tile = placement.tile_at(Vec3::new(3., 1., 7.), Vec3::Z).unwrap();
        assert_eq!(tile.start, [2.5, 0.]);
        let corners = tile.corners();
        close(corners[0], Vec3::new(2.5, 0., 7.));
        close(corners[2], Vec3::new(4.5, 3., 7.));
        close(tile.center(), Vec3::new(3.5, 1.5, 7.));
    }

    #[test]
    fn a_corner_drag_keeps_the_opposite_corner_and_a_move_keeps_the_size() {
        let tile = Placement {
            offset: [0., 0.],
            size: [1., 1.],
        }
        .tile_at(Vec3::new(0.5, 0.5, 0.), Vec3::Z)
        .unwrap();
        // Upper-right corner pulled out to (3, 2): the lower-left stays at the origin.
        let wide = tile.dragged(
            Grip::Corner(2),
            Vec3::new(1., 1., 0.),
            Vec3::new(3., 2., 0.),
        );
        assert_eq!((wide.start, wide.size), ([0., 0.], [3., 2.]));
        // Lower-left pulled past the fixed corner flips the span, not the image.
        let crossed = tile.dragged(Grip::Corner(0), Vec3::ZERO, Vec3::new(2., 3., 0.));
        assert_eq!((crossed.start, crossed.size), ([1., 1.], [1., 2.]));
        let collapsed = tile.dragged(Grip::Corner(2), Vec3::ZERO, Vec3::ZERO);
        assert_eq!(collapsed.size, [MIN_TILE, MIN_TILE]);
        let moved = tile.dragged(
            Grip::Move,
            Vec3::new(0.5, 0.5, 0.),
            Vec3::new(0.75, 0.25, 9.),
        );
        assert_eq!((moved.start, moved.size), ([0.25, -0.25], [1., 1.]));
    }

    #[test]
    fn applying_a_placement_keeps_offsets_within_one_tile() {
        let mut material = Material::default();
        Placement {
            offset: [7.5, -0.5],
            size: [2., 3.],
        }
        .apply(&mut material);
        assert_eq!(
            [
                material.texture_offset_u,
                material.texture_offset_v,
                material.texture_scale_u,
                material.texture_scale_v
            ],
            [1.5, 2.5, 2., 3.]
        );
        material.validate().unwrap();
    }
}
