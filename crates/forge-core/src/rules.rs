//! Rules turn a Snapshot into findings. Pure functions: easy to test, no I/O.
use crate::bloat::{BloatDb, Kind, Verdict};
use crate::ipc::{
    CONTENT_DELIVERY, DIRECTX_PREFS, DIRECTX_PREFS_VALUE, GAME_BAR, GAME_DVR, GRAPHICS_DRIVERS,
    MOUSE, MOUSE_VALUES, STORAGE_SENSE, dx_pref, with_dx_pref,
};
use crate::model::*;
use crate::power;
use crate::snapshot::{DiskKind, Snapshot};

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(&self, s: &Snapshot, db: &BloatDb) -> Vec<Finding>;
}

pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(StartupRule),
        Box::new(AppxRule),
        Box::new(PowerPlanRule),
        Box::new(MemorySpeedRule),
        Box::new(SingleChannelRule),
        Box::new(StorageRule),
        Box::new(RefreshRateRule),
        Box::new(GameDvrRule),
        Box::new(GameModeRule),
        Box::new(MouseAccelRule),
        Box::new(SuggestionsRule),
        Box::new(PowerModeRule),
        Box::new(HagsRule),
        Box::new(WindowedGamesRule),
        Box::new(StorageSenseRule),
        Box::new(RestartRule),
        Box::new(GpuDriverRule),
        Box::new(MemoryPressureRule),
    ]
}

/// Marks protected startup items and attaches what the bloat DB knows, so the Startup page can show
/// every item with the right switch (protected ones are shown but can't be switched off).
pub fn annotate(s: &mut Snapshot, db: &BloatDb) {
    for i in &mut s.startup {
        if let Some(e) = db.lookup(Kind::Startup, &format!("{} {}", i.name, i.command)) {
            i.protected = e.verdict == Verdict::Keep;
            i.note = Some(e.reason.clone());
        }
    }
}

fn finding(id: String, category: Category, title: String) -> Finding {
    Finding {
        id,
        category,
        title,
        why: String::new(),
        evidence: vec![],
        impact: Impact::default(),
        risk: Risk::Safe,
        confidence: Confidence::High,
        fix: None,
        advice: None,
        priority: 0.0,
        score: 0.0,
    }
}

fn location_label(l: StartupLocation) -> &'static str {
    match l {
        StartupLocation::HkcuRun => "Registry, this user (Run)",
        StartupLocation::HklmRun => "Registry, all users (Run)",
        StartupLocation::HklmRun32 => "Registry, all users, 32-bit (Run)",
        StartupLocation::StartupFolderUser => "Startup folder, this user",
        StartupLocation::StartupFolderCommon => "Startup folder, all users",
    }
}

pub struct StartupRule;
impl Rule for StartupRule {
    fn id(&self) -> &'static str {
        "startup"
    }
    fn evaluate(&self, s: &Snapshot, db: &BloatDb) -> Vec<Finding> {
        s.startup
            .iter()
            .filter(|i| i.enabled)
            .filter_map(|i| {
                let e = db.lookup(Kind::Startup, &format!("{} {}", i.name, i.command))?;
                if e.verdict == Verdict::Keep {
                    return None;
                }
                let safe = e.verdict == Verdict::Safe;
                let mut f = finding(
                    format!("startup:{}", i.name),
                    Category::Startup,
                    format!("{} starts with Windows", i.name),
                );
                f.why = e.reason.clone();
                f.evidence = vec![i.command.clone(), location_label(i.location).into()];
                // Rough, identical for every item until the Prove step measures it.
                f.impact = Impact {
                    boot_ms: 600,
                    idle_ram_mb: 80,
                    idle_cpu_pct: 0.2,
                };
                f.risk = if safe { Risk::Safe } else { Risk::Caution };
                f.confidence = if safe {
                    Confidence::Medium
                } else {
                    Confidence::Low
                };
                f.fix = Some(FixPlan {
                    steps: vec![FixStep::DisableStartupItem {
                        name: i.name.clone(),
                        location: i.location,
                    }],
                    reversible: true,
                });
                Some(f)
            })
            .collect()
    }
}

