//! The file's skeleton: one row per item at its span, and the seams a change or an error sits in.

use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::summary;

/// One item: the lines it covers and the head it reads by.
pub(crate) struct Block {
    /// Its first line, 0-based.
    pub(crate) start: usize,
    /// Its last line, 0-based.
    pub(crate) end: usize,
    /// Its first line, cut to something that fits beside its numbers.
    pub(crate) head: String,
}

/// Every brace pair's closing line, per opening line; a line that opens twice keeps the wider pair.
pub(crate) fn closes(code: &[String]) -> Vec<Option<usize>> {
    let mut out: Vec<Option<usize>> = vec![None; code.len()];
    for (open, close) in sparse::frames(code) {
        if let Some(slot) = out.get_mut(open) {
            *slot = Some(slot.map_or(close, |held| held.max(close)));
        }
    }
    out
}

/// The items at the shallowest indent of one line range.
pub(crate) fn items(
    raw: &[&str],
    code: &[String],
    closes: &[Option<usize>],
    from: usize,
    to: usize,
) -> Vec<Block> {
    if raw.is_empty() || from > to {
        return Vec::new();
    }
    let to = to.min(raw.len().saturating_sub(1));
    let mut width = usize::MAX;
    for (at, line) in raw.iter().enumerate().take(to + 1).skip(from) {
        if no_item(line, code.get(at).map(String::as_str)) {
            continue;
        }
        width = width.min(indent(line));
    }
    if width == usize::MAX {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = from;
    while at <= to {
        let line = raw[at];
        if indent(line) != width || no_item(line, code.get(at).map(String::as_str)) {
            at += 1;
            continue;
        }
        let end = block_end(code, closes, at, to);
        out.push(Block {
            start: at,
            end,
            head: head_of(line),
        });
        at = end + 1;
    }
    out
}

/// The innermost item that holds a 0-based line.
pub(crate) fn innermost(
    raw: &[&str],
    code: &[String],
    closes: &[Option<usize>],
    at: usize,
) -> Option<Block> {
    let mut found: Option<Block> = None;
    let mut from = 0;
    let mut to = raw.len().saturating_sub(1);
    loop {
        let Some(block) = items(raw, code, closes, from, to)
            .into_iter()
            .find(|block| block.start <= at && at <= block.end)
        else {
            return found;
        };
        from = block.start + 1;
        to = block.end;
        found = Some(block);
        if from > to {
            return found;
        }
    }
}

/// A line that carries no item: blank, a closing brace, a comment, an attribute.
fn no_item(line: &str, code: Option<&str>) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('}')
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("#[")
        || trimmed.starts_with("#![")
    {
        return true;
    }
    code.is_some_and(|code| change::head_of(code.trim_start()).is_none())
}

/// Where the item starting at `at` ends: its closing brace, or the line that finishes it.
fn block_end(code: &[String], closes: &[Option<usize>], at: usize, limit: usize) -> usize {
    for (offset, line) in code.iter().enumerate().take(limit + 1).skip(at) {
        if line.contains('{') {
            return closes[offset].unwrap_or(limit).min(limit);
        }
        if line.contains(';') {
            return offset;
        }
    }
    at
}

/// A width-aligned skeleton row.
pub(crate) fn row(width: usize, start: usize, end: usize, head: &str, note: &str) -> String {
    format!(
        "{:>width$}..{:>width$} | {head}{note}",
        start + 1,
        end + 1,
        width = width
    )
}

/// Append the rows a budget may still take, and say so when the rest is gone.
pub(crate) fn trim(out: &mut Vec<String>, rows: Vec<String>, room: usize) {
    let room = room.saturating_sub(out.len());
    if room == 0 {
        return;
    }
    if rows.len() > room {
        out.extend(rows.into_iter().take(room.saturating_sub(1)));
        out.push("[outline cut: the file is longer]".to_string());
        return;
    }
    out.extend(rows);
}

/// The lines around one range: two above, its own edges, two below, the middle folded when the
/// range is long.
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

/// A line's head, cut to something that fits beside its numbers.
pub(crate) fn head_of(line: &str) -> String {
    let trimmed = line.trim().trim_end_matches('{').trim_end();
    summary::clip(trimmed, 64)
}

/// Columns of leading whitespace; a tab counts as four.
pub(crate) fn indent(line: &str) -> usize {
    let mut width = 0;
    for ch in line.chars() {
        match ch {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}
