//! Where Forge keeps its files. Shared by the app and the helper so they always agree.
use std::path::PathBuf;

/// Set by the app in development to run the helper unelevated over stdio (see README).
pub const DEV_ENV: &str = "FORGE_HELPER_STDIO";

pub fn dev_mode() -> bool {
    std::env::var_os(DEV_ENV).is_some_and(|v| v == "1")
}

/// The undo journal. Release builds keep it in `%ProgramData%\Forge`, a folder only
/// administrators can write (the helper creates it with that ACL and refuses one it didn't
/// create). Dev mode uses the user's local app data because the helper is not elevated.
pub fn journal_path(dev: bool) -> PathBuf {
    let base = if dev {
        std::env::var_os("LOCALAPPDATA")
    } else {
        std::env::var_os("ProgramData")
    };
    base.map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Forge")
        .join(if dev {
            "dev-journal.jsonl"
        } else {
            "journal.jsonl"
        })
}
