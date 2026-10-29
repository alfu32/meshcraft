//! Isolated procedural material/object form preview on the existing graphics owner.
use super::*;

pub(super) struct Preview {
    bridge: ViewportBridge,
    canvas: Entity<RaylibCanvas>,
    document: Arc<Document>,
    camera: OrbitCamera,
    drag: Option<(gpui::MouseButton, f32, f32)>,
    observed: Option<Result<Material, String>>,
    next_check: Instant,
    due: Option<Instant>,
    pending: Option<u64>,
    error: Option<String>,
}

pub(super) fn activate(shell: &MeshcraftShell) {
    if let Some(preview) = shell
        .materials
        .editor
        .as_ref()
        .and_then(|e| e.preview.as_ref())
    {
        if let Ok(mut active) = shell.bridge.preview.lock() {
            *active = Some(preview.bridge.clone());
        }
        shell.bridge.request_redraw();
    }
}
pub(super) fn detach(shell: &MeshcraftShell) {
    let Some(own) = shell
        .materials
        .editor
        .as_ref()
        .and_then(|e| e.preview.as_ref())
    else {
        return;
    };
    if let Ok(mut active) = shell.bridge.preview.lock()
        && active
            .as_ref()
            .is_some_and(|a| Arc::ptr_eq(&a.scene, &own.bridge.scene))
    {
        *active = None;
        shell.bridge.request_redraw();
    }
}
pub(super) fn open(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    let mut bridge = ViewportBridge::new();
    bridge.redraw = shell.bridge.redraw.clone();
    bridge.publish_layout(ViewportLayout {
        width: 640,
        height: 480,
        scale: 1.,
        ..Default::default()
    });
    let canvas = cx.new(|_| RaylibCanvas {
        bridge: bridge.clone(),
        frame_watch_started: false,
        presented_image: None,
    });
    let Some(editor) = shell.materials.editor.as_mut() else {
        return;
    };
    editor.preview = Some(Preview {
        bridge,
        canvas,
        document: Arc::new(Document::new()),
        camera: OrbitCamera::default(),
        drag: None,
        observed: None,
        next_check: Instant::now(),
        due: None,
        pending: None,
        error: None,
    });
    activate(shell);
    evaluate(shell, cx, true);
}
fn preview_document(source: &Document, material: &Material) -> Result<Document, String> {
    material.validate()?;
    let config = material
        .procedural
        .as_ref()
        .ok_or("Shader program is missing")?;
    let program = source
        .procedural_programs()
        .into_iter()
        .find(|p| p.id == config.program)
        .ok_or("Shader program is unavailable")?;
    program.resolve(&config.parameters, &BTreeMap::new())?;
    let base = config
        .base_material
        .as_ref()
        .and_then(|id| source.materials().into_iter().find(|m| &m.id == id));
    shader_editor::preview_document(&program, material, base.as_ref())
}

fn evaluate(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>, fit: bool) {
    let result = shell
        .materials
        .editor
        .as_ref()
        .ok_or("Shader settings are closed".to_owned())
        .and_then(|e| draft_material(e, cx));
    let document = result
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|material| preview_document(&shell.cached_document, material));
    let Some(preview) = shell
        .materials
        .editor
        .as_mut()
        .and_then(|e| e.preview.as_mut())
    else {
        return;
    };
    preview.observed = Some(result);
    preview.next_check = Instant::now() + Duration::from_millis(100);
    preview.due = None;
    match document {
        Ok(document) => {
            preview.document = Arc::new(document);
            if fit {
                preview.camera.fit(&preview.document);
            }
            preview.bridge.publish_scene(RenderScene {
                document: Arc::clone(&preview.document),
                lighting: shell.lighting,
                grid_visible: false,
                guides_visible: false,
                black_model_lines: false,
                ..Default::default()
            });
            preview.bridge.publish_camera(preview.camera);
            preview.pending = preview.bridge.redraw.0.lock().ok().map(|v| *v);
            preview.error = None;
        }
        Err(error) => {
            preview.error = Some(error);
            preview.pending = None;
        }
    }
}

