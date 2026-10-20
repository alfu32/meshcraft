//! Unified script IDE and an independent camera sharing the existing offscreen worker.
use super::*;
use crate::script_runtime::{Clock, Runtime};
use meshcraft_core::document::{DefinitionId, scripts::ScriptDefinition};
#[path = "gpui_script_api.rs"]
mod api;

#[derive(Default)]
pub(super) struct ScriptsUi {
    pub editor: Option<Editor>,
    runtime: Option<Runtime>,
    clock: Clock,
    failed_key: Option<u64>,
    pub errors: Vec<String>,
}
pub(super) struct Editor {
    id: Option<DefinitionId>,
    code: Entity<InputState>,
    bounds: Arc<Mutex<gpui::Bounds<gpui::Pixels>>>,
    completion: Option<api::Popup>,
    _subscription: gpui::Subscription,
    document: Arc<Document>,
    bridge: ViewportBridge,
    canvas: Entity<RaylibCanvas>,
    camera: OrbitCamera,
    drag: Option<(gpui::MouseButton, f32, f32)>,
    runtime: Runtime,
    clock: Clock,
    key: u64,
    running: bool,
    waiting: bool,
    valid: bool,
    error: Option<String>,
}
const EXAMPLE: &str = "// Shared helpers and constants can be defined outside the hooks.\nconst bodyMaterial = phong('#83bdeaff');\n\nfunction onCreate(instance, model) {\n  this.height = 2;\n  this.width = 3;\n}\n\nfunction onAnimationFrame(instance, model) {\n  const width = hotspot(new Point(this.width, 0, 0), 'width');\n  return [this.geometry,\n    cube(new Point(0, 0, 0),\n         new Point(Math.max(0.1, width.position.x), this.height, 1),\n         'body', bodyMaterial),\n    width];\n}\n";
fn key(shell: &MeshcraftShell) -> u64 {
    let mut value = shell
        .session
        .document_revision()
        .wrapping_add(shell.session.document_generation().wrapping_mul(1000000007));
    if let Some(drag) = &shell.hotspot_drag {
        for p in [drag.target.x, drag.target.y, drag.target.z] {
            value = value.wrapping_mul(31).wrapping_add(p.to_bits());
        }
    }
    value
}
pub(super) fn tick(shell: &mut MeshcraftShell) -> bool {
    let mut state = std::mem::take(&mut shell.scripts);
    let mut changed = false;
    if let Some(editor) = state.editor.as_mut() {
        if let Some(response) = editor.runtime.poll() {
            editor.waiting = false;
            if response.key == editor.key {
                if response.errors.is_empty() {
                    let mut document = (*editor.document).clone();
                    document.install_script_frames(response.frames);
                    let visual = document.visual_revision() != editor.document.visual_revision();
                    changed = visual || !editor.valid || editor.error.is_some();
                    editor.document = Arc::new(document);
                    editor.valid = true;
                    editor.error = None;
                    if visual {
                        publish_preview(editor, shell.lighting);
                    }
                } else {
                    editor.error = Some(response.errors.join("\n"));
                    editor.running = false;
                    editor.valid = false;
                    changed = true;
                }
            }
        }
        if editor.running
            && !editor.waiting
            && editor.clock.last.elapsed() >= Duration::from_millis(30)
        {
            let delta = editor.clock.last.elapsed().as_secs_f64();
            editor.clock.last = Instant::now();
            editor.runtime.submit(
                editor.document.clone(),
                editor.key,
                editor.clock.start.elapsed().as_secs_f64(),
                delta,
            );
            editor.waiting = true;
        }
    } else if !shell.cached_document.scripted_occurrences().is_empty() {
        let current = key(shell);
        let runtime = state.runtime.get_or_insert_with(Runtime::new);
        if let Some(response) = runtime.poll()
            && response.key == current
        {
            let errors_changed = state.errors != response.errors;
            state.errors = response.errors;
            state.failed_key =
                (!state.errors.is_empty() && response.frames.is_empty()).then_some(current);
            if shell.hotspot_drag.is_some() {
                let mut document = (*shell.cached_document).clone();
                document.install_script_frames(response.frames);
                shell.cached_document = Arc::new(document);
                changed = true;
            } else {
                let before = shell.session.native_visual_revision();
                shell.session.install_native_script_frames(response.frames);
                changed = before != shell.session.native_visual_revision();
            }
            changed |= errors_changed;
        }
        if state.failed_key != Some(current)
            && state.clock.last.elapsed() >= Duration::from_millis(30)
        {
            let delta = state.clock.last.elapsed().as_secs_f64();
            state.clock.last = Instant::now();
            let document = if shell.hotspot_drag.is_some() {
                shell.cached_document.clone()
            } else {
                Arc::new(shell.session.document_snapshot())
            };
            runtime.submit(
                document,
                current,
                state.clock.start.elapsed().as_secs_f64(),
                delta,
            );
        }
    } else {
        changed = !state.errors.is_empty();
        state.errors.clear();
        state.failed_key = None;
        state.runtime = None;
    }
    shell.scripts = state;
    changed
}
fn publish_preview(editor: &Editor, lighting: lighting::Lighting) {
    editor.bridge.publish_scene(RenderScene {
        document: editor.document.clone(),
        lighting,
        ..Default::default()
    });
    editor.bridge.publish_camera(editor.camera);
}
pub(super) fn edit(
    shell: &mut MeshcraftShell,
    new: bool,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let id = if new {
        None
    } else {
        shell.session.native_active_object().map(|x| x.0)
    };
    if !new && id.is_none() {
        shell
            .session
            .set_native_status("Select an object instance or prototype to edit its script.".into());
        return;
    }
    let source = id
        .and_then(|id| shell.cached_document.script(id))
        .unwrap_or_else(|| {
            if new {
                ScriptDefinition {
                    source: EXAMPLE.into(),
                    ..Default::default()
                }
            } else {
                ScriptDefinition::default()
            }
        });
    let mut source_document = shell.session.document_snapshot();
    let target = match id {
        Some(id) => id,
        None => {
            let Ok(path) = source_document.add_scripted_object("Preview", source.clone()) else {
                return;
            };
            let Some(resolved) = source_document.resolve_occurrence(&path) else {
                return;
            };
            resolved.definition.id
        }
    };
    let Ok((mut document, _)) = source_document.script_preview(target) else {
        return;
    };
    let _ = document.set_script(target, source.clone());
    let code = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor("javascript")
            .default_value(source.editor_source())
    });
    let subscription = cx.subscribe_in(&code, window, |this, _, event: &InputEvent, _, cx| {
        if matches!(event, InputEvent::Change)
            && let Some(editor) = this.scripts.editor.as_mut()
        {
            editor.valid = false;
            editor.running = false;
            editor.key = editor.key.wrapping_add(1);
            if let Some(popup) = editor.completion.as_mut() {
                popup.selected = 0;
            }
            cx.notify();
        }
    });
    let mut bridge = ViewportBridge::new();
    bridge.redraw = shell.bridge.redraw.clone();
    let canvas = cx.new(|_| RaylibCanvas {
        bridge: bridge.clone(),
        frame_watch_started: false,
        presented_image: None,
    });
    let editor = Editor {
        id,
        code: code.clone(),
        bounds: Arc::new(Mutex::new(gpui::Bounds::default())),
        completion: None,
        _subscription: subscription,
        document: Arc::new(document),
        bridge: bridge.clone(),
        canvas,
        camera: OrbitCamera::default(),
        drag: None,
        runtime: Runtime::with_query_model(Arc::new(source_document)),
        clock: Clock::default(),
        key: 1,
        running: true,
        waiting: false,
        valid: false,
        error: None,
    };
    publish_preview(&editor, shell.lighting);
    if let Ok(mut preview) = shell.bridge.preview.lock() {
        *preview = Some(bridge);
    }
    shell.scripts.editor = Some(editor);
    shell.command_palette_open = false;
    shell.open_menu = None;
    code.update(cx, |input, cx| input.focus(window, cx));
    cx.notify();
}
pub(super) fn close(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    shell.scripts.editor = None;
    if let Ok(mut preview) = shell.bridge.preview.lock() {
        *preview = None;
    }
    shell.bridge.request_redraw();
    window.focus(&shell.focus);
    cx.notify();
}
fn sources(editor: &Editor, cx: &Context<MeshcraftShell>) -> ScriptDefinition {
    ScriptDefinition {
        source: editor.code.read(cx).value().to_string(),
        ..Default::default()
    }
}
fn evaluate_draft(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) {
    if let Some(editor) = shell.scripts.editor.as_mut() {
        let source = sources(editor, cx);
        let mut document = (*editor.document).clone();
        if let Some(path) = document
            .occurrence_refs()
            .ok()
            .and_then(|p| p.first().cloned())
            && let Some(resolved) = document.resolve_occurrence(&path)
        {
            let _ = document.set_script(resolved.definition.id, source);
        }
        editor.document = Arc::new(document);
        editor.key = editor.key.wrapping_add(1);
        editor.running = true;
        editor.valid = false;
        editor.error = None;
        editor.completion = None;
    }
}
pub(super) fn capture_key(
    shell: &mut MeshcraftShell,
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    api::capture_key(shell, event, window, cx)
}
pub(super) fn overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let editor = shell.scripts.editor.as_ref()?;
    let font = cx.global::<Theme>().mono_font_family.clone();
    let measured = editor.bounds.clone();
    let code = div()
        .w(gpui::relative(0.53))
        .h_full()
        .min_w(px(0.))
        .relative()
        .child(
            Input::new(&editor.code)
                .size_full()
                .font_family(font)
                .text_size(px(14.)),
        )
        .child(
            canvas(
                move |bounds, _, _| {
                    if let Ok(mut current) = measured.lock() {
                        *current = bounds;
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
    let preview_bridge = editor.bridge.clone();
    let preview = div()
        .id("script/preview")
        .flex_1()
        .h_full()
        .min_w(px(0.))
        .relative()
        .overflow_hidden()
        .bg(rgb(0xe0edf7))
        .child(editor.canvas.clone())
        .child(
            canvas(
                move |bounds, window, _| {
                    let scale = window.scale_factor();
                    preview_bridge.publish_layout(ViewportLayout {
                        x: 0,
                        y: 0,
                        width: (f32::from(bounds.size.width) * scale).max(1.) as u32,
                        height: (f32::from(bounds.size.height) * scale).max(1.) as u32,
                        scale,
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if let Some(e) = this.scripts.editor.as_mut() {
                    e.drag = Some((
                        if event.modifiers.shift {
                            gpui::MouseButton::Middle
                        } else {
                            gpui::MouseButton::Right
                        },
                        event.position.x.into(),
                        event.position.y.into(),
                    ));
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            gpui::MouseButton::Middle,
            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                if let Some(e) = this.scripts.editor.as_mut() {
                    e.drag = Some((
                        gpui::MouseButton::Middle,
                        event.position.x.into(),
                        event.position.y.into(),
                    ));
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Right,
            cx.listener(|this, _, _, _| {
                if let Some(e) = this.scripts.editor.as_mut() {
                    e.drag = None;
                }
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Middle,
            cx.listener(|this, _, _, _| {
                if let Some(e) = this.scripts.editor.as_mut() {
                    e.drag = None;
                }
            }),
        )
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            if let Some(e) = this.scripts.editor.as_mut()
                && let Some((button, x, y)) = e.drag
            {
                if event.pressed_button.is_none() {
                    e.drag = None;
                    return;
                }
                let nx = f32::from(event.position.x);
                let ny = f32::from(event.position.y);
                let dx = nx - x;
                let dy = ny - y;
                if button == gpui::MouseButton::Right {
                    e.camera.orbit_by_drag(dx, dy);
                } else {
                    let forward = (e.camera.target - e.camera.position())
                        .try_normalized(1e-12)
                        .unwrap_or(Vec3::Z);
                    let right = forward
                        .cross(Vec3::Y)
                        .try_normalized(1e-12)
                        .unwrap_or(Vec3::X);
                    let up = right
                        .cross(forward)
                        .try_normalized(1e-12)
                        .unwrap_or(Vec3::Y);
                    e.camera.target += (right * (-f64::from(dx)) + up * f64::from(dy))
                        * f64::from(e.camera.distance * 0.0009375);
                }
                e.drag = Some((button, nx, ny));
                e.bridge.publish_camera(e.camera);
                cx.notify();
            }
            cx.stop_propagation();
        }))
        .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
            if let Some(e) = this.scripts.editor.as_mut() {
                let y = f32::from(event.delta.pixel_delta(px(20.)).y);
                e.camera.distance = (e.camera.distance * (-y * 0.006).exp()).clamp(0.25, 1e6);
                e.bridge.publish_camera(e.camera);
            }
            cx.stop_propagation();
        }));
    let buttons = div()
        .flex()
        .justify_end()
        .gap(px(8.))
        .child(
            Button::new("script/cancel")
                .label("Cancel")
                .on_click(cx.listener(|this, _, window, cx| close(this, window, cx))),
        )
        .child(
            Button::new("script/api")
                .label("API · Ctrl+Space")
                .on_click(cx.listener(|this, _, window, cx| {
                    api::open(this, window, cx);
                })),
        )
        .child(
            Button::new("script/preview-button")
                .label("Preview")
                .on_click(cx.listener(|this, _, _, cx| {
                    evaluate_draft(this, cx);
                    cx.notify();
                })),
        )
        .child(
            Button::new("script/save")
                .label("Save")
                .primary()
                .disabled(!editor.valid)
                .on_click(cx.listener(|this, _, window, cx| {
                    let Some(editor) = this.scripts.editor.as_ref() else {
                        return;
                    };
                    if !editor.valid {
                        return;
                    }
                    let source = sources(editor, cx);
                    let result = if let Some(id) = editor.id {
                        this.session.save_native_script(id, source)
                    } else {
                        this.session
                            .create_native_scripted_object(source)
                            .map(|_| ())
                    };
                    match result {
                        Ok(()) => {
                            close(this, window, cx);
                            this.refresh_scene();
                        }
                        Err(error) => {
                            if let Some(editor) = this.scripts.editor.as_mut() {
                                editor.error = Some(error);
                            }
                        }
                    }
                    cx.notify();
                })),
        );
    let mut modal = div().id("script/modal").absolute().inset_0().occlude().p(px(24.)).flex().flex_col().gap(px(10.)).bg(rgb(0x1b222b))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .capture_action(cx.listener(|this,_:&gpui_component::input::MoveUp,window,cx|{if api::navigate(this,"up",window,cx){cx.stop_propagation();}else{cx.propagate();}}))
        .capture_action(cx.listener(|this,_:&gpui_component::input::MoveDown,window,cx|{if api::navigate(this,"down",window,cx){cx.stop_propagation();}else{cx.propagate();}}))
        .capture_action(cx.listener(|this,_:&gpui_component::input::Enter,window,cx|{if api::navigate(this,"enter",window,cx){cx.stop_propagation();}else{cx.propagate();}}))
        .capture_action(cx.listener(|this,_:&gpui_component::input::IndentInline,window,cx|{if api::navigate(this,"tab",window,cx){cx.stop_propagation();}else{cx.propagate();}}))
        .capture_action(cx.listener(|this,_:&gpui_component::input::Escape,window,cx|{if api::navigate(this,"escape",window,cx){cx.stop_propagation();}else{cx.propagate();}}))
        .child(div().text_lg().child("Object Script Editor"))
        .child(div().text_sm().child("Define onCreate and onAnimationFrame here, along with your own helpers. Ctrl+Space: API help/completion. Preview: right-drag orbit · Shift+right / middle pan · wheel zoom."))
        .child(div().flex().flex_1().min_h(px(0.)).gap(px(12.)).child(code).child(preview))
        .when_some(editor.error.clone(), |e, error| e.child(div().text_color(rgb(0xff9999)).child(error)))
        .child(buttons);
    if let Some(popup) = api::render(shell, cx) {
        modal = modal.child(popup);
    }
    Some(modal.into_any_element())
}
