#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Thin shell. All logic lives in forge-core / forge-scan so it stays testable without a window.
//! Security property: the UI sends only a finding id or a journal id. The fix plan is looked up
//! here from the last scan, and undo steps are worked out by the helper from its own journal,
//! so a compromised webview cannot ask the helper to run an arbitrary step.
mod helper;

use forge_core::{
    bloat::BloatDb,
    engine,
    ipc::{Request, Response},
    journal,
    model::Finding,
    paths, rules,
    snapshot::Snapshot,
};
use forge_scan::{BackgroundGuard, SelfMeter, SelfStats};
use helper::{CallError, HelperConn};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{State, ipc::Channel};

/// The helper exits after 5 minutes idle; reconnect a little before that.
const HELPER_STALE: Duration = Duration::from_secs(290);

#[derive(Serialize, Clone)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
enum ScanEvent {
    Machine(Snapshot),
    Finding(Finding),
    /// Finding ids, best first.
    Done {
        order: Vec<String>,
    },
}

type Slot = Option<(HelperConn, Instant)>;

#[derive(Default)]
struct AppState {
    findings: Arc<Mutex<HashMap<String, Finding>>>,
    helper: Arc<Mutex<Slot>>,
    restore_point_asked: Arc<AtomicBool>,
    meter: Mutex<SelfMeter>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sends one request, (re)starting the helper as needed. Retries once only when the request
/// provably never reached a helper.
fn call(slot: &mut Slot, req: &Request, timeout: Duration) -> Result<Response, String> {
    if slot
        .as_ref()
        .is_some_and(|(_, last)| last.elapsed() > HELPER_STALE)
    {
        *slot = None;
    }
    for attempt in 0..2 {
        if slot.is_none() {
            *slot = Some((HelperConn::start()?, Instant::now()));
        }
        let Some((conn, last)) = slot.as_mut() else {
            continue;
        };
        match conn.call(req, timeout) {
            Ok(r) => {
                *last = Instant::now();
                return Ok(r);
            }
            Err(CallError::NotSent) if attempt == 0 => *slot = None,
            Err(CallError::NotSent) => {
                *slot = None;
                return Err("Could not reach Forge's helper. Nothing was changed.".into());
            }
            Err(CallError::Lost) => {
                *slot = None;
                return Err("Forge's helper stopped before confirming the change. Open History to see whether it was applied.".into());
            }
            Err(CallError::Timeout) => {
                *slot = None;
                return Err("Forge's helper didn't answer in time. Open History to see whether the change was applied.".into());
            }
        }
    }
    Err("Could not reach Forge's helper. Nothing was changed.".into())
}

#[tauri::command]
async fn scan(state: State<'_, AppState>, on_event: Channel<ScanEvent>) -> Result<(), String> {
    let found = tauri::async_runtime::spawn_blocking(move || {
        let _bg = BackgroundGuard::enter(); // low CPU/IO priority so a scan never disturbs a running game
        let started = Instant::now();
        let snap = forge_scan::collect();
        let _ = on_event.send(ScanEvent::Machine(snap.clone()));
        let all = engine::scan(&snap, &BloatDb::embedded(), &rules::default_rules(), |f| {
            let _ = on_event.send(ScanEvent::Finding(f.clone()));
        });
        let _ = on_event.send(ScanEvent::Done {
            order: all.iter().map(|f| f.id.clone()).collect(),
        });
        if cfg!(debug_assertions) {
            eprintln!("scan took {:?}", started.elapsed());
        }
        all
    })
    .await
    .map_err(|_| "The scan stopped unexpectedly. Nothing was changed. Try again.".to_string())?;
    *lock(&state.findings) = found.into_iter().map(|f| (f.id.clone(), f)).collect();
    Ok(())
}

#[derive(Serialize, Default)]
struct ApplyResult {
    /// Shown once, after the first change of a session.
    restore_point: Option<String>,
    warning: Option<String>,
}

#[tauri::command]
async fn apply_fix(state: State<'_, AppState>, finding_id: String) -> Result<ApplyResult, String> {
    let finding = lock(&state.findings)
        .get(&finding_id)
        .cloned()
        .ok_or("Scan again, then apply the fix.")?;
    let plan = finding
        .fix
        .clone()
        .ok_or("That finding has no automatic fix. Follow the advice shown instead.")?;
    let helper = state.helper.clone();
    let asked = state.restore_point_asked.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = lock(&helper);
        let mut out = ApplyResult::default();
        if !asked.swap(true, Ordering::SeqCst) {
            match call(
                &mut slot,
                &Request::CreateRestorePoint,
                Duration::from_secs(240),
            ) {
                Ok(Response::RestorePoint { message, .. }) => out.restore_point = Some(message),
                Ok(_) => {}
                Err(e) => {
                    asked.store(false, Ordering::SeqCst);
                    return Err(e);
                }
            }
        }
        for step in plan.steps {
            let req = Request::Apply {
                finding_id: finding.id.clone(),
                title: finding.title.clone(),
                step,
            };
            match call(&mut slot, &req, Duration::from_secs(30))? {
                Response::Applied { .. } => {}
                Response::AppliedWithWarning { warning, .. } => out.warning = Some(warning),
                Response::Rejected { reason } => {
                    return Err(format!("Forge refused this change: {reason}"));
                }
                Response::Failed { reason } => {
                    return Err(format!(
                        "Windows refused the change, so nothing was changed. {reason}"
                    ));
                }
                _ => return Err("Forge's helper gave an unexpected answer.".into()),
            }
        }
        Ok(out)
    })
    .await
    .map_err(|_| "Applying stopped unexpectedly. Open History to check.".to_string())?
}

#[tauri::command]
async fn undo(state: State<'_, AppState>, journal_id: u64) -> Result<(), String> {
    let helper = state.helper.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = lock(&helper);
        match call(
            &mut slot,
            &Request::Undo { journal_id },
            Duration::from_secs(30),
        )? {
            Response::Undone { .. } => Ok(()),
            Response::Rejected { reason } | Response::Failed { reason } => Err(reason),
            _ => Err("Forge's helper gave an unexpected answer.".into()),
        }
    })
    .await
    .map_err(|_| "Undo stopped unexpectedly. Open History to check.".to_string())?
}

#[tauri::command]
fn history() -> Result<Vec<journal::Item>, String> {
    let path = paths::journal_path(paths::dev_mode());
    journal::read_records(&path)
        .map(|r| journal::history(&r))
        .map_err(|e| format!("Could not read History ({}): {e}", path.display()))
}

#[tauri::command]
fn self_stats(state: State<'_, AppState>) -> SelfStats {
    lock(&state.meter).sample()
}

/// Set at the top of `main`, for the cold-start measurement.
static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Called once by the UI after its first frame. Only when the measurement script launched the app
/// with `FORGE_PERF_LOG=<file>` does this write the milliseconds since process start to that file.
#[tauri::command]
fn ui_ready() {
    let (Some(path), Some(start)) = (std::env::var_os("FORGE_PERF_LOG"), STARTED.get()) else {
        return;
    };
    let _ = std::fs::write(path, start.elapsed().as_millis().to_string());
}

fn main() {
    STARTED.get_or_init(Instant::now);
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            scan, apply_fix, undo, history, self_stats, ui_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
