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

/// StartupApproved\Run values: first byte 0x02/0x06 = enabled, 0x03/0x07 = disabled.
/// A missing value means the item is enabled.
pub fn startup_enabled(approved_bytes: Option<&[u8]>) -> bool {
    match approved_bytes.and_then(|b| b.first()) {
        Some(b) => b & 1 == 0,
        None => true,
    }
}

/// "Microsoft.BingNews_4.55.0_x64__8wekyb3d8bbwe" -> "Microsoft.BingNews".
pub fn appx_name(repository_key: &str) -> &str {
    repository_key.split('_').next().unwrap_or(repository_key)
}
