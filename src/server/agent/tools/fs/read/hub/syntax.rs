//! Rust read by line: `mod` declarations, `pub` items, `pub use`, and the use trees under them.

use std::path::Path;

use super::text::{clip, strip_attrs, strip_vis};
use crate::server::agent::tools::fs::lang::{self, Table};

/// `pub mod x;` lines, in the order the hub writes them.
pub(super) fn declarations(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let raw = strip_attrs(line.trim_start());
        let vis = if raw.starts_with("pub") {
            let rest = strip_vis(raw);
            if rest.len() < raw.len() {
                let end = raw
                    .find(')')
                    .map(|i| i + 1)
                    .unwrap_or_else(|| raw.find(' ').unwrap_or(raw.len()));
                raw[..end].trim().to_string()
            } else {
                "pub".to_string()
            }
        } else {
            "private".to_string()
        };
        let rest = strip_vis(raw);
        let Some(rest) = rest.strip_prefix("mod ") else {
            continue;
        };
        let rest = rest.trim_end();
        if !rest.ends_with(';') || rest.contains('{') {
            continue;
        }
        let name = rest.trim_end_matches(';').trim();
        if name.is_empty() || name.contains(' ') {
            continue;
        }
        out.push((name.to_string(), vis));
    }
    out
}

/// A file's `pub` items, signature only: no bodies, no private items.
pub(super) fn export_items(path: &Path, text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let table = lang::table(path, text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        if !raw.starts_with("pub") || !exportable(&table, i) {
            i += 1;
            continue;
        }
        let mut buf = raw.to_string();
        let mut j = i;
        while !buf.contains('{') && !buf.contains(';') && j + 1 < lines.len() && j - i < 6 {
            j += 1;
            buf.push(' ');
            buf.push_str(lines[j].trim());
        }
        let cut = buf.find(|c| c == '{' || c == ';').unwrap_or(buf.len());
        let signature = buf[..cut]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace("( ", "(")
            .replace(" )", ")")
            .replace(",)", ")");
        out.push(clip(signature));
        i = j + 1;
    }
    out
}
/// Whether this line opens a piece a child may export: not `use`, not `impl`, not `mod`.
fn exportable(table: &Table, at: usize) -> bool {
    table.line(at).is_some_and(|line| {
        line.head() && !matches!(lang::shape(line), None | Some("use" | "impl" | "mod"))
    })
}

/// The hub's `pub use` statements, each on one line.
pub(super) fn re_exports(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        if !raw.starts_with("pub") || !strip_vis(raw).starts_with("use ") {
            i += 1;
            continue;
        }
        let mut buf = raw.to_string();
        while !buf.contains(';') && i + 1 < lines.len() {
            i += 1;
            buf.push(' ');
            buf.push_str(lines[i].trim());
        }
        let statement = buf.split(';').next().unwrap_or("").trim().to_string();
        out.push(clip(statement));
        i += 1;
    }
    out
}

/// Every `use` statement in a file: the line it starts on, the full paths it names, and whether
/// it forwards those names rather than consuming them.
pub(super) fn uses(text: &str) -> Vec<(usize, Vec<Vec<String>>, bool)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        let public = raw.starts_with("pub");
        let stripped = strip_vis(raw);
        let Some(rest) = stripped.strip_prefix("use ") else {
            i += 1;
            continue;
        };
        let start = i;
        let mut buf = rest.trim().to_string();
        while !buf.contains(';') && i + 1 < lines.len() {
            i += 1;
            buf.push(' ');
            buf.push_str(lines[i].trim());
        }
        let body = buf.split(';').next().unwrap_or("");
        out.push((start + 1, expand(body), public));
        i += 1;
    }
    out
}

/// One use tree → the full paths it names. `crate::a::{B, c::D}` → `crate::a::B`, `crate::a::c::D`.
fn expand(tree: &str) -> Vec<Vec<String>> {
    expand_named(tree)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// The same, each path carrying the name it is visible under: `a::B as C` names `C`, and a glob
/// names nothing that can be traced.
pub(super) fn expand_named(tree: &str) -> Vec<(Vec<String>, String)> {
    let tree = tree.trim();
    if tree.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(Vec<String>, String)> = Vec::new();
    match split_group(tree) {
        Some((head, inner)) => {
            for part in split_top(inner) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                match split_group(part) {
                    Some((nested, body)) => {
                        let mut prefix: Vec<String> = segments(head);
                        prefix.extend(segments(nested));
                        out.extend(expand_named(&format!("{}::{{{body}}}", prefix.join("::"))));
                    }
                    None => {
                        let mut prefix: Vec<String> = segments(head);
                        let (tail, name) = split_alias(part);
                        prefix.extend(segments(tail));
                        if named(&name) {
                            out.push((prefix, name));
                        }
                    }
                }
            }
        }
        None => {
            let (tail, name) = split_alias(tree);
            if named(&name) {
                out.push((segments(tail), name));
            }
        }
    }
    out
}

/// A name a consumer can write: not empty, not a glob.
fn named(name: &str) -> bool {
    !name.is_empty() && name != "*"
}

/// A leaf tree and the name it is seen under: `B as C` → (`B`, `C`), `a::B` → (`a::B`, `B`).
fn split_alias(text: &str) -> (&str, String) {
    let text = text.trim();
    match text.split_once(" as ") {
        Some((left, right)) => (left.trim(), right.trim().to_string()),
        None => {
            let name = text
                .rsplit("::")
                .find(|part| !part.is_empty())
                .unwrap_or("");
            (text, name.to_string())
        }
    }
}

/// A path's segments, minus an `as` alias.
fn segments(text: &str) -> Vec<String> {
    let name = text.split(" as ").next().unwrap_or(text).trim();
    name.split("::")
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// The first `{…}` group in a tree, and what stands before it.
fn split_group(tree: &str) -> Option<(&str, &str)> {
    let open = tree.find('{')?;
    let mut depth = 0;
    let mut close = None;
    for (at, ch) in tree.char_indices().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let head = tree[..open].trim_end_matches("::").trim();
    Some((head, &tree[open + 1..close]))
}

/// Split a group's body on the commas that are not inside a nested group.
fn split_top(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (at, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}
