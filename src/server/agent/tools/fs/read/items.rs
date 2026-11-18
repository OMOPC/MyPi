// The item mode: a header the model remembers, answered by every piece whose own text says it.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::tool::{MAX_BYTES_SOURCE, MAX_LINES_SOURCE, header};
use crate::server::agent::tools::contract::env::{Env, License};
use crate::server::agent::tools::fs::lang::graph::loader;
use crate::server::agent::tools::fs::lang::graph::parts;
use crate::server::agent::tools::fs::lang::{self, Line, Table};
use crate::server::agent::tools::fs::skeleton;
use crate::server::agent::tools::summary;
use crate::server::agent::tools::workspace::canonicalize_lenient;

// Every piece under `path` whose own text says `wanted`, whole, every line numbered.
pub(crate) fn answer(env: &mut Env, path: &Path, wanted: &str) -> Result<String> {
    let cwd = canonicalize_lenient(&env.workspace.cwd());
    let (text, seen) = render(path, &cwd, wanted)?;
    for (file, body) in &seen {
        stamp(env, file, body);
    }
    Ok(text)
}

// The answer, and the files whose pieces it drew.
fn render(path: &Path, cwd: &Path, wanted: &str) -> Result<(String, Vec<(PathBuf, String)>)> {
    let needle = summary::flat(wanted);
    anyhow::ensure!(!needle.is_empty(), "`item` names no text");

    let mut out = String::new();
    let mut seen: Vec<(PathBuf, String)> = Vec::new();
    let mut room = Room::default();
    for file in scope(path) {
        let Some(text) = loader::text(&file) else {
            continue;
        };
        let table = lang::table(&file, &text);
        let hits: Vec<usize> = table
            .pieces()
            .into_iter()
            .filter(|(_, line)| says(line, &needle))
            .map(|(at, _)| at)
            .collect();
        if hits.is_empty() {
            continue;
        }
        let raw: Vec<&str> = text.lines().collect();
        let width = skeleton::width(raw.len());

        // A directory names the file, because one header may stand in several of them.
        let mut opened = false;
        for at in hits {
            let block = block(&raw, &table, at, width);
            let head = match opened || !path.is_dir() {
                true => String::new(),
                false => header(&parts::rel(cwd, &file)),
            };
            let sep = usize::from(!out.is_empty());
            let bytes = head.len() + block.len() + sep;
            let lines = head.lines().count() + block.lines().count() + sep;

            if !room.fits(lines, bytes) {
                room.left += 1;
                continue;
            }
            room.take(lines, bytes);

            if sep == 1 {
                out.push('\n');
            }
            opened = true;
            out.push_str(&head);
            out.push_str(&block);
        }
        if opened {
            seen.push((file, text));
        }
    }

    if out.is_empty() {
        return Ok((empty(path, &needle, cwd), seen));
    }
    if room.left > 0 {
        out.push_str(&format!("[+{} more items]\n", room.left));
    }
    Ok((out, seen))
}

// What one answer may still spend: lines and bytes both.
#[derive(Default)]
struct Room {
    lines: usize,
    bytes: usize,

    // Items the budget pushed away.
    left: usize,
}

impl Room {
    fn fits(&self, lines: usize, bytes: usize) -> bool {
        self.lines + lines <= MAX_LINES_SOURCE && self.bytes + bytes <= MAX_BYTES_SOURCE
    }

    fn take(&mut self, lines: usize, bytes: usize) {
        self.lines += lines;
        self.bytes += bytes;
    }
}

// The files the call looks through: the one it named, or every sorted file under the directory.
fn scope(path: &Path) -> Vec<PathBuf> {
    match path.is_dir() {
        true => lang::files(path),
        false => vec![path.to_path_buf()],
    }
}

// One piece whole: its own header with the piece above it, its span, and every line numbered.
fn block(raw: &[&str], table: &Table, at: usize, width: usize) -> String {
    let end = table.end(at).unwrap_or(at).min(raw.len().saturating_sub(1));
    let exact = table
        .line(at)
        .and_then(|line| line.exact.as_deref())
        .unwrap_or("");

    let mut out = match outer(table, at) {
        Some(outer) => format!("{exact} ({outer})  {}..{}\n", at + 1, end + 1),
        None => format!("{exact}  {}..{}\n", at + 1, end + 1),
    };
    for index in at..=end {
        match raw[index].trim().is_empty() {
            true => out.push('\n'),
            false => {
                out.push_str(&skeleton::numbered(width, Some(index + 1), raw[index]));
                out.push('\n');
            }
        }
    }
    out
}

// The piece directly holding one piece, as its own text reads.
fn outer(table: &Table, at: usize) -> Option<&str> {
    let chain = table.around(at);
    let spot = chain.iter().position(|(open, _)| *open == at)?;
    let (_, line) = chain.get(spot.checked_sub(1)?)?;
    line.exact.as_deref()
}

// Whether this piece's own text carries what the model remembers.
fn says(line: &Line, needle: &str) -> bool {
    line.exact
        .as_deref()
        .map(summary::flat)
        .is_some_and(|exact| exact.contains(needle))
}

// What an answer with nothing in it owes the model.
fn empty(path: &Path, needle: &str, cwd: &Path) -> String {
    let named = parts::rel(cwd, path);
    match path.is_file() && lang::owner(path).is_none() {
        true => format!("[item] {named}: no sorter reads this file\n"),
        false => format!("[item] {needle}: nothing under {named} says it\n"),
    }
}

// The file is one the model has now seen, under `splice`'s own definition.
fn stamp(env: &mut Env, path: &Path, text: &str) {
    env.seen.files.insert(path.to_path_buf(), License::of(text));
}
