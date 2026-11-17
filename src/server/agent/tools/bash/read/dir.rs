//! Directory listings: one level per directory, and each level judging its own documents.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, bail};

use super::hub;
use crate::server::platform::time;

/// Direct files at or past this count fold into counts.
const FOLD_AT: usize = 10;

/// The most lines one listing may take.
const MAX_LINES: usize = 800;

/// A file this big is not counted line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// One directory's listing. The root is special: `src/` becomes a section of its own. `docs` names
/// the documents the user keeps current, which take no part in any level's verdict.
pub(crate) fn listing(path: &Path, cwd: &Path, titled: bool, docs: &[String]) -> Result<String> {
    let root = hub::canonical(cwd);
    let target = hub::canonical(path);
    let mut list = List::new();
    if target == root {
        if titled {
            list.line(format!("./  {}", counts(&root)));
        }
        let src = root.join("src");
        fill(&mut list, &root, 1, false, Some(&src), docs);
        if hub::src_of(&src).is_some() {
            if !list.out.is_empty() {
                list.out.push('\n');
            }
            list.line(format!("src/  {}", counts(&src)));
            fill(&mut list, &src, 1, true, None, docs);
        }
        return Ok(list.finish());
    }
    if hub::src_of(&target).is_none() {
        bail!("`read` lists directories at the session root and inside a crate's `src/` only");
    }
    if titled {
        list.line(format!("{}/  {}", shown(cwd, &target), counts(&target)));
    }
    fill(&mut list, &target, 1, true, None, docs);
    Ok(list.finish())
}

/// The listing being built: its lines, and how many the budget pushed away.
struct List {
    out: String,
    left: usize,
    over: usize,
}

impl List {
    fn new() -> Self {
        List {
            out: String::new(),
            left: MAX_LINES,
            over: 0,
        }
    }

    /// One printed line, or one counted away when the budget is spent.
    fn line(&mut self, text: String) {
        if self.left == 0 {
            self.over += 1;
            return;
        }
        self.left -= 1;
        self.out.push_str(&text);
        self.out.push('\n');
    }

    fn finish(mut self) -> String {
        if self.over > 0 {
            self.out.push_str(&format!("[+{} entries]\n", self.over));
        }
        self.out
    }
}

/// One level: files carry their size and their stamp, directories their entry counts.
fn fill(
    list: &mut List,
    dir: &Path,
    depth: usize,
    expand: bool,
    skip: Option<&Path>,
    docs: &[String],
) {
    let pad = "  ".repeat(depth);
    for row in level(dir, skip, docs) {
        if !row.dir {
            list.line(format!("{pad}{}  {}", row.label(), row.size));
        } else if expand {
            expand_one(list, &row, depth, docs);
        } else {
            list.line(format!("{pad}{}/  {}", row.name, counts(&row.path)));
        }
    }
}

/// A child directory, one level deep: few files are listed, many are counted.
fn expand_one(list: &mut List, dir: &Entry, depth: usize, docs: &[String]) {
    let rows = level(&dir.path, None, docs);
    let files: Vec<&Entry> = rows.iter().filter(|row| !row.dir).collect();
    let dirs: Vec<&Entry> = rows.iter().filter(|row| row.dir).collect();
    let pad = "  ".repeat(depth);
    let folded = files.len() >= FOLD_AT;
    list.line(format!(
        "{pad}{}/  {} files, {} dirs",
        dir.name,
        files.len(),
        dirs.len()
    ));
    if folded {
        return;
    }
    let inner = "  ".repeat(depth + 1);
    for file in files {
        list.line(format!("{inner}{}  {}", file.label(), file.size));
    }
    for child in dirs {
        list.line(format!("{inner}{}/  {}", child.name, counts(&child.path)));
    }
}

/// One level's entries with the level's own verdict applied: a level whose documents are all newer
/// than everything beside them keeps no stamps and marks the documents instead.
fn level(dir: &Path, skip: Option<&Path>, docs: &[String]) -> Vec<Entry> {
    let mut rows: Vec<Entry> = hub::entries_all(dir)
        .into_iter()
        .filter(|(_, path, _)| skip.is_none_or(|skip| skip != path.as_path()))
        .map(|(name, path, is_dir)| row(name, path, is_dir, docs))
        .collect();
    let current = current_docs(&rows);
    for row in &mut rows {
        row.tag = match row.kind {
            Kind::Doc if current => "(latest)",
            Kind::Pinned => "(persistent)",
            _ => "",
        };
        if current {
            row.at = None;
        }
    }
    rows
}

