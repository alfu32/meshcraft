//! Native architecture properties and color drafts; profile controls are in the canvas.
use super::*;

use gpui_component::{
    checkbox::Checkbox,
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use meshcraft_core::document::architecture::{Control, Element};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Ui {
    key: Option<(u64, String)>,
    colors: BTreeMap<&'static str, Entity<SelectState<Vec<MaterialChoice>>>>,
    dirty: bool,
    changed_colors: BTreeSet<&'static str>,
    subscriptions: Vec<gpui::Subscription>,
    pub drag: Option<ProfileDrag>,
}
#[derive(Clone)]
struct MaterialChoice {
    id: String,
    label: String,
}
impl SelectItem for MaterialChoice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.id
    }
}
const COLORS: &[(&str, &str)] = &[
    ("wall_exterior_material", "Exterior"),
    ("wall_interior_material", "Interior"),
    ("slab_top_material", "Top"),
    ("slab_bottom_material", "Bottom"),
    ("slab_side_material", "Sides"),
    ("frame_material", "Material"),
    ("glazing_material", "Glazing"),
];
fn inspected(
    shell: &MeshcraftShell,
) -> Option<meshcraft_core::document::architecture::PlacedElement> {
    shell
        .session
        .native_selected_architecture()
        .filter(|_| shell.session.active_tool_label() == "Select")
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let selected = inspected(shell);
    let key = (
        shell.session.document_generation(),
        selected
            .as_ref()
            .map(|e| format!("{:?}", e.target))
            .unwrap_or_else(|| format!("defaults/{}", shell.session.active_tool_label())),
    );
    if shell.architecture.key.as_ref() != Some(&key) {
        shell.architecture.key = Some(key);
        shell.architecture.dirty = false;
        shell.architecture.changed_colors.clear();
    }
    let values = shell.session.native_architecture_payload();
    let catalog = shell.cached_document.materials();
    for &(field, _) in COLORS {
        let selected = values
            .values
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned);
        let choices = catalog
            .iter()
            .map(|m| MaterialChoice {
                id: m.id.clone(),
                label: m.name.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(select) = shell.architecture.colors.get(field).cloned() {
            select.update(cx, |s, cx| {
                s.set_items(choices, window, cx);
                if !shell.architecture.dirty {
                    s.set_selected_value(&selected.clone().unwrap_or_default(), window, cx);
                }
            });
        } else {
            let index = selected
                .and_then(|id| catalog.iter().position(|m| m.id == id))
                .map(|r| gpui_component::IndexPath::default().row(r));
            let select = cx.new(|cx| SelectState::new(choices, index, window, cx));
            shell.architecture.subscriptions.push(cx.subscribe(
                &select,
                move |this, _, event: &SelectEvent<Vec<MaterialChoice>>, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) {
                        this.architecture.dirty = true;
                        this.architecture.changed_colors.insert(field);
                        cx.notify();
                    }
                },
            ));
            shell.architecture.colors.insert(field, select);
        }
    }
}
fn apply_colors(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    let mut payload = CommandPayload::default();
    for field in &shell.architecture.changed_colors {
        if let Some(id) = shell
            .architecture
            .colors
            .get(field)
            .and_then(|s| s.read(cx).selected_value())
            .cloned()
        {
            payload.values.insert((*field).into(), json!(id));
        }
    }
    shell.dispatch_payload(
        if inspected(shell).is_some() {
            "architecture.update"
        } else {
            "architecture.settings"
        },
        &payload,
    );
    shell.architecture.dirty = false;
    shell.architecture.changed_colors.clear();
}

pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected = inspected(shell);
    let owners = if selected.is_some() {
        shell.session.native_selected_architectures()
    } else {
        vec![]
    };
    let mut body = div().flex().flex_col().gap(px(7.));
    if let Some(e) = &selected {
        body = body.child(if owners.len() > 1 {
            format!("Selected {} architectural elements", owners.len())
        } else {
            format!("Selected {}", e.element.name())
        });
        if owners
            .iter()
            .all(|owner| !shell.session.native_architecture_editable(&owner.target))
        {
            return body.child("Open the object's prototype to edit these properties, or unlock its type in Selection.").into_any_element();
        }
        if let Some(field) = settings::render_field(shell, "Architecture name") {
            body = body.child(field);
        }
    } else {
        body = body.child("Defaults for new architecture");
    }
    body = body
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xa5adb7))
                .child("Values are internal; measurements use the configured scale."),
        )
        .child(settings::architecture_table_header());
    for (title, prefix, labels) in [
        (
            "Walls",
            "wall_",
            &[
                "Wall thickness",
                "Wall start height",
                "Wall end height",
                "Wall axis offset",
                "Wall inclination",
            ][..],
        ),
        ("Slabs", "slab_", &["Slab thickness"][..]),
        ("Frames", "frame_", &["Frame depth", "Frame width"][..]),
    ] {
        let show = owners.is_empty()
            || owners.iter().any(|e| {
                matches!(
                    (&e.element, prefix),
                    (Element::Wall(_), "wall_")
                        | (Element::Slab(_), "slab_")
                        | (Element::Frame(_), "frame_")
                )
            });
        if !show {
            continue;
        }
        let mut section = div().flex().flex_col().gap(px(6.)).child(
            div()
                .mt(px(4.))
                .pb(px(3.))
                .border_b_1()
                .border_color(rgb(0x48515c))
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        );
        let readonly = owners.iter().any(|owner| {
            let matching = matches!(
                (&owner.element, prefix),
                (Element::Wall(_), "wall_")
                    | (Element::Slab(_), "slab_")
                    | (Element::Frame(_), "frame_")
            );
            matching && !shell.session.native_architecture_editable(&owner.target)
        });
        if readonly {
            body = body.child(section.child(div().text_xs().child(
                "Unlock the selected type or open its prototype to edit these properties.",
            )));
            continue;
        }
        for label in labels {
            if let Some(field) = settings::render_field(shell, label) {
                section = section.child(field);
            }
        }
        if prefix == "wall_" {
            let enabled = shell
                .session
                .native_architecture_payload()
                .values
                .get("wall_auto_join")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            section = section.child(
                Checkbox::new("architecture/auto-join")
                    .label("Autojoin")
                    .checked(enabled)
                    .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                        let mut payload = CommandPayload::default();
                        payload
                            .values
                            .insert("wall_auto_join".into(), json!(*enabled));
                        this.dispatch_payload(
                            if inspected(this).is_some() {
                                "architecture.update"
                            } else {
                                "architecture.settings"
                            },
                            &payload,
                        );
                        cx.notify();
                    })),
            );
        }
        for &(key, label) in COLORS {
            if (key.starts_with(prefix) || (prefix == "frame_" && key == "glazing_material"))
                && let Some(picker) = shell.architecture.colors.get(key)
            {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(label)
                        .child(Select::new(picker).small().placeholder("Choose material")),
                );
            }
        }
        if prefix == "frame_" {
            let enabled = shell
                .session
                .native_architecture_payload()
                .values
                .get("glazing_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            section = section.child(
                Checkbox::new("architecture/glazing")
                    .label("Glazing")
                    .checked(enabled)
                    .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                        let mut payload = CommandPayload::default();
                        payload
                            .values
                            .insert("glazing_enabled".into(), json!(*enabled));
                        this.dispatch_payload(
                            if inspected(this).is_some() {
                                "architecture.update"
                            } else {
                                "architecture.settings"
                            },
                            &payload,
                        );
                        cx.notify();
                    })),
            );
        }
        body = body.child(section);
    }
    body = body.child(
        Button::new("architecture/apply-colors")
            .small()
            .label("Apply Materials")
            .disabled(!shell.architecture.dirty)
            .on_click(cx.listener(|this, _, _, cx| {
                apply_colors(this, cx);
                cx.notify();
            })),
    );
    if let Some(e) = &selected {
        for hole in e.element.holes() {
            let target = e.target.clone();
            let id = hole.id.clone();
            let name = if hole.name.is_empty() {
                "Opening"
            } else {
                &hole.name
            };
            body = body.child(
                Button::new(SharedString::from(format!("architecture/hole/{id}")))
                    .small()
                    .label(format!("Delete {name}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let mut payload = CommandPayload::default();
                        if let SelectedEntity::SemanticHandle { id, .. } = &target.entity {
                            payload.values.insert("id".into(), json!(id));
                        }
                        payload
                            .values
                            .insert("occurrence".into(), json!(target.occurrence));
                        payload.values.insert("hole_id".into(), json!(id));
                        this.dispatch_payload("architecture.remove_hole", &payload);
                        cx.notify();
                    })),
            );
        }
        if matches!(e.element, Element::Slab(_)) {
            body=body.child(div().text_xs().child("Drag blue profile controls freely in 3D; snap to geometry or guides. Wireframe previews commit on release. Orange controls resize openings."));
        }
    }
    if let Some(e) = &selected {
        let hint = match e.element {
            Element::Wall(_) => {
                "Blue base controls move freely. Magenta triangles adjust each end's height."
            }
            Element::Frame(_) => "Drag the two purple corners on the definition plane to resize.",
            Element::Slab(_) => "",
        };
        if !hint.is_empty() {
            body = body.child(div().text_xs().child(hint));
        }
    }
    body.child(div().text_xs().text_color(rgb(0xa5adb7)).child("Add Hole cuts only its picked wall or slab. Windows and doors are independent objects and never cut an opening automatically.")).into_any_element()
}

