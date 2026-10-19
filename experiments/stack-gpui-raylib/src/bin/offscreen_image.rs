use std::{
    slice,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use gpui::{
    App, Application, Context, ImageSource, RenderImage, Styled, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, img, prelude::*, px, rgb, size,
};
use image::{Frame, ImageBuffer, Rgba};
use raylib::{
    consts::PixelFormat,
    prelude::{
        Camera3D, Color, RaylibDraw, RaylibDraw3D, RaylibMode3DExt, RaylibRenderTexture2D,
        RaylibTexture2D, RaylibTextureModeExt, TraceLogLevel, Vector3,
    },
};

const FRAME_WIDTH: u32 = 1280;
const FRAME_HEIGHT: u32 = 800;
const TARGET_FRAME_TIME: Duration = Duration::from_micros(16_667);

#[derive(Clone)]
struct ViewFrame {
    image: Arc<RenderImage>,
    sequence: u64,
    frame_ms: f32,
    readback_ms: f32,
    conversion_ms: f32,
}

#[derive(Clone, Default)]
struct FrameBridge {
    latest: Arc<Mutex<Option<ViewFrame>>>,
}

impl FrameBridge {
    fn publish(&self, frame: ViewFrame) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some(frame);
        }
    }

    fn latest(&self) -> Option<ViewFrame> {
        self.latest.lock().ok().and_then(|latest| latest.clone())
    }
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

struct RaylibWorker {
    bridge: FrameBridge,
    shutdown: ShutdownSignal,
    thread: Option<JoinHandle<()>>,
}

impl RaylibWorker {
    fn start() -> Self {
        let bridge = FrameBridge::default();
        let worker_bridge = bridge.clone();
        let shutdown = ShutdownSignal {
            stop: Arc::new(AtomicBool::new(false)),
            finished: Arc::new((Mutex::new(false), Condvar::new())),
        };
        let worker_stop = shutdown.stop.clone();
        let worker_finished = shutdown.finished.clone();
        let thread = thread::Builder::new()
            .name("raylib-offscreen-renderer".into())
            .spawn(move || render_loop(worker_bridge, worker_stop, worker_finished))
            .expect("start offscreen raylib renderer");
        Self {
            bridge,
            shutdown,
            thread: Some(thread),
        }
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

fn render_loop(bridge: FrameBridge, stop: Arc<AtomicBool>, finished: Arc<(Mutex<bool>, Condvar)>) {
    let _finished = RenderThreadGuard(finished);
    let (mut rl, thread) = raylib::init()
        .log_level(TraceLogLevel::LOG_WARNING)
        .size(FRAME_WIDTH as i32, FRAME_HEIGHT as i32)
        .title("Meshcraft offscreen render target")
        .hidden()
        .build();
    let mut target = match rl.load_render_texture(&thread, FRAME_WIDTH, FRAME_HEIGHT) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("create raylib offscreen target: {error}");
            return;
        }
    };

    let scene_start = Instant::now();
    let target_point = Vector3::new(0.0, 0.8, 0.0);
    let mut sequence = 0_u64;

