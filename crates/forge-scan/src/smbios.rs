//! Minimal SMBIOS parser: reads DIMM rated vs configured speed from Type 17 structures.
//! Input is the table data without the 8-byte RawSMBIOSData header.

/// Returns (highest configured MT/s, highest rated MT/s) across populated DIMMs.
pub fn memory_speeds(table: &[u8]) -> (Option<u32>, Option<u32>) {
    let (mut configured, mut rated) = (None::<u32>, None::<u32>);
    let mut i = 0usize;
    while i + 4 <= table.len() {
        let (ty, len) = (table[i], table[i + 1] as usize);
        if len < 4 || i + len > table.len() {
            break;
        }
        if ty == 127 {
            break;
        } // end-of-table
        if ty == 17 && len >= 0x22 {
            let rd = |off: usize| u16::from_le_bytes([table[i + off], table[i + off + 1]]) as u32;
            let (speed, cfg) = (rd(0x15), rd(0x20));
            let valid = |v: u32| v != 0 && v != 0xFFFF;
            if valid(speed) {
                rated = Some(rated.map_or(speed, |r| r.max(speed)));
            }
            if valid(cfg) {
                configured = Some(configured.map_or(cfg, |c| c.max(cfg)));
            }
        }
        // Skip the formatted area, then the string set (terminated by a double NUL).
        let mut j = i + len;
        while j + 1 < table.len() && !(table[j] == 0 && table[j + 1] == 0) {
            j += 1;
        }
        i = j + 2;
    }
    (configured, rated)
}
