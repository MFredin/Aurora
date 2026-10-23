use forge_core::bloat::{BloatDb, Kind, Verdict};
use forge_core::engine::scan;
use forge_core::ipc::{is_dx_prefs, validate};
use forge_core::journal::{self, Entry, Outcome, Prior, Record, Status};
use forge_core::model::*;
use forge_core::power;
use forge_core::rules::{annotate, days_between, default_rules, friendly_package};
use forge_core::snapshot::{Display, Snapshot};

fn run(s: &Snapshot) -> Vec<Finding> {
    scan(s, &BloatDb::embedded(), &default_rules(), |_| {})
}
fn ids(f: &[Finding]) -> Vec<&str> {
    f.iter().map(|f| f.id.as_str()).collect()
}
fn db() -> BloatDb {
    BloatDb::embedded()
}

// ---------- findings on the sample PC ----------

#[test]
fn sample_pc_produces_expected_findings() {
    let f = run(&Snapshot::sample_gaming_pc());
    let ids = ids(&f);
    for want in [
        "startup:Discord",
        "startup:Spotify",
        "startup:AdobeGCInvoker-1.0",
        "power:mode",
        "ram:xmp",
        "storage:low:C:",
        "appx:king.com.CandyCrushSaga",
        "appx:Microsoft.BingNews",
        "display:refresh:Display 1",
        "gaming:dvr",
        "gaming:mouse-accel",
        "bloat:suggestions",
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
fn keep_wins_over_an_earlier_broad_match() {
    // "spotify" (caution) appears in the command, but the item is an NVIDIA component (keep).
    let db = db();
    let e = db
        .lookup(Kind::Startup, "NvBackend C:\\nvidia\\spotify-overlay.exe")
        .unwrap();
    assert_eq!(e.verdict, Verdict::Keep);
}

#[test]
fn results_are_sorted_best_first_and_scored() {
    let f = run(&Snapshot::sample_gaming_pc());
    assert!(f.windows(2).all(|w| w[0].score >= w[1].score));
    assert!(
        f.iter().all(|f| f.score > 0.0),
        "every finding needs a non-zero score"
    );
}

#[test]
fn advice_only_findings_do_not_sink_to_the_bottom() {
    let f = run(&Snapshot::sample_gaming_pc());
    let pos = |id: &str| f.iter().position(|f| f.id == id).unwrap();
    // A 60 Hz monitor that supports 165 Hz matters more than a preinstalled app.
    assert!(pos("display:refresh:Display 1") < pos("appx:Microsoft.BingNews"));
    assert!(pos("ram:xmp") < pos("appx:Microsoft.BingNews"));
}

// ---------- power plan ----------

/// The sample PC uses Windows 11 power modes; these tests cover a PC with plans only.
fn plan_only_pc() -> Snapshot {
    let mut s = Snapshot::sample_gaming_pc();
    s.power_mode = None;
    s
}

#[test]
fn power_plan_finding_on_a_pc_without_power_modes() {
    assert!(ids(&run(&plan_only_pc())).contains(&"power:plan"));
    assert!(!ids(&run(&Snapshot::sample_gaming_pc())).contains(&"power:plan"));
}

#[test]
fn no_high_performance_plan_on_laptop_battery() {
    let mut s = plan_only_pc();
    s.is_laptop = true;
    s.on_battery = true;
    assert!(!ids(&run(&s)).contains(&"power:plan"));
}

#[test]
fn no_high_performance_plan_on_dual_ccd_x3d() {
    let mut s = plan_only_pc();
    s.cpu_name = "AMD Ryzen 9 7950X3D 16-Core Processor".into();
    assert!(!ids(&run(&s)).contains(&"power:plan"));
    assert!(power::is_dual_ccd_x3d("AMD Ryzen 9 9900X3D"));
    assert!(!power::is_dual_ccd_x3d("AMD Ryzen 7 7800X3D"));
    assert!(!power::is_dual_ccd_x3d("AMD Ryzen 7 5800X3D"));
    assert!(!power::is_dual_ccd_x3d("Intel Core i9-14900K"));
}

#[test]
fn no_power_fix_when_high_performance_plan_is_missing() {
    let mut s = plan_only_pc();
    s.power_plans.retain(|p| p.guid != power::HIGH_PERFORMANCE);
    assert!(!ids(&run(&s)).contains(&"power:plan"));
}

#[test]
fn no_power_finding_when_not_on_balanced() {
    let mut s = plan_only_pc();
    s.active_power_plan = s.power_plans.get(1).cloned();
    assert!(!ids(&run(&s)).contains(&"power:plan"));
}

// ---------- memory ----------

#[test]
fn hardware_findings_offer_advice_not_a_fix() {
    let f = run(&Snapshot::sample_gaming_pc());
    for id in ["ram:xmp", "display:refresh:Display 1"] {
        let x = f.iter().find(|f| f.id == id).unwrap();
        assert!(x.fix.is_none() && x.advice.is_some(), "{id}");
    }
}

#[test]
fn xmp_does_not_fire_when_running_at_rated_speed() {
    let mut s = Snapshot::sample_gaming_pc();
    s.ram_configured_mts = Some(6000);
    assert!(!ids(&run(&s)).contains(&"ram:xmp"));
}

#[test]
fn single_module_on_desktop_is_flagged_but_not_on_laptop() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(!ids(&run(&s)).contains(&"ram:single-channel"));
    s.ram_modules = Some(1);
    assert!(ids(&run(&s)).contains(&"ram:single-channel"));
    s.is_laptop = true;
    assert!(!ids(&run(&s)).contains(&"ram:single-channel"));
}

// ---------- storage ----------

#[test]
fn storage_rules_fire_only_for_the_system_drive() {
    let mut s = Snapshot::sample_gaming_pc();
    let f = run(&s);
    assert!(
        !ids(&f).iter().any(|i| i.starts_with("storage:hdd")),
        "D: is not the system drive"
    );
    s.disks[0].free_gb = 400;
    assert!(!ids(&run(&s)).contains(&"storage:low:C:"));
    s.disks[0].kind = forge_core::snapshot::DiskKind::Hdd;
    assert!(ids(&run(&s)).contains(&"storage:hdd:C:"));
}

// ---------- gaming ----------

#[test]
fn refresh_rate_ignores_rounding_differences() {
    let mut s = Snapshot::sample_gaming_pc();
    s.displays = vec![Display {
        name: "Display 1".into(),
        width: 1920,
        height: 1080,
        current_hz: 59,
        max_hz: 60,
    }];
    assert!(!ids(&run(&s)).iter().any(|i| i.starts_with("display:")));
}

#[test]
fn game_dvr_fix_is_allowlisted_and_reversible() {
    let f = run(&Snapshot::sample_gaming_pc());
    let dvr = f.iter().find(|f| f.id == "gaming:dvr").unwrap();
    let plan = dvr.fix.as_ref().unwrap();
    assert!(plan.reversible);
    for step in &plan.steps {
        validate(step, &db()).unwrap();
    }
    let mut s = Snapshot::sample_gaming_pc();
    s.game_dvr_background = Some(false);
    assert!(!ids(&run(&s)).contains(&"gaming:dvr"));
}

#[test]
fn every_offered_fix_passes_the_helper_allowlist() {
    for f in run(&Snapshot::sample_gaming_pc()) {
        for step in f.fix.iter().flat_map(|p| &p.steps) {
            validate(step, &db()).unwrap_or_else(|e| panic!("{}: {e}", f.id));
        }
    }
}

// ---------- helper allowlist ----------

#[test]
fn helper_allowlist_rejects_dangerous_steps() {
    let db = db();
    let bad_reg = FixStep::SetRegistryDword {
        hive: Hive::Hklm,
        key: r"SOFTWARE\Microsoft\Windows Defender".into(),
        name: "DisableAntiSpyware".into(),
        value: 1,
    };
    assert!(validate(&bad_reg, &db).is_err());
    assert!(
        validate(
            &FixStep::DeleteRegistryValue {
                hive: Hive::Hklm,
                key: r"SYSTEM\CurrentControlSet\Services\WinDefend".into(),
                name: "Start".into(),
            },
            &db
        )
        .is_err()
    );
    let wrong_value = FixStep::SetRegistryDword {
        hive: Hive::Hkcu,
        key: r"Software\Microsoft\Windows\CurrentVersion\GameDVR".into(),
        name: "HistoricalCaptureEnabled".into(),
        value: 7,
    };
    assert!(validate(&wrong_value, &db).is_err());
    assert!(
        validate(
            &FixStep::SetPowerPlan {
                guid: "not-a-guid".into()
            },
            &db
        )
        .is_err()
    );
    assert!(
        validate(
            &FixStep::SetPowerPlan {
                guid: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c /x".into()
            },
            &db
        )
        .is_err()
    );
    assert!(
        validate(
            &FixStep::DisableStartupItem {
                name: "x & del /q *".into(),
                location: StartupLocation::HkcuRun
            },
            &db
        )
        .is_err()
    );
    assert!(
        validate(
            &FixStep::SetPowerPlan {
                guid: power::HIGH_PERFORMANCE.into()
            },
            &db
        )
        .is_ok()
    );
}

#[test]
fn helper_refuses_to_disable_protected_startup_items() {
    for name in ["SecurityHealth", "RtkAudUService", "NvBackend"] {
        let step = FixStep::DisableStartupItem {
            name: name.into(),
            location: StartupLocation::HklmRun,
        };
        assert!(validate(&step, &db()).is_err(), "{name} must be refused");
    }
}

#[test]
fn restore_step_bytes_are_bounded() {
    let mk = |prior| FixStep::RestoreStartupItem {
        name: "Discord".into(),
        location: StartupLocation::HkcuRun,
        prior,
    };
    assert!(validate(&mk(None), &db()).is_ok());
    assert!(validate(&mk(Some(vec![2; 12])), &db()).is_ok());
    assert!(validate(&mk(Some(vec![])), &db()).is_err());
    assert!(validate(&mk(Some(vec![2; 65])), &db()).is_err());
}

// ---------- journal ----------

fn entry(id: u64, step: FixStep, prior: Prior) -> Entry {
    Entry {
        id,
        unix_ts: 0,
        finding_id: "f".into(),
        title: "t".into(),
        step,
        prior,
        undoes: None,
        group: None,
    }
}

#[test]
fn journal_roundtrip_on_disk() {
    let dir = std::env::temp_dir().join(format!("forge-test-{}", std::process::id()));
    let path = dir.join("journal.jsonl");
    let e = entry(
        1,
        FixStep::SetPowerPlan {
            guid: power::HIGH_PERFORMANCE.into(),
        },
        Prior::PowerPlan {
            guid: power::BALANCED.into(),
        },
    );
    journal::append(&path, &Record::Intent(e.clone())).unwrap();
    journal::append(
        &path,
        &Record::Outcome {
            id: 1,
            outcome: Outcome::Applied,
        },
    )
    .unwrap();
    // A torn last line must not break reading.
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, b"{\"record\":\"int"))
        .unwrap();
    let recs = journal::read_records(&path).unwrap();
    assert_eq!(recs.len(), 2);
    let h = journal::history(&recs);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].status, Status::Applied);
    assert!(h[0].undoable);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn inverses() {
    let startup = entry(
        1,
        FixStep::DisableStartupItem {
            name: "Discord".into(),
            location: StartupLocation::HkcuRun,
        },
        Prior::StartupApproved {
            bytes: Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        },
    );
    assert_eq!(
        journal::inverse(&startup),
        Some(FixStep::RestoreStartupItem {
            name: "Discord".into(),
            location: StartupLocation::HkcuRun,
            prior: Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        })
    );
    let key = r"Software\Microsoft\Windows\CurrentVersion\GameDVR".to_string();
    let set = |existed| {
        entry(
            2,
            FixStep::SetRegistryDword {
                hive: Hive::Hkcu,
                key: key.clone(),
                name: "HistoricalCaptureEnabled".into(),
                value: 0,
            },
            Prior::Dword { existed, value: 1 },
        )
    };
    assert!(matches!(
        journal::inverse(&set(true)),
        Some(FixStep::SetRegistryDword { value: 1, .. })
    ));
    assert!(matches!(
        journal::inverse(&set(false)),
        Some(FixStep::DeleteRegistryValue { .. })
    ));
    for e in [startup, set(true), set(false)] {
        validate(&journal::inverse(&e).unwrap(), &db()).unwrap();
    }
}

