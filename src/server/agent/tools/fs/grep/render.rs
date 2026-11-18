//! The answer: the type section, the module section, the x-ray, the hits, and the stamps.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::server::agent::tools::contract::env::{GrepLimits, License, Seen};
use crate::server::agent::tools::fs::lang::{self, Relation, Source};

use super::{outline, scan};

// The census's own room, its header included: the one bar a grep answer knows.
const LEDGER_MAX: usize = 500;

/// One answered call's text, and the files whose lines it showed.
pub(crate) fn answer(
    query: &str,
    mode: &str,
    files: &[scan::File],
    zone: &scan::Zone,
    caps: GrepLimits,
    lavish: bool,
    misses: &[String],
) -> (String, BTreeSet<usize>) {
    let total: usize = files.iter().map(|file| file.hits.len()).sum();
    let touched = files.iter().filter(|file| !file.hits.is_empty()).count();
    let counted = if total <= caps.deep {
        1
    } else if total <= caps.fold {
        2
    } else {
        3
    };
    let tier = if lavish && counted > 1 {
        counted - 1
    } else {
        counted
    };
    let mut out = format!(
        "grep \"{query}\": {}, {}, {mode}, tier {tier}\n",
        plural(total, "hit"),
        plural(touched, "file"),
    );
    if counted >= 2 {
        let hint = if lavish {
            "narrow the scope"
        } else {
            "go lavish, or narrow the scope"
        };
        out.push_str(&format!("[folded: {hint}]\n"));
    }

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
    out.push_str(&hits(files, zone, counted, caps, lavish, &mut shown));
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

// The pool a call draws from: files in name order, lines top down, each file's own cap first.
fn pool(files: &[scan::File], caps: GrepLimits) -> Vec<(usize, usize)> {
    let mut pool: Vec<(usize, usize)> = Vec::new();
    for (at, file) in files.iter().enumerate() {
        if file.hits.is_empty() {
            continue;
        }
        for &line in file.hits.iter().take(caps.file) {
            pool.push((at, line));
        }
    }
    pool
}

// One file's capped hits: the plain sketch hands this over as one set of trees.
fn capped(file: &scan::File, caps: GrepLimits) -> Vec<usize> {
    file.hits.iter().take(caps.file).copied().collect()
}

/// The hits at the density their count allows: whole items, sketches, then the census. Lavish
/// lifts the pool's front one tier; the rest of the pool stays at its own tier's density.
fn hits(
    files: &[scan::File],
    zone: &scan::Zone,
    counted: usize,
    caps: GrepLimits,
    lavish: bool,
    shown: &mut BTreeSet<usize>,
) -> String {
    let pool = pool(files, caps);
    let total: usize = files.iter().map(|file| file.hits.len()).sum();
    let mut out = String::new();

    // Counted tier 1: every hit hands over its piece whole, whatever its size; two hits inside
    // one piece hand it over once, the second is already visible.
    if counted == 1 {
        let mut handed: BTreeSet<(usize, usize, usize)> = BTreeSet::new();
        for &(at, line) in &pool {
            let span = item_span(&files[at], line);
            if !handed.insert((at, span.0, span.1)) {
                shown.insert(at);
                continue;
            }
            out.push_str(&item(&files[at], line, &rel(zone, &files[at].path)));
            shown.insert(at);
        }
        tails(files, zone, caps, &mut out);
        return out;
    }

    // Counted tier 2, no lavish: one sketch per file, the trees its capped hits hang in.
    if counted == 2 && !lavish {
        for (at, file) in files.iter().enumerate() {
            if file.hits.is_empty() {
                continue;
            }
            let raw: Vec<&str> = file.text.lines().collect();
            let want = capped(file, caps);
            out.push_str(&outline::file(file, &raw, &rel(zone, &file.path), &want));
            shown.insert(at);
        }
        tails(files, zone, caps, &mut out);
        return out;
    }

    // Counted tier 2 with lavish: the pool's front hands its pieces whole, the rest keeps its
    // sketch. The pool is one foldMax deep by construction, so all of it is drawn.
    if counted == 2 {
        let front = pool.len().min(caps.deep);
        if pool.len() > front {
            out.push_str(&format!(
                "[whole: {front} hits, sketched: {} hits]\n",
                pool.len() - front
            ));
        }
        let mut handed: BTreeSet<(usize, usize, usize)> = BTreeSet::new();
        for &(at, line) in pool.iter().take(front) {
            let span = item_span(&files[at], line);
            if !handed.insert((at, span.0, span.1)) {
                shown.insert(at);
                continue;
            }
            out.push_str(&item(&files[at], line, &rel(zone, &files[at].path)));
            shown.insert(at);
        }
        let mut rest: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &(at, line) in pool.iter().skip(front) {
            rest.entry(at).or_default().push(line);
        }
        for (at, want) in rest {
            let file = &files[at];
            let raw: Vec<&str> = file.text.lines().collect();
            out.push_str(&outline::file(file, &raw, &rel(zone, &file.path), &want));
            shown.insert(at);
        }
        tails(files, zone, caps, &mut out);
        return out;
    }

    // Counted tier 3, no lavish: the census alone, from its own room.
    if !lavish {
        out.push_str("[hits] too wide to list; per file\n");
        out.push_str(&census(files, zone, shown, LEDGER_MAX.saturating_sub(1)));
        return out;
    }

    // Counted tier 3 with lavish: the pool's front is sketched out of the census's own room, the
    // rest counts per file, and what the sketches took the census cannot draw. A file's front
    // hits sketch together, as one call of the skeleton.
    let front = pool.len().min(caps.fold);
    out.push_str(&format!("[sketched: {front} of {total} hits]\n"));
    out.push_str("[hits] too wide to list; per file\n");
    let mut left = LEDGER_MAX.saturating_sub(2);
    let mut rest: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(at, line) in pool.iter().take(front) {
        rest.entry(at).or_default().push(line);
    }
    for (at, want) in rest {
        let file = &files[at];
        let raw: Vec<&str> = file.text.lines().collect();
        let block = outline::file(file, &raw, &rel(zone, &file.path), &want);
        left = left.saturating_sub(block.lines().count());
        shown.insert(at);
        out.push_str(&block);
    }
    out.push_str(&census(files, zone, shown, left));
    out
}

// One line per file whose hits ran past its own cap: a cap that bites names what it left.
fn tails(files: &[scan::File], zone: &scan::Zone, caps: GrepLimits, out: &mut String) {
    for file in files.iter() {
        let over = file.hits.len().saturating_sub(caps.file);
        if over > 0 {
            out.push_str(&format!(
                "  {} [+{over} more in this file]\n",
                rel(zone, &file.path)
            ));
        }
    }
}

// The census's rows: one per file that holds a hit, widest first, within the room left.
fn census(
    files: &[scan::File],
    zone: &scan::Zone,
    shown: &mut BTreeSet<usize>,
    room: usize,
) -> String {
    let mut rows: Vec<(usize, String, usize)> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.hits.is_empty())
        .map(|(at, file)| (at, rel(zone, &file.path), file.hits.len()))
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2));
    let mut out = String::new();
    for (at, path, count) in rows.iter().take(room) {
        out.push_str(&format!("  {path}  {count}\n"));
        shown.insert(*at);
    }
    if rows.len() > room {
        out.push_str(&format!("  [+{} more files]\n", rows.len() - room));
    }
    out
}

