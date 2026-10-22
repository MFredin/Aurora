//! Collectors. `collect()` gathers a Snapshot using cheap OS reads only.
pub mod parse;
pub mod smbios;
#[cfg(windows)]
mod win;

use forge_core::snapshot::{Disk, DiskKind, Snapshot};
use serde::Serialize;
use sysinfo::{Disks, ProcessesToUpdate, System};

/// Runs the current thread at background priority until dropped. No-op off Windows.
#[cfg(windows)]
pub use win::BackgroundGuard;
#[cfg(not(windows))]
pub struct BackgroundGuard;
#[cfg(not(windows))]
impl BackgroundGuard {
    pub fn enter() -> Self {
        BackgroundGuard
    }
}

pub fn collect() -> Snapshot {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    let system_drive = if cfg!(windows) {
        std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
    } else {
        "/".into()
    };
    let disks = Disks::new_with_refreshed_list()
        .iter()
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().to_string();
            Disk {
                is_system: mount
                    .to_lowercase()
                    .starts_with(&system_drive.to_lowercase()),
                mount,
                kind: match d.kind() {
                    sysinfo::DiskKind::HDD => DiskKind::Hdd,
                    sysinfo::DiskKind::SSD => DiskKind::Ssd,
                    _ => DiskKind::Unknown,
                },
                total_gb: d.total_space() / 1_000_000_000,
                free_gb: d.available_space() / 1_000_000_000,
            }
        })
        .collect();

    #[allow(unused_mut)]
    let mut snap = Snapshot {
        os: System::long_os_version().unwrap_or_default(),
        cpu_name: sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default(),
        cpu_threads: sys.cpus().len() as u32,
        ram_total_mb: sys.total_memory() / 1_048_576,
        ram_used_mb: sys.used_memory() / 1_048_576,
        disks,
        ..Default::default()
    };
    #[cfg(windows)]
    win::fill(&mut snap);
    snap
}

/// Resource use of Forge itself. Shown in the UI so the app is accountable for its own cost.
#[derive(Serialize, Clone, Copy, Debug)]
pub struct SelfStats {
    pub rss_mb: f32,
    pub cpu_pct: f32,
}

pub struct SelfMeter {
    sys: System,
    pid: sysinfo::Pid,
}
impl SelfMeter {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            pid: sysinfo::get_current_pid().expect("pid"),
        }
    }
    /// CPU % is measured between consecutive calls, so poll at a low rate (e.g. every 2 s while visible).
    pub fn sample(&mut self) -> SelfStats {
        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
        self.sys
            .process(self.pid)
            .map(|p| SelfStats {
                rss_mb: p.memory() as f32 / 1_048_576.0,
                cpu_pct: p.cpu_usage(),
            })
            .unwrap_or(SelfStats {
                rss_mb: 0.0,
                cpu_pct: 0.0,
            })
    }
}
impl Default for SelfMeter {
    fn default() -> Self {
        Self::new()
    }
}
