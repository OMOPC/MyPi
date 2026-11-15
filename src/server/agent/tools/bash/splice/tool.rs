//! The `splice` tool: replace whole lines by number, many ranges in one file.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::edit::{self, Edit, Place};
use super::fmt;
use super::items;
use super::receipt::{self, Row, State, Why};
use crate::server::agent::request::contract::ToolCall;
use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::bash::gate;
use crate::server::agent::tools::bash::halt::stopped;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::contract::env::{Answer, Env, Stamp};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;
use crate::server::platform::stop::Stop;

const PATH: usize = 0;
const EDITS: usize = 1;

const EDIT_FIELDS: &[Param] = &[
    Param::new("start", Kind::Line, "First line of the range, 1-based."),
    Param::new(
        "end",
        Kind::Line,
        "Last line of the range, inclusive. `start - 1` inserts instead of replacing.",
    ),
    Param::new(
        "new",
        Kind::Text,
        "The replacement; any number of lines. Empty deletes the range.",
    ),
];

const PARAMS: &[Param] = &[
    Param::new("path", Kind::Text, "File path."),
    Param::new(
        "edits",
        Kind::Objects(EDIT_FIELDS),
        "The ranges to replace, one item each; they may not overlap. Every call on one file in \
         one message runs as one write.",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "splice",
    about: "Replace whole lines by number in one file, many ranges at once; an empty `new` \
            removes the range. Rust is formatted and gated by rustfmt. By matched text → `edit`.",
    fields: "path and edits are always needed. Each item has `start`, `end` (both 1-based, \
             inclusive, `end = start - 1` to insert) and `new` (any number of lines, empty to \
             delete). Ranges MUST NOT overlap; the range's own lines come from a fresh `read`. \
             Every splice call on the same file in one message is one write, and an overlapping \
             later range is refused.",
    params: PARAMS,
    run,
    summary,
};

fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    // A merged run already answered for this call, or the one before it did.
    if let Some(answer) = env.handed.get(&env.current).cloned() {
        return Ok(answer.output());
    }
    let raw = args.str(PATH)?;
    let path = env.workspace.resolve(&raw, true)?;
    let plans = plans(env, &path);
    anyhow::ensure!(!plans.is_empty(), "this call is not in its own batch");
    let stamp = env.seen.files.get(&path).copied();
    let current = env.current.clone();

    let stop = env.stop.clone();
    let Some(built) = env
        .workspace
        .with_write(&path, || compute(&path, stamp, &plans, &stop))?
    else {
        // A sibling of this batch reaches its own turn, finds the stop still set, and says the same
        // thing about the same untouched file: it is said here, never kept for `env.handed`.
        return Ok(stopped(&path));
    };

    // The compiler runs after the file's lock is released: it reads the whole crate, not this file.
    // A file that does not parse would only repeat the formatter's verdict.
    let check = if built.written.is_none() || built.bare.is_some() {
        gate::Check::Skipped
    } else if stop.is_stopped() {
        gate::Check::Unchecked
    } else {
        gate::check(&path, &stop)
    };
    let answers = built.answers(&plans, &path, &check);

    if let Some(written) = &built.written {
        env.seen.files.insert(
            path.clone(),
            Stamp {
                lines: written.lines().count(),
                hash: hash(written),
            },
        );
    }
    let mut mine = None;
    for (plan, answer) in plans.iter().zip(answers) {
        if plan.call_id == current {
            mine = Some(answer);
        } else {
            env.handed.insert(plan.call_id.clone(), answer);
        }
    }
    mine.map(|answer| answer.output())
        .ok_or_else(|| anyhow!("this call is not in its own batch"))
}

impl Answer {
    /// What the front door hands the model.
    fn output(&self) -> ToolOutput {
        let mut out = match &self.memo {
            Some(memo) => ToolOutput::with_memo(self.text.clone(), memo.clone()),
            None => ToolOutput::text(self.text.clone()),
        };
        if let Some(reason) = &self.refusal {
            out = out.failure(reason.clone());
        }
        out
    }
}

/// One splice call of this batch that names the same file.
struct Plan {
    call_id: String,
    /// Its arguments, read by the same table the schema is built from.
    args: Result<Args>,
}

/// Every splice call of the batch that names `path`, in arrival order.
fn plans(env: &Env, path: &Path) -> Vec<Plan> {
    let mut out = Vec::new();
    for call in &env.batch {
        if call.name != TOOL.name || !names(call, env, path) {
            continue;
        }
        out.push(Plan {
            call_id: call.call_id.clone(),
            args: Args::fill(TOOL.params, &call.args),
        });
    }
    out
}

