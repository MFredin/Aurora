//! Minimal SMBIOS parser: DIMM speeds and count (Type 17) and chassis type (Type 3).
//! Input is the table data without the 8-byte RawSMBIOSData header.

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Info {
    /// Highest configured speed across populated DIMMs, MT/s.
    pub configured_mts: Option<u32>,
    /// Highest rated speed across populated DIMMs, MT/s.
    pub rated_mts: Option<u32>,
    /// Populated memory devices.
    pub modules: Option<u32>,
    /// True for laptop-style chassis (portable, laptop, notebook, convertible...).
    pub portable: Option<bool>,
}

/// SMBIOS chassis types that are portable (DSP0134, 7.4.1).
const PORTABLE: [u8; 8] = [8, 9, 10, 11, 14, 30, 31, 32];

pub fn parse(table: &[u8]) -> Info {
    let mut info = Info::default();
    let mut modules = 0u32;
    let mut saw_memory = false;
    let mut i = 0usize;
    while i + 4 <= table.len() {
        let (ty, len) = (table[i], table[i + 1] as usize);
        if len < 4 || i + len > table.len() {
            break;
        }
        if ty == 127 {
            break; // end-of-table
        }
        let s = &table[i..i + len];
        let rd16 = |off: usize| {
            (off + 2 <= s.len()).then(|| u16::from_le_bytes([s[off], s[off + 1]]) as u32)
        };
        let rd32 = |off: usize| {
            (off + 4 <= s.len())
                .then(|| u32::from_le_bytes([s[off], s[off + 1], s[off + 2], s[off + 3]]))
        };
        if ty == 3 && len > 5 && info.portable.is_none() {
            info.portable = Some(PORTABLE.contains(&(s[5] & 0x7F)));
        }
        if ty == 17 && len >= 0x22 {
            saw_memory = true;
            // Size 0 means the slot is empty.
            if rd16(0x0C).is_some_and(|sz| sz != 0) {
                modules += 1;
            }
            // 0xFFFF means "see the 32-bit extended field" (SMBIOS 3.3+, offsets 0x54/0x58).
            let speed = |base: usize, ext: usize| match rd16(base) {
                Some(0xFFFF) => rd32(ext),
                v => v,
            };
            let valid = |v: &u32| *v != 0 && *v != 0xFFFF;
            if let Some(v) = speed(0x15, 0x54).filter(valid) {
                info.rated_mts = Some(info.rated_mts.map_or(v, |r| r.max(v)));
            }
            if let Some(v) = speed(0x20, 0x58).filter(valid) {
                info.configured_mts = Some(info.configured_mts.map_or(v, |c| c.max(v)));
            }
        }
        // Skip the formatted area, then the string set (terminated by a double NUL).
        let mut j = i + len;
        while j + 1 < table.len() && !(table[j] == 0 && table[j + 1] == 0) {
            j += 1;
        }
        i = j + 2;
    }
    if saw_memory {
        info.modules = Some(modules);
    }
    info
}

/// Returns (highest configured MT/s, highest rated MT/s) across populated DIMMs.
pub fn memory_speeds(table: &[u8]) -> (Option<u32>, Option<u32>) {
    let i = parse(table);
    (i.configured_mts, i.rated_mts)
}
