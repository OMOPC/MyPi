// The answer: the type section, the module section, the x-ray, the items, and the stamps.

use std::collections::BTreeSet;

use crate::server::agent::tools::contract::env::{GrepLimits, License, Seen};
use crate::server::agent::tools::fs::lang::graph::parts;
use crate::server::agent::tools::fs::lang::{self, Relation, Source};
use crate::server::agent::tools::fs::skeleton;
use crate::server::agent::tools::summary;

use super::scan;

// The answer's own room at tier 3, its header included.
const LEDGER_MAX: usize = 500;

// One drawn source line's own width: past this it is cut.
const LINE_MAX: usize = 240;

// One answered call's text, and the files whose lines it showed.
pub(crate) fn answer(
    query: &str,
    mode: &str,
    files: &[scan::File],
    zone: &scan::Zone,
    caps: GrepLimits,
    lavish: bool,
    misses: &[String],
) -> (String, BTreeSet<usize>) {
    let hits: usize = files
        .iter()
        .flat_map(|file| &file.items)
        .map(|item| item.hits.len())
        .sum();
    let items: usize = files.iter().map(|file| file.items.len()).sum();
    let touched = files.iter().filter(|file| !file.items.is_empty()).count();
    let counted: usize = if items <= caps.deep {
        1
    } else if items <= caps.fold {
        2
    } else {
        3
    };
    // Lavish lifts the whole answer one tier: no front, no mixture of two densities.
    let tier = if lavish {
        counted.saturating_sub(1).max(1)
    } else {
        counted
    };
    let mut out = format!(
        "grep \"{query}\": {}, {}, {}, {mode}, tier {tier}\n",
        parts::plural(hits, "hit"),
        parts::plural(items, "item"),
        parts::plural(touched, "file"),
    );
    if tier > 1 {
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
        let relation = (relations.scan)(&sources, &zone.root, query, caps.members, caps.impls);
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
    draw(files, zone, tier, &mut out, &mut shown);
    for miss in misses {
        out.push_str(miss);
        out.push('\n');
    }
    (out, shown)
}

// A relation's rows as one block: each heading, then its lines, in the language's own order.
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

// The hits at the tier's density: whole items, then outlines, then the census alone.
fn draw(
    files: &[scan::File],
    zone: &scan::Zone,
    tier: usize,
    out: &mut String,
    shown: &mut BTreeSet<usize>,
) {
    if tier == 3 {
        out.push_str("[hits] too wide to list; per file\n");
        let room = LEDGER_MAX.saturating_sub(out.lines().count());
        out.push_str(&census(files, zone, shown, room));
        return;
    }
    for (at, file) in files.iter().enumerate() {
        if file.items.is_empty() {
            continue;
        }
        let raw: Vec<&str> = file.text.lines().collect();
        let width = skeleton::width(raw.len());
        let label = parts::rel(&zone.root, &file.path);
        let block = if tier == 1 {
            whole(file, &raw, width, &label)
        } else {
            outline(file, &raw, width, &label)
        };
        out.push_str(&block);
        shown.insert(at);
    }
}

// Tier 1: every line of every item; only key lines and hit lines carry a number.
fn whole(file: &scan::File, raw: &[&str], width: usize, label: &str) -> String {
    let mut out = String::new();
    for item in &file.items {
        out.push_str(&format!("{label}:{}\n", item.open + 1));
        let above: Vec<String> = file
            .table
            .around(item.open)
            .iter()
            .filter(|(at, _)| *at != item.open)
            .filter_map(|(at, piece)| {
                let exact = piece.exact.as_deref()?;
                let close = piece.mate.unwrap_or(*at) + 1;
                Some(format!("{exact} ({}..{close})", at + 1))
            })
            .collect();
        if !above.is_empty() {
            out.push_str(&format!("  in: {}\n", above.join(" > ")));
        }
        for index in item.open..=item.close {
            let text = summary::cut(raw[index], LINE_MAX);
            let mark = (item.hits.contains(&index) || file.table.key(index)).then_some(index + 1);
            out.push_str(&skeleton::numbered(width, mark, &text));
            out.push('\n');
        }
    }
    out
}

// Tier 2: the outline of every item — its ancestors' head lines once per file, then its key lines
// and hit lines, the item's own head row carrying where the piece closes.
fn outline(file: &scan::File, raw: &[&str], width: usize, label: &str) -> String {
    let mut out = format!("{label}\n");
    let mut heads: BTreeSet<usize> = BTreeSet::new();
    for item in &file.items {
        for (at, _) in file.table.around(item.open) {
            if at != item.open && heads.insert(at) {
                let text = summary::cut(raw[at], LINE_MAX);
                out.push_str(&skeleton::numbered(width, Some(at + 1), &text));
                out.push('\n');
            }
        }
        for index in item.open..=item.close {
            if !file.table.key(index) && !item.hits.contains(&index) {
                continue;
            }
            let mut text = summary::cut(raw[index], LINE_MAX);
            if index == item.open && item.close > item.open {
                text.push_str(&format!(" ..{}", item.close + 1));
            }
            out.push_str(&skeleton::numbered(width, Some(index + 1), &text));
            out.push('\n');
        }
    }
    out
}

// The census's rows: one per file that holds an item, widest first, within the room left.
fn census(
    files: &[scan::File],
    zone: &scan::Zone,
    shown: &mut BTreeSet<usize>,
    room: usize,
) -> String {
    if room == 0 {
        return String::new();
    }
    let mut rows: Vec<(usize, String, usize)> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.items.is_empty())
        .map(|(at, file)| (at, parts::rel(&zone.root, &file.path), file.items.len()))
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2));
    let more = rows.len() > room;
    let keep = if more { room.saturating_sub(1) } else { room };
    let mut out = String::new();
    for (at, path, count) in rows.iter().take(keep) {
        out.push_str(&format!("  {path}  {}\n", parts::plural(*count, "item")));
        shown.insert(*at);
    }
    if more {
        out.push_str(&format!("  [+{} more files]\n", rows.len() - keep));
    }
    out
}

// One answered call's stamp: the files whose lines an answer showed, into `seen`.
pub(crate) fn stamp(seen: &mut Seen, files: &[scan::File], shown: &BTreeSet<usize>) {
    for at in shown {
        let file = &files[*at];
        seen.files
            .insert(file.path.clone(), License::of(&file.text));
    }
}
