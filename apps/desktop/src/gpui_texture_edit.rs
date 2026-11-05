//! Texture edit mode. Clicking a textured face shows one tile of its material's image on that
//! face, with a handle at each corner and one in the middle. Dragging a corner resizes the tile
//! while the opposite corner stays put; dragging the middle slides it. Releasing saves the
//! placement to the material, so every face using that material changes, as one undoable edit.
use super::*;
use meshcraft_core::{
    document::{
        materials::material_id,
        texture_mapping::{Grip, Placement, Tile},
    },
    geometry::{Affine3, GeometryTolerance},
    selection::SelectionKind,
};

/// Handles within this many pixels of the pointer can be grabbed.
const GRAB_PIXELS: f32 = 12.;

/// What the viewport draws: the tile outline and its handles, in world space.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Frame {
    pub corners: [Vec3; 4],
    pub center: Vec3,
    pub grip: Option<Grip>,
}

/// The face being edited. Points and normals are in its prototype's space, like the mapping.
#[derive(Clone)]
struct Target {
    material: String,
    world: Affine3,
    inverse: Affine3,
    point: Vec3,
    normal: Vec3,
}

struct Drag {
    grip: Grip,
    from: Vec3,
    start: Tile,
    tile: Tile,
    screen: (f32, f32),
    moved: bool,
}

#[derive(Default)]
pub(super) struct State {
    target: Option<Target>,
    drag: Option<Drag>,
}

impl State {
    fn placement(&self, document: &Document) -> Option<(Target, Placement)> {
        let target = self.target.clone()?;
        let material = document
            .materials()
            .into_iter()
            .find(|m| m.id == target.material && m.texture_path.is_some())?;
        Some((target, Placement::of(&material)))
    }

    fn tile(&self, document: &Document) -> Option<(Target, Tile)> {
        if let (Some(target), Some(drag)) = (&self.target, &self.drag) {
            return Some((target.clone(), drag.tile));
        }
        let (target, placement) = self.placement(document)?;
        let tile = placement.tile_at(target.point, target.normal)?;
        Some((target, tile))
    }

    pub fn frame(&self, document: &Document) -> Option<Frame> {
        let (target, tile) = self.tile(document)?;
        Some(Frame {
            corners: tile.corners().map(|p| target.world.transform_point(p)),
            center: target.world.transform_point(tile.center()),
            grip: self.drag.as_ref().map(|d| d.grip),
        })
    }

    /// The placement being dragged, so the viewport shows it before it is saved.
    pub fn preview(&self) -> Option<(String, Placement)> {
        let drag = self.drag.as_ref().filter(|d| d.moved)?;
        Some((
            self.target.as_ref()?.material.clone(),
            drag.tile.placement(),
        ))
    }
}

/// The nearest visible face along a ray, with its material, in its prototype's space. The normal
/// faces the camera, as the shader's does.
fn pick_face(
    document: &Document,
    policy: &meshcraft_core::selection::SelectionDisplayPolicy,
    origin: Vec3,
    direction: Vec3,
) -> Option<(Target, Option<String>)> {
    if !policy.can_draw(SelectionKind::Face) {
        return None;
    }
    let mut best: Option<(f64, Target, Option<String>)> = None;
    for occurrence in document.scene_geometry_refs().ok()? {
        if !policy.occurrence_visible(document, &occurrence) {
            continue;
        }
        let (Some(resolved), Some(geometry)) = (
            document.resolve_occurrence(&occurrence),
            document.effective_geometry(&occurrence),
        ) else {
            continue;
        };
        let world = resolved.world_transform;
        let Some(inverse) = world.inverse(GeometryTolerance::default()) else {
            continue;
        };
        for face in geometry.triangles.values() {
            let [Some(a), Some(b), Some(c)] = face.vertices.map(|r| geometry.vertex_position(r))
            else {
                continue;
            };
            let [wa, wb, wc] = [a, b, c].map(|p| world.transform_point(p));
            let Some(distance) = ray_triangle(origin, direction, [wa, wb, wc]) else {
                continue;
            };
            if best.as_ref().is_some_and(|(d, _, _)| *d <= distance) {
                continue;
            }
            let Some(mut normal) = (b - a).cross(c - a).try_normalized(1.0e-12) else {
                continue;
            };
            if (wb - wa).cross(wc - wa).dot(direction) > 0. {
                normal = -normal;
            }
            if world.determinant() < 0. {
                normal = -normal;
            }
            let material = material_id(&face.extensions).map(str::to_owned);
            best = Some((
                distance,
                Target {
                    material: material.clone().unwrap_or_default(),
                    world,
                    inverse,
                    point: inverse.transform_point(origin + direction * distance),
                    normal,
                },
                material,
            ));
        }
    }
    best.map(|(_, target, material)| (target, material))
}

