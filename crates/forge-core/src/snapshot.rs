//! A point-in-time description of the machine. Collectors fill it, rules read it.
use crate::model::StartupLocation;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Snapshot {
    pub os: String,
    pub is_laptop: bool,
    pub on_battery: bool,
    pub cpu_name: String,
    pub cpu_threads: u32,
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    /// DIMM speed as configured vs. as rated (MT/s), when the collector can read both.
    pub ram_configured_mts: Option<u32>,
    pub ram_rated_mts: Option<u32>,
    /// Number of populated memory slots, from SMBIOS.
    pub ram_modules: Option<u32>,
    pub gpus: Vec<String>,
    pub displays: Vec<Display>,
    pub disks: Vec<Disk>,
    pub startup: Vec<StartupItem>,
    pub appx: Vec<String>,
    pub active_power_plan: Option<PowerPlan>,
    /// Every power plan present on this PC (some are missing on Modern Standby laptops).
    pub power_plans: Vec<PowerPlan>,
    /// Game Bar "Record what happened" (background recording).
    pub game_dvr_background: Option<bool>,
    /// Game Mode switch; `None` when Windows uses its default (on).
    pub game_mode: Option<bool>,
    /// "Enhance pointer precision".
    pub mouse_acceleration: Option<bool>,
    /// Suggestion/promoted-app switches (from `ipc::SUGGESTION_VALUES`) that are explicitly on.
    pub windows_suggestions_on: Vec<String>,
    /// Bytes in the user's temp folder older than a day (what "Clean up" would remove).
    pub temp_bytes: Option<u64>,
    pub recycle_bin_bytes: Option<u64>,
    /// Storage Sense (automatic cleanup) switched on.
    pub storage_sense: Option<bool>,
    /// Windows 11 power mode on mains power (overlay GUID); None when the PC doesn't use power modes.
    pub power_mode: Option<String>,
    /// Hardware-accelerated GPU scheduling; None when Windows uses its default.
    pub hags: Option<bool>,
    /// The user's DirectX preferences string (holds "Optimizations for windowed games").
    pub dx_user_prefs: Option<String>,
    /// Windows Update is waiting for a restart.
    pub reboot_pending: bool,
    pub uptime_hours: u64,
    pub gpu_drivers: Vec<GpuDriver>,
    /// Date of the scan, YYYY-MM-DD (rules stay pure; the collector supplies "today").
    pub scan_date: String,
    /// Biggest memory users right now, by app.
    pub top_memory: Vec<ProcessUse>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Disk {
    pub mount: String,
    pub is_system: bool,
    pub kind: DiskKind,
    /// Sizes in GB as Windows Explorer shows them (1 GB = 1024^3 bytes).
    pub total_gb: u64,
    pub free_gb: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiskKind {
    Hdd,
    Ssd,
    Nvme,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StartupItem {
    pub name: String,
    pub command: String,
    pub location: StartupLocation,
    pub enabled: bool,
    /// A Windows, security or driver component (bloat DB `keep`). Shown, never switched off.
    #[serde(default)]
    pub protected: bool,
    /// What the bloat DB says about it, if it knows the item.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GpuDriver {
    pub name: String,
    pub version: String,
    /// Driver date as YYYY-MM-DD, when Windows records one.
    pub date: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProcessUse {
    pub name: String,
    pub memory_mb: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Display {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub current_hz: u32,
    /// Highest refresh rate the display offers at its current resolution.
    pub max_hz: u32,
}

impl Snapshot {
    /// Deterministic sample used by tests and by the UI's dev mode.
    pub fn sample_gaming_pc() -> Self {
        use StartupLocation::*;
        let item = |n: &str, c: &str, l| StartupItem {
            name: n.into(),
            command: c.into(),
            location: l,
            enabled: true,
            protected: false,
            note: None,
        };
        let plan = |guid: &str, name: &str| PowerPlan {
            guid: guid.into(),
            name: name.into(),
        };
        Snapshot {
            os: "Windows 11 25H2".into(),
            cpu_name: "AMD Ryzen 7 7800X3D".into(),
            cpu_threads: 16,
            ram_total_mb: 32768,
            ram_used_mb: 9800,
            ram_configured_mts: Some(4800),
            ram_rated_mts: Some(6000),
            ram_modules: Some(2),
            gpus: vec!["NVIDIA GeForce RTX 4070".into()],
            displays: vec![Display {
                name: "Display 1".into(),
                width: 2560,
                height: 1440,
                current_hz: 60,
                max_hz: 165,
            }],
            disks: vec![
                Disk {
                    mount: "C:".into(),
                    is_system: true,
                    kind: DiskKind::Nvme,
                    total_gb: 931,
                    free_gb: 84,
                },
                Disk {
                    mount: "D:".into(),
                    is_system: false,
                    kind: DiskKind::Hdd,
                    total_gb: 1863,
                    free_gb: 838,
                },
            ],
            startup: vec![
                item(
                    "Discord",
                    "C:\\Users\\u\\AppData\\Local\\Discord\\Update.exe --processStart Discord.exe",
                    HkcuRun,
                ),
                item(
                    "Spotify",
                    "C:\\Users\\u\\AppData\\Roaming\\Spotify\\Spotify.exe /background",
                    HkcuRun,
                ),
                item(
                    "AdobeGCInvoker-1.0",
                    "C:\\Program Files (x86)\\Common Files\\Adobe\\AdobeGCClient\\AGCInvokerUtility.exe",
                    HklmRun32,
                ),
                item(
                    "SecurityHealth",
                    "%windir%\\system32\\SecurityHealthSystray.exe",
                    HklmRun,
                ),
                item(
                    "RtkAudUService",
                    "C:\\Windows\\System32\\DriverStore\\RtkAudUService64.exe",
                    HklmRun,
                ),
            ],
            appx: vec![
                "king.com.CandyCrushSaga".into(),
                "Microsoft.BingNews".into(),
                "Microsoft.XboxIdentityProvider".into(),
            ],
            active_power_plan: Some(plan(crate::power::BALANCED, "Balanced")),
            power_plans: vec![
                plan(crate::power::BALANCED, "Balanced"),
                plan(crate::power::HIGH_PERFORMANCE, "High performance"),
                plan(crate::power::POWER_SAVER, "Power saver"),
            ],
            game_dvr_background: Some(true),
            game_mode: Some(true),
            temp_bytes: Some(2_300_000_000),
            recycle_bin_bytes: Some(640_000_000),
            storage_sense: Some(false),
            power_mode: Some(crate::power::MODE_BALANCED.into()),
            hags: Some(false),
            dx_user_prefs: Some("VRROptimizeEnable=0;".into()),
            reboot_pending: true,
            uptime_hours: 9 * 24,
            gpu_drivers: vec![GpuDriver {
                name: "NVIDIA GeForce RTX 4070".into(),
                version: "32.0.15.5612".into(),
                date: Some("2026-01-14".into()),
            }],
            scan_date: "2026-10-23".into(),
            top_memory: vec![
                ProcessUse {
                    name: "chrome".into(),
                    memory_mb: 3100,
                },
                ProcessUse {
                    name: "Discord".into(),
                    memory_mb: 640,
                },
                ProcessUse {
                    name: "Steam".into(),
                    memory_mb: 410,
                },
            ],
            mouse_acceleration: Some(true),
            windows_suggestions_on: vec![
                "SilentInstalledAppsEnabled".into(),
                "SubscribedContent-338388Enabled".into(),
            ],
            ..Default::default()
        }
    }
}
