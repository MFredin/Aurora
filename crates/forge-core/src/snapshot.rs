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
    /// DIMM speed as configured vs. as rated, when the collector can read both.
    pub ram_configured_mhz: Option<u32>,
    pub ram_rated_mhz: Option<u32>,
    pub gpus: Vec<String>,
    pub disks: Vec<Disk>,
    pub startup: Vec<StartupItem>,
    pub appx: Vec<String>,
    pub active_power_plan: Option<PowerPlan>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Disk {
    pub mount: String,
    pub is_system: bool,
    pub kind: DiskKind,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
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
        Snapshot {
            os: "Windows 11 25H2".into(),
            cpu_name: "AMD Ryzen 7 7800X3D".into(),
            cpu_threads: 16,
            ram_total_mb: 32768,
            ram_used_mb: 9800,
            ram_configured_mhz: Some(4800),
            ram_rated_mhz: Some(6000),
            gpus: vec!["NVIDIA GeForce RTX 4070".into()],
            disks: vec![
                Disk {
                    mount: "C:".into(),
                    is_system: true,
                    kind: DiskKind::Nvme,
                    total_gb: 1000,
                    free_gb: 90,
                },
                Disk {
                    mount: "D:".into(),
                    is_system: false,
                    kind: DiskKind::Hdd,
                    total_gb: 2000,
                    free_gb: 900,
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
                    HklmRun,
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
            active_power_plan: Some(PowerPlan {
                guid: crate::power::BALANCED.into(),
                name: "Balanced".into(),
            }),
            ..Default::default()
        }
    }
}
