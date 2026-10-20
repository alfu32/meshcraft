//! Screen-sized instance handles, rendered over the offscreen scene, with one undoable drag.
use super::*;
use meshcraft_core::document::{
    OccurrenceRef,
    hotspots::{Hotspot, Shape},
};

#[derive(Clone)]
pub(super) struct Handle {
    pub occurrence: OccurrenceRef,
    pub hotspot: Hotspot,
    pub world: Vec3,
}
#[derive(Clone)]
pub(super) struct Drag {
    pub handle: Handle,
    pub authoring: bool,
    pub start: (f32, f32),
    pub start_pick: Vec3,
    pub start_target: Vec3,
    pub target: Vec3,
    pub inverse: meshcraft_core::geometry::Affine3,
    pub moved: bool,
}

pub(super) fn handles(document: &Document) -> Vec<Handle> {
    let mut result = Vec::new();
    for occurrence in document.scene_geometry_refs().unwrap_or_default() {
        let Some(r) = document.resolve_occurrence(&occurrence) else {
            continue;
        };
        let Ok(hotspots) = document.hotspots(r.definition.id) else {
            continue;
        };
        let positions = document
            .evaluated_hotspot_positions(&occurrence)
            .unwrap_or_default();
        for hotspot in hotspots {
            let position = positions
                .get(&hotspot.id)
                .copied()
                .unwrap_or(hotspot.position);
            result.push(Handle {
                occurrence: occurrence.clone(),
                world: r.world_transform.transform_point(position),
                hotspot,
            });
        }
    }
    result
}

pub(super) fn hit(shell: &MeshcraftShell, x: f32, y: f32) -> Option<Handle> {
    handles(&shell.cached_document)
        .into_iter()
        .filter(|handle| {
            shell.session.native_target_editable(&SelectionTarget {
                occurrence: handle.occurrence.clone(),
                entity: SelectedEntity::SemanticHandle {
                    kind: meshcraft_core::selection::SelectionKind::Hotspot,
                    id: handle.hotspot.id.clone(),
                },
            })
        })
        .filter_map(|handle| {
            let (hx, hy) = project_point(
                shell.camera,
                (shell.layout.width as f32, shell.layout.height as f32),
                handle.world,
            )?;
            let distance = (hx - x).hypot(hy - y);
            (distance <= 12.).then_some((distance, handle))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, handle)| handle)
}

pub(super) fn draw(
    drawing: &mut impl RaylibDraw,
    scene: &RenderScene,
    camera: OrbitCamera,
    size: (f32, f32),
    handles: &[Handle],
) {
    for handle in handles {
        let Some((x, y)) = project_point(camera, size, handle.world) else {
            continue;
        };
        let selected = scene.selection.iter().any(|target| target.occurrence == handle.occurrence && matches!(&target.entity,
            SelectedEntity::SemanticHandle { kind: meshcraft_core::selection::SelectionKind::Hotspot, id } if id == &handle.hotspot.id));
        let wireframe = scene
            .display_policy
            .state(meshcraft_core::selection::SelectionKind::Hotspot)
            .wireframe;
        let outline = if wireframe {
            ray_color(handle.hotspot.color.rgba())
        } else {
            Color::new(30, 45, 60, 255)
        };
        let color = if selected {
            Color::new(242, 69, 59, 255)
        } else {
            ray_color(handle.hotspot.color.rgba())
        };
        let point = raylib::prelude::Vector2::new(x, y);
        match handle.hotspot.shape {
            Shape::Circle => {
                if !wireframe {
                    drawing.draw_circle(x as i32, y as i32, 7., color);
                }
                drawing.draw_circle_lines(x as i32, y as i32, 7., outline);
            }
            Shape::Square => {
                if !wireframe {
                    drawing.draw_rectangle(x as i32 - 7, y as i32 - 7, 14, 14, color);
                }
                drawing.draw_rectangle_lines(x as i32 - 7, y as i32 - 7, 14, 14, outline);
            }
            shape => {
                let offsets: &[(f32, f32)] = match shape {
                    Shape::TriangleUp => &[(0., -8.), (-8., 7.), (8., 7.)],
                    Shape::TriangleDown => &[(0., 8.), (8., -7.), (-8., -7.)],
                    _ => &[(0., -9.), (-9., 0.), (0., 9.), (9., 0.)],
                };
                let points = offsets
                    .iter()
                    .map(|(dx, dy)| raylib::prelude::Vector2::new(point.x + dx, point.y + dy))
                    .collect::<Vec<_>>();
                if !wireframe {
                    for i in 1..points.len() - 1 {
                        drawing.draw_triangle(points[0], points[i], points[i + 1], color);
                    }
                }
                for i in 0..points.len() {
                    drawing.draw_line_v(points[i], points[(i + 1) % points.len()], outline);
                }
            }
        }
    }
}

impl MeshcraftShell {
    pub(super) fn begin_hotspot_drag(
        &mut self,
        x: f32,
        y: f32,
        modifiers: gpui::Modifiers,
    ) -> bool {
        if self.session.native_pointer_tool_active() {
            return false;
        }
        let Some(handle) = hit(self, x, y) else {
            return false;
        };
        self.session.select_native_hotspot(
            handle.occurrence.clone(),
            handle.hotspot.id.clone(),
            if modifiers.shift {
                SelectionMode::Add
            } else if modifiers.control {
                SelectionMode::Remove
            } else {
                SelectionMode::Replace
            },
        );
        // Authoring moves the shared marker; closed-instance dragging runs its operator.
        let authoring = self
            .session
            .native_hotspot_authoring_allowed(&handle.occurrence);
        if let Some(r) = self.cached_document.resolve_occurrence(&handle.occurrence)
            && let Some(inverse) = r
                .world_transform
                .inverse(meshcraft_core::geometry::GeometryTolerance::default())
        {
            let start_target = if authoring {
                handle.hotspot.position
            } else {
                r.edits
                    .and_then(|e| e.hotspot_positions.get(&handle.hotspot.id))
                    .copied()
                    .unwrap_or(handle.hotspot.position)
            };
            self.hotspot_drag = Some(Drag {
                start: (x, y),
                start_pick: self.cursor_snap.map_or(handle.world, |s| s.point),
                start_target,
                target: start_target,
                inverse,
                handle,
                authoring,
                moved: false,
            });
        }
        self.refresh_scene();
        true
    }

    pub(super) fn update_hotspot_drag(&mut self, x: f32, y: f32) {
        let Some(drag) = self.hotspot_drag.as_mut() else {
            return;
        };
        if (x - drag.start.0).hypot(y - drag.start.1) < 4. && !drag.moved {
            return;
        }
        let Some(snap) = self.cursor_snap else {
            return;
        };
        drag.moved = true;
        drag.target =
            drag.start_target + drag.inverse.transform_vector(snap.point - drag.start_pick);
        self.refresh_scene();
    }

    pub(super) fn finish_hotspot_drag(&mut self) -> bool {
        let Some(drag) = self.hotspot_drag.take() else {
            return false;
        };
        if drag.moved {
            let result = if drag.authoring {
                self.session.commit_native_hotspot_anchor(
                    &drag.handle.occurrence,
                    &drag.handle.hotspot.id,
                    drag.target,
                )
            } else {
                self.session.commit_native_hotspot_drag(
                    &drag.handle.occurrence,
                    &drag.handle.hotspot.id,
                    drag.target,
                )
            };
            if let Err(error) = result {
                self.session.set_native_status(error);
            }
        }
        // An aborted or clicked handle must restore the authoritative snapshot as well.
        self.cached_document_revision = None;
        self.refresh_scene();
        true
    }
}
