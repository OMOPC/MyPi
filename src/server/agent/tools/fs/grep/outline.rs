//! The skeleton a folded answer draws: a row per piece at its span, the hit lines below them.

use std::collections::BTreeSet;

use super::scan;
use crate::server::agent::tools::contract::env::GrepLimits;
use crate::server::agent::tools::fs::lang::{self, Table};
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
    let top: Vec<usize> = file.table.roots().into_iter().map(|(at, _)| at).collect();
    let touched: Vec<usize> = top
        .iter()
        .copied()
        .filter(|at| covers(&file.table, *at, &hits))
        .collect();
    let shown: Vec<usize> = if touched.len() >= 2 { top } else { touched };
    let mut out = format!("{label}\n");
    for (first, last) in runs(&file.table, &shown) {
        let from = first;
        let to = file.table.end(last).unwrap_or(last);
        let head = if first == last {
            head_of(&file.table, first)
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
        let children = file.table.inner(first);
        let mine = within(from, to, &hits);
        for line in mine.iter().copied().filter(|line| {
            !children
                .iter()
                .any(|(at, _)| covers(&file.table, *at, &once(*line)))
        }) {
            out.push_str(&hit(width, raw, line, HIT));
        }
        // A child that holds a hit is always drawn; the others fill what `items` leaves over.
        let held = children
            .iter()
            .filter(|(at, _)| covers(&file.table, *at, &hits))
            .count();
        let mut room = caps.items.saturating_sub(held);
        for (at, _) in &children {
            let end = file.table.end(*at).unwrap_or(*at);
            if !covers(&file.table, *at, &hits) {
                if room == 0 {
                    continue;
                }
                room -= 1;
            }
            out.push_str(&row(width, *at, end, &head_of(&file.table, *at), CHILD));
            for line in mine
                .iter()
                .copied()
                .filter(|line| *at <= *line && *line <= end)
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
fn runs(table: &Table, shown: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &at in shown {
        let import = is_use(table, at);
        match out.last_mut() {
            Some((_, last)) if import && is_use(table, *last) => *last = at,
            _ => out.push((at, at)),
        }
    }
    out
}

/// Whether this piece is a `use` statement.
fn is_use(table: &Table, at: usize) -> bool {
    table
        .line(at)
        .is_some_and(|line| lang::shape(table, line) == Some("use"))
}

/// Does this piece's own lines hold any of the hits?
fn covers(table: &Table, at: usize, hits: &BTreeSet<usize>) -> bool {
    let end = table.end(at).unwrap_or(at);
    hits.iter().any(|line| at <= *line && *line <= end)
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

/// A piece's head as it reads on one row: its own name, cut to fit.
fn head_of(table: &Table, at: usize) -> String {
    let exact = table
        .line(at)
        .and_then(|line| line.exact.as_deref())
        .unwrap_or("");
    summary::clip(exact.trim(), 64)
}
