//! Prints what the collectors see on this machine, plus the findings, as JSON. Use it to compare
//! against Task Manager, Device Manager and CPU-Z (docs/M1-TASKS.md, step 2).
use forge_core::{bloat::BloatDb, engine::scan, rules::default_rules};

fn main() {
    let started = std::time::Instant::now();
    let snap = forge_scan::collect();
    let collected = started.elapsed();
    let findings = scan(&snap, &BloatDb::embedded(), &default_rules(), |_| {});
    let out = serde_json::json!({
        "collect_ms": collected.as_millis(),
        "machine": snap,
        "findings": findings,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&out).expect("snapshot serializes")
    );
}
