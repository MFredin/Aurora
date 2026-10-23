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
    /// HKCU\...\Run
    HkcuRun,
    /// HKLM\...\Run (64-bit view)
    HklmRun,
    /// HKLM\Software\WOW6432Node\...\Run (32-bit apps)
    HklmRun32,
    /// %APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup
    StartupFolderUser,
    /// %ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp
    StartupFolderCommon,
}

impl StartupLocation {
    /// Where Windows (and Task Manager) records the enabled/disabled state for this location.
    pub fn approved_key(self) -> (Hive, &'static str) {
        const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        const RUN32: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
        const FOLDER: &str =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";
        match self {
            Self::HkcuRun => (Hive::Hkcu, RUN),
            Self::HklmRun => (Hive::Hklm, RUN),
            Self::HklmRun32 => (Hive::Hklm, RUN32),
            Self::StartupFolderUser => (Hive::Hkcu, FOLDER),
            Self::StartupFolderCommon => (Hive::Hklm, FOLDER),
        }
    }
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
    /// Mark a startup item disabled, the same way Task Manager does.
    DisableStartupItem {
        name: String,
        location: StartupLocation,
    },
    /// Put back the StartupApproved value a disable replaced (`None`: there was none).
    RestoreStartupItem {
        name: String,
        location: StartupLocation,
        prior: Option<Vec<u8>>,
    },
    SetPowerPlan {
        guid: String,
    },
    SetRegistryDword {
        hive: Hive,
        key: String,
        name: String,
        value: u32,
    },
    /// A text registry value (e.g. mouse settings, which Windows stores as strings).
    SetRegistryString {
        hive: Hive,
        key: String,
        name: String,
        value: String,
    },
    /// Inverse of `SetRegistry*` when the value did not exist before.
    DeleteRegistryValue {
        hive: Hive,
        key: String,
        name: String,
    },
    /// Uninstall a packaged app for the signed-in user. Not reversible: it can be reinstalled
    /// from the Microsoft Store (ADR-014).
    RemoveAppx {
        package_name: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FixPlan {
    pub steps: Vec<FixStep>,
    /// False when undo is impossible.
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
    /// None means advice only: the fix is hardware, a BIOS setting, or something Forge does not
    /// change for you.
    pub fix: Option<FixPlan>,
    pub advice: Option<String>,
    /// Importance used for ranking when there is no impact estimate (advice-only findings).
    /// Chosen per rule; see docs/DECISIONS.md ADR-009.
    pub priority: f32,
    /// Ranking score, filled in by the engine. Higher is shown first.
    #[serde(default)]
    pub score: f32,
}

impl Finding {
    /// Ranking score: the larger of the estimated benefit and the rule's priority, discounted
    /// by confidence and risk.
    pub fn compute_score(&self) -> f32 {
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
        benefit.max(self.priority) * c * r
    }
}
