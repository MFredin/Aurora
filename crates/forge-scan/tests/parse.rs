use forge_scan::{parse, smbios};

#[test]
fn parses_powercfg_output() {
    let out = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)\r\n";
    assert_eq!(
        parse::active_scheme(out),
        Some((
            "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
            "Balanced".into()
        ))
    );
    assert_eq!(parse::active_scheme("garbage"), None);
}

#[test]
fn startup_approved_flags() {
    assert!(parse::startup_enabled(None));
    assert!(parse::startup_enabled(Some(&[0x02, 0, 0])));
    assert!(parse::startup_enabled(Some(&[0x06, 0, 0])));
    assert!(!parse::startup_enabled(Some(&[0x03, 0, 0])));
    assert!(!parse::startup_enabled(Some(&[0x07, 0, 0])));
}

#[test]
fn appx_names() {
    assert_eq!(
        parse::appx_name("Microsoft.BingNews_4.55.0.0_x64__8wekyb3d8bbwe"),
        "Microsoft.BingNews"
    );
}

fn dimm(speed: u16, configured: u16) -> Vec<u8> {
    let mut s = vec![0u8; 0x28];
    s[0] = 17;
    s[1] = 0x28;
    s[0x15..0x17].copy_from_slice(&speed.to_le_bytes());
    s[0x20..0x22].copy_from_slice(&configured.to_le_bytes());
    s.extend_from_slice(&[0, 0]); // empty string set
    s
}

#[test]
fn smbios_reads_rated_and_configured_speed() {
    let mut t = dimm(6000, 4800);
    t.extend(dimm(6000, 4800));
    t.extend_from_slice(&[127, 4, 0, 0, 0, 0]);
    assert_eq!(smbios::memory_speeds(&t), (Some(4800), Some(6000)));
}

#[test]
fn smbios_ignores_empty_slots_and_truncated_data() {
    let mut t = dimm(0, 0);
    t.extend(dimm(3200, 3200));
    assert_eq!(smbios::memory_speeds(&t), (Some(3200), Some(3200)));
    assert_eq!(smbios::memory_speeds(&t[..10]), (None, None));
}

#[test]
fn collect_works_on_this_machine() {
    let s = forge_scan::collect();
    assert!(s.ram_total_mb > 0 && s.cpu_threads > 0);
    let mut m = forge_scan::SelfMeter::new();
    assert!(m.sample().rss_mb > 0.0);
}
