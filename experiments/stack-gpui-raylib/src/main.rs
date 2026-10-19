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
    App, Application, Context, StatefulInteractiveElement, Styled, TitlebarOptions, Window,
    WindowBounds, WindowOptions, canvas, div, prelude::*, px, rgb, size,
};
use gpui_component::{
    Disableable, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
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
        } => orbit_camera(camera, dx_pixels, dy_pixels),
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

fn orbit_camera(camera: &mut Camera3D, dx_pixels: f32, dy_pixels: f32) {
    const ORBIT_SENSITIVITY: f32 = 0.003;
    const MAX_PITCH: f32 = 85.0_f32.to_radians();

    let target = camera.target;
    let offset_x = camera.position.x - target.x;
    let offset_y = camera.position.y - target.y;
    let offset_z = camera.position.z - target.z;
    let radius = (offset_x * offset_x + offset_y * offset_y + offset_z * offset_z).sqrt();
    if radius <= f32::EPSILON {
        return;
    }

    let mut yaw = offset_x.atan2(offset_z);
    let mut pitch = offset_y.atan2((offset_x * offset_x + offset_z * offset_z).sqrt());
    yaw -= dx_pixels * ORBIT_SENSITIVITY;
    pitch = (pitch - dy_pixels * ORBIT_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);

    let horizontal_radius = radius * pitch.cos();
    camera.position = Vector3::new(
        target.x + horizontal_radius * yaw.sin(),
        target.y + radius * pitch.sin(),
        target.z + horizontal_radius * yaw.cos(),
    );
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
    const LEFT_PANEL_WIDTH: f32 = 230.0;
    const RIGHT_PANEL_WIDTH: f32 = 330.0;
    const HEADER_HEIGHT: f32 = 140.0;
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
        Camera3D, ViewportBridge, ViewportEvent, ViewportInput, apply_input_event, orbit_camera,
        physical_rect, viewport_rect,
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
        assert_eq!(viewport_rect(1400.0, 900.0, 1.0), (230, 140, 840, 734));
        assert_eq!(viewport_rect(1200.0, 760.0, 1.0), (230, 140, 640, 594));
    }

    #[test]
    fn horizontal_orbit_delta_yaws_around_the_fixed_target() {
        let mut camera = Camera3D::perspective(
            Vector3::new(8.0, 7.0, 8.0),
            Vector3::new(0.0, 0.8, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            45.0,
        );
        let target = camera.target;
        let initial_y = camera.position.y;
        let initial_radius = distance(camera.position, camera.target);

        orbit_camera(&mut camera, 80.0, 0.0);

        assert_eq!(camera.target, target);
        assert!((camera.position.y - initial_y).abs() < 1e-5);
        assert!((distance(camera.position, camera.target) - initial_radius).abs() < 1e-4);
        assert!((camera.position.x - 8.0).abs() > 1e-3 || (camera.position.z - 8.0).abs() > 1e-3);
    }

    #[test]
    fn vertical_orbit_delta_pitches_around_the_fixed_target() {
        let mut camera = Camera3D::perspective(
            Vector3::new(8.0, 7.0, 8.0),
            Vector3::new(0.0, 0.8, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            45.0,
        );
        let target = camera.target;
        let initial_radius = distance(camera.position, camera.target);
        let initial_y = camera.position.y;

        orbit_camera(&mut camera, 0.0, 80.0);

        assert_eq!(camera.target, target);
        assert!((camera.position.y - initial_y).abs() > 1e-3);
        assert!((distance(camera.position, camera.target) - initial_radius).abs() < 1e-4);
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

    fn distance(a: Vector3, b: Vector3) -> f32 {
        let x = a.x - b.x;
        let y = a.y - b.y;
        let z = a.z - b.z;
        (x * x + y * y + z * z).sqrt()
    }
}

struct MeshcraftShell {
    bridge: ViewportBridge,
}

fn disabled_button(namespace: &'static str, command: &'static str, label: &'static str) -> Button {
    Button::new(gpui::SharedString::from(format!("{namespace}/{command}")))
        .label(label)
        .disabled(true)
        .small()
        .ghost()
        .compact()
        .tooltip(command)
}

fn menu_button(menu: &'static str) -> Button {
    Button::new(gpui::SharedString::from(format!(
        "menu/{}",
        menu.to_lowercase()
    )))
    .label(menu)
    .disabled(true)
    .xsmall()
    .text()
}

fn command_row(
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(4.))
        .children(
            commands
                .iter()
                .map(|(command, label)| disabled_button(namespace, command, label)),
        )
}

fn palette(
    title: &'static str,
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .p(px(7.))
        .border_1()
        .border_color(rgb(0x3b4654))
        .bg(rgb(0x242b34))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgb(0xc4d6eb))
                .child(title),
        )
        .child(
            div().flex().flex_wrap().gap(px(3.)).children(
                commands
                    .iter()
                    .map(|(command, label)| disabled_button(namespace, command, label).w(px(66.))),
            ),
        )
}