/// Whether the documents this level holds are newer than every other file beside them, so no stamp
/// is needed to know they stand as written. A level holding no such document answers yes.
fn current_docs(rows: &[Entry]) -> bool {
    let mut oldest_doc: Option<SystemTime> = None;
    let mut newest_other: Option<SystemTime> = None;
    for row in rows.iter().filter(|row| !row.dir) {
        let at = match row.at {
            Some(at) => at,
            None if matches!(row.kind, Kind::Pinned) => continue,
            None => return false,
        };
        match row.kind {
            Kind::Doc => oldest_doc = Some(oldest_doc.map_or(at, |old| old.min(at))),
            Kind::Plain => newest_other = Some(newest_other.map_or(at, |old| old.max(at))),
            Kind::Pinned => {}
        }
    }
    match (oldest_doc, newest_other) {
        (Some(doc), Some(other)) => doc > other,
        _ => true,
    }
}

/// One entry: what it is, how big, when it was last written, and the tag its level gave it.
struct Entry {
    name: String,
    path: PathBuf,
    dir: bool,
    kind: Kind,
    /// The file's last write, in the process's zone; cleared when the level vouches for its docs.
    at: Option<SystemTime>,
    size: String,
    /// `(latest)`, `(persistent)`, or nothing.
    tag: &'static str,
}

impl Entry {
    /// The name as one line shows it: the stamp first when the level carries stamps, then the name
    /// and the tag its level gave it.
    fn label(&self) -> String {
        let named = format!("{}{}", self.name, self.tag);
        match self.at {
            Some(at) => format!("{}  {named}", time::stamp_at(at)),
            None => named,
        }
    }
}

/// What one file is to the freshness rule.
#[derive(Clone, Copy)]
enum Kind {
    /// A document the model would have to weigh: upper case, and not one the user declared.
    Doc,
    /// A document the user keeps current himself: never weighed, never dated.
    Pinned,
    /// Anything else, and every directory.
    Plain,
}

fn row(name: String, path: PathBuf, dir: bool, docs: &[String]) -> Entry {
    if dir {
        return Entry {
            name,
            path,
            dir,
            kind: Kind::Plain,
            at: None,
            size: String::new(),
            tag: "",
        };
    }
    let (at, size) = facts(&path);
    Entry {
        kind: classify(&name, docs),
        name,
        path,
        dir,
        at,
        size,
        tag: "",
    }
}

/// A name's part in the rule: only `.md` takes part, and only the stem is compared with the
/// configured ones. Upper case means no lower-case letter and at least one upper-case one.
fn classify(name: &str, docs: &[String]) -> Kind {
    let Some(stem) = name.strip_suffix(".md") else {
        return Kind::Plain;
    };
    if docs.iter().any(|doc| doc == stem) {
        return Kind::Pinned;
    }
    let mut letters = stem.chars().filter(|c| c.is_ascii_alphabetic());
    let head = letters.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest = letters.all(|c| c.is_ascii_uppercase());
    if head && rest { Kind::Doc } else { Kind::Plain }
}

/// A file's stamp and size from one look at its metadata.
fn facts(path: &Path) -> (Option<SystemTime>, String) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (None, "unknown".to_string());
    };
    let at = meta.modified().ok();
    if meta.len() > LARGE {
        return (at, "large".to_string());
    }
    let Ok(bytes) = std::fs::read(path) else {
        return (at, "unknown".to_string());
    };
    if bytes.is_empty() {
        return (at, "0 lines".to_string());
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    let lines = if bytes.ends_with(b"\n") {
        newlines
    } else {
        newlines + 1
    };
    (at, format!("{lines} lines"))
}

/// A directory's own entry counts, without opening what it holds.
fn counts(dir: &Path) -> String {
    let kids = hub::entries_all(dir);
    let files = kids.iter().filter(|kid| !kid.2).count();
    let dirs = kids.len() - files;
    format!(
        "{}, {}",
        hub::plural(files, "file"),
        hub::plural(dirs, "dir")
    )
}

fn shown(cwd: &Path, path: &Path) -> String {
    let path: PathBuf = path
        .strip_prefix(cwd)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf());
    path.to_string_lossy().replace('\\', "/")
}
