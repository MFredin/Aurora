//! forge-helper: the only elevated process. It exposes a tiny, allowlisted set of operations.
//!
//! Transport: in release the app starts this binary through a UAC prompt with
//! `--pipe \\.\pipe\forge-<id> --user-sid <SID>` and talks newline-delimited JSON over that
//! named pipe. With `FORGE_HELPER_STDIO=1` (development) the app runs it unelevated on stdio.
//! Either way the helper exits after 5 minutes without a request, or as soon as the app closes the channel, so it costs nothing when unused.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use forge_core::bloat::BloatDb;
use forge_core::ipc::{Request, Response, validate};
use forge_core::journal::{self, Entry, Outcome, Prior, Record};
use forge_core::model::FixStep;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

#[cfg(windows)]
mod win;

/// Long enough that applying several fixes needs one UAC prompt, short enough that an
/// elevated process never lingers (ADR-007).
const IDLE_EXIT: Duration = Duration::from_secs(300);

/// Platform side of the helper. Everything that touches the system lives behind this trait so
/// the protocol, journal and safety logic are tested on any OS.
trait Executor {
    /// Extra checks that need the live system (e.g. re-reading a startup item's command).
    fn check(&mut self, step: &FixStep, db: &BloatDb) -> Result<(), String>;
    /// What the step will replace, read before anything changes.
    fn prior(&mut self, step: &FixStep) -> Result<Prior, String>;
    fn apply(&mut self, step: &FixStep) -> Result<(), String>;
    /// Makes sure the journal folder exists and only administrators can write it.
    fn prepare_journal_dir(&mut self, dir: &Path) -> Result<(), String>;
    fn restore_point(&mut self) -> Result<(bool, String), String>;
}

struct Helper<'a> {
    exec: &'a mut dyn Executor,
    journal: PathBuf,
    db: BloatDb,
}

impl Helper<'_> {
    fn handle(&mut self, req: Request) -> Response {
        match req {
            Request::Ping => Response::Pong,
            Request::CreateRestorePoint => match self.exec.restore_point() {
                Ok((created, message)) => Response::RestorePoint { created, message },
                Err(message) => Response::RestorePoint {
                    created: false,
                    message,
                },
            },
            Request::Apply {
                finding_id,
                title,
                step,
                group,
            } => {
                if title.len() > 300 || finding_id.len() > 300 {
                    return Response::Rejected {
                        reason: "request too long".into(),
                    };
                }
                match self.run(&finding_id, &title, step, None, group) {
                    Ok((id, None)) => Response::Applied { journal_id: id },
                    Ok((id, Some(warning))) => Response::AppliedWithWarning {
                        journal_id: id,
                        warning,
                    },
                    Err(r) => r,
                }
            }
            Request::Undo { journal_id } => {
                let records = match journal::read_records(&self.journal) {
                    Ok(r) => r,
                    Err(e) => {
                        return Response::Rejected {
                            reason: format!("Could not read the undo journal: {e}"),
                        };
                    }
                };
                let entry = match journal::undoable(&records, journal_id) {
                    Ok(e) => e,
                    Err(reason) => return Response::Rejected { reason },
                };
                let Some(step) = journal::inverse(&entry) else {
                    return Response::Rejected {
                        reason: "This change cannot be undone automatically.".into(),
                    };
                };
                let title = format!("Undo: {}", entry.title);
                match self.run(
                    &entry.finding_id,
                    &title,
                    step,
                    Some(journal_id),
                    entry.group,
                ) {
                    Ok(_) => Response::Undone { journal_id },
                    Err(r) => r,
                }
            }
        }
    }

    /// Validate, journal the intent, apply, journal the outcome. Returns the new journal id and
    /// a warning when the change landed but its outcome could not be recorded.
    fn run(
        &mut self,
        finding_id: &str,
        title: &str,
        step: FixStep,
        undoes: Option<u64>,
        group: Option<u64>,
    ) -> Result<(u64, Option<String>), Response> {
        let rejected = |reason: String| Response::Rejected { reason };
        validate(&step, &self.db).map_err(rejected)?;
        self.exec.check(&step, &self.db).map_err(rejected)?;
        if let Some(dir) = self.journal.parent() {
            self.exec.prepare_journal_dir(dir).map_err(rejected)?;
        }
        let records = journal::read_records(&self.journal)
            .map_err(|e| rejected(format!("Could not read the undo journal: {e}")))?;
        let prior = self
            .exec
            .prior(&step)
            .map_err(|reason| Response::Failed { reason })?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let entry = Entry {
            id: journal::next_id(&records, now.as_millis() as u64),
            unix_ts: now.as_secs(),
            finding_id: finding_id.into(),
            title: title.into(),
            step,
            prior,
            undoes,
            group,
        };
        let id = entry.id;
        // Write-ahead: no journal entry, no change.
        journal::append(&self.journal, &Record::Intent(entry.clone())).map_err(|e| {
            rejected(format!(
                "Could not write the undo journal, so nothing was changed: {e}"
            ))
        })?;
        match self.exec.apply(&entry.step) {
            Ok(()) => {
                let outcome = Record::Outcome {
                    id,
                    outcome: Outcome::Applied,
                };
                let warning = journal::append(&self.journal, &outcome).err().map(|e| {
                    format!("The change was made, but Forge could not record that it finished ({e}). Undo still works from History.")
                });
                Ok((id, warning))
            }
            Err(reason) => {
                let _ = journal::append(
                    &self.journal,
                    &Record::Outcome {
                        id,
                        outcome: Outcome::Failed {
                            reason: reason.clone(),
                        },
                    },
                );
                Err(Response::Failed { reason })
            }
        }
    }
}

