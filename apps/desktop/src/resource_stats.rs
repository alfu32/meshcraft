//! Low-frequency process telemetry. No scene traversal, GPU readback or subprocesses.
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System};

pub(crate) const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Memory {
    pub used: u64,
    pub total: u64,
}
impl Memory {
    pub fn percent(self) -> Option<f64> {
        (self.total > 0).then(|| self.used as f64 / self.total as f64 * 100.)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GpuScope {
    Process,
    Device,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuMemory {
    pub memory: Memory,
    pub scope: GpuScope,
    pub source: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    pub ram: Option<Memory>,
    pub cpu: Option<f64>,
    pub gpu: Option<GpuMemory>,
}

pub(crate) struct Monitor {
    snapshot: Arc<Mutex<Snapshot>>,
    // Dropping the sender interrupts the worker's wait and releases its sysinfo state.
    _stop: mpsc::Sender<()>,
}
impl Monitor {
    pub fn start() -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = Arc::clone(&snapshot);
        let (stop, wait) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("meshcraft-resource-stats".into())
            .spawn(move || {
                let mut system = System::new();
                system.refresh_memory_specifics(MemoryRefreshKind::new().with_ram());
                system.refresh_cpu_list(CpuRefreshKind::new());
                let Ok(pid) = sysinfo::get_current_pid() else {
                    return;
                };
                let mut warmed = false;
                loop {
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        ProcessRefreshKind::new().with_cpu().with_memory(),
                    );
                    let value = system
                        .process(pid)
                        .map(|process| Snapshot {
                            ram: Some(Memory {
                                used: process.memory(),
                                total: system.total_memory(),
                            }),
                            cpu: warmed.then(|| {
                                normalized_cpu(process.cpu_usage() as f64, system.cpus().len())
                            }),
                            gpu: platform_gpu(),
                        })
                        .unwrap_or_default();
                    if let Ok(mut target) = shared.lock() {
                        *target = value;
                    }
                    warmed = true;
                    if !matches!(
                        wait.recv_timeout(SAMPLE_INTERVAL),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        break;
                    }
                }
            });
        Self {
            snapshot,
            _stop: stop,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }
}
fn normalized_cpu(value: f64, processors: usize) -> f64 {
    if !value.is_finite() || processors == 0 {
        return 0.;
    }
    (value / processors as f64).clamp(0., 100.)
}
pub(crate) fn memory_text(value: Option<Memory>) -> String {
    value.map_or_else(
        || "N/A".into(),
        |m| match m.percent() {
            Some(p) => format!("{:>20} B ({p:>5.1}%)", m.used),
            None => format!("{} B (N/A)", m.used),
        },
    )
}
pub(crate) fn gpu_choice(
    process: Option<GpuMemory>,
    context: Option<GpuMemory>,
) -> Option<GpuMemory> {
    if process
        .as_ref()
        .is_some_and(|g| g.scope == GpuScope::Process)
    {
        process
    } else {
        context.or(process)
    }
}

#[cfg(target_os = "linux")]
#[path = "resource_stats_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
fn platform_gpu() -> Option<GpuMemory> {
    linux::sample()
}
#[cfg(target_os = "windows")]
fn platform_gpu() -> Option<GpuMemory> {
    use windows::{Win32::Graphics::Dxgi::*, core::Interface};
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut used = 0u64;
        let mut total = 0u64;
        let mut reported = false;
        for index in 0..32 {
            let Ok(adapter) = factory.EnumAdapters1(index) else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            // UMA/shared RAM is not dedicated VRAM.
            if desc.DedicatedVideoMemory == 0
                || desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0
            {
                continue;
            }
            let Ok(adapter) = adapter.cast::<IDXGIAdapter3>() else {
                continue;
            };
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            if adapter
                .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                .is_ok()
                && info.CurrentUsage > 0
            {
                used = used.checked_add(info.CurrentUsage)?;
                total = total.checked_add(desc.DedicatedVideoMemory as u64)?;
                reported = true;
            }
        }
        reported.then_some(GpuMemory {
            memory: Memory { used, total },
            scope: GpuScope::Process,
            source: "DXGI process local-memory usage / physical dedicated VRAM (active adapters)",
        })
    }
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn platform_gpu() -> Option<GpuMemory> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentages_and_exact_bytes_have_explicit_denominators() {
        assert_eq!(
            memory_text(Some(Memory {
                used: 1536,
                total: 4096
            })),
            "                1536 B ( 37.5%)"
        );
        assert_eq!(memory_text(Some(Memory { used: 0, total: 0 })), "0 B (N/A)");
        assert_eq!(normalized_cpu(400., 8), 50.);
        assert_eq!(normalized_cpu(f64::NAN, 8), 0.);
        assert_eq!(memory_text(None), "N/A");
        assert_eq!(
            memory_text(Some(Memory {
                used: 1,
                total: 100
            }))
            .len(),
            memory_text(Some(Memory {
                used: 10_000_000_000,
                total: 10_000_000_000
            }))
            .len()
        );
    }
    #[test]
    fn process_gpu_wins_and_device_fallback_keeps_its_scope() {
        let device = GpuMemory {
            memory: Memory { used: 3, total: 8 },
            scope: GpuScope::Device,
            source: "GL",
        };
        let process = GpuMemory {
            memory: Memory { used: 1, total: 8 },
            scope: GpuScope::Process,
            source: "DRM",
        };
        assert_eq!(
            gpu_choice(Some(process.clone()), Some(device.clone())),
            Some(process)
        );
        assert_eq!(gpu_choice(None, Some(device.clone())), Some(device));
        assert_eq!(gpu_choice(None, None), None);
    }
    #[test]
    fn process_monitor_reports_real_resident_bytes_without_waiting_on_ui() {
        let monitor = Monitor::start();
        for _ in 0..100 {
            if let Some(ram) = monitor.snapshot().ram {
                assert!(ram.used > 0);
                assert!(ram.total > 0);
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no process memory sample");
    }
}
