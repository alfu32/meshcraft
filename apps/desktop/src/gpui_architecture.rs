//! Native architecture properties and color drafts; profile controls are in the canvas.
use super::*;
use gpui_component::Colorize;
use gpui_component::{
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
};
use meshcraft_core::document::architecture::{Color as SurfaceColor, Element};

#[derive(Default)]
pub(super) struct Ui {
    key: Option<(u64, String)>,
    colors: BTreeMap<&'static str, Entity<ColorPickerState>>,
    dirty: bool,
    subscriptions: Vec<gpui::Subscription>,
    pub drag: Option<ProfileDrag>,
}
const COLORS: &[(&str, &str)] = &[
    ("wall_exterior_color", "Exterior"),
    ("wall_interior_color", "Interior"),
    ("slab_top_color", "Top"),
    ("slab_bottom_color", "Bottom"),
    ("slab_side_color", "Sides"),
    ("frame_color", "Frame"),
    ("glazing_color", "Glazing"),
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
    }
    let values = shell.session.native_architecture_payload();
    for &(field, _) in COLORS {
        let Some(color) = values
            .values
            .get(field)
            .and_then(|v| serde_json::from_value::<SurfaceColor>(v.clone()).ok())
        else {
            continue;
        };
        let c = color.rgba();
        let hex = format!("#{:02x}{:02x}{:02x}{:02x}", c.red, c.green, c.blue, c.alpha);
        let hsla = gpui::Hsla::parse_hex(&hex).unwrap_or_default();
        if let Some(picker) = shell.architecture.colors.get(field).cloned() {
            if !shell.architecture.dirty && picker.read(cx).value() != Some(hsla) {
                picker.update(cx, |p, cx| p.set_value(hsla, window, cx));
            }
        } else {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(hsla));
            shell.architecture.subscriptions.push(cx.subscribe(
                &picker,
                |this, _, event: &ColorPickerEvent, cx| {
                    if matches!(event, ColorPickerEvent::Change(_)) {
                        this.architecture.dirty = true;
                        cx.notify();
                    }
                },
            ));
            shell.architecture.colors.insert(field, picker);
        }
    }
}
fn apply_colors(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    let selected = inspected(shell);
    let prefix = selected.as_ref().map(|e| match e.element {
        Element::Wall(_) => "wall_",
        Element::Slab(_) => "slab_",
        Element::Frame(_) => "frame_",
    });
    let mut payload = CommandPayload::default();
    for &(key, _) in COLORS {
        if prefix.is_some_and(|p| !key.starts_with(p) && !(p == "frame_" && key == "glazing_color"))
        {
            continue;
        }
        if let Some(color) = shell
            .architecture
            .colors
            .get(key)
            .and_then(|p| p.read(cx).value())
        {
            let c: gpui::Rgba = color.into();
            payload
                .values
                .insert(key.into(), json!({"r":c.r,"g":c.g,"b":c.b,"a":c.a}));
        }
    }
    shell.dispatch_payload(
        if selected.is_some() {
            "architecture.update"
        } else {
            "architecture.settings"
        },
        &payload,
    );
    shell.architecture.dirty = false;
}
pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected = inspected(shell);
    let mut body = div().flex().flex_col().gap(px(7.));
    if let Some(e) = &selected {
        body = body.child(format!("Selected {}", e.element.name()));
        if !shell.session.native_architecture_editable(&e.target) {
            return body.child("Open the object's prototype to edit these properties, or unlock its type in Selection.").into_any_element();
        }
        if let Some(field) = settings::render_field(shell, "Architecture name") {
            body = body.child(field);
        }
    } else {
        body = body.child("Defaults for new architecture");
    }
    let wall = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Wall(_)));
    let slab = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Slab(_)));
    let frame = selected
        .as_ref()
        .is_none_or(|e| matches!(e.element, Element::Frame(_)));
    for (show, labels) in [
        (
            wall,
            &["Wall thickness", "Wall height", "Wall inclination"][..],
        ),
        (slab, &["Slab thickness"][..]),
        (frame, &["Frame depth", "Frame width"][..]),
    ] {
        if show {
            for label in labels {
                if let Some(field) = settings::render_field(shell, label) {
                    body = body.child(field);
                }
            }
        }
    }
    for &(key, label) in COLORS {
        let show = if key.starts_with("wall_") {
            wall
        } else if key.starts_with("slab_") {
            slab
        } else {
            frame
        };
        if show && let Some(picker) = shell.architecture.colors.get(key) {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label)
                    .child(ColorPicker::new(picker).small()),
            );
        }
    }
    if frame {
        let enabled = shell
            .session
            .native_architecture_payload()
            .values
            .get("glazing_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        body = body.child(
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
    body = body.child(
        Button::new("architecture/apply-colors")
            .small()
            .label("Apply Colors")
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
            body=body.child(div().text_xs().child("Drag the slab's blue profile controls in the canvas. Holes remain owned by this slab."));
        }
    }
    body.child(div().text_xs().text_color(rgb(0xa5adb7)).child("Add Hole cuts only its picked wall or slab. Windows and doors are independent objects and never cut an opening automatically.")).into_any_element()
}

