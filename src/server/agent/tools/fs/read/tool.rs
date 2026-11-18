//! The `read` tool: files by line numbers, directories by name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};

use super::{dir, items, sniff};
use crate::server::agent::request::contract::ItemOk;
use crate::server::agent::request::message::ImagePart;
use crate::server::agent::tools::contract::env::{Env, License};
use crate::server::agent::tools::contract::output::ToolOutput;
use crate::server::agent::tools::contract::view::CallRecord;
use crate::server::agent::tools::fs::lang::graph::{self, Loader};
use crate::server::agent::tools::fs::lang::{self, Table};
use crate::server::agent::tools::fs::splice::TOOL as SPLICE;
use crate::server::agent::tools::spec::{Args, Item, Kind, Param, Tool, ToolProgress};
use crate::server::agent::tools::summary;
use base64::Engine as _;

const FILES: usize = 0;

/// A source file's own budget: 800 lines, 256 KiB.
pub(crate) const MAX_LINES_SOURCE: usize = 800;
pub(crate) const MAX_BYTES_SOURCE: usize = 256 * 1024;

/// Every other file's budget: 200 lines, 64 KiB.
const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 64 * 1024;

/// An image's byte cap: a bigger file fails with the cap named.
const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

const FILE_FIELDS: &[Param] = &[
    Param::new("path", Kind::Text, "File or directory path."),
    Param::df("offset", Kind::Line, "First line to read (1-based).", "1"),
    Param::new(
        "limit",
        Kind::Number,
        "Max lines this file may take; a source file answers 800 when this is left out, anything else \
         200, and neither cap moves.",
    ),
    Param::new(
        "item",
        Kind::Text,
        "A piece's own header, as the model remembers it: every piece under this path whose header \
         says that text answers whole, every line numbered. Omitted: the path answers as a file or a \
         listing, and `offset` and `limit` hold.",
    ),
];

const PARAMS: &[Param] = &[Param::new(
    "files",
    Kind::Objects(FILE_FIELDS),
    "One item per path, answered in this order. One item's failure never stops the ones after it.",
)];

