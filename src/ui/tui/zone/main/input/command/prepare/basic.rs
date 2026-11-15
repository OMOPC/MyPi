//! The workspace listing: a copy of the `read` tool's directory view, for the preparation.

use std::path::{Path, PathBuf};

/// Direct files at or past this count fold into counts.
const FOLD_AT: usize = 10;

/// The most lines one listing may take.
const MAX_LINES: usize = 800;

/// A file this big is not counted line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// The session root's listing. `src/` becomes a section of its own.
pub(super) fn root(cwd: &Path) -> String {
    let root = canonical(cwd);
    let mut list = List::new();
    list.line(format!("./  {}", counts(&root)));
    let src = root.join("src");
    fill(&mut list, &root, 1, false, Some(&src));
    if src_of(&src).is_some() {
        if !list.out.is_empty() {
            list.out.push('\n');
        }
        list.line(format!("src/  {}", counts(&src)));
        fill(&mut list, &src, 1, true, None);
    }
    list.finish()
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

/// One level: files carry their line count, directories their entry counts.
fn fill(list: &mut List, dir: &Path, depth: usize, expand: bool, skip: Option<&Path>) {
    let pad = "  ".repeat(depth);
    for (name, path, is_dir) in entries_all(dir) {
        if skip.is_some_and(|skip| skip == path) {
            continue;
        }
        if !is_dir {
            list.line(format!("{pad}{name}  {}", size_of(&path)));
        } else if expand {
            expand_one(list, &name, &path, depth);
        } else {
            list.line(format!("{pad}{name}/  {}", counts(&path)));
        }
    }
}

/// A child directory, one level deep: few files are listed, many are counted.
fn expand_one(list: &mut List, name: &str, dir: &Path, depth: usize) {
    let kids = entries_all(dir);
    let files: Vec<_> = kids.iter().filter(|kid| !kid.2).collect();
    let dirs: Vec<_> = kids.iter().filter(|kid| kid.2).collect();
    let pad = "  ".repeat(depth);
    let folded = files.len() >= FOLD_AT;
    list.line(format!(
        "{pad}{name}/  {} files, {} dirs",
        files.len(),
        dirs.len()
    ));
    if folded {
        return;
    }
    let inner = "  ".repeat(depth + 1);
    for (child, path, _) in files {
        list.line(format!("{inner}{child}  {}", size_of(path)));
    }
    for (child, path, _) in dirs {
        list.line(format!("{inner}{child}/  {}", counts(path)));
    }
}

/// A directory's own entry counts, without opening what it holds.
fn counts(dir: &Path) -> String {
    let kids = entries_all(dir);
    let files = kids.iter().filter(|kid| !kid.2).count();
    let dirs = kids.len() - files;
    format!("{}, {}", plural(files, "file"), plural(dirs, "dir"))
}

/// A file's total line count, without opening a file too big to count.
fn size_of(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "unknown".to_string();
    };
    if meta.len() > LARGE {
        return "large".to_string();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return "unknown".to_string();
    };
    if bytes.is_empty() {
        return "0 lines".to_string();
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    let lines = if bytes.ends_with(b"\n") {
        newlines
    } else {
        newlines + 1
    };
    format!("{lines} lines")
}

/// One directory's entries as a listing shows them: every name, junk included, never entered.
fn entries_all(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, bool)> = read
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
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

/// The crate's `src/` when `dir` sits inside it.
fn src_of(dir: &Path) -> Option<PathBuf> {
    for root in dir.ancestors() {
        if root.join("Cargo.toml").is_file() {
            let src = root.join("src");
            return dir.starts_with(&src).then_some(src);
        }
    }
    None
}

fn canonical(path: &Path) -> PathBuf {
    canonicalize_lenient(path)
}

/// Canonicalize as far as it goes, then re-attach the tail.
fn canonicalize_lenient(p: &Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(real) = cur.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (cur.parent().map(Path::to_path_buf), cur.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                cur = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}
