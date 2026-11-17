//! One change: the lines it takes, the lines it puts in their place, and the snapshot it lands on.

use anyhow::{Result, anyhow, bail};

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
    /// A range a `@@CUT(a..b)@@` asked to be removed; carries no `new` of its own.
    pub cut: bool,

    /// The words the `start` line must open with, if the model gave them: a drift check.
    pub expect_start: String,

    /// The words the `end` line must open with, if any: a drift check.
    pub expect_end: String,
}

impl Edit {
    /// One `edits[k]` of one call, read against a snapshot of `base` lines. A `@@COPY(a..b)@@` in
    /// `new` becomes the snapshot's own lines; a `@@CUT(a..b)@@` does the same and adds the change
    /// that takes those lines away.
    pub(crate) fn read(item: &Item, call: usize, index: usize, base: &[&str]) -> Result<Vec<Self>> {
        let total = base.len();
        let (start, expect_start) = item.axis("start")?;
        let (end, expect_end) = item.axis("end")?;
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
        let (text, cuts) = rewritten(&item.text("new")?, base, item)?;
        let mut out = vec![Self {
            call,
            index,
            start: start - 1,
            end,
            block: block_of(&text),
            cut: false,
            expect_start,
            expect_end,
        }];
        for (first, last) in cuts {
            anyhow::ensure!(
                !(first < end && start - 1 < last),
                "`{}.new` cuts {first}..{last}, which its own range already takes; a cut belongs \
                 to another edit",
                item.label()
            );
            out.push(Self {
                call,
                index,
                start: first,
                end: last,
                block: Vec::new(),
                cut: true,
                expect_start: String::new(),
                expect_end: String::new(),
            });
        }
        Ok(out)
    }

    /// The same change with a comment around its block, so the formatter's output can be walked
    /// back to it. `tag` names this run and `at` its position in the list handed to the formatter.
    pub(crate) fn marked(&self, tag: &str, at: usize) -> Self {
        let mut out = self.clone();
        let mut block = Vec::with_capacity(self.block.len() + 2);
        block.push(format!("// {tag} {at} open"));
        block.extend(self.block.iter().cloned());
        block.push(format!("// {tag} {at} close"));
        out.block = block;
        out
    }

    /// The lines it takes, as the snapshot holds them.
    pub(crate) fn taken<'a>(&self, lines: &'a [String]) -> &'a [String] {
        &lines[self.start.min(lines.len())..self.end.min(lines.len())]
    }
}

/// A line with its leading whitespace gone and its inner runs of whitespace folded to one space,
/// so spacing never hides a drift match.
pub(crate) fn flat(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `line` opens with the words `expect`, compared as a flat prefix: the model walks the
/// line from its front and everything it sent must match there, in order; it may stop short of the
/// whole line, so `pub fn myfunc` confirms `pub fn myfunc() -> Result<()> {`.
pub(crate) fn opens(line: &str, expect: &str) -> bool {
    let line = flat(line);
    let expect = flat(expect);
    expect.is_empty() || line.starts_with(&expect)
}
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

/// One call's ranges, measured against the snapshot it was written against. A range may be cut
/// once per call: two macros cutting into one another is the model contradicting itself.
pub(crate) fn call_edits(items: &[Item], call: usize, base: &[&str]) -> Result<Vec<Edit>> {
    anyhow::ensure!(!items.is_empty(), "`edits` must name at least one range");
    let mut out: Vec<Edit> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        out.extend(Edit::read(item, call, index + 1, base)?);
    }
    let mut cuts: Vec<(usize, usize, usize)> = out
        .iter()
        .filter(|edit| edit.cut)
        .map(|edit| (edit.index, edit.start, edit.end))
        .collect();
    cuts.sort_by_key(|(_, start, _)| *start);
    for pair in cuts.windows(2) {
        let (first, _, end) = pair[0];
        let (second, other, _) = pair[1];
        anyhow::ensure!(
            end <= other,
            "`edits[{first}].new` and `edits[{second}].new` cut overlapping lines; a range may be \
             cut once"
        );
    }
    Ok(out)
}

/// A `new` string with its macros filled in, and the 0-based ranges it wants cut away.
///
/// `@@COPY(a..b)@@` and `@@CUT(a..b)@@` name lines of the snapshot, 1-based inclusive, and are
/// filled with those lines as they stand. Filling is one pass: a token inside the copied lines is
/// copied, never followed.
fn rewritten(new: &str, base: &[&str], item: &Item) -> Result<(String, Vec<(usize, usize)>)> {
    let label = item.label();
    let mut text = String::with_capacity(new.len());
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut at = 0usize;
    while let Some(found) = new[at..].find("@@") {
        let from = at + found;
        let tail = &new[from + 2..];
        let (cut, kind, body_start) = if tail.starts_with("COPY(") {
            (false, "COPY", from + 2 + "COPY(".len())
        } else if tail.starts_with("CUT(") {
            (true, "CUT", from + 2 + "CUT(".len())
        } else {
            bail!(
                "`{label}.new` holds `@@` at byte {from} that starts no `@@COPY(a..b)@@` or \
                 `@@CUT(a..b)@@`"
            );
        };
        let rest = &new[body_start..];
        let closing = rest.find(")@@").ok_or_else(|| {
            anyhow!("`{label}.new` opens a macro at byte {from} with no closing `)@@`")
        })?;
        let body = &rest[..closing];
        let (first, last) = body.split_once("..").ok_or_else(|| {
            anyhow!("`{label}.new` has `@@{kind}({body})@@` at byte {from}; a range reads `a..b`")
        })?;
        let number = |raw: &str| -> Result<usize> {
            raw.trim()
                .parse::<usize>()
                .map_err(|_| anyhow!("`{label}.new` names `{body}`; a range reads `a..b`"))
        };
        let first = number(first)?;
        let last = number(last)?;
        anyhow::ensure!(
            first >= 1 && last >= first,
            "`{label}.new` names `{body}`; a range counts upwards from 1"
        );
        anyhow::ensure!(
            last <= base.len(),
            "`{label}.new` names `{body}`, past the last line: the file has {} lines",
            base.len()
        );
        text.push_str(&new[at..from]);
        text.push_str(&base[first - 1..last].join("\n"));
        if cut {
            cuts.push((first - 1, last));
        }
        at = body_start + closing + 3;
    }
    text.push_str(&new[at..]);
    Ok((text, cuts))
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

