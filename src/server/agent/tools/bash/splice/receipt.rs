//! What the model reads back: the file's skeleton at its new line numbers, the seams the changes
//! sit in, and every change that was left out with the text that caused it.

use serde_json::{Value, json};

use super::edit::{Edit, Place};
use super::fmt::Broken;
use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::summary;

/// How many lines the skeleton and the seams may take together.
const OUTLINE_LINES: usize = 40;

/// Why one change was left out of the write.
pub(crate) enum Why {
    /// The formatter could not make sense of what the file became.
    Broken(Broken),
    /// It fights an earlier change for the same lines.
    Clash(String),
}

impl Why {
    /// What a model has to read.
    fn text(&self) -> &str {
        match self {
            Why::Broken(why) => &why.message,
            Why::Clash(why) => why,
        }
    }
}

/// One change left out, in its own call's numbering.
pub(crate) struct Rejected {
    /// Its position among this call's `edits`, 0-based.
    pub index: usize,
    pub why: Why,
}

/// One top-level item: its range and its head.
struct Block {
    start: usize,
    end: usize,
    head: String,
}

/// The receipt and the memo a front end draws from.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    base: &str,
    text: &str,
    edits: &[Edit],
    kept: &[usize],
    blocks: &[Place],
    rejected: &[Rejected],
    path: &str,
    rust: bool,
    outline: bool,
    after: usize,
) -> (String, Value) {
    let mut head = if rejected.is_empty() {
        format!("[Splice OK] {} edit(s) applied", kept.len())
    } else {
        format!(
            "[Splice Partial] {}/{} edits applied, {} left out",
            kept.len(),
            edits.len(),
            rejected.len()
        )
    };
    if rust {
        head.push_str(", formatted by rustfmt");
    }
    head.push_str(&format!("; {after} lines: {path}"));

    let mut out = vec![head];
    if outline && !kept.is_empty() {
        let raw: Vec<&str> = text.lines().collect();
        let code = sparse::stripped(text);
        let closes = closes(&code);
        let top = items(&raw, &code, &closes, 0, raw.len().saturating_sub(1));
        let width = after.to_string().len().max(3);

        let active = kept.iter().find_map(|index| {
            let at = blocks.get(*index).copied().flatten()?.0;
            top.iter()
                .position(|block| block.start < at && at <= block.end + 1)
        });

        out.push("--- Skeleton (post-format line numbers) ---".to_string());
        let mut rows: Vec<String> = Vec::new();
        for (at, block) in top.iter().enumerate() {
            let mark = if active == Some(at) {
                " <- modified"
            } else {
                ""
            };
            rows.push(row(width, block.start, block.end, &block.head, mark));
            if active != Some(at) {
                continue;
            }
            for child in items(&raw, &code, &closes, block.start + 1, block.end) {
                let names: Vec<String> = kept
                    .iter()
                    .filter(|index| {
                        blocks
                            .get(**index)
                            .copied()
                            .flatten()
                            .is_some_and(|(first, last)| {
                                child.start < last && first <= child.end + 1
                            })
                    })
                    .map(|index| format!("edit {}", edits[*index].index))
                    .collect();
                let note = if names.is_empty() {
                    String::new()
                } else {
                    format!(" <- {}", names.join(", "))
                };
                rows.push(format!(
                    "    {}",
                    row(width, child.start, child.end, &child.head, &note)
                ));
            }
        }
        trim(&mut out, rows);

        let mut seams = Vec::new();
        for index in kept {
            let Some((first, last)) = blocks.get(*index).copied().flatten() else {
                continue;
            };
            seams.push(format!(
                "--- edit {} · was {} · now {} ---",
                edits[*index].index,
                edits[*index].was(),
                range_name(first, last)
            ));
            seams.extend(seam(&raw, first, last, width));
        }
        trim(&mut out, seams);
    }

    for one in rejected {
        let edit = &edits[one.index];
        out.push(format!(
            "--- edit {} left out · was {} ---",
            edit.index,
            edit.was()
        ));
        match &one.why {
            Why::Broken(why) => match why.inside(edit) {
                Some(at) => out.push(format!("your block, line {at}: {}", why.message)),
                None => out.push(format!(
                    "the file stopped parsing at {}: {}",
                    why.line
                        .map(|line| format!("line {line}"))
                        .unwrap_or_else(|| "an unnamed line".to_string()),
                    why.message
                )),
            },
            Why::Clash(why) => out.push(why.clone()),
        }
        if edit.block.is_empty() {
            out.push("(the block was empty: the range is simply gone)".to_string());
        }
        for (at, line) in edit.block.iter().enumerate() {
            out.push(format!("{:>4} | {line}", at + 1));
        }
    }

    let memo = memo(base, text, edits, kept, blocks, rejected);
    (out.join("\n"), memo)
}