pub(crate) const TOOL: Tool = Tool {
    name: "read",
    about: "Read files or directories; a sorted language answers with key line numbers under \
            splice, a source file with an 800-line budget and a module view when it has one, \
            anything else 200.",
    fields: "`files` takes one item per path, with `offset` (1-based, default 1), `limit` (default: \
             800 for a source file, 200 otherwise, never above that file's own cap) and `item`. \
             Without `item`, a file answers its lines and a directory its entries. A directory \
             answers its immediate children, files with their line counts and directories by name, \
             then opens each child one level further, folding a directory of ten files or more into \
             counts. A level whose upper-case `.md` documents are newer than every file beside it \
             drops the stamps and marks them `(latest)`; a document the user keeps current is \
             marked `(persistent)` and never dated. Any other level stamps every file of it. The \
             session root carries a bonus. With `item`, the path is a scope instead: every piece \
             under it whose own header says that text answers whole, every line numbered, in line \
             order; a directory names each answer's file.",
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
    let mut images: Vec<ImagePart> = Vec::new();
    for (item, target) in items.iter().zip(targets) {
        let result = match target {
            Ok(path) => match item.opt_str("item")? {
                Some(wanted) => named(env, item, &path, &wanted, total > 1),
                None => block(env, &batch, item, &path, total > 1),
            },
            Err(e) => Err(e),
        };
        match result {
            Ok((text, attached)) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text);
                images.extend(attached);
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
    produced.images = images;
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

fn named(
    env: &mut Env,
    item: &Item,
    path: &Path,
    wanted: &str,
    headed: bool,
) -> Result<(String, Vec<ImagePart>)> {
    let raw = item.text("path")?;
    let text = items::answer(env, path, wanted)?;
    let answered = if headed {
        format!("==> {raw}\n{text}")
    } else {
        text
    };
    Ok((answered, Vec::new()))
}

/// An image's answer, when the file is one and this session's model can see: a one-line card and
/// the bytes as a part. A non-image, or a model without vision, answers `None`.
fn image(env: &Env, path: &Path) -> Result<Option<(String, ImagePart)>> {
    let Some(format) = sniff::detect(path)? else {
        return Ok(None);
    };
    anyhow::ensure!(
        env.vision,
        "the session's model takes no images: `{}` answers text only",
        path.display()
    );
    let bytes = std::fs::read(path).context("failed to read image")?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_IMAGE_BYTES,
        "the image is {} MiB; the cap is {} MiB",
        bytes.len() / (1024 * 1024),
        MAX_IMAGE_BYTES / (1024 * 1024),
    );
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let card = format!(
        "[image: {format}, {} KiB — attached as an image part]",
        bytes.len() / 1024
    );
    Ok(Some((
        card,
        ImagePart {
            format: format.to_string(),
            data,
        },
    )))
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
) -> Result<(String, Vec<ImagePart>)> {
    let raw = item.text("path")?;
    let offset = item.line("offset")?;

    if path.is_dir() {
        let text = dir::listing(path, &env.workspace.cwd(), !headed, &env.persistent_docs)?;
        let listed = if headed {
            format!("==> {raw}\n{text}")
        } else {
            text
        };
        return Ok((listed, Vec::new()));
    }

    if let Some((card, image)) = image(env, path)? {
        let text = if headed {
            format!("==> {raw}\n{card}")
        } else {
            card
        };
        return Ok((text, vec![image]));
    }

    let splice = env.allows(SPLICE.name);
    let language = lang::owner(path).is_some();
    let source = lang::source(path);
    // A sorted language is read whole so its key lines can be numbered.
    let limit = window(item, if source { MAX_LINES_SOURCE } else { MAX_LINES })?;
    let whole = if language {
        std::fs::read_to_string(path).ok()
    } else {
        None
    };
    let body = match &whole {
        Some(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len();
            let table = lang::table(path, text);
            let numbers = match splice {
                true => Numbers::Table(&table),
                false => Numbers::None,
            };
            assemble(
                offset,
                total,
                limit,
                lines.into_iter().skip(offset.saturating_sub(1)),
                &numbers,
                if source { MAX_BYTES_SOURCE } else { MAX_BYTES },
            )
        }
        None => {
            let streamed = streamed(path, offset, limit)?;
            // Only a window is shown, but a later splice still needs the whole-file licence.
            if let Ok(all) = std::fs::read_to_string(path) {
                env.seen.files.insert(path.to_path_buf(), License::of(&all));
            }
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
        env.seen.files.insert(path.to_path_buf(), License::of(text));
    }

    // The status line is part of the body: it comes first, so the body is known before it is read.
    let mut text = String::new();
    if let Some(status) = &body.status {
        text.push_str(status);
        text.push('\n');
    }
    text.push_str(&body.text);

    if let Some(facts) = lang::hub(path)
        && let Some(unit) = graph::unit(facts, path)
    {
        let loader = Loader::new();
        let is_entry = unit.entry.as_deref() == Some(path);
        if is_entry {
            text.push_str(&sketch(facts, &loader, path, batch));
            env.seen.dirs.insert(unit.place);
        } else if env.seen.dirs.insert(unit.place) {
            text.push_str(&sketch(facts, &loader, path, batch));
        }
    }

    Ok((
        if headed {
            format!("==> {raw}\n{text}")
        } else {
            text
        },
        Vec::new(),
    ))
}

// One file's neighbourhood, printed and bounded on its own.
fn sketch(facts: &lang::Hub, loader: &Loader, path: &Path, batch: &BTreeSet<PathBuf>) -> String {
    let Some(around) = graph::build(facts, loader, path) else {
        return String::new();
    };
    let mut text = String::new();
    for row in graph::render(facts, &around, loader, batch) {
        if let Some(heading) = row.heading {
            text.push_str(&heading);
            text.push('\n');
        }
        text.push_str(&row.text);
        text.push('\n');
    }
    let mut text = graph::parts::cut(text, 800, 256 * 1024);
    text.push('\n');
    text
}

/// How a file's lines are prefixed, if at all.
enum Numbers<'a> {
    /// A session with `edit` and without `splice`: no numbers.
    None,
    /// Another language under `splice`: every line carries one.
    All,
    /// A sorted language under `splice`: only the key lines do.
    Table(&'a Table),
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
            Numbers::Table(table) => table.key(at.saturating_sub(1)),
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