fn ribbon_group(
    title: &'static str,
    namespace: &'static str,
    commands: &'static [(&'static str, &'static str)],
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .gap(px(4.))
        .px(px(7.))
        .py(px(4.))
        .border_r_1()
        .border_color(rgb(0x3b4654))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0x9caec3))
                .child(title),
        )
        .child(
            div().flex().flex_wrap().gap(px(3.)).children(
                commands
                    .iter()
                    .map(|(command, label)| disabled_button(namespace, command, label).w(px(76.))),
            ),
        )
}

fn setting_field(label: &'static str, value: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.))
        .gap(px(3.))
        .child(div().text_xs().text_color(rgb(0xc2c8d0)).child(label))
        .child(
            div()
                .h(px(25.))
                .flex()
                .items_center()
                .px(px(7.))
                .border_1()
                .border_color(rgb(0x454b53))
                .bg(rgb(0x292e34))
                .text_sm()
                .text_color(rgb(0x9299a2))
                .child(value),
        )
}

fn setting_pair(
    left_label: &'static str,
    left_value: &'static str,
    right_label: &'static str,
    right_value: &'static str,
) -> impl IntoElement {
    div()
        .flex()
        .gap(px(8.))
        .child(setting_field(left_label, left_value))
        .child(setting_field(right_label, right_value))
}

fn setting_check(label: &'static str, checked: bool) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_sm()
        .text_color(rgb(0xaab2bd))
        .child(
            div()
                .w(px(14.))
                .h(px(14.))
                .border_1()
                .border_color(rgb(0x646d79))
                .bg(if checked {
                    rgb(0x3d6388)
                } else {
                    rgb(0x30363e)
                }),
        )
        .child(label)
}

fn setting_section(title: &'static str, controls: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .border_1()
        .border_color(rgb(0x414a55))
        .bg(rgb(0x343a42))
        .child(
            div()
                .px(px(8.))
                .py(px(5.))
                .bg(rgb(0x24537a))
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(0xe0ebf7))
                .child(title),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .px(px(8.))
                .pb(px(8.))
                .child(controls),
        )
}

fn stat_row(label: &'static str, value: &'static str) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .gap(px(8.))
        .text_sm()
        .text_color(rgb(0xc0c6cf))
        .child(label)
        .child(div().text_color(rgb(0x8f98a4)).child(value))
}