/// Whether this call is one of the batch's, aimed at `path`, before its arguments are trusted.
fn names(call: &ToolCall, env: &Env, path: &Path) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(&call.args) else {
        return false;
    };
    let Some(raw) = value.get("path").and_then(Value::as_str) else {
        return false;
    };
    env.workspace.resolve(raw, true).ok().as_deref() == Some(path)
}

/// One call's ranges, measured against the file as it stands.
fn read_edits(args: &Args, call: usize, total: usize) -> Result<Vec<Edit>> {
    let items = args.objects(EDITS)?;
    anyhow::ensure!(!items.is_empty(), "`edits` must name at least one range");
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        out.push(Edit::read(item, call, index + 1, total)?);
    }
    Ok(out)
}

/// What one merged run leaves behind: owned, so the compiler can read the crate after the file's
/// lock is gone.
struct Built {
    /// The file before the write.
    base: String,
    /// The file as it now stands: what was written, or the base when nothing was.
    shown: String,
    after: usize,
    /// Every edit of the batch, in arrival order, and each call's slice of them.
    edits: Vec<Edit>,
    spans: Vec<(usize, usize)>,
    /// The edits that were written, as positions into `edits`, and where each one landed.
    held: Vec<usize>,
    landings: Vec<Place>,
    /// What stopped each edit that was not written.
    dropped: Vec<(usize, Why)>,
    /// The calls whose arguments could not be read at all.
    unreadable: Vec<Option<String>>,
    /// The file did not parse before any change, so every edit was written unformatted.
    bare: Option<gate::Broken>,
    written: Option<String>,
}

impl Built {
    /// One answer per call of the batch: its own edits, its own verdicts.
    fn answers(&self, plans: &[Plan], path: &Path, check: &gate::Check) -> Vec<Answer> {
        let lines: Vec<&str> = self.shown.lines().collect();
        let base: Vec<String> = self.base.lines().map(str::to_string).collect();
        let anchors = sparse::anchors(&self.shown);
        let layout = items::Layout::analyse(&self.shown);
        let (faults, elsewhere, note) = self.split_check(path, check);
        let shown = path.display().to_string();

        (0..plans.len())
            .map(|at| {
                if let Some(why) = &self.unreadable[at] {
                    return Answer {
                        text: why.clone(),
                        memo: None,
                        refusal: Some("the call's ranges could not be read".to_string()),
                    };
                }
                let (from, to) = self.spans[at];
                let rows: Vec<Row> = (from..to)
                    .map(|index| self.row(index, &lines, &base, &layout, &faults))
                    .collect();
                let (text, memo) = receipt::build(
                    &shown,
                    self.after,
                    &anchors,
                    &rows,
                    self.bare.as_ref(),
                    &elsewhere,
                    note.as_deref(),
                );
                Answer {
                    text,
                    memo: Some(memo),
                    refusal: None,
                }
            })
            .collect()
    }

    /// One edit, as its own call's answer shows it.
    fn row(
        &self,
        at: usize,
        lines: &[&str],
        base: &[String],
        layout: &items::Layout,
        faults: &[Vec<String>],
    ) -> Row {
        let edit = &self.edits[at];
        if let Some(position) = self.held.iter().position(|held| *held == at) {
            let landing = self.landings.get(position).copied().flatten();
            let old = edit.taken(base).to_vec();
            let new = match landing {
                Some((first, last)) => take(lines, first, last),
                None => Vec::new(),
            };
            let item = landing.and_then(|(first, _)| item_of(layout, first));
            return Row {
                index: edit.index,
                from: edit.start + 1,
                state: State::Applied {
                    at: landing,
                    old,
                    new,
                    item,
                    faults: faults[at].clone(),
                },
            };
        }

        let why = self
            .dropped
            .iter()
            .find(|(index, _)| *index == at)
            .map(|(_, why)| why.clone())
            .unwrap_or(Why::Conflict);
        let needle: Vec<&str> = edit.taken(base).iter().map(String::as_str).collect();
        let near = self.near(at);
        let (place, taken) = match edit::locate(lines, &needle, near) {
            Some(first) if needle.is_empty() => (Some((first + 1, first)), Vec::new()),
            Some(first) => {
                let last = first + needle.len();
                let mut kept = Vec::with_capacity(needle.len());
                for line in &lines[first..last] {
                    kept.push((*line).to_string());
                }
                (Some((first + 1, last)), kept)
            }
            None => (
                None,
                needle.iter().map(|line| (*line).to_string()).collect(),
            ),
        };
        Row {
            index: edit.index,
            from: edit.start + 1,
            state: State::Refused {
                at: place,
                taken,
                block: edit.block.clone(),
                why,
                faults: faults[at].clone(),
            },
        }
    }

