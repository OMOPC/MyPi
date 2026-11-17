//! The answer: the type section, the module section, the x-ray, the hits, and the stamps.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::server::agent::tools::contract::env::{GrepLimits, Seen, Stamp};
use crate::server::agent::tools::fs::lang::{self, Line, Table};
use crate::server::agent::tools::summary;

use super::{outline, scan};

/// One answered call's text, and the files whose lines it showed.
pub(crate) fn answer(
    query: &str,
    mode: &str,
    files: &[scan::File],
    zone: &scan::Zone,
    caps: GrepLimits,
    budget: usize,
    misses: &[String],
) -> (String, BTreeSet<usize>) {
    let budget = budget.max(24);
    let total: usize = files.iter().map(|file| file.hits.len()).sum();
    let touched = files.iter().filter(|file| !file.hits.is_empty()).count();
    let tier = if total <= caps.deep {
        1
    } else if total <= caps.fold {
        2
    } else {
        3
    };
    let mut out = format!(
        "grep \"{query}\": {}, {}, {mode}, tier {tier}\n",
        plural(total, "hit"),
        plural(touched, "file"),
    );

    let mut shown: BTreeSet<usize> = BTreeSet::new();
    if let Some((a, b)) = two_segments(query) {
        out.push('\n');
        out.push_str(&type_section(files, zone, &a, &b, caps, &mut shown));
        out.push('\n');
        out.push_str(&module_section(files, zone, &a, &b, caps, &mut shown));
    }
    if let Some(name) = single_name(query) {
        let rays = x_ray(files, zone, name, caps, &mut shown);
        if !rays.is_empty() {
            out.push('\n');
            out.push_str(&rays);
        }
    }
    out.push('\n');
    let left = budget.saturating_sub(out.lines().count()).max(12);
    out.push_str(&hits(files, zone, tier, caps, left, &mut shown));
    for miss in misses {
        out.push_str(miss);
        out.push('\n');
    }
    (out, shown)
}

// ---- the three sections ----

/// `A::b` where `b` is declared on the type `A`: its `impl`, `trait`, `enum` or `struct` bodies.
fn type_section(
    files: &[scan::File],
    zone: &scan::Zone,
    a: &str,
    b: &str,
    caps: GrepLimits,
    shown: &mut BTreeSet<usize>,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    for (at, file) in files.iter().enumerate() {
        let raw: Vec<&str> = file.text.lines().collect();
        let label = rel(zone, &file.path);
        for (index, piece) in file.table.pieces() {
            if !bears(piece) || !names(piece, a) {
                continue;
            }
            let mut mine: Vec<String> = Vec::new();
            if is_impl(piece) || lang::shape(piece) == Some("trait") {
                for (member_at, member) in file.table.inner(index) {
                    if names(member, b) && declared(member) {
                        mine.push(member_row(&file.table, member_at));
                    }
                }
            }
            if matches!(lang::shape(piece), Some("enum" | "struct")) {
                let end = file.table.end(index).unwrap_or(index);
                for line in members(&raw, index + 1, end, b) {
                    mine.push(format!("{}  {}", line + 1, summary::clip(raw[line], 120)));
                }
            }
            if mine.is_empty() {
                continue;
            }
            shown.insert(at);
            rows.push(format!(
                "  {label}:{}  {}",
                index + 1,
                summary::clip(exact(piece), 200)
            ));
            for row in mine.iter().take(caps.items) {
                rows.push(format!("    {row}"));
            }
            if mine.len() > caps.items {
                rows.push(format!("    [+{} more]", mine.len() - caps.items));
            }
        }
    }
    if rows.is_empty() {
        return format!(
            "[type] {a}::{b}: no `{a}` in this crate declares `{b}`; this is not a \
             crate-defined type member\n"
        );
    }
    let mut out = format!("[type] {a}::{b}\n");
    for row in rows {
        out.push_str(&row);
        out.push('\n');
    }
    out
}

