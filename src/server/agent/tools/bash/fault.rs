//! The batch's shared audit: after a batch's last write, one `cargo check` per crate written into.
//! Faults that land on a written edit hang under it; the crate's report rides on the last written
//! answer.

use std::path::{Path, PathBuf};

use super::gate::{self, Check, Fault, Unparsed};
use super::skeleton;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::contract::env::{Env, Shell, Slot};
use crate::server::agent::tools::contract::output::ToolResult;
use crate::server::platform::stop::Stop;

/// How many lines one error block may take.
const ROOM: usize = 16;

/// A file that did not parse: the formatter's verdict is the whole report.
pub(crate) fn unparsed(why: &Unparsed) -> String {
    let at = match why.line {
        Some(line) => format!("line {line}: "),
        None => String::new(),
    };
    format!(
        "--- [Check] not type-checked: this file does not parse ---\n! {at}{}",
        why.message
    )
}

/// Finish every written call's answer with the batch's one check.
pub(crate) fn settle(env: &mut Env, results: &mut [ToolResult]) {
    let stop = env.stop.clone();
    let shells = std::mem::take(&mut env.shells);
    if shells.is_empty() {
        return;
    }
    if stop.is_stopped() {
        let note = ["[i] not type-checked: the turn was stopped".to_string()];
        for shell in &shells {
            append(results, &shell.call_id, &note);
        }
        return;
    }
    let checks = gather(&shells, &stop);
    finish(&shells, &checks, results);
}

/// One check per crate, in the order the writes named them.
fn gather(shells: &[Shell], stop: &Stop) -> Vec<(PathBuf, Check)> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for shell in shells {
        if !roots.contains(&shell.root) {
            roots.push(shell.root.clone());
        }
    }
    roots
        .into_iter()
        .map(|root| {
            let check = gate::check_crate(&root, stop);
            (root, check)
        })
        .collect()
}

/// The audit's pure half: every diagnostic lands once — under the edit whose lines it hit, under
/// the write that replaced the whole file, or in the crate's own list.
pub(crate) fn finish(shells: &[Shell], checks: &[(PathBuf, Check)], results: &mut [ToolResult]) {
    let unfold = shells.iter().any(|shell| shell.unfold);
    let mut report: Vec<String> = Vec::new();

    for (root, check) in checks {
        let name = gate::crate_name(root);
        for shell in shells.iter().filter(|s| s.root == *root && s.parse_failed) {
            report.push(format!(
                "[i] fmt · {} does not parse, written as it stands",
                shell.mine
            ));
        }
        let (faults, warns) = match check {
            Check::Clean => {
                report.push(format!("[i] check · {name} clean"));
                continue;
            }
            Check::Unchecked(why) => {
                report.push(format!("[i] check · {name} not type-checked: {why}"));
                continue;
            }
            Check::Faults { faults, warns } => (faults, warns),
        };

        let mut groups: Vec<(String, usize, Vec<String>)> = Vec::new();
        let mut lones: (Vec<Fault>, Vec<Fault>) = (Vec::new(), Vec::new());
        for fault in faults {
            place(root, shells, fault, &mut groups, &mut lones.0);
        }
        for fault in warns {
            place(root, shells, fault, &mut groups, &mut lones.1);
        }

        // A later insert point first: the earlier ones keep their line numbers.
        groups.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        for (call_id, at, lines) in &groups {
            insert(results, call_id, *at, lines);
        }

        if !lones.0.is_empty() || !lones.1.is_empty() {
            report.push(head(&name, lones.0.len(), lones.1.len()));
            report.extend(listing(&lones.0, &lones.1, unfold));
        }
    }

    // What the batch still owes the reader rides on the last written answer.
    if let Some(holder) = shells
        .iter()
        .rev()
        .find(|shell| holds(results, &shell.call_id))
    {
        append(results, &holder.call_id, &report);
    }
}

/// Whether one call's answer can still take lines.
fn holds(results: &[ToolResult], call_id: &str) -> bool {
    results
        .iter()
        .any(|result| result.call_id == call_id && result.reason.is_none())
}

/// Where one diagnostic belongs: under the edit whose lines it landed on, under the write that
/// replaced the whole file, or nowhere a shell can hold it.
fn place(
    root: &Path,
    shells: &[Shell],
    fault: &Fault,
    groups: &mut Vec<(String, usize, Vec<String>)>,
    lone: &mut Vec<Fault>,
) {
    let mine = || {
        shells
            .iter()
            .filter(move |shell| shell.root == *root && shell.mine == fault.file)
    };
    if let Some((call_id, at)) = mine().find_map(|shell| {
        let slot = shell.slots.iter().find(|slot| covers(slot, fault.line))?;
        Some((shell.call_id.clone(), slot.insert_at))
    }) {
        group(groups, &call_id, at, vec![hanging(fault)]);
        return;
    }
    if let Some(shell) = mine().rev().find(|shell| shell.seam) {
        let at = shell.slots.first().map(|slot| slot.insert_at).unwrap_or(0);
        group(groups, &shell.call_id, at, seam_block(root, fault));
        return;
    }
    lone.push(fault.clone());
}

