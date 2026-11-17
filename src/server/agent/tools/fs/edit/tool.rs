//! The `edit` tool: replace one unique run of text.

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

use crate::server::agent::tools::contract::env::{Env, License};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::fs::change;
use crate::server::agent::tools::fs::lang;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;

const PATH: usize = 0;
const OLD: usize = 1;
const NEW: usize = 2;

const PARAMS: &[Param] = &[
    Param::new("path", Kind::Text, "File path."),
    Param::new(
        "old",
        Kind::Text,
        "The text to replace. MUST match the file character for character, exactly once.",
    ),
    Param::new(
        "new",
        Kind::Text,
        "The replacement; any number of lines. Empty deletes the target.",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "edit",
    about: "Replace one unique run of text in one file. MUST read the file first; NEVER guess its \
            text. New file → `write`; by line number → `splice`.",
    fields: "path, old and new are always needed: the file, the text that must appear exactly \
             once, and what stands in its place.",
    params: PARAMS,
    run,
    summary,
};

fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let path = args.str(PATH)?;
    let p = env.workspace.resolve(&path, true)?;
    let old = args.str(OLD)?;
    let new = args.str(NEW)?;

    let updated = env.workspace.with_write(&p, || {
        let text = std::fs::read_to_string(&p).context("failed to read file")?;
        let table = lang::table(&p, &text);
        let (updated, memo, message) = exact_text(&text, &table, &old, &new, env.full_item_budget)?;
        write_atomic(&p, &updated)?;
        Ok((updated, memo, message))
    })?;
    let (updated, memo, message) = updated;
    env.seen.files.insert(p.clone(), License::of(&updated));
    Ok(ToolOutput::with_memo(
        format!("{message}: {}", p.display()),
        memo,
    ))
}

/// Replace one unique run of text.
pub(crate) fn exact_text(
    text: &str,
    table: &lang::Table,
    old: &str,
    new: &str,
    cap: usize,
) -> Result<(String, Value, String)> {
    anyhow::ensure!(!old.is_empty(), "`old` must not be empty");

    match text.matches(old).count() {
        0 => Err(anyhow!("`old` not found in the file")),
        1 => {
            let pos = text.find(old).expect("count == 1 checked above");
            let at = change::line_of(text, pos);
            let old_lines = change::lines_of(old).len();
            let new_lines = change::lines_of(new).len();
            let total = text.lines().count() + new_lines - old_lines;
            Ok((
                text.replacen(old, new, 1),
                change::context(table, text, at, old_lines, cap),
                change::receipt(at, old_lines, new_lines, total),
            ))
        }
        n => Err(anyhow!(
            "`old` appears {n} times; make it longer with surrounding context, \
             or use `splice` to replace by line number"
        )),
    }
}

/// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "path").unwrap_or_default(), 96)
}
