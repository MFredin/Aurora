//! Temporary-file cleanup. Runs unelevated, on the signed-in user's own temp folder only: deleting
//! the user's files needs no admin rights, and never deleting from an elevated process avoids the
//! classic "elevated delete through a planted link" attack. Links and junctions are never followed.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Files must be at least this old to be counted or removed, so open work files are left alone.
pub const MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Stop walking after this many entries, so a huge temp folder can't stall a scan.
const MAX_ENTRIES: usize = 300_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Tally {
    pub files: u64,
    pub bytes: u64,
}

/// The user's temp folder (%TEMP%).
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn old_enough(meta: &std::fs::Metadata, now: SystemTime, min_age: Duration) -> bool {
    meta.modified()
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .is_some_and(|age| age >= min_age)
}

/// Walks `dir` without following links, calling `on_file` for each old-enough regular file.
fn walk(dir: &Path, min_age: Duration, mut on_file: impl FnMut(&Path, u64)) {
    let now = SystemTime::now();
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return;
            }
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if is_link(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() && old_enough(&meta, now, min_age) {
                on_file(&path, meta.len());
            }
        }
    }
}

/// What a cleanup would remove: old files in `dir`.
pub fn measure(dir: &Path, min_age: Duration) -> Tally {
    let mut t = Tally::default();
    walk(dir, min_age, |_, len| {
        t.files += 1;
        t.bytes += len;
    });
    t
}

/// Deletes old files in `dir`. Files in use simply fail to delete and are skipped.
/// Returns what was actually removed.
pub fn clean(dir: &Path, min_age: Duration) -> Tally {
    let mut doomed = vec![];
    walk(dir, min_age, |p, len| doomed.push((p.to_path_buf(), len)));
    let mut t = Tally::default();
    for (p, len) in doomed {
        // Re-check right before deleting: still a plain file, not a link swapped in meanwhile.
        let still_plain = std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file() && !is_link(&m));
        if still_plain && std::fs::remove_file(&p).is_ok() {
            t.files += 1;
            t.bytes += len;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_cleans_only_old_files() {
        let root = std::env::temp_dir().join(format!("forge-clean-{}", std::process::id()));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.tmp"), vec![0u8; 100]).unwrap();
        std::fs::write(sub.join("b.tmp"), vec![0u8; 50]).unwrap();
        // Everything was just written, so nothing is a day old.
        assert_eq!(measure(&root, MIN_AGE), Tally::default());
        // With no minimum age, both files count and are removed; folders stay.
        assert_eq!(
            measure(&root, Duration::ZERO),
            Tally {
                files: 2,
                bytes: 150
            }
        );
        assert_eq!(
            clean(&root, Duration::ZERO),
            Tally {
                files: 2,
                bytes: 150
            }
        );
        assert!(sub.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_links() {
        let root = std::env::temp_dir().join(format!("forge-link-{}", std::process::id()));
        let outside = std::env::temp_dir().join(format!("forge-outside-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"precious").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        assert_eq!(clean(&root, Duration::ZERO).files, 0);
        assert!(outside.join("keep.txt").exists());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }
}