    /// Where a dropped edit's own lines start now, by the written edits above it.
    fn near(&self, at: usize) -> usize {
        let edit = &self.edits[at];
        let mut shift: isize = 0;
        for held in &self.held {
            let other = &self.edits[*held];
            if other.start >= edit.start {
                continue;
            }
            shift += other.block.len() as isize - (other.end - other.start) as isize;
        }
        (edit.start as isize + shift).max(0) as usize
    }

    /// What the compiler said, split three ways: per edit, elsewhere, and a note about the run.
    fn split_check(
        &self,
        path: &Path,
        check: &gate::Check,
    ) -> (Vec<Vec<String>>, Vec<String>, Option<String>) {
        let mut per = vec![Vec::new(); self.edits.len()];
        let faults = match check {
            gate::Check::Faults { faults, .. } => faults,
            gate::Check::TimedOut => {
                return (
                    per,
                    Vec::new(),
                    Some(format!(
                        "check did not finish in {} ms",
                        gate::CHECK_LIMIT.as_millis()
                    )),
                );
            }
            gate::Check::Unchecked => {
                return (
                    per,
                    Vec::new(),
                    Some("not type-checked: the turn was stopped".to_string()),
                );
            }
            gate::Check::Skipped | gate::Check::Clean => return (per, Vec::new(), None),
        };
        let mine = gate::crate_root(path)
            .map(|root| gate::shown(&root, path))
            .unwrap_or_default();
        let mut elsewhere = Vec::new();
        for fault in faults {
            let mut placed = false;
            if fault.file == mine {
                for (position, held) in self.held.iter().enumerate() {
                    let Some((first, last)) = self.landings.get(position).copied().flatten() else {
                        continue;
                    };
                    if first <= fault.line && fault.line <= last {
                        per[*held].push(format!("line {}: {}", fault.line, fault.message));
                        placed = true;
                        break;
                    }
                }
            }
            if !placed {
                elsewhere.push(format!("{}:{}  {}", fault.file, fault.line, fault.message));
            }
        }
        (per, elsewhere, None)
    }
}

/// The whole group: the snapshot, one write, and one answer per call. `None` = the turn was stopped
/// before the write, so the file is exactly as it was and no call has an answer.
fn compute(
    path: &Path,
    stamp: Option<Stamp>,
    plans: &[Plan],
    stop: &Stop,
) -> Result<Option<Built>> {
    let text = std::fs::read_to_string(path).context("failed to read file")?;
    let total = text.lines().count();
    if let Some(stamp) = stamp
        && (stamp.lines != total || stamp.hash != hash(&text))
    {
        bail!(
            "`{}` changed on disk after it was read; it had {} lines then and has {total} now. \
             Read it again and resend",
            path.display(),
            stamp.lines
        );
    }

    let (edits, spans, unreadable) = flatten(plans, total);
    let (kept, mut dropped) = settle_overlaps(&edits)?;
    let rust = path.extension().and_then(|e| e.to_str()) == Some("rs");

    let mut held: Vec<usize> = Vec::new();
    let mut landings: Vec<Place> = Vec::new();
    let mut bare: Option<gate::Broken> = None;
    let mut written: Option<String> = None;

    if !kept.is_empty() {
        if rust {
            let edition = gate::edition(path);
            let picked: Vec<Edit> = kept.iter().map(|at| edits[*at].clone()).collect();
            let Some(settled) = fmt::settle(&text, &picked, &edition, stop) else {
                return Ok(None);
            };
            bare = settled.bare;
            for one in settled.dropped {
                let why = match one.why {
                    fmt::Why::Syntax(why) => Why::Syntax(why.message),
                    fmt::Why::Conflict => Why::Conflict,
                };
                dropped.push((kept[one.at], why));
            }
            held = settled.kept.iter().map(|at| kept[*at]).collect();
            let kept_edits: Vec<Edit> = held.iter().map(|at| edits[*at].clone()).collect();
            if bare.is_some() {
                // Nothing was formatted, so every block sits exactly where its own length puts it.
                landings = edit::shifted(&kept_edits);
            } else {
                let refs: Vec<&Edit> = kept_edits.iter().collect();
                landings = fmt::place(&text, &refs, &edition, stop);
            }
            if stop.is_stopped() {
                return Ok(None);
            }
            if !held.is_empty() && settled.text != text {
                written = Some(settled.text);
            }
        } else {
            let picked: Vec<Edit> = kept.iter().map(|at| edits[*at].clone()).collect();
            let refs: Vec<&Edit> = picked.iter().collect();
            let out = edit::spliced(&text, &refs);
            landings = edit::shifted(&picked);
            held = kept.clone();
            if !held.is_empty() && out != text {
                written = Some(out);
            }
        }
    }

    if let Some(write) = &written {
        write_atomic(path, write)?;
    }
    let shown = written.clone().unwrap_or_else(|| text.clone());
    let after = shown.lines().count();
    Ok(Some(Built {
        base: text,
        shown,
        after,
        edits,
        spans,
        held,
        landings,
        dropped,
        unreadable,
        bare,
        written,
    }))
}

