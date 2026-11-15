//! What the model reads back: the edits that were left out, each with the lines it takes and the
//! block it puts there, then the ones that were written, each at the lines it landed on.
//!
//! A line number here is a global one only where `read` would number that line; a block is
//! numbered inside itself. Nothing else is printed.

use serde_json::{Value, json};

use super::edit::Place;
use crate::server::agent::tools::bash::gate::Broken;

/// How many lines one side of a diff shows before its middle is folded away.
const SIDE: usize = 12;

/// How wide one source line reads inside a diff.
const WIDE: usize = 140;

/// Why one edit was left out of the write.
#[derive(Debug, Clone)]
pub(crate) enum Why {
    /// The file stopped parsing once this block was in it; the formatter's own words.
    Syntax(String),

    /// It fights an earlier splice call on this file for the same lines.
    Clash,

    /// It parses on its own, but not beside the other edits of this message.
    Conflict,
}

impl Why {
    fn says(&self) -> String {
        match self {
            Why::Syntax(why) => why.clone(),
            Why::Clash => "overlaps an earlier splice call on this file".to_string(),
            Why::Conflict => "parses on its own, not beside the other edits here".to_string(),
        }
    }
}

/// One edit of one call, as that call's answer shows it.
pub(crate) struct Row {
    /// Which item of the call's `edits`, 1-based.
    pub index: usize,
    /// The first line it named, 1-based, in the file as it was before the write.
    pub from: usize,
    pub state: State,
}

pub(crate) enum State {
    /// It was written: where it landed, the lines it took and the ones that stand there now.
    Applied {
        at: Place,
        old: Vec<String>,
        new: Vec<String>,
        /// The item holding it, outermost first: `impl X for Y > fn z`.
        item: Option<String>,
        faults: Vec<String>,
    },

    /// It was left out: where its own lines sit now, and the block it would have put there.
    Refused {
        at: Place,
        taken: Vec<String>,
        block: Vec<String>,
        why: Why,
        faults: Vec<String>,
    },
}

/// The answer's text and the memo a front end draws from.
pub(crate) fn build(
    path: &str,
    after: usize,
    anchors: &[bool],
    rows: &[Row],
    bare: Option<&Broken>,
    elsewhere: &[String],
    note: Option<&str>,
) -> (String, Value) {
    let landed = rows
        .iter()
        .filter(|row| matches!(row.state, State::Applied { .. }))
        .count();

    let mut out = vec![head(landed, rows.len(), after, bare), path.to_string()];
    let width = after.to_string().len().max(2);
    let mut sections: Vec<String> = Vec::new();
    if let Some(why) = bare {
        sections.push(bare_note(why));
    }
    for row in rows
        .iter()
        .filter(|row| matches!(row.state, State::Refused { .. }))
    {
        sections.push(refused(row, anchors, width));
    }
    if let Some(section) = applied(rows) {
        sections.push(section);
    }
    if !elsewhere.is_empty() {
        sections.push(elsewhere_note(elsewhere));
    }
    if let Some(note) = note {
        sections.push(format!("[i] {note}"));
    }
    for section in sections {
        out.push(String::new());
        out.push(section);
    }
    (out.join("\n"), memo(rows))
}

/// The verdict, then the file's length: the whole point of the first line.
fn head(landed: usize, total: usize, after: usize, bare: Option<&Broken>) -> String {
    let verdict = match bare {
        Some(why) if why.line.is_some() => "WRITTEN, FILE ALREADY BROKEN",
        Some(_) => "WRITTEN, UNVERIFIED",
        None if landed == total => "OK",
        None if landed == 0 => "FAILED, NOTHING WRITTEN",
        None => "PARTLY FAILED, LINES SHIFTED",
    };
    format!("[Splice] {verdict} · now {after} lines")
}

/// Why nothing could be judged, and what was written anyway.
fn bare_note(why: &Broken) -> String {
    match why.line {
        Some(line) => format!(
            "[i] the file did not parse before this change (line {line}): {}; every edit was written unformatted",
            why.message
        ),
        None => format!(
            "[i] the formatter could not judge the file before this change: {}; every edit was written unformatted",
            why.message
        ),
    }
}

/// One refused edit: what it takes, what it would put there, and what stopped it.
fn refused(row: &Row, anchors: &[bool], width: usize) -> String {
    let State::Refused {
        at,
        taken,
        block,
        why,
        faults,
    } = &row.state
    else {
        return String::new();
    };
    let named = range(*at)
        .unwrap_or_else(|| "their lines are not in the file as it now stands".to_string());
    let mut out = vec![format!("[!] edit {} · {named}", row.index)];

    out.extend(taken_side(*at, taken, anchors, width));
    out.extend(block_side(block, width));
    out.push(format!("  ! {}", why.says()));
    for fault in faults {
        out.push(format!("  ! {fault}"));
    }
    out.join("\n")
}