#[test]
fn interrupted_failed_and_undone_states() {
    let pp = |id| {
        entry(
            id,
            FixStep::SetPowerPlan {
                guid: power::HIGH_PERFORMANCE.into(),
            },
            Prior::PowerPlan {
                guid: power::BALANCED.into(),
            },
        )
    };
    let mut undo = pp(4);
    undo.undoes = Some(1);
    let recs = vec![
        Record::Intent(pp(1)),
        Record::Outcome {
            id: 1,
            outcome: Outcome::Applied,
        },
        Record::Intent(pp(2)),
        Record::Outcome {
            id: 2,
            outcome: Outcome::Failed { reason: "x".into() },
        },
        Record::Intent(pp(3)), // no outcome: helper died
        Record::Intent(undo),
        Record::Outcome {
            id: 4,
            outcome: Outcome::Applied,
        },
    ];
    let h = journal::history(&recs);
    let st = |id| {
        h.iter()
            .find(|i| i.id == id)
            .map(|i| (i.status, i.undoable))
    };
    assert_eq!(h.len(), 3, "undo entries are not listed on their own");
    assert_eq!(h[0].id, 3, "newest first");
    assert_eq!(st(1), Some((Status::Undone, false)));
    assert_eq!(st(2), Some((Status::Failed, false)));
    assert_eq!(st(3), Some((Status::Interrupted, true)));
    assert!(journal::undoable(&recs, 1).is_err());
    assert!(journal::undoable(&recs, 2).is_err());
    assert!(journal::undoable(&recs, 3).is_ok());
    assert!(journal::next_id(&recs, 0) > 4);
}

