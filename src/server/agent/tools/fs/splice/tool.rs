// The `splice` tool: replace whole lines by number, many ranges in one file.

use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::Value;

use super::edit::{self, Edit, Place};
use super::fmt::{self, Word};
use super::receipt::{self, Row};
use crate::server::agent::request::contract::ToolCall;
use crate::server::agent::tools::contract::env::{Answer, Env, License, Shell, Slot};
use crate::server::agent::tools::contract::halt::stopped;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::fs::lang;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;
use crate::server::platform::stop::Stop;

const PATH: usize = 0;
const EDITS: usize = 1;
const UNFOLD: usize = 2;

const EDIT_FIELDS: &[Param] = &[
    Param::new(
        "start",
        Kind::Axis,
        "First line of the range, 1-based: `N the words that line opens with`, e.g. `12 pub fn \
         run`. A bare `N` names a blank line and nothing else; on any other line it is refused and \
         nothing is written.",
    ),
    Param::new(
        "end",
        Kind::Axis,
        "Last line of the range, inclusive, in the shape `start` reads: `N the words that line \
         opens with`, checked when the range covers more than one line. `start - 1` inserts \
         instead of replacing, and names no line.",
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
            `@@CUT(a..b)@@` line macros. Auto-formats through the file's own language \
            (writes and warns on syntax error). By matched text → `edit`. One check per batch, \
            reported on the last write.",
    fields: "Requires `path` and non-overlapping `edits` (`{start, end, new}`). \
             Delete: `new: \"\"`. Insert before `start`: `end = start - 1`. \
             `start` reads `N the words that line opens with`, e.g. `12 pub fn run`; a bare `N` \
             the same, and its words are checked when the range covers more than one line; an \
             insert (`start - 1`) names no line at all. \
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
    let license = env.seen.files.get(&path).cloned();
    let current = env.current.clone();

    let stop = env.stop.clone();
    let Some(built) = env
        .workspace
        .with_write(&path, || compute(&path, license, &plans, &stop))?
    else {
        // A sibling of this batch reaches its own turn, finds the stop still set, and says the same
        // thing about the same untouched file: it is said here, never kept for `env.handed`.
        return Ok(stopped(&path));
    };
    // The compiler waits for the batch's last write: the audit finishes every written answer then.
    let answers = built.answers(&plans, &path);

    let revoked = !built.dropped.is_empty() || built.unreadable.iter().any(|why| why.is_some());
    if revoked {
        env.seen.files.remove(&path);
    } else {
        env.seen
            .files
            .insert(path.clone(), License::of(&built.shown));
    }
    // Every written Rust call hands its shell to the audit: its faults hang under its edits when
    // the batch's check has run, and the crate's report rides on the last written answer.
    let audit = (built.written.is_some() && !matches!(built.word, Word::Plain))
        .then(|| lang::checker(&path))
        .flatten()
        .and_then(|checker| {
            let root = (checker.unit)(&path)?;
            let shown = (checker.shown)(&root, &path);
            Some((root, shown, matches!(built.word, Word::Unparsed(_))))
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
                file: path.clone(),
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
    // What the front door hands the model.
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

// One splice call of this batch that names the same file.
struct Plan {
    call_id: String,
    // Its arguments, read by the same table the schema is built from.
    args: Result<Args>,
}

// Every splice call of the batch that names `path`, in arrival order.
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

// Whether this call is one of the batch's, aimed at `path`, before its arguments are trusted.
fn names(call: &ToolCall, env: &Env, path: &Path) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(&call.args) else {
        return false;
    };
    let Some(raw) = value.get("path").and_then(Value::as_str) else {
        return false;
    };
    env.workspace.resolve(raw, true).ok().as_deref() == Some(path)
}

// What one merged run leaves behind: owned, so the compiler can read the crate after the file's
// lock is gone.
struct Built {
    // The file before the write.
    base: String,
    // The file as it now stands: what was written, or the base when nothing was.
    shown: String,
    after: usize,
    // Every edit of the batch, in arrival order, and each call's slice of them.
    edits: Vec<Edit>,
    spans: Vec<(usize, usize)>,
    // The edits that were written, as positions into `edits`, and where each one landed.
    held: Vec<usize>,
    landings: Vec<Place>,
    // The edits that lost to an earlier call, as positions into `edits`.
    dropped: Vec<usize>,
    // The calls whose arguments could not be read at all.
    unreadable: Vec<Option<String>>,
    // What the formatter made of the merged text.
    word: Word,
    // The edits whose block does not parse on its own, with the parser's words.
    blamed: Vec<(usize, String)>,
    written: Option<String>,
}

impl Built {
    // One answer per call of the batch: its own edits, its own verdicts, and where the audit's
    // check lines will hang.
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
                    let fate = receipt::Fate {
                        applied: self.held.contains(&index),
                        at: landing,
                        blame: self
                            .blamed
                            .iter()
                            .find(|(held, _)| *held == index)
                            .map(|(_, why)| why.clone()),
                    };
                    rows.push(receipt::row(
                        &self.edits[index],
                        &fate,
                        &lines,
                        &base,
                        &table,
                    ));
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

    // Whether a written edit above a refused one moved the refused one's lines.
    fn moved(&self) -> bool {
        self.dropped.iter().any(|at| {
            self.held
                .iter()
                .any(|held| self.edits[*held].start < self.edits[*at].start)
        })
    }
}

// The whole group: the snapshot, one write, and one answer per call. `None` = the turn was stopped
// before the write, so the file is exactly as it was and no call has an answer.
fn compute(
    path: &Path,
    license: Option<License>,
    plans: &[Plan],
    stop: &Stop,
) -> Result<Option<Built>> {
    let text = std::fs::read_to_string(path).context("failed to read file")?;
    let base: Vec<&str> = text.lines().collect();
    let total = base.len();
    let Some(license) = license else {
        bail!(
            "`{}` has not been read in this conversation, so `splice` may not write it. Read it first \
             with `read` or `grep` and resend.",
            path.display()
        );
    };
    if !license.approval {
        bail!(
            "`{}`'s last write was refused, so it was left unwritten and its licence revoked. Read it \
             again with `read` or `grep` and resend.",
            path.display()
        );
    }
    if license.hash != License::of(&text).hash {
        bail!(
            "`{}` changed on disk after it was licensed (now {total} lines). Read it again and resend.",
            path.display()
        );
    }

    let (edits, spans, unreadable) = flatten(plans, &base);
    let (kept, dropped) = edit::settle_overlaps(&edits, &base)?;
    let formatter = lang::format(path);

    let mut held: Vec<usize> = Vec::new();
    let mut landings: Vec<Place> = Vec::new();
    let mut word = Word::Plain;
    let mut blamed: Vec<(usize, String)> = Vec::new();
    let mut written: Option<String> = None;

    if !kept.is_empty() {
        let picked: Vec<Edit> = kept.iter().map(|at| edits[*at].clone()).collect();
        let now = if let Some(format) = formatter {
            let Some(settled) = fmt::settle(&text, &picked, format, path, stop) else {
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

// The batch's edits in arrival order, each call's slice of them, and the calls whose arguments
// could not be read at all.
type Flattened = (Vec<Edit>, Vec<(usize, usize)>, Vec<Option<String>>);

fn flatten(plans: &[Plan], base: &[&str]) -> Flattened {
    let mut edits = Vec::new();
    let mut spans = Vec::with_capacity(plans.len());
    let mut unreadable: Vec<Option<String>> = vec![None; plans.len()];
    for (at, plan) in plans.iter().enumerate() {
        let from = edits.len();
        match &plan.args {
            Ok(args) => match args
                .objects(EDITS)
                .and_then(|items| edit::call_edits(&items, at, base))
            {
                Ok(list) => edits.extend(list),
                Err(why) => unreadable[at] = Some(format!("{why:#}")),
            },
            Err(why) => unreadable[at] = Some(format!("{why:#}")),
        }
        spans.push((from, edits.len()));
    }
    (edits, spans, unreadable)
}

// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::path(call)
}
