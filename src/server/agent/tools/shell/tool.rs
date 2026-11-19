// The `shell` tool: raw text through the gate, then one shell line.

use anyhow::Result;

use super::{admit, runner};
use crate::server::agent::tools::contract::env::Env;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;

const COMMAND: usize = 0;

const PARAMS: &[Param] = &[Param::new(
    "command",
    Kind::Text,
    "Shell commands; lines run in order.",
)];

pub(crate) const TOOL: Tool = Tool {
    name: "shell",
    about: "Run non-interactive shell commands (bash/zsh on unix, pwsh/niu on windows). Blocks the \
            rest of this message until it exits: long work → `kairos`. Search with `rg`/`fd`; \
            files → `read`/`edit`/`write`.",
    fields: "command is the only field; its lines run in order.",
    params: PARAMS,
    run,
    summary,
};

fn run(env: &mut Env, args: &Args, progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let command = args.str(COMMAND)?;
    let raw = command.trim();
    anyhow::ensure!(!raw.is_empty(), "empty command");
    let admitted = admit::admit(env, raw)?;

    let cwd = env.workspace.cwd();
    let (text, exit) = runner::run(
        &env.shell.backend,
        &cwd,
        &admitted.cmd,
        env.shell.timeout,
        &env.stop,
        progress,
    )?;

    // The oversized tail is the front door's business (file it, append the ref); the tool hands up text.
    let text = match admitted.warn {
        Some(ids) => format!("[warn: {ids}]\n{text}"),
        None => text,
    };
    // A stop is the front door's to report; a deadline has no one else to speak for it.
    let out = ToolOutput::text(text);
    Ok(match exit.killed && !exit.canceled {
        true => out.failure(format!("timed out after {}s", exit.timeout_secs)),
        false => out,
    })
}

// One line for a history listing: the command, flattened, and whether it failed.
fn summary(call: &CallRecord) -> String {
    let command = summary::field(call, "command").unwrap_or_else(|| call.args.clone());
    summary::flat(&command)
}
