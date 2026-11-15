//! Directory listings: the session root, and any directory inside a crate's `src/`.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::hub;

/// Direct files at or past this count fold into counts.
const FOLD_AT: usize = 10;

/// The most lines one listing may take.
const MAX_LINES: usize = 800;

/// A file this big is not counted line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// One directory's listing. The root is special: `src/` becomes a section of its own.
pub(crate) fn listing(path: &Path, cwd: &Path, titled: bool) -> Result<String> {
    let root = hub::canonical(cwd);
    let target = hub::canonical(path);
    let mut list = List::new();
    if target == root {
        if titled {
            list.line(format!("./  {}", counts(&root)));
        }
        let src = root.join("src");
        fill(&mut list, &root, 1, false, Some(&src));
        if hub::src_of(&src).is_some() {
            if !list.out.is_empty() {
                list.out.push('\n');
            }
            list.line(format!("src/  {}", counts(&src)));
            fill(&mut list, &src, 1, true, None);
        }
        return Ok(list.finish());
    }
    if hub::src_of(&target).is_none() {
        bail!("`read` lists directories at the session root and inside a crate's `src/` only");
    }
    if titled {
        list.line(format!("{}/  {}", shown(cwd, &target), counts(&target)));
    }
    fill(&mut list, &target, 1, true, None);
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

/// One level: files carry their line count, directories their entry counts.
fn fill(list: &mut List, dir: &Path, depth: usize, expand: bool, skip: Option<&Path>) {
    let pad = "  ".repeat(depth);
    for (name, path, is_dir) in hub::entries_all(dir) {
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
    let kids = hub::entries_all(dir);
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
    let kids = hub::entries_all(dir);
    let files = kids.iter().filter(|kid| !kid.2).count();
    let dirs = kids.len() - files;
    format!(
        "{}, {}",
        hub::plural(files, "file"),
        hub::plural(dirs, "dir")
    )
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

fn shown(cwd: &Path, path: &Path) -> String {
    let path: PathBuf = path
        .strip_prefix(cwd)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf());
    path.to_string_lossy().replace('\\', "/")
}
