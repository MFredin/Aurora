//! Wire protocol between the unprivileged app and the elevated helper, plus the
//! allowlist the helper enforces. The helper never executes anything outside this list.
use crate::bloat::{BloatDb, Kind};
use crate::model::{FixStep, Hive};
use crate::power;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    /// Apply one step of a finding's fix plan. `title` is only stored in the journal for History.
    Apply {
        finding_id: String,
        title: String,
        step: FixStep,
    },
    /// Reverse an earlier change. The helper reads its own journal to work out the inverse; the
    /// app never sends the inverse step itself.
    Undo {
        journal_id: u64,
    },
    /// Ask Windows for a System Restore point before the first change of a session.
    CreateRestorePoint,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Applied {
        journal_id: u64,
    },
    /// The change happened but its outcome could not be recorded. Undo still works from the
    /// intent record written before the change; the UI must still say this.
    AppliedWithWarning {
        journal_id: u64,
        warning: String,
    },
    Undone {
        journal_id: u64,
    },
    RestorePoint {
        created: bool,
        message: String,
    },
    /// Refused before anything was touched (allowlist, journal not writable, bad request).
    Rejected {
        reason: String,
    },
    /// Windows refused the change. Nothing was changed.
    Failed {
        reason: String,
    },
}

/// One registry value the helper may write, and the values it may set.
pub struct DwordRule {
    pub hive: Hive,
    pub key: &'static str,
    pub name: &'static str,
    pub allowed: &'static [u32],
}

/// Registry values the helper may write. Everything else is refused.
pub const DWORD_ALLOWLIST: &[DwordRule] = &[DwordRule {
    // Game Bar "Record what happened" (background recording).
    hive: Hive::Hkcu,
    key: r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
    name: "HistoricalCaptureEnabled",
    allowed: &[0, 1],
}];

pub const MAX_NAME: usize = 260;
pub const MAX_APPROVED_BYTES: usize = 64;

fn dword_rule(hive: Hive, key: &str, name: &str) -> Option<&'static DwordRule> {
    DWORD_ALLOWLIST.iter().find(|r| {
        r.hive == hive && r.key.eq_ignore_ascii_case(key) && r.name.eq_ignore_ascii_case(name)
    })
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > MAX_NAME
        || name
            .chars()
            .any(|c| c.is_control() || "&|;<>\"`\\/*?".contains(c))
    {
        Err("name contains disallowed characters".into())
    } else {
        Ok(())
    }
}

/// The helper's allowlist. Runs on every request, independent of who produced the step.
pub fn validate(step: &FixStep, db: &BloatDb) -> Result<(), String> {
    match step {
        // Any plan GUID is allowed so undo can restore OEM or custom plans; activating an
        // existing plan cannot weaken security. The format check keeps arguments to powercfg plain.
        FixStep::SetPowerPlan { guid } if power::is_guid(guid) => Ok(()),
        FixStep::SetPowerPlan { guid } => Err(format!("not a power plan id: {guid}")),
        FixStep::SetRegistryDword {
            hive,
            key,
            name,
            value,
        } => match dword_rule(*hive, key, name) {
            Some(r) if r.allowed.contains(value) => Ok(()),
            Some(_) => Err(format!("value {value} is not allowed for {key}\\{name}")),
            None => Err(format!(
                "registry value {key}\\{name} is not on the allowlist"
            )),
        },
        FixStep::DeleteRegistryValue { hive, key, name } => match dword_rule(*hive, key, name) {
            Some(_) => Ok(()),
            None => Err(format!(
                "registry value {key}\\{name} is not on the allowlist"
            )),
        },
        FixStep::DisableStartupItem { name, .. } => {
            check_name(name)?;
            if db.is_keep(Kind::Startup, name) {
                return Err(format!("{name} is a protected Windows or driver component"));
            }
            Ok(())
        }
        FixStep::RestoreStartupItem { name, prior, .. } => {
            check_name(name)?;
            match prior {
                Some(b) if b.is_empty() || b.len() > MAX_APPROVED_BYTES => {
                    Err("unexpected startup state data".into())
                }
                _ => Ok(()),
            }
        }
    }
}

/// Short, plain description of a step for errors and logs.
pub fn describe(step: &FixStep) -> String {
    match step {
        FixStep::DisableStartupItem { name, .. } => {
            format!("Stop {name} from starting with Windows")
        }
        FixStep::RestoreStartupItem { name, .. } => format!("Let {name} start with Windows again"),
        FixStep::SetPowerPlan { guid } => format!("Switch the power plan to {}", power::name(guid)),
        FixStep::SetRegistryDword { name, value, .. } => format!("Set {name} to {value}"),
        FixStep::DeleteRegistryValue { name, .. } => format!("Remove the {name} setting"),
    }
}