fn left_toolbars() -> impl IntoElement {
    div()
        .w(px(230.))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .p(px(7.))
        .bg(rgb(0x1e242c))
        .id("toolbar/palettes-scroll")
        .overflow_y_scroll()
        .child(palette(
            "Point Construction",
            "toolbar/point-construction",
            &[
                ("point", "Point"),
                ("line", "Line"),
                ("polyline", "Pline"),
                ("double-line", "2-Line"),
                ("meshed-double-line", "Mesh 2L"),
                ("closed-mesh", "Planar Mesh"),
                ("rectangle", "Rect"),
                ("circle", "Circle"),
                ("arc", "Arc"),
                ("hotspot", "Hotspot"),
            ],
        ))
        .child(palette(
            "Actions",
            "toolbar/actions",
            &[
                ("open", "Open"),
                ("save", "Save"),
                ("undo", "Undo"),
                ("redo", "Redo"),
                ("copy", "Copy"),
                ("delete", "Delete"),
                ("create-object", "Object"),
                ("explode", "Explode"),
                ("paste", "Paste"),
                ("array", "Array"),
            ],
        ))
        .child(palette(
            "Modification",
            "toolbar/modification",
            &[
                ("select", "Select"),
                ("move", "Move"),
                ("rotate-3d", "Rotate 3D"),
                ("rotate-copy", "Rotate Copy"),
                ("scale", "Scale"),
                ("stretch", "Stretch"),
                ("extrude", "Extrude"),
                ("mirror", "Mirror"),
                ("offset", "Offset"),
                ("sweep", "Sweep"),
                ("fillet", "Fillet"),
                ("chamfer", "Chamfer"),
            ],
        ))
        .child(palette(
            "Entity Construction",
            "toolbar/entity-construction",
            &[
                ("dimension", "Dim"),
                ("text", "Text"),
                ("leader", "Leader"),
                ("wall", "Wall"),
                ("slab", "Slab"),
                ("stair", "Stair"),
                ("frame", "Frame"),
                ("voxel", "Voxel"),
                ("pipe", "Pipe"),
                ("duct", "Duct"),
            ],
        ))
        .child(palette(
            "Rendering",
            "toolbar/rendering",
            &[
                ("ray-trace", "RT"),
                ("global-illumination", "GI"),
                ("stop-render", "Stop"),
                ("save-render", "Save"),
            ],
        ))
        .child(palette(
            "Architecture",
            "toolbar/architecture",
            &[
                ("wall", "Wall"),
                ("slab", "Slab"),
                ("stair", "Stair"),
                ("frame", "Frame"),
                ("door", "Door"),
                ("window", "Window"),
            ],
        ))
        .child(palette(
            "Camera",
            "toolbar/camera",
            &[
                ("reset-camera", "Reset"),
                ("front-view", "Front"),
                ("top-view", "Top"),
                ("orbit", "Orbit"),
                ("pan", "Pan"),
                ("zoom", "Zoom"),
            ],
        ))
        .child(palette(
            "Modification 2",
            "toolbar/modification-2",
            &[
                ("planar-regularize", "Regularize"),
                ("surface-remesh", "Remesh"),
                ("random-offset", "Fuzzy Offset"),
                ("surface-array", "Surface Array"),
                ("mesh-intersection", "Intersect"),
            ],
        ))
        .child(palette(
            "Mech",
            "toolbar/mech",
            &[
                ("beam", "Beam"),
                ("circle-profile", "Circle"),
                ("ring-profile", "Ring"),
                ("spring", "Spring"),
            ],
        ))
        .child(palette(
            "Spatial Arrays",
            "toolbar/arrays",
            &[
                ("linear-array", "Linear"),
                ("planar-array", "Planar"),
                ("spatial-array", "Spatial"),
                ("rotational-array", "Rotate"),
                ("helical-array", "Helical"),
            ],
        ))
        .child(palette(
            "Primitives",
            "toolbar/primitives",
            &[
                ("sphere", "Sphere"),
                ("cylinder", "Cylinder"),
                ("cone", "Cone"),
                ("tube", "Tube"),
            ],
        ))
        .child(palette(
            "Voxel",
            "toolbar/voxel",
            &[
                ("voxel", "Voxel"),
                ("voxel-fill", "Fill"),
                ("voxel-edit", "Edit"),
            ],
        ))
        .child(palette(
            "In-tool Operators",
            "toolbar/in-tool-operators",
            &[
                ("select-operator", "Select"),
                ("distance-operator", "Distance"),
            ],
        ))
        .child(palette(
            "HVAC",
            "toolbar/hvac",
            &[("plumbing", "Plumbing"), ("ventilation", "Ventilation")],
        ))
}

