//! Rules turn a Snapshot into findings. Pure functions: easy to test, no I/O.
use crate::bloat::{BloatDb, Kind, Verdict};
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
        Box::new(StorageRule),
    ]
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
                let risk = if e.verdict == Verdict::Safe {
                    Risk::Safe
                } else {
                    Risk::Caution
                };
                Some(Finding {
                    id: format!("startup:{}", i.name),
                    category: Category::Startup,
                    title: format!("{} starts with Windows", i.name),
                    why: e.reason.clone(),
                    evidence: vec![i.command.clone()],
                    // Placeholder estimates; the Prove step replaces them with measured values.
                    impact: Impact {
                        boot_ms: 600,
                        idle_ram_mb: 80,
                        idle_cpu_pct: 0.2,
                    },
                    risk,
                    confidence: if e.verdict == Verdict::Safe {
                        Confidence::High
                    } else {
                        Confidence::Medium
                    },
                    fix: Some(FixPlan {
                        steps: vec![FixStep::DisableStartupItem {
                            name: i.name.clone(),
                            location: i.location,
                        }],
                        reversible: true,
                    }),
                    advice: None,
                })
            })
            .collect()
    }
}

pub struct AppxRule;
impl Rule for AppxRule {
    fn id(&self) -> &'static str {
        "appx"
    }
    fn evaluate(&self, s: &Snapshot, db: &BloatDb) -> Vec<Finding> {
        s.appx.iter().filter_map(|pkg| {
            let e = db.lookup(Kind::Appx, pkg)?;
            if e.verdict != Verdict::Safe { return None; }
            Some(Finding {
                id: format!("appx:{pkg}"),
                category: Category::Bloat,
                title: format!("{pkg} is preinstalled"),
                why: e.reason.clone(),
                evidence: vec![format!("Packaged app: {pkg}")],
                impact: Impact { boot_ms: 0, idle_ram_mb: 0, idle_cpu_pct: 0.0 },
                risk: Risk::Caution,
                confidence: Confidence::High,
                fix: Some(FixPlan { steps: vec![FixStep::RemoveAppx { package_name: pkg.clone() }], reversible: false }),
                advice: Some("Frees disk space only. Reinstall later from the Microsoft Store if needed.".into()),
            })
        }).collect()
    }
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
        let Some(plan) = &s.active_power_plan else {
            return vec![];
        };
        if plan.guid != power::BALANCED {
            return vec![];
        }
        vec![Finding {
            id: "power:plan".into(),
            category: Category::Power,
            title: "Windows is on the Balanced power plan".into(),
            why: "Balanced lets the CPU clock down between bursts, which can add latency in games. High performance keeps it responsive at the cost of idle power.".into(),
            evidence: vec![format!("Active plan: {} ({})", plan.name, plan.guid)],
            impact: Impact { boot_ms: 0, idle_ram_mb: 0, idle_cpu_pct: 0.0 },
            risk: Risk::Safe,
            confidence: Confidence::Medium,
            fix: Some(FixPlan { steps: vec![FixStep::SetPowerPlan { guid: power::HIGH_PERFORMANCE.into() }], reversible: true }),
            advice: Some("Gain varies by CPU. The Prove step will measure it on this machine.".into()),
        }]
    }
}

pub struct MemorySpeedRule;
impl Rule for MemorySpeedRule {
    fn id(&self) -> &'static str {
        "ram-speed"
    }
    fn evaluate(&self, s: &Snapshot, _: &BloatDb) -> Vec<Finding> {
        let (Some(cfg), Some(rated)) = (s.ram_configured_mhz, s.ram_rated_mhz) else {
            return vec![];
        };
        if (rated as f32) < cfg as f32 * 1.15 {
            return vec![];
        }
        vec![Finding {
            id: "ram:xmp".into(),
            category: Category::Memory,
            title: "Memory runs slower than its rated speed".into(),
            why: "Your RAM is rated faster than it is running. The usual cause is XMP or EXPO being off in the BIOS.".into(),
            evidence: vec![format!("Running at {cfg} MHz, rated {rated} MHz")],
            impact: Impact { boot_ms: 0, idle_ram_mb: 0, idle_cpu_pct: 0.0 },
            risk: Risk::Expert,
            confidence: Confidence::Medium,
            fix: None,
            advice: Some("Restart into the BIOS and enable XMP or EXPO. Forge will not change firmware settings for you.".into()),
        }]
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
                out.push(Finding {
                    id: format!("storage:low:{}", d.mount),
                    category: Category::Storage,
                    title: format!("{} is almost full", d.mount),
                    why: "SSDs slow down and Windows struggles to update when the system drive is nearly full.".into(),
                    evidence: vec![format!("{} GB free of {} GB ({free_pct:.0}%)", d.free_gb, d.total_gb)],
                    impact: Impact::default(), risk: Risk::Safe, confidence: Confidence::High,
                    fix: None,
                    advice: Some("Review the largest folders and move games or media to another drive.".into()),
                });
            }
            if d.kind == DiskKind::Hdd {
                out.push(Finding {
                    id: format!("storage:hdd:{}", d.mount),
                    category: Category::Storage,
                    title: "Windows is installed on a hard drive".into(),
                    why: "No software setting matches the speed-up of moving Windows to an SSD."
                        .into(),
                    evidence: vec![format!("{} is a spinning disk", d.mount)],
                    impact: Impact {
                        boot_ms: 30_000,
                        idle_ram_mb: 0,
                        idle_cpu_pct: 0.0,
                    },
                    risk: Risk::Safe,
                    confidence: Confidence::High,
                    fix: None,
                    advice: Some("Clone or reinstall Windows onto an SSD.".into()),
                });
            }
        }
        out
    }
}