/// The lines an edit takes, by their current numbers, and only where `read` would number them.
fn taken_side(at: Place, taken: &[String], anchors: &[bool], width: usize) -> Vec<String> {
    if taken.is_empty() {
        return vec![format!("  - {:>width$} | (nothing)", "", width = width)];
    }
    let Some((first, _)) = at else {
        return taken
            .iter()
            .map(|line| format!("  - {:>width$} | {}", "", shown(line), width = width))
            .collect();
    };
    let mut picked: Vec<(usize, &str)> = Vec::new();
    for (offset, line) in taken.iter().enumerate() {
        let number = first + offset;
        if anchors.get(number - 1).copied().unwrap_or(false) {
            picked.push((number, line));
        }
    }
    // A range that holds no anchor still took lines: its own ends are what a reader needs.
    if picked.is_empty() {
        picked.push((first, &taken[0]));
        if taken.len() > 1 {
            picked.push((first + taken.len() - 1, &taken[taken.len() - 1]));
        }
    }
    fold(picked, |number, line| {
        format!("  - {number:>width$} | {}", shown(line), width = width)
    })
}

/// The block an edit would put there, numbered inside itself.
fn block_side(block: &[String], width: usize) -> Vec<String> {
    if block.is_empty() {
        return vec![format!(
            "  + {:>width$} | (nothing: the range is gone)",
            "",
            width = width
        )];
    }
    let rows: Vec<(usize, &str)> = block
        .iter()
        .enumerate()
        .map(|(at, line)| (at + 1, line.as_str()))
        .collect();
    fold(rows, |number, line| {
        format!("  + {number:>width$} | {}", shown(line), width = width)
    })
}

/// A side of the diff, its middle folded away once it grows past what one answer may take.
fn fold(rows: Vec<(usize, &str)>, row: impl Fn(usize, &str) -> String) -> Vec<String> {
    if rows.len() <= SIDE {
        return rows.into_iter().map(|(at, line)| row(at, line)).collect();
    }
    let half = SIDE / 2;
    let mut out: Vec<String> = rows[..half]
        .iter()
        .map(|(at, line)| row(*at, line))
        .collect();
    out.push("  …".to_string());
    out.extend(
        rows[rows.len() - half..]
            .iter()
            .map(|(at, line)| row(*at, line)),
    );
    out
}

/// One source line as a diff row: its own indentation kept, its tail cut.
fn shown(line: &str) -> String {
    match line.char_indices().nth(WIDE) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

/// The edits that were written, one line each.
fn applied(rows: &[Row]) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for row in rows {
        let State::Applied {
            at, item, faults, ..
        } = &row.state
        else {
            continue;
        };
        let named = range(*at).unwrap_or_else(|| "written, landing not found".to_string());
        let mut line = format!("[+] edit {} · {named}", row.index);
        if let Some(item) = item {
            line.push_str(" in ");
            line.push_str(item);
        }
        out.push(line);
        for fault in faults {
            out.push(format!("  ! {fault}"));
        }
    }
    (!out.is_empty()).then(|| out.join("\n"))
}

/// The compiler's own words about files this call did not touch.
fn elsewhere_note(elsewhere: &[String]) -> String {
    let mut out = vec![format!(
        "[!] check · {} error(s) elsewhere, none of them in these edits",
        elsewhere.len()
    )];
    for line in elsewhere.iter().take(3) {
        out.push(format!("  {line}"));
    }
    if elsewhere.len() > 3 {
        out.push(format!("  (+{} more)", elsewhere.len() - 3));
    }
    out.join("\n")
}

/// A range as a reader names it: `84-86`, `84`, where nothing stands, or `None` when it has no
/// place in the file as it now stands.
fn range(at: Place) -> Option<String> {
    let (first, last) = at?;
    Some(match last.checked_sub(first) {
        None => format!("nothing at line {first}"),
        Some(0) => format!("line {first}"),
        Some(_) => format!("{first}-{last}"),
    })
}

/// The memo a front end draws from: one segment per edit, in the call's order.
fn memo(rows: &[Row]) -> Value {
    let mut segments = Vec::new();
    for row in rows {
        match &row.state {
            State::Applied { at, old, new, .. } => {
                segments.push(json!({
                    "edit": row.index,
                    "state": "applied",
                    "fromLine": row.from,
                    "atLine": at.map(|(first, _)| first).unwrap_or(row.from),
                    "old": old.join("\n"),
                    "new": new.join("\n"),
                }));
            }
            State::Refused {
                at,
                taken,
                block,
                why,
                ..
            } => {
                // The segment's own lines are the ones it takes now, not the ones it named before.
                let now = at.map(|(first, _)| first).unwrap_or(row.from);
                segments.push(json!({
                    "edit": row.index,
                    "state": "rejected",
                    "fromLine": now,
                    "atLine": now,
                    "old": taken.join("\n"),
                    "new": block.join("\n"),
                    "error": why.says(),
                }));
            }
        }
    }
    json!({ "segments": segments })
}