/// Uninstalls apps the bloat database marks safe. Not reversible; the Store reinstalls them
/// (ADR-014).
pub struct AppxRule;
impl Rule for AppxRule {
    fn id(&self) -> &'static str {
        "appx"
    }
    fn evaluate(&self, s: &Snapshot, db: &BloatDb) -> Vec<Finding> {
        s.appx
            .iter()
            .filter_map(|pkg| {
                let e = db.lookup(Kind::Appx, pkg)?;
                if e.verdict != Verdict::Safe {
                    return None;
                }
                let mut f = finding(
                    format!("appx:{pkg}"),
                    Category::Bloat,
                    format!("{} is preinstalled", friendly_package(pkg)),
                );
                f.why = e.reason.clone();
                f.evidence = vec![format!("Packaged app: {pkg}")];
                f.priority = 0.3;
                f.risk = Risk::Caution;
                f.fix = Some(FixPlan {
                    steps: vec![FixStep::RemoveAppx {
                        package_name: pkg.clone(),
                    }],
                    reversible: false,
                });
                f.advice = Some(
                    "Removes it for your account. Forge can't put it back, but you can reinstall it from the Microsoft Store at any time."
                        .into(),
                );
                Some(f)
            })
            .collect()
    }
}

/// "Microsoft.BingNews" -> "Bing News", "king.com.CandyCrushSaga" -> "Candy Crush Saga".
pub fn friendly_package(pkg: &str) -> String {
    let last = pkg.rsplit('.').next().unwrap_or(pkg);
    let mut out = String::new();
    let mut prev_lower = false;
    for c in last.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase();
        out.push(c);
    }
    out
}

pub struct PowerPlanRule;
impl Rule for PowerPlanRule {
    fn id(&self) -> &'static str {
        "power"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        // Never push a high-performance plan on a laptop running on battery.
        if s.is_laptop && s.on_battery {
            return vec![];
        }
        // Dual-CCD X3D parts need Balanced for core parking (see power::is_dual_ccd_x3d).
        if power::is_dual_ccd_x3d(&s.cpu_name) {
            return vec![];
        }
        let Some(plan) = &s.active_power_plan else {
            return vec![];
        };
        if plan.guid != power::BALANCED {
            return vec![];
        }
        // PCs with Windows 11 power modes are tuned with the mode (PowerModeRule), not the plan.
        if s.power_mode.is_some() {
            return vec![];
        }
        // Modern Standby PCs often have no High performance plan; don't offer a fix that fails.
        let Some(target) = s
            .power_plans
            .iter()
            .find(|p| p.guid == power::HIGH_PERFORMANCE)
        else {
            return vec![];
        };
        let mut f = finding(
            "power:plan".into(),
            Category::Power,
            "Windows is on the Balanced power plan".into(),
        );
        f.why = format!(
            "Balanced lets the CPU drop to low clocks between bursts of work. {} keeps it ready at the cost of more power use when idle. The effect on games is often small and varies by CPU.",
            target.name
        );
        f.evidence = vec![format!("Active plan: {} ({})", plan.name, plan.guid)];
        f.confidence = Confidence::Low;
        f.priority = 0.6;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetPowerPlan {
                guid: power::HIGH_PERFORMANCE.into(),
            }],
            reversible: true,
        });
        f.advice = Some(if s.is_laptop {
            "On a laptop this drains the battery faster when unplugged. Switch back from History if you notice.".into()
        } else {
            "Gain varies by CPU. Undo it from History if you see no difference.".into()
        });
        vec![f]
    }
}

pub struct MemorySpeedRule;
impl Rule for MemorySpeedRule {
    fn id(&self) -> &'static str {
        "ram-speed"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        let (Some(cfg), Some(rated)) = (s.ram_configured_mts, s.ram_rated_mts) else {
            return vec![];
        };
        if (rated as f32) < cfg as f32 * 1.15 {
            return vec![];
        }
        let mut f = finding(
            "ram:xmp".into(),
            Category::Memory,
            "Memory runs slower than its rated speed".into(),
        );
        f.why = "Your RAM is rated faster than it is running. The usual cause is XMP or EXPO being off in the BIOS.".into();
        f.evidence = vec![format!("Running at {cfg} MT/s, rated {rated} MT/s")];
        f.risk = Risk::Expert;
        f.confidence = Confidence::Medium;
        f.priority = 1.2;
        f.advice = Some("Restart into the BIOS and enable XMP or EXPO. If the PC becomes unstable, turn it off again. Forge will not change firmware settings for you.".into());
        vec![f]
    }
}

