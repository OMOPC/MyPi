//! rustfmt as the gate and the formatter: one stdin round trip, and a walk that finds the change
//! that broke the file.

use super::edit::{self, Edit};
use crate::server::agent::tools::bash::gate;

/// The comment a change is wrapped in, so the formatter's output can be walked back to it.
pub(crate) use gate::MARK;

/// What the formatter could not make sense of.
pub(crate) use gate::Broken;

/// What survived a file that did not parse as a whole.
pub(crate) struct Settled {
    /// The text to write.
    pub text: String,

    /// The changes it kept, ascending.
    pub kept: Vec<usize>,

    /// The changes it dropped, with what the formatter said about each.
    pub dropped: Vec<(usize, Broken)>,

    /// What the file said before any change, when nothing could be kept.
    pub bare: Option<Broken>,
}

/// All changes at once; when the file does not parse, one change at a time until the text does.
/// Answers come from the cache, so a retry of the same text costs no new process.
pub(crate) fn settle(base: &str, edits: &[Edit], edition: &str) -> Settled {
    let all: Vec<&Edit> = edits.iter().collect();
    if let gate::Outcome::Formatted(text) = gate::format(&edit::spliced(base, &all), edition) {
        return Settled {
            text,
            kept: (0..edits.len()).collect(),
            dropped: Vec::new(),
            bare: None,
        };
    }
    let mut kept: Vec<usize> = Vec::new();
    let mut dropped: Vec<(usize, Broken)> = Vec::new();
    let mut settled: Option<String> = None;
    for index in (0..edits.len()).rev() {
        let mut trial = kept.clone();
        trial.push(index);
        trial.sort_unstable();
        let picked: Vec<&Edit> = trial.iter().map(|held| &edits[*held]).collect();
        match gate::format(&edit::spliced(base, &picked), edition) {
            gate::Outcome::Formatted(text) => {
                kept = trial;
                settled = Some(text);
            }
            gate::Outcome::Broken(why) => dropped.push((index, why)),
        }
    }
    dropped.sort_by_key(|(index, _)| *index);

    let bare = match &settled {
        None => match gate::format(base, edition) {
            gate::Outcome::Broken(why) => Some(why),
            gate::Outcome::Formatted(_) => None,
        },
        Some(_) => None,
    };
    Settled {
        text: settled.unwrap_or_else(|| base.to_string()),
        kept,
        dropped,
        bare,
    }
}

/// Where each change's own lines ended up, in the order the changes were handed over.
///
/// The text is formatted once more with a comment around each change; that comment survives, so
/// its neighbours are the exact lines the change became.
pub(crate) fn place(base: &str, edits: &[&Edit], edition: &str) -> Vec<edit::Place> {
    let marked: Vec<Edit> = edits
        .iter()
        .enumerate()
        .map(|(at, change)| change.marked(at))
        .collect();
    let picked: Vec<&Edit> = marked.iter().collect();
    let gate::Outcome::Formatted(text) = gate::format(&edit::spliced(base, &picked), edition) else {
        return vec![None; edits.len()];
    };

    let mut blocks: Vec<edit::Place> = vec![None; edits.len()];
    let mut open: Option<(usize, usize)> = None;
    let mut at = 0usize;
    for line in text.lines() {
        let Some((index, edge)) = mark_at(line) else {
            at += 1;
            continue;
        };
        match edge {
            Edge::Open => open = Some((index, at)),
            Edge::Close => {
                if let Some((index, first)) = open.take()
                    && let Some(slot) = blocks.get_mut(index)
                {
                    *slot = Some((first + 1, at));
                }
            }
        }
        at += 1;
    }
    blocks
}

/// One edge of a change's comment.
enum Edge {
    Open,
    Close,
}

/// The change index and edge this line carries, when it is one of the wrapper comments.
fn mark_at(line: &str) -> Option<(usize, Edge)> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("//")?.trim_start();
    let mut words = rest.split_whitespace();
    if words.next()? != gate::MARK {
        return None;
    }
    let index = words.next()?.parse().ok()?;
    let edge = match words.next()? {
        "open" => Edge::Open,
        "close" => Edge::Close,
        _ => return None,
    };
    Some((index, edge))
}
