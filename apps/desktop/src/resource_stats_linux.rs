//! DRM fdinfo accounting, de-duplicated by device/client; sysfs device fallback.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn bytes(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let count: u64 = parts.next()?.parse().ok()?;
    let factor = match parts.next() {
        None | Some("B") => 1,
        Some("KiB" | "kB") => 1024,
        Some("MiB") => 1024 * 1024,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    count.checked_mul(factor)
}
fn clients(contents: impl IntoIterator<Item = String>) -> BTreeMap<String, (u64, bool)> {
    let mut seen = BTreeSet::new();
    let mut devices = BTreeMap::new();
    for text in contents {
        let fields = text
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k, v.trim()))
            .collect::<BTreeMap<_, _>>();
        let Some(device) = fields.get("drm-pdev") else {
            continue;
        };
        let Some(client) = fields.get("drm-client-id") else {
            devices.entry(device.to_string()).or_insert((0u64, true)).1 = false;
            continue;
        };
        if !seen.insert((device.to_string(), client.to_string())) {
            continue;
        }
        let used = fields
            .get("drm-resident-vram")
            .or_else(|| fields.get("drm-memory-vram"))
            .and_then(|v| bytes(v));
        let entry = devices.entry(device.to_string()).or_insert((0u64, true));
        if let Some(used) = used {
            if let Some(sum) = entry.0.checked_add(used) {
                entry.0 = sum;
            } else {
                entry.1 = false;
            }
        } else {
            entry.1 = false;
        }
    }
    devices
}
pub(super) fn sample() -> Option<GpuMemory> {
    let devices = clients(
        fs::read_dir("/proc/self/fdinfo")
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|e| fs::read_to_string(e.path()).ok()),
    );
    let mut process_used = 0u64;
    let mut device_used = 0u64;
    let mut total = 0u64;
    let mut complete = true;
    let mut any = false;
    let mut fallback = true;
    for (device, (used, known)) in devices {
        // drm-pdev is a kernel-exported PCI identity, never guess card0 on hybrid laptops.
        if device.contains('/') || device.contains("..") {
            continue;
        }
        let path = Path::new("/sys/bus/pci/devices").join(device);
        let Some(capacity) = fs::read_to_string(path.join("mem_info_vram_total"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
        else {
            continue;
        };
        total = total.checked_add(capacity)?;
        process_used = process_used.checked_add(used)?;
        complete &= known;
        any = true;
        if let Some(used) = fs::read_to_string(path.join("mem_info_vram_used"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            device_used = device_used.checked_add(used)?;
        } else {
            fallback = false;
        }
    }
    if !any {
        return None;
    }
    if complete {
        Some(GpuMemory {
            memory: Memory {
                used: process_used,
                total,
            },
            scope: GpuScope::Process,
            source: "DRM resident VRAM for Meshcraft clients / dedicated VRAM of active adapters",
        })
    } else if fallback {
        Some(GpuMemory {
            memory: Memory {
                used: device_used,
                total,
            },
            scope: GpuScope::Device,
            source: "GPU-wide sysfs VRAM usage for adapters opened by Meshcraft; includes other processes",
        })
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fdinfo_deduplicates_clients_and_prefers_resident_bytes() {
        let a = "drm-pdev: 0000:01:00.0\ndrm-client-id: 7\ndrm-resident-vram: 2 MiB\ndrm-memory-vram: 1 KiB\n";
        let b = "drm-pdev: 0000:01:00.0\ndrm-client-id: 8\ndrm-memory-vram: 3 KiB\n";
        let data = clients([a, a, b].map(str::to_owned));
        assert_eq!(data["0000:01:00.0"], (2 * 1024 * 1024 + 3 * 1024, true));
        assert_eq!(bytes("18446744073709551615 MiB"), None);
    }
    #[test]
    fn missing_or_invalid_vram_is_unknown_not_zero() {
        let data = clients([
            "drm-pdev: 0000:01:00.0\ndrm-client-id: 1\ndrm-resident-memory: 1024 KiB\n".into(),
        ]);
        assert_eq!(data["0000:01:00.0"], (0, false));
        assert_eq!(bytes("12 invalid"), None);
    }
}
