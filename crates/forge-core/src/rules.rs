//! Rules turn a Snapshot into findings. Pure functions: easy to test, no I/O.
use crate::bloat::{BloatDb, Kind, Verdict};
use crate::ipc::DWORD_ALLOWLIST;
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
    ]
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

/// Advice only: removing packaged apps needs the Windows package manager, which Forge does not
/// drive yet (ADR-008). Settings removes them cleanly and the Store reinstalls them.
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
                f.priority = 0.1;
                f.advice = Some(
                    "If you don't use it, uninstall it from Settings > Apps > Installed apps. It mostly frees disk space. You can reinstall it from the Microsoft Store."
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
        let r = &DWORD_ALLOWLIST[0];
        let mut f = finding(
            "gaming:dvr".into(),
            Category::Gaming,
            "Game Bar records gameplay in the background".into(),
        );
        f.why = "\"Record what happened\" keeps encoding the last minutes of every game so you can save a clip. That costs some GPU and disk work while you play.".into();
        f.evidence = vec![format!("{}\\{} = 1", r.key, r.name)];
        f.confidence = Confidence::Medium;
        f.priority = 0.8;
        f.fix = Some(FixPlan {
            steps: vec![FixStep::SetRegistryDword {
                hive: r.hive,
                key: r.key.into(),
                name: r.name.into(),
                value: 0,
            }],
            reversible: true,
        });
        f.advice = Some("Screenshots and manual recording (Win + Alt + R) keep working.".into());
        vec![f]
    }
}
