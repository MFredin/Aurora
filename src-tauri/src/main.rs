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
    model::{Finding, FixStep},
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
    Machine(Box<Snapshot>),
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
    /// The last scan, so startup switches act only on items the scan found.
    snapshot: Arc<Mutex<Option<Snapshot>>>,
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
        let db = BloatDb::embedded();
        let mut snap = forge_scan::collect();
        rules::annotate(&mut snap, &db);
        let _ = on_event.send(ScanEvent::Machine(Box::new(snap.clone())));
        let all = engine::scan(&snap, &db, &rules::default_rules(), |f| {
            let _ = on_event.send(ScanEvent::Finding(f.clone()));
        });
        let _ = on_event.send(ScanEvent::Done {
            order: all.iter().map(|f| f.id.clone()).collect(),
        });
        if cfg!(debug_assertions) {
            eprintln!("scan took {:?}", started.elapsed());
        }
        (all, snap)
    })
    .await
    .map_err(|_| "The scan stopped unexpectedly. Nothing was changed. Try again.".to_string())?;
    let (found, snap) = found;
    *lock(&state.findings) = found.into_iter().map(|f| (f.id.clone(), f)).collect();
    *lock(&state.snapshot) = Some(snap);
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
    run_steps(&state, finding.id, finding.title, plan.steps).await
}

/// Sends a fix's steps to the helper as one group, asking for a restore point before the first
/// change of the session.
async fn run_steps(
    state: &State<'_, AppState>,
    finding_id: String,
    title: String,
    steps: Vec<FixStep>,
) -> Result<ApplyResult, String> {
    let helper = state.helper.clone();
    let asked = state.restore_point_asked.clone();
    let group = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
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
        for step in steps {
            let req = Request::Apply {
                finding_id: finding_id.clone(),
                title: title.clone(),
                step,
                group: Some(group),
            };
            match call(&mut slot, &req, Duration::from_secs(120))? {
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

/// Turns one startup item from the last scan on or off. The UI names the item by its position in
/// that scan; the step is built here from what the scan found.
#[tauri::command]
async fn set_startup(
    state: State<'_, AppState>,
    index: usize,
    enabled: bool,
) -> Result<ApplyResult, String> {
    let item = lock(&state.snapshot)
        .as_ref()
        .and_then(|s| s.startup.get(index).cloned())
        .ok_or("Scan again, then change startup apps.")?;
    if item.protected && !enabled {
        return Err(format!(
            "{} is a Windows or driver component, so Forge leaves it on.",
            item.name
        ));
    }
    let step = if enabled {
        // The value Task Manager writes for "Enabled".
        FixStep::RestoreStartupItem {
            name: item.name.clone(),
            location: item.location,
            prior: Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        }
    } else {
        FixStep::DisableStartupItem {
            name: item.name.clone(),
            location: item.location,
        }
    };
    let title = format!("{} at startup", item.name);
    let out = run_steps(
        &state,
        format!("startup-app:{}", item.name),
        title,
        vec![step],
    )
    .await?;
    if let Some(s) = lock(&state.snapshot).as_mut() {
        if let Some(i) = s.startup.get_mut(index) {
            i.enabled = enabled;
        }
    }
    Ok(out)
}

/// Deletes temp files older than a day from the user's own temp folder. Runs in this
/// (unelevated) process on purpose: see `forge_scan::cleanup`.
#[tauri::command]
async fn clean_temp() -> Result<forge_scan::cleanup::Tally, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _bg = BackgroundGuard::enter();
        forge_scan::cleanup::clean(
            &forge_scan::cleanup::temp_dir(),
            forge_scan::cleanup::MIN_AGE,
        )
    })
    .await
    .map_err(|_| "Cleanup stopped unexpectedly.".to_string())
}

#[tauri::command]
async fn empty_recycle_bin() -> Result<(), String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(forge_scan::empty_recycle_bin)
            .await
            .map_err(|_| "Emptying the Recycle Bin stopped unexpectedly.".to_string())?
    }
    #[cfg(not(windows))]
    Err("The Recycle Bin is only on Windows.".into())
}

/// Undoes one change, given the journal ids of all its steps. Newest step first, so a fix with
/// several steps unwinds in reverse order.
#[tauri::command]
async fn undo(state: State<'_, AppState>, journal_ids: Vec<u64>) -> Result<(), String> {
    let helper = state.helper.clone();
    let mut ids = journal_ids;
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.dedup();
    if ids.is_empty() || ids.len() > 32 {
        return Err("Nothing to undo.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = lock(&helper);
        for journal_id in ids {
            match call(
                &mut slot,
                &Request::Undo { journal_id },
                Duration::from_secs(30),
            )? {
                Response::Undone { .. } => {}
                Response::Rejected { reason } | Response::Failed { reason } => return Err(reason),
                _ => return Err("Forge's helper gave an unexpected answer.".into()),
            }
        }
        Ok(())
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

/// Opens a vendor page from advice text in the default browser. Only the exact URLs in
/// `rules::HELP_LINKS` are allowed, so the webview can't use this to open anything else.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !forge_core::rules::HELP_LINKS.contains(&url.as_str()) {
        return Err("Forge only opens the driver pages it links to.".into());
    }
    open_in_browser(&url)
}

#[cfg(windows)]
fn open_in_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (verb, file) = (wide("open"), wide(url));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value greater than 32 on success.
    if r as isize > 32 {
        Ok(())
    } else {
        Err("Windows couldn't open your browser. Copy the address instead.".into())
    }
}

#[cfg(not(windows))]
fn open_in_browser(_url: &str) -> Result<(), String> {
    Err("Opening links is only supported on Windows.".into())
}

#[tauri::command]
fn self_stats(state: State<'_, AppState>) -> SelfStats {
    lock(&state.meter).sample()
}

/// WebView2 keeps running scripts and timers while its window is minimized unless the host says
/// otherwise. Marking it invisible fires `visibilitychange` (the self-meter stops polling) and
/// lets WebView2 throttle the page, so Forge costs nothing while minimized.
fn set_webview_visible(window: &tauri::Window, visible: bool) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        if let Some(wv) = window.app_handle().get_webview_window(window.label()) {
            let _ = wv.with_webview(move |p| unsafe {
                let _ = p.controller().SetIsVisible(visible);
            });
        }
    }
    #[cfg(not(windows))]
    let _ = (window, visible);
}

/// Opens at a size where Overview fits without scrolling (1320 x 860), shrunk to 92% of the
/// screen's work area on smaller displays, and centred.
fn fit_to_screen(w: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = w.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area().size.to_logical::<f64>(scale);
    let width = 1320f64.min(area.width * 0.92).max(1024f64.min(area.width));
    let height = 860f64.min(area.height * 0.92).max(680f64.min(area.height));
    let _ = w.set_size(tauri::LogicalSize::new(width, height));
    let _ = w.center();
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
        .setup(|app| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                fit_to_screen(&w);
                let _ = w.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Resized(_) = event {
                let minimized = window.is_minimized().unwrap_or(false);
                set_webview_visible(window, !minimized);
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            apply_fix,
            set_startup,
            clean_temp,
            empty_recycle_bin,
            undo,
            history,
            self_stats,
            open_link,
            ui_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