fn right_settings_panel() -> impl IntoElement {
    div()
        .w(px(330.))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(8.))
        .bg(rgb(0x20262e))
        .id("panel/settings-scroll")
        .overflow_y_scroll()
        .child(setting_section(
            "Selection",
            div().flex().flex_col().gap(px(4.)).children([
                stat_row("Type                         T    S    D    M    W", ""),
                stat_row("Edges", "0    0    ✓    ✓    □"),
                stat_row("Faces", "0    0    ✓    ✓    □"),
                stat_row("Voxels", "0    0    ✓    ✓    □"),
                stat_row("Hotspots", "0    0    ✓    ✓    □"),
                stat_row("Objects", "0    0    ✓    ✓    □"),
                stat_row("Dimensions / Texts", "0    0    ✓    ✓    □"),
                stat_row("Walls / Slabs / Stairs", "0    0    ✓    ✓    □"),
            ]),
        ))
        .child(setting_section(
            "Object",
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xa5adb7))
                        .child("No object selected"),
                )
                .child(setting_field("Name", ""))
                .child(command_row(
                    "panel/object",
                    &[
                        ("rename-object", "Rename"),
                        ("edit-definition", "Edit Definition"),
                    ],
                )),
        ))
        .child(setting_section(
            "Objects",
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xa5adb7))
                        .child("0 object instances in the active scene"),
                )
                .child(setting_field("Object list", "Empty scene"))
                .child(command_row(
                    "panel/objects",
                    &[
                        ("create-object", "Create Object"),
                        ("import-objects", "Import Objects"),
                    ],
                )),
        ))
        .child(setting_section(
            "Model Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Unit name", "unit"))
                .child(setting_pair("Unit size", "1.0000", "Grid size", "1.0000"))
                .child(setting_pair("Circle segments", "24", "Snap radius", "12.0"))
                .child(setting_pair(
                    "Walk speed",
                    "24.000",
                    "Jump velocity",
                    "16.000",
                ))
                .child(setting_pair(
                    "Gravity",
                    "48.000",
                    "Height adjust speed",
                    "8.000",
                ))
                .child(command_row(
                    "panel/model-settings",
                    &[
                        ("apply-model-settings", "Apply Settings"),
                        ("reset-model-settings", "Reset Defaults"),
                    ],
                )),
        ))
        .child(setting_section(
            "UI Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "UI text size",
                    "1x",
                    "Toolbar size",
                    "32 × 32",
                ))
                .child(setting_check("Auto-collapse toolbars", false))
                .child(setting_check("Show permanent grid", true))
                .child(setting_field("Grid normal / extent", "Y up / 20 steps"))
                .child(command_row(
                    "panel/ui-settings",
                    &[
                        ("flow-left-to-right", "Flow L→R"),
                        ("flow-top-to-down", "Flow T→D"),
                    ],
                )),
        ))
        .child(setting_section(
            "Polyline & Vector Text",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Arc max length",
                    "0.500",
                    "Double line size",
                    "1.000",
                ))
                .child(setting_pair(
                    "Double line offset",
                    "0.500",
                    "Text size",
                    "1.000",
                ))
                .child(setting_pair("Tracking", "0.100", "Line spacing", "1.250"))
                .child(setting_field("Glyph source", "embedded:alphabet"))
                .child(command_row(
                    "panel/vector-text",
                    &[
                        ("load-glyph-source", "Load Glyphs"),
                        ("browse-glyph-source", "Browse…"),
                    ],
                )),
        ))
        .child(setting_section(
            "Architecture — Walls / Slabs / Stairs / Frames",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair(
                    "Wall thickness",
                    "0.200",
                    "Wall height",
                    "2.700",
                ))
                .child(setting_pair(
                    "Slab thickness",
                    "0.200",
                    "Stair height / steps",
                    "2.700 / 14",
                ))
                .child(setting_pair("Frame depth", "0.120", "Frame width", "0.060"))
                .child(setting_check("Stair left and right rail grids", true))
                .child(setting_field(
                    "Material colors",
                    "Exterior / interior / glazing",
                )),
        ))
        .child(setting_section(
            "HVAC Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair("Pipe diameter", "0.200", "Pipe sides", "16"))
                .child(setting_pair("Duct width", "0.500", "Duct height", "0.250"))
                .child(setting_pair(
                    "Hump span",
                    "0.625",
                    "Hump clearance",
                    "0.050",
                ))
                .child(setting_check("Ventilation auto-join", true))
                .child(setting_field(
                    "Pipe / duct colors",
                    "Default material colors",
                )),
        ))
        .child(setting_section(
            "Hotspot Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Name", ""))
                .child(setting_pair("Operation", "MOVE", "Shape", "CIRCLE"))
                .child(setting_field("Color", "338cf2ff"))
                .child(stat_row("Attached geometry", "0 edges · 0 faces"))
                .child(stat_row("Reference", "none"))
                .child(command_row(
                    "panel/hotspots",
                    &[
                        ("attach-selection", "Attach"),
                        ("select-attached", "Select Attached"),
                        ("pick-reference", "Reference"),
                        ("add-hotspot", "Add At Cursor"),
                        ("delete-hotspot", "Delete"),
                    ],
                )),
        ))
        .child(setting_section(
            "Random Tools & Surface Remesh",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_field("Fuzzy offset strength [0…100]", "1"))
                .child(setting_pair("Copy count", "12", "Scale fuzz [0…100]", "0"))
                .child(setting_pair(
                    "Rotation fuzz [0…100]",
                    "0",
                    "Subdivisions",
                    "12",
                ))
                .child(setting_check("Align to surface normal", true))
                .child(setting_field("Projection tolerance", "0.05")),
        ))
        .child(setting_section(
            "Tutorials",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(stat_row("Mode / step", "Idle · 0 / 0"))
                .child(setting_field("Tutorial", "01A. Full House Exterior"))
                .child(setting_field("Lesson", "01. Lines / Extrude-swipe"))
                .child(command_row(
                    "panel/tutorials",
                    &[
                        ("record", "Record"),
                        ("stop-recording", "Stop Rec"),
                        ("play", "Play"),
                        ("pause", "Pause"),
                        ("stop", "Stop"),
                    ],
                )),
        ))
        .child(setting_section(
            "Shader Settings",
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(setting_pair("Shadow value", "0.59", "Shadow alpha", "0.50"))
                .child(setting_pair(
                    "Directional light",
                    "0.73",
                    "Ambient light",
                    "0.59",
                ))
                .child(setting_pair(
                    "Specular value",
                    "0.20",
                    "Specular alpha",
                    "0.95",
                ))
                .child(setting_pair("Shadow bias", "2500", "Normal bias", "5620"))
                .child(setting_check("Use CSM", true))
                .child(setting_check("Dither shadows", false)),
        ))
        .child(setting_section(
            "Help / About",
            div()
                .flex()
                .flex_col()
                .gap(px(5.))
                .text_sm()
                .text_color(rgb(0xa5adb7))
                .child("Meshcraft · GPUI shell with native raylib viewport")
                .child("Desktop · Commands are headless and UI-independent"),
        ))
}

