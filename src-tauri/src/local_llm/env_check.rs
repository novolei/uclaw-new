// SPDX-License-Identifier: Apache-2.0
//! Hardware environment check for the local-model onboarding wizard:
//! OS/arch/RAM/disk/Metal/cpu-cores → a recommended quant + warnings.

use serde::Serialize;

/// Recommended quant string, chosen by available RAM. Pure + unit-tested.
pub fn recommended_quant(total_ram_bytes: u64) -> &'static str {
    const GB: u64 = 1_000_000_000;
    if total_ram_bytes >= 32 * GB {
        "Q8_0"
    } else {
        "Q4_K_M"
    }
}

/// Per-resource hardware report surfaced to the wizard's env-check step.
#[derive(Debug, Clone, Serialize)]
pub struct EnvReport {
    pub os: String,
    pub arch: String,
    pub total_ram: u64,
    pub free_disk: u64,
    pub metal_available: bool,
    pub cpu_cores: usize,
    pub recommended_quant: String,
    pub warnings: Vec<String>,
}

/// True if a candle Metal device initialises (macOS GPU acceleration).
pub fn metal_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        candle_core::Device::new_metal(0).is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Build warnings from the gathered numbers (pure — unit-tested).
pub fn build_warnings(total_ram: u64, free_disk: u64) -> Vec<String> {
    const GB: u64 = 1_000_000_000;
    let mut w = Vec::new();
    if free_disk < 1_200_000_000 {
        w.push(format!("磁盘空间不足：剩余 {} MB，建议至少 1200 MB", free_disk / 1_000_000));
    }
    if total_ram < 8 * GB {
        w.push(format!("内存较小：{} GB，本地模型可能与其他应用争用内存", total_ram / GB));
    }
    w
}

/// Collect the full report (sysinfo + Metal probe + Slice C's free_disk_bytes).
/// sysinfo 0.31 `total_memory()` returns **bytes** (confirmed from source docs).
pub fn collect_env_report(data_dir: &std::path::Path) -> EnvReport {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram = sys.total_memory();
    let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let free_disk = crate::local_llm::model_manager::free_disk_bytes(data_dir).unwrap_or(0);
    EnvReport {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        total_ram,
        free_disk,
        metal_available: metal_available(),
        cpu_cores,
        recommended_quant: recommended_quant(total_ram).to_string(),
        warnings: build_warnings(total_ram, free_disk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_q4_for_small_ram() {
        assert_eq!(recommended_quant(8_000_000_000), "Q4_K_M");
        assert_eq!(recommended_quant(16_000_000_000), "Q4_K_M");
    }
    #[test]
    fn recommends_q8_for_large_ram() {
        assert_eq!(recommended_quant(64_000_000_000), "Q8_0");
    }
    #[test]
    fn warns_on_low_disk() {
        assert!(build_warnings(16_000_000_000, 500_000_000).iter().any(|s| s.contains("磁盘")));
    }
    #[test]
    fn warns_on_low_ram() {
        assert!(build_warnings(4_000_000_000, 50_000_000_000).iter().any(|s| s.contains("内存")));
    }
    #[test]
    fn no_warnings_on_healthy_box() {
        assert!(build_warnings(16_000_000_000, 50_000_000_000).is_empty());
    }
}
