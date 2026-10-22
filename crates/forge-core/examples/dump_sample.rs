//! Prints the engine's findings for the sample PC as JSON. Used as the UI's dev-mode mock data,
//! so the mock can never drift from the real model.
use forge_core::{bloat::BloatDb, engine::scan, rules::default_rules, snapshot::Snapshot};
fn main() {
    let snap = Snapshot::sample_gaming_pc();
    let f = scan(&snap, &BloatDb::embedded(), &default_rules(), |_| {});
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "machine": snap, "findings": f }))
            .unwrap()
    );
}
