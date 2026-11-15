//! The `write` tool: overwrite a whole file.

use anyhow::Result;

use crate::server::agent::tools::bash::fault;
use crate::server::agent::tools::bash::gate;
use crate::server::agent::tools::bash::halt::stopped;
use crate::server::agent::tools::contract::env::Env;
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;

const PATH: usize = 0;
const CONTENT: usize = 1;

const PARAMS: &[Param] = &[
    Param::new("path", Kind::Text, "File path."),
    Param::new(
        "content",
        Kind::Text,
        "The file's full content, written verbatim.",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "write",
    about: "Write the whole file, **overwriting** any existing one. A small change → `edit`.",
    fields: "both fields are always needed: the path, and the whole content. The parent folder \
             must already exist.",
    params: PARAMS,
    run,
    summary,
};

/// Write the whole file, **overwriting** an existing one. The counterpart of `edit`: `edit` refuses
/// ambiguous matches, `write` refuses nothing. The caller brings the full content.
fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let path = args.str(PATH)?;
    let content = args.str(CONTENT)?;

    let p = env.workspace.resolve(&path, false)?;
    if let Some(parent) = p.parent() {
        anyhow::ensure!(
            parent.is_dir(),
            "parent directory does not exist: {} (create it with bash mkdir -p first)",
            parent.display()
        );
    }
    let existed = p.exists();

    // Rust is formatted first: the caller's content is what it meant, not what it lays out.
    let rust = p.extension().and_then(|e| e.to_str()) == Some("rs");
    let mut parse = None;
    let stop = &env.stop;
    let written = if rust {
        match gate::format(&content, &gate::edition(&p), stop) {
            gate::Outcome::Formatted(text) => text,
            gate::Outcome::Broken(why) => {
                parse = Some(why);
                content.clone()
            }
            // The formatter was killed: the file is untouched, and the answer says why.
            gate::Outcome::Stopped => return Ok(stopped(&p)),
        }
    } else {
        content.clone()
    };

    env.workspace.with_write(&p, || {
        crate::server::agent::tools::workspace::write_atomic(&p, &written)
    })?;
    let bytes = written.len();
    let mut text = format!(
        "{} {} ({bytes} bytes)",
        if existed { "overwrote" } else { "created" },
        p.display()
    );

    // The compiler answers for the file as it now stands, outside the file's lock. A file that does
    // not parse has its verdict already; the compiler would only repeat it.
    let check = match (rust, &parse) {
        (true, None) => gate::check(&p, stop),
        _ => gate::Check::Skipped,
    };
    if let Some(why) = &parse {
        text.push('\n');
        text.push_str(&fault::unparsed(&why.message));
    }
    if let Some(report) = fault::receipt(&p, &written, &check) {
        text.push('\n');
        text.push_str(&report);
    }
    Ok(ToolOutput::text(text))
}

/// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "path").unwrap_or_default(), 96)
}