    while !stop.load(Ordering::Acquire) {
        let frame_start = Instant::now();
        let turn = scene_start.elapsed().as_secs_f32() * 0.25;
        let camera = Camera3D::perspective(
            Vector3::new(11.3 * turn.cos(), 7.0, 11.3 * turn.sin()),
            target_point,
            Vector3::new(0.0, 1.0, 0.0),
            45.0,
        );

        {
            let mut drawing = rl.begin_texture_mode(&thread, &mut target);
            drawing.clear_background(Color::new(224, 236, 247, 255));
            {
                let mut scene = drawing.begin_mode3D(camera);
                draw_workplane_grid(&mut scene);
                scene.draw_cube(target_point, 2.0, 2.0, 2.0, Color::new(187, 197, 210, 255));
                scene.draw_cube_wires(target_point, 2.0, 2.0, 2.0, Color::new(24, 29, 36, 255));
            }
            drawing.draw_text(
                "raylib 6 RenderTexture2D -> GPUI ImageSource::Render",
                18,
                16,
                20,
                Color::new(36, 48, 61, 255),
            );
        }

        let Some((image, readback_ms, conversion_ms)) = copy_target_to_gpui_image(&target) else {
            eprintln!("could not copy raylib render texture to a GPUI image");
            return;
        };
        sequence += 1;
        bridge.publish(ViewFrame {
            image: Arc::new(RenderImage::new(vec![Frame::new(image)])),
            sequence,
            frame_ms: frame_start.elapsed().as_secs_f32() * 1_000.0,
            readback_ms,
            conversion_ms,
        });

        if sequence % 30 == 0 {
            eprintln!(
                "offscreen: frame {sequence}, {:.1} ms total, {:.1} ms GPU readback, {:.1} ms CPU conversion",
                frame_start.elapsed().as_secs_f32() * 1_000.0,
                readback_ms,
                conversion_ms,
            );
        }
        let elapsed = frame_start.elapsed();
        if elapsed < TARGET_FRAME_TIME {
            thread::sleep(TARGET_FRAME_TIME - elapsed);
        }
    }
}

fn copy_target_to_gpui_image(
    target: &impl RaylibRenderTexture2D,
) -> Option<(ImageBuffer<Rgba<u8>, Vec<u8>>, f32, f32)> {
    let readback_start = Instant::now();
    let image = target.texture().load_image().ok()?;
    let readback_ms = readback_start.elapsed().as_secs_f32() * 1_000.0;
    let conversion_start = Instant::now();
    let width = u32::try_from(image.width()).ok()?;
    let height = u32::try_from(image.height()).ok()?;
    if image.format() != PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 {
        eprintln!("unexpected raylib readback format: {:?}", image.format());
        return None;
    }

    let byte_count = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    // LoadImageFromTexture owns this tightly packed RGBA buffer until `image` drops.
    let rgba = unsafe { slice::from_raw_parts(image.data().cast::<u8>(), byte_count) };
    let mut bgra = rgba.to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    let mut output = ImageBuffer::from_raw(width, height, bgra)?;
    image::imageops::flip_vertical_in_place(&mut output);
    Some((
        output,
        readback_ms,
        conversion_start.elapsed().as_secs_f32() * 1_000.0,
    ))
}

fn draw_workplane_grid(scene: &mut impl RaylibDraw3D) {
    const EXTENT: i32 = 24;
    let minor = Color::new(132, 149, 164, 255);
    let major = Color::new(83, 103, 121, 255);
    for step in -EXTENT..=EXTENT {
        let position = step as f32;
        let color = if step % 5 == 0 { major } else { minor };
        scene.draw_line3D(
            Vector3::new(-(EXTENT as f32), 0.0, position),
            Vector3::new(EXTENT as f32, 0.0, position),
            color,
        );
        scene.draw_line3D(
            Vector3::new(position, 0.0, -(EXTENT as f32)),
            Vector3::new(position, 0.0, EXTENT as f32),
            color,
        );
    }
    scene.draw_line3D(
        Vector3::new(-(EXTENT as f32), 0.005, 0.0),
        Vector3::new(EXTENT as f32, 0.005, 0.0),
        Color::new(190, 91, 91, 255),
    );
    scene.draw_line3D(
        Vector3::new(0.0, 0.005, -(EXTENT as f32)),
        Vector3::new(0.0, 0.005, EXTENT as f32),
        Color::new(78, 120, 177, 255),
    );
}

struct OffscreenShell {
    bridge: FrameBridge,
    last_seen_sequence: u64,
    frames_since_report: u64,
    report_started: Instant,
}