#[cfg_attr(windows, allow(dead_code))]
struct Unsupported;
impl Executor for Unsupported {
    fn check(&mut self, _: &FixStep, _: &BloatDb) -> Result<(), String> {
        Ok(())
    }
    fn prior(&mut self, _: &FixStep) -> Result<Prior, String> {
        Err("The helper only changes settings on Windows.".into())
    }
    fn apply(&mut self, _: &FixStep) -> Result<(), String> {
        Err("The helper only changes settings on Windows.".into())
    }
    fn prepare_journal_dir(&mut self, _: &Path) -> Result<(), String> {
        Ok(())
    }
    fn restore_point(&mut self) -> Result<(bool, String), String> {
        Ok((false, "Restore points exist only on Windows.".into()))
    }
}

struct Args {
    pipe: Option<String>,
    user_sid: Option<String>,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        pipe: None,
        user_sid: None,
    };
    while let Some(flag) = it.next() {
        let val = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--pipe" if val.starts_with(r"\\.\pipe\forge-") && val.len() < 100 => {
                a.pipe = Some(val)
            }
            "--user-sid"
                if val.starts_with("S-1-")
                    && val.len() < 200
                    && val
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '-' || c == 'S') =>
            {
                a.user_sid = Some(val)
            }
            _ => return Err(format!("bad argument {flag}")),
        }
    }
    Ok(a)
}

