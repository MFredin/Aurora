//! Wire protocol between the unprivileged app and the elevated helper, plus the
//! allowlist the helper enforces. The helper never executes anything outside this list.
use crate::model::{FixStep, Hive};
use crate::power;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    Apply { finding_id: String, step: FixStep },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Applied { journal_id: u64 },
    Rejected { reason: String },
}

/// Registry values the helper may write. Everything else is refused.
const DWORD_ALLOWLIST: &[(Hive, &str, &str)] = &[
    (
        Hive::Hkcu,
        r"Software\Microsoft\GameBar",
        "AutoGameModeEnabled",
    ),
    (
        Hive::Hkcu,
        r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager",
        "SubscribedContent-338388Enabled",
    ),
];

pub fn validate(step: &FixStep) -> Result<(), String> {
    match step {
        FixStep::SetPowerPlan { guid } if power::KNOWN_PLANS.contains(&guid.as_str()) => Ok(()),
        FixStep::SetPowerPlan { guid } => Err(format!("unknown power plan {guid}")),
        FixStep::SetRegistryDword {
            hive, key, name, ..
        } => {
            if DWORD_ALLOWLIST.iter().any(|(h, k, n)| {
                h == hive && k.eq_ignore_ascii_case(key) && n.eq_ignore_ascii_case(name)
            }) {
                Ok(())
            } else {
                Err(format!(
                    "registry value {key}\\{name} is not on the allowlist"
                ))
            }
        }
        FixStep::DisableStartupItem { name, .. } | FixStep::RemoveAppx { package_name: name } => {
            if name.is_empty()
                || name.len() > 200
                || name
                    .chars()
                    .any(|c| c.is_control() || "&|;<>\"`".contains(c))
            {
                Err("name contains disallowed characters".into())
            } else {
                Ok(())
            }
        }
    }
}
