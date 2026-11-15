//! The `read` tool: files by line numbers, directories by name.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};

use super::dir;
use super::hub;
use super::sparse;
use crate::server::agent::request::contract::ItemOk;
use crate::server::agent::tools::bash::splice::TOOL as SPLICE;
use crate::server::agent::tools::contract::env::{Env, Stamp};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::spec::{Args, Item, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;

const FILES: usize = 0;

/// A Rust source file's own budget: 800 lines, 256 KiB.
const MAX_LINES_RS: usize = 800;
const MAX_BYTES_RS: usize = 256 * 1024;

/// Every other file's budget: 200 lines, 64 KiB.
const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 64 * 1024;

const FILE_FIELDS: &[Param] = &[
    Param::new("path", Kind::Text, "File or directory path."),
    Param::df("offset", Kind::Line, "First line to read (1-based).", "1"),
    Param::new(
        "limit",
        Kind::Number,
        "Max lines this file may take; a `.rs` file answers 800 when this is left out, anything \
         else 200, and neither cap moves.",
    ),
];

const PARAMS: &[Param] = &[Param::new(
    "files",
    Kind::Objects(FILE_FIELDS),
    "One item per path, answered in this order. One item's failure never stops the ones after it.",
)];

pub(crate) const TOOL: Tool = Tool {
    name: "read",
    about: "Read files or directories; .rs files answer with anchor line numbers under splice, an \
            800-line budget, and a module view on their entry file.",
    fields: "`files` takes one item per path, with `offset` (1-based, default 1) and `limit` \
             (default: 800 for a `.rs` file, 200 otherwise, never above that file's own cap). A \
             file answers its lines. A directory answers its immediate children, files with \
             their line counts and directories by name, then opens each child one level further, \
             folding a directory of ten files or more into counts. The session root carries a \
             bonus.",
    params: PARAMS,
    run,
    summary,
};

/// Read every named path, answering in the call's order.
fn run(env: &mut Env, args: &Args, _progress: &mut dyn FnMut(ToolProgress)) -> Result<ToolOutput> {
    let items = args.objects(FILES)?;
    anyhow::ensure!(!items.is_empty(), "`files` must name at least one path");

    let total = items.len();
    // Resolve first: the module view folds a child that this same call reads.
    let mut targets: Vec<Result<PathBuf>> = Vec::with_capacity(total);
    for item in &items {
        targets.push(resolved(env, item));
    }
    let batch: BTreeSet<PathBuf> = targets
        .iter()
        .filter_map(|one| one.as_ref().ok().cloned())
        .collect();

    let mut out = String::new();
    let mut outcomes: Vec<ItemOk> = Vec::with_capacity(total);
    let mut failed = 0usize;
    for (item, target) in items.iter().zip(targets) {
        let result = match target {
            Ok(path) => block(env, &batch, item, &path, total > 1),
            Err(e) => Err(e),
        };
        match result {
            Ok(text) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text);
                outcomes.push(ItemOk::ok());
            }
            Err(e) => {
                // One path's failure is that path's own: the rest still run and still answer.
                if total == 1 {
                    return Err(e);
                }
                failed += 1;
                let path = item.text("path").unwrap_or_else(|_| item.label());
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("==> {path}\n[error: {e:#}]"));
                outcomes.push(ItemOk::failed(format!("{e:#}")));
            }
        }
    }
    let mut produced = ToolOutput::text(out);
    produced.items = outcomes;
    if failed > 0 {
        produced = produced.failure(format!("{failed} of {total} paths failed"));
    }
    Ok(produced)
}

fn resolved(env: &Env, item: &Item) -> Result<PathBuf> {
    let raw = item.text("path")?;
    anyhow::ensure!(!raw.trim().is_empty(), "`{}` has no path", item.label());
    env.workspace.resolve_any(&raw, true)
}

/// The lines one item may take: the file's own cap, or what the call named when that is smaller.
fn window(item: &Item, cap: usize) -> Result<usize> {
    match item.opt_int("limit")? {
        None => Ok(cap),
        Some(n) => {
            anyhow::ensure!(n >= 1, "`{}.limit` must be at least 1", item.label());
            Ok((n as usize).min(cap))
        }
    }
}

/// One path's answer: a listing, or a body with its module view.
fn block(
    env: &mut Env,
    batch: &BTreeSet<PathBuf>,
    item: &Item,
    path: &Path,
    headed: bool,
) -> Result<String> {
    let raw = item.text("path")?;
    let offset = item.line("offset")?;

    if path.is_dir() {
        let text = dir::listing(path, &env.workspace.cwd(), !headed)?;
        return Ok(if headed {
            format!("==> {raw}\n{text}")
        } else {
            text
        });
    }

    let splice = env.allows(SPLICE.name);
    let rust = path.extension().and_then(|e| e.to_str()) == Some("rs");
    let limit = window(item, if rust { MAX_LINES_RS } else { MAX_LINES })?;
    let whole = if rust {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };
    let body = match &whole {
        Some(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len();
            let anchors = if splice {
                sparse::anchors(text)
            } else {
                Vec::new()
            };
            let numbers = if splice {
                Numbers::Anchors(&anchors)
            } else {
                Numbers::None
            };
            assemble(
                offset,
                total,
                limit,
                lines.into_iter().skip(offset.saturating_sub(1)),
                &numbers,
                MAX_BYTES_RS,
            )
        }
        None => {
            let streamed = streamed(path, offset, limit)?;
            let numbers = if splice { Numbers::All } else { Numbers::None };
            assemble(
                offset,
                streamed.total,
                limit,
                streamed.lines.iter().map(String::as_str),
                &numbers,
                MAX_BYTES,
            )
        }
    };

    if let Some(text) = &whole {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        env.seen.files.insert(
            path.to_path_buf(),
            Stamp {
                lines: text.lines().count(),
                hash: hasher.finish(),
            },
        );
    }

    // The status line is part of the body: it comes first, so the body is known before it is read.
    let mut text = String::new();
    if let Some(status) = &body.status {
        text.push_str(status);
        text.push('\n');
    }
    text.push_str(&body.text);

    if rust && let Some(module) = hub::locate(path) {
        let is_hub = module.hub.as_deref() == Some(path)
            || whole.as_deref().is_some_and(hub::declares_modules);
        if is_hub {
            text.push_str(&hub::view(&module, batch, false));
            env.seen.dirs.insert(module.dir.clone());
        } else if env.seen.dirs.insert(module.dir.clone()) {
            text.push_str(&hub::view(&module, batch, true));
        }
    }

    Ok(if headed {
        format!("==> {raw}\n{text}")
    } else {
        text
    })
}