impl Render for OffscreenShell {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let frame = self.bridge.latest();
        if let Some(frame) = frame.as_ref()
            && frame.sequence > self.last_seen_sequence
        {
            self.frames_since_report += frame.sequence - self.last_seen_sequence;
            self.last_seen_sequence = frame.sequence;
            if self.frames_since_report >= 60 {
                let elapsed = self.report_started.elapsed().as_secs_f32();
                eprintln!(
                    "gpui consumer: {:.1} new images/sec",
                    self.frames_since_report as f32 / elapsed
                );
                self.frames_since_report = 0;
                self.report_started = Instant::now();
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x20262f))
            .child(
                div()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .bg(rgb(0x171b21))
                    .text_color(rgb(0xd8e1ec))
                    .child("Meshcraft")
                    .child("File   Edit   View   Insert   Format   Draw   Modify   Tools   Render"),
            )
            .child(
                div()
                    .h(px(70.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(18.))
                    .bg(rgb(0x242c37))
                    .text_color(rgb(0xc4d2e2))
                    .child("Home     Construction     Modify     Arrays     Architecture     Inspect     View     Settings")
                    .child("   Select     Line     Mesh     Move     Rotate     Array"),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(
                        div()
                            .w(px(220.))
                            .p(px(12.))
                            .text_color(rgb(0xc4d2e2))
                            .child("Construction\n\nPoint\nLine\nPolyline\nPlanar Mesh\n\nActions\n\nOpen\nSave\nUndo\nRedo"),
                    )
                    .child(
                        div()
                            .id("offscreen-viewport")
                            .relative()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .bg(rgb(0xe0ecf7))
                            .child(match frame.as_ref() {
                                Some(frame) => img(ImageSource::Render(frame.image.clone()))
                                    .size_full()
                                    .into_any_element(),
                                None => div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0x334155))
                                    .child("Waiting for first offscreen frame…")
                                    .into_any_element(),
                            })
                            .child(
                                div()
                                    .absolute()
                                    .top(px(12.))
                                    .left(px(12.))
                                    .px(px(10.))
                                    .py(px(7.))
                                    .rounded(px(4.))
                                    .bg(rgb(0x1d2937))
                                    .text_color(rgb(0xf4f7fb))
                                    .child(match frame.as_ref() {
                                        Some(frame) => format!(
                                            "GPUI overlay above raylib image · frame {} · {:.1} ms readback + {:.1} ms copy / {:.1} ms total",
                                            frame.sequence, frame.readback_ms, frame.conversion_ms, frame.frame_ms
                                        ),
                                        None => "Normal GPUI layer over viewport image".into(),
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(270.))
                            .p(px(12.))
                            .bg(rgb(0x2c333d))
                            .text_color(rgb(0xd6e0eb))
                            .child("Selection\n\nEdges        0\nFaces        0\nObjects      0\n\nModel Settings\n\nUnit name\nunit\n\nGrid size\n1.0000"),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .bg(rgb(0x171b21))
                    .text_color(rgb(0xaeb8c4))
                    .child("Offscreen raylib frame displayed as a normal GPUI image element"),
            )
    }
}

fn main() {
    if gpui::guess_compositor() != "X11" {
        eprintln!("launch with WAYLAND_DISPLAY unset; this proof requires X11/XWayland");
        std::process::exit(2);
    }
    let mut renderer = RaylibWorker::start();
    let bridge = renderer.bridge.clone();
    let shutdown = renderer.shutdown.clone();

    Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        let window_bounds = WindowBounds::centered(size(px(1400.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some("Meshcraft offscreen viewport proof".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let shutdown = shutdown.clone();
                window.on_window_should_close(cx, move |_, _| {
                    shutdown.stop_and_wait();
                    true
                });
                cx.new(|_| OffscreenShell {
                    bridge,
                    last_seen_sequence: 0,
                    frames_since_report: 0,
                    report_started: Instant::now(),
                })
            },
        )
        .expect("open offscreen image proof window");
    });
    renderer.stop();
}
