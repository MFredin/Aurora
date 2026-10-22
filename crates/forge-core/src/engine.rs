use crate::bloat::BloatDb;
use crate::model::Finding;
use crate::rules::Rule;
use crate::snapshot::Snapshot;

/// Runs every rule and calls `on_finding` as results appear, so the UI can stream them.
/// Returns all findings sorted best-first.
pub fn scan(
    s: &Snapshot,
    db: &BloatDb,
    rules: &[Box<dyn Rule>],
    mut on_finding: impl FnMut(&Finding),
) -> Vec<Finding> {
    let mut all = Vec::new();
    for rule in rules {
        for f in rule.evaluate(s, db) {
            on_finding(&f);
            all.push(f);
        }
    }
    all.sort_by(|a, b| b.score().total_cmp(&a.score()));
    all
}
