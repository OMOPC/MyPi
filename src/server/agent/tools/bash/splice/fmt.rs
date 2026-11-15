//! rustfmt as the gate and the formatter: one stdin round trip, and a walk that finds the change
//! that broke the file.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, anyhow};

use super::edit::{self, Edit};

/// The comment a change is wrapped in, so the formatter's output can be walked back to it.
pub(crate) const MARK: &str = "mypi:splice";

/// What the formatter could not make sense of.
#[derive(Debug, Clone)]
pub(crate) struct Broken {
    /// The line it pointed at, 1-based, in the text it was given.
    pub line: Option<usize>,
    /// What it said, in one line.
    pub message: String,
}

impl Broken {
    /// The line inside one change's own block, when the formatter pointed there.
    ///
    /// The text it saw holds the lines above the change untouched, then the block.
    pub(crate) fn inside(&self, edit: &Edit) -> Option<usize> {
        let line = self.line?;
        let first = edit.start + 1;
        let last = edit.start + edit.block.len();
        (first <= line && line <= last).then_some(line - edit.start)
    }
}

/// One change's turn through the formatter.
pub(crate) enum Outcome {
    /// The text as rustfmt left it.
    Formatted(String),
    Broken(Broken),
}

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

/// Format one text, or say what stopped it.
pub(crate) fn format(text: &str, edition: &str) -> Result<Outcome> {
    let bin = binary();
    let mut child = Command::new(&bin)
        .args(["--edition", edition, "--emit", "stdout", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to run `{}`; put rustfmt beside this executable or on PATH",
                bin.display()
            )
        })?;
    {
        let mut stdin = child.stdin.take().expect("stdin is piped");
        stdin
            .write_all(text.as_bytes())
            .context("failed to hand the text to rustfmt")?;
    }
    let out = child.wait_with_output().context("rustfmt did not finish")?;
    if out.status.success() {
        let text =
            String::from_utf8(out.stdout).map_err(|_| anyhow!("rustfmt answered no text"))?;
        return Ok(Outcome::Formatted(text));
    }
    Ok(Outcome::Broken(parse(&String::from_utf8_lossy(
        &out.stderr,
    ))))
}

/// All changes at once; when the file does not parse, from the bottom up one at a time until it
/// does. The first station that parses is the answer, and the dropped ones carry their own error.
pub(crate) fn settle(base: &str, edits: &[Edit], edition: &str) -> Result<Settled> {
    let all: Vec<&Edit> = edits.iter().collect();
    if let Outcome::Formatted(text) = format(&edit::spliced(base, &all), edition)? {
        return Ok(Settled {
            text,
            kept: (0..edits.len()).collect(),
            dropped: Vec::new(),
            bare: None,
        });
    }
    let mut kept: Vec<usize> = Vec::new();
    let mut dropped: Vec<(usize, Broken)> = Vec::new();
    let mut settled: Option<String> = None;
    for index in (0..edits.len()).rev() {
        let mut trial = kept.clone();
        trial.push(index);
        trial.sort_unstable();
        let picked: Vec<&Edit> = trial.iter().map(|held| &edits[*held]).collect();
        match format(&edit::spliced(base, &picked), edition)? {
            Outcome::Formatted(text) => {
                kept = trial;
                settled = Some(text);
            }
            Outcome::Broken(why) => dropped.push((index, why)),
        }
    }
    dropped.sort_by_key(|(index, _)| *index);
    let bare = match (&settled, format(base, edition)?) {
        (None, Outcome::Broken(why)) => Some(why),
        _ => None,
    };
    Ok(Settled {
        text: settled.unwrap_or_else(|| base.to_string()),
        kept,
        dropped,
        bare,
    })
}

/// Where each change's own lines ended up, in the order the changes were handed over.
///
/// The text is formatted once more with a comment around each change; that comment survives, so
/// its neighbours are the exact lines the change became.
pub(crate) fn place(base: &str, edits: &[&Edit], edition: &str) -> Result<Vec<edit::Place>> {
    let marked: Vec<Edit> = edits
        .iter()
        .enumerate()
        .map(|(at, change)| change.marked(at))
        .collect();
    let picked: Vec<&Edit> = marked.iter().collect();
    let Outcome::Formatted(text) = format(&edit::spliced(base, &picked), edition)? else {
        return Ok(vec![None; edits.len()]);
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
    Ok(blocks)
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
    if words.next()? != MARK {
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

/// The `rustfmt` to run: the one beside this executable first, then whatever PATH holds.
fn binary() -> PathBuf {
    let name = if cfg!(windows) {
        "rustfmt.exe"
    } else {
        "rustfmt"
    };
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside = dir.join(name);
        if beside.is_file() {
            return beside;
        }
    }
    PathBuf::from(name)
}

/// The crate's edition, from the nearest `Cargo.toml` above the file. A missing or unreadable
/// manifest leaves 2021, which accepts everything an older edition would.
pub(crate) fn edition(path: &Path) -> String {
    for dir in path.ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        return std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|text| edition_of(&text))
            .unwrap_or_else(|| "2021".to_string());
    }
    "2021".to_string()
}

/// The first `edition = "…"` in a manifest.
fn edition_of(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("edition") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if matches!(value, "2015" | "2018" | "2021" | "2024") {
            return Some(value.to_string());
        }
    }
    None
}

/// rustfmt's stderr, cut to the one line and the line number a model can act on.
fn parse(stderr: &str) -> Broken {
    let mut message: Option<String> = None;
    let mut line = None;
    for raw in stderr.lines() {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            line = rest.split(':').nth(1).and_then(|spot| spot.parse().ok());
            continue;
        }
        if message.is_some() {
            continue;
        }
        let cleaned = trimmed.strip_prefix("error: ").or_else(|| {
            trimmed
                .strip_prefix("error[")
                .and_then(|rest| rest.split_once("]: ").map(|(_, tail)| tail))
        });
        if let Some(cleaned) = cleaned {
            message = Some(cleaned.trim_end_matches(':').to_string());
        }
    }
    Broken {
        line,
        message: message.unwrap_or_else(|| "rustfmt refused the text".to_string()),
    }
}
