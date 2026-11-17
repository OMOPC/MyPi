//! The `splice` tool: replace whole lines by number, many ranges in one file.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::edit::{self, Edit, Place};
use super::fmt::{self, Word};
use super::receipt::{self, Row, State};
use crate::server::agent::request::contract::ToolCall;
use crate::server::agent::tools::contract::env::{Answer, Env, Shell, Slot, Stamp};
use crate::server::agent::tools::contract::halt::stopped;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::fs::change;
use crate::server::agent::tools::fs::gate;
use crate::server::agent::tools::fs::lang::{self, Table};
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;
use crate::server::platform::stop::Stop;

const PATH: usize = 0;
const EDITS: usize = 1;
const UNFOLD: usize = 2;

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
        "The replacement; any number of lines. Empty deletes the range. `@@COPY(a..b)@@` fills \
         in lines a..b of the file as it stands before this message; `@@CUT(a..b)@@` fills them \
         in and also removes them from where they were.",
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
    Param::df(
        "unfold",
        Kind::Flag,
        "List every fault of the batch's shared check instead of folding it.",
        "false",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "splice",
    about: "Batch 1-based inclusive line edits in one file. Supports `@@COPY(a..b)@@` / \
            `@@CUT(a..b)@@` line macros. Auto-formats Rust via rustfmt \
            (writes and warns on syntax error). By matched text → `edit`. One `cargo check` \
            per batch, reported on the last write.",
    fields: "Requires `path` and non-overlapping `edits` (`{start, end, new}`). \
             Delete: `new: \"\"`. Insert before `start`: `end = start - 1`. \
             Same-file calls in one turn merge into one write against pre-turn line numbers.\n\
             Macros in `new` (1-based inclusive, pre-turn snapshot, single-pass): \
             `@@COPY(a..b)@@` copies lines `a..b`; `@@CUT(a..b)@@` moves them (auto-deletes `a..b`).\n\
             `unfold` lifts the shared check's fold for every written file of the batch.",
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
    // The compiler waits for the batch's last write: the audit finishes every written answer then.
    let answers = built.answers(&plans, &path);

    if let Some(written) = &built.written {
        env.seen.files.insert(
            path.clone(),
            Stamp {
                lines: written.lines().count(),
                hash: hash(written),
            },
        );
    }
    // Every written Rust call hands its shell to the audit: its faults hang under its edits when
    // the batch's check has run, and the crate's report rides on the last written answer.
    let audit = (built.written.is_some() && !matches!(built.word, Word::Plain))
        .then(|| gate::crate_root(&path))
        .flatten()
        .map(|root| {
            let shown = gate::shown(&root, &path);
            (root, shown, matches!(built.word, Word::Unparsed(_)))
        });
    let mut mine = None;
    for (plan, (answer, slots)) in plans.iter().zip(answers) {
        if let Some((root, shown, parse_failed)) = &audit {
            let unfold = plan
                .args
                .as_ref()
                .ok()
                .and_then(|args| args.flag(UNFOLD).ok())
                .unwrap_or(false);
            env.shells.push(Shell {
                call_id: plan.call_id.clone(),
                mine: shown.clone(),
                root: root.clone(),
                parse_failed: *parse_failed,
                unfold,
                seam: false,
                slots,
            });
        }
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

/// One call's ranges, measured against the snapshot it was written against.
fn read_edits(args: &Args, call: usize, base: &[&str]) -> Result<Vec<Edit>> {
    let items = args.objects(EDITS)?;
    edit::call_edits(&items, call, base)
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
    /// The edits that lost to an earlier call, as positions into `edits`.
    dropped: Vec<usize>,
    /// The calls whose arguments could not be read at all.
    unreadable: Vec<Option<String>>,
    /// What the formatter made of the merged text.
    word: Word,
    /// The edits whose block does not parse on its own, with the parser's words.
    blamed: Vec<(usize, String)>,
    written: Option<String>,
}

impl Built {
    /// One answer per call of the batch: its own edits, its own verdicts, and where the audit's
    /// check lines will hang.
    fn answers(&self, plans: &[Plan], path: &Path) -> Vec<(Answer, Vec<Slot>)> {
        let lines: Vec<&str> = self.shown.lines().collect();
        let base: Vec<String> = self.base.lines().map(str::to_string).collect();
        let table = lang::table(path, &self.shown);
        let shown = path.display().to_string();
        let shifted = self.moved();

        (0..plans.len())
            .map(|at| {
                if let Some(why) = &self.unreadable[at] {
                    return (
                        Answer {
                            text: why.clone(),
                            memo: None,
                            refusal: Some("the call's ranges could not be read".to_string()),
                        },
                        Vec::new(),
                    );
                }
                let (from, to) = self.spans[at];
                let mut rows: Vec<Row> = Vec::with_capacity(to - from);
                let mut slots: Vec<Slot> = Vec::with_capacity(to - from);
                for index in from..to {
                    let landing = self
                        .held
                        .iter()
                        .position(|held| *held == index)
                        .and_then(|position| self.landings.get(position).copied().flatten());
                    rows.push(self.row(index, &lines, &base, &table));
                    slots.push(Slot {
                        landing,
                        insert_at: 0,
                    });
                }
                let (text, memo, marks) = receipt::build(
                    receipt::Sheet {
                        path: &shown,
                        after: self.after,
                        table: &table,
                        shifted,
                    },
                    &rows,
                    &self.word,
                );
                for (slot, mark) in slots.iter_mut().zip(marks) {
                    slot.insert_at = mark;
                }
                (
                    Answer {
                        text,
                        memo: Some(memo),
                        refusal: None,
                    },
                    slots,
                )
            })
            .collect()
    }

    /// Whether a written edit above a refused one moved the refused one's lines.
    fn moved(&self) -> bool {
        self.dropped.iter().any(|at| {
            self.held
                .iter()
                .any(|held| self.edits[*held].start < self.edits[*at].start)
        })
    }

    /// One edit, as its own call's answer shows it.
    fn row(&self, at: usize, lines: &[&str], base: &[String], table: &Table) -> Row {
        let edit = &self.edits[at];
        let blame = self
            .blamed
            .iter()
            .find(|(index, _)| *index == at)
            .map(|(_, why)| why.clone());
        if let Some(position) = self.held.iter().position(|held| *held == at) {
            let landing = self.landings.get(position).copied().flatten();
            let old = edit.taken(base).to_vec();
            let new = match landing {
                Some((first, last)) => take(lines, first, last),
                None => Vec::new(),
            };
            let item = match edit.cut {
                true => None,
                false => landing.and_then(|(first, _)| item_of(table, first)),
            };
            return Row {
                index: edit.index,
                from: edit.start + 1,
                state: State::Applied {
                    at: landing,
                    old,
                    new,
                    item,
                    blame,
                },
            };
        }

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
                item: place.and_then(|(first, _)| item_of(table, first)),
                why: "overlaps an earlier splice call on this file",
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
    let base: Vec<&str> = text.lines().collect();
    let total = base.len();
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

    let (edits, spans, unreadable) = flatten(plans, &base);
    let (kept, dropped) = settle_overlaps(&edits)?;
    let rust = path.extension().and_then(|e| e.to_str()) == Some("rs");

    let mut held: Vec<usize> = Vec::new();
    let mut landings: Vec<Place> = Vec::new();
    let mut word = Word::Plain;
    let mut blamed: Vec<(usize, String)> = Vec::new();
    let mut written: Option<String> = None;

    if !kept.is_empty() {
        let picked: Vec<Edit> = kept.iter().map(|at| edits[*at].clone()).collect();
        let now = if rust {
            let edition = gate::edition(path);
            let Some(settled) = fmt::settle(&text, &picked, &edition, stop) else {
                return Ok(None);
            };
            blamed = settled
                .blamed
                .iter()
                .map(|(at, why)| (kept[*at], why.clone()))
                .collect();
            word = settled.word;
            landings = settled.landings;
            settled.text
        } else {
            let refs: Vec<&Edit> = picked.iter().collect();
            landings = edit::shifted(&picked);
            edit::spliced(&text, &refs)
        };
        held = kept;
        if stop.is_stopped() {
            return Ok(None);
        }
        if !held.is_empty() && now != text {
            written = Some(now);
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
        word,
        blamed,
        written,
    }))
}

/// The batch's edits in arrival order, each call's slice of them, and the calls whose arguments
/// could not be read at all.
type Flattened = (Vec<Edit>, Vec<(usize, usize)>, Vec<Option<String>>);

fn flatten(plans: &[Plan], base: &[&str]) -> Flattened {
    let mut edits = Vec::new();
    let mut spans = Vec::with_capacity(plans.len());
    let mut unreadable: Vec<Option<String>> = vec![None; plans.len()];
    for (at, plan) in plans.iter().enumerate() {
        let from = edits.len();
        match &plan.args {
            Ok(args) => match read_edits(args, at, base) {
                Ok(list) => edits.extend(list),
                Err(why) => unreadable[at] = Some(format!("{why:#}")),
            },
            Err(why) => unreadable[at] = Some(format!("{why:#}")),
        }
        spans.push((from, edits.len()));
    }
    (edits, spans, unreadable)
}

/// Which edits may run, and which lost to an earlier call: the kept positions, then the refused
/// ones. Two of one call's own ranges may not overlap at all — that is the model contradicting
/// itself, and the file is left alone.
fn settle_overlaps(edits: &[Edit]) -> Result<(Vec<usize>, Vec<usize>)> {
    let mut kept: Vec<usize> = Vec::new();
    let mut dropped: Vec<usize> = Vec::new();
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
            dropped.push(at);
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

/// The innermost two items a landing sits in, outermost first.
fn item_of(table: &Table, first: usize) -> Option<String> {
    let chain = table.around(first.saturating_sub(1));
    let tail: Vec<&str> = chain
        .iter()
        .rev()
        .take(2)
        .rev()
        .filter_map(|(_, line)| line.exact.as_deref())
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
    table: &Table,
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
        cut: false,
    };
    let updated = edit::spliced(text, &[&one]);
    let old_text = lines[from - 1..to].join("\n");
    let mut memo = change::context(table, text, from, to - from + 1, cap);
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
