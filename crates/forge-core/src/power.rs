/// Built-in Windows power plan GUIDs.
pub const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const POWER_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
pub const KNOWN_PLANS: [&str; 3] = [BALANCED, HIGH_PERFORMANCE, POWER_SAVER];

/// Windows 11 power modes (overlays on the Balanced plan), as set in Settings > Power.
pub const MODE_BEST_EFFICIENCY: &str = "961cc777-2547-4f9d-8174-7d86181b8a7a";
pub const MODE_BALANCED: &str = "00000000-0000-0000-0000-000000000000";
pub const MODE_BEST_PERFORMANCE: &str = "ded574b5-45a0-4f42-8737-46345c09c238";

pub fn mode_name(guid: &str) -> &'static str {
    match guid {
        MODE_BEST_EFFICIENCY => "Best power efficiency",
        MODE_BALANCED => "Balanced",
        MODE_BEST_PERFORMANCE => "Best performance",
        _ => "another mode",
    }
}

/// True for a lowercase or uppercase GUID in 8-4-4-4-12 form.
pub fn is_guid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// English name of a built-in plan, for messages. The UI shows the localized name from the scan.
pub fn name(guid: &str) -> &'static str {
    match guid {
        BALANCED => "Balanced",
        HIGH_PERFORMANCE => "High performance",
        POWER_SAVER => "Power saver",
        _ => "another plan",
    }
}

/// Ryzen X3D parts with two CCDs (7900X3D, 7950X3D, 9900X3D, 9950X3D). AMD's V-Cache driver
/// parks the non-cache CCD during games and relies on the Balanced plan to do it, so High
/// performance can make these CPUs slower in games.
pub fn is_dual_ccd_x3d(cpu_name: &str) -> bool {
    cpu_name.split_whitespace().any(|tok| {
        let t = tok.to_ascii_uppercase();
        let Some(num) = t.strip_suffix("X3D") else {
            return false;
        };
        num.parse::<u32>()
            .is_ok_and(|n| n >= 1000 && n % 1000 >= 900)
    })
}
