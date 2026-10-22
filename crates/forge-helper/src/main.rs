//! forge-helper: the only elevated process. It exposes a tiny, allowlisted set of operations.
//!
//! Production transport: a named pipe whose ACL admits only the installing user's SID. The app
//! launches this binary via a UAC prompt on first Apply and it exits after ~30 s idle, so it costs
//! nothing when not in use. For the scaffold the transport is newline-delimited JSON on stdio,
//! which keeps the protocol and safety logic testable on any OS.
use forge_core::ipc::{Request, Response, validate};
use forge_core::journal::{self, Entry, Prior};
use forge_core::model::FixStep;
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// Applies one validated step and reports what it replaced. Platform-specific.
trait Executor {
    fn apply(&mut self, step: &FixStep) -> Result<Prior, String>;
}

fn handle(req: Request, exec: &mut dyn Executor, journal_path: &std::path::Path) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Apply { finding_id, step } => {
            if let Err(reason) = validate(&step) {
                return Response::Rejected { reason };
            }
            let prior = match exec.apply(&step) {
                Ok(p) => p,
                Err(reason) => return Response::Rejected { reason },
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let entry = Entry {
                id: now.as_nanos() as u64,
                unix_ts: now.as_secs(),
                finding_id,
                step,
                prior,
            };
            // The change already happened; if journaling fails the UI must say so, never hide it.
            match journal::append(journal_path, &entry) {
                Ok(()) => Response::Applied {
                    journal_id: entry.id,
                },
                Err(e) => Response::Rejected {
                    reason: format!("applied but could not write undo journal: {e}"),
                },
            }
        }
    }
}

#[cfg(windows)]
mod exec_win {
    use super::*;
    use forge_core::model::StartupLocation;
    use std::os::windows::process::CommandExt;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    pub struct WinExec;
    impl Executor for WinExec {
        fn apply(&mut self, step: &FixStep) -> Result<Prior, String> {
            match step {
                FixStep::SetPowerPlan { guid } => {
                    let out = powercfg(&["/getactivescheme"])?;
                    let (prev, _) = forge_scan::parse::active_scheme(&out)
                        .ok_or("could not read current power plan")?;
                    powercfg(&["/setactive", guid])?;
                    Ok(Prior::PowerPlan { guid: prev })
                }
                FixStep::DisableStartupItem { name, location } => {
                    let hive = match location {
                        StartupLocation::HkcuRun => HKEY_CURRENT_USER,
                        StartupLocation::HklmRun => HKEY_LOCAL_MACHINE,
                        _ => return Err("startup folder items are not supported yet".into()),
                    };
                    let (key, _) = RegKey::predef(hive)
                        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run")
                        .map_err(|e| e.to_string())?;
                    let mut bytes = vec![0u8; 12];
                    bytes[0] = 0x03; // disabled, same as Task Manager
                    key.set_raw_value(
                        name,
                        &RegValue {
                            bytes: bytes.into(),
                            vtype: REG_BINARY,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(Prior::StartupWasEnabled)
                }
                FixStep::RemoveAppx { .. } => {
                    Err("removing packaged apps is not implemented yet".into())
                }
                FixStep::SetRegistryDword { .. } => {
                    Err("registry writes are not implemented yet".into())
                }
            }
        }
    }

    fn powercfg(args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new("powercfg")
            .args(args)
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }
}

struct Unsupported;
impl Executor for Unsupported {
    fn apply(&mut self, _: &FixStep) -> Result<Prior, String> {
        Err("the helper only applies changes on Windows".into())
    }
}

fn journal_path() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Forge").join("journal.jsonl")
}

fn main() {
    #[cfg(windows)]
    let mut exec = exec_win::WinExec;
    #[cfg(not(windows))]
    let mut exec = Unsupported;
    let path = journal_path();
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle(req, &mut exec, &path),
            Err(e) => Response::Rejected {
                reason: format!("bad request: {e}"),
            },
        };
        let mut out = stdout.lock();
        let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::model::Hive;

    struct Fake {
        calls: usize,
    }
    impl Executor for Fake {
        fn apply(&mut self, _: &FixStep) -> Result<Prior, String> {
            self.calls += 1;
            Ok(Prior::StartupWasEnabled)
        }
    }

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!(
            "forge-helper-{}-{}.jsonl",
            std::process::id(),
            std::thread::current().name().unwrap_or("t")
        ))
    }

    #[test]
    fn rejected_steps_never_reach_the_executor_or_journal() {
        let (mut ex, p) = (Fake { calls: 0 }, tmp());
        let bad = FixStep::SetRegistryDword {
            hive: Hive::Hklm,
            key: "SOFTWARE\\Policies\\Microsoft\\Windows Defender".into(),
            name: "DisableAntiSpyware".into(),
            value: 1,
        };
        let r = handle(
            Request::Apply {
                finding_id: "x".into(),
                step: bad,
            },
            &mut ex,
            &p,
        );
        assert!(matches!(r, Response::Rejected { .. }));
        assert_eq!(ex.calls, 0);
        assert!(!p.exists());
    }

    #[test]
    fn allowed_step_is_applied_and_journaled() {
        let (mut ex, p) = (Fake { calls: 0 }, tmp());
        let step = FixStep::SetPowerPlan {
            guid: forge_core::power::HIGH_PERFORMANCE.into(),
        };
        let r = handle(
            Request::Apply {
                finding_id: "power:plan".into(),
                step,
            },
            &mut ex,
            &p,
        );
        assert!(matches!(r, Response::Applied { .. }));
        assert_eq!(ex.calls, 1);
        assert_eq!(journal::read_all(&p).unwrap().len(), 1);
        std::fs::remove_file(p).ok();
    }
}
