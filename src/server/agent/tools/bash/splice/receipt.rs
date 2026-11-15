//! What the model reads back: the file's skeleton at its new line numbers, the seams the changes
//! sit in, and every change that was left out with the text that caused it.

use serde_json::{Value, json};

use super::edit::{Edit, Place};
use super::fmt::Broken;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::bash::skeleton;

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
        let closes = skeleton::closes(&code);
        let top = skeleton::items(&raw, &code, &closes, 0, raw.len().saturating_sub(1));
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
            rows.push(skeleton::row(
                width, block.start, block.end, &block.head, mark,
            ));
            if active != Some(at) {
                continue;
            }
            for child in skeleton::items(&raw, &code, &closes, block.start + 1, block.end) {
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
                    skeleton::row(width, child.start, child.end, &child.head, &note)
                ));
            }
        }
        skeleton::trim(&mut out, rows, OUTLINE_LINES);

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
            seams.extend(skeleton::seam(&raw, first, last, width));
        }
        skeleton::trim(&mut out, seams, OUTLINE_LINES);
    }

    for one in rejected {
        let edit = &edits[one.index];
        out.push(format!(
            "--- edit {} left out · was {} ---",
            edit.index,
            edit.was()
        ));
        match &one.why {
            Why::Broken(why) => match why.inside(edit.start, edit.block.len()) {
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
                && let Some(at) = why.inside(edit.start, edit.block.len())
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