/// A drag holds one owner and emits a wireframe; the document is committed once.
pub(super) struct ProfileDrag {
    target: SelectionTarget,
    control: Control,
    axis: Option<Vec3>,
    start: (f32, f32),
    original: Vec3,
    point: Vec3,
    normal: Vec3,
    element: Element,
    transform: meshcraft_core::geometry::Affine3,
    candidate: Option<Element>,
    preview_point: Option<Vec3>,
    ghost: Option<ToolPreview>,
    free: bool,
    moved: bool,
}
struct Handle {
    target: SelectionTarget,
    control: Control,
    point: Vec3,
    normal: Vec3,
    axis: Option<Vec3>,
}
fn handles(shell: &MeshcraftShell) -> Vec<Handle> {
    if shell.session.native_pointer_tool_active() {
        return vec![];
    }
    let selected = shell.session.native_selection_targets();
    let mut owners = BTreeMap::new();
    for target in selected {
        let placed = shell
            .cached_document
            .architecture_element(&target)
            .or_else(|| {
                if target.entity != SelectedEntity::Occurrence {
                    return None;
                }
                shell
                    .cached_document
                    .architecture_elements()
                    .into_iter()
                    .find(|p| {
                        p.target.occurrence == target.occurrence
                            && matches!(p.element, Element::Frame(_))
                    })
            });
        if let Some(p) = placed
            && shell.session.native_architecture_editable(&p.target)
        {
            owners.insert(p.target.clone(), p);
        }
    }
    owners
        .into_values()
        .map(|mut placed| {
            if let Some(drag) = shell.architecture.drag.as_ref()
                && drag.target == placed.target
                && let Some(candidate) = &drag.candidate
            {
                placed.element = candidate.clone();
            }
            placed
        })
        .flat_map(|placed| {
            placed
                .element
                .controls()
                .unwrap_or_default()
                .into_iter()
                .filter_map(move |p| {
                    Some(Handle {
                        target: placed.target.clone(),
                        control: p.control,
                        point: placed.transform.transform_point(p.point),
                        normal: placed.transform.transform_normal(
                            p.normal,
                            meshcraft_core::geometry::GeometryTolerance::default(),
                        )?,
                        axis: p.axis.map(|a| placed.transform.transform_vector(a)),
                    })
                })
        })
        .collect()
}
pub(super) fn controls(shell: &MeshcraftShell) -> Vec<gpui::AnyElement> {
    handles(shell)
        .into_iter()
        .filter_map(|p| {
            let (x, y) = project_point(
                shell.camera,
                (shell.layout.width as f32, shell.layout.height as f32),
                p.point,
            )?;
            let color = match p.control {
                Control::WallTop { .. } => 0xe256d7,
                Control::HoleCorner { .. } => 0xffa13a,
                Control::FrameCorner { .. } => 0xb68aff,
                _ => 0x39a8ff,
            };
            Some(
                div()
                    .absolute()
                    .left(px(x / shell.layout.scale - 5.))
                    .top(px(y / shell.layout.scale - 5.))
                    .size(px(10.))
                    .when(matches!(p.control, Control::WallBase { end: true }), |v| {
                        v.rounded_full()
                    })
                    .when(!matches!(p.control, Control::WallTop { .. }), |v| {
                        v.bg(rgb(color)).border_1().border_color(rgb(0x102d46))
                    })
                    .when(matches!(p.control, Control::WallTop { .. }), |v| {
                        v.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let mut path = gpui::PathBuilder::fill();
                                    path.move_to(gpui::point(bounds.center().x, bounds.top()));
                                    path.line_to(gpui::point(bounds.right(), bounds.bottom()));
                                    path.line_to(gpui::point(bounds.left(), bounds.bottom()));
                                    path.close();
                                    if let Ok(path) = path.build() {
                                        window.paint_path(path, rgb(color));
                                    }
                                },
                            )
                            .size_full(),
                        )
                    })
                    .into_any_element(),
            )
        })
        .collect()
}
// A grid seen exactly edge-on cannot define free pointer movement; use the
// view plane only in that case, after real grid/geometry/guide snapping.
fn free_control_fallback(
    origin: Vec3,
    direction: Vec3,
    original: Vec3,
    grid_normal: Vec3,
    view_normal: Vec3,
) -> Option<SnapResult> {
    let normal = if direction.dot(grid_normal).abs() < 1e-8 {
        view_normal.try_normalized(1e-12)?
    } else {
        grid_normal
    };
    let denominator = direction.dot(normal);
    if denominator.abs() < 1e-10 {
        return None;
    }
    let t = (original - origin).dot(normal) / denominator;
    (t >= 0. && t.is_finite()).then_some(SnapResult {
        point: origin + direction * t,
        normal,
        kind: SnapKind::Free,
        screen_distance: 0.,
    })
}

