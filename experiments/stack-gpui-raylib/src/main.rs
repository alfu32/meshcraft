use std::{
    num::NonZeroU32,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use gpui::{
    App, Application, Context, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions,
    canvas, div, prelude::*, px, rgb, size,
};
use raylib::prelude::*;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ViewportLayout {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ViewportInput {
    OrbitDelta { dx_pixels: f32, dy_pixels: f32 },
    ZoomSteps { steps: f32 },
    ResetCamera,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewportEvent {
    protocol_version: u16,
    sequence: u64,
    input: ViewportInput,
}

#[derive(Clone)]
struct ViewportBridge {
    layout: Arc<Mutex<ViewportLayout>>,
    latest_event: Arc<Mutex<Option<ViewportEvent>>>,
}

impl ViewportBridge {
    fn publish_layout(&self, layout: ViewportLayout) {
        if let Ok(mut current) = self.layout.lock() {
            *current = layout;
        }
    }

    fn record_input(&self, event: ViewportEvent) {
        if let Ok(mut latest) = self.latest_event.lock() {
            *latest = Some(event);
        }
    }
}

struct RaylibWorker {
    bridge: ViewportBridge,
    shutdown: ShutdownSignal,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct ShutdownSignal {
    stop: Arc<AtomicBool>,
    finished: Arc<(Mutex<bool>, Condvar)>,
}

impl ShutdownSignal {
    fn stop_and_wait(&self) {
        self.stop.store(true, Ordering::Release);
        let (finished, changed) = &*self.finished;
        let Ok(finished) = finished.lock() else {
            return;
        };
        let _finished = changed.wait_while(finished, |finished| !*finished);
    }
}

struct RenderThreadGuard(Arc<(Mutex<bool>, Condvar)>);

impl Drop for RenderThreadGuard {
    fn drop(&mut self) {
        let (finished, changed) = &*self.0;
        if let Ok(mut finished) = finished.lock() {
            *finished = true;
            changed.notify_all();
        }
    }
}

impl RaylibWorker {
    fn start() -> Self {
        let bridge = ViewportBridge {
            layout: Arc::new(Mutex::new(ViewportLayout::default())),
            latest_event: Arc::new(Mutex::new(None)),
        };
        let worker_bridge = bridge.clone();
        let shutdown = ShutdownSignal {
            stop: Arc::new(AtomicBool::new(false)),
            finished: Arc::new((Mutex::new(false), Condvar::new())),
        };
        let worker_stop = shutdown.stop.clone();
        let worker_finished = shutdown.finished.clone();
        let thread = thread::Builder::new()
            .name("raylib-viewport".into())
            .spawn(move || render_loop(worker_bridge, worker_stop, worker_finished))
            .expect("start raylib viewport thread");
        Self {
            bridge,
            shutdown,
            thread: Some(thread),
        }
    }

    fn bridge(&self) -> ViewportBridge {
        self.bridge.clone()
    }

    fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown.clone()
    }

    fn stop(&mut self) {
        self.shutdown.stop_and_wait();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RaylibWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn render_loop(
    bridge: ViewportBridge,
    stop: Arc<AtomicBool>,
    finished: Arc<(Mutex<bool>, Condvar)>,
) {
    let _finished = RenderThreadGuard(finished);
    let mut builder = raylib::init();
    let (mut rl, thread) = builder
        .size(640, 480)
        .title("Meshcraft viewport proof")
        .undecorated()
        .hidden()
        .vsync()
        .build();
    rl.set_target_fps(60);

    // Raylib 6 on this Linux proof is built with GLFW/X11. GetWindowHandle
    // points to raylib's X11 Window value, not to a GLFWwindow.
    let raylib_window = unsafe { rl.get_window_handle() };
    if raylib_window.is_null() {
        eprintln!("raylib did not expose an X11 window handle");
        return;
    }
    let child_id = unsafe { *(raylib_window.cast::<std::ffi::c_ulong>()) as u32 };
    let (connection, _) = match x11rb::connect(None) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("connect to the X11/XWayland server: {error}");
            return;
        }
    };

    let pid_atom = intern_atom(&connection, b"_NET_WM_PID");
    let title_atom = intern_atom(&connection, b"_NET_WM_NAME");
    let utf8_atom = intern_atom(&connection, b"UTF8_STRING");
    let mut gpui_parent = None;
    let process_id = std::process::id();

    let mut attached_parent = None;
    let root = connection.setup().roots.first().map(|screen| screen.root);
    let mut current_size = (640_u32, 480_u32);
    let mut last_layout = None;
    let mut geometry_synced = false;
    let mut camera = Camera3D::perspective(
        Vector3::new(8.0, 7.0, 8.0),
        Vector3::new(0.0, 0.8, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        45.0,
    );
    let mut sequence = 0_u64;
    let mut last_sequence = 0_u64;

    while !stop.load(Ordering::Acquire) && !rl.window_should_close() {
        let layout = bridge.layout.lock().map(|value| *value).unwrap_or_default();
        if gpui_parent.is_none() {
            gpui_parent =
                find_gpui_window(&connection, pid_atom, title_atom, utf8_atom, process_id);
        }
        if let Some(parent) = gpui_parent.filter(|_| layout.width > 0 && layout.height > 0) {
            if attached_parent != Some(parent) {
                if let Err(error) = connection.reparent_window(
                    child_id,
                    parent.get(),
                    layout.x as i16,
                    layout.y as i16,
                ) {
                    eprintln!("reparent raylib window into GPUI: {error}");
                    break;
                }
                attached_parent = Some(parent);
                geometry_synced = false;
                rl.clear_window_state(WindowState::default().set_window_hidden(true));
                eprintln!(
                    "raylib child window {child_id:#x} attached to GPUI X11 window {:#x}",
                    parent.get()
                );
            }

            let wanted = (layout.width.max(1), layout.height.max(1));
            let next_layout = (parent, layout.x, layout.y, wanted.0, wanted.1);
            if last_layout != Some(next_layout) {
                last_layout = Some(next_layout);
                geometry_synced = false;
                eprintln!(
                    "canvas bounds: ({}, {}) {}x{} at {:.2}x DPI",
                    layout.x, layout.y, wanted.0, wanted.1, layout.scale
                );
                if wanted != current_size {
                    current_size = wanted;
                    rl.set_window_size(wanted.0 as i32, wanted.1 as i32);
                }
            }
            if !geometry_synced {
                let geometry = connection
                    .get_geometry(child_id)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok());
                if geometry.is_some_and(|geometry| {
                    geometry.x == layout.x as i16
                        && geometry.y == layout.y as i16
                        && geometry.width == wanted.0 as u16
                        && geometry.height == wanted.1 as u16
                }) {
                    geometry_synced = true;
                } else {
                    if let Err(error) = connection.configure_window(
                        child_id,
                        &ConfigureWindowAux::new().x(layout.x).y(layout.y),
                    ) {
                        eprintln!("position raylib child window: {error}");
                        break;
                    }
                    let _ = connection.flush();
                }
            }
        }

        let pointer_delta = rl.get_mouse_delta();
        if rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT)
            && (pointer_delta.x != 0.0 || pointer_delta.y != 0.0)
        {
            sequence += 1;
            apply_input_event(
                ViewportEvent {
                    protocol_version: 1,
                    sequence,
                    input: ViewportInput::OrbitDelta {
                        dx_pixels: pointer_delta.x,
                        dy_pixels: pointer_delta.y,
                    },
                },
                &bridge,
                &mut last_sequence,
                &mut camera,
            );
        }
        let wheel = rl.get_mouse_wheel_move();
        if wheel != 0.0 {
            sequence += 1;
            apply_input_event(
                ViewportEvent {
                    protocol_version: 1,
                    sequence,
                    input: ViewportInput::ZoomSteps { steps: wheel },
                },
                &bridge,
                &mut last_sequence,
                &mut camera,
            );
        }
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            sequence += 1;
            apply_input_event(
                ViewportEvent {
                    protocol_version: 1,
                    sequence,
                    input: ViewportInput::ResetCamera,
                },
                &bridge,
                &mut last_sequence,
                &mut camera,
            );
        }

        let mut drawing = rl.begin_drawing(&thread);
        drawing.clear_background(Color::new(35, 41, 50, 255));
        {
            let mut scene = drawing.begin_mode3D(camera);
            scene.draw_grid(24, 1.0);
            scene.draw_cube(
                Vector3::new(0.0, 1.0, 0.0),
                2.0,
                2.0,
                2.0,
                Color::new(187, 197, 210, 255),
            );
            scene.draw_cube_wires(
                Vector3::new(0.0, 1.0, 0.0),
                2.0,
                2.0,
                2.0,
                Color::new(24, 29, 36, 255),
            );
        }
        drawing.draw_text(
            "GPUI layout + raylib OpenGL child surface",
            18,
            16,
            20,
            Color::RAYWHITE,
        );
        drawing.draw_text(
            &format!(
                "{}x{} at {:.2}x DPI | drag: orbit, wheel: zoom",
                current_size.0, current_size.1, layout.scale
            ),
            18,
            42,
            16,
            Color::new(180, 204, 228, 255),
        );
        thread::sleep(Duration::from_millis(1));
    }

    // A child window is destroyed recursively with its GPUI parent. Detach it
    // before the close callback allows that parent to disappear, so GLFW can
    // destroy its own X11 window and GL context without a stale-window error.
    if let Some(root) = root {
        let _ = connection.unmap_window(child_id);
        let _ = connection.reparent_window(child_id, root, 0, 0);
        let _ = connection.flush();
    }
}

fn intern_atom(connection: &impl Connection, name: &[u8]) -> Option<u32> {
    connection
        .intern_atom(false, name)
        .ok()?
        .reply()
        .ok()
        .map(|reply| reply.atom)
}

fn find_gpui_window(
    connection: &impl Connection,
    pid_atom: Option<u32>,
    title_atom: Option<u32>,
    utf8_atom: Option<u32>,
    process_id: u32,
) -> Option<NonZeroU32> {
    let pid_atom = pid_atom?;
    let root = connection.setup().roots.first()?.root;
    let mut pending = connection.query_tree(root).ok()?.reply().ok()?.children;

    while let Some(window) = pending.pop() {
        let pid = connection
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut value| value.next()));
        let net_title = title_atom
            .zip(utf8_atom)
            .and_then(|(title_atom, utf8_atom)| {
                connection
                    .get_property(false, window, title_atom, utf8_atom, 0, 128)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .map(|reply| reply.value)
            });
        let wm_title = connection
            .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 128)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.value);
        if pid == Some(process_id)
            && (net_title.is_some_and(|title| title.as_slice() == b"Meshcraft stack proof")
                || wm_title.is_some_and(|title| title.as_slice() == b"Meshcraft stack proof"))
        {
            return NonZeroU32::new(window);
        }

        if let Ok(tree) = connection.query_tree(window)
            && let Ok(tree) = tree.reply()
        {
            pending.extend(tree.children);
        }
    }
    None
}

