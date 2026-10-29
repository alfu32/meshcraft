//! Full-window shader IDE. Draft scenes never enter the document or undo history.
use super::*;
#[path = "gpui_shader_language.rs"]
mod language;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Definition,
    Vertex,
    Fragment,
}
impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Definition => "Definition · JSON",
            Self::Vertex => "Vertex · GLSL",
            Self::Fragment => "Fragment · GLSL",
        }
    }
}

pub(super) struct Editor {
    code: Entity<InputState>,
    vertex: Entity<InputState>,
    fragment: Entity<InputState>,
    tab: Tab,
    _subscriptions: Vec<gpui::Subscription>,
    material: Material,
    base: Option<Material>,
    bridge: ViewportBridge,
    canvas: Entity<RaylibCanvas>,
    camera: OrbitCamera,
    drag: Option<(gpui::MouseButton, f32, f32)>,
    due: Option<Instant>,
    pending: Option<u64>,
    valid: bool,
    error: Option<String>,
}

fn parse_program(metadata: &str, vertex: &str, fragment: &str) -> Result<Program, String> {
    let mut value: Value = serde_json::from_str(metadata).map_err(|e| e.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or("Program definition must be a JSON object")?;
    object.insert("vertex".into(), json!(vertex));
    object.insert("fragment".into(), json!(fragment));
    let program: Program = serde_json::from_value(value).map_err(|e| e.to_string())?;
    program.validate()?;
    Ok(program)
}
fn program(editor: &Editor, cx: &Context<MeshcraftShell>) -> Result<Program, String> {
    parse_program(
        &editor.code.read(cx).value(),
        &editor.vertex.read(cx).value(),
        &editor.fragment.read(cx).value(),
    )
}

/// Schema edits keep matching, valid material values; new/changed fields use defaults.
fn configure(material: &Material, program: &Program) -> Material {
    let mut material = material.clone();
    let config = material.procedural.get_or_insert_with(Default::default);
    config.program = program.id.clone();
    config.parameters.retain(|id, value| {
        program
            .parameters
            .iter()
            .any(|p| &p.id == id && p.validate_value(value).is_ok())
    });
    material
}

pub(super) fn preview_document(
    program: &Program,
    material: &Material,
    base: Option<&Material>,
) -> Result<Document, String> {
    let mut document = Document::new();
    let mut program = program.clone();
    // A private ID also permits previews of a built-in copy without replacing it.
    program
        .extensions
        .insert(PREVIEW_PROGRAM_SOURCE.into(), json!(program.id));
    program.id = "meshcraft.shader-preview".into();
    document.put_procedural_program(program.clone())?;
    if let Some(base) = base {
        document.put_material(base.clone())?;
    }
    let mut material = configure(material, &program);
    if program.is_object_generator() {
        let id = document.create_procedural_tree()?;
        document.set_procedural_object(id, None, material)?;
        meshcraft_core::operations::objects::place_definition(
            &mut document,
            &EditContext::Document,
            id,
            Vec3::ZERO,
            None,
            false,
        )?;
    } else {
        material.id = "shader-preview".into();
        document.put_material(material.clone())?;
        let mut selection = meshcraft_core::selection::SelectionSession::default();
        if program.generation == "surface" {
            let occurrence = document
                .add_cuboid("Preview", Vec3::new(4., 4., 4.), Vec3::new(-2., 0., -2.))
                .ok_or("Could not create preview cube")?;
            for face in document
                .effective_geometry(&occurrence)
                .ok_or("Preview cube missing")?
                .triangles
                .keys()
            {
                selection.select(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Triangle(*face),
                });
            }
        } else {
            let (_, faces) = document.append_planar_mesh_to_scene_root(
                vec![
                    Vec3::new(-3., 0.1, -3.),
                    Vec3::new(3., 0.1, -3.),
                    Vec3::new(3., 0.1, 3.),
                    Vec3::new(-3., 0.1, 3.),
                ],
                vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                vec![[0, 2, 1], [0, 3, 2]],
            )?;
            let occurrence = document
                .scene_geometry_refs()?
                .into_iter()
                .next()
                .ok_or("Preview surface missing")?;
            for face in faces {
                selection.select(SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity: SelectedEntity::Triangle(face),
                });
            }
        }
        meshcraft_core::operations::transforms::assign_material(
            &mut document,
            &selection,
            &material.id,
        )?;
    }
    Ok(document)
}

