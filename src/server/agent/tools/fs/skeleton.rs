// The skeleton a report draws: numbered source lines, the pieces of a line range, the seam around
// an error.

use super::lang::Table;
use crate::server::agent::tools::summary;

// One piece, as a report reads it.
pub(crate) struct Block {
    // Its first line, 0-based.
    pub(crate) start: usize,

    // Its last line, 0-based.
    pub(crate) end: usize,

    // Its name, cut to something that fits beside its numbers.
    pub(crate) head: String,
}

// How wide a line-number gutter is for a file of `total` lines.
pub(crate) fn width(total: usize) -> usize {
    total.to_string().len().max(3)
}

// One source line behind its gutter: the number right-aligned, then the pipe, then the text. A
// blank number leaves the same width of spaces, so both kinds start in one column.
pub(crate) fn numbered(width: usize, mark: Option<usize>, line: &str) -> String {
    match mark {
        Some(at) => format!("{at:>width$}|{line}"),
        None => format!("{:>width$}|{line}", ""),
    }
}

// The innermost piece holding a 0-based line.
pub(crate) fn innermost(table: &Table, at: usize) -> Option<Block> {
    let (open, line) = table.innermost(at)?;
    Some(Block {
        start: open,
        end: line.mate.unwrap_or(open),
        head: head_of(line.exact.as_deref()),
    })
}

// The lines around one range: two above, its own edges, two below, the middle folded when the
// range is long.
pub(crate) fn seam(raw: &[&str], first: usize, last: usize, width: usize) -> Vec<String> {
    let n = raw.len();
    if n == 0 {
        return Vec::new();
    }
    let first = first.clamp(1, n);
    let last = last.clamp(1, n).max(first);
    let mut picks: Vec<Option<usize>> = Vec::new();

    // Line 1 has no lines above it: the walk may not start below the first one.
    for at in first.saturating_sub(2).max(1)..first {
        picks.push(Some(at));
    }
    let inside: Vec<usize> = (first..=last).collect();
    let long = inside.len() > 8;
    for at in inside.iter().take(if long { 3 } else { inside.len() }) {
        picks.push(Some(*at));
    }
    if long {
        picks.push(None);
        for at in &inside[inside.len() - 3..] {
            picks.push(Some(*at));
        }
    }
    for at in last + 1..=(last + 2).min(n) {
        picks.push(Some(at));
    }
    picks
        .into_iter()
        .map(|at| match at {
            Some(at) => format!("{:>width$} | {}", at, raw[at - 1], width = width),
            None => "…".to_string(),
        })
        .collect()
}

// A piece's own name, cut to something that fits beside its numbers.
fn head_of(exact: Option<&str>) -> String {
    summary::clip(exact.unwrap_or("").trim(), 64)
}