/// The report's head: how many of each the crate still owes the reader.
fn head(name: &str, faults: usize, warns: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if faults > 0 {
        parts.push(match faults {
            1 => "1 error".to_string(),
            n => format!("{n} errors"),
        });
    }
    if warns > 0 {
        parts.push(match warns {
            1 => "1 warn".to_string(),
            n => format!("{n} warns"),
        });
    }
    format!("[i] check · {name} {}", parts.join(", "))
}

/// The report's lines: every error, then every warning; folded past the cap unless unfolded.
fn listing(faults: &[Fault], warns: &[Fault], unfold: bool) -> Vec<String> {
    let lines: Vec<String> = faults.iter().chain(warns.iter()).map(one).collect();
    if unfold || lines.len() <= gate::FAULTS {
        return lines;
    }
    let mut out: Vec<String> = lines[..gate::FAULTS].to_vec();
    out.push(format!(
        "(+{} more) — use the Grep tool to catch them",
        lines.len() - gate::FAULTS
    ));
    out
}

/// One report line: where, what, and the compiler's own suggestion.
fn one(fault: &Fault) -> String {
    let mut line = format!("  {}:{}: {}", fault.file, fault.line, fault.message);
    if !fault.helps.is_empty() {
        line.push_str(&format!(" | help: {}", fault.helps.join("; ")));
    }
    line
}

/// One fault's line as it hangs under an edit of a spliced file.
fn hanging(fault: &Fault) -> String {
    let mut line = format!("  ! line {}: {}", fault.line, fault.message);
    if !fault.helps.is_empty() {
        line.push_str(&format!(" | help: {}", fault.helps.join("; ")));
    }
    line
}

/// One fault's lines as they stand under a written file's answer: the source seam and the words.
fn seam_block(root: &Path, fault: &Fault) -> Vec<String> {
    let mut out = vec![format!("--- {}:{} ---", fault.file, fault.line)];
    if let Ok(source) = std::fs::read_to_string(root.join(&fault.file)) {
        out.extend(seam(&source, fault.line));
    }
    out.push(format!("! {}", fault.message));
    for help in &fault.helps {
        out.push(format!("! {help}"));
    }
    out
}

/// Whether a fault's line lands inside one edit's landing.
fn covers(slot: &Slot, line: usize) -> bool {
    matches!(slot.landing, Some((first, last)) if first <= line && line <= last)
}

/// Collect one insert point's lines, in the order the compiler named them.
fn group(
    groups: &mut Vec<(String, usize, Vec<String>)>,
    call_id: &str,
    at: usize,
    lines: Vec<String>,
) {
    match groups
        .iter_mut()
        .find(|(call, held, _)| call.as_str() == call_id && *held == at)
    {
        Some((_, _, held)) => held.extend(lines),
        None => groups.push((call_id.to_string(), at, lines)),
    }
}

/// Insert whole lines into one answer's text; a refused or missing answer stays as it is.
fn insert(results: &mut [ToolResult], call_id: &str, at: usize, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let Some(result) = results
        .iter_mut()
        .find(|result| result.call_id == call_id && result.reason.is_none())
    else {
        return;
    };
    let mut text: Vec<String> = result.result.lines().map(str::to_string).collect();
    let at = at.min(text.len());
    for (offset, line) in lines.iter().enumerate() {
        text.insert(at + offset, line.clone());
    }
    result.result = text.join("\n");
}

/// Append whole lines to one answer's text, on the same terms.
fn append(results: &mut [ToolResult], call_id: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let Some(result) = results
        .iter_mut()
        .find(|result| result.call_id == call_id && result.reason.is_none())
    else {
        return;
    };
    let mut text = result.result.clone();
    for line in lines {
        text.push('\n');
        text.push_str(line);
    }
    result.result = text;
}

/// The error's own lines, two above and two below, and the item they sit in.
fn seam(text: &str, line: usize) -> Vec<String> {
    let raw: Vec<&str> = text.lines().collect();
    let code = sparse::stripped(text);
    let closes = skeleton::closes(&code);
    let mut out: Vec<String> = Vec::new();
    let width = raw.len().to_string().len().max(3);
    let spot = line.saturating_sub(1);
    if let Some(block) = skeleton::innermost(&raw, &code, &closes, spot) {
        out.push(format!(
            "in: {} ({}..{})",
            block.head,
            block.start + 1,
            block.end + 1
        ));
    }
    let mut rows = skeleton::seam(&raw, line, line, width);
    rows.truncate(ROOM);
    out.extend(rows);
    out
}