/// `A::b` where `A` is a module: the item `b` that module declares.
fn module_section(
    files: &[scan::File],
    zone: &scan::Zone,
    a: &str,
    b: &str,
    caps: GrepLimits,
    shown: &mut BTreeSet<usize>,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    let mut known = false;
    let mut seen: BTreeSet<(String, usize)> = BTreeSet::new();
    for (at, file) in files.iter().enumerate() {
        let Some(base) = file.path.parent() else {
            continue;
        };
        for (index, declaration) in file.table.pieces() {
            if lang::shape(declaration) != Some("mod") || !names(declaration, a) {
                continue;
            }
            // `mod a { … }` lives in this file; `mod a;` names a sibling file or directory.
            let end = file.table.end(index).unwrap_or(index);
            let inline = end > index;
            let mut roots: Vec<std::path::PathBuf> = Vec::new();
            if !inline {
                let dir = base.join(a);
                let single = base.join(format!("{a}.rs"));
                if dir.is_dir() {
                    roots.push(dir);
                }
                if single.is_file() {
                    roots.push(single);
                }
                if roots.is_empty() {
                    continue;
                }
            }
            known = true;
            for (other, target) in files.iter().enumerate() {
                let inside = roots
                    .iter()
                    .any(|root| target.path == *root || target.path.starts_with(root));
                if !inside && !(inline && other == at) {
                    continue;
                }
                let label = rel(zone, &target.path);
                for (item_at, item) in target.table.pieces() {
                    if !names(item, b) || lang::shape(item) == Some("use") {
                        continue;
                    }
                    if other == at && inline && !(item_at > index && item_at <= end) {
                        continue;
                    }
                    if !seen.insert((label.clone(), item_at)) {
                        continue;
                    }
                    shown.insert(other);
                    rows.push(format!(
                        "  {label}:{}  {}  ({}..{})",
                        item_at + 1,
                        summary::clip(exact(item), 200),
                        item_at + 1,
                        target.table.end(item_at).unwrap_or(item_at) + 1
                    ));
                }
            }
        }
    }
    if rows.is_empty() {
        return if known {
            format!("[module] {a}::{b}: module `{a}` declares no `{b}`\n")
        } else {
            format!("[module] {a}::{b}: no module `{a}` in this crate\n")
        };
    }
    let mut out = format!("[module] {a}::{b}\n");
    for row in rows.iter().take(caps.items) {
        out.push_str(row);
        out.push('\n');
    }
    if rows.len() > caps.items {
        out.push_str(&format!("  [+{} more]\n", rows.len() - caps.items));
    }
    out
}

/// The type's own declaration, and every `impl` block for it, with their items' spans.
fn x_ray(
    files: &[scan::File],
    zone: &scan::Zone,
    name: &str,
    caps: GrepLimits,
    shown: &mut BTreeSet<usize>,
) -> String {
    let declares = |line: &Line| {
        matches!(
            lang::shape(line),
            Some("struct" | "enum" | "trait" | "union")
        ) && names(line, name)
    };
    if !files
        .iter()
        .any(|file| file.table.pieces().iter().any(|(_, line)| declares(line)))
    {
        return String::new();
    }
    let mut out = format!("[x-ray] {name}\n");
    for (at, file) in files.iter().enumerate() {
        let label = rel(zone, &file.path);
        for (index, line) in file.table.pieces() {
            if !declares(line) {
                continue;
            }
            shown.insert(at);
            out.push_str(&format!(
                "  {label}:{}  {}  ({}..{})\n",
                index + 1,
                summary::clip(exact(line), 200),
                index + 1,
                file.table.end(index).unwrap_or(index) + 1
            ));
        }
    }
    let mut blocks = 0usize;
    let mut listed = 0usize;
    for (at, file) in files.iter().enumerate() {
        let label = rel(zone, &file.path);
        for (index, line) in file.table.pieces() {
            if !is_impl(line) || !names(line, name) {
                continue;
            }
            blocks += 1;
            if listed >= caps.impls {
                continue;
            }
            listed += 1;
            shown.insert(at);
            out.push_str(&format!(
                "  {label}:{}..{}  {}\n",
                index + 1,
                file.table.end(index).unwrap_or(index) + 1,
                summary::clip(exact(line), 200)
            ));
            let members = file.table.inner(index);
            for (member_at, _) in members.iter().take(caps.items) {
                out.push_str(&format!("    {}\n", member_row(&file.table, *member_at)));
            }
            if members.len() > caps.items {
                out.push_str(&format!("    [+{} more]\n", members.len() - caps.items));
            }
        }
    }
    if blocks == 0 {
        out.push_str("  [no impl blocks in this crate]\n");
    } else if blocks > listed {
        out.push_str(&format!("  [+{} more impl blocks]\n", blocks - listed));
    }
    out
}

