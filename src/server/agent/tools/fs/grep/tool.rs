//! The `grep` tool: hits inside the crate's own source, each with the piece around it.

use super::{render, scan};
use crate::server::agent::tools::contract::env::Env;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use anyhow::Result;

const QUERY: usize = 0;
const REGEX: usize = 1;
const SCOPE: usize = 2;

const PARAMS: &[Param] = &[
    Param::new(
        "query",
        Kind::Text,
        "Exact literal string (or regex if `regex: true`). Supports `A::b` to locate `b` inside type/module `A`.",
    ),
    Param::df("regex", Kind::Flag, "Parse `query` as regex", "false"),
    Param::new(
        "scope",
        Kind::TextList,
        "`src/...` files or dirs, whatever suffix a sorter claims (defaults to whole `src/`; invalid \
         entries skipped).",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "grep",
    about: "Structural search across the crate's `src/`, Rust source and Markdown. Returns \
            hits with the enclosing piece's hierarchy and line numbers. Supports `A::b` member \
            lookup.",
    fields: "Requires `query`; `regex` and `scope` are optional.\n\
             - `A::b` syntax: besides literal matches, also locates where type/trait/module `A` declares `b`.\n\
             - Output: 1-based line numbers + surrounding item scopes.",
    params: PARAMS,
    run,
    summary,
};

fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let query = args.str(QUERY)?;
    anyhow::ensure!(!query.is_empty(), "`query` is empty");
    let regex = args.flag(REGEX)?;
    let zone = scan::zone(&env.workspace.cwd())?;
    let matcher = if regex {
        scan::Matcher::pattern(&query)?
    } else {
        scan::Matcher::text(&query)
    };
    let (dirs, picked, misses) = scan::roots(&zone, &args.str_list(SCOPE)?);
    let files = scan::collect(&dirs, &picked, &matcher);
    let mode = if regex { "regex" } else { "literal" };
    let (text, shown) = render::answer(
        &query,
        mode,
        &files,
        &zone,
        env.grep,
        env.limits.lines,
        &misses,
    );
    render::stamp(&mut env.seen, &files, &shown);
    Ok(ToolOutput::text(text))
}

/// One line for a history listing: the query.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "query").unwrap_or_default(), 96)
}
