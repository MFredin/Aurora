#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Thin shell. All logic lives in forge-core / forge-scan so it stays testable without a window.
//! Security property: the UI sends only a finding id. The fix plan is looked up here from the
//! last scan, so a compromised webview cannot ask the helper to run an arbitrary step.
use forge_core::{bloat::BloatDb, engine, ipc::{Request, Response}, journal, model::Finding, rules, snapshot::Snapshot};
use forge_scan::{BackgroundGuard, SelfMeter, SelfStats};
use serde::Serialize;
use std::{collections::HashMap, io::{BufRead, BufReader, Write}, process::{Child, ChildStdin, ChildStdout, Command, Stdio}, sync::Mutex};
use tauri::{ipc::Channel, State};

#[derive(Serialize, Clone)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
enum ScanEvent { Machine(Snapshot), Finding(Finding), Done { count: usize } }

#[derive(Default)]
struct AppState {
    findings: Mutex<HashMap<String, Finding>>,
    meter: Mutex<SelfMeter>,
    helper: Mutex<Option<Helper>>,
}

/// Dev transport: a child process over stdio. Production swaps this for an elevated named-pipe client.
struct Helper { _child: Child, stdin: ChildStdin, stdout: BufReader<ChildStdout> }

impl Helper {
    fn spawn() -> Result<Self, String> {
        // FORGE_HELPER overrides the path in dev, where the app and helper build into different target dirs.
        let path = match std::env::var_os("FORGE_HELPER") {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let name = if cfg!(windows) { "forge-helper.exe" } else { "forge-helper" };
                exe.parent().ok_or("no exe dir")?.join(name)
            }
        };
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()
            .map_err(|e| format!("could not start the helper: {e}"))?;
        let stdin = child.stdin.take().ok_or("no helper stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("no helper stdout")?);
        Ok(Self { _child: child, stdin, stdout })
    }
    fn call(&mut self, req: &Request) -> Result<Response, String> {
        writeln!(self.stdin, "{}", serde_json::to_string(req).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.stdout.read_line(&mut line).map_err(|e| e.to_string())?;
        serde_json::from_str(&line).map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn scan(state: State<'_, AppState>, on_event: Channel<ScanEvent>) -> Result<(), String> {
    let found = tauri::async_runtime::spawn_blocking(move || {
        let _bg = BackgroundGuard::enter(); // low CPU/IO priority so a scan never disturbs a running game
        let snap = forge_scan::collect();
        let _ = on_event.send(ScanEvent::Machine(snap.clone()));
        let all = engine::scan(&snap, &BloatDb::embedded(), &rules::default_rules(), |f| { let _ = on_event.send(ScanEvent::Finding(f.clone())); });
        let _ = on_event.send(ScanEvent::Done { count: all.len() });
        all
    }).await.map_err(|e| e.to_string())?;
    *state.findings.lock().unwrap() = found.into_iter().map(|f| (f.id.clone(), f)).collect();
    Ok(())
}

#[tauri::command]
async fn apply_fix(state: State<'_, AppState>, finding_id: String) -> Result<(), String> {
    let plan = state.findings.lock().unwrap().get(&finding_id).and_then(|f| f.fix.clone())
        .ok_or("That finding has no automatic fix. Follow the advice shown instead.")?;
    let mut guard = state.helper.lock().unwrap();
    if guard.is_none() { *guard = Some(Helper::spawn()?); }
    let helper = guard.as_mut().unwrap();
    for step in plan.steps {
        match helper.call(&Request::Apply { finding_id: finding_id.clone(), step })? {
            Response::Applied { .. } => {}
            Response::Rejected { reason } => return Err(reason),
            Response::Pong => return Err("unexpected helper reply".into()),
        }
    }
    Ok(())
}

#[tauri::command]
fn self_stats(state: State<'_, AppState>) -> SelfStats { state.meter.lock().unwrap().sample() }

#[tauri::command]
fn history() -> Vec<journal::Entry> {
    let base = std::env::var_os("ProgramData").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    journal::read_all(&base.join("Forge").join("journal.jsonl")).unwrap_or_default()
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![scan, apply_fix, self_stats, history])
        .run(tauri::generate_context!())
        .expect("error while running Forge");
}
