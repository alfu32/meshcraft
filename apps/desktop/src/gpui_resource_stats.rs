//! A separately invalidated, fixed-width status HUD; telemetry never refreshes the scene.
use super::*;
use crate::resource_stats::{self, GpuMemory, GpuScope, Memory, Monitor, Snapshot};
use std::ffi::CStr;

pub(super) const RAM_WIDTH: f32 = 238.;
pub(super) const CPU_WIDTH: f32 = 96.;
pub(super) const GPU_WIDTH: f32 = 280.;
pub(super) const FRAME_WIDTH: f32 = 220.;
const GAP: f32 = 8.;
pub(super) const HUD_WIDTH: f32 = RAM_WIDTH + CPU_WIDTH + GPU_WIDTH + FRAME_WIDTH + 3. * GAP;

pub(super) struct Hud {
    bridge: ViewportBridge,
    monitor: Monitor,
    snapshot: Snapshot,
    watching: bool,
}
impl Hud {
    pub fn new(bridge: ViewportBridge) -> Self {
        Self {
            bridge,
            monitor: Monitor::start(),
            snapshot: Snapshot::default(),
            watching: false,
        }
    }
}
fn cell(id: &'static str, text: String, width: f32, tooltip: String) -> impl IntoElement {
    div()
        .id(id)
        .w(px(width))
        .min_w(px(width))
        .flex_shrink_0()
        .truncate()
        .child(text)
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
}
impl Render for Hud {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.watching {
            self.watching = true;
            cx.spawn_in(window, async move |hud, cx| {
                loop {
                    gpui::Timer::after(resource_stats::SAMPLE_INTERVAL).await;
                    if hud
                        .update(cx, |hud, cx| {
                            hud.snapshot = hud.monitor.snapshot();
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        let gpu = resource_stats::gpu_choice(
            self.snapshot.gpu.clone(),
            self.bridge.gpu_memory.lock().ok().and_then(|v| v.clone()),
        );
        let (gpu_label, gpu_value, gpu_tip) = match gpu {
            None => (
                "VRAM", "N/A".to_owned(),
                "VRAM usage is unavailable from this driver. Shared/unified RAM is not presented as dedicated VRAM.".to_owned(),
            ),
            Some(g) => {
                let device = g.scope == GpuScope::Device;
                let label = if device { "GPU total" } else { "VRAM app" };
                let scope = if device {
                    "GPU-wide usage includes other applications."
                } else {
                    "Driver-reported Meshcraft process usage."
                };
                (label, resource_stats::memory_text(Some(g.memory)), format!(
                    "{}\nDedicated capacity: {} bytes. {scope}", g.source, g.memory.total,
                ))
            }
        };
        let frame = self.bridge.latest_frame();
        let frame_text = frame.as_ref().map_or_else(
            || "Frame N/A".to_owned(),
            |f| format!("Frame {:>10} · {:>7.1} ms", f.sequence, f.frame_ms),
        );
        let cpu = self
            .snapshot
            .cpu
            .map_or_else(|| "N/A".into(), |v| format!("{v:.1}%"));
        let ram_tip = format!(
            "Meshcraft resident RAM / total physical RAM. Capacity: {} bytes. Updated once per second.",
            self.snapshot.ram.map_or(0, |r| r.total)
        );
        div().flex().items_center().gap(px(GAP)).w(px(HUD_WIDTH)).min_w(px(HUD_WIDTH)).flex_shrink_0()
            .font_family(cx.global::<Theme>().mono_font_family.clone()).text_size(px(11.)).overflow_hidden()
            .child(cell("resource-ram",format!("RAM {}",resource_stats::memory_text(self.snapshot.ram)),RAM_WIDTH,ram_tip))
            .child(cell("resource-cpu",format!("CPU {cpu:>6}"),CPU_WIDTH,"Meshcraft CPU usage across all logical processors: 100% means the entire machine's CPU capacity. Sampled once per second.".into()))
            .child(cell("resource-vram",format!("{gpu_label:<9} {gpu_value}"),GPU_WIDTH,gpu_tip))
            .child(cell("resource-frame",frame_text,FRAME_WIDTH,"Last rendered viewport frame and its elapsed time. Sampling this counter does not request another GPU frame.".into()))
    }
}

/// Created and queried only on the existing raylib/OpenGL owner thread.
pub(super) struct GlMemoryProbe {
    get: unsafe extern "system" fn(u32, *mut i32),
}
impl GlMemoryProbe {
    pub fn new() -> Option<Self> {
        unsafe {
            let get_string = raylib::ffi::rlGetProcAddress(c"glGetString".as_ptr());
            if get_string.is_null() {
                return None;
            }
            let get_string: unsafe extern "system" fn(u32) -> *const std::ffi::c_char =
                std::mem::transmute(get_string);
            let renderer = get_string(0x1f01);
            if renderer.is_null() || software_renderer(&CStr::from_ptr(renderer).to_string_lossy())
            {
                return None;
            }
            let get = raylib::ffi::rlGetProcAddress(c"glGetIntegerv".as_ptr());
            let string = raylib::ffi::rlGetProcAddress(c"glGetStringi".as_ptr());
            if get.is_null() || string.is_null() {
                return None;
            }
            let get: unsafe extern "system" fn(u32, *mut i32) = std::mem::transmute(get);
            let string: unsafe extern "system" fn(u32, u32) -> *const std::ffi::c_char =
                std::mem::transmute(string);
            let mut count = 0;
            get(0x821d, &mut count);
            for i in 0..count.clamp(0, 4096) as u32 {
                let s = string(0x1f03, i);
                if !s.is_null() && CStr::from_ptr(s) == c"GL_NVX_gpu_memory_info" {
                    return Some(Self { get });
                }
            }
            None
        }
    }
    pub fn sample(&self) -> Option<GpuMemory> {
        unsafe {
            let mut total = 0;
            let mut free = 0;
            (self.get)(0x9047, &mut total);
            (self.get)(0x9049, &mut free);
            nvx_memory(total, free)
        }
    }
}
fn software_renderer(renderer: &str) -> bool {
    let renderer = renderer.to_ascii_lowercase();
    [
        "llvmpipe",
        "softpipe",
        "lavapipe",
        "swiftshader",
        "swrast",
        "software",
        "gdi generic",
        "basic render driver",
    ]
    .iter()
    .any(|name| renderer.contains(name))
}
fn nvx_memory(total: i32, free: i32) -> Option<GpuMemory> {
    if total <= 0 || free < 0 || free > total {
        return None;
    }
    Some(GpuMemory {
        memory: Memory {
            used: (total - free) as u64 * 1024,
            total: total as u64 * 1024,
        },
        scope: GpuScope::Device,
        source: "OpenGL NVX GPU-wide dedicated memory estimate; includes other processes",
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_driver_units_and_unknown_counters_are_handled() {
        let m = nvx_memory(8192, 2048).unwrap();
        assert_eq!(m.memory.used, 6144 * 1024);
        assert_eq!(m.memory.percent(), Some(75.));
        assert_eq!(m.scope, GpuScope::Device);
        assert!(nvx_memory(0, 0).is_none());
        assert!(nvx_memory(1, 2).is_none());
        assert!(software_renderer("llvmpipe (LLVM 20, 256 bits)"));
        assert!(software_renderer("zink Vulkan llvmpipe"));
        assert!(!software_renderer("NVIDIA GeForce RTX 4080"));
    }
    #[test]
    #[ignore = "requires private X11/OpenGL; checks telemetry leaves GL error state clean"]
    fn gpu_telemetry_supported_or_unavailable_without_gl_errors() {
        let (_rl, _thread) = raylib::init()
            .size(64, 64)
            .hidden()
            .title("Resource counters")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        if let Some(probe) = GlMemoryProbe::new() {
            if let Some(gpu) = probe.sample() {
                assert!(gpu.memory.total > 0);
                assert!(gpu.memory.percent().is_some());
            }
        }
        unsafe {
            let get: unsafe extern "system" fn() -> u32 =
                std::mem::transmute(raylib::ffi::rlGetProcAddress(c"glGetError".as_ptr()));
            assert_eq!(get(), 0);
        }
    }
}
