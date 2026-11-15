//! The `splice` tool: replace whole lines by number, many ranges in one file.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::edit::{self, Edit, Place};
use super::fmt;
use super::receipt::{self, Rejected, Why};
use crate::server::agent::request::contract::ToolCall;
use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::bash::fault;
use crate::server::agent::tools::bash::gate;
use crate::server::agent::tools::contract::env::{Answer, Env, Stamp};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;

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

    let mut done = env
        .workspace
        .with_write(&path, || compute(&path, stamp, &plans, &current))?;

    // The compiler runs after the file's lock is released: it reads the whole crate, not this file.
    if done.written.is_some() {
        let check = gate::check(&path);
        if let Some(at) = plans.iter().position(|plan| plan.call_id == current) {
            done.report(at, &path, &check);
        }
    }

    if let Some(written) = &done.written {
        env.seen.files.insert(
            path.clone(),
            Stamp {
                lines: written.lines().count(),
                hash: hash(written),
            },
        );
    }
    let mut mine = None;
    for (plan, answer) in plans.iter().zip(done.answers) {
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

/// What one merged run produced: each call's answer, and the text it wrote.
struct Done {
    answers: Vec<Answer>,
    written: Option<String>,
}

impl Done {
    /// Give one call's answer the compiler's report, when the compiler had anything to say.
    fn report(&mut self, at: usize, path: &Path, check: &gate::Check) {
        let Some(shown) = self.written.as_deref() else {
            return;
        };
        let Some(report) = fault::receipt(path, shown, check) else {
            return;
        };
        if let Some(answer) = self.answers.get_mut(at) {
            answer.text.push('\n');
            answer.text.push_str(&report);
        }
    }
}

/// The whole group: the snapshot, one write, and one answer per call.
fn compute(path: &Path, stamp: Option<Stamp>, plans: &[Plan], current: &str) -> Result<Done> {
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
    let (kept, clashes) = settle_overlaps(&edits)?;
    let rust = path.extension().and_then(|e| e.to_str()) == Some("rs");

    let mut final_ids: Vec<usize> = Vec::new();
    let mut final_edits: Vec<Edit> = Vec::new();
    let mut blocks: Vec<edit::Place> = Vec::new();
    let mut broken: Vec<(usize, fmt::Broken)> = Vec::new();
    let mut written: Option<String> = None;

    if !kept.is_empty() {
        let picked: Vec<Edit> = kept.iter().map(|at| edits[*at].clone()).collect();
        if rust {
            let edition = gate::edition(path);
            let settled = fmt::settle(&text, &picked, &edition);
            if settled.kept.is_empty()
                && let Some(why) = &settled.bare
            {
                bail!(
                    "the file does not parse before any change: {} (line {})",
                    why.message,
                    why.line.map(|n| n.to_string()).unwrap_or("?".to_string())
                );
            }
            for (at, why) in &settled.dropped {
                broken.push((kept[*at], why.clone()));
            }
            final_ids = settled.kept.iter().map(|at| kept[*at]).collect();
            final_edits = settled.kept.iter().map(|at| picked[*at].clone()).collect();
            let refs: Vec<&Edit> = final_edits.iter().collect();
            blocks = fmt::place(&text, &refs, &edition);
            if !final_ids.is_empty() && settled.text != text {
                written = Some(settled.text);
            }
        } else {
            let picked_refs: Vec<&Edit> = picked.iter().collect();
            let out = edit::spliced(&text, &picked_refs);
            final_ids = kept.clone();
            final_edits = picked;
            blocks = edit::shifted(&final_edits);
            if !final_ids.is_empty() && out != text {
                written = Some(out);
            }
        }
    }

    if let Some(text) = &written {
        write_atomic(path, text)?;
    }
    let after = written
        .as_deref()
        .map(|text| text.lines().count())
        .unwrap_or(total);
    let shown = written.as_deref().unwrap_or(&text);

    let group = Group {
        base: &text,
        shown,
        after,
        path,
        runner: current,
        edits: &edits,
        spans: &spans,
        held: &final_ids,
        placed: &final_edits,
        blocks: &blocks,
        broken: &broken,
        clashes: &clashes,
        unreadable: &unreadable,
        rust,
    };
    let answers = plans
        .iter()
        .enumerate()
        .map(|(at, plan)| group.answer(at, &plan.call_id))
        .collect();
    Ok(Done { answers, written })
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
type Overlaps = (Vec<usize>, Vec<(usize, String)>);

fn settle_overlaps(edits: &[Edit]) -> Result<Overlaps> {
    let mut kept: Vec<usize> = Vec::new();
    let mut clashes: Vec<(usize, String)> = Vec::new();
    for at in 0..edits.len() {
        let mut clash = None;
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
            clash = Some(format!(
                "overlaps {} of an earlier splice call on this file",
                edits[*held].was()
            ));
            break;
        }
        match clash {
            Some(why) => clashes.push((at, why)),
            None => kept.push(at),
        }
    }
    Ok((kept, clashes))
}

/// One merged run's facts, as each call's answer needs them.
struct Group<'a> {
    /// The file before the write.
    base: &'a str,
    /// The file as it now stands: what was written, or the base when nothing was.
    shown: &'a str,
    after: usize,
    path: &'a Path,
    /// The call that ran the merged write; it alone carries the skeleton.
    runner: &'a str,
    /// Every edit of the batch, in arrival order, and each call's slice of them.
    edits: &'a [Edit],
    spans: &'a [(usize, usize)],
    /// The edits that were written, as positions into `edits`, and the same edits in that order.
    held: &'a [usize],
    placed: &'a [Edit],
    /// Where each written edit landed.
    blocks: &'a [Place],
    /// The edits left out: rustfmt refused them, or an earlier change took their lines.
    broken: &'a [(usize, fmt::Broken)],
    clashes: &'a [(usize, String)],
    /// The calls whose arguments could not be read at all.
    unreadable: &'a [Option<String>],
    rust: bool,
}

impl Group<'_> {
    /// One call's answer: the file's skeleton for the call that ran the write, and only that
    /// call's own ranges and refusals.
    fn answer(&self, at: usize, call_id: &str) -> Answer {
        if let Some(why) = &self.unreadable[at] {
            return Answer {
                text: why.clone(),
                memo: None,
                refusal: Some("the call's ranges could not be read".to_string()),
            };
        }
        let (from, to) = self.spans[at];
        let own = &self.edits[from..to];
        let mut kept = Vec::new();
        let mut own_blocks = vec![None; own.len()];
        for (position, edit) in self.placed.iter().enumerate() {
            if edit.call != at {
                continue;
            }
            let local = self.held[position] - from;
            kept.push(local);
            own_blocks[local] = self.blocks[position];
        }
        let mut rejected: Vec<Rejected> = Vec::new();
        for (index, why) in self.clashes {
            if (from..to).contains(index) {
                rejected.push(Rejected {
                    index: index - from,
                    why: Why::Clash(why.clone()),
                });
            }
        }
        for (index, why) in self.broken {
            if (from..to).contains(index) {
                rejected.push(Rejected {
                    index: index - from,
                    why: Why::Broken(why.clone()),
                });
            }
        }
        rejected.sort_by_key(|one| one.index);
        let (text, memo) = receipt::build(
            self.base,
            self.shown,
            own,
            &kept,
            &own_blocks,
            &rejected,
            &self.path.display().to_string(),
            self.rust,
            self.rust && call_id == self.runner,
            self.after,
        );
        Answer {
            text,
            memo: Some(memo),
            refusal: None,
        }
    }
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
