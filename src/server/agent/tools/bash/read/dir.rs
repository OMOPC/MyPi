//! Directory listings: the session root, and any directory inside a crate's `src/`.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::hub;

/// Direct files at or past this count fold into one line.
const FOLD_AT: usize = 10;

/// The most lines one listing may take.
const MAX_LINES: usize = 800;

/// A file this big is not counted line by line.
const LARGE: u64 = 4 * 1024 * 1024;

/// One directory's listing. The root is special: `src/` is expanded in place of a bare name.
pub(crate) fn listing(path: &Path, cwd: &Path, titled: bool) -> Result<String> {
    let root = hub::canonical(cwd);
    let target = hub::canonical(path);
    let mut out = String::new();
    let mut left = MAX_LINES;
    if target == root {
        if titled {
            line(&mut out, &mut left, "./".to_string());
        }
        let src = root.join("src");
        let mut ok = fill(&root, 1, false, Some(&src), &mut out, &mut left);
        if ok && hub::src_of(&src).is_some() {
            if titled {
                ok = line(&mut out, &mut left, "src/".to_string());
            }
            if ok {
                ok = fill(&src, 1, true, None, &mut out, &mut left);
            }
        }
        if !ok {
            out.push_str("[more entries]\n");
        }
        return Ok(out);
    }
    if hub::src_of(&target).is_none() {
        bail!("`read` lists directories at the session root and inside a crate's `src/` only");
    }
    if titled {
        line(&mut out, &mut left, format!("{}/", shown(cwd, &target)));
    }
    if !fill(&target, 1, true, None, &mut out, &mut left) {
        out.push_str("[more entries]\n");
    }
    Ok(out)
}

/// One printed line, unless the listing is already as long as it may be.
fn line(out: &mut String, left: &mut usize, text: String) -> bool {
    if *left == 0 {
        return false;
    }
    *left -= 1;
    out.push_str(&text);
    out.push('\n');
    true
}

/// One level: files carry their line count, directories follow the expansion rule.
fn fill(
    dir: &Path,
    depth: usize,
    expand: bool,
    skip: Option<&Path>,
    out: &mut String,
    left: &mut usize,
) -> bool {
    let pad = "  ".repeat(depth);
    for (name, path, is_dir) in hub::entries(dir) {
        if skip.is_some_and(|skip| skip == path) {
            continue;
        }
        let ok = if !is_dir {
            line(out, left, format!("{pad}{name} ({})", size_of(&path)))
        } else if expand {
            expand_one(&name, &path, depth, out, left)
        } else {
            line(out, left, format!("{pad}{name}/ (dir)"))
        };
        if !ok {
            return false;
        }
    }
    true
}

/// A child directory, one level deep: few files are listed, many are counted.
fn expand_one(name: &str, dir: &Path, depth: usize, out: &mut String, left: &mut usize) -> bool {
    let kids = hub::entries(dir);
    let files: Vec<_> = kids.iter().filter(|kid| !kid.2).collect();
    let dirs: Vec<_> = kids.iter().filter(|kid| kid.2).collect();
    let pad = "  ".repeat(depth);
    let folded = files.len() >= FOLD_AT;
    let header = if folded {
        let mut counts = format!("{} files", files.len());
        if !dirs.is_empty() {
            counts.push_str(&format!(", {} folders", dirs.len()));
        }
        format!("{pad}{name}/ ({counts})")
    } else if dirs.is_empty() {
        format!("{pad}{name}/")
    } else {
        format!("{pad}{name}/ ({} folders)", dirs.len())
    };
    if !line(out, left, header) {
        return false;
    }
    if folded {
        return true;
    }
    let inner = "  ".repeat(depth + 1);
    for (child, path, _) in files {
        if !line(out, left, format!("{inner}{child} ({})", size_of(path))) {
            return false;
        }
    }
    for (child, _, _) in dirs {
        if !line(out, left, format!("{inner}{child}/ (dir)")) {
            return false;
        }
    }
    true
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
