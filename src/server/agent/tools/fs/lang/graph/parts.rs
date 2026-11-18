// The printing parts: every view is assembled from these, so widths and counts never drift.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Boundary, Edge, Size};

// A path relative to the view's base, with slashes.
pub(crate) fn rel(base: &Path, path: &Path) -> String {
    let path = path.strip_prefix(base).unwrap_or(path);
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

// Whether `place` sits at or under `root`.
pub(crate) fn within(root: &std::path::Path, place: &std::path::Path) -> bool {
    place == root
        || place
            .strip_prefix(root)
            .is_ok_and(|rest| !rest.as_os_str().is_empty())
}

// A count and its word: `1 file`, `3 files`.
pub(crate) fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

// Every item on one line, nothing counted away.
pub(crate) fn joined_all(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join("; ")
    }
}

// A list on one line, cut at a character budget with the rest counted.
pub(crate) fn joined(items: &[String]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    const MAX: usize = 300;
    let mut out = String::new();
    let mut used = 0usize;
    for (at, item) in items.iter().enumerate() {
        let add = item.chars().count() + usize::from(at > 0) * 2;
        if used + add > MAX {
            out.push_str(&format!(" [+{} more]", items.len() - at));
            break;
        }
        if at > 0 {
            out.push_str("; ");
        }
        out.push_str(item);
        used += add;
    }
    out
}

// A line over the view's own width: its tail counted, never a silent ellipsis.
pub(crate) fn clip(text: String) -> String {
    const MAX: usize = 400;
    let count = text.chars().count();
    if count <= MAX {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(MAX)
        .map(|(at, _)| at)
        .unwrap_or(text.len());
    format!("{}[+{}]", &text[..cut], count - MAX)
}

// A section's first line: `up (12):`.
pub(crate) fn heading(label: &str, count: usize) -> String {
    format!("{label} ({count}):")
}

// One interface item: `  pub fn rows(&self) -> Vec<Row>`.
pub(crate) fn item_line(text: &str) -> String {
    clip(format!("  {text}"))
}

// One child's first line: `  a/  3 files`, `  a.rs  40L`.
pub(crate) fn node_line(name: &str, size: Size) -> String {
    match size {
        Size::Files(n) => format!("  {name}/  {}", plural(n, "file")),
        Size::Lines(n) => format!("  {name}  {n}L"),
        Size::Unknown => format!("  {name}"),
    }
}

// One resolved reference: `  src/a/b.rs:12 imports B`, `  … (external)`.
pub(crate) fn edge_line(base: &Path, edge: &Edge) -> String {
    let mut line = format!(
        "  {}:{} imports {}",
        rel(base, &edge.at),
        edge.line + 1,
        joined(&edge.symbols)
    );
    if edge.to.is_none() {
        line.push_str(&format!("  {} (external)", edge.wrote));
    }
    if let Some(via) = edge.via.first() {
        line.push_str(&format!("  via {}", rel(base, via)));
    }
    clip(line)
}

// Importers folded by directory, for a list too long to print whole.
pub(crate) fn grouped(base: &Path, edges: &[Edge]) -> Vec<String> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in edges {
        let full = rel(base, &edge.at);
        let (dir, name) = match full.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), full),
        };
        map.entry(dir)
            .or_default()
            .push(format!("{name}:{}", edge.line + 1));
    }
    map.into_iter()
        .map(|(dir, files)| {
            let count = files.len();
            clip(format!("{dir} ({count} files): {}", files.join(", ")))
        })
        .collect()
}

// Where the walk stopped and how far it got.
pub(crate) fn boundary_line(base: &Path, b: &Boundary) -> String {
    let up = if b.no_above {
        "none".to_string()
    } else if b.complete {
        "complete".to_string()
    } else {
        match &b.stop {
            Some(dir) => format!("{} (budget)", rel(base, dir)),
            None => "stopped".to_string(),
        }
    };
    format!(
        "[scan: up→{up} | {}, {} looked at]",
        plural(b.levels, "level"),
        plural(b.files, "file")
    )
}

// Keep a text inside a line budget and a byte budget, counting what was dropped.
pub(crate) fn cut(text: String, lines: usize, bytes: usize) -> String {
    let total = text.lines().count();
    if total <= lines && text.len() <= bytes {
        return text;
    }
    let mut out = String::new();
    for line in text.lines().take(lines) {
        if out.len() + line.len() + 1 > bytes {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    let kept = out.lines().count();
    out.push_str(&format!("[+{} lines]\n", total.saturating_sub(kept)));
    out
}