/// One range as a reader names it: `197..215`, or where nothing stands.
fn range_name(first: usize, last: usize) -> String {
    match last.checked_sub(first) {
        None => format!("nothing at line {first}"),
        Some(0) => format!("line {first}"),
        Some(_) => format!("{first}..{last}"),
    }
}

/// A width-aligned skeleton row.
fn row(width: usize, start: usize, end: usize, head: &str, note: &str) -> String {
    format!(
        "{:>width$}..{:>width$} | {head}{note}",
        start + 1,
        end + 1,
        width = width
    )
}

/// Append the rows an outline may still take, and say so when the rest is gone.
fn trim(out: &mut Vec<String>, rows: Vec<String>) {
    let room = OUTLINE_LINES.saturating_sub(out.len());
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

/// The lines around one change: two above, its own edges, two below, the middle folded away when
/// the change is long.
fn seam(raw: &[&str], first: usize, last: usize, width: usize) -> Vec<String> {
    let n = raw.len();
    if n == 0 {
        return Vec::new();
    }
    let first = first.clamp(1, n);
    let last = last.clamp(1, n).max(first);
    let mut picks: Vec<Option<usize>> = Vec::new();
    for at in first.saturating_sub(2)..first {
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

/// The memo a front end draws from: one segment per change, in the call's order.
fn memo(
    base: &str,
    text: &str,
    edits: &[Edit],
    kept: &[usize],
    blocks: &[Place],
    rejected: &[Rejected],
) -> Value {
    let base_lines: Vec<String> = base.lines().map(str::to_string).collect();
    let text_lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut segments = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let mut segment = json!({
            "edit": edit.index,
            "fromLine": edit.start + 1,
            "atLine": edit.start + 1,
        });
        if kept.contains(&index) {
            let (first, last) = blocks
                .get(index)
                .copied()
                .flatten()
                .unwrap_or((edit.start + 1, edit.start + edit.block.len()));
            segment["state"] = json!("applied");
            segment["atLine"] = json!(first);
            segment["old"] = json!(edit.taken(&base_lines).join("\n"));
            segment["new"] = json!(take(&text_lines, first, last));
        } else if let Some(one) = rejected.iter().find(|one| one.index == index) {
            segment["state"] = json!("rejected");
            segment["new"] = json!(edit.block.join("\n"));
            segment["error"] = json!(one.why.text());
            if let Why::Broken(why) = &one.why
                && let Some(at) = why.inside(edit)
            {
                segment["errorLine"] = json!(at);
            }
        } else {
            continue;
        }
        segments.push(segment);
    }
    json!({ "segments": segments })
}

/// The lines one 1-based inclusive range holds, joined; empty when the range is empty.
fn take(lines: &[String], first: usize, last: usize) -> String {
    let first = first.clamp(1, lines.len() + 1);
    let last = last.min(lines.len());
    if last < first {
        return String::new();
    }
    lines[first - 1..last].join("\n")
}

/// Every brace pair's closing line, per opening line; a line that opens twice keeps the wider pair.
fn closes(code: &[String]) -> Vec<Option<usize>> {
    let mut out: Vec<Option<usize>> = vec![None; code.len()];
    for (open, close) in sparse::frames(code) {
        if let Some(slot) = out.get_mut(open) {
            *slot = Some(slot.map_or(close, |held| held.max(close)));
        }
    }
    out
}

/// The items at the shallowest indent of one line range.
fn items(
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

/// A line's head, cut to something that fits beside its numbers.
fn head_of(line: &str) -> String {
    let trimmed = line.trim().trim_end_matches('{').trim_end();
    summary::clip(trimmed, 64)
}

/// Columns of leading whitespace; a tab counts as four.
fn indent(line: &str) -> usize {
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