// ---------- bloat DB quality gates ----------

#[test]
fn bloat_patterns_are_specific_and_unambiguous() {
    let db = db();
    for e in &db.entries {
        for p in &e.patterns {
            assert!(p.len() >= 4, "{}: pattern {p:?} is too short", e.id);
            assert_eq!(p, &p.to_lowercase(), "{}: patterns are lowercase", e.id);
        }
    }
    // No pattern may be matched by entries with different non-keep verdicts.
    for a in &db.entries {
        for p in &a.patterns {
            for b in db
                .entries
                .iter()
                .filter(|b| b.kind == a.kind && b.id != a.id)
            {
                if b.patterns.iter().any(|q| p.contains(q.as_str())) {
                    assert!(
                        a.verdict == Verdict::Keep
                            || b.verdict == Verdict::Keep
                            || a.verdict == b.verdict,
                        "{} and {} disagree on {p:?}",
                        a.id,
                        b.id
                    );
                }
            }
        }
    }
}

#[test]
fn friendly_package_names() {
    assert_eq!(
        friendly_package("king.com.CandyCrushSaga"),
        "Candy Crush Saga"
    );
    assert_eq!(friendly_package("Microsoft.BingNews"), "Bing News");
}

// ---------- actions: app removal and settings ----------

#[test]
fn preinstalled_apps_get_a_removal_fix_marked_irreversible() {
    let f = run(&Snapshot::sample_gaming_pc());
    let candy = f
        .iter()
        .find(|f| f.id == "appx:king.com.CandyCrushSaga")
        .unwrap();
    let plan = candy.fix.as_ref().unwrap();
    assert!(!plan.reversible);
    assert_eq!(
        candy.risk,
        Risk::Caution,
        "never part of the low-risk batch"
    );
    assert_eq!(candy.title, "Candy Crush Saga is preinstalled");
}

