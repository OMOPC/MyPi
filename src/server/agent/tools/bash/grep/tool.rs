//! The `grep` tool: hits inside the crate's own source, each with the Rust item around it.

use anyhow::Result;

use super::{render, scan};
use crate::server::agent::tools::contract::env::Env;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;

const QUERY: usize = 0;
const REGEX: usize = 1;
const SCOPE: usize = 2;

const PARAMS: &[Param] = &[
    Param::new(
        "query",
        Kind::Text,
        "The word exactly as it appears in code; no character is special.",
    ),
    Param::df(
        "regex",
        Kind::Flag,
        "Read `query` as a pattern instead of as text",
        "false",
    ),
    Param::new(
        "scope",
        Kind::TextList,
        "Directories to search, relative to the crate root and inside its `src/`; omitted, the \
         whole `src/`. A directory that is missing or outside `src/` is skipped and named at the \
         end of the answer.",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "grep",
    about: "Search the crate's own src/ for a word. Each hit comes with the Rust item it sits in, \
            and `A::b` also reports where `A` declares `b` (a type's members, or a module's item). \
            Files it shows are stamped, so `splice` works on them at once.",
    fields: "`query` is the text; with `regex` off every character stands for itself. `scope` \
             narrows the search to directories under the crate's `src/`. Hits and excerpts carry \
             line numbers. Only .rs files under the crate's src/ are read.",
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
    let (dirs, misses) = scan::roots(&zone, &args.str_list(SCOPE)?);
    let files = scan::collect(&dirs, &matcher);
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