/// How a file's lines are prefixed, if at all.
enum Numbers<'a> {
    /// A session with `edit` and without `splice`: no numbers.
    None,
    /// Another language under `splice`: every line carries one.
    All,
    /// Rust under `splice`: only the anchors do.
    Anchors(&'a [bool]),
}

impl Numbers<'_> {
    /// Do any lines carry a number? A gutter is only reserved when they do.
    fn on(&self) -> bool {
        !matches!(self, Numbers::None)
    }

    fn numbered(&self, at: usize) -> bool {
        match self {
            Numbers::None => false,
            Numbers::All => true,
            Numbers::Anchors(anchors) => anchors.get(at.saturating_sub(1)).copied().unwrap_or(true),
        }
    }
}

/// A body and, when the file continues past it, the line that says so.
struct Body {
    text: String,
    status: Option<String>,
}

/// The window's lines, prefixed by a gutter as wide as the file's own numbers, capped by bytes.
///
/// The gutter is `total`'s digit count plus one pipe, so a numbered line and an unnumbered one
/// start in the same column and an empty line costs a newline and nothing else.
fn assemble<'a>(
    first: usize,
    total: usize,
    limit: usize,
    lines: impl Iterator<Item = &'a str>,
    numbers: &Numbers<'_>,
    max_bytes: usize,
) -> Body {
    let width = total.to_string().len();
    let mut text = String::new();
    let mut bytes = 0usize;
    let mut shown = 0usize;
    let mut by_bytes = false;
    for (index, line) in lines.enumerate() {
        if shown >= limit {
            break;
        }
        let at = first + index;
        let blank = line.trim().is_empty();
        let number = numbers.numbered(at);
        let gutter = if blank || !numbers.on() { 0 } else { width + 1 };
        let room = max_bytes.saturating_sub(bytes + gutter);
        if line.len() + 1 > room {
            // The line is longer than the bytes left: keep what fits, and let the next read
            // start at it again so no character is lost.
            let keep = floor_boundary(line, room.saturating_sub(1));
            if keep > 0 {
                push(&mut text, at, &line[..keep], number, numbers.on(), width);
                by_bytes = true;
            }
            break;
        }
        push(&mut text, at, line, number, numbers.on(), width);
        bytes += gutter + line.len() + 1;
        shown += 1;
    }
    let last = first.saturating_sub(1) + shown;
    let status = (last < total).then(|| {
        let cap = if by_bytes {
            format!("{} KiB cap; ", max_bytes / 1024)
        } else {
            String::new()
        };
        format!("[{cap}{last} of {total} lines; next offset {}]", last + 1)
    });
    Body { text, status }
}

/// One line: its number, or the same width of blanks, then the pipe that ends the gutter.
fn push(out: &mut String, at: usize, line: &str, number: bool, on: bool, width: usize) {
    if !on {
        // Without numbers the body is the file itself, so a line of spaces stays a line of spaces.
        out.push_str(line);
        out.push('\n');
        return;
    }
    if line.trim().is_empty() {
        out.push('\n');
        return;
    }
    if number {
        out.push_str(&format!("{at:>width$}|{line}\n"));
    } else {
        for _ in 0..width {
            out.push(' ');
        }
        out.push('|');
        out.push_str(line);
        out.push('\n');
    }
}

struct Streamed {
    total: usize,
    /// The window's lines, unprefixed: the gutter needs the total, which is only known at the end.
    lines: Vec<String>,
}

/// A file of any other kind, streamed line by line so a huge one never lands in memory whole.
fn streamed(path: &Path, offset: usize, limit: usize) -> Result<Streamed> {
    use std::io::BufRead as _;

    let file = std::fs::File::open(path).context("failed to open file")?;
    let mut reader = std::io::BufReader::new(file);
    let max_lines = limit.min(MAX_LINES);
    let mut buf: Vec<u8> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut kept = 0usize;
    let mut total = 0usize;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        total += 1;
        if total < offset || lines.len() >= max_lines || kept >= MAX_BYTES {
            continue;
        }
        let line = std::str::from_utf8(&buf).map_err(|_| anyhow!("not a valid UTF-8 text file"))?;
        let line = line.trim_end_matches(['\n', '\r']);
        kept += line.len() + 1;
        lines.push(line.to_string());
    }
    Ok(Streamed { total, lines })
}

/// The largest character boundary within the first `bytes` bytes.
fn floor_boundary(line: &str, bytes: usize) -> usize {
    let mut end = bytes.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// One line for a history listing: the paths, the first named and the rest counted.
fn summary(call: &CallRecord) -> String {
    let paths = summary::paths(call, "files");
    let first = paths.first().cloned().unwrap_or_default();
    summary::clip(&summary::counted(&first, paths.len()), 96)
}
