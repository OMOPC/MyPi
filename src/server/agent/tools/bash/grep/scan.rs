//! The walk: the crate's own `src/`, the files under it, and the lines that match.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};

use super::rust::{self, Layout};

/// A file this big is not read line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// Directories inside `src/` that are never source.
const SKIP: &[&str] = &["target", "node_modules"];

/// Where the crate stands: its root, and the source tree under it.
pub(crate) struct Zone {
    pub root: PathBuf,
    pub src: PathBuf,
}

/// The crate the session stands in: the nearest `Cargo.toml` above the working directory.
pub(crate) fn zone(cwd: &Path) -> Result<Zone> {
    for dir in cwd.ancestors() {
        if !dir.join("Cargo.toml").is_file() {
            continue;
        }
        let src = dir.join("src");
        if !src.is_dir() {
            bail!("{} has a Cargo.toml but no src directory", dir.display());
        }
        return Ok(Zone {
            root: dir.to_path_buf(),
            src,
        });
    }
    Err(anyhow!(
        "no Cargo.toml above {}; grep reads a crate's own src/",
        cwd.display()
    ))
}

/// What the call looks for.
pub(crate) enum Matcher {
    Text(String),
    Pattern(regex::Regex),
}

impl Matcher {
    pub fn text(query: &str) -> Self {
        Self::Text(query.to_string())
    }

    /// A pattern as written; its own syntax errors are the model's to fix.
    pub fn pattern(query: &str) -> Result<Self> {
        regex::Regex::new(query).map(Self::Pattern).map_err(|e| {
            anyhow!("`query` is not a valid pattern: {e}; leave `regex` off to search it as text")
        })
    }

    fn hits(&self, line: &str) -> bool {
        match self {
            Self::Text(needle) => line.contains(needle.as_str()),
            Self::Pattern(pattern) => pattern.is_match(line),
        }
    }
}

/// One source file, its matching lines, and its items.
pub(crate) struct File {
    pub path: PathBuf,
    pub text: String,
    /// Matching lines, 0-based.
    pub hits: Vec<usize>,
    pub layout: Layout,
}

/// Every `.rs` file under the crate's `src/`, with the lines that match.
pub(crate) fn collect(zone: &Zone, matcher: &Matcher) -> Vec<File> {
    let mut paths = Vec::new();
    sources(&zone.src, &mut paths);
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(size) = std::fs::metadata(&path).map(|meta| meta.len()) else {
            continue;
        };
        if size > LARGE {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let hits: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| matcher.hits(line))
            .map(|(at, _)| at)
            .collect();
        let layout = rust::analyse(&text);
        files.push(File {
            path,
            text,
            hits,
            layout,
        });
    }
    files
}

/// One directory's `.rs` files, recursively; dot entries and build trees stay out.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                continue;
            }
            sources(&entry.path(), out);
        } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
            out.push(entry.path());
        }
    }
}
