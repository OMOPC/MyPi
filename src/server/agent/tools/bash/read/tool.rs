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
    Param::df("limit", Kind::Number, "Max lines this file may take.", "800"),
];

const PARAMS: &[Param] = &[Param::new(
    "files",
    Kind::Objects(FILE_FIELDS),
    "One item per path, answered in this order. One item's failure never stops the ones after it.",
)];

pub(crate) const TOOL: Tool = Tool {
    name: "read",
    about: "Read files (directory paths list entries), so later turns can cite exact lines. MUST \
            read before editing; NEVER guess the current text.",
    fields: "files is the only field a call needs: one item per path, with `offset` (1-based, \
             default 1) and `limit` (default 800). A Rust file gets 800 lines and 256 KiB; any \
             other file gets 200 lines and 64 KiB. In a session with `splice`, a Rust file's \
             numbers sit on block edges only; without it, lines carry no numbers at all.",
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
    let batch: BTreeSet<PathBuf> = targets.iter().filter_map(|one| one.as_ref().ok().cloned()).collect();

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

/// One path's answer: a listing, or a body plus its module view.
fn block(
    env: &mut Env,
    batch: &BTreeSet<PathBuf>,
    item: &Item,
    path: &Path,
    headed: bool,
) -> Result<String> {
    let raw = item.text("path")?;
    let offset = item.line("offset")?;
    let limit = item.int("limit")?;
    anyhow::ensure!(limit >= 1, "`{}.limit` must be at least 1", item.label());
    let limit = limit as usize;

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
    let whole = if rust {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };
    let (body, total, more, by_bytes) = match &whole {
        Some(text) => {
            let window = windowed(text, offset, limit, splice);
            let total = text.lines().count();
            (window.text, total, window.more, window.by_bytes)
        }
        None => {
            let body = streamed(path, offset, limit, splice)?;
            (body.text, body.lines, body.more, body.by_bytes)
        }
    };

    if let Some(text) = &whole {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        env.seen.files.insert(
            path.to_path_buf(),
            Stamp {
                lines: total,
                hash: hasher.finish(),
            },
        );
    }

    let cap_bytes = if rust { MAX_BYTES_RS } else { MAX_BYTES };
    let mut text = finished(body, offset, total, more, by_bytes, cap_bytes);

    if rust && let Some(module) = hub::locate(path) {
        let is_hub = module.hub.as_deref() == Some(path)
            || whole.as_deref().is_some_and(hub::declares_modules);
        if is_hub {
            let view = hub::view(&module, batch);
            text.push_str(&view);
            env.seen.dirs.insert(module.dir.clone());
        } else if env.seen.dirs.insert(module.dir.clone()) {
            text = format!("{}{text}", hub::view(&module, batch));
        }
    }

    Ok(if headed {
        format!("==> {raw}\n{text}")
    } else {
        text
    })
}

struct Window {
    text: String,
    more: bool,
    by_bytes: bool,
}

/// A whole Rust file, windowed and numbered: sparse numbers in splice mode, none without it.
fn windowed(text: &str, offset: usize, limit: usize, splice: bool) -> Window {
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let max_lines = limit.min(MAX_LINES_RS);
    let anchors = if splice {
        sparse::anchors(text)
    } else {
        Vec::new()
    };
    let mut out = String::new();
    let mut shown = 0usize;
    let mut bytes = 0usize;
    let mut by_bytes = false;
    for at in offset.saturating_sub(1)..total {
        if shown >= max_lines {
            break;
        }
        let line = lines[at];
        let room = MAX_BYTES_RS.saturating_sub(bytes);
        if line.len() > room {
            let cut = floor_boundary(line, room);
            if cut > 0 {
                push(&mut out, at, &line[..cut], splice, &anchors);
            }
            by_bytes = true;
            break;
        }
        push(&mut out, at, line, splice, &anchors);
        shown += 1;
        bytes += line.len() + 1;
    }
    let more = by_bytes || offset.saturating_sub(1) + shown < total;
    Window {
        text: out,
        more,
        by_bytes,
    }
}

struct Streamed {
    text: String,
    lines: usize,
    more: bool,
    by_bytes: bool,
}

/// A file of any other kind, streamed line by line so a huge one never lands in memory whole.
fn streamed(path: &Path, offset: usize, limit: usize, splice: bool) -> Result<Streamed> {
    use std::io::BufRead as _;

    let file = std::fs::File::open(path).context("failed to open file")?;
    let mut reader = std::io::BufReader::new(file);
    let max_lines = limit.min(MAX_LINES);
    let mut buf: Vec<u8> = Vec::new();
    let mut out = String::new();
    let mut line_no = 0usize;
    let mut shown = 0usize;
    let mut bytes = 0usize;
    let mut more = false;
    let mut by_bytes = false;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        line_no += 1;
        if line_no < offset {
            continue;
        }
        let line = std::str::from_utf8(&buf).map_err(|_| anyhow!("not a valid UTF-8 text file"))?;
        let line = line.trim_end_matches(['\n', '\r']);
        if shown >= max_lines {
            more = true;
            break;
        }
        let room = MAX_BYTES.saturating_sub(bytes);
        if line.len() > room {
            let cut = floor_boundary(line, room);
            if cut > 0 {
                push(&mut out, line_no - 1, &line[..cut], splice, &[]);
            }
            more = true;
            by_bytes = true;
            break;
        }
        push(&mut out, line_no - 1, line, splice, &[]);
        bytes += line.len();
        shown += 1;
    }
    // The count in the footer is the file's, not the window's: read out the rest without keeping it.
    while more {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        line_no += 1;
    }
    Ok(Streamed {
        text: out,
        lines: line_no,
        more,
        by_bytes,
    })
}

/// One line, numbered or bare, and blank when it carries no number.
fn push(out: &mut String, at: usize, line: &str, splice: bool, anchors: &[bool]) {
    if !splice {
        out.push_str(line);
        out.push('\n');
    } else if anchors.get(at).copied().unwrap_or(true) {
        out.push_str(&format!("{:>5}\t{line}\n", at + 1));
    } else if line.trim().is_empty() {
        out.push('\n');
    } else {
        out.push_str("     \t");
        out.push_str(line);
        out.push('\n');
    }
}

/// The footer: how much is left and where it starts, and nothing when the file ended.
fn finished(
    body: String,
    offset: usize,
    total: usize,
    more: bool,
    by_bytes: bool,
    cap_bytes: usize,
) -> String {
    let mut text = body;
    if more {
        let shown = text.lines().count();
        let unread = total.saturating_sub(offset.saturating_sub(1) + shown);
        let next = offset + shown;
        if by_bytes {
            text.push_str(&format!(
                "\n[cut at the {cap_bytes} byte cap; {unread} lines unread, next offset: {next}]"
            ));
        } else {
            text.push_str(&format!("\n[{unread} lines unread, next offset: {next}]"));
        }
    }
    text
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
