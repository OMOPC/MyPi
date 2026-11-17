//! The `write` tool: overwrite a whole file. The batch's audit type-checks it after the batch's
//! last write; this call only formats, writes, and hands the file's shell to the audit.
use anyhow::Result;

use crate::server::agent::tools::bash::fault;
use crate::server::agent::tools::bash::gate;
use crate::server::agent::tools::bash::halt::stopped;
use crate::server::agent::tools::contract::env::{Env, Shell, Slot};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;

const PATH: usize = 0;
const CONTENT: usize = 1;
const UNFOLD: usize = 2;

const PARAMS: &[Param] = &[
    Param::new("path", Kind::Text, "File path."),
    Param::new(
        "content",
        Kind::Text,
        "The file's full content, written verbatim.",
    ),
    Param::df(
        "unfold",
        Kind::Flag,
        "List every fault of the batch's shared check instead of folding it.",
        "false",
    ),
];

pub(crate) const TOOL: Tool = Tool {
    name: "write",
    about: "Write the whole file, **overwriting** any existing one. A small change → `edit`. \
            Type-checked with the batch, reported on the last write.",
    fields: "both fields are always needed: the path, and the whole content. The parent folder \
             must already exist.\n\
             `unfold` lifts the shared check's fold for every written file of the batch.",
    params: PARAMS,
    run,
    summary,
};

/// Write the whole file, **overwriting** an existing one. The counterpart of `edit`: `edit` refuses
/// ambiguous matches, `write` refuses nothing. The caller brings the full content.
fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let path = args.str(PATH)?;
    let content = args.str(CONTENT)?;
    let unfold = args.flag(UNFOLD).unwrap_or(false);

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
    let mut unparsed = None;
    let mut undecided = None;
    let stop = &env.stop;
    let written = if rust {
        match gate::format(&content, &gate::edition(&p), stop) {
            gate::Outcome::Formatted(text) => text,
            // A text the formatter cannot parse is written as it stands, and said so.
            gate::Outcome::Unparsed(why) => {
                unparsed = Some(why);
                content.clone()
            }
            // The formatter decided nothing about the text: it is written as it stands, and the
            // batch's check is left to say what it makes of it.
            gate::Outcome::Undecided(why) => {
                undecided = Some(why);
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
    if let Some(why) = &unparsed {
        text.push('\n');
        text.push_str(&fault::unparsed(why));
    }
    if let Some(why) = &undecided {
        text.push('\n');
        text.push_str(&format!("[i] not formatted, written as it stands: {why}"));
    }

    // The compiler answers for the file as it now stands, after the batch's last write; the seam
    // stands under this answer at its end. A file that does not parse has the formatter's verdict
    // already. Only a Rust file joins the batch's check.
    if rust && let Some(root) = gate::crate_root(&p) {
        env.shells.push(Shell {
            call_id: env.current.clone(),
            mine: gate::shown(&root, &p),
            root,
            parse_failed: unparsed.is_some(),
            unfold,
            seam: true,
            slots: vec![Slot {
                landing: None,
                insert_at: text.lines().count(),
            }],
        });
    }
    Ok(ToolOutput::text(text))
}

/// One line for a history listing: which file.
fn summary(call: &CallRecord) -> String {
    summary::clip(&summary::field(call, "path").unwrap_or_default(), 96)
}
