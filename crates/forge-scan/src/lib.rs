//! Collectors. `collect()` gathers a Snapshot using cheap OS reads only.
pub mod cleanup;
pub mod parse;
pub mod smbios;
#[cfg(windows)]
mod win;

use forge_core::snapshot::{Disk, DiskKind, Snapshot};
use serde::Serialize;
use std::collections::HashSet;
use sysinfo::{Disks, Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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

#[cfg(windows)]
pub use win::{empty_recycle_bin, read_active_plan, read_power_mode};

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
            let raw = d.mount_point().to_string_lossy().to_string();
            let mount = parse::mount_label(&raw);
            Disk {
                is_system: mount.eq_ignore_ascii_case(&parse::mount_label(&system_drive)),
                kind: match d.kind() {
                    sysinfo::DiskKind::HDD => DiskKind::Hdd,
                    sysinfo::DiskKind::SSD => DiskKind::Ssd,
                    _ => DiskKind::Unknown,
                },
                mount,
                total_gb: parse::gib(d.total_space()),
                free_gb: parse::gib(d.available_space()),
            }
        })
        .collect();

    #[allow(unused_mut)]
    let mut snap = Snapshot {
        os: System::long_os_version().unwrap_or_default(),
        cpu_name: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        cpu_threads: sys.cpus().len() as u32,
        ram_total_mb: sys.total_memory() / 1_048_576,
        ram_used_mb: sys.used_memory() / 1_048_576,
        disks,
        uptime_hours: System::uptime() / 3600,
        scan_date: parse::civil_date(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        ),
        top_memory: top_memory(5),
        temp_bytes: Some(cleanup::measure(&cleanup::temp_dir(), cleanup::MIN_AGE).bytes),
        ..Default::default()
    };
    #[cfg(windows)]
    win::fill(&mut snap);
    snap
}

/// The apps using the most memory right now, summed over their processes (browsers run many).
fn top_memory(n: usize) -> Vec<forge_core::snapshot::ProcessUse> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_exe(UpdateKind::Never),
    );
    let mut by_name: std::collections::HashMap<String, u64> = Default::default();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy();
        let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
        *by_name.entry(name).or_default() += p.memory();
    }
    let mut v: Vec<_> = by_name
        .into_iter()
        .filter(|(n, _)| {
            !matches!(
                n.as_str(),
                "System" | "Registry" | "Memory Compression" | "MemCompression"
            )
        })
        .map(|(name, bytes)| forge_core::snapshot::ProcessUse {
            name,
            memory_mb: bytes / 1_048_576,
        })
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.memory_mb));
    v.truncate(n);
    v
}

/// Resource use of Forge itself, including its child processes (the WebView2 renderer and
/// GPU processes, and the helper when it runs). Shown in the UI so the app is accountable.
#[derive(Serialize, Clone, Copy, Debug)]
pub struct SelfStats {
    /// Sum of private working sets, MB: the "Memory" column in Task Manager. (Working set would
    /// also count pages shared with every other process and overstate Forge's cost.)
    pub memory_mb: f32,
    /// Share of total CPU capacity, 0-100.
    pub cpu_pct: f32,
    pub processes: u32,
}

/// How often (in samples) to re-list every process to find new children. In between only the
/// known tree is refreshed, which keeps the meter itself cheap.
const RESCAN_EVERY: u32 = 10;

pub struct SelfMeter {
    sys: System,
    pid: Option<Pid>,
    cores: f32,
    tree: Vec<Pid>,
    samples: u32,
}
impl SelfMeter {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            pid: sysinfo::get_current_pid().ok(),
            cores: std::thread::available_parallelism().map_or(1, |n| n.get()) as f32,
            tree: vec![],
            samples: 0,
        }
    }
    /// CPU % is measured between consecutive calls, so poll at a low rate (every 2 s while visible).
    pub fn sample(&mut self) -> SelfStats {
        let Some(root) = self.pid else {
            return SelfStats {
                memory_mb: 0.0,
                cpu_pct: 0.0,
                processes: 0,
            };
        };
        let kind = ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_exe(UpdateKind::Never);
        if self.samples % RESCAN_EVERY == 0 || self.tree.is_empty() {
            self.sys
                .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
            self.tree = descendants(
                root,
                self.sys.processes().iter().map(|(p, pr)| (*p, pr.parent())),
            )
            .into_iter()
            .collect();
        } else {
            self.sys
                .refresh_processes_specifics(ProcessesToUpdate::Some(&self.tree), true, kind);
        }
        self.samples = self.samples.wrapping_add(1);
        let (mut mem, mut cpu, mut alive) = (0u64, 0f32, 0u32);
        for pid in &self.tree {
            if let Some(p) = self.sys.process(*pid) {
                mem += private_memory(*pid).unwrap_or_else(|| p.memory());
                cpu += p.cpu_usage();
                alive += 1;
            }
        }
        SelfStats {
            memory_mb: mem as f32 / 1_048_576.0,
            cpu_pct: cpu / self.cores,
            processes: alive,
        }
    }
}

#[cfg(windows)]
fn private_memory(pid: Pid) -> Option<u64> {
    win::private_working_set(pid.as_u32())
}
#[cfg(not(windows))]
fn private_memory(_: Pid) -> Option<u64> {
    None
}

impl Default for SelfMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// `root` plus every process whose parent chain leads to it.
pub fn descendants(root: Pid, procs: impl Iterator<Item = (Pid, Option<Pid>)>) -> HashSet<Pid> {
    let procs: Vec<(Pid, Option<Pid>)> = procs.collect();
    let mut set = HashSet::from([root]);
    loop {
        let before = set.len();
        for (pid, parent) in &procs {
            if parent.is_some_and(|p| set.contains(&p)) {
                set.insert(*pid);
            }
        }
        if set.len() == before {
            return set;
        }
    }
}
