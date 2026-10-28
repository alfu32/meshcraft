//! MCP lifecycle panel and main-thread request pump.
use super::*;
use crate::mcp;
use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct Ui {
    server: Option<mcp::Server>,
    port: Option<Entity<InputState>>,
    subscription: Option<gpui::Subscription>,
    error: Option<String>,
    logs: VecDeque<Value>,
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    if shell.mcp.port.is_some() {
        return;
    }
    let value = shell.session.native_mcp_port().to_string();
    let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
    shell.mcp.subscription = Some(cx.subscribe_in(
        &input,
        window,
        |this, _, event: &gpui_component::input::InputEvent, window, cx| {
            match event {
                gpui_component::input::InputEvent::Focus => this.settings.active = Some("mcp_port"),
                gpui_component::input::InputEvent::Blur => {
                    save_port(this, cx);
                    this.settings.active = None;
                }
                gpui_component::input::InputEvent::PressEnter { .. } => {
                    save_port(this, cx);
                    this.settings.active = None;
                    window.focus(&this.focus);
                }
                _ => {}
            }
            cx.notify();
        },
    ));
    shell.mcp.port = Some(input);
}
fn save_port(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> bool {
    let value = shell
        .mcp
        .port
        .as_ref()
        .map(|i| i.read(cx).value().to_string())
        .unwrap_or_default();
    let result = value
        .parse::<u16>()
        .map_err(|_| "Port must be an integer from 1 to 65535".to_owned())
        .and_then(|p| shell.session.native_set_mcp_port(p));
    shell.mcp.error = result.err();
    shell.mcp.error.is_none()
}
fn control(shell: &mut MeshcraftShell, action: &str, cx: &mut Context<MeshcraftShell>) {
    if action != "stop" && !save_port(shell, cx) {
        return;
    }
    if action != "start" || !running(shell) {
        shell.mcp.server = None;
        let _ = shell.session.cancel_mcp_script();
    }
    if action != "stop" && shell.mcp.server.is_none() {
        match mcp::Server::start(shell.session.native_mcp_port()) {
            Ok(server) => {
                shell.mcp.server = Some(server);
                shell.mcp.error = None;
            }
            Err(error) => shell.mcp.error = Some(format!("Could not start MCP: {error}")),
        }
    }
    shell.session.set_native_status(format!(
        "MCP server {}.",
        if running(shell) { "running" } else { "stopped" }
    ));
    cx.notify();
}
fn running(shell: &MeshcraftShell) -> bool {
    shell.mcp.server.as_ref().is_some_and(mcp::Server::running)
}
/// Explicit startup opt-in for automation; default interactive launches stay stopped.
pub(super) fn start_on_launch(
    shell: &mut MeshcraftShell,
    port: u16,
    cx: &mut Context<MeshcraftShell>,
) {
    let result = shell
        .session
        .native_set_mcp_port(port)
        .and_then(|()| mcp::Server::start(port));
    match result {
        Ok(server) => shell.mcp.server = Some(server),
        Err(error) => {
            eprintln!("MCP startup failed: {error}");
            shell.mcp.error = Some(error);
        }
    }
    cx.notify();
}
pub(super) fn panel(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let active = running(shell);
    let port = shell
        .mcp
        .server
        .as_ref()
        .map_or(shell.session.native_mcp_port(), |s| s.port);
    let mut row = div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .text_sm()
        .child(if active {
            "Running · localhost only"
        } else {
            "Stopped"
        })
        .child(
            div()
                .text_xs()
                .child(format!("http://127.0.0.1:{port}/mcp")),
        )
        .child("Port (Restart applies changes)")
        .when_some(shell.mcp.port.as_ref(), |row, input| {
            row.child(Input::new(input))
        });
    let mut buttons = div().flex().gap(px(5.));
    for (action, label) in [("start", "Start"), ("stop", "Stop"), ("restart", "Restart")] {
        buttons = buttons.child(
            Button::new(SharedString::from(format!("mcp/{action}")))
                .small()
                .label(label)
                .disabled(if action == "start" { active } else { !active })
                .on_click(cx.listener(move |this, _, window, cx| {
                    control(this, action, cx);
                    this.settings.active = None;
                    window.focus(&this.focus);
                })),
        );
    }
    row=row.child(buttons).child(div().text_xs().text_color(rgb(0xa5adb7)).child("MCP · commands, scene/selection, materials, object scripts and full JavaScript API documentation. Connect with Streamable HTTP."));
    if let Some(error) = &shell.mcp.error {
        row = row.child(div().text_color(rgb(0xffb19e)).child(error.clone()));
    }
    if let Some(last) = shell.mcp.logs.back() {
        row = row.child(div().text_xs().child(format!(
            "Last request: {}",
            last["operation"].as_str().unwrap_or("request")
        )));
    }
    row.into_any_element()
}
pub(super) fn poll(shell: &mut MeshcraftShell) -> bool {
    let mut handled = false;
    for _ in 0..4 {
        let Some(work) = shell.mcp.server.as_ref().and_then(mcp::Server::try_recv) else {
            break;
        };
        if !work.live() {
            work.finish(Err("request canceled before execution".into()));
            continue;
        }
        // A host job may have committed immediately before this queue was pumped.
        // Publish it before a queued capture can compare its frame revision.
        if shell.cached_document_revision != Some(shell.session.native_visual_revision())
            || shell.cached_document_generation != Some(shell.session.document_generation())
            || shell.cached_edit_context.as_ref() != Some(&shell.session.native_edit_context())
        {
            shell.refresh_scene();
        }
        if !work.live() {
            work.finish(Err("request expired before execution".into()));
            continue;
        }
        let operation = if work.method == "tools/call" {
            work.params["name"].as_str().unwrap_or("tool")
        } else {
            work.params["uri"].as_str().unwrap_or("resource")
        }
        .chars()
        .take(256)
        .collect::<String>();
        let changed = work.method == "tools/call"
            && !matches!(
                operation.as_str(),
                "k3d_contract"
                    | "k3d_status"
                    | "k3d_list_commands"
                    | "meshcraft_scene"
                    | "meshcraft_geometry"
                    | "meshcraft_script_api"
                    | "meshcraft_get_job"
                    | "meshcraft_capture_view"
            );
        let start = Instant::now();
        let mut result = if work.method == "tools/call" {
            call(
                shell,
                work.params["name"].as_str().unwrap_or(""),
                work.params.get("arguments").unwrap_or(&json!({})),
            )
        } else {
            read(shell, work.params["uri"].as_str().unwrap_or(""))
        };
        // Publish each edit before acknowledging it or processing the next request.
        // Generic commands already do this through apply_command_effects.
        if changed && operation != "k3d_execute_command" {
            shell.refresh_scene();
        }
        if work.method == "tools/call"
            && let Ok(Value::Object(value)) = &mut result
        {
            value.insert(
                "_timing".into(),
                json!({"ownerThreadMs":start.elapsed().as_secs_f64()*1000.}),
            );
        }
        let log = json!({"operation":operation,"success":result.is_ok(),"elapsedMs":start.elapsed().as_millis(),"revision":shell.session.document_revision(),"error":result.as_ref().err().map(|s|s.chars().take(1024).collect::<String>())});
        shell.mcp.logs.push_back(log);
        while shell.mcp.logs.len() > 100 {
            shell.mcp.logs.pop_front();
        }
        work.finish(result);
        handled = true;
    }
    handled
}
fn call(shell: &mut MeshcraftShell, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "k3d_contract" => Ok(mcp::contract()),
        "k3d_status" => {
            let mut state = shell.session.mcp_status();
            state["server"] = json!({"running":running(shell),"port":shell.mcp.server.as_ref().map(|s|s.port),"configuredPort":shell.session.native_mcp_port()});
            state["camera"] = json!({"position":shell.camera.position(),"target":shell.camera.target,"orthographic":shell.camera.orthographic,"walkthrough":shell.walking,"fieldOfView":shell.camera.field_of_view});
            state["scriptErrors"] = json!(shell.scripts.errors);
            Ok(state)
        }
        "k3d_list_commands" => Ok(shell.session.mcp_commands(args)),
        "k3d_execute_command" => {
            let id = args["commandId"].as_str().ok_or("commandId required")?;
            if id.len() > 128 {
                return Err("commandId exceeds 128 bytes".into());
            }
            let values = args.get("arguments").cloned().unwrap_or(json!({}));
            let values = values
                .as_object()
                .ok_or("arguments must be an object")?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let payload = CommandPayload {
                values,
                ..Default::default()
            };
            if (matches!(
                id,
                "file.open"
                    | "file.save_as"
                    | "object.import_scene"
                    | "object.reference_scene"
                    | "import.mesh"
                    | "export.mesh"
                    | "export.ifc_model"
            ) || id == "file.save" && shell.session.current_project_path().is_none())
                && !payload.values.contains_key("path")
            {
                return Err(
                    "provide an absolute path; MCP does not open interactive file dialogs".into(),
                );
            }
            let dispatch = shell.session.dispatch_command(id, &payload);
            shell.apply_command_effects(id, &dispatch);
            if !dispatch.accepted || dispatch.result.completion == CommandCompletion::Failed {
                return Err(dispatch.result.diagnostics.join("; "));
            }
            let mut result = json!({"dispatch":dispatch,"status":shell.session.mcp_status()});
            if matches!(
                id,
                "edit.copy_selection" | "edit.cut_selection" | "edit.copy_as_script"
            ) {
                result["clipboard"] = json!(shell.session.mcp_clipboard());
            }
            Ok(result)
        }
        "k3d_pointer_event" => {
            let result = shell.session.mcp_pointer(args)?;
            if matches!(args["action"].as_str(), Some("move" | "click")) {
                let point = Vec3::new(
                    args["worldX"].as_f64().unwrap(),
                    args["worldY"].as_f64().unwrap(),
                    args["worldZ"].as_f64().unwrap(),
                );
                shell.cursor_snap = Some(SnapResult {
                    point,
                    normal: Vec3::Y,
                    kind: SnapKind::Free,
                    screen_distance: 0.,
                });
                shell.cursor_local = project_point(
                    shell.camera,
                    (shell.layout.width as f32, shell.layout.height as f32),
                    point,
                );
                if shell.cursor_local.is_none() {
                    shell.cursor_local = Some((0., 0.));
                }
            } else if args["action"] == "cancel" {
                shell.cursor_snap = None;
                shell.last_pick = None;
            }
            Ok(result)
        }
        "meshcraft_scene" => shell.session.mcp_scene(args),
        "meshcraft_geometry" => shell.session.mcp_geometry(args),
        "meshcraft_select" => shell.session.mcp_select(args),
        "meshcraft_script_api" => {
            let q = args["query"].as_str().unwrap_or("").to_lowercase();
            Ok(
                json!({"entries":crate::script_api::catalog().into_iter().filter(|e|q.is_empty()||format!("{} {} {}",e.name,e.signature,e.help).to_lowercase().contains(&q)).collect::<Vec<_>>(),"scope":"Same SDK as scripted objects. model is immutable; this/instance owns its JSON properties. See meshcraft://guide and meshcraft://scripting/sdk."}),
            )
        }
        "meshcraft_ifc" => {
            use meshcraft_core::document::{DefinitionId, LayoutEntryId, OccurrenceRef, ifc};
            match args["action"].as_str() {
                Some("presets") => Ok(json!({"schema":ifc::SCHEMA,"presets":ifc::presets::ALL})),
                Some("preset") => {
                    let definition = DefinitionId(
                        args["definition_id"]
                            .as_u64()
                            .ok_or("definition_id required")?,
                    );
                    let paths: Vec<Vec<u64>> =
                        serde_json::from_value(args.get("paths").cloned().unwrap_or(json!([])))
                            .map_err(|e| e.to_string())?;
                    let paths = paths
                        .into_iter()
                        .map(|path| OccurrenceRef {
                            path: path.into_iter().map(LayoutEntryId).collect(),
                        })
                        .collect();
                    let preset = args["preset"].as_str().ok_or("preset required")?;
                    let added = shell
                        .session
                        .apply_native_ifc_preset(definition, paths, preset)?;
                    Ok(json!({"added":added,"revision":shell.session.document_revision()}))
                }
                Some("catalog") => {
                    let q = args["query"].as_str().unwrap_or("").to_lowercase();
                    Ok(
                        json!({"schema":ifc::SCHEMA,"types":ifc::catalog().types,"valueTypes":ifc::catalog().value_types,"properties":ifc::catalog().properties.iter().filter(|p|format!("{} {} {}",p.set,p.name,p.data_type).to_lowercase().contains(&q)).collect::<Vec<_>>()}),
                    )
                }
                Some("get") => {
                    let prototype = args["prototype"].as_bool().unwrap_or(false);
                    let (id, paths, props) = shell
                        .session
                        .native_ifc_selection(prototype)
                        .ok_or("Select an object instance or prototype")?;
                    Ok(
                        json!({"schema":ifc::SCHEMA,"definition_id":id,"paths":paths.iter().map(|p|&p.path).collect::<Vec<_>>(),"properties":props}),
                    )
                }
                Some(action @ ("set" | "delete")) => {
                    let id = DefinitionId(
                        args["definition_id"]
                            .as_u64()
                            .ok_or("definition_id required")?,
                    );
                    let paths: Vec<Vec<u64>> =
                        serde_json::from_value(args.get("paths").cloned().unwrap_or(json!([])))
                            .map_err(|e| e.to_string())?;
                    let paths = paths
                        .into_iter()
                        .map(|p| OccurrenceRef {
                            path: p.into_iter().map(LayoutEntryId).collect(),
                        })
                        .collect();
                    let set = args["set"].as_str().ok_or("set required")?.to_owned();
                    let name = args["name"].as_str().ok_or("name required")?.to_owned();
                    let property = if action == "set" {
                        Some(
                            serde_json::from_value(args["property"].clone())
                                .map_err(|e| e.to_string())?,
                        )
                    } else {
                        None
                    };
                    shell.session.edit_native_ifc(
                        id,
                        paths,
                        Some((set.clone(), name.clone())),
                        set,
                        name,
                        property,
                    )?;
                    Ok(json!({"revision":shell.session.document_revision()}))
                }
                _ => Err("action must be get, catalog, presets, preset, set or delete".into()),
            }
        }
        "meshcraft_parameters" => match args["action"].as_str() {
            Some("get") => Ok(json!({"parameters":shell.session.native_script_parameters()})),
            Some("set") => {
                let definition = meshcraft_core::document::DefinitionId(
                    args["definition_id"]
                        .as_u64()
                        .ok_or("definition_id required")?,
                );
                let id = args["id"].as_str().ok_or("id required")?;
                let value = args.get("value").cloned().ok_or("value required")?;
                let paths: Vec<Vec<u64>> =
                    serde_json::from_value(args.get("paths").cloned().unwrap_or(json!([])))
                        .map_err(|e| e.to_string())?;
                let paths = paths
                    .into_iter()
                    .map(|p| meshcraft_core::document::OccurrenceRef {
                        path: p
                            .into_iter()
                            .map(meshcraft_core::document::LayoutEntryId)
                            .collect(),
                    })
                    .collect();
                shell
                    .session
                    .set_native_script_parameter(definition, paths, id, value)?;
                Ok(
                    json!({"parameters":shell.session.native_script_parameters(),"revision":shell.session.document_revision()}),
                )
            }
            _ => Err("action must be get or set".into()),
        },
        "meshcraft_object_script" => shell.session.mcp_script(args),
        "meshcraft_run_script" => shell.session.mcp_run_script(args),
        "meshcraft_cancel_script" => shell.session.cancel_mcp_script(),
        "meshcraft_capture_view" => capture(shell, args),
        "meshcraft_resolve_document" => shell.session.mcp_resolve_document(args),
        "meshcraft_get_job" => {
            let id = args["job_id"].as_str().ok_or("job_id required")?;
            shell
                .session
                .command_job_result(id)
                .map(|r| json!({"job_id":id,"result":r}))
                .ok_or("unknown job ID".into())
        }
        _ => Err("unknown tool".into()),
    }
}
fn read(shell: &mut MeshcraftShell, uri: &str) -> Result<Value, String> {
    if uri.len() > 1024 {
        return Err("resource URI exceeds 1024 bytes".into());
    }
    let uri = uri.replace("k3d://", "meshcraft://");
    let uri = if uri == "meshcraft://selection/summary" {
        "meshcraft://selection"
    } else {
        &uri
    };
    let (mime, text) = match uri {
        "meshcraft://guide" => ("text/markdown", mcp::guide().into()),
        "meshcraft://contract" => ("application/json", mcp::contract().to_string()),
        "meshcraft://commands" => (
            "application/json",
            shell.session.mcp_commands(&json!({})).to_string(),
        ),
        "meshcraft://scene/summary" => (
            "application/json",
            shell.session.mcp_scene(&json!({}))?.to_string(),
        ),
        "meshcraft://selection" => ("application/json", shell.session.mcp_status().to_string()),
        "meshcraft://scripting/api" => (
            "application/json",
            serde_json::to_string(&crate::script_api::catalog()).map_err(|e| e.to_string())?,
        ),
        "meshcraft://scripting/sdk" => ("text/javascript", include_str!("script_sdk.js").into()),
        "meshcraft://logs" => ("application/json", json!(shell.mcp.logs).to_string()),
        _ => return Err("unknown resource URI; use resources/list".into()),
    };
    Ok(json!({"contents":[{"uri":uri,"mimeType":mime,"text":text}]}))
}

