//! The walk: the crate's own `src/`, the directories a call narrows it to, and the lines that match.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};

use crate::server::agent::tools::fs::lang::{self, Table};
use crate::server::agent::tools::workspace::canonicalize_lenient;

/// A file this big is not read line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// Directories inside `src/` that are never source.
const SKIP: &[&str] = &["target", "node_modules"];

/// Threads one answer may read its files on.
const THREADS: usize = 8;

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

/// The files and directories to read, and one line per scope entry that could not be used.
pub(crate) fn roots(zone: &Zone, scope: &[String]) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<String>) {
    if scope.is_empty() {
        return (vec![zone.src.clone()], Vec::new(), Vec::new());
    }
    let src = canonicalize_lenient(&zone.src);
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut misses: Vec<String> = Vec::new();

    for entry in scope {
        let raw = entry.trim();
        if raw.is_empty() {
            misses.push("[scope] (empty): no path".to_string());
            continue;
        }
        let named = Path::new(raw);
        let joined = if named.is_absolute() {
            named.to_path_buf()
        } else {
            zone.root.join(named)
        };
        let real = canonicalize_lenient(&joined);
        if !real.exists() {
            misses.push(format!("[scope] {raw}: does not exist"));
        } else if !real.starts_with(&src) {
            misses.push(format!("[scope] {raw}: is outside src/"));
        } else if real.is_dir() {
            if !dirs.contains(&real) {
                dirs.push(real);
            }
        } else if real.extension().is_some_and(|e| e == "rs") {
            if !files.contains(&real) {
                files.push(real);
            }
        } else {
            misses.push(format!("[scope] {raw}: only .rs files are read"));
        }
    }
    (dirs, files, misses)
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

/// One source file, its matching lines, and its pieces.
pub(crate) struct File {
    pub path: PathBuf,
    pub text: String,
    /// Matching lines, 0-based.
    pub hits: Vec<usize>,
    pub table: Table,
}

/// Every named file, and every `.rs` file under these directories; read in parallel.
pub(crate) fn collect(dirs: &[PathBuf], files: &[PathBuf], matcher: &Matcher) -> Vec<File> {
    let mut named: BTreeSet<PathBuf> = files.iter().cloned().collect();
    for dir in dirs {
        sources(dir, &mut named);
    }
    let paths: Vec<PathBuf> = named.into_iter().collect();

    if paths.len() <= 1 {
        return paths
            .iter()
            .filter_map(|path| read(path, matcher))
            .collect();
    }
    let batch = paths.len().div_ceil(THREADS).max(1);
    let mut out: Vec<File> = Vec::with_capacity(paths.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(batch)
            .map(|chunk| scope.spawn(move || read_all(chunk, matcher)))
            .collect();
        for handle in handles {
            out.extend(handle.join().unwrap_or_default());
        }
    });
    out
}

fn read_all(paths: &[PathBuf], matcher: &Matcher) -> Vec<File> {
    paths
        .iter()
        .filter_map(|path| read(path, matcher))
        .collect()
}

/// One file's hits and its table; a file that cannot be read is simply not there.
fn read(path: &Path, matcher: &Matcher) -> Option<File> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > LARGE {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let hits: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| matcher.hits(line))
        .map(|(at, _)| at)
        .collect();
    let table = lang::table(path, &text);
    Some(File {
        path: path.to_path_buf(),
        text,
        hits,
        table,
    })
}

/// One directory's `.rs` files, recursively; dot entries and build trees stay out.
fn sources(dir: &Path, out: &mut BTreeSet<PathBuf>) {
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
            out.insert(entry.path());
        }
    }
}
