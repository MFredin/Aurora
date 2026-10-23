//! Wire protocol between the unprivileged app and the elevated helper, plus the
//! allowlist the helper enforces. The helper never executes anything outside this list.
use crate::bloat::{BloatDb, Kind, Verdict};
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

/// What a registry value on the allowlist may be set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allowed {
    Dword(&'static [u32]),
    /// Short decimal text, e.g. the mouse threshold values ("0", "6", "10").
    DigitText,
}

/// One registry value the helper may write, and the values it may set.
pub struct RegRule {
    pub hive: Hive,
    pub key: &'static str,
    pub name: &'static str,
    pub allowed: Allowed,
}

pub const GAME_DVR: &str = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
pub const GAME_BAR: &str = r"Software\Microsoft\GameBar";
pub const MOUSE: &str = r"Control Panel\Mouse";
pub const CONTENT_DELIVERY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";

/// Windows "suggestions" switches: silently installed promoted apps, Start and Settings
/// suggestions, and tips. All per user, 0 = off.
pub const SUGGESTION_VALUES: [&str; 6] = [
    "SilentInstalledAppsEnabled",
    "SystemPaneSuggestionsEnabled",
    "SubscribedContent-338388Enabled",
    "SubscribedContent-338389Enabled",
    "SubscribedContent-353694Enabled",
    "SubscribedContent-353696Enabled",
];

pub const MOUSE_VALUES: [&str; 3] = ["MouseSpeed", "MouseThreshold1", "MouseThreshold2"];

const fn dword(key: &'static str, name: &'static str) -> RegRule {
    RegRule {
        hive: Hive::Hkcu,
        key,
        name,
        allowed: Allowed::Dword(&[0, 1]),
    }
}
const fn digits(key: &'static str, name: &'static str) -> RegRule {
    RegRule {
        hive: Hive::Hkcu,
        key,
        name,
        allowed: Allowed::DigitText,
    }
}

/// Registry values the helper may write. Everything else is refused. All per user (HKCU), all
/// user preferences with nothing to do with security.
pub const REG_ALLOWLIST: &[RegRule] = &[
    // Game Bar "Record what happened" (background recording).
    dword(GAME_DVR, "HistoricalCaptureEnabled"),
    // Game Mode.
    dword(GAME_BAR, "AutoGameModeEnabled"),
    // "Enhance pointer precision" (mouse acceleration).
    digits(MOUSE, "MouseSpeed"),
    digits(MOUSE, "MouseThreshold1"),
    digits(MOUSE, "MouseThreshold2"),
    // Promoted apps and suggestions.
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[0]),
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[1]),
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[2]),
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[3]),
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[4]),
    dword(CONTENT_DELIVERY, SUGGESTION_VALUES[5]),
];

pub const MAX_NAME: usize = 260;
pub const MAX_APPROVED_BYTES: usize = 64;

pub fn reg_rule(hive: Hive, key: &str, name: &str) -> Option<&'static RegRule> {
    REG_ALLOWLIST.iter().find(|r| {
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

fn not_listed(key: &str, name: &str) -> String {
    format!("registry value {key}\\{name} is not on the allowlist")
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
        } => match reg_rule(*hive, key, name).map(|r| r.allowed) {
            Some(Allowed::Dword(ok)) if ok.contains(value) => Ok(()),
            Some(_) => Err(format!("value {value} is not allowed for {key}\\{name}")),
            None => Err(not_listed(key, name)),
        },
        FixStep::SetRegistryString {
            hive,
            key,
            name,
            value,
        } => match reg_rule(*hive, key, name).map(|r| r.allowed) {
            Some(Allowed::DigitText)
                if !value.is_empty()
                    && value.len() <= 4
                    && value.chars().all(|c| c.is_ascii_digit()) =>
            {
                Ok(())
            }
            Some(_) => Err(format!("value {value:?} is not allowed for {key}\\{name}")),
            None => Err(not_listed(key, name)),
        },
        FixStep::DeleteRegistryValue { hive, key, name } => match reg_rule(*hive, key, name) {
            Some(_) => Ok(()),
            None => Err(not_listed(key, name)),
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
        // Only apps the bloat database explicitly marks safe to remove. Never a keep entry,
        // never an app the database doesn't know.
        FixStep::RemoveAppx { package_name } => {
            let well_formed = !package_name.is_empty()
                && package_name.len() <= 128
                && package_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            if !well_formed {
                return Err("not a package name".into());
            }
            match db.lookup(Kind::Appx, package_name) {
                Some(e) if e.verdict == Verdict::Safe => Ok(()),
                _ => Err(format!(
                    "{package_name} is not on Forge's list of apps that are safe to remove"
                )),
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
        FixStep::SetRegistryString { name, value, .. } => format!("Set {name} to {value}"),
        FixStep::DeleteRegistryValue { name, .. } => format!("Remove the {name} setting"),
        FixStep::RemoveAppx { package_name } => format!("Uninstall {package_name}"),
    }
}
