//! One change: the lines it takes, the lines it puts in their place, and the snapshot it lands on.

use anyhow::{Result, anyhow};

use crate::server::agent::tools::spec::Item;

/// One change's landing place: its first and last line, both 1-based, `last < first` when it left
/// nothing there; `None` when the formatter's output could not be walked back to it.
pub(crate) type Place = Option<(usize, usize)>;

/// One change, measured in the snapshot it was written against.
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    /// Which call of the batch wrote it, in arrival order.
    pub call: usize,
    /// Which item of that call's `edits`, 1-based.
    pub index: usize,
    /// The first line it takes, 0-based.
    pub start: usize,
    /// One past the last line it takes, 0-based; `start` itself when it inserts.
    pub end: usize,
    /// The lines that stand in the range's place.
    pub block: Vec<String>,
}

impl Edit {
    /// One `edits[k]` of one call, read against a file of `total` lines.
    pub(crate) fn read(item: &Item, call: usize, index: usize, total: usize) -> Result<Self> {
        let start = item.line("start")?;
        let end = usize::try_from(item.int("end")?)
            .map_err(|_| anyhow!("`{}.end` must be a line number, 0 or more", item.label()))?;
        anyhow::ensure!(
            start <= total + 1,
            "`{}.start` is {start}, past the last line: the file has {total} lines",
            item.label()
        );
        anyhow::ensure!(
            end + 1 >= start,
            "`{}.end` is {end}, above its own `start` ({start}); `end = start - 1` inserts \
             before `start`",
            item.label()
        );
        anyhow::ensure!(
            end <= total,
            "`{}.end` is {end}, past the last line: the file has {total} lines",
            item.label()
        );
        Ok(Self {
            call,
            index,
            start: start - 1,
            end,
            block: block_of(&item.text("new")?),
        })
    }

    /// The same change with a comment around its block, so the formatter's output can be walked
    /// back to it. `at` is its position in the list handed to the formatter.
    pub(crate) fn marked(&self, at: usize) -> Self {
        let mut out = self.clone();
        let mut block = Vec::with_capacity(self.block.len() + 2);
        block.push(format!("// {} {at} open", super::fmt::MARK));
        block.extend(self.block.iter().cloned());
        block.push(format!("// {} {at} close", super::fmt::MARK));
        out.block = block;
        out
    }

    /// The lines it takes, as the snapshot holds them.
    pub(crate) fn taken<'a>(&self, lines: &'a [String]) -> &'a [String] {
        &lines[self.start.min(lines.len())..self.end.min(lines.len())]
    }
}

/// The lines of one `new` string: one trailing newline is its terminator, not an extra line.
pub(crate) fn block_of(new: &str) -> Vec<String> {
    if new.is_empty() {
        return Vec::new();
    }
    new.strip_suffix('\n')
        .unwrap_or(new)
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// Whether two changes fight over the same lines. A point (an insertion) collides with the range
/// that holds it strictly inside, and with another point on the same line.
pub(crate) fn collide(a: &Edit, b: &Edit) -> bool {
    match (a.start == a.end, b.start == b.end) {
        (true, true) => a.start == b.start,
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
        (false, false) => a.start < b.end && b.start < a.end,
    }
}

/// The snapshot with every change in place, applied from the bottom up so the lines above keep
/// their numbers while they are still being used.
pub(crate) fn spliced(text: &str, edits: &[&Edit]) -> String {
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    let mut ordered: Vec<&&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| edit.start);
    for edit in ordered.iter().rev() {
        out.splice(edit.start..edit.end, edit.block.iter().cloned());
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') && !joined.is_empty() {
        joined.push('\n');
    }
    joined
}

/// Where each change lands when nothing but the changes themselves moves the lines: the ranges are
/// ascending, so each one's own shift is the sum of the deltas above it.
pub(crate) fn shifted(edits: &[Edit]) -> Vec<Place> {
    let mut shift: isize = 0;
    let mut out = Vec::with_capacity(edits.len());
    for edit in edits {
        let first = (edit.start as isize + shift).max(0) as usize + 1;
        out.push(Some((first, first + edit.block.len() - 1)));
        shift += edit.block.len() as isize - (edit.end - edit.start) as isize;
    }
    out
}

/// `12-14` or `12`, 1-based, ascending.
pub(crate) fn parse_range(text: &str) -> Result<(usize, usize)> {
    let text = text.trim();
    let (from, to) = match text.split_once('-') {
        Some((from, to)) => (from.trim(), to.trim()),
        None => (text, text),
    };
    let number = |raw: &str| -> Result<usize> {
        raw.parse::<usize>()
            .map_err(|_| anyhow!("`range` must be `12-14` or `12`; got `{text}`"))
    };
    Ok((number(from)?, number(to)?))
}

/// Where a run of lines sits in a text: the first of the matches nearest to `near`, 0-based.
///
/// The run is compared whole, so a line that merely starts alike never answers. An empty run is a
/// place rather than a stretch: it lands where `near` says, past the text's end if it must.
pub(crate) fn locate(lines: &[&str], needle: &[&str], near: usize) -> Option<usize> {
    let last = lines.len().checked_sub(needle.len())?;
    let mut best: Option<(usize, usize)> = None;
    for start in 0..=last {
        if lines[start..start + needle.len()] != *needle {
            continue;
        }
        let distance = start.abs_diff(near);
        if best.is_none_or(|(held, _)| distance < held) {
            best = Some((distance, start));
        }
    }
    match best {
        Some((_, start)) => Some(start),
        None if needle.is_empty() => Some(near.min(lines.len())),
        None => None,
    }
}