fn capture(shell: &MeshcraftShell, args: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    use image::ImageEncoder as _;
    let preview = args["preview"].as_bool().unwrap_or(false);
    let bridge = if preview {
        shell
            .bridge
            .preview
            .lock()
            .map_err(|_| "renderer is unavailable")?
            .clone()
            .ok_or("no active editor preview; retry capture")?
    } else {
        shell.bridge.clone()
    };
    let frame = bridge
        .latest_frame()
        .ok_or("viewport has not rendered yet; retry capture")?;
    let requested = *bridge
        .redraw
        .0
        .lock()
        .map_err(|_| "renderer is unavailable")?;
    let fresh = frame.request_revision == requested;
    if !fresh && !args["allow_stale"].as_bool().unwrap_or(false) {
        return Err("viewport rendering is pending; retry capture, or set allow_stale=true".into());
    }
    let size = frame.image.size(0);
    let (width, height) = (
        u32::try_from(size.width.0).map_err(|_| "invalid viewport width")?,
        u32::try_from(size.height.0).map_err(|_| "invalid viewport height")?,
    );
    let mut bytes = frame
        .image
        .as_bytes(0)
        .ok_or("viewport frame is empty")?
        .to_vec();
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image =
        image::RgbaImage::from_raw(width, height, bytes).ok_or("invalid viewport image size")?;
    let max = args["max_width"].as_u64().unwrap_or(1024) as u32;
    let image = if width > max {
        image::imageops::resize(
            &image,
            max,
            (height as f64 * max as f64 / width as f64).round().max(1.) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    Ok(
        json!({"_mcpContent":[{"type":"image","mimeType":"image/png","data":data}],"width":image.width(),"height":image.height(),"procedural":{"diagnostics":frame.procedural_diagnostics,"candidateInstances":frame.procedural_candidates,"drawCalls":frame.procedural_draw_calls},"frameSequence":frame.sequence,"renderFrameMs":frame.frame_ms,"fresh":fresh,"preview":preview,"revision":shell.session.document_revision(),"generation":shell.session.document_generation(),"includes":"3D viewport only; GPUI labels/panels are separate; revision/generation describe the drawing, not uncommitted editor drafts"}),
    )
}
