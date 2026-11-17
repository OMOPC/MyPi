//! What the model reads back: the verdict, then one block per edit, shaped like `grep`'s hits —
//! `path:line`, the item it sits in, and numbered source. A block that stands there carries `+`;
//! a deletion shows the lines it removed with `-`. The batch's audit hangs its lines where each
//! edit's block ended.

use serde_json::{Value, json};

use super::edit::Place;
use super::fmt::Word;

/// How many lines one side of a diff shows before its middle is folded away.
const SIDE: usize = 12;

/// How wide one source line reads inside the receipt.
const WIDE: usize = 140;

/// One edit of one call, as that call's answer shows it.
pub(crate) struct Row {
    /// Which item of the call's `edits`, 1-based; a range cut by a macro carries its owner's.
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
        /// The item holding it, innermost two, outermost first: `impl X for Y > fn z`.
        item: Option<String>,
        /// The parser's words, when its block does not parse on its own.
        blame: Option<String>,
    },

    /// It was left out: where its own lines sit now, the block it would have put there, and the
    /// item those lines are in.
    Refused {
        at: Place,
        taken: Vec<String>,
        block: Vec<String>,
        item: Option<String>,
        why: &'static str,
    },
}

/// The answer's fixed points: the file, its length, where `read` numbers lines, and whether a
/// refused edit's lines moved.
pub(crate) struct Sheet<'a> {
    pub path: &'a str,
    pub after: usize,
    pub anchors: &'a [bool],
    pub shifted: bool,
}

/// The answer's text, the memo a front end draws from, and where each edit's audit lines hang.
pub(crate) fn build(sheet: Sheet<'_>, rows: &[Row], word: &Word) -> (String, Value, Vec<usize>) {
    let Sheet {
        path,
        after,
        anchors,
        shifted,
    } = sheet;
    let landed = rows
        .iter()
        .filter(|row| matches!(row.state, State::Applied { .. }))
        .count();

    let mut out = vec![head(word, landed, rows.len(), after, shifted)];
    let mut marks: Vec<usize> = Vec::new();
    let width = after.to_string().len().max(3);
    for row in rows {
        out.push(heading(path, row));
        if let Some(item) = item(row) {
            out.push(format!("  in: {item}"));
        }
        match &row.state {
            State::Applied {
                old, new, blame, ..
            } => {
                // What stands there now; a deletion has nothing to show and says what it removed.
                if new.is_empty() {
                    out.extend(taken_side(None, old, anchors, width));
                }
                out.extend(block_side(new, width));
                if let Some(blame) = blame {
                    out.push(format!("  ! {blame}"));
                }
            }
            State::Refused {
                at,
                taken,
                block,
                why,
                ..
            } => {
                // Its lines are still in the file: they are numbered where it now finds them.
                out.extend(taken_side(
                    at.map(|(first, _)| first),
                    taken,
                    anchors,
                    width,
                ));
                out.extend(block_side(block, width));
                out.push(format!("  ! {why}"));
            }
        }
        marks.push(out.len());
    }
    if let Some(line) = verdict_note(word, rows) {
        out.push(line);
    }
    (out.join("\n"), memo(rows), marks)
}

/// The verdict, then the file's length: the whole point of the first line.
fn head(word: &Word, landed: usize, total: usize, after: usize, shifted: bool) -> String {
    let verdict = if landed == 0 {
        "FAILED, NOTHING WRITTEN"
    } else if landed < total {
        match shifted {
            true => "PARTLY FAILED, LINES SHIFTED",
            false => "PARTLY FAILED",
        }
    } else {
        match word {
            Word::Plain | Word::Formatted => "OK",
            Word::Unparsed(_) => "WRITTEN, NOT PARSED",
            Word::Undecided(_) => "WRITTEN, UNFORMATTED",
        }
    };
    format!("[Splice] {verdict} · now {after} lines")
}

/// What the formatter said about the whole text, when no edit carries the words itself.
fn verdict_note(word: &Word, rows: &[Row]) -> Option<String> {
    let blamed = rows
        .iter()
        .any(|row| matches!(&row.state, State::Applied { blame: Some(_), .. }));
    match word {
        Word::Unparsed(why) if !blamed => Some(format!("[i] the file does not parse: {why}")),
        Word::Undecided(why) => Some(format!("[i] not formatted, written as it stands: {why}")),
        _ => None,
    }
}

/// One place as `grep` names a hit: `path:line`, `path:first-last`, or a line saying the range is
/// no longer in the file.
fn where_at(path: &str, at: Place) -> String {
    match at {
        Some((first, last)) if last > first => format!("{path}:{first}-{last}"),
        Some((first, _)) => format!("{path}:{first}"),
        None => format!("{path}: their lines are not in the file as it now stands"),
    }
}

/// Where one edit sits, as `grep` names a hit. A written edit is named where it landed; a deletion
/// has no landing of its own, so it keeps the range it named.
fn heading(path: &str, row: &Row) -> String {
    match &row.state {
        State::Applied { old, new, at, .. } => {
            if new.is_empty() && !old.is_empty() {
                where_at(path, Some((row.from, row.from + old.len() - 1)))
            } else {
                where_at(path, *at)
            }
        }
        State::Refused { at, .. } => where_at(path, *at),
    }
}

fn item(row: &Row) -> Option<&str> {
    match &row.state {
        State::Applied { item, .. } => item.as_deref(),
        State::Refused { item, .. } => item.as_deref(),
    }
}

/// The lines an edit takes, unnumbered when they are gone and by their current numbers when they
/// are still in the file, where `read` would number them.
fn taken_side(
    first: Option<usize>,
    taken: &[String],
    anchors: &[bool],
    width: usize,
) -> Vec<String> {
    if taken.is_empty() {
        return vec![format!("- {:>width$}|(nothing)", "", width = width)];
    }
    let Some(first) = first else {
        return taken
            .iter()
            .map(|line| format!("- {:>width$}|{}", "", shown(line), width = width))
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
        format!("- {number:>width$}|{}", shown(line), width = width)
    })
}

/// A block, numbered inside itself.
fn block_side(block: &[String], width: usize) -> Vec<String> {
    if block.is_empty() {
        return vec![format!(
            "+ {:>width$}|(nothing: the range is gone)",
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
        format!("+ {number:>width$}|{}", shown(line), width = width)
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

/// One source line as a receipt row: its own indentation kept, its tail cut.
fn shown(line: &str) -> String {
    match line.char_indices().nth(WIDE) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
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
                    "error": why,
                }));
            }
        }
    }
    json!({ "segments": segments })
}
