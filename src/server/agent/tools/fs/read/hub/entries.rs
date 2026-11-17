//! One directory's entries, in the order a listing and a walk both read them.

use std::path::{Path, PathBuf};

/// One directory's entries for a scan: junk and dot entries are dropped, so no walk enters a
/// build tree. Directories come first, and the order is case-insensitive.
pub(super) fn entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    entries_where(dir, false)
}

/// One directory's entries as a listing shows them: every name, junk included, never entered.
pub(crate) fn entries_all(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    entries_where(dir, true)
}

fn entries_where(dir: &Path, all: bool) -> Vec<(String, PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, bool)> = read
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !all && junk(&name) {
                return None;
            }
            let is_dir = entry.file_type().ok()?.is_dir();
            Some((name, entry.path(), is_dir))
        })
        .collect();
    out.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    out
}

fn junk(name: &str) -> bool {
    matches!(name, "target" | "node_modules") || name.starts_with('.')
}