/// A drag previews a private snapshot and commits once, leaving Escape and undo atomic.
pub(super) struct ProfileDrag {
    target: SelectionTarget,
    index: usize,
    start: (f32, f32),
    original: Vec3,
    point: Vec3,
    normal: Vec3,
    base: Arc<Document>,
    moved: bool,
}
fn profile(shell: &MeshcraftShell) -> Option<(SelectionTarget, Vec<Vec3>, Vec3)> {
    if shell.session.native_pointer_tool_active() {
        return None;
    }
    let selected = inspected(shell)?;
    let placed = shell
        .cached_document
        .architecture_element(&selected.target)?;
    if !shell.session.native_architecture_editable(&placed.target) {
        return None;
    }
    let Element::Slab(slab) = &placed.element else {
        return None;
    };
    let plane = placed.element.plane().ok()?;
    let normal = placed
        .transform
        .transform_vector(plane.u)
        .cross(placed.transform.transform_vector(plane.v))
        .try_normalized(1e-12)?;
    let points = slab
        .profile
        .points
        .iter()
        .map(|p| placed.transform.transform_point(*p))
        .collect();
    Some((placed.target, points, normal))
}
pub(super) fn controls(shell: &MeshcraftShell) -> Vec<gpui::AnyElement> {
    let Some((_, points, _)) = profile(shell) else {
        return Vec::new();
    };
    points
        .into_iter()
        .filter_map(|p| {
            let (x, y) = project_point(
                shell.camera,
                (shell.layout.width as f32, shell.layout.height as f32),
                p,
            )?;
            Some(
                div()
                    .absolute()
                    .left(px(x / shell.layout.scale - 5.))
                    .top(px(y / shell.layout.scale - 5.))
                    .size(px(10.))
                    .bg(rgb(0x39a8ff))
                    .border_1()
                    .border_color(rgb(0x102d46))
                    .into_any_element(),
            )
        })
        .collect()
}
impl MeshcraftShell {
    pub(super) fn begin_architecture_drag(&mut self, x: f32, y: f32) -> bool {
        let Some((target, points, normal)) = profile(self) else {
            return false;
        };
        let Some((index, original)) = points
            .into_iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let (hx, hy) = project_point(
                    self.camera,
                    (self.layout.width as f32, self.layout.height as f32),
                    p,
                )?;
                let distance = (hx - x).hypot(hy - y);
                (distance <= 10. * self.layout.scale).then_some((distance, i, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i, p)| (i, p))
        else {
            return false;
        };
        self.architecture.drag = Some(ProfileDrag {
            target,
            index,
            start: (x, y),
            original,
            point: original,
            normal,
            base: Arc::clone(&self.cached_document),
            moved: false,
        });
        true
    }
    pub(super) fn architecture_drag_snap(
        &self,
        origin: Vec3,
        direction: Vec3,
        snap: Option<SnapResult>,
    ) -> Option<SnapResult> {
        let Some(drag) = &self.architecture.drag else {
            return snap;
        };
        if let Some(s) = snap
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
        drag.point = snap.point - drag.normal * (snap.point - drag.original).dot(drag.normal);
        drag.moved = true;
        self.refresh_scene();
    }
    pub(super) fn preview_architecture_drag(&mut self) {
        let Some(drag) = &self.architecture.drag else {
            return;
        };
        if !drag.moved {
            return;
        }
        let mut preview = (*drag.base).clone();
        let result = preview.move_slab_vertex(&drag.target, drag.index, drag.point);
        let delta = drag.point - drag.original;
        self.feedback = Some(ToolFeedback {
            screen_position: egui::Pos2::new(drag.start.0, drag.start.1),
            text: format!(
                "Slab profile · Δ ({:.3}, {:.3}, {:.3}) · {:.3} {}{}",
                delta.x,
                delta.y,
                delta.z,
                delta.length(),
                preview.model_settings().unit_name,
                if result.is_ok() {
                    ""
                } else {
                    " · invalid outline"
                }
            ),
        });
        self.cached_document = if result.is_ok() {
            Arc::new(preview)
        } else {
            Arc::clone(&drag.base)
        };
    }
    pub(super) fn finish_architecture_drag(&mut self) -> bool {
        let Some(drag) = self.architecture.drag.take() else {
            return false;
        };
        if drag.moved
            && let Err(error) =
                self.session
                    .commit_native_slab_control(&drag.target, drag.index, drag.point)
        {
            self.session.set_native_status(error);
        }
        self.cached_document_revision = None;
        self.refresh_scene();
        true
    }
}
