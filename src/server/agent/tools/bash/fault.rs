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
pub(crate) fn gather(shells: &[Shell], stop: &Stop) -> Vec<(PathBuf, Check)> {
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

/// The audit's pure half: hang the faults under their edits, and the crate's report on the last
/// written answer.
pub(crate) fn finish(shells: &[Shell], checks: &[(PathBuf, Check)], results: &mut [ToolResult]) {
    let unfold = shells.iter().any(|shell| shell.unfold);

    // The faults that land on an edit hang under that edit.
    for (root, check) in checks {
        let Check::Faults { faults, .. } = check else {
            continue;
        };
        for shell in shells.iter().filter(|shell| shell.root == *root) {
            let mut groups: Vec<(usize, Vec<String>)> = Vec::new();
            for fault in faults.iter().filter(|fault| fault.file == shell.mine) {
                if shell.seam || !shell.slots.iter().any(|slot| covers(slot, fault.line)) {
                    let at = shell.slots.first().map(|slot| slot.insert_at).unwrap_or(0);
                    group(&mut groups, at, seam_block(root, fault));
                } else if let Some(slot) = shell.slots.iter().find(|slot| covers(slot, fault.line))
                {
                    group(&mut groups, slot.insert_at, vec![hanging(fault)]);
                }
            }
            groups.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            for (at, lines) in &groups {
                insert(results, &shell.call_id, *at, lines);
            }
        }
    }

    // The crate's report rides on the last written answer.
    let mut report: Vec<String> = Vec::new();
    for (root, check) in checks {
        let name = gate::crate_name(root);
        for shell in shells.iter().filter(|s| s.root == *root && s.parse_failed) {
            report.push(format!(
                "[i] fmt · {} does not parse, written as it stands",
                shell.mine
            ));
        }
        match check {
            Check::Clean => report.push(format!("[i] check · {name} clean")),
            Check::Unchecked(why) => {
                report.push(format!("[i] check · {name} not type-checked: {why}"));
            }
            Check::Faults { faults, warns } => {
                report.push(head(&name, faults.len(), warns.len()));
                report.extend(listing(faults, warns, unfold));
            }
        }
    }
    if let Some(last) = shells.last() {
        append(results, &last.call_id, &report);
    }
}

/// The report's head: clean, the warning count, or the warning count and `Broken`.
fn head(name: &str, faults: usize, warns: usize) -> String {
    if faults == 0 && warns == 0 {
        return format!("[i] check · {name} clean");
    }
    let warns = if warns > 0 {
        format!(" {warns} warns")
    } else {
        String::new()
    };
    let broken = if faults > 0 { ", Broken:" } else { "" };
    format!("[i] check · {name}{warns}{broken}")
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

/// Collect one insert point's lines.
fn group(groups: &mut Vec<(usize, Vec<String>)>, at: usize, lines: Vec<String>) {
    match groups.iter_mut().find(|(held, _)| *held == at) {
        Some((_, held)) => held.extend(lines),
        None => groups.push((at, lines)),
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