/// Cheap form reads at 10 Hz, debounced scene publication; camera/animation never
/// rebuild a document. Invalid values retain the previous valid preview.
pub(super) fn tick(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> bool {
    let Some(preview) = shell
        .materials
        .editor
        .as_ref()
        .and_then(|e| e.preview.as_ref())
    else {
        return false;
    };
    let now = Instant::now();
    let check = now >= preview.next_check;
    let draft = check.then(|| draft_material(shell.materials.editor.as_ref().unwrap(), cx));
    let preview = shell
        .materials
        .editor
        .as_mut()
        .unwrap()
        .preview
        .as_mut()
        .unwrap();
    let mut changed = false;
    if let Some(draft) = draft {
        preview.next_check = now + Duration::from_millis(100);
        if preview.observed.as_ref() != Some(&draft) {
            preview.pending = None;
            match &draft {
                Ok(_) => preview.due = Some(now + Duration::from_millis(150)),
                Err(error) => {
                    preview.due = None;
                    preview.error = Some(error.clone());
                }
            }
            preview.observed = Some(draft);
            changed = true;
        }
    }
    if preview.due.is_some_and(|due| now >= due) {
        evaluate(shell, cx, false);
        return true;
    }
    if let Some(pending) = preview.pending
        && let Some(frame) = preview.bridge.latest_frame()
        && frame.request_revision >= pending
    {
        preview.pending = None;
        preview.error = (!frame.procedural_diagnostics.is_empty())
            .then(|| frame.procedural_diagnostics.join("\n"));
        changed = true;
    }
    changed
}

pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let Some(preview) = shell
        .materials
        .editor
        .as_ref()
        .and_then(|e| e.preview.as_ref())
    else {
        return div().into_any_element();
    };
    let bridge = preview.bridge.clone();
    let mut viewport = div()
        .id("object-settings-preview")
        .relative()
        .flex_1()
        .w_full()
        .min_h(px(0.))
        .overflow_hidden()
        .bg(rgb(0xe0edf7))
        .child(preview.canvas.clone())
        .child(
            canvas(
                move |bounds, window, _| {
                    let scale = window.scale_factor();
                    if f32::from(bounds.size.width) < 2. || f32::from(bounds.size.height) < 2. {
                        return;
                    }
                    bridge.publish_layout(ViewportLayout {
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
            .inset_0()
            .size_full(),
        );
    for button in [gpui::MouseButton::Right, gpui::MouseButton::Middle] {
        viewport = viewport
            .on_mouse_down(
                button,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if let Some(p) = this
                        .materials
                        .editor
                        .as_mut()
                        .and_then(|e| e.preview.as_mut())
                    {
                        p.drag = Some((
                            if event.modifiers.shift {
                                gpui::MouseButton::Middle
                            } else {
                                button
                            },
                            event.position.x.into(),
                            event.position.y.into(),
                        ));
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                button,
                cx.listener(|this, _, _, cx| {
                    if let Some(p) = this
                        .materials
                        .editor
                        .as_mut()
                        .and_then(|e| e.preview.as_mut())
                    {
                        p.drag = None;
                    }
                    cx.stop_propagation();
                }),
            );
    }
    viewport = viewport
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            if let Some(p) = this
                .materials
                .editor
                .as_mut()
                .and_then(|e| e.preview.as_mut())
                && let Some((button, x, y)) = p.drag
            {
                if event.pressed_button.is_none() {
                    p.drag = None;
                    return;
                }
                let nx = f32::from(event.position.x);
                let ny = f32::from(event.position.y);
                let dx = nx - x;
                let dy = ny - y;
                if button == gpui::MouseButton::Right {
                    p.camera.orbit_by_drag(dx, dy);
                } else {
                    let forward = (p.camera.target - p.camera.position())
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
                    p.camera.target += (right * (-f64::from(dx)) + up * f64::from(dy))
                        * f64::from(p.camera.distance * 0.0009375);
                }
                p.drag = Some((button, nx, ny));
                p.bridge.publish_camera(p.camera);
                cx.notify();
            }
            cx.stop_propagation();
        }))
        .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
            if let Some(p) = this
                .materials
                .editor
                .as_mut()
                .and_then(|e| e.preview.as_mut())
            {
                let y = f32::from(event.delta.pixel_delta(px(20.)).y);
                p.camera.distance = (p.camera.distance * (-y * 0.006).exp()).clamp(0.25, 1e6);
                p.bridge.publish_camera(p.camera);
            }
            cx.stop_propagation();
        }));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child("Preview")
                .child(
                    Button::new("object-preview/fit")
                        .small()
                        .label("Fit View")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this
                                .materials
                                .editor
                                .as_mut()
                                .and_then(|e| e.preview.as_mut())
                            {
                                p.camera.fit(&p.document);
                                p.bridge.publish_camera(p.camera);
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xa9b7c5))
                .child("Right-drag orbit · middle / Shift+right-drag pan · wheel zoom"),
        )
        .child(viewport)
        .when_some(preview.error.clone(), |row, error| {
            row.child(
                div()
                    .id("object-preview/error")
                    .max_h(px(80.))
                    .overflow_y_scroll()
                    .text_color(rgb(0xff9999))
                    .child(error),
            )
        })
        .child(div().text_xs().text_color(rgb(0xa9b7c5)).child(
            if preview.due.is_some() || preview.pending.is_some() {
                "Updating preview…"
            } else {
                "Draft preview · Apply or Save changes the drawing"
            },
        ))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_form_preview_uses_the_draft_without_editing_the_drawing() {
        let source = Document::new();
        let before = source.clone();
        for id in [
            procedural::BRICK,
            procedural::WOOD,
            procedural::WATER,
            procedural::GRASS,
            procedural::FOLIAGE,
        ] {
            let mut draft = Material::default();
            procedural::apply_preset(&mut draft, id);
            if matches!(id, procedural::BRICK | procedural::WOOD) {
                draft.procedural.as_mut().unwrap().parameters.extend([
                    ("rotationU".into(), json!(25.)),
                    ("rotationV".into(), json!(-60.)),
                ]);
            }
            let preview = preview_document(&source, &draft).unwrap();
            let configured = preview
                .materials()
                .into_iter()
                .find(|m| m.id == "shader-preview")
                .unwrap();
            assert_eq!(
                configured.procedural.as_ref().unwrap().parameters,
                draft.procedural.as_ref().unwrap().parameters
            );
            assert!(preview.scene_geometry_refs().unwrap().iter().any(|p| {
                preview
                    .effective_geometry(p)
                    .is_some_and(|g| !g.triangles.is_empty())
            }));
            assert_eq!(source, before);
        }
    }

    #[test]
    fn object_form_preview_uses_private_values_and_preserves_the_source() {
        let mut source = Document::new();
        let id = source.create_procedural_tree().unwrap();
        let path = meshcraft_core::operations::objects::place_definition(
            &mut source,
            &EditContext::Document,
            id,
            Vec3::X * 10.,
            None,
            false,
        )
        .unwrap();
        source
            .add_cuboid("Unrelated geometry", Vec3::new(2., 3., 4.), Vec3::ZERO)
            .unwrap();
        let base = Material {
            id: "custom-bark".into(),
            name: "Bark".into(),
            color: Rgba8::new(30, 60, 90, 255),
            ..Default::default()
        };
        source.put_material(base.clone()).unwrap();
        let mut settings = source.procedural_object(&path).unwrap();
        settings.procedural.as_mut().unwrap().base_material = Some(base.id.clone());
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), json!(9.));
        source
            .set_procedural_object(id, Some(&path), settings.clone())
            .unwrap();
        let before = source.clone();
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), json!(18.));
        let preview = preview_document(&source, &settings).unwrap();
        assert_eq!(source, before);
        assert_eq!(preview.occurrence_refs().unwrap().len(), 1);
        assert!(preview.definitions().all(|d| d.geometry.is_empty()));
        let object = preview
            .procedural_object(&preview.occurrence_refs().unwrap()[0])
            .unwrap();
        assert_eq!(
            object.procedural.as_ref().unwrap().parameters["height"],
            json!(18.)
        );
        assert!(preview.materials().contains(&base));
        settings
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("height".into(), json!(-1.));
        assert!(preview_document(&source, &settings).is_err());
        assert_eq!(source, before);
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    #[test]
    #[ignore = "requires private X11/OpenGL; verifies actual object form shader output and idle flags"]
    fn procedural_object_form_gpu_preview_animates_and_pauses() {
        let (mut rl, thread) = raylib::init()
            .size(640, 480)
            .hidden()
            .title("Object settings preview")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut target = rl.load_render_texture(&thread, 640, 480).unwrap();
        let source = Document::new();
        let mut material = Material::default();
        procedural::apply_preset(&mut material, procedural::TREE);
        let mut scene = RenderScene {
            document: Arc::new(preview_document(&source, &material).unwrap()),
            grid_visible: false,
            guides_visible: false,
            ..Default::default()
        };
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        let mut camera = OrbitCamera::default();
        camera.fit(&scene.document);
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.gpu.procedural.animated);
        super::super::super::draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let image = super::super::super::copy_target_to_gpui_image(&target).unwrap();
        assert!(
            image
                .pixels()
                .filter(
                    |p| p[1] as u32 > p[0] as u32 * 12 / 10 && p[1] as u32 > p[2] as u32 * 12 / 10
                )
                .count()
                > 500,
            "preview must contain rendered leaves"
        );
        material
            .procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("wind".into(), json!(0.));
        scene.document = Arc::new(preview_document(&source, &material).unwrap());
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            !renderer.gpu.procedural.animated,
            "zero wind pauses the built-in object preview"
        );
        renderer.prepare(&mut rl, &thread, &RenderScene::default());
        assert!(
            !renderer.gpu.procedural.animated,
            "returning to an empty main scene stops preview animation"
        );
    }
}
