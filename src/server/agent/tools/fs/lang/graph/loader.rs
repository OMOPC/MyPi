// One read-only view of the tree: entries and text, with the junk list and the size cap in one place.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// A file past this size is not read: the same cap `grep` uses.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

// One directory entry, already sorted.
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) dir: bool,
}

// The only door to the disk the machine and the languages share. It never writes.
pub(crate) struct Loader {
    cache: RefCell<HashMap<PathBuf, Option<String>>>,
}

impl Loader {
    pub(crate) fn new() -> Self {
        Loader {
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// One file's text; a missing, unreadable or oversized file answers `None`.
    pub(crate) fn read(&self, path: &Path) -> Option<String> {
        if let Some(hit) = self.cache.borrow().get(path) {
            return hit.clone();
        }
        let text = read_one(path);
        self.cache
            .borrow_mut()
            .insert(path.to_path_buf(), text.clone());
        text
    }

    /// One directory's entries, junk skipped, directories first, names case-insensitive.
    pub(crate) fn entries(&self, dir: &Path) -> Vec<Entry> {
        entries(dir)
    }
}

impl Default for Loader {
    fn default() -> Self {
        Loader::new()
    }
}

fn read_one(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

// One directory's entries for a walk: junk skipped, directories first, names case-insensitive.
pub(crate) fn entries(dir: &Path) -> Vec<Entry> {
    entries_where(dir, false)
}

// One directory's entries for a listing: every name, junk included.
pub(crate) fn entries_all(dir: &Path) -> Vec<Entry> {
    entries_where(dir, true)
}

fn entries_where(dir: &Path, all: bool) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = read
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !all && junk(&name) {
                return None;
            }
            let dir = entry.file_type().ok()?.is_dir();
            Some(Entry {
                name,
                path: entry.path(),
                dir,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn junk(name: &str) -> bool {
    matches!(name, "target" | "node_modules") || name.starts_with('.')
}
