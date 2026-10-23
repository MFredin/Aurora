//! Pure parsing helpers. Kept platform-independent so they are unit-tested on any OS.

/// Parses `powercfg /getactivescheme`, e.g.
/// "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)".
pub fn active_scheme(output: &str) -> Option<(String, String)> {
    let after = output.split("GUID:").nth(1)?.trim();
    let guid: String = after
        .chars()
        .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    if guid.len() != 36 {
        return None;
    }
    let name = after
        .split('(')
        .nth(1)
        .and_then(|s| s.split(')').next())
        .unwrap_or("")
        .trim()
        .to_string();
    Some((guid.to_lowercase(), name))
}

/// Power scheme `FriendlyName` registry values look like
/// "@%SystemRoot%\system32\powrprof.dll,-14,Balanced". The text after the last comma is the
/// fallback name; custom plans store a plain name.
pub fn power_friendly_name(raw: &str) -> String {
    let raw = raw.trim_end_matches('\0').trim();
    if raw.starts_with('@') {
        raw.rsplit(',').next().unwrap_or(raw).trim().to_string()
    } else {
        raw.to_string()
    }
}

/// StartupApproved values: first byte 0x02/0x06 = enabled, 0x03/0x07 = disabled.
/// A missing value means the item is enabled.
pub fn startup_enabled(approved_bytes: Option<&[u8]>) -> bool {
    match approved_bytes.and_then(|b| b.first()) {
        Some(b) => b & 1 == 0,
        None => true,
    }
}

/// The 12-byte value Task Manager writes to disable an item: 0x03 followed by a FILETIME.
pub fn disabled_bytes(filetime: u64) -> Vec<u8> {
    let mut b = vec![0x03, 0, 0, 0];
    b.extend_from_slice(&filetime.to_le_bytes());
    b
}

/// "Microsoft.BingNews_4.55.0_x64__8wekyb3d8bbwe" -> "Microsoft.BingNews".
pub fn appx_name(repository_key: &str) -> &str {
    repository_key.split('_').next().unwrap_or(repository_key)
}

/// Drops display adapters that aren't real GPUs (basic/remote/virtual drivers) and duplicates.
pub fn clean_gpus(names: Vec<String>) -> Vec<String> {
    const SKIP: [&str; 8] = [
        "microsoft basic display",
        "microsoft remote display",
        "remote desktop",
        "virtual display",
        "parsec",
        "meta virtual monitor",
        "spacedesk",
        "citrix",
    ];
    let mut out: Vec<String> = vec![];
    for n in names {
        let n = n.trim().to_string();
        let l = n.to_lowercase();
        if n.is_empty() || SKIP.iter().any(|s| l.contains(s)) || out.contains(&n) {
            continue;
        }
        out.push(n);
    }
    out
}

/// Highest refresh rate among `modes` (width, height, hz) at the current resolution.
pub fn best_refresh(width: u32, height: u32, modes: &[(u32, u32, u32)]) -> Option<u32> {
    modes
        .iter()
        .filter(|(w, h, _)| *w == width && *h == height)
        .map(|(_, _, hz)| *hz)
        .filter(|hz| *hz > 1)
        .max()
}

/// "YYYY-MM-DD" (UTC) for a Unix time.
pub fn civil_date(unix_secs: u64) -> String {
    // Civil from days (Howard Hinnant's algorithm).
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Driver dates in the registry look like "1-14-2026" (M-D-YYYY). Returns "2026-01-14".
pub fn driver_date(raw: &str) -> Option<String> {
    let mut it = raw.trim().split('-').map(|p| p.parse::<u32>().ok());
    let (m, d, y) = (it.next()??, it.next()??, it.next()??);
    ((1..=12).contains(&m) && (1..=31).contains(&d) && y > 1990)
        .then(|| format!("{y:04}-{m:02}-{d:02}"))
}

/// Bytes to GB the way Explorer shows them (1024^3).
pub fn gib(bytes: u64) -> u64 {
    bytes / (1024 * 1024 * 1024)
}

/// "C:\\" -> "C:", leaves other mount points alone.
pub fn mount_label(mount: &str) -> String {
    let t = mount.trim_end_matches(['\\', '/']);
    if t.is_empty() {
        mount.to_string()
    } else {
        t.to_string()
    }
}