impl Render for MeshcraftShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1b2027))
            .text_color(rgb(0xe5e8ed))
            .child(
                div()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(9.))
                    .bg(rgb(0x171b22))
                    .child(
                        div()
                            .mr(px(7.))
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(0xe5edf7))
                            .child("Meshcraft"),
                    )
                    .children(
                        [
                            "File", "Edit", "View", "Insert", "Format", "Draw", "Dimension",
                            "Modify", "Objects", "Tools", "Render", "Window", "Help",
                        ]
                        .into_iter()
                        .map(menu_button),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("menu/commands")
                            .label("Commands  Ctrl+Shift+P")
                            .disabled(true)
                            .small()
                            .primary(),
                    )
                    .child(
                        div()
                            .ml(px(7.))
                            .text_xs()
                            .text_color(rgb(0x91a8c2))
                            .child("GPUI 0.2.2 · raylib 6.0.0"),
                    ),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(9.))
                    .bg(rgb(0x232a33))
                    .children(
                        [
                            "Home", "Construction", "Modify", "Arrays", "Architecture",
                            "Inspect", "View", "Settings",
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, tab)| {
                            let button = Button::new(gpui::SharedString::from(format!(
                                "ribbon-tab/{}",
                                tab.to_lowercase()
                            )))
                                .label(tab)
                                .disabled(true)
                                .small();
                            if index == 0 {
                                button.primary()
                            } else {
                                button.ghost()
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(78.))
                    .flex()
                    .gap(px(3.))
                    .px(px(5.))
                    .bg(rgb(0x20262e))
                    .child(ribbon_group(
                        "Construction",
                        "ribbon/construction",
                        &[
                            ("point", "Point"), ("line", "Line"), ("polyline", "Polyline"),
                            ("double-line", "2-Line"), ("meshed-double-line", "Mesh 2-Line"),
                            ("closed-mesh", "Planar Mesh"), ("rectangle", "Rectangle"),
                            ("circle", "Circle"),
                        ],
                    ))
                    .child(ribbon_group(
                        "Modify Selection",
                        "ribbon/modify",
                        &[
                            ("select", "Select"), ("window-select", "Window"),
                            ("move", "Move"), ("copy", "Copy"), ("rotate-3d", "Rotate 3D"),
                            ("scale", "Scale"), ("stretch", "Stretch"), ("extrude", "Extrude"),
                        ],
                    ))
                    .child(ribbon_group(
                        "Arrays & Objects",
                        "ribbon/arrays",
                        &[
                            ("create-object", "Make Object"), ("edit-object", "Edit Object"),
                            ("linear-array", "Linear"), ("planar-array", "Planar"),
                            ("spatial-array", "Spatial"), ("rotational-array", "Rotate"),
                            ("helical-array", "Helical"),
                        ],
                    ))
                    .child(ribbon_group(
                        "Entities & View",
                        "ribbon/entities",
                        &[
                            ("dimension", "Dimension"), ("hotspot", "Hotspot"),
                            ("wall", "Wall"), ("slab", "Slab"), ("stair", "Stair"),
                            ("frame", "Frame"), ("orbit", "Orbit"), ("zoom", "Zoom"),
                        ],
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(left_toolbars())
                    .child(
                        canvas(
                            |_, _, _| (),
                            |_, _, _, _| {},
                        )
                        .flex_1()
                        .min_w(px(0.))
                        .h_full(),
                    )
                    .child(right_settings_panel()),
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
                    .child("Ready · Empty document · Select  |  raylib viewport · Shell controls disabled for this stack proof"),
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
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
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
