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
fn power_friendly_names() {
    assert_eq!(
        parse::power_friendly_name("@%SystemRoot%\\system32\\powrprof.dll,-14,Balanced"),
        "Balanced"
    );
    assert_eq!(
        parse::power_friendly_name("AMD Ryzen Balanced\0"),
        "AMD Ryzen Balanced"
    );
    // Localized fallback text survives.
    assert_eq!(
        parse::power_friendly_name("@%SystemRoot%\\system32\\powrprof.dll,-14,Ausbalanciert"),
        "Ausbalanciert"
    );
}

#[test]
fn startup_approved_flags() {
    assert!(parse::startup_enabled(None));
    assert!(parse::startup_enabled(Some(&[0x02, 0, 0])));
    assert!(parse::startup_enabled(Some(&[0x06, 0, 0])));
    assert!(!parse::startup_enabled(Some(&[0x03, 0, 0])));
    assert!(!parse::startup_enabled(Some(&[0x07, 0, 0])));
    let b = parse::disabled_bytes(0x0102_0304_0506_0708);
    assert_eq!(b.len(), 12);
    assert!(!parse::startup_enabled(Some(&b)));
}

#[test]
fn appx_names() {
    assert_eq!(
        parse::appx_name("Microsoft.BingNews_4.55.0.0_x64__8wekyb3d8bbwe"),
        "Microsoft.BingNews"
    );
}

#[test]
fn gpu_list_drops_virtual_adapters_and_duplicates() {
    let raw = vec![
        "NVIDIA GeForce RTX 4070".to_string(),
        "Microsoft Basic Display Adapter".into(),
        "NVIDIA GeForce RTX 4070".into(),
        "Parsec Virtual Display Adapter".into(),
        "AMD Radeon(TM) Graphics".into(),
        " ".into(),
    ];
    assert_eq!(
        parse::clean_gpus(raw),
        vec!["NVIDIA GeForce RTX 4070", "AMD Radeon(TM) Graphics"]
    );
}

#[test]
fn best_refresh_is_per_resolution() {
    let modes = [
        (2560, 1440, 60),
        (2560, 1440, 144),
        (2560, 1440, 165),
        (1920, 1080, 240),
        (2560, 1440, 1),
    ];
    assert_eq!(parse::best_refresh(2560, 1440, &modes), Some(165));
    assert_eq!(parse::best_refresh(3840, 2160, &modes), None);
}

#[test]
fn sizes_and_mounts_match_explorer() {
    assert_eq!(parse::gib(1_000_204_886_016), 931); // a "1 TB" drive
    assert_eq!(parse::mount_label("C:\\"), "C:");
    assert_eq!(parse::mount_label("/"), "/");
}

fn dimm(size: u16, speed: u16, configured: u16) -> Vec<u8> {
    let mut s = vec![0u8; 0x5C];
    s[0] = 17;
    s[1] = 0x5C;
    s[0x0C..0x0E].copy_from_slice(&size.to_le_bytes());
    s[0x15..0x17].copy_from_slice(&speed.to_le_bytes());
    s[0x20..0x22].copy_from_slice(&configured.to_le_bytes());
    s.extend_from_slice(&[0, 0]); // empty string set
    s
}

fn chassis(kind: u8) -> Vec<u8> {
    let mut s = vec![3u8, 0x0D, 0, 0, 0, kind, 0, 0, 0, 0, 0, 0, 0];
    s.extend_from_slice(&[0, 0]);
    s
}

#[test]
fn smbios_reads_rated_and_configured_speed() {
    let mut t = dimm(16384, 6000, 4800);
    t.extend(dimm(16384, 6000, 4800));
    t.extend_from_slice(&[127, 4, 0, 0, 0, 0]);
    assert_eq!(smbios::memory_speeds(&t), (Some(4800), Some(6000)));
    assert_eq!(smbios::parse(&t).modules, Some(2));
}

#[test]
fn smbios_ignores_empty_slots_and_truncated_data() {
    let mut t = dimm(0, 0, 0);
    t.extend(dimm(8192, 3200, 3200));
    let i = smbios::parse(&t);
    assert_eq!(
        (i.configured_mts, i.rated_mts, i.modules),
        (Some(3200), Some(3200), Some(1))
    );
    assert_eq!(smbios::memory_speeds(&t[..10]), (None, None));
}

#[test]
fn smbios_extended_speed_fields() {
    let mut d = dimm(16384, 0xFFFF, 0xFFFF);
    d[0x54..0x58].copy_from_slice(&8400u32.to_le_bytes());
    d[0x58..0x5C].copy_from_slice(&8000u32.to_le_bytes());
    assert_eq!(smbios::memory_speeds(&d), (Some(8000), Some(8400)));
}

#[test]
fn smbios_chassis_type() {
    assert_eq!(smbios::parse(&chassis(10)).portable, Some(true)); // notebook
    assert_eq!(smbios::parse(&chassis(3)).portable, Some(false)); // desktop
    assert_eq!(smbios::parse(&chassis(0x80 | 9)).portable, Some(true)); // lock bit set
    assert_eq!(smbios::parse(&[]).portable, None);
}

#[test]
fn process_tree_includes_grandchildren_only() {
    use sysinfo::Pid;
    let p = Pid::from_u32;
    let procs = vec![
        (p(1), None),
        (p(10), Some(p(1))),  // app
        (p(11), Some(p(10))), // msedgewebview2 browser
        (p(12), Some(p(11))), // renderer
        (p(20), Some(p(1))),  // unrelated
    ];
    let set = forge_scan::descendants(p(10), procs.into_iter());
    assert_eq!(set.len(), 3);
    assert!(!set.contains(&p(20)));
}

#[test]
fn collect_works_on_this_machine() {
    let s = forge_scan::collect();
    assert!(s.ram_total_mb > 0 && s.cpu_threads > 0);
    let mut m = forge_scan::SelfMeter::new();
    let st = m.sample();
    assert!(st.memory_mb > 0.0 && st.processes >= 1);
    assert!(
        m.sample().processes >= 1,
        "second sample refreshes only the known tree"
    );
}