/// Reads requests on a thread so the main loop can stop after `IDLE_EXIT` without one.
fn serve(input: Box<dyn Read + Send>, mut output: Box<dyn Write>, helper: &mut Helper) {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(input).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    while let Ok(line) = rx.recv_timeout(IDLE_EXIT) {
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => helper.handle(req),
            Err(e) => Response::Rejected {
                reason: format!("bad request: {e}"),
            },
        };
        let Ok(text) = serde_json::to_string(&resp) else {
            break;
        };
        if writeln!(output, "{text}")
            .and_then(|_| output.flush())
            .is_err()
        {
            break; // the app went away
        }
    }
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("forge-helper: {e}");
            std::process::exit(2);
        }
    };
    let dev = args.pipe.is_none();
    #[cfg(windows)]
    let mut exec = win::WinExec::new(args.user_sid.clone(), dev);
    #[cfg(not(windows))]
    let mut exec = Unsupported;
    #[cfg(not(windows))]
    let _ = &args.user_sid;

    let mut helper = Helper {
        exec: &mut exec,
        journal: forge_core::paths::journal_path(dev),
        db: BloatDb::embedded(),
    };
    match &args.pipe {
        // Two one-way pipes (see src-tauri/src/helper.rs for why not one duplex pipe).
        Some(base) => {
            let req = std::fs::OpenOptions::new()
                .read(true)
                .open(format!("{base}-req"));
            let resp = std::fs::OpenOptions::new()
                .write(true)
                .open(format!("{base}-resp"));
            match (req, resp) {
                (Ok(req), Ok(resp)) => serve(Box::new(req), Box::new(resp), &mut helper),
                _ => std::process::exit(3),
            }
        }
        None => serve(
            Box::new(std::io::stdin()),
            Box::new(std::io::stdout()),
            &mut helper,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::model::{Hive, StartupLocation};
    use forge_core::power;

    /// Records calls and simulates a system with a power plan and a registry value.
    struct Fake {
        applied: Vec<FixStep>,
        plan: String,
        fail_apply: bool,
        bad_dir: bool,
    }
    impl Fake {
        fn new() -> Self {
            Fake {
                applied: vec![],
                plan: power::BALANCED.into(),
                fail_apply: false,
                bad_dir: false,
            }
        }
    }
    impl Executor for Fake {
        fn check(&mut self, _: &FixStep, _: &BloatDb) -> Result<(), String> {
            Ok(())
        }
        fn prior(&mut self, step: &FixStep) -> Result<Prior, String> {
            Ok(match step {
                FixStep::SetPowerPlan { .. } => Prior::PowerPlan {
                    guid: self.plan.clone(),
                },
                FixStep::DisableStartupItem { .. } | FixStep::RestoreStartupItem { .. } => {
                    Prior::StartupApproved { bytes: None }
                }
                _ => Prior::Dword {
                    existed: false,
                    value: 0,
                },
            })
        }
        fn apply(&mut self, step: &FixStep) -> Result<(), String> {
            if self.fail_apply {
                return Err("Access is denied.".into());
            }
            if let FixStep::SetPowerPlan { guid } = step {
                self.plan = guid.clone();
            }
            self.applied.push(step.clone());
            Ok(())
        }
        fn prepare_journal_dir(&mut self, _: &Path) -> Result<(), String> {
            if self.bad_dir {
                Err("journal folder not owned by administrators".into())
            } else {
                Ok(())
            }
        }
        fn restore_point(&mut self) -> Result<(bool, String), String> {
            Ok((true, "Restore point created.".into()))
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("forge-helper-{}-{tag}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn apply(step: FixStep) -> Request {
        Request::Apply {
            finding_id: "f".into(),
            title: "Title".into(),
            group: None,
            step,
        }
    }

    fn hp() -> FixStep {
        FixStep::SetPowerPlan {
            guid: power::HIGH_PERFORMANCE.into(),
        }
    }

    #[test]
    fn rejected_steps_never_reach_the_executor_or_journal() {
        let (mut ex, p) = (Fake::new(), tmp("reject"));
        let mut h = Helper {
            exec: &mut ex,
            journal: p.clone(),
            db: BloatDb::embedded(),
        };
        for bad in [
            FixStep::SetRegistryDword {
                hive: Hive::Hklm,
                key: "SOFTWARE\\Policies\\Microsoft\\Windows Defender".into(),
                name: "DisableAntiSpyware".into(),
                value: 1,
            },
            FixStep::DisableStartupItem {
                name: "SecurityHealth".into(),
                location: StartupLocation::HklmRun,
            },
        ] {
            assert!(matches!(h.handle(apply(bad)), Response::Rejected { .. }));
        }
        assert!(ex.applied.is_empty());
        assert!(!p.exists());
    }

    #[test]
    fn apply_then_undo_restores_the_prior_plan() {
        let (mut ex, p) = (Fake::new(), tmp("undo"));
        let mut h = Helper {
            exec: &mut ex,
            journal: p.clone(),
            db: BloatDb::embedded(),
        };
        let Response::Applied { journal_id } = h.handle(apply(hp())) else {
            panic!("apply failed")
        };
        assert_eq!(
            h.handle(Request::Undo { journal_id }),
            Response::Undone { journal_id }
        );
        // A second undo is refused, and History shows one undone change.
        assert!(matches!(
            h.handle(Request::Undo { journal_id }),
            Response::Rejected { .. }
        ));
        let hist = journal::history(&journal::read_records(&p).unwrap());
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].status, journal::Status::Undone);
        assert_eq!(ex.plan, power::BALANCED);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn failed_apply_is_journaled_as_failed_and_reported() {
        let (mut ex, p) = (Fake::new(), tmp("fail"));
        ex.fail_apply = true;
        let mut h = Helper {
            exec: &mut ex,
            journal: p.clone(),
            db: BloatDb::embedded(),
        };
        assert!(matches!(h.handle(apply(hp())), Response::Failed { .. }));
        let hist = journal::history(&journal::read_records(&p).unwrap());
        assert_eq!(hist[0].status, journal::Status::Failed);
        assert!(!hist[0].undoable);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn untrusted_journal_folder_blocks_changes() {
        let (mut ex, p) = (Fake::new(), tmp("dir"));
        ex.bad_dir = true;
        let mut h = Helper {
            exec: &mut ex,
            journal: p.clone(),
            db: BloatDb::embedded(),
        };
        assert!(matches!(h.handle(apply(hp())), Response::Rejected { .. }));
        assert!(ex.applied.is_empty());
    }

    #[test]
    fn unwritable_journal_means_nothing_changes() {
        let (mut ex, _) = (Fake::new(), ());
        // A path under a regular file can never be created.
        let file = tmp("blocker");
        std::fs::write(&file, b"x").unwrap();
        let mut h = Helper {
            exec: &mut ex,
            journal: file.join("journal.jsonl"),
            db: BloatDb::embedded(),
        };
        let r = h.handle(apply(hp()));
        assert!(matches!(r, Response::Rejected { .. }), "{r:?}");
        assert!(ex.applied.is_empty());
        std::fs::remove_file(file).ok();
    }

    #[test]
    fn arguments_are_strict() {
        let ok = parse_args(
            [
                "--pipe",
                r"\\.\pipe\forge-123-abc",
                "--user-sid",
                "S-1-5-21-1-2-3-1001",
            ]
            .map(String::from)
            .into_iter(),
        )
        .unwrap();
        assert!(ok.pipe.is_some() && ok.user_sid.is_some());
        for bad in [
            vec!["--pipe", r"\\.\pipe\other"],
            vec!["--user-sid", "S-1-5; rm"],
            vec!["--pipe"],
            vec!["--evil", "x"],
        ] {
            assert!(parse_args(bad.into_iter().map(String::from)).is_err());
        }
    }

    #[test]
    fn idle_helper_exits_after_input_closes() {
        let (mut ex, p) = (Fake::new(), tmp("serve"));
        let mut h = Helper {
            exec: &mut ex,
            journal: p,
            db: BloatDb::embedded(),
        };
        let input = b"{\"type\":\"ping\"}\nnot json\n".to_vec();
        let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        struct Sink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serve(
            Box::new(std::io::Cursor::new(input)),
            Box::new(Sink(out.clone())),
            &mut h,
        );
        let text = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("pong") && lines[1].contains("rejected"));
    }
}