fn apply_input_event(
    event: ViewportEvent,
    bridge: &ViewportBridge,
    last_sequence: &mut u64,
    camera: &mut Camera3D,
) {
    if event.protocol_version != 1 || event.sequence <= *last_sequence {
        return;
    }
    *last_sequence = event.sequence;
    bridge.record_input(event);

    match event.input {
        ViewportInput::OrbitDelta {
            dx_pixels,
            dy_pixels,
        } => camera.update_camera_pro(
            Vector3::zero(),
            Vector3::new(-dy_pixels * 0.25, -dx_pixels * 0.25, 0.0),
            0.0,
        ),
        ViewportInput::ZoomSteps { steps } => {
            camera.update_camera_pro(Vector3::zero(), Vector3::zero(), -steps);
        }
        ViewportInput::ResetCamera => {
            *camera = Camera3D::perspective(
                Vector3::new(8.0, 7.0, 8.0),
                Vector3::new(0.0, 0.8, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                45.0,
            );
        }
    }
}

fn physical_rect(x: f32, y: f32, width: f32, height: f32, scale: f32) -> (i32, i32, u32, u32) {
    (
        (x * scale).round() as i32,
        (y * scale).round() as i32,
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

fn viewport_rect(window_width: f32, window_height: f32, scale: f32) -> (i32, i32, u32, u32) {
    const LEFT_PANEL_WIDTH: f32 = 170.0;
    const RIGHT_PANEL_WIDTH: f32 = 260.0;
    const HEADER_HEIGHT: f32 = 38.0;
    const STATUS_HEIGHT: f32 = 26.0;

    physical_rect(
        LEFT_PANEL_WIDTH,
        HEADER_HEIGHT,
        (window_width - LEFT_PANEL_WIDTH - RIGHT_PANEL_WIDTH).max(1.0),
        (window_height - HEADER_HEIGHT - STATUS_HEIGHT).max(1.0),
        scale,
    )
}

fn publish_viewport_layout(bridge: &ViewportBridge, window: &Window) {
    let bounds = window.bounds();
    let scale = window.scale_factor();
    let (x, y, width, height) = viewport_rect(
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
        scale,
    );
    bridge.publish_layout(ViewportLayout {
        x,
        y,
        width,
        height,
        scale,
    });
}

#[cfg(test)]
mod tests {
    use super::{
        Camera3D, ViewportBridge, ViewportEvent, ViewportInput, apply_input_event, physical_rect,
        viewport_rect,
    };
    use raylib::prelude::Vector3;
    use std::sync::{Arc, Mutex};

    #[test]
    fn maps_layout_bounds_to_physical_pixels_at_high_dpi() {
        assert_eq!(
            physical_rect(170.0, 38.0, 970.0, 836.0, 2.0),
            (340, 76, 1940, 1672)
        );
    }

    #[test]
    fn tracks_central_canvas_size_when_host_window_resizes() {
        assert_eq!(viewport_rect(1400.0, 900.0, 1.0), (170, 38, 970, 836));
        assert_eq!(viewport_rect(1200.0, 760.0, 1.0), (170, 38, 770, 696));
    }

    #[test]
    fn rejects_unsupported_and_replayed_viewport_events() {
        let bridge = ViewportBridge {
            layout: Arc::new(Mutex::new(Default::default())),
            latest_event: Arc::new(Mutex::new(None)),
        };
        let mut last_sequence = 0;
        let mut camera = Camera3D::perspective(
            Vector3::new(8.0, 7.0, 8.0),
            Vector3::new(0.0, 0.8, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            45.0,
        );
        let event = ViewportEvent {
            protocol_version: 1,
            sequence: 2,
            input: ViewportInput::ZoomSteps { steps: 1.0 },
        };

        apply_input_event(
            ViewportEvent {
                protocol_version: 2,
                ..event
            },
            &bridge,
            &mut last_sequence,
            &mut camera,
        );
        apply_input_event(event, &bridge, &mut last_sequence, &mut camera);
        apply_input_event(
            ViewportEvent {
                sequence: 1,
                ..event
            },
            &bridge,
            &mut last_sequence,
            &mut camera,
        );

        assert_eq!(last_sequence, 2);
        assert_eq!(*bridge.latest_event.lock().unwrap(), Some(event));
    }
}

struct MeshcraftShell {
    bridge: ViewportBridge,
}

impl Render for MeshcraftShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e232b))
            .text_color(rgb(0xe5e8ed))
            .child(
                div()
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .gap(px(18.))
                    .px(px(16.))
                    .bg(rgb(0x171b22))
                    .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Meshcraft"))
                    .child(div().text_sm().text_color(rgb(0x91a8c2)).child("File   Edit   View   Create   Modify   Select   Tools   Window   Help"))
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(rgb(0x91a8c2)).child("GPUI 0.2.2  ·  raylib-rs 6.0.0  ·  X11/XWayland")),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        div()
                            .w(px(170.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .p(px(12.))
                            .bg(rgb(0x252b34))
                            .child(div().text_sm().font_weight(gpui::FontWeight::BOLD).child("COMMANDS"))
                            .child(div().text_sm().child("Select"))
                            .child(div().text_sm().child("Line"))
                            .child(div().text_sm().child("Move"))
                            .child(div().text_sm().child("Array")),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            |_, _, _, _| {},
                        )
                        .flex_1()
                        .h_full(),
                    )
                    .child(
                        div()
                            .w(px(260.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .p(px(12.))
                            .bg(rgb(0x252b34))
                            .child(div().text_sm().font_weight(gpui::FontWeight::BOLD).child("INSPECTOR"))
                            .child(div().text_sm().child("Selection"))
                            .child(div().text_sm().child("Model settings"))
                            .child(div().text_sm().child("Tool settings")),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .px(px(10.))
                    .bg(rgb(0x171b22))
                    .text_xs()
                    .text_color(rgb(0xaab5c3))
                    .child("Ready  ·  Select  ·  Native raylib canvas receives its own pointer/keyboard focus"),
            )
    }
}

fn main() {
    if gpui::guess_compositor() != "X11" {
        eprintln!(
            "launch with WAYLAND_DISPLAY unset; this proof requires GPUI and GLFW to share X11/XWayland"
        );
        std::process::exit(2);
    }

    let mut renderer = RaylibWorker::start();
    let bridge = renderer.bridge();
    let shutdown = renderer.shutdown_signal();
    Application::new().run(move |cx: &mut App| {
        let window_bounds = WindowBounds::centered(size(px(1400.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some("Meshcraft stack proof".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                window.on_window_should_close(cx, move |_, _| {
                    shutdown.stop_and_wait();
                    true
                });
                let bridge = bridge.clone();
                cx.new(|view_cx| {
                    publish_viewport_layout(&bridge, window);
                    view_cx
                        .observe_window_bounds(
                            window,
                            move |view: &mut MeshcraftShell,
                                  window: &mut Window,
                                  _cx: &mut Context<MeshcraftShell>| {
                                publish_viewport_layout(&view.bridge, window);
                            },
                        )
                        .detach();
                    MeshcraftShell { bridge }
                })
            },
        )
        .expect("open GPUI window");
        cx.activate(true);
    });
    renderer.stop();
}
