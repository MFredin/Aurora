//! Data model shared by the scanner, the UI (via serde) and the elevated helper.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Startup,
    Bloat,
    Power,
    Memory,
    Storage,
    Drivers,
    Thermal,
    Gaming,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,
    Caution,
    Expert,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Estimated gain. These are estimates until the Prove step measures them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Impact {
    pub boot_ms: u32,
    pub idle_ram_mb: u32,
    pub idle_cpu_pct: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartupLocation {
    HkcuRun,
    HklmRun,
    StartupFolder,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Hive {
    Hkcu,
    Hklm,
}

/// The only operations the elevated helper will ever perform. No shell strings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FixStep {
    DisableStartupItem {
        name: String,
        location: StartupLocation,
    },
    SetPowerPlan {
        guid: String,
    },
    RemoveAppx {
        package_name: String,
    },
    SetRegistryDword {
        hive: Hive,
        key: String,
        name: String,
        value: u32,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FixPlan {
    pub steps: Vec<FixStep>,
    /// False when undo is impossible (e.g. removing a packaged app).
    pub reversible: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub id: String,
    pub category: Category,
    pub title: String,
    /// Plain-language reason, shown first in the UI.
    pub why: String,
    /// Raw facts the conclusion rests on, so the user can check it.
    pub evidence: Vec<String>,
    pub impact: Impact,
    pub risk: Risk,
    pub confidence: Confidence,
    /// None means advice only: the fix is hardware or a BIOS setting.
    pub fix: Option<FixPlan>,
    pub advice: Option<String>,
}

impl Finding {
    /// Ranking score: estimated benefit, discounted by confidence and risk.
    pub fn score(&self) -> f32 {
        let benefit = self.impact.boot_ms as f32 / 1000.0
            + self.impact.idle_ram_mb as f32 / 200.0
            + self.impact.idle_cpu_pct * 2.0;
        let c = match self.confidence {
            Confidence::High => 1.0,
            Confidence::Medium => 0.7,
            Confidence::Low => 0.4,
        };
        let r = match self.risk {
            Risk::Safe => 1.0,
            Risk::Caution => 0.8,
            Risk::Expert => 0.5,
        };
        benefit * c * r
    }
}
