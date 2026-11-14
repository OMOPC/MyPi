//! The `splice` tool: replace whole lines by number.

use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::contract::env::Env;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::write_atomic;

const PATH: usize = 0;
const RANGE: usize = 1;
const NEW: usize = 2;

const PARAMS: &[Param] = &[
    Param::new("path", Kind::Text, "File path."),
    Param::new(
        "range",
        Kind::Text,
        "The lines to replace, 1-based: `12-14` or `12`.",
    ),
    Param::new(
        "new",
        Kind::Text,
        "The replacement; any number of lines. Empty deletes the target.",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "splice",
    about: "Replace whole lines by number in one file; an empty `new` deletes them. By matched \
            text → `edit`.",
    fields: "path, range and new are always needed: the file, the lines (1-based, `12-14` or \
             `12`), and what stands in their place.",
    params: PARAMS,
    run,
    summary,
};

fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let path = args.str(PATH)?;
    let p = env.workspace.resolve(&path, true)?;

    env.workspace.with_write(&p, || {
        let text = std::fs::read_to_string(&p).context("failed to read file")?;
        let (from, to) = parse_range(&args.str(RANGE)?)?;
        let (updated, memo, message) =
            mass_text(&text, from, to, &args.str(NEW)?, env.full_item_budget)?;
        write_atomic(&p, &updated)?;
        Ok(ToolOutput::with_memo(
            format!("{message}: {}", p.display()),
            memo,
        ))
    })
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

    let block: Vec<&str> = match new {
        "" => Vec::new(),
        // One trailing newline is the block's own terminator, not an extra empty line.
        text => text
            .strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .collect(),
    };
    let old_lines = to - from + 1;
    let new_lines = block.len();

    let mut out: Vec<&str> = Vec::with_capacity(total - old_lines + new_lines);
    out.extend_from_slice(&lines[..from - 1]);
    out.extend_from_slice(&block);
    out.extend_from_slice(&lines[to..]);
    let mut text_out = out.join("\n");
    if text.ends_with('\n') {
        text_out.push('\n');
    }

    let old_text = lines[from - 1..to].join("\n");
    let after = total - old_lines + new_lines;
    let mut memo = change::context(text, from, old_lines, cap);
    memo["atLine"] = json!(from);
    memo["old"] = json!(old_text);
    Ok((
        text_out,
        memo,
        change::receipt(from, old_lines, new_lines, after),
    ))
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

/// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "path").unwrap_or_default(), 96)
}
