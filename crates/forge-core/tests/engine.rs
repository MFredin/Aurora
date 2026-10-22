use forge_core::bloat::BloatDb;
use forge_core::engine::scan;
use forge_core::ipc::validate;
use forge_core::journal::{self, Entry, Prior};
use forge_core::model::*;
use forge_core::rules::default_rules;
use forge_core::snapshot::Snapshot;

fn run(s: &Snapshot) -> Vec<Finding> {
    scan(s, &BloatDb::embedded(), &default_rules(), |_| {})
}

#[test]
fn sample_pc_produces_expected_findings() {
    let f = run(&Snapshot::sample_gaming_pc());
    let ids: Vec<&str> = f.iter().map(|f| f.id.as_str()).collect();
    for want in [
        "startup:Discord",
        "startup:Spotify",
        "startup:AdobeGCInvoker-1.0",
        "power:plan",
        "ram:xmp",
        "storage:low:C:",
        "appx:king.com.CandyCrushSaga",
    ] {
        assert!(ids.contains(&want), "missing {want}; got {ids:?}");
    }
}

#[test]
fn keep_listed_items_are_never_flagged() {
    let f = run(&Snapshot::sample_gaming_pc());
    assert!(!f.iter().any(|f| f.id.contains("SecurityHealth")
        || f.id.contains("RtkAud")
        || f.id.contains("XboxIdentity")));
}

#[test]
fn results_are_sorted_best_first() {
    let f = run(&Snapshot::sample_gaming_pc());
    assert!(f.windows(2).all(|w| w[0].score() >= w[1].score()));
}

#[test]
fn no_high_performance_plan_on_laptop_battery() {
    let mut s = Snapshot::sample_gaming_pc();
    s.is_laptop = true;
    s.on_battery = true;
    assert!(!run(&s).iter().any(|f| f.id == "power:plan"));
}

#[test]
fn hardware_findings_offer_advice_not_a_fix() {
    let f = run(&Snapshot::sample_gaming_pc());
    let xmp = f.iter().find(|f| f.id == "ram:xmp").unwrap();
    assert!(xmp.fix.is_none() && xmp.advice.is_some());
}

#[test]
fn helper_allowlist_rejects_dangerous_steps() {
    let bad_reg = FixStep::SetRegistryDword {
        hive: Hive::Hklm,
        key: r"SOFTWARE\Microsoft\Windows Defender".into(),
        name: "DisableAntiSpyware".into(),
        value: 1,
    };
    assert!(validate(&bad_reg).is_err());
    assert!(
        validate(&FixStep::SetPowerPlan {
            guid: "not-a-guid".into()
        })
        .is_err()
    );
    assert!(
        validate(&FixStep::RemoveAppx {
            package_name: "x & del /q *".into()
        })
        .is_err()
    );
    assert!(
        validate(&FixStep::SetPowerPlan {
            guid: forge_core::power::HIGH_PERFORMANCE.into()
        })
        .is_ok()
    );
}

#[test]
fn journal_roundtrip_and_inverse() {
    let dir = std::env::temp_dir().join(format!("forge-test-{}", std::process::id()));
    let path = dir.join("journal.jsonl");
    let e = Entry {
        id: 1,
        unix_ts: 0,
        finding_id: "power:plan".into(),
        step: FixStep::SetPowerPlan {
            guid: forge_core::power::HIGH_PERFORMANCE.into(),
        },
        prior: Prior::PowerPlan {
            guid: forge_core::power::BALANCED.into(),
        },
    };
    journal::append(&path, &e).unwrap();
    let back = journal::read_all(&path).unwrap();
    assert_eq!(back, vec![e.clone()]);
    assert_eq!(
        journal::inverse(&e),
        Some(FixStep::SetPowerPlan {
            guid: forge_core::power::BALANCED.into()
        })
    );
    let appx = Entry {
        step: FixStep::RemoveAppx {
            package_name: "x".into(),
        },
        prior: Prior::Irreversible,
        ..e
    };
    assert!(!journal::is_reversible(&appx) && journal::inverse(&appx).is_none());
    std::fs::remove_dir_all(dir).ok();
}
