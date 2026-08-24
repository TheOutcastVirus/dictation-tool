//! VRAM accounting for the loaded model.
//!
//! The interesting number is not how full the card is, it is how much of the
//! card *this process* is holding: the whisper weights plus its compute
//! buffers, resident in GPU memory. amdgpu exposes that per DRM client in
//! `/proc/self/fdinfo`, which is the only place it can be read without
//! rocm-smi.

use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct VramStats {
    /// VRAM held by this process on the primary card.
    pub model_mib: u64,
    /// Capacity of that card.
    pub total_mib: u64,
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The AMD card with the largest VRAM pool, as `(pci_slot, total_mib)`.
/// Vendor filtering alone can't separate a discrete card from an iGPU, and
/// the model always lands on the big one.
fn primary_amd_card() -> Option<(String, u64)> {
    let mut best: Option<(String, u64)> = None;

    for entry in fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        if fs::read_to_string(device.join("vendor"))
            .unwrap_or_default()
            .trim()
            != "0x1002"
        {
            continue;
        }
        let Some(total) = read_u64(&device.join("mem_info_vram_total")) else {
            continue;
        };
        // `device` is a symlink into /sys/devices/...; its final component is
        // the PCI slot, which is what fdinfo reports as drm-pdev.
        let Some(slot) = fs::canonicalize(&device)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            continue;
        };
        let total_mib = total / 1024 / 1024;
        if best.as_ref().map(|(_, t)| total_mib > *t).unwrap_or(true) {
            best = Some((slot, total_mib));
        }
    }

    best
}

/// Sums this process's resident VRAM on `slot`, in KiB.
///
/// Every DRM file descriptor gets its own fdinfo, and a dup'd fd repeats the
/// same client, so entries are counted once per `drm-client-id`.
fn process_vram_kib(slot: &str) -> u64 {
    let Ok(entries) = fs::read_dir("/proc/self/fdinfo") else {
        return 0;
    };
    let mut seen: Vec<u64> = Vec::new();
    let mut total = 0;

    for entry in entries.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let mut pdev = None;
        let mut client = None;
        let mut vram = None;
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key {
                "drm-pdev" => pdev = Some(value.to_string()),
                "drm-client-id" => client = value.parse::<u64>().ok(),
                // "1758364 KiB"
                "drm-memory-vram" => {
                    vram = value.split_whitespace().next().and_then(|n| n.parse::<u64>().ok())
                }
                _ => {}
            }
        }
        let (Some(pdev), Some(client), Some(vram)) = (pdev, client, vram) else {
            continue;
        };
        if pdev != slot || seen.contains(&client) {
            continue;
        }
        seen.push(client);
        total += vram;
    }

    total
}

pub fn read_model_vram() -> Option<VramStats> {
    let (slot, total_mib) = primary_amd_card()?;
    Some(VramStats {
        model_mib: process_vram_kib(&slot) / 1024,
        total_mib,
    })
}
