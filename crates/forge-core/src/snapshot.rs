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
            mouse_acceleration: Some(true),
            windows_suggestions_on: vec![
                "SilentInstalledAppsEnabled".into(),
                "SubscribedContent-338388Enabled".into(),
            ],
            ..Default::default()
        }
    }
}