/// The span one hit hands over: its innermost piece's lines, or the hit line alone when no piece
/// holds it; two hits of one span hand their lines over once.
fn item_span(file: &scan::File, line: usize) -> (usize, usize) {
    match file.table.innermost(line) {
        Some((open, piece)) => (open, piece.mate.unwrap_or(open)),
        None => (line, line),
    }
}

/// One hit with the piece it sits in, whole: the breadcrumb, then every line from the piece's
/// head to its close; a hit no piece holds hands over its own line alone.
fn item(file: &scan::File, line: usize, label: &str) -> String {
    let raw: Vec<&str> = file.text.lines().collect();
    let width = (raw.len().to_string().len()).max(3);
    let mut out = format!("{label}:{}\n", line + 1);
    let Some((open, piece)) = file.table.innermost(line) else {
        out.push_str(&format!("{:>width$}|{}\n", line + 1, clip(raw[line])));
        return out;
    };
    let close = piece.mate.unwrap_or(open);
    let parts: Vec<String> = file
        .table
        .around(line)
        .iter()
        .filter_map(|(at, piece)| {
            piece
                .exact
                .as_deref()
                .map(|exact| format!("{exact} ({}..{})", at + 1, piece.mate.unwrap_or(*at) + 1))
        })
        .collect();
    out.push_str(&format!("  in: {}\n", parts.join(" > ")));
    for index in open..=close {
        let numbered = index == line || file.table.key(index);
        let number = if numbered {
            format!("{:>width$}", index + 1)
        } else {
            " ".repeat(width)
        };
        out.push_str(&format!("{number}|{}\n", clip(raw[index])));
    }
    out
}

/// One drawn line: long lines are cut, so one line never drowns the item around it.
fn clip(text: &str) -> String {
    const CAP: usize = 240;
    match text.char_indices().nth(CAP) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

// ---- the stamps ----

/// One answered call's stamp: the files whose lines an answer showed, into `seen`.
pub(crate) fn stamp(seen: &mut Seen, files: &[scan::File], shown: &BTreeSet<usize>) {
    for at in shown {
        let file = &files[*at];
        seen.files
            .insert(file.path.clone(), License::of(&file.text));
    }
}

// ---- shapes and paths ----

/// A path as the crate reads it: `src/server/...`.
fn rel(zone: &scan::Zone, path: &Path) -> String {
    let shown = path.strip_prefix(&zone.root).unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

/// One count with its noun: one hit, two hits.
fn plural(count: usize, word: &str) -> String {
    match count {
        1 => format!("1 {word}"),
        _ => format!("{count} {word}s"),
    }
}