pub(super) fn open(
    shell: &mut MeshcraftShell,
    source: Program,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    language::register();
    {
        let theme = cx.global_mut::<Theme>();
        theme.highlight_theme = Arc::new(scripts::language::theme(&theme.highlight_theme));
    }
    let mut metadata = serde_json::to_value(&source).unwrap();
    metadata.as_object_mut().unwrap().remove("vertex");
    metadata.as_object_mut().unwrap().remove("fragment");
    let code = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor("json")
            .default_value(serde_json::to_string_pretty(&metadata).unwrap())
    });
    let vertex = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor(language::NAME)
            .default_value(source.vertex.clone())
    });
    let fragment = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor(language::NAME)
            .default_value(source.fragment.clone())
    });
    let subscriptions = [&code, &vertex, &fragment]
        .into_iter()
        .map(|input| {
            cx.subscribe_in(input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(editor) = this.materials.program_editor.as_mut()
                {
                    editor.due = Some(Instant::now() + Duration::from_millis(300));
                    editor.valid = false;
                    editor.pending = None;
                    editor.error = None;
                    cx.notify();
                }
            })
        })
        .collect();
    let mut material = shell
        .materials
        .editor
        .as_ref()
        .map(|e| draft_material(e, cx).unwrap_or_else(|_| e.material.clone()))
        .unwrap_or_default();
    if let Some(parent) = shell.materials.editor.as_ref() {
        if let Ok(values) = read_parameter_fields(parent, cx)
            && let Some(config) = material.procedural.as_mut()
        {
            config.parameters = values;
        }
        if let Ok(color) = gpui::Hsla::parse_hex(&parent.color.read(cx).value()) {
            let c: gpui::Rgba = color.into();
            material.color = Rgba8::new(
                (c.r * 255.).round() as u8,
                (c.g * 255.).round() as u8,
                (c.b * 255.).round() as u8,
                (c.a * 255.).round() as u8,
            );
        }
    }
    let base = material
        .procedural
        .as_ref()
        .and_then(|c| c.base_material.as_ref())
        .and_then(|id| {
            shell
                .cached_document
                .materials()
                .into_iter()
                .find(|m| &m.id == id && m.procedural.is_none())
        });
    let mut bridge = ViewportBridge::new();
    bridge.redraw = shell.bridge.redraw.clone();
    let canvas = cx.new(|_| RaylibCanvas {
        bridge: bridge.clone(),
        frame_watch_started: false,
        presented_image: None,
    });
    shell.materials.program_editor = Some(Editor {
        code: code.clone(),
        vertex,
        fragment,
        tab: Tab::Definition,
        _subscriptions: subscriptions,
        material,
        base,
        bridge: bridge.clone(),
        canvas,
        camera: OrbitCamera::default(),
        drag: None,
        due: None,
        pending: None,
        valid: false,
        error: None,
    });
    if let Ok(mut preview) = shell.bridge.preview.lock() {
        *preview = Some(bridge);
    }
    evaluate(shell, cx, true);
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
    shell.materials.program_editor = None;
    if let Ok(mut preview) = shell.bridge.preview.lock() {
        *preview = None;
    }
    super::object_preview::activate(shell);
    shell.bridge.request_redraw();
    window.focus(&shell.focus);
    cx.notify();
}

fn evaluate(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>, fit: bool) {
    let Some(editor) = shell.materials.program_editor.as_mut() else {
        return;
    };
    editor.due = None;
    editor.valid = false;
    editor.pending = None;
    let result = program(editor, cx)
        .and_then(|p| preview_document(&p, &editor.material, editor.base.as_ref()));
    match result {
        Ok(document) => {
            if fit {
                editor.camera.fit(&document);
            }
            editor.bridge.publish_scene(RenderScene {
                document: Arc::new(document),
                lighting: shell.lighting,
                grid_visible: false,
                guides_visible: false,
                black_model_lines: false,
                ..Default::default()
            });
            editor.bridge.publish_camera(editor.camera);
            editor.pending = editor.bridge.redraw.0.lock().ok().map(|r| *r);
            editor.error = None;
        }
        Err(error) => {
            editor.error = Some(error);
        }
    }
}

