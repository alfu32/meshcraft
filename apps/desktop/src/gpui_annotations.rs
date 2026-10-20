//! Native labels and annotation properties; model edges remain in the 3D depth-tested pass.
use super::*;
use gpui_component::input::Input;
use meshcraft_core::document::annotations::Annotation;

#[derive(Default)]
pub(super) struct Ui {
    key: Option<(u64, u64, u64, String)>,
    text: Option<Entity<InputState>>,
    size: Option<Entity<InputState>>,
    subscriptions: Vec<gpui::Subscription>,
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let key = (
        shell.session.document_generation(),
        shell.session.native_selection_revision(),
        shell.session.document_revision(),
        shell.session.active_tool_label().to_owned(),
    );
    if shell.annotations.key.as_ref() == Some(&key) {
        return;
    }
    shell.annotations.key = Some(key);
    let (mut text, mut size) = shell.session.native_annotation_defaults();
    if let Some(a) = shell.session.native_selected_annotation() {
        text = a
            .record
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into();
        size = a.record.get("size").and_then(Value::as_f64).unwrap_or(size);
    }
    for (field, value) in [
        ("annotation_label", text),
        ("annotation_size", size.to_string()),
    ] {
        let previous = if field == "annotation_label" {
            shell.annotations.text.clone()
        } else {
            shell.annotations.size.clone()
        };
        if let Some(input) = previous {
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        } else {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            shell.annotations.subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::Focus => this.settings.active = Some(field),
                        InputEvent::Blur => {
                            apply(this, cx);
                            this.settings.active = None;
                        }
                        InputEvent::PressEnter { .. } => {
                            apply(this, cx);
                            this.settings.active = None;
                            window.focus(&this.focus);
                        }
                        _ => {}
                    }
                    cx.notify();
                },
            ));
            if field == "annotation_label" {
                shell.annotations.text = Some(input);
            } else {
                shell.annotations.size = Some(input);
            }
        }
    }
}
fn apply(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    let text = shell
        .annotations
        .text
        .as_ref()
        .unwrap()
        .read(cx)
        .value()
        .to_string();
    let size = shell
        .annotations
        .size
        .as_ref()
        .unwrap()
        .read(cx)
        .value()
        .parse::<f64>();
    let Ok(size) = size else {
        shell
            .session
            .set_native_status("Text size must be a number.".into());
        return;
    };
    let mut payload = CommandPayload::default();
    payload.values.insert("text".into(), json!(text));
    payload.values.insert("size".into(), json!(size));
    shell.dispatch_payload("annotation.update", &payload);
    shell.annotations.key = None;
}
pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected = shell.session.native_selected_annotation().is_some();
    div().flex().flex_col().gap(px(6.))
        .child(if selected { "Selected annotation" } else { "Defaults for new annotations" })
        .child("Label (empty = measured dimension)")
        .when_some(shell.annotations.text.as_ref(), |row, input| row.child(Input::new(input)))
        .child("Text size (model units)")
        .when_some(shell.annotations.size.as_ref(), |row, input| row.child(Input::new(input)))
        .child(Button::new("annotations/apply").small().label("Apply Properties").on_click(cx.listener(|this, _, window, cx| { apply(this, cx); this.settings.active = None; window.focus(&this.focus); cx.notify(); })))
        .child(div().text_xs().text_color(rgb(0xa5adb7)).child("Dim: two endpoints, then offset. Leader: anchor, elbow, label. Text: one placement point. Backspace removes a pending point."))
        .into_any_element()
}
fn font_size(annotation: &Annotation, camera: OrbitCamera, height: f32) -> f32 {
    let depth = if camera.orthographic {
        f64::from(camera.distance)
    } else {
        let forward = (camera.target - camera.position())
            .try_normalized(1e-12)
            .unwrap_or(-Vec3::Z);
        (annotation.position - camera.position())
            .dot(forward)
            .max(0.01)
    };
    let units = 2. * depth * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
        / f64::from(height.max(1.));
    ((annotation.size / units) as f32).round()
}
pub(super) fn label_rect(
    annotation: &Annotation,
    camera: OrbitCamera,
    size: (f32, f32),
) -> Option<[f32; 4]> {
    let (x, y) = project_point(camera, size, annotation.position)?;
    let font = font_size(annotation, camera, size.1).clamp(8., 128.);
    let width = annotation
        .label
        .lines()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(1) as f32
        * font
        * 0.65
        + 8.;
    let height = annotation.label.lines().count().max(1) as f32 * font * 1.4 + 4.;
    Some([x + 4., y - height - 4., x + width + 4., y - 4.])
}
pub(super) fn labels(shell: &MeshcraftShell) -> Vec<gpui::AnyElement> {
    let size = (shell.layout.width as f32, shell.layout.height as f32);
    let scale = shell.layout.scale;
    let policy = shell.session.native_selection_display_policy();
    shell
        .cached_annotations
        .iter()
        .filter(|a| policy.can_draw_target(&shell.cached_document, &a.target))
        .filter_map(|a| {
            let rect = label_rect(a, shell.camera, size)?;
            Some(
                div()
                    .absolute()
                    .left(px(rect[0] / scale))
                    .top(px(rect[1] / scale))
                    .text_size(px(
                        font_size(a, shell.camera, size.1).clamp(8., 128.) / scale
                    ))
                    .text_color(rgb(if shell.session.native_selection_contains(&a.target) {
                        0xb34c0c
                    } else if shell.session.native_target_editable(&a.target) {
                        0x172839
                    } else {
                        0x718096
                    }))
                    .child(a.label.clone())
                    .into_any_element(),
            )
        })
        .collect()
}
pub(super) fn hit(shell: &MeshcraftShell, x: f32, y: f32) -> Option<SelectionTarget> {
    let size = (shell.layout.width as f32, shell.layout.height as f32);
    let policy = shell.session.native_selection_display_policy();
    shell.cached_annotations.iter().rev().find_map(|a| {
        if !policy.can_draw_target(&shell.cached_document, &a.target) {
            return None;
        }
        let label = label_rect(a, shell.camera, size).is_some_and(|r| rect_contains(r, x, y));
        let edge = a.edges.iter().any(|[a, b]| {
            let (Some(a), Some(b)) = (
                project_point(shell.camera, size, *a),
                project_point(shell.camera, size, *b),
            ) else {
                return false;
            };
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let t =
                (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0., 1.);
            (x - a.0 - t * dx).hypot(y - a.1 - t * dy) < 6. * shell.layout.scale
        });
        let target = shell
            .session
            .native_normalize_selection_target(a.target.clone());
        ((label || edge) && shell.session.native_target_editable(&target)).then_some(target)
    })
}
pub(super) fn draw(
    view: &mut impl RaylibDraw3D,
    scene: &RenderScene,
    camera: OrbitCamera,
    height: f32,
    selection: &lighting::SelectionIndex,
) {
    unsafe {
        raylib::ffi::rlDisableBackfaceCulling();
    }
    for annotation in scene.annotations.iter().filter(|a| {
        scene
            .display_policy
            .can_draw_target(&scene.document, &a.target)
    }) {
        let selected = selection.contains(&annotation.target);
        let color = if selected {
            Color::new(220, 100, 20, 255)
        } else {
            Color::new(24, 39, 53, 255)
        };
        let width = if selected {
            scene.thick_line_width
        } else {
            scene.normal_line_width
        };
        for edge in &annotation.edges {
            lighting::stroke(view, *edge, color, width, camera, height);
        }
        let mut arrows = annotation.arrow.into_iter().collect::<Vec<_>>();
        if matches!(
            annotation.target.entity,
            SelectedEntity::SemanticHandle {
                kind: meshcraft_core::selection::SelectionKind::Dimension,
                ..
            }
        ) && let Some([a, b]) = annotation.edges.last()
        {
            arrows.push([*a, *b]);
        }
        // A face-anchored arrowhead must remain legible instead of intersecting that face.
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlDisableDepthTest();
        }
        for [from, tip] in arrows {
            let forward = (camera.target - camera.position())
                .try_normalized(1e-12)
                .unwrap_or(-Vec3::Z);
            let vector = tip - from;
            let Some(direction) = (vector - forward * vector.dot(forward)).try_normalized(1e-12)
            else {
                continue;
            };
            let side = direction
                .cross(forward)
                .try_normalized(1e-12)
                .unwrap_or(Vec3::X);
            let depth = if camera.orthographic {
                f64::from(camera.distance)
            } else {
                (tip - camera.position()).dot(forward)
            };
            let units = 2. * depth * (f64::from(camera.field_of_view).to_radians() * 0.5).tan()
                / f64::from(height.max(1.));
            let arrow_size = (units * 10.).min(vector.length() * 0.25);
            let tip = tip - forward * units;
            for sign in [-1., 1.] {
                lighting::stroke(
                    view,
                    [
                        tip,
                        tip - direction * arrow_size + side * (sign * arrow_size * 0.4),
                    ],
                    color,
                    width,
                    camera,
                    height,
                );
            }
        }
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlEnableDepthTest();
        }
    }
    unsafe {
        raylib::ffi::rlDrawRenderBatchActive();
        raylib::ffi::rlEnableBackfaceCulling();
    }
}
