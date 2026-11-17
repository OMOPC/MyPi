//! The skeleton a folded answer draws: a row per item at its span, the hit lines below them.

use std::collections::BTreeSet;

use super::rust::Item;
use super::scan;
use crate::server::agent::tools::contract::env::GrepLimits;
use crate::server::agent::tools::summary;

/// Columns a line number takes beside its neighbour.
const MIN_WIDTH: usize = 3;

/// How far one child row and one hit row sit under the block they belong to.
const CHILD: usize = 4;
const HIT: usize = 8;

/// One file's skeleton: the block the hits fall in, or the whole file once they spread over
/// several.
pub(crate) fn file(file: &scan::File, raw: &[&str], label: &str, caps: GrepLimits) -> String {
    let hits: BTreeSet<usize> = file.hits.iter().copied().take(caps.file).collect();
    let width = raw.len().to_string().len().max(MIN_WIDTH);
    let top = file.layout.roots();
    let touched: Vec<usize> = top
        .iter()
        .copied()
        .filter(|at| covers(&file.layout.items[*at], &hits))
        .collect();
    let shown: Vec<usize> = if touched.len() >= 2 { top } else { touched };
    let mut out = format!("{label}\n");
    for (first, last) in runs(file, &shown) {
        let (from, to) = span_of(file, first, last);
        let head = if first == last {
            head_of(raw[file.layout.items[first].head])
        } else {
            "use …".to_string()
        };
        out.push_str(&row(width, from, to, &head, 0));
        if first != last {
            for line in within(from, to, &hits) {
                out.push_str(&hit(width, raw, line, HIT));
            }
            continue;
        }
        let block = &file.layout.items[first];
        let children = file.layout.inner(first);
        let mine = within(block.head, block.close, &hits);
        for line in mine
            .iter()
            .copied()
            .filter(|line| !children.iter().any(|child| covers(child, &once(*line))))
        {
            out.push_str(&hit(width, raw, line, HIT));
        }
        // A child that holds a hit is always drawn; the others fill what `items` leaves over.
        let held = children.iter().filter(|child| covers(child, &hits)).count();
        let mut room = caps.items.saturating_sub(held);
        for child in &children {
            if !covers(child, &hits) {
                if room == 0 {
                    continue;
                }
                room -= 1;
            }
            out.push_str(&row(
                width,
                child.head,
                child.close,
                &head_of(raw[child.head]),
                CHILD,
            ));
            for line in mine
                .iter()
                .copied()
                .filter(|at| child.head <= *at && *at <= child.close)
            {
                out.push_str(&hit(width, raw, line, CHILD + HIT));
            }
        }
        let drawn = held + caps.items.saturating_sub(held);
        if children.len() > drawn {
            out.push_str(&format!(
                "{:CHILD$}[+{} more]\n",
                "",
                children.len() - drawn
            ));
        }
    }
    out
}

/// The blocks to draw, with a run of `use` statements folded into one.
fn runs(file: &scan::File, shown: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &at in shown {
        let import = file.layout.items[at].kind == "use";
        match out.last_mut() {
            Some((_, last)) if import && file.layout.items[*last].kind == "use" => *last = at,
            _ => out.push((at, at)),
        }
    }
    out
}

/// The lines a run covers: its first item's head to its last item's close.
fn span_of(file: &scan::File, first: usize, last: usize) -> (usize, usize) {
    let start = file.layout.items[first].head;
    let end = file.layout.items[last].close;
    (start, end)
}

/// Does this item's own lines hold any of the hits?
fn covers(item: &Item, hits: &BTreeSet<usize>) -> bool {
    hits.iter()
        .any(|line| item.head <= *line && *line <= item.close)
}

fn within(from: usize, to: usize, hits: &BTreeSet<usize>) -> Vec<usize> {
    hits.iter()
        .copied()
        .filter(|line| from <= *line && *line <= to)
        .collect()
}

fn once(line: usize) -> BTreeSet<usize> {
    let mut set = BTreeSet::new();
    set.insert(line);
    set
}

/// One block: its two ends, then its head.
fn row(width: usize, start: usize, end: usize, head: &str, indent: usize) -> String {
    format!(
        "{:indent$}{:>width$}..{:>width$} | {head}\n",
        "",
        start + 1,
        end + 1,
    )
}

/// One hit: its own line number, then its text.
fn hit(width: usize, raw: &[&str], line: usize, indent: usize) -> String {
    format!(
        "{:indent$}{:>width$} | {}\n",
        "",
        line + 1,
        summary::clip(raw[line], 160),
    )
}

/// A block's head as it reads on one row: the source line, its brace dropped.
fn head_of(line: &str) -> String {
    summary::clip(line.trim().trim_end_matches('{').trim_end(), 64)
}
