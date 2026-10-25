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
    sync_key: Option<CacheKey>,
    catalog: Vec<MaterialChoice>,
    owners: Vec<meshcraft_core::document::architecture::PlacedElement>,
    any_editable: bool,
    readonly_kinds: BTreeSet<meshcraft_core::selection::SelectionKind>,
    pub(super) payload: CommandPayload,
    handles_key: Option<CacheKey>,
    handles: Arc<Vec<Handle>>,
    draft_handles: Option<(SelectionTarget, Vec3, Arc<Vec<Handle>>)>,
}
#[derive(Clone, PartialEq)]
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
    ("stair_tread_material", "Treads"),
    ("stair_support_material", "Support"),
    ("wall_exterior_material", "Exterior"),
    ("wall_interior_material", "Interior"),
    ("slab_top_material", "Top"),
    ("slab_bottom_material", "Bottom"),
    ("slab_side_material", "Sides"),
    ("frame_material", "Material"),
    ("glazing_material", "Glazing"),
];
#[derive(Clone, PartialEq)]
struct CacheKey {
    generation: u64,
    revision: u64,
    selection: u64,
    context: EditContext,
    policy: meshcraft_core::selection::SelectionDisplayPolicy,
    tool: String,
    pointer_tool: bool,
}
impl CacheKey {
    fn new(shell: &MeshcraftShell) -> Self {
        Self {
            generation: shell.session.document_generation(),
            revision: shell.session.document_revision(),
            selection: shell.session.native_selection_revision(),
            context: shell.session.native_edit_context(),
            policy: shell.session.native_selection_display_policy(),
            tool: shell.session.active_tool_label().into(),
            pointer_tool: shell.session.native_pointer_tool_active(),
        }
    }
}
fn inspected(
    shell: &MeshcraftShell,
) -> Option<&meshcraft_core::document::architecture::PlacedElement> {
    shell.architecture.owners.first()
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let sync_key = CacheKey::new(shell);
    if shell.architecture.sync_key.as_ref() == Some(&sync_key) {
        return;
    }
    shell.architecture.sync_key = Some(sync_key);
    let owners = if shell.session.active_tool_label() == "Select" {
        shell.session.native_selected_architectures()
    } else {
        vec![]
    };
    let selected = owners.first();
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
    shell.architecture.any_editable = false;
    shell.architecture.readonly_kinds.clear();
    for owner in &owners {
        if shell.session.native_architecture_editable(&owner.target) {
            shell.architecture.any_editable = true;
        } else {
            shell
                .architecture
                .readonly_kinds
                .insert(owner.element.kind());
        }
    }
    shell.architecture.owners = owners;
    shell.architecture.payload = values.clone();
    let catalog = shell
        .cached_document
        .materials()
        .into_iter()
        .map(|m| MaterialChoice {
            id: m.id,
            label: m.name,
        })
        .collect::<Vec<_>>();
    let catalog_changed = shell.architecture.catalog != catalog;
    shell.architecture.catalog = catalog.clone();
    for &(field, _) in COLORS {
        let selected = values
            .values
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned);
        let choices = catalog.clone();
        if let Some(select) = shell.architecture.colors.get(field).cloned() {
            select.update(cx, |s, cx| {
                if catalog_changed {
                    s.set_items(choices, window, cx);
                }
                if !shell.architecture.dirty
                    && (catalog_changed || s.selected_value() != selected.as_ref())
                {
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
    let applied = shell.session.native_architecture_payload();
    // Keep the draft if validation rejects an assignment; do not silently
    // reload old values and make a failed commit look successful.
    if payload
        .values
        .iter()
        .all(|(key, value)| applied.values.get(key) == Some(value))
    {
        shell.architecture.dirty = false;
        shell.architecture.changed_colors.clear();
    }
}

pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected = inspected(shell);
    let owners = &shell.architecture.owners;
    let mut body = div().flex().flex_col().gap(px(7.));
    if let Some(e) = &selected {
        body = body.child(if owners.len() > 1 {
            format!("Selected {} architectural elements", owners.len())
        } else {
            format!("Selected {}", e.element.name())
        });
        if !shell.architecture.any_editable {
            return body.child("Open the object's prototype to edit these properties, or unlock its type in Selection.").into_any_element();
        }
        if let Some(field) = settings::render_field(shell, "Architecture name") {
            body = body.child(field);
        }
    } else {
        body = body.child("Defaults for new architecture");
    }
    if selected.is_none()
        || shell.session.active_tool_label() == "Stair"
        || shell.session.native_stair_contour_ready()
    {
        body=body.child(div().text_sm().child(if shell.session.native_stair_contour_ready(){"Contour C ready. Select walking path P, then create the stair."}else{"Stair inputs: select contour C, then select walking path P. Object instances or connected edge selections are accepted."}))
            .child(div().flex().gap(px(6.)).flex_wrap()
                .child(Button::new("stair/use-contour").small().label("Contour C from Selection").on_click(cx.listener(|this,_,_,cx|{this.dispatch("architecture.stair_contour");cx.notify();})))
                .child(Button::new("stair/use-path").small().label("Walking Path P from Selection").disabled(!shell.session.native_stair_contour_ready()).on_click(cx.listener(|this,_,_,cx|{this.dispatch("architecture.stair_path");cx.notify();}))));
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
        (
            "Stairs",
            "stair_",
            &[
                "Stair height",
                "Stair steps",
                "Stair support thickness",
                "Stair guide spacing",
                "Stair guide levels",
            ][..],
        ),
        ("Frames", "frame_", &["Frame depth", "Frame width"][..]),
    ] {
        let show = owners.is_empty()
            || owners.iter().any(|e| {
                matches!(
                    (&e.element, prefix),
                    (Element::Wall(_), "wall_")
                        | (Element::Slab(_), "slab_")
                        | (Element::Frame(_), "frame_")
                        | (Element::Stair(_), "stair_")
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
        let kind = match prefix {
            "wall_" => meshcraft_core::selection::SelectionKind::Wall,
            "slab_" => meshcraft_core::selection::SelectionKind::Slab,
            "stair_" => meshcraft_core::selection::SelectionKind::Stair,
            _ => meshcraft_core::selection::SelectionKind::Frame,
        };
        let readonly = shell.architecture.readonly_kinds.contains(&kind);
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
        if prefix == "stair_" {
            for (key, label) in [
                ("stair_left", "Left guardrail guides"),
                ("stair_right", "Right guardrail guides"),
            ] {
                let enabled = shell
                    .architecture
                    .payload
                    .values
                    .get(key)
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                section = section.child(Checkbox::new(key).label(label).checked(enabled).on_click(
                    cx.listener(move |this, checked: &bool, _, cx| {
                        let mut payload = CommandPayload::default();
                        payload.values.insert(key.into(), json!(*checked));
                        this.dispatch_payload(
                            if inspected(this).is_some() {
                                "architecture.update"
                            } else {
                                "architecture.settings"
                            },
                            &payload,
                        );
                        cx.notify();
                    }),
                ));
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
            Element::Stair(_) => {
                "Drag the triangle handle to change stair height. Preview is a wireframe; release commits once."
            }
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
#[derive(Clone)]
pub(super) struct Handle {
    target: SelectionTarget,
    control: Control,
    point: Vec3,
    normal: Vec3,
    axis: Option<Vec3>,
}
fn build_handles(
    document: &Document,
    selected: &[SelectionTarget],
    editable: impl Fn(&SelectionTarget) -> bool,
) -> Vec<Handle> {
    if selected.len() != 1 {
        return vec![];
    }
    let mut owners = BTreeMap::new();
    for target in selected {
        let placed = document.architecture_element(target).or_else(|| {
            if target.entity != SelectedEntity::Occurrence {
                return None;
            }
            let r = document.resolve_occurrence(&target.occurrence)?;
            let id = r
                .definition
                .domain_entity_ids
                .get(meshcraft_core::document::architecture::FRAMES)?
                .first()?;
            document.architecture_element(&SelectionTarget {
                occurrence: target.occurrence.clone(),
                entity: SelectedEntity::SemanticHandle {
                    kind: meshcraft_core::selection::SelectionKind::Frame,
                    id: id.clone(),
                },
            })
        });
        if let Some(p) = placed
            && editable(&p.target)
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
                        axis: p.axis.map(|a| placed.transform.transform_vector(a)),
                    })
                })
        })
        .collect()
}
fn update_handles(
    ui: &mut Ui,
    key: CacheKey,
    document: &Document,
    selected: &[SelectionTarget],
    editable: impl Fn(&SelectionTarget) -> bool,
) {
    if ui.handles_key.as_ref() != Some(&key) {
        ui.handles = Arc::new(if key.pointer_tool {
            vec![]
        } else {
            build_handles(document, selected, editable)
        });
        ui.handles_key = Some(key);
        ui.draft_handles = None;
    }
}
/// Cache world-space controls independently of camera, pointer and UI redraws.
pub(super) fn handles(shell: &mut MeshcraftShell) -> Arc<Vec<Handle>> {
    let key = CacheKey::new(shell);
    let selected = shell.session.native_selection_targets();
    let session = &shell.session;
    update_handles(
        &mut shell.architecture,
        key,
        &shell.cached_document,
        &selected,
        |t| session.native_architecture_editable(t),
    );
    if selected.len() != 1 {
        return shell.architecture.handles.clone();
    }
    if let Some(drag) = shell.architecture.drag.as_ref()
        && let Some(candidate) = &drag.candidate
    {
        if let Some((target, point, handles)) = &shell.architecture.draft_handles
            && target == &drag.target
            && *point == drag.point
        {
            return handles.clone();
        }
        let mut handles = shell
            .architecture
            .handles
            .iter()
            .filter(|h| h.target != drag.target)
            .cloned()
            .collect::<Vec<_>>();
        handles.extend(
            candidate
                .controls()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| {
                    Some(Handle {
                        target: drag.target.clone(),
                        control: p.control,
                        point: drag.transform.transform_point(p.point),
                        normal: drag.transform.transform_normal(
                            p.normal,
                            meshcraft_core::geometry::GeometryTolerance::default(),
                        )?,
                        axis: p.axis.map(|a| drag.transform.transform_vector(a)),
                    })
                }),
        );
        let handles = Arc::new(handles);
        shell.architecture.draft_handles = Some((drag.target.clone(), drag.point, handles.clone()));
        return handles;
    }
    shell.architecture.draft_handles = None;
    shell.architecture.handles.clone()
}

/// Batched canvas markers: no per-handle GPUI elements, paths or layout work.
pub(super) fn draw(
    drawing: &mut impl RaylibDraw,
    handles: &[Handle],
    camera: OrbitCamera,
    size: (f32, f32),
    scale: f32,
) {
    let radius = 5. * scale;
    for handle in handles {
        let Some((x, y)) = project_point(camera, size, handle.point) else {
            continue;
        };
        if x < -radius || y < -radius || x > size.0 + radius || y > size.1 + radius {
            continue;
        }
        let color = match handle.control {
            Control::WallTop { .. } => Color::new(226, 86, 215, 255),
            Control::HoleCorner { .. } => Color::new(255, 161, 58, 255),
            Control::FrameCorner { .. } => Color::new(182, 138, 255, 255),
            _ => Color::new(57, 168, 255, 255),
        };
        let border = Color::new(16, 45, 70, 255);
        match handle.control {
            Control::WallBase { end: true } => {
                drawing.draw_circle(x as i32, y as i32, radius, color);
                drawing.draw_circle_lines(x as i32, y as i32, radius, border);
            }
            Control::WallTop { .. } | Control::StairHeight => drawing.draw_triangle(
                raylib::prelude::Vector2::new(x, y - radius),
                raylib::prelude::Vector2::new(x - radius, y + radius),
                raylib::prelude::Vector2::new(x + radius, y + radius),
                color,
            ),
            _ => {
                drawing.draw_rectangle(
                    (x - radius) as i32,
                    (y - radius) as i32,
                    (2. * radius) as i32,
                    (2. * radius) as i32,
                    color,
                );
                drawing.draw_rectangle_lines(
                    (x - radius) as i32,
                    (y - radius) as i32,
                    (2. * radius) as i32,
                    (2. * radius) as i32,
                    border,
                );
            }
        }
    }
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
            .iter()
            .cloned()
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

#[cfg(test)]
mod cache_tests {
    use super::*;
    use meshcraft_core::{
        document::architecture::Wall,
        selection::{SelectionDisplayPolicy, SelectionKind},
    };
    fn key(d: &Document) -> CacheKey {
        CacheKey {
            generation: 0,
            revision: d.revision(),
            selection: 1,
            context: EditContext::Document,
            policy: SelectionDisplayPolicy::default(),
            tool: "Select".into(),
            pointer_tool: false,
        }
    }
    #[test]
    fn controls_are_reused_until_geometry_selection_policy_or_tool_changes() {
        let mut d = Document::new();
        let definition = d.ensure_scene_root();
        let mut wall = Wall {
            start: Vec3::ZERO,
            end: Vec3::X * 5.,
            height: 3.,
            ..Default::default()
        };
        let id = d
            .put_architecture(definition, None, Element::Wall(wall.clone()))
            .unwrap();
        let target = SelectionTarget {
            occurrence: meshcraft_core::document::OccurrenceRef::scene_root(),
            entity: SelectedEntity::SemanticHandle {
                kind: SelectionKind::Wall,
                id: id.clone(),
            },
        };
        let mut ui = Ui::default();
        let calls = std::cell::Cell::new(0);
        let first_key = key(&d);
        update_handles(
            &mut ui,
            first_key.clone(),
            &d,
            std::slice::from_ref(&target),
            |_| {
                calls.set(calls.get() + 1);
                true
            },
        );
        let first = ui.handles.clone();
        assert_eq!(first.len(), 4);
        assert_eq!(calls.get(), 1);
        for _ in 0..100 {
            update_handles(
                &mut ui,
                first_key.clone(),
                &d,
                std::slice::from_ref(&target),
                |_| panic!("unchanged render must not rebuild controls"),
            );
            assert!(Arc::ptr_eq(&first, &ui.handles));
        }
        let mut multiple = first_key.clone();
        multiple.selection += 1;
        let mut other = target.clone();
        other.entity = SelectedEntity::Occurrence;
        update_handles(&mut ui, multiple, &d, &[target.clone(), other], |_| {
            panic!("multi-selection must skip owner/control evaluation");
        });
        assert!(ui.handles.is_empty());
        wall.height = 7.;
        d.put_architecture(definition, Some(&id), Element::Wall(wall))
            .unwrap();
        let changed = key(&d);
        update_handles(
            &mut ui,
            changed.clone(),
            &d,
            std::slice::from_ref(&target),
            |_| true,
        );
        assert!(!Arc::ptr_eq(&first, &ui.handles));
        assert_eq!(ui.handles.iter().map(|h| h.point.y).fold(0., f64::max), 7.);
        let mut locked = changed.clone();
        locked.policy.set_unlocked(SelectionKind::Wall, false);
        update_handles(&mut ui, locked, &d, std::slice::from_ref(&target), |_| {
            false
        });
        assert!(ui.handles.is_empty());
        let mut active = changed.clone();
        active.pointer_tool = true;
        active.tool = "Wall".into();
        update_handles(&mut ui, active, &d, std::slice::from_ref(&target), |_| true);
        assert!(ui.handles.is_empty());
        let mut cleared = changed;
        cleared.selection += 1;
        update_handles(&mut ui, cleared, &d, &[], |_| true);
        assert!(ui.handles.is_empty());
    }
}