pub struct SingleChannelRule;
impl Rule for SingleChannelRule {
    fn id(&self) -> &'static str {
        "ram-channels"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        // Laptops often have soldered memory, so a second module may not be possible.
        if s.is_laptop || s.ram_modules != Some(1) {
            return vec![];
        }
        let mut f = finding(
            "ram:single-channel".into(),
            Category::Memory,
            "Only one memory module is installed".into(),
        );
        f.why = "With a single module the CPU uses one memory channel, which halves memory bandwidth. Games that lean on the CPU often run noticeably slower.".into();
        f.evidence = vec![format!(
            "1 module, {} GB total",
            (s.ram_total_mb + 512) / 1024
        )];
        f.confidence = Confidence::Medium;
        f.priority = 1.0;
        f.advice = Some("Add a matching second module in the slot your motherboard manual recommends for dual channel.".into());
        vec![f]
    }
}

pub struct StorageRule;
impl Rule for StorageRule {
    fn id(&self) -> &'static str {
        "storage"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        let mut out = vec![];
        for d in s.disks.iter().filter(|d| d.is_system) {
            let free_pct = d.free_gb as f32 / d.total_gb.max(1) as f32 * 100.0;
            if free_pct < 10.0 {
                let mut f = finding(
                    format!("storage:low:{}", d.mount),
                    Category::Storage,
                    format!("{} is almost full", d.mount),
                );
                f.why = "SSDs slow down and Windows struggles to update when the system drive is nearly full.".into();
                f.evidence = vec![format!(
                    "{} GB free of {} GB ({free_pct:.0}%)",
                    d.free_gb, d.total_gb
                )];
                f.priority = 1.0;
                f.advice = Some("Review the largest folders in Settings > System > Storage, and move games or media to another drive.".into());
                out.push(f);
            }
            if d.kind == DiskKind::Hdd {
                let mut f = finding(
                    format!("storage:hdd:{}", d.mount),
                    Category::Storage,
                    "Windows is installed on a hard drive".into(),
                );
                f.why =
                    "No software setting matches the speed-up of moving Windows to an SSD.".into();
                f.evidence = vec![format!("{} is a spinning disk", d.mount)];
                f.impact = Impact {
                    boot_ms: 30_000,
                    idle_ram_mb: 0,
                    idle_cpu_pct: 0.0,
                };
                f.advice = Some("Clone or reinstall Windows onto an SSD.".into());
                out.push(f);
            }
        }
        out
    }
}

pub struct RefreshRateRule;
impl Rule for RefreshRateRule {
    fn id(&self) -> &'static str {
        "refresh-rate"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        s.displays
            .iter()
            // 59 vs 60 Hz style differences are rounding, not a setting.
            .filter(|d| d.max_hz > d.current_hz + 2)
            .map(|d| {
                let mut f = finding(
                    format!("display:refresh:{}", d.name),
                    Category::Gaming,
                    format!("{} runs at {} Hz but supports {} Hz", d.name, d.current_hz, d.max_hz),
                );
                f.why = "Windows often leaves a new monitor at 60 Hz. Games can look and feel much smoother at the higher rate, and nothing else changes.".into();
                f.evidence = vec![format!(
                    "{} × {} at {} Hz; highest offered at this resolution: {} Hz",
                    d.width, d.height, d.current_hz, d.max_hz
                )];
                f.priority = 1.5;
                f.advice = Some(format!(
                    "Open Settings > System > Display > Advanced display and choose {} Hz. Also check that the cable and port support it (DisplayPort, or HDMI 2.0 or newer).",
                    d.max_hz
                ));
                f
            })
            .collect()
    }
}

pub struct GameDvrRule;
impl Rule for GameDvrRule {
    fn id(&self) -> &'static str {
        "game-dvr"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.game_dvr_background != Some(true) {
            return vec![];
        }
        let mut f = finding(
            "gaming:dvr".into(),
            Category::Gaming,
            "Game Bar records gameplay in the background".into(),
        );
        f.why = "\"Record what happened\" keeps encoding the last minutes of every game so you can save a clip. That costs some GPU and disk work while you play.".into();
        f.evidence = vec![format!("{GAME_DVR}\\HistoricalCaptureEnabled = 1")];
        f.confidence = Confidence::Medium;
        f.priority = 0.8;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryDword {
                hive: Hive::Hkcu,
                key: GAME_DVR.into(),
                name: "HistoricalCaptureEnabled".into(),
                value: 0,
            }],
            reversible: true,
        });
        f.advice = Some("Screenshots and manual recording (Win + Alt + R) keep working.".into());
        vec![f]
    }
}

