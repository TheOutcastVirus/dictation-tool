//! GPU memory accounting for the loaded model.
//!
//! The interesting number is not how full the card is, it is how much of the
//! card *this process* is holding: the whisper weights plus its compute
//! buffers. amdgpu exposes that per DRM client in `/proc/self/fdinfo`, which
//! is the only place it can be read without rocm-smi.
//!
//! amdgpu keeps two pools and which one the weights land in depends on the
//! part. A discrete card puts them in dedicated VRAM and leaves GTT holding
//! little more than staging buffers; an APU carves a small VRAM window out of
//! system RAM and lands the bulk in GTT instead -- on gfx1152 a 1.5 GB model
//! reports ~149 MiB of VRAM against ~1.6 GiB of GTT, so reading VRAM alone
//! under-reports it by an order of magnitude.
//!
//! Both pools are therefore summed for the process figure, and the capacity
//! shown alongside it is whichever pool the bulk actually landed in. That is
//! read off the allocation rather than off the hardware on purpose: PCI
//! device class does not separate the two cases (a discrete GPU in a hybrid
//! laptop reports 0x0380 exactly like an iGPU), whereas where the bytes went
//! is the thing being measured and needs no per-model knowledge.

use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct VramStats {
    /// GPU memory held by this process on the primary card.
    pub model_mib: u64,
    /// Capacity of the pool holding it.
    pub total_mib: u64,
}

/// A quantity split across amdgpu's two pools, in whatever unit the caller
/// read: bytes for the sysfs capacities, KiB for the fdinfo residencies.
#[derive(Clone, Copy, Default)]
struct Pools {
    vram: u64,
    gtt: u64,
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The AMD card with the largest VRAM pool, as `(pci_slot, capacities)`.
/// Vendor filtering alone can't separate a discrete card from an iGPU, and
/// the model always lands on the big one.
fn primary_amd_card() -> Option<(String, Pools)> {
    let mut best: Option<(String, Pools)> = None;

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
        let Some(vram) = read_u64(&device.join("mem_info_vram_total")) else {
            continue;
        };
        // GTT is absent on some older parts; treat it as empty rather than
        // skipping a card that reports a perfectly good VRAM figure.
        let gtt = read_u64(&device.join("mem_info_gtt_total")).unwrap_or(0);
        // `device` is a symlink into /sys/devices/...; its final component is
        // the PCI slot, which is what fdinfo reports as drm-pdev.
        let Some(slot) = fs::canonicalize(&device)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            continue;
        };
        if best.as_ref().map(|(_, p)| vram > p.vram).unwrap_or(true) {
            best = Some((slot, Pools { vram, gtt }));
        }
    }

    best
}

/// This process's resident memory on `slot`, in KiB, split by pool.
///
/// Every DRM file descriptor gets its own fdinfo, and a dup'd fd repeats the
/// same client, so entries are counted once per `drm-client-id`.
fn process_memory_kib(slot: &str) -> Pools {
    let Ok(entries) = fs::read_dir("/proc/self/fdinfo") else {
        return Pools::default();
    };
    let mut seen: Vec<u64> = Vec::new();
    let mut total = Pools::default();

    for entry in entries.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let mut pdev = None;
        let mut client = None;
        let mut held = Pools::default();
        let mut is_amdgpu = false;
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            // "1758364 KiB"
            let kib = || value.split_whitespace().next().and_then(|n| n.parse::<u64>().ok());
            match key {
                "drm-pdev" => pdev = Some(value.to_string()),
                "drm-client-id" => client = value.parse::<u64>().ok(),
                "drm-memory-vram" => {
                    held.vram = kib().unwrap_or(0);
                    is_amdgpu = true;
                }
                "drm-memory-gtt" => held.gtt = kib().unwrap_or(0),
                _ => {}
            }
        }
        let (Some(pdev), Some(client)) = (pdev, client) else {
            continue;
        };
        if !is_amdgpu || pdev != slot || seen.contains(&client) {
            continue;
        }
        seen.push(client);
        total.vram += held.vram;
        total.gtt += held.gtt;
    }

    total
}

pub fn read_model_vram() -> Option<VramStats> {
    let (slot, capacity) = primary_amd_card()?;
    let held = process_memory_kib(&slot);
    // Whichever pool the weights went to is the one worth showing a capacity
    // for. Before the model loads there is nothing to weigh, and the tie
    // falls to VRAM -- the right answer on a discrete card and a harmless
    // placeholder on an APU, where the first load moves it to GTT.
    let total = if held.gtt > held.vram {
        capacity.gtt
    } else {
        capacity.vram
    };
    Some(VramStats {
        model_mib: (held.vram + held.gtt) / 1024,
        total_mib: total / 1024 / 1024,
    })
}
