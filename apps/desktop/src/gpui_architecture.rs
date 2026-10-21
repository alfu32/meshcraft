//! Native architecture properties and color drafts; profile controls are in the canvas.
use super::*;
use gpui_component::Colorize;
use gpui_component::{
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
};
use meshcraft_core::document::architecture::{Color as SurfaceColor, Control, Element};

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
            &[
                "Wall thickness",
                "Wall start height",
                "Wall end height",
                "Wall axis offset",
                "Wall inclination",
            ][..],
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
    if wall {
        let enabled = shell
            .session
            .native_architecture_payload()
            .values
            .get("wall_auto_join")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        body = body.child(
            Checkbox::new("architecture/auto-join")
                .label("Autojoin walls")
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
        body=body.child(div().text_xs().child("Blue base handles change length/elevation; Shift-drag moves in plan. Magenta top handles adjust each end's height."));
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
            body=body.child(div().text_xs().child("Drag blue controls in plan; Shift-drag changes their height to shape a roof. Orange controls resize owned openings."));
        }
    }
    if frame {
        body = body.child(
            div()
                .text_xs()
                .child("Drag the two purple frame corners on its definition plane to resize."),
        );
    }
    body.child(div().text_xs().text_color(rgb(0xa5adb7)).child("Add Hole cuts only its picked wall or slab. Windows and doors are independent objects and never cut an opening automatically.")).into_any_element()
}

/// A drag previews a private snapshot and commits once, leaving Escape and undo atomic.
pub(super) struct ProfileDrag {
    target: SelectionTarget,
    control: Control,
    axis: Option<Vec3>,
    start: (f32, f32),
    original: Vec3,
    point: Vec3,
    normal: Vec3,
    base: Arc<Document>,
    moved: bool,
}
struct Handle {
    target: SelectionTarget,
    control: Control,
    point: Vec3,
    normal: Vec3,
    alternate_normal: Option<Vec3>,
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
                        alternate_normal: p.alternate_normal.and_then(|n| {
                            placed.transform.transform_normal(
                                n,
                                meshcraft_core::geometry::GeometryTolerance::default(),
                            )
                        }),
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
                    .bg(rgb(color))
                    .border_1()
                    .border_color(rgb(0x102d46))
                    .into_any_element(),
            )
        })
        .collect()
}
impl MeshcraftShell {
    pub(super) fn begin_architecture_drag(
        &mut self,
        x: f32,
        y: f32,
        modifiers: gpui::Modifiers,
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
        let vertical_slab = modifiers.shift && matches!(p.control, Control::SlabVertex { .. });
        let normal = if vertical_slab {
            // A view-facing vertical plane keeps height dragging usable at every azimuth.
            let forward = self.camera.target - self.camera.position();
            let up = p.normal;
            (forward - up * forward.dot(up))
                .try_normalized(1e-9)
                .unwrap_or(p.alternate_normal.unwrap_or(Vec3::Z))
        } else if modifiers.shift {
            p.alternate_normal.unwrap_or(p.normal)
        } else {
            p.normal
        };
        self.architecture.drag = Some(ProfileDrag {
            target: p.target,
            control: p.control,
            axis: if vertical_slab {
                Some(p.normal)
            } else {
                p.axis
            },
            start: (x, y),
            original: p.point,
            point: p.point,
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
        if let Some(axis) = drag.axis {
            drag.point = drag.original
                + axis * ((drag.point - drag.original).dot(axis) / axis.length_squared());
        }
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
        let result = preview.move_architecture_control(&drag.target, &drag.control, drag.point);
        let delta = drag.point - drag.original;
        self.feedback = Some(ToolFeedback {
            screen_position: egui::Pos2::new(drag.start.0, drag.start.1),
            text: format!(
                "{} · Δ ({:.3}, {:.3}, {:.3}) · {:.3} {}{}",
                drag.control.label(),
                delta.x,
                delta.y,
                delta.z,
                delta.length(),
                preview.model_settings().unit_name,
                if result.is_ok() {
                    ""
                } else {
                    " · invalid geometry"
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
            && let Err(error) = self.session.commit_native_architecture_control(
                &drag.target,
                &drag.control,
                drag.point,
            )
        {
            self.session.set_native_status(error);
        }
        self.cached_document_revision = None;
        self.refresh_scene();
        true
    }
}