/// Debounce typing and consume compile diagnostics only for the current draft.
/// Camera motion and animation never rebuild the preview document.
pub(super) fn tick(shell: &mut MeshcraftShell, cx: &Context<MeshcraftShell>) -> bool {
    if shell
        .materials
        .program_editor
        .as_ref()
        .and_then(|e| e.due)
        .is_some_and(|due| Instant::now() >= due)
    {
        evaluate(shell, cx, false);
        return true;
    }
    if let Some(editor) = shell.materials.program_editor.as_mut()
        && let Some(revision) = editor.pending
        && let Some(frame) = editor.bridge.latest_frame()
        && frame.request_revision >= revision
    {
        editor.pending = None;
        editor.valid = frame.procedural_diagnostics.is_empty();
        editor.error = (!editor.valid).then(|| frame.procedural_diagnostics.join("\n"));
        return true;
    }
    false
}

fn save(shell: &mut MeshcraftShell, window: &mut Window, cx: &mut Context<MeshcraftShell>) {
    let Some(editor) = shell.materials.program_editor.as_ref() else {
        return;
    };
    if !editor.valid || editor.due.is_some() || editor.pending.is_some() {
        return;
    }
    let result = program(editor, cx).and_then(|p| {
        shell.session.save_native_procedural_program(p.clone())?;
        Ok(p)
    });
    match result {
        Ok(program) => {
            let parent = shell.materials.editor.as_ref().map(|e| {
                let mut material = draft_material(e, cx).unwrap_or_else(|_| e.material.clone());
                if let Ok(values) = read_parameter_fields(e, cx)
                    && let Some(config) = material.procedural.as_mut()
                {
                    config.parameters = values;
                }
                (configure(&material, &program), e.object.clone())
            });
            close(shell, window, cx);
            shell.refresh_scene();
            if let Some((material, object)) = parent {
                edit(shell, material, window, cx);
                shell.materials.editor.as_mut().unwrap().object = object;
            }
        }
        Err(error) => {
            shell.materials.program_editor.as_mut().unwrap().error = Some(error);
            cx.notify();
        }
    }
}

