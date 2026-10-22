//! Prints the engine's findings for the sample PC as JSON. Used as the UI's dev-mode mock data,
//! so the mock can never drift from the real model. Findings are in streaming order (rule order);
//! `order` is the ranking the app receives when the scan finishes.
use forge_core::{bloat::BloatDb, engine::scan, rules::default_rules, snapshot::Snapshot};
fn main() {
    let snap = Snapshot::sample_gaming_pc();
    let mut streamed = vec![];
    let ranked = scan(&snap, &BloatDb::embedded(), &default_rules(), |f| {
        streamed.push(f.clone())
    });
    let order: Vec<&str> = ranked.iter().map(|f| f.id.as_str()).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({ "machine": snap, "findings": streamed, "order": order })
        )
        .expect("sample serializes")
    );
}