pub struct GameModeRule;
impl Rule for GameModeRule {
    fn id(&self) -> &'static str {
        "game-mode"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        // Missing means the Windows default, which is on. Only flag an explicit off.
        if s.game_mode != Some(false) {
            return vec![];
        }
        let mut f = finding(
            "gaming:game-mode".into(),
            Category::Gaming,
            "Game Mode is turned off".into(),
        );
        f.why = "Game Mode tells Windows to hold back updates, notifications and background work while a game runs.".into();
        f.evidence = vec![format!("{GAME_BAR}\\AutoGameModeEnabled = 0")];
        f.confidence = Confidence::Medium;
        f.priority = 0.7;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryDword {
                hive: Hive::Hkcu,
                key: GAME_BAR.into(),
                name: "AutoGameModeEnabled".into(),
                value: 1,
            }],
            reversible: true,
        });
        vec![f]
    }
}

pub struct MouseAccelRule;
impl Rule for MouseAccelRule {
    fn id(&self) -> &'static str {
        "mouse-accel"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.mouse_acceleration != Some(true) {
            return vec![];
        }
        let mut f = finding(
            "gaming:mouse-accel".into(),
            Category::Gaming,
            "Mouse acceleration is on".into(),
        );
        f.why = "\"Enhance pointer precision\" makes the pointer travel further when you move the mouse faster, so the same hand movement doesn't always aim the same distance. Most players turn it off.".into();
        f.evidence = vec![format!("{MOUSE}\\MouseSpeed = 1")];
        // A matter of feel, so never part of the low-risk batch.
        f.risk = Risk::Caution;
        f.confidence = Confidence::Medium;
        f.priority = 0.6;
        f.fix = Some(FixPlan {
            steps: MOUSE_VALUES
                .iter()
                .map(|n| FixStep::SetRegistryString {
                    hive: Hive::Hkcu,
                    key: MOUSE.into(),
                    name: (*n).into(),
                    value: "0".into(),
                })
                .collect(),
            reversible: true,
        });
        f.advice = Some("Takes effect the next time you sign in. The pointer will feel slower at first; adjust the pointer speed in Settings if needed.".into());
        vec![f]
    }
}

pub struct SuggestionsRule;
impl Rule for SuggestionsRule {
    fn id(&self) -> &'static str {
        "suggestions"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.windows_suggestions_on.is_empty() {
            return vec![];
        }
        let installs = s
            .windows_suggestions_on
            .iter()
            .any(|n| n == "SilentInstalledAppsEnabled");
        let mut f = finding(
            "bloat:suggestions".into(),
            Category::Bloat,
            if installs {
                "Windows installs promoted apps on its own".into()
            } else {
                "Windows shows app suggestions and tips".into()
            },
        );
        f.why = "Windows uses these switches to add sponsored apps to Start, install promoted apps and show suggestions. Turning them off keeps removed apps from coming back.".into();
        f.evidence = s
            .windows_suggestions_on
            .iter()
            .map(|n| format!("{CONTENT_DELIVERY}\\{n} = 1"))
            .collect();
        f.priority = 0.9;
        f.fix = Some(FixPlan {
            steps: s
                .windows_suggestions_on
                .iter()
                .map(|n| FixStep::SetRegistryDword {
                    hive: Hive::Hkcu,
                    key: CONTENT_DELIVERY.into(),
                    name: n.clone(),
                    value: 0,
                })
                .collect(),
            reversible: true,
        });
        f.advice = Some("Apps already installed stay until you remove them.".into());
        vec![f]
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
}

