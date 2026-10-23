//! Append-only, write-ahead undo journal.
//!
//! For every change the helper first appends an `Intent` record holding the prior state, then
//! makes the change, then appends an `Outcome`. A crash between the two leaves an intent with no
//! outcome: History shows it as interrupted and still offers undo, because restoring the prior
//! state is correct whether or not the change landed.
use crate::model::FixStep;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prior {
    /// The StartupApproved value before the change (`None`: there was no value, i.e. enabled).
    StartupApproved {
        bytes: Option<Vec<u8>>,
    },
    PowerPlan {
        guid: String,
    },
    Dword {
        existed: bool,
        value: u32,
    },
    Text {
        existed: bool,
        value: String,
    },
    /// A removed app package. Kept for the record; there is no automatic inverse.
    Package {
        full_name: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub unix_ts: u64,
    pub finding_id: String,
    pub title: String,
    pub step: FixStep,
    pub prior: Prior,
    /// Set when this entry is itself the undo of another entry.
    #[serde(default)]
    pub undoes: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    Failed { reason: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum Record {
    Intent(Entry),
    Outcome {
        id: u64,
        #[serde(flatten)]
        outcome: Outcome,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Applied,
    Failed,
    /// Intent recorded, no outcome: the helper stopped mid-change.
    Interrupted,
    Undone,
}

/// One change as History shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    pub id: u64,
    pub unix_ts: u64,
    pub finding_id: String,
    pub title: String,
    pub step: FixStep,
    pub status: Status,
    pub undoable: bool,
}

/// The step that reverses `e`, or None if there is nothing to reverse.
pub fn inverse(e: &Entry) -> Option<FixStep> {
    match (&e.step, &e.prior) {
        (
            FixStep::DisableStartupItem { name, location }
            | FixStep::RestoreStartupItem { name, location, .. },
            Prior::StartupApproved { bytes },
        ) => Some(FixStep::RestoreStartupItem {
            name: name.clone(),
            location: *location,
            prior: bytes.clone(),
        }),
        (FixStep::SetPowerPlan { .. }, Prior::PowerPlan { guid }) => {
            Some(FixStep::SetPowerPlan { guid: guid.clone() })
        }
        (
            FixStep::SetRegistryDword {
                hive, key, name, ..
            }
            | FixStep::DeleteRegistryValue { hive, key, name },
            Prior::Dword { existed, value },
        ) => Some(if *existed {
            FixStep::SetRegistryDword {
                hive: *hive,
                key: key.clone(),
                name: name.clone(),
                value: *value,
            }
        } else {
            FixStep::DeleteRegistryValue {
                hive: *hive,
                key: key.clone(),
                name: name.clone(),
            }
        }),
        (
            FixStep::SetRegistryString {
                hive, key, name, ..
            }
            | FixStep::DeleteRegistryValue { hive, key, name },
            Prior::Text { existed, value },
        ) => Some(if *existed {
            FixStep::SetRegistryString {
                hive: *hive,
                key: key.clone(),
                name: name.clone(),
                value: value.clone(),
            }
        } else {
            FixStep::DeleteRegistryValue {
                hive: *hive,
                key: key.clone(),
                name: name.clone(),
            }
        }),
        // App removal (Prior::Package) has no automatic inverse.
        _ => None,
    }
}

pub fn append(path: &Path, r: &Record) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_string(r).map_err(std::io::Error::other)?;
    line.push('\n');
    f.write_all(line.as_bytes())?;
    f.sync_data()
}

/// All readable records. Unreadable lines (e.g. a torn final write) are skipped.
pub fn read_records(path: &Path) -> std::io::Result<Vec<Record>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e),
    }
}

/// Folds records into the user's change list, newest first. Undo entries are not listed on
/// their own; they mark the entry they reversed as `Undone`.
pub fn fold(records: &[Record]) -> Vec<(Entry, Status)> {
    let mut entries: Vec<(Entry, Status)> = vec![];
    for r in records {
        match r {
            Record::Intent(e) => entries.push((e.clone(), Status::Interrupted)),
            Record::Outcome { id, outcome } => {
                let status = match outcome {
                    Outcome::Applied => Status::Applied,
                    Outcome::Failed { .. } => Status::Failed,
                };
                if let Some(slot) = entries.iter_mut().find(|(e, _)| e.id == *id) {
                    slot.1 = status;
                }
            }
        }
    }
    let undone: Vec<u64> = entries
        .iter()
        .filter(|(e, s)| *s == Status::Applied && e.undoes.is_some())
        .filter_map(|(e, _)| e.undoes)
        .collect();
    for (e, s) in entries.iter_mut() {
        if undone.contains(&e.id) {
            *s = Status::Undone;
        }
    }
    entries.retain(|(e, _)| e.undoes.is_none());
    entries.reverse();
    entries
}

pub fn history(records: &[Record]) -> Vec<Item> {
    fold(records)
        .into_iter()
        .map(|(e, status)| Item {
            undoable: matches!(status, Status::Applied | Status::Interrupted)
                && inverse(&e).is_some(),
            id: e.id,
            unix_ts: e.unix_ts,
            finding_id: e.finding_id,
            title: e.title,
            step: e.step,
            status,
        })
        .collect()
}

/// The entry `id` if it can be undone now.
pub fn undoable(records: &[Record], id: u64) -> Result<Entry, String> {
    let (e, status) = fold(records)
        .into_iter()
        .find(|(e, _)| e.id == id)
        .ok_or("That change is not in the journal.")?;
    match status {
        Status::Applied | Status::Interrupted => Ok(e),
        Status::Undone => Err("That change was already undone.".into()),
        Status::Failed => Err("That change never happened, so there is nothing to undo.".into()),
    }
}

/// Next free journal id: one more than the largest seen, so ids stay unique and ordered.
pub fn next_id(records: &[Record], now_ms: u64) -> u64 {
    let max = records
        .iter()
        .map(|r| match r {
            Record::Intent(e) => e.id,
            Record::Outcome { id, .. } => *id,
        })
        .max()
        .unwrap_or(0);
    now_ms.max(max + 1)
}
