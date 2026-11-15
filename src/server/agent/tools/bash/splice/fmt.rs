//! rustfmt as the gate: one stdin round trip per candidate text, and the trial that says which
//! edit the file stopped parsing for.

use super::edit::{self, Edit};
use crate::server::agent::tools::bash::gate;
use crate::server::platform::stop::Stop;

/// The comment a change is wrapped in, so its own lines can be found again after formatting.
pub(crate) use gate::MARK;

/// What the formatter could not make sense of.
pub(crate) use gate::Broken;

/// One edit that did not join the write, and what stopped it.
pub(crate) struct Refused {
    /// Its position among the edits handed over.
    pub at: usize,
    pub why: Why,
}

/// Why one edit did not join the write.
pub(crate) enum Why {
    /// On its own, this block makes the file stop parsing.
    Syntax(Broken),

    /// It parses on its own, but not beside the edits that came with it.
    Conflict,
}

/// What survived a file that did not parse as a whole.
pub(crate) struct Settled {
    /// The text to write.
    pub text: String,

    /// The edits it kept, ascending.
    pub kept: Vec<usize>,

    /// The edits it dropped, with what stopped each.
    pub dropped: Vec<Refused>,

    /// What the file said before any change, when it did not parse even then.
    pub bare: Option<Broken>,
}

/// Every edit at once; failing that, each one on its own, so a refusal blames the block that caused
/// it and not the one beside it. Answers come from the cache, so a retry of the same text costs no
/// new process. `None` = the turn was stopped, so the formatter decided nothing.
pub(crate) fn settle(base: &str, edits: &[Edit], edition: &str, stop: &Stop) -> Option<Settled> {
    let all: Vec<&Edit> = edits.iter().collect();
    match gate::format(&edit::spliced(base, &all), edition, stop) {
        gate::Outcome::Formatted(text) => {
            return Some(Settled {
                text,
                kept: (0..edits.len()).collect(),
                dropped: Vec::new(),
                bare: None,
            });
        }
        gate::Outcome::Stopped => return None,
        gate::Outcome::Broken(_) => {}
    }

    // A file that did not parse before the change says nothing about any block: every edit is
    // written as it stands, unformatted, and the answer says so.
    match gate::format(base, edition, stop) {
        gate::Outcome::Broken(bare) => {
            return Some(Settled {
                text: edit::spliced(base, &all),
                kept: (0..edits.len()).collect(),
                dropped: Vec::new(),
                bare: Some(bare),
            });
        }
        gate::Outcome::Stopped => return None,
        gate::Outcome::Formatted(_) => {}
    }

    // The file parsed before the change, so one block at a time says whose fault the break is.
    let mut alone = vec![false; edits.len()];
    let mut dropped: Vec<Refused> = Vec::new();
    for (at, edit) in edits.iter().enumerate() {
        match gate::format(&edit::spliced(base, &[edit]), edition, stop) {
            gate::Outcome::Formatted(_) => alone[at] = true,
            gate::Outcome::Broken(why) => dropped.push(Refused {
                at,
                why: Why::Syntax(why),
            }),
            gate::Outcome::Stopped => return None,
        }
    }

    let keep: Vec<usize> = (0..edits.len()).filter(|at| alone[*at]).collect();
    let picked: Vec<&Edit> = keep.iter().map(|at| &edits[*at]).collect();
    match gate::format(&edit::spliced(base, &picked), edition, stop) {
        gate::Outcome::Formatted(text) => {
            return Some(Settled {
                text,
                kept: keep,
                dropped,
                bare: None,
            });
        }
        gate::Outcome::Stopped => return None,
        gate::Outcome::Broken(_) => {}
    }

    // Each one parses alone, but not all of them together: keep the largest set that does, and say
    // of the rest that it was the company, not the block.
    let mut kept: Vec<usize> = Vec::new();
    let mut settled: Option<String> = None;
    for at in keep.iter().rev() {
        let mut trial = kept.clone();
        trial.push(*at);
        trial.sort_unstable();
        let picked: Vec<&Edit> = trial.iter().map(|held| &edits[*held]).collect();
        match gate::format(&edit::spliced(base, &picked), edition, stop) {
            gate::Outcome::Formatted(text) => {
                kept = trial;
                settled = Some(text);
            }
            gate::Outcome::Broken(_) => dropped.push(Refused {
                at: *at,
                why: Why::Conflict,
            }),
            gate::Outcome::Stopped => return None,
        }
    }
    dropped.sort_by_key(|one| one.at);
    Some(Settled {
        text: settled.unwrap_or_else(|| base.to_string()),
        kept,
        dropped,
        bare: None,
    })
}

/// Where each change's own lines ended up, in the order the changes were handed over.
///
/// The text is formatted once more with a comment around each change; that comment takes no line
/// number of its own, so its neighbours are the exact lines the change became.
pub(crate) fn place(base: &str, edits: &[&Edit], edition: &str, stop: &Stop) -> Vec<edit::Place> {
    let marked: Vec<Edit> = edits
        .iter()
        .enumerate()
        .map(|(at, change)| change.marked(at))
        .collect();
    let picked: Vec<&Edit> = marked.iter().collect();
    let gate::Outcome::Formatted(text) = gate::format(&edit::spliced(base, &picked), edition, stop)
    else {
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