pub struct PowerModeRule;
impl Rule for PowerModeRule {
    fn id(&self) -> &'static str {
        "power-mode"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        // Never push performance on a laptop running on battery; dual-CCD X3D parts need Balanced.
        if (s.is_laptop && s.on_battery) || power::is_dual_ccd_x3d(&s.cpu_name) {
            return vec![];
        }
        let Some(mode) = &s.power_mode else {
            return vec![];
        };
        if s.active_power_plan.as_ref().map(|p| p.guid.as_str()) != Some(power::BALANCED)
            || mode == power::MODE_BEST_PERFORMANCE
        {
            return vec![];
        }
        let mut f = finding(
            "power:mode".into(),
            Category::Power,
            format!("Power mode is {}", power::mode_name(mode)),
        );
        f.why = "Windows 11's power mode decides how quickly the CPU ramps up. Best performance keeps it ready for games, at the cost of more power and heat while plugged in.".into();
        f.evidence = vec![format!(
            "Power mode on mains power: {} ({mode})",
            power::mode_name(mode)
        )];
        f.confidence = Confidence::Low;
        f.priority = 0.6;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetPowerMode {
                guid: power::MODE_BEST_PERFORMANCE.into(),
            }],
            reversible: true,
        });
        f.advice = Some(if s.is_laptop {
            "Applies when plugged in. Battery mode is left alone.".into()
        } else {
            "The effect is often small. Undo it from History if you see no difference.".into()
        });
        vec![f]
    }
}

pub struct HagsRule;
impl Rule for HagsRule {
    fn id(&self) -> &'static str {
        "hags"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.hags != Some(false) {
            return vec![];
        }
        let mut f = finding(
            "gaming:hags".into(),
            Category::Gaming,
            "Hardware-accelerated GPU scheduling is off".into(),
        );
        f.why = "Lets the graphics card manage its own memory and work queue. It can lower latency slightly and is required for DLSS Frame Generation. Effects vary by game and driver.".into();
        f.evidence = vec![format!("HKLM\\{GRAPHICS_DRIVERS}\\HwSchMode = 1")];
        f.risk = Risk::Caution;
        f.confidence = Confidence::Low;
        f.priority = 0.5;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryDword {
                hive: Hive::Hklm,
                key: GRAPHICS_DRIVERS.into(),
                name: "HwSchMode".into(),
                value: 2,
            }],
            reversible: true,
        });
        f.advice = Some("Takes effect after a restart. Older graphics cards ignore it.".into());
        vec![f]
    }
}

pub struct WindowedGamesRule;
impl Rule for WindowedGamesRule {
    fn id(&self) -> &'static str {
        "windowed-games"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        // None: the collector couldn't read DirectX preferences (e.g. not Windows).
        let Some(prefs) = s.dx_user_prefs.clone() else {
            return vec![];
        };
        if dx_pref(&prefs, "SwapEffectUpgradeEnable").as_deref() == Some("1") {
            return vec![];
        }
        let mut f = finding(
            "gaming:windowed".into(),
            Category::Gaming,
            "Optimizations for windowed games are off".into(),
        );
        f.why = "Windows can present older DirectX games running in a window or borderless window the same way as full screen, which lowers input latency and allows variable refresh rate.".into();
        f.evidence = vec![format!(
            "{DIRECTX_PREFS}\\{DIRECTX_PREFS_VALUE} = {:?}",
            prefs
        )];
        f.confidence = Confidence::Medium;
        f.priority = 0.7;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryString {
                hive: Hive::Hkcu,
                key: DIRECTX_PREFS.into(),
                name: DIRECTX_PREFS_VALUE.into(),
                value: with_dx_pref(&prefs, "SwapEffectUpgradeEnable", "1"),
            }],
            reversible: true,
        });
        f.advice = Some("Same switch as Settings > Display > Graphics > Optimizations for windowed games. Applies the next time a game starts.".into());
        vec![f]
    }
}

pub struct StorageSenseRule;
impl Rule for StorageSenseRule {
    fn id(&self) -> &'static str {
        "storage-sense"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.storage_sense != Some(false) {
            return vec![];
        }
        let mut f = finding(
            "storage:sense".into(),
            Category::Storage,
            "Storage Sense is off".into(),
        );
        f.why = "Storage Sense is Windows' own automatic cleanup. It deletes old temporary files and empties the Recycle Bin on a schedule, so the drive doesn't slowly fill up.".into();
        f.evidence = vec![format!("{STORAGE_SENSE}\\01 = 0")];
        f.priority = 0.5;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryDword {
                hive: Hive::Hkcu,
                key: STORAGE_SENSE.into(),
                name: "01".into(),
                value: 1,
            }],
            reversible: true,
        });
        f.advice = Some(
            "Uses Windows' default schedule. Adjust it in Settings > System > Storage.".into(),
        );
        vec![f]
    }
}