#[test]
fn app_removal_is_limited_to_apps_the_db_marks_safe() {
    let db = db();
    let rm = |n: &str| FixStep::RemoveAppx {
        package_name: n.into(),
    };
    assert!(validate(&rm("king.com.CandyCrushSaga"), &db).is_ok());
    assert!(validate(&rm("Microsoft.BingNews"), &db).is_ok());
    for bad in [
        "Microsoft.WindowsStore",         // keep
        "Microsoft.XboxIdentityProvider", // keep
        "Microsoft.WindowsCalculator",    // unknown to the DB
        "Microsoft.BingNews; del",        // not a package name
        "",
    ] {
        assert!(validate(&rm(bad), &db).is_err(), "{bad} must be refused");
    }
}

#[test]
fn removal_has_no_inverse_and_shows_as_not_undoable() {
    let e = entry(
        9,
        FixStep::RemoveAppx {
            package_name: "Microsoft.BingNews".into(),
        },
        Prior::Package {
            full_name: "Microsoft.BingNews_4.55.0.0_x64__8wekyb3d8bbwe".into(),
        },
    );
    assert!(journal::inverse(&e).is_none());
    let h = journal::history(&[
        Record::Intent(e),
        Record::Outcome {
            id: 9,
            outcome: Outcome::Applied,
        },
    ]);
    assert_eq!(h[0].status, Status::Applied);
    assert!(!h[0].undoable);
}