/// The batch's edits in arrival order, each call's slice of them, and the calls whose arguments
/// could not be read at all.
type Flattened = (Vec<Edit>, Vec<(usize, usize)>, Vec<Option<String>>);

fn flatten(plans: &[Plan], total: usize) -> Flattened {
    let mut edits = Vec::new();
    let mut spans = Vec::with_capacity(plans.len());
    let mut unreadable: Vec<Option<String>> = vec![None; plans.len()];
    for (at, plan) in plans.iter().enumerate() {
        let from = edits.len();
        match &plan.args {
            Ok(args) => match read_edits(args, at, total) {
                Ok(list) => edits.extend(list),
                Err(why) => unreadable[at] = Some(format!("{why:#}")),
            },
            Err(why) => unreadable[at] = Some(format!("{why:#}")),
        }
        spans.push((from, edits.len()));
    }
    (edits, spans, unreadable)
}

/// Which edits may run, and which lost to an earlier one: the kept positions, then the position and
/// reason of each refusal. Two of one call's own ranges may not overlap at all — that is the model
/// contradicting itself, and the file is left alone.
type Overlaps = (Vec<usize>, Vec<(usize, Why)>);

fn settle_overlaps(edits: &[Edit]) -> Result<Overlaps> {
    let mut kept: Vec<usize> = Vec::new();
    let mut dropped: Vec<(usize, Why)> = Vec::new();
    for at in 0..edits.len() {
        let mut clash = false;
        for held in &kept {
            if !edit::collide(&edits[*held], &edits[at]) {
                continue;
            }
            if edits[*held].call == edits[at].call {
                bail!(
                    "`edits[{}]` and `edits[{}]` of this call overlap; send one range per item",
                    edits[*held].index,
                    edits[at].index
                );
            }
            clash = true;
            break;
        }
        if clash {
            dropped.push((at, Why::Clash));
        } else {
            kept.push(at);
        }
    }
    Ok((kept, dropped))
}

/// The lines one 1-based inclusive range holds, owned.
fn take(lines: &[&str], first: usize, last: usize) -> Vec<String> {
    if lines.is_empty() || last < first {
        return Vec::new();
    }
    let first = first.clamp(1, lines.len());
    let last = last.min(lines.len());
    if last < first {
        return Vec::new();
    }
    lines[first - 1..last]
        .iter()
        .map(|line| (*line).to_string())
        .collect()
}

/// The item a landing sits in, named by its own headers, outermost first.
fn item_of(layout: &items::Layout, first: usize) -> Option<String> {
    let chain = layout.around(first.saturating_sub(1));
    let tail: Vec<&str> = chain
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(|item| item.sig.as_str())
        .collect();
    (!tail.is_empty()).then(|| tail.join(" > "))
}

/// A text's content hash, the same one `read` stamps a file with.
fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Replace whole lines by 1-based number: `from..=to` becomes `new`, any number of lines.
pub(crate) fn mass_text(
    text: &str,
    from: usize,
    to: usize,
    new: &str,
    cap: usize,
) -> Result<(String, Value, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    anyhow::ensure!(from >= 1, "`range` is 1-based; got {from}");
    anyhow::ensure!(from <= to, "`range` must count upwards; got {from}-{to}");
    anyhow::ensure!(
        to <= total,
        "line {to} does not exist; file has {total} lines"
    );
    let one = Edit {
        call: 0,
        index: 1,
        start: from - 1,
        end: to,
        block: edit::block_of(new),
    };
    let updated = edit::spliced(text, &[&one]);
    let old_text = lines[from - 1..to].join("\n");
    let mut memo = change::context(text, from, to - from + 1, cap);
    memo["atLine"] = json!(from);
    memo["old"] = json!(old_text);
    Ok((
        updated,
        memo,
        change::receipt(
            from,
            to - from + 1,
            one.block.len(),
            total - (to - from + 1) + one.block.len(),
        ),
    ))
}

/// `12-14` or `12`, 1-based, ascending.
pub(crate) fn parse_range(text: &str) -> Result<(usize, usize)> {
    edit::parse_range(text)
}

/// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "path").unwrap_or_default(), 96)
}