pub struct RestartRule;
impl Rule for RestartRule {
    fn id(&self) -> &'static str {
        "restart"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.reboot_pending {
            let mut f = finding(
                "system:reboot-pending".into(),
                Category::System,
                "A restart is waiting to finish updates".into(),
            );
            f.why = "Windows Update has installed changes that only take effect after a restart. Until then, updated drivers and fixes aren't fully in use.".into();
            f.evidence = vec!["Windows Update reports that a restart is required".into()];
            f.priority = 0.9;
            f.advice = Some("Save your work and restart when it suits you.".into());
            return vec![f];
        }
        if s.uptime_hours >= 7 * 24 {
            let days = s.uptime_hours / 24;
            let mut f = finding(
                "system:uptime".into(),
                Category::System,
                format!("This PC hasn't restarted in {days} days"),
            );
            f.why = "Shutting down with Fast Startup doesn't fully reset Windows, so memory leaks and stuck background work can build up. A restart clears them.".into();
            f.evidence = vec![format!("Up for {days} days")];
            f.confidence = Confidence::Medium;
            f.priority = 0.4;
            f.advice = Some("Use Restart rather than Shut down; it always starts fresh.".into());
            return vec![f];
        }
        vec![]
    }
}

/// Days between two YYYY-MM-DD dates (b - a), if both parse.
pub fn days_between(a: &str, b: &str) -> Option<i64> {
    fn days(d: &str) -> Option<i64> {
        let mut it = d.split('-').map(|x| x.parse::<i64>().ok());
        let (y, m, d) = (it.next()??, it.next()??, it.next()??);
        // Days from civil (Howard Hinnant's algorithm).
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Some(era * 146_097 + doe)
    }
    Some(days(b)? - days(a)?)
}

/// The only web pages the app will open from advice text. The shell refuses any other URL.
pub const HELP_LINKS: &[&str] = &[
    "https://www.nvidia.com/drivers",
    "https://www.amd.com/en/support/download/drivers.html",
    "https://www.intel.com/content/www/us/en/support/detect.html",
];

pub struct GpuDriverRule;
impl Rule for GpuDriverRule {
    fn id(&self) -> &'static str {
        "gpu-driver"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        s.gpu_drivers
            .iter()
            .filter_map(|g| {
                let date = g.date.as_deref()?;
                let age = days_between(date, &s.scan_date)?;
                if age < 180 {
                    return None;
                }
                let l = g.name.to_lowercase();
                let link = if l.contains("nvidia") {
                    HELP_LINKS[0]
                } else if l.contains("amd") || l.contains("radeon") {
                    HELP_LINKS[1]
                } else if l.contains("intel") {
                    HELP_LINKS[2]
                } else {
                    return None;
                };
                let mut f = finding(
                    format!("driver:gpu:{}", g.name),
                    Category::Drivers,
                    format!("Graphics driver is {} months old", age / 30),
                );
                f.why = "Graphics drivers often bring fixes and speed-ups for recent games. Laptop makers sometimes ship their own versions, so check theirs first on a laptop.".into();
                f.evidence = vec![format!("{}: version {}, dated {date}", g.name, g.version)];
                f.confidence = Confidence::Medium;
                f.priority = 0.8;
                f.advice = Some(format!(
                    "Download the current driver from the official site: {link}. Forge doesn't install drivers for you."
                ));
                Some(f)
            })
            .collect()
    }
}

pub struct MemoryPressureRule;
impl Rule for MemoryPressureRule {
    fn id(&self) -> &'static str {
        "memory-pressure"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        if s.ram_total_mb == 0 || s.ram_used_mb * 100 < s.ram_total_mb * 85 {
            return vec![];
        }
        let pct = s.ram_used_mb * 100 / s.ram_total_mb;
        let mut f = finding(
            "memory:pressure".into(),
            Category::Memory,
            format!("Memory is {pct}% full"),
        );
        f.why = "When memory is nearly full, Windows moves data to the disk and everything slows down, games included.".into();
        f.evidence = std::iter::once(format!(
            "{} of {} in use",
            gb(s.ram_used_mb * 1_048_576),
            gb(s.ram_total_mb * 1_048_576)
        ))
        .chain(
            s.top_memory
                .iter()
                .map(|p| format!("{}: {} MB", p.name, p.memory_mb)),
        )
        .collect();
        f.confidence = Confidence::Medium;
        f.priority = 1.1;
        f.advice = Some(
            "Close the apps you aren't using, especially browsers with many tabs, before you play."
                .into(),
        );
        vec![f]
    }
}