#[test]
fn game_mode_fires_only_when_explicitly_off() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(!ids(&run(&s)).contains(&"gaming:game-mode"));
    s.game_mode = None;
    assert!(!ids(&run(&s)).contains(&"gaming:game-mode"));
    s.game_mode = Some(false);
    let f = run(&s);
    let gm = f.iter().find(|f| f.id == "gaming:game-mode").unwrap();
    for step in &gm.fix.as_ref().unwrap().steps {
        validate(step, &db()).unwrap();
    }
}

#[test]
fn mouse_acceleration_fix_sets_all_three_values_and_undoes_to_text() {
    let mut s = Snapshot::sample_gaming_pc();
    let f = run(&s);
    let m = f.iter().find(|f| f.id == "gaming:mouse-accel").unwrap();
    let steps = &m.fix.as_ref().unwrap().steps;
    assert_eq!(steps.len(), 3);
    for step in steps {
        validate(step, &db()).unwrap();
    }
    let e = entry(
        3,
        steps[1].clone(),
        Prior::Text {
            existed: true,
            value: "6".into(),
        },
    );
    let inv = journal::inverse(&e).unwrap();
    assert!(matches!(&inv, FixStep::SetRegistryString { value, .. } if value == "6"));
    validate(&inv, &db()).unwrap();
    s.mouse_acceleration = Some(false);
    assert!(!ids(&run(&s)).contains(&"gaming:mouse-accel"));
}

#[test]
fn text_values_must_be_short_digits() {
    let mk = |v: &str| FixStep::SetRegistryString {
        hive: Hive::Hkcu,
        key: r"Control Panel\Mouse".into(),
        name: "MouseThreshold1".into(),
        value: v.into(),
    };
    assert!(validate(&mk("0"), &db()).is_ok());
    assert!(validate(&mk("10"), &db()).is_ok());
    for bad in ["", "1; x", "-1", "99999", "abc"] {
        assert!(validate(&mk(bad), &db()).is_err(), "{bad:?}");
    }
    let other_key = FixStep::SetRegistryString {
        hive: Hive::Hkcu,
        key: r"Control Panel\Desktop".into(),
        name: "MouseThreshold1".into(),
        value: "0".into(),
    };
    assert!(validate(&other_key, &db()).is_err());
}

#[test]
fn suggestions_fix_turns_off_only_the_switches_that_are_on() {
    let mut s = Snapshot::sample_gaming_pc();
    let f = run(&s);
    let sg = f.iter().find(|f| f.id == "bloat:suggestions").unwrap();
    assert_eq!(sg.title, "Windows installs promoted apps on its own");
    let steps = &sg.fix.as_ref().unwrap().steps;
    assert_eq!(steps.len(), 2);
    for step in steps {
        validate(step, &db()).unwrap();
    }
    s.windows_suggestions_on.clear();
    assert!(!ids(&run(&s)).contains(&"bloat:suggestions"));
}

// ---------- more optimizations ----------

fn has(s: &Snapshot, id: &str) -> bool {
    ids(&run(s)).contains(&id)
}

fn fix_validates(s: &Snapshot, id: &str) {
    let f = run(s);
    let x = f
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("{id} missing"));
    for step in &x.fix.as_ref().unwrap().steps {
        validate(step, &db()).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

#[test]
fn power_mode_rule() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(has(&s, "power:mode"));
    fix_validates(&s, "power:mode");
    s.power_mode = Some(power::MODE_BEST_PERFORMANCE.into());
    assert!(!has(&s, "power:mode"));
    s.power_mode = Some(power::MODE_BALANCED.into());
    s.is_laptop = true;
    s.on_battery = true;
    assert!(!has(&s, "power:mode"), "never on battery");
    s.on_battery = false;
    assert!(has(&s, "power:mode"), "laptop plugged in is fine");
    s.cpu_name = "AMD Ryzen 9 7950X3D".into();
    assert!(!has(&s, "power:mode"), "dual-CCD X3D keeps Balanced");
}

#[test]
fn hags_rule() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(has(&s, "gaming:hags"));
    fix_validates(&s, "gaming:hags");
    for v in [Some(true), None] {
        s.hags = v;
        assert!(!has(&s, "gaming:hags"));
    }
}

