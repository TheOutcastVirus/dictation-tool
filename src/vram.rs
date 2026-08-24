use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Default)]
pub struct VramStats {
    pub used_mib: u64,
    pub total_mib: u64,
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Picks the AMD GPU with the largest total VRAM pool (the discrete GPU, on
/// systems with both an iGPU and a discrete card — vendor filtering alone
/// isn't enough to tell them apart).
pub fn read_primary_amd_vram() -> Option<VramStats> {
    let drm = Path::new("/sys/class/drm");
    let mut best: Option<VramStats> = None;

    for entry in fs::read_dir(drm).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        let vendor = fs::read_to_string(device.join("vendor")).unwrap_or_default();
        if vendor.trim() != "0x1002" {
            continue;
        }
        let Some(used) = read_u64(&device.join("mem_info_vram_used")) else {
            continue;
        };
        let Some(total) = read_u64(&device.join("mem_info_vram_total")) else {
            continue;
        };
        let stats = VramStats {
            used_mib: used / 1024 / 1024,
            total_mib: total / 1024 / 1024,
        };
        if best.map(|b| stats.total_mib > b.total_mib).unwrap_or(true) {
            best = Some(stats);
        }
    }

    best
}
