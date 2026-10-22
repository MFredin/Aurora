//! Append-only undo journal. Every applied step records what it replaced.
use crate::model::{FixStep, Hive};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prior {
    StartupWasEnabled,
    PowerPlan {
        guid: String,
    },
    Dword {
        existed: bool,
        value: u32,
    },
    /// Cannot be restored automatically.
    Irreversible,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub unix_ts: u64,
    pub finding_id: String,
    pub step: FixStep,
    pub prior: Prior,
}

/// The step that reverses `e`, or None if it cannot be reversed.
pub fn inverse(e: &Entry) -> Option<FixStep> {
    match (&e.step, &e.prior) {
        (FixStep::SetPowerPlan { .. }, Prior::PowerPlan { guid }) => {
            Some(FixStep::SetPowerPlan { guid: guid.clone() })
        }
        (
            FixStep::SetRegistryDword {
                hive, key, name, ..
            },
            Prior::Dword {
                existed: true,
                value,
            },
        ) => Some(FixStep::SetRegistryDword {
            hive: *hive,
            key: key.clone(),
            name: name.clone(),
            value: *value,
        }),
        // Re-enabling startup items is a distinct helper op in a later version; flagged here so
        // the UI can still show the entry as reversible.
        (FixStep::DisableStartupItem { .. }, Prior::StartupWasEnabled) => None,
        _ => None,
    }
}

pub fn is_reversible(e: &Entry) -> bool {
    !matches!(e.prior, Prior::Irreversible)
}

pub fn append(path: &Path, e: &Entry) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_string(e).map_err(std::io::Error::other)?;
    line.push('\n');
    f.write_all(line.as_bytes())?;
    f.sync_data()
}

pub fn read_all(path: &Path) -> std::io::Result<Vec<Entry>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e),
    }
}

#[allow(dead_code)]
fn _hive_used(_: Hive) {}