/// Whether this piece is an impl block, of either shape.
fn is_impl(line: &Line) -> bool {
    matches!(lang::shape(line), Some("impl"))
}

/// Whether this piece is one of the kinds `A::b` asks about.
fn bears(line: &Line) -> bool {
    matches!(
        lang::shape(line),
        Some("impl" | "trait" | "struct" | "enum" | "union")
    )
}

/// Whether this piece's body declares functions, consts, types or statics by name.
fn declared(line: &Line) -> bool {
    matches!(lang::shape(line), Some("fn" | "const" | "type" | "static"))
}

/// Whether this piece binds `word`: its own name, or the trait and target of an `impl`.
fn names(line: &Line, word: &str) -> bool {
    lang::binds(line, word)
}

/// The line a row shows: the piece's own header, never the shape.
fn exact(line: &Line) -> &str {
    line.exact.as_deref().unwrap_or("")
}

/// One member as a row: its span, and its own name.
fn member_row(table: &Table, at: usize) -> String {
    let end = table.end(at).unwrap_or(at);
    format!(
        "{}..{}  {}",
        at + 1,
        end + 1,
        table.line(at).map_or("", exact)
    )
}

/// Lines inside one piece's body whose own line binds `name`: a variant, a field, a const.
fn members(raw: &[&str], from: usize, to: usize, name: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for at in from..to.min(raw.len()) {
        let line = raw[at].trim();
        let Some(rest) = line.strip_prefix(name) else {
            continue;
        };
        let next = rest.chars().next();
        if matches!(
            next,
            None | Some(' ' | ',' | '{' | '(' | '=' | ':' | ';' | '<')
        ) {
            out.push(at);
        }
    }
    out
}

// ---- the hits ----

/// Hit lines, at the density the total allows: whole items, the file's skeleton, then counts.
fn hits(
    files: &[scan::File],
    zone: &scan::Zone,
    tier: usize,
    caps: GrepLimits,
    budget: usize,
    shown: &mut BTreeSet<usize>,
) -> String {
    let mut out = String::new();
    let mut left = budget;
    let total: usize = files.iter().map(|file| file.hits.len()).sum();
    let mut covered = 0usize;

    if tier == 3 {
        let mut rows: Vec<(String, usize)> = files
            .iter()
            .filter(|file| !file.hits.is_empty())
            .map(|file| (rel(zone, &file.path), file.hits.len()))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        let room = budget.saturating_sub(2);
        out.push_str("[hits] too wide to list; per file\n");
        for (path, count) in rows.iter().take(room) {
            out.push_str(&format!("  {path}  {count}\n"));
        }
        if rows.len() > room {
            out.push_str(&format!("  [+{} more files]\n", rows.len() - room));
        }
        return out;
    }

    for (at, file) in files.iter().enumerate() {
        if file.hits.is_empty() {
            continue;
        }
        let raw: Vec<&str> = file.text.lines().collect();
        let label = rel(zone, &file.path);
        if tier == 2 {
            let block = outline::file(file, &raw, &label, caps);
            let rows = block.lines().count();
            if rows >= left {
                out.push_str(&format!(
                    "[+{} more files; narrow the query]\n",
                    touched_after(files, at)
                ));
                return out;
            }
            left -= rows;
            covered += file.hits.len();
            shown.insert(at);
            out.push_str(&block);
            continue;
        }
        // Two hits inside one item hand over the same lines once; the second is already visible.
        let mut shown_span: Option<(usize, usize)> = None;
        for &line in file.hits.iter().take(caps.file) {
            let span = excerpt(file, line);
            if shown_span == Some(span) {
                covered += 1;
                shown.insert(at);
                continue;
            }
            shown_span = Some(span);
            let block = block(file, &raw, &label, line);
            let rows = block.lines().count();
            if rows >= left {
                out.push_str(&format!(
                    "[+{} more hits; narrow the query]\n",
                    total - covered
                ));
                return out;
            }
            left -= rows;
            covered += 1;
            shown.insert(at);
            out.push_str(&block);
        }
        if file.hits.len() > caps.file {
            out.push_str(&format!(
                "  {label} [+{} more in this file]\n",
                file.hits.len() - caps.file
            ));
        }
    }
    out
}