/// Möller–Trumbore, counting both sides of the triangle.
fn ray_triangle(origin: Vec3, direction: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f64> {
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inv = 1. / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    let q = s.cross(ab);
    let v = direction.dot(q) * inv;
    if !(-1.0e-9..=1. + 1.0e-9).contains(&u) || v < -1.0e-9 || u + v > 1. + 1.0e-9 {
        return None;
    }
    let t = ac.dot(q) * inv;
    (t > 1.0e-9).then_some(t)
}

impl MeshcraftShell {
    pub(super) fn texture_edit_active(&self) -> bool {
        self.session.native_texture_edit_active()
    }

    /// The project's folder, which relative texture paths are stored against.
    pub(super) fn texture_root(&self) -> Option<std::path::PathBuf> {
        self.session
            .current_project_path()
            .and_then(|p| std::path::Path::new(&p).parent().map(Into::into))
    }

    /// Where the pointer is on the edited face's plane, in its prototype's space. A snap on that
    /// plane wins, so a corner can land exactly on an edge or endpoint.
    fn texture_plane_point(&self, target: &Target, x: f32, y: f32) -> Option<Vec3> {
        let plane = target.normal.dot(target.point);
        if let Some(snap) = self.cursor_snap {
            let local = target.inverse.transform_point(snap.point);
            if (target.normal.dot(local) - plane).abs() < 1.0e-6 {
                return Some(local);
            }
        }
        let (origin, direction) =
            self.camera
                .ray(x, y, self.layout.width as f32, self.layout.height as f32)?;
        let origin = target.inverse.transform_point(origin);
        let direction = target.inverse.transform_vector(direction);
        let along = target.normal.dot(direction);
        if along.abs() < 1.0e-12 {
            return None;
        }
        let t = (plane - target.normal.dot(origin)) / along;
        (t > 0.).then(|| origin + direction * t)
    }

    /// Handles a press in the viewport. Returns false when the mode is off.
    pub(super) fn begin_texture_edit(&mut self, x: f32, y: f32) -> bool {
        if !self.texture_edit_active() {
            return false;
        }
        let size = (self.layout.width as f32, self.layout.height as f32);
        if let Some(frame) = self.texture_edit.frame(&self.cached_document) {
            let grips = (0..4)
                .map(|i| (Grip::Corner(i), frame.corners[i]))
                .chain([(Grip::Move, frame.center)]);
            let grabbed = grips
                .filter_map(|(grip, point)| {
                    let (px, py) = project_point(self.camera, size, point)?;
                    let distance = (px - x).hypot(py - y);
                    (distance <= GRAB_PIXELS).then_some((distance, grip))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, grip)) = grabbed
                && let Some((_, tile)) = self.texture_edit.tile(&self.cached_document)
            {
                let from = match grip {
                    Grip::Move => tile.center(),
                    Grip::Corner(i) => tile.corners()[i],
                };
                self.texture_edit.drag = Some(Drag {
                    grip,
                    from,
                    start: tile,
                    tile,
                    screen: (x, y),
                    moved: false,
                });
                self.refresh_scene();
                return true;
            }
        }
        let Some((origin, direction)) = self.camera.ray(x, y, size.0, size.1) else {
            return true;
        };
        let policy = self.session.native_selection_display_policy();
        let picked = pick_face(&self.cached_document, &policy, origin, direction);
        let textured = |id: &Option<String>| {
            id.as_ref().is_some_and(|id| {
                self.cached_document
                    .materials()
                    .iter()
                    .any(|m| &m.id == id && m.texture_path.is_some())
            })
        };
        match picked {
            Some((target, material)) if textured(&material) => {
                self.session.set_native_status(format!(
                    "Texture of {}: drag a corner to size it or the middle to move it. Esc finishes.",
                    self.material_name(&target.material)
                ));
                self.texture_edit.target = Some(target);
            }
            Some(_) => {
                self.session.set_native_status(
                    "That face's material has no texture. Pick an image in the Material Editor first."
                        .into(),
                );
            }
            None => self.texture_edit.target = None,
        }
        self.refresh_scene();
        true
    }

    pub(super) fn update_texture_edit(&mut self, x: f32, y: f32) {
        let Some(target) = self.texture_edit.target.clone() else {
            return;
        };
        let Some(drag) = self.texture_edit.drag.as_ref() else {
            return;
        };
        if !drag.moved && (x - drag.screen.0).hypot(y - drag.screen.1) < 3. {
            return;
        }
        let Some(to) = self.texture_plane_point(&target, x, y) else {
            return;
        };
        let drag = self.texture_edit.drag.as_mut().unwrap();
        drag.tile = drag.start.dragged(drag.grip, drag.from, to);
        drag.screen = (x, y);
        drag.moved = true;
        self.refresh_scene();
    }

    /// The tile's size beside the pointer while dragging, in the drawing's units.
    pub(super) fn texture_drag_feedback(&self) -> Option<ToolFeedback> {
        let drag = self.texture_edit.drag.as_ref().filter(|d| d.moved)?;
        let settings = self.cached_document.model_settings();
        Some(ToolFeedback {
            screen_position: egui::Pos2::new(drag.screen.0, drag.screen.1),
            text: format!(
                "Texture {:.3} × {:.3} {}",
                drag.tile.size[0] * settings.unit_size,
                drag.tile.size[1] * settings.unit_size,
                settings.unit_name
            ),
        })
    }

    pub(super) fn cancel_texture_drag(&mut self) -> bool {
        let cancelled = self.texture_edit.drag.take().is_some();
        if cancelled {
            self.refresh_scene();
        }
        cancelled
    }

    fn material_name(&self, id: &str) -> String {
        self.cached_document
            .materials()
            .into_iter()
            .find(|m| m.id == id)
            .map_or_else(|| id.to_owned(), |m| m.name)
    }

    /// Saves a finished drag to the material. Returns false when no drag was in progress.
    pub(super) fn finish_texture_edit(&mut self) -> bool {
        let Some(drag) = self.texture_edit.drag.take() else {
            return false;
        };
        let Some(target) = self.texture_edit.target.as_mut() else {
            return true;
        };
        if drag.moved {
            // Keep the outline on the tile that was dragged.
            target.point = drag.tile.center();
            let material = self
                .cached_document
                .materials()
                .into_iter()
                .find(|m| m.id == target.material);
            let result = material
                .ok_or_else(|| "the material was removed".to_owned())
                .and_then(|mut material| {
                    drag.tile.placement().apply(&mut material);
                    self.session.save_native_material(material)
                });
            let id = target.material.clone();
            match result {
                Ok(()) => {
                    let name = self.material_name(&id);
                    self.session.set_native_status(format!(
                        "Texture of {name} is now {:.3} × {:.3} units.",
                        drag.tile.size[0], drag.tile.size[1]
                    ))
                }
                Err(error) => self.session.set_native_status(error),
            }
        }
        self.refresh_scene();
        true
    }

    /// Leaves the mode when the session ends it (Esc, another tool, a new drawing).
    pub(super) fn sync_texture_edit(&mut self) {
        if !self.texture_edit_active()
            && (self.texture_edit.target.is_some() || self.texture_edit.drag.is_some())
        {
            self.texture_edit = State::default();
            self.refresh_scene();
        }
    }
}

/// The tile outline over the scene, with square corner handles and a round middle one.
pub(super) fn draw(
    drawing: &mut impl RaylibDraw,
    frame: &Frame,
    camera: OrbitCamera,
    size: (f32, f32),
) {
    use raylib::prelude::Vector2;
    let casing = Color::new(30, 45, 60, 255);
    let line = Color::new(255, 196, 0, 255);
    let active = Color::new(242, 69, 59, 255);
    let screen = |p: Vec3| project_point(camera, size, p).map(|(x, y)| Vector2::new(x, y));
    let corners = frame.corners.map(screen);
    for i in 0..4 {
        if let (Some(a), Some(b)) = (corners[i], corners[(i + 1) % 4]) {
            drawing.draw_line_ex(a, b, 4., casing);
            drawing.draw_line_ex(a, b, 2., line);
        }
    }
    for (i, corner) in corners.iter().enumerate() {
        let Some(p) = corner else { continue };
        let fill = if frame.grip == Some(Grip::Corner(i)) {
            active
        } else {
            line
        };
        drawing.draw_rectangle(p.x as i32 - 6, p.y as i32 - 6, 12, 12, casing);
        drawing.draw_rectangle(p.x as i32 - 4, p.y as i32 - 4, 8, 8, fill);
    }
    if let Some(p) = screen(frame.center) {
        let fill = if frame.grip == Some(Grip::Move) {
            active
        } else {
            line
        };
        drawing.draw_circle_v(p, 7., casing);
        drawing.draw_circle_v(p, 5., fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_hit_both_sides_of_a_triangle_and_miss_outside_it() {
        let triangle = [Vec3::ZERO, Vec3::X * 2., Vec3::Y * 2.];
        let front = ray_triangle(Vec3::new(0.5, 0.5, 3.), -Vec3::Z, triangle);
        assert!((front.unwrap() - 3.).abs() < 1.0e-12);
        let back = ray_triangle(Vec3::new(0.5, 0.5, -1.), Vec3::Z, triangle);
        assert!((back.unwrap() - 1.).abs() < 1.0e-12);
        assert!(ray_triangle(Vec3::new(1.5, 1.5, 3.), -Vec3::Z, triangle).is_none());
        assert!(ray_triangle(Vec3::new(0.5, 0.5, 3.), Vec3::Z, triangle).is_none());
    }
}