pub(super) fn overlay(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let editor = shell.materials.program_editor.as_ref()?;
    let tabs = div().flex().gap(px(6.)).children(
        [Tab::Definition, Tab::Vertex, Tab::Fragment]
            .into_iter()
            .map(|tab| {
                Button::new(tab.label())
                    .label(tab.label())
                    .selected(editor.tab == tab)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(e) = this.materials.program_editor.as_mut() {
                            e.tab = tab;
                            let input = match tab {
                                Tab::Definition => &e.code,
                                Tab::Vertex => &e.vertex,
                                Tab::Fragment => &e.fragment,
                            };
                            input.update(cx, |i, cx| i.focus(window, cx));
                        }
                        cx.notify();
                    }))
            }),
    );
    let input = match editor.tab {
        Tab::Definition => &editor.code,
        Tab::Vertex => &editor.vertex,
        Tab::Fragment => &editor.fragment,
    };
    let code = div()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(tabs)
        .child(
            div().flex_1().min_h(px(0.)).child(
                Input::new(input)
                    .size_full()
                    .font_family(cx.global::<Theme>().mono_font_family.clone())
                    .text_size(px(14.)),
            ),
        );
    let bridge = editor.bridge.clone();
    let mut preview = div()
        .id("program/preview")
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
            .inset_0(),
        );
    for button in [gpui::MouseButton::Right, gpui::MouseButton::Middle] {
        preview = preview
            .on_mouse_down(
                button,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    if let Some(e) = this.materials.program_editor.as_mut() {
                        e.drag = Some((
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
                cx.listener(|this, _, _, _| {
                    if let Some(e) = this.materials.program_editor.as_mut() {
                        e.drag = None;
                    }
                }),
            );
    }
    preview = preview
        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
            if let Some(e) = this.materials.program_editor.as_mut()
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
            if let Some(e) = this.materials.program_editor.as_mut() {
                let y = f32::from(event.delta.pixel_delta(px(20.)).y);
                e.camera.distance = (e.camera.distance * (-y * 0.006).exp()).clamp(0.25, 1e6);
                e.bridge.publish_camera(e.camera);
            }
            cx.stop_propagation();
        }));
    let status = if editor.due.is_some() {
        "Waiting for typing…"
    } else if editor.pending.is_some() {
        "Compiling preview…"
    } else if editor.valid {
        "Preview ready"
    } else {
        "Fix the draft to preview"
    };
    Some(div().id("program/modal").absolute().inset_0().top(px(layout::TITLE_BAR_HEIGHT)).occlude().p(px(24.)).flex().flex_col().gap(px(10.)).bg(rgb(0x1b222b))
        .on_scroll_wheel(|_,_,cx| cx.stop_propagation())
        .child(div().text_lg().child("Shader Editor"))
        .child(div().text_sm().child("GLSL 330 · Parameters: u_<id> · time: seconds. Preview: right-drag orbit · Shift+right / middle-drag pan · wheel zoom."))
        .child(div().flex().flex_1().min_h(px(0.)).gap(px(12.)).child(code).child(preview))
        .when_some(editor.error.clone(), |row, error| row.child(div().id("program/errors").max_h(px(100.)).overflow_y_scroll().text_sm().text_color(rgb(0xff9999)).child(error)))
        .child(div().flex().items_center().justify_between()
            .child(div().text_sm().child(status))
            .child(div().flex().gap(px(8.))
                .child(Button::new("program/cancel").label("Cancel").on_click(cx.listener(|this,_,window,cx| close(this,window,cx))))
                .child(Button::new("program/preview").label("Preview").on_click(cx.listener(|this,_,_,cx| {evaluate(this,cx,false);cx.notify();})))
                .child(Button::new("program/save").label("Save Program").primary().disabled(!editor.valid || editor.due.is_some() || editor.pending.is_some()).on_click(cx.listener(|this,_,window,cx| save(this,window,cx))))))
        .into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shader_draft_parses_all_three_tabs_and_rejects_invalid_metadata() {
        let p = parse_program(
            r#"{"id":"custom.test","name":"Test"}"#,
            "vertex",
            "fragment",
        )
        .unwrap();
        assert_eq!(p.vertex, "vertex");
        assert_eq!(p.fragment, "fragment");
        for invalid in ["{", "[]", "null", r#"{"id":"","name":"Test"}"#] {
            assert!(parse_program(invalid, "", "").is_err());
        }
    }
    #[test]
    fn every_builtin_has_an_isolated_preview_with_its_shader_assigned() {
        let original = Document::new();
        let before = original.procedural_programs();
        for p in before.iter() {
            let preview = preview_document(p, &Material::default(), None).unwrap();
            assert_eq!(original.procedural_programs(), before);
            let custom = preview
                .procedural_programs()
                .into_iter()
                .find(|p| p.id == "meshcraft.shader-preview")
                .unwrap();
            assert_eq!(custom.vertex, p.vertex);
            assert_eq!(custom.fragment, p.fragment);
            let paths = preview.scene_geometry_refs().unwrap();
            assert!(!paths.is_empty());
            if p.is_object_generator() {
                assert!(
                    paths
                        .iter()
                        .any(|path| preview.procedural_object(path).is_some())
                );
            } else {
                assert!(
                    paths
                        .iter()
                        .filter_map(|path| preview.effective_geometry(path))
                        .flat_map(|g| g.triangles.values())
                        .any(
                            |f| meshcraft_core::document::materials::material_id(&f.extensions)
                                == Some("shader-preview")
                        )
                );
            }
        }
    }
    #[test]
    fn schema_changes_preserve_compatible_material_values() {
        let p = procedural::grass_program();
        let material = Material {
            procedural: Some(ProceduralMaterial {
                program: p.id.clone(),
                parameters: BTreeMap::from([
                    ("height".into(), json!(0.8)),
                    ("removed".into(), json!(1)),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let updated = configure(&material, &p);
        let values = &updated.procedural.as_ref().unwrap().parameters;
        assert_eq!(values.get("height"), Some(&json!(0.8)));
        assert!(!values.contains_key("removed"));
        assert!(p.resolve(values, &BTreeMap::new()).is_ok());
    }
}