/// Files from `at` on that still hold hits: what a budget cut left undrawn.
fn touched_after(files: &[scan::File], at: usize) -> usize {
    files
        .iter()
        .skip(at)
        .filter(|file| !file.hits.is_empty())
        .count()
}

/// One hit with the piece it sits in, whole when the piece is short enough.
fn block(file: &scan::File, raw: &[&str], label: &str, line: usize) -> String {
    let mut out = format!("{label}:{}\n", line + 1);
    let chain = file.table.around(line);
    if !chain.is_empty() {
        let parts: Vec<String> = chain
            .iter()
            .filter_map(|(at, piece)| {
                piece
                    .exact
                    .as_deref()
                    .map(|exact| format!("{exact} ({}..{})", at + 1, piece.mate.unwrap_or(*at) + 1))
            })
            .collect();
        out.push_str(&format!("  in: {}\n", parts.join(" > ")));
    }
    let (from, to) = excerpt(file, line);
    let width = (raw.len().to_string().len()).max(3);
    for index in from..=to {
        let numbered = index == line || file.table.key(index);
        let number = if numbered {
            format!("{:>width$}", index + 1)
        } else {
            " ".repeat(width)
        };
        out.push_str(&format!("{number}|{}\n", clip_line(raw[index])));
    }
    out
}

/// The lines one hit hands over: its piece when that is short, else a window around it.
fn excerpt(file: &scan::File, line: usize) -> (usize, usize) {
    const ITEM_CAP: usize = 40;
    const WINDOW: usize = 6;
    let lines = file.text.lines().count();
    match file.table.innermost(line) {
        Some((open, piece)) => {
            let end = piece.mate.unwrap_or(open);
            if end.saturating_sub(open) <= ITEM_CAP {
                (open, end)
            } else {
                (
                    line.saturating_sub(WINDOW),
                    (line + WINDOW).min(lines.saturating_sub(1)),
                )
            }
        }
        None => (
            line.saturating_sub(WINDOW),
            (line + WINDOW).min(lines.saturating_sub(1)),
        ),
    }
}

/// One excerpt line: long lines are cut, so one line never eats the budget.
fn clip_line(text: &str) -> String {
    const CAP: usize = 240;
    match text.char_indices().nth(CAP) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

// ---- the stamps ----

/// Every file whose lines this answer showed is a file the model has now seen, under `splice`'s
/// own definition: line count and content hash at this moment.
pub(crate) fn stamp(seen: &mut Seen, files: &[scan::File], shown: &BTreeSet<usize>) {
    for &index in shown {
        let file = &files[index];
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        file.text.hash(&mut hasher);
        seen.files.insert(
            file.path.clone(),
            Stamp {
                lines: file.text.lines().count(),
                hash: hasher.finish(),
            },
        );
    }
}

// ---- shapes and paths ----

/// `A::b`, exactly two identifiers.
fn two_segments(query: &str) -> Option<(String, String)> {
    let query = query.trim();
    let (a, b) = query.split_once("::")?;
    if b.contains("::") {
        return None;
    }
    (ident(a) && ident(b)).then(|| (a.to_string(), b.to_string()))
}

/// A lone identifier.
fn single_name(query: &str) -> Option<&str> {
    let query = query.trim();
    ident(query).then_some(query)
}

fn ident(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// A path as the crate reads it: `src/server/...`.
fn rel(zone: &scan::Zone, path: &Path) -> String {
    let shown = path.strip_prefix(&zone.root).unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

fn plural(count: usize, word: &str) -> String {
    match count {
        1 => format!("1 {word}"),
        _ => format!("{count} {word}s"),
    }
}
