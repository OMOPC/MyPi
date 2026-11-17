//! The answer: the type section, the module section, the x-ray, the hits, and the stamps.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::server::agent::tools::contract::env::{GrepLimits, Seen, Stamp};
use crate::server::agent::tools::fs::lang::{self, Relation, Source};

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
    // The three sections ask a language's own relation index; a language that names none skips them.
    let relations = files.iter().find_map(|file| lang::relations(&file.table));
    if let Some(relations) = relations {
        let sources: Vec<Source> = files
            .iter()
            .map(|file| Source {
                path: &file.path,
                text: &file.text,
                table: &file.table,
            })
            .collect();
        let relation = (relations.scan)(&sources, &zone.root, query, caps.items, caps.impls);
        for &index in &relation.used {
            shown.insert(index);
        }
        let block = paint(&relation);
        if !block.is_empty() {
            out.push('\n');
            out.push_str(&block);
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

// ---- the relation block ----

/// A relation's rows as one block: each heading, then its lines, in the language's own order.
fn paint(relation: &Relation) -> String {
    let mut out = String::new();
    for row in &relation.rows {
        match &row.heading {
            Some(heading) => out.push_str(heading),
            None => out.push_str(&row.text),
        }
        out.push('\n');
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