#[test]
fn windowed_games_rule_keeps_other_prefs() {
    let mut s = Snapshot::sample_gaming_pc();
    let f = run(&s);
    let w = f.iter().find(|f| f.id == "gaming:windowed").unwrap();
    match &w.fix.as_ref().unwrap().steps[0] {
        FixStep::SetRegistryString { value, .. } => {
            assert_eq!(value, "VRROptimizeEnable=0;SwapEffectUpgradeEnable=1;")
        }
        other => panic!("{other:?}"),
    }
    fix_validates(&s, "gaming:windowed");
    s.dx_user_prefs = Some("SwapEffectUpgradeEnable=1;".into());
    assert!(!has(&s, "gaming:windowed"));
    s.dx_user_prefs = None;
    assert!(!has(&s, "gaming:windowed"));
    s.dx_user_prefs = Some(String::new());
    assert!(has(&s, "gaming:windowed"));
}

#[test]
fn dx_prefs_validation() {
    assert!(is_dx_prefs("SwapEffectUpgradeEnable=1;"));
    assert!(is_dx_prefs("A=1;B=0;"));
    assert!(is_dx_prefs(""));
    for bad in ["A=1;B", "A=x;", "A B=1;", "A=1;\"evil\"", "=1;"] {
        assert!(!is_dx_prefs(bad), "{bad:?}");
    }
}

#[test]
fn storage_sense_rule() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(has(&s, "storage:sense"));
    fix_validates(&s, "storage:sense");
    s.storage_sense = Some(true);
    assert!(!has(&s, "storage:sense"));
}

#[test]
fn restart_rules() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(has(&s, "system:reboot-pending"));
    assert!(!has(&s, "system:uptime"), "pending restart covers it");
    s.reboot_pending = false;
    assert!(has(&s, "system:uptime"));
    s.uptime_hours = 30;
    assert!(!has(&s, "system:uptime"));
}

#[test]
fn gpu_driver_age() {
    assert_eq!(days_between("2026-01-14", "2026-10-23"), Some(282));
    assert_eq!(days_between("2024-02-28", "2024-03-01"), Some(2));
    assert_eq!(days_between("bad", "2026-10-23"), None);
    let mut s = Snapshot::sample_gaming_pc();
    let f = run(&s);
    let d = f.iter().find(|f| f.id.starts_with("driver:gpu:")).unwrap();
    assert!(d.fix.is_none() && d.advice.as_ref().unwrap().contains("nvidia.com"));
    s.gpu_drivers[0].date = Some("2026-09-01".into());
    assert!(!run(&s).iter().any(|f| f.id.starts_with("driver:gpu:")));
    s.gpu_drivers[0].date = Some("2020-01-01".into());
    s.gpu_drivers[0].name = "Microsoft Basic Display Adapter".into();
    assert!(
        !run(&s).iter().any(|f| f.id.starts_with("driver:gpu:")),
        "unknown vendor: no link to give"
    );
}

#[test]
fn memory_pressure_rule_lists_top_users() {
    let mut s = Snapshot::sample_gaming_pc();
    assert!(!has(&s, "memory:pressure"));
    s.ram_used_mb = 30_000;
    let f = run(&s);
    let m = f.iter().find(|f| f.id == "memory:pressure").unwrap();
    assert!(m.evidence.iter().any(|e| e.starts_with("chrome")));
}

#[test]
fn startup_items_are_annotated() {
    let mut s = Snapshot::sample_gaming_pc();
    annotate(&mut s, &db());
    let get = |n: &str| s.startup.iter().find(|i| i.name == n).unwrap();
    assert!(get("SecurityHealth").protected);
    assert!(get("RtkAudUService").protected);
    assert!(!get("Discord").protected && get("Discord").note.is_some());
}

#[test]
fn grouped_steps_share_a_group_in_history() {
    let mut a = entry(
        1,
        FixStep::SetPowerPlan {
            guid: power::HIGH_PERFORMANCE.into(),
        },
        Prior::PowerPlan {
            guid: power::BALANCED.into(),
        },
    );
    a.group = Some(77);
    let mut b = a.clone();
    b.id = 2;
    let h = journal::history(&[Record::Intent(a), Record::Intent(b)]);
    assert!(h.iter().all(|i| i.group == Some(77)));
}

#[test]
fn power_mode_inverse() {
    let e = entry(
        4,
        FixStep::SetPowerMode {
            guid: power::MODE_BEST_PERFORMANCE.into(),
        },
        Prior::PowerMode {
            guid: power::MODE_BALANCED.into(),
        },
    );
    let inv = journal::inverse(&e).unwrap();
    assert_eq!(
        inv,
        FixStep::SetPowerMode {
            guid: power::MODE_BALANCED.into()
        }
    );
    validate(&inv, &db()).unwrap();
}