impl MeshcraftShell {
    pub(super) fn begin_architecture_drag(
        &mut self,
        x: f32,
        y: f32,
        _modifiers: gpui::Modifiers,
    ) -> bool {
        let handle = handles(self)
            .into_iter()
            .filter_map(|p| {
                let (hx, hy) = project_point(
                    self.camera,
                    (self.layout.width as f32, self.layout.height as f32),
                    p.point,
                )?;
                let distance = (hx - x).hypot(hy - y);
                (distance <= 10. * self.layout.scale).then_some((distance, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p);
        let Some(p) = handle else {
            return false;
        };
        let free = matches!(
            p.control,
            Control::WallBase { .. } | Control::SlabVertex { .. }
        );
        let normal = if free {
            self.session
                .native_ui_preferences()
                .grid_normal_axis
                .vector()
        } else {
            p.normal
        };
        let Some(placed) = self.cached_document.architecture_element(&p.target) else {
            return false;
        };
        self.architecture.drag = Some(ProfileDrag {
            target: p.target,
            control: p.control,
            axis: p.axis,
            start: (x, y),
            original: p.point,
            point: p.point,
            normal,
            element: placed.element,
            transform: placed.transform,
            candidate: None,
            preview_point: None,
            ghost: None,
            free,
            moved: false,
        });
        true
    }
    pub(super) fn architecture_drag_excluded_target(&self) -> Option<&SelectionTarget> {
        self.architecture
            .drag
            .as_ref()
            .filter(|d| d.free)
            .map(|d| &d.target)
    }
    pub(super) fn architecture_drag_fallback(
        &self,
        origin: Vec3,
        direction: Vec3,
        snap: Option<SnapResult>,
    ) -> Option<SnapResult> {
        let Some(drag) = self.architecture.drag.as_ref().filter(|d| d.free) else {
            return snap;
        };
        if snap.is_some_and(|s| s.kind != SnapKind::Free) {
            return snap;
        }
        // No reference was captured: preserve elevation on a plane parallel to
        // the active grid. Actual ground-grid/guide snaps can change elevation.
        free_control_fallback(
            origin,
            direction,
            drag.original,
            drag.normal,
            self.camera.target - self.camera.position(),
        )
        .or(snap)
    }
    pub(super) fn architecture_drag_snap(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        snap: Option<SnapResult>,
    ) -> Option<SnapResult> {
        let Some(drag) = &self.architecture.drag else {
            return snap;
        };
        // Free controls use the real geometry/grid/guide candidates first. Do not
        // turn a drag fallback plane into a surface lattice for helper snapping.
        if drag.free {
            return snap;
        }
        if let Some(s) = snap
            && !drag.free
            && (s.point - drag.original).dot(drag.normal).abs() < 1e-6
            && matches!(
                s.kind,
                SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Line
            )
        {
            return Some(s);
        }
        let d = direction.dot(drag.normal);
        if d.abs() < 1e-10 {
            return None;
        }
        let t = (drag.original - origin).dot(drag.normal) / d;
        (t >= 0. && t.is_finite()).then_some(SnapResult {
            point: origin + direction * t,
            normal: drag.normal,
            kind: SnapKind::Face,
            screen_distance: 0.,
        })
    }
    pub(super) fn update_architecture_drag(&mut self, x: f32, y: f32) {
        let Some(drag) = self.architecture.drag.as_mut() else {
            return;
        };
        if (x - drag.start.0).hypot(y - drag.start.1) < 4. && !drag.moved {
            return;
        }
        let Some(snap) = self.cursor_snap else {
            return;
        };
        drag.point = if drag.free {
            snap.point
        } else {
            snap.point - drag.normal * (snap.point - drag.original).dot(drag.normal)
        };
        if let Some(axis) = drag.axis {
            drag.point = drag.original
                + axis * ((drag.point - drag.original).dot(axis) / axis.length_squared());
        }
        drag.moved = true;
        self.refresh_scene();
    }
    pub(super) fn preview_architecture_drag(&mut self) -> Option<ToolPreview> {
        let drag = self.architecture.drag.as_mut().filter(|d| d.moved)?;
        if drag.preview_point != Some(drag.point) {
            let result = drag
                .transform
                .inverse(meshcraft_core::geometry::GeometryTolerance::default())
                .ok_or_else(|| "Architecture transform is singular.".to_owned())
                .and_then(|inverse| {
                    drag.element
                        .with_control_at(&drag.control, inverse.transform_point(drag.point))
                })
                .and_then(|element| element.mesh().map(|mesh| (element, mesh)));
            drag.preview_point = Some(drag.point);
            drag.ghost = Some(match result {
                Ok((element, mesh)) => {
                    drag.candidate = Some(element);
                    ToolPreview::Transform {
                        edges: mesh
                            .edges
                            .iter()
                            .map(|e| e.map(|i| drag.transform.transform_point(mesh.points[i])))
                            .collect(),
                    }
                }
                Err(_) => {
                    drag.candidate = None;
                    ToolPreview::Point {
                        position: drag.point,
                    }
                }
            });
        }
        let delta = (drag.point - drag.original) * self.cached_document.model_settings().unit_size;
        self.feedback = Some(ToolFeedback {
            screen_position: egui::Pos2::new(drag.start.0, drag.start.1),
            text: format!(
                "{} · Δ ({:.3}, {:.3}, {:.3}) · {:.3} {}{}",
                drag.control.label(),
                delta.x,
                delta.y,
                delta.z,
                delta.length(),
                self.cached_document.model_settings().unit_name,
                if drag.candidate.is_some() {
                    ""
                } else {
                    " · invalid geometry"
                }
            ),
        });
        drag.ghost.clone()
    }
    pub(super) fn finish_architecture_drag(&mut self) -> bool {
        let Some(drag) = self.architecture.drag.take() else {
            return false;
        };
        if drag.moved
            && let Err(error) = self.session.commit_native_architecture_control(
                &drag.target,
                &drag.control,
                drag.point,
            )
        {
            self.session.set_native_status(error);
        }
        self.refresh_scene();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn free_controls_preserve_grid_elevation_and_remain_draggable_in_edge_on_views() {
        let hit =
            free_control_fallback(Vec3::new(3., 10., 5.), -Vec3::Y, Vec3::Y, Vec3::Y, -Vec3::Z)
                .unwrap();
        assert_eq!(Vec3::new(3., 1., 5.), hit.point);
        let front = free_control_fallback(
            Vec3::new(3., 2., 10.),
            -Vec3::Z,
            Vec3::ZERO,
            Vec3::Y,
            -Vec3::Z,
        )
        .unwrap();
        assert_eq!(Vec3::new(3., 2., 0.), front.point);
    }
}
