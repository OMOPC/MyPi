//! The compiler's report, drawn: one block per error, its line marked, the item it sits in named.

use std::path::Path;

use super::gate::{self, Check};
use super::skeleton;
use crate::server::agent::tools::bash::read::sparse;

/// How many lines one error block may take.
const ROOM: usize = 16;

/// What the compiler said about one changed file, or `None` when it had nothing to add.
///
/// The compiler names its files against the crate root, so every path here is read from there.
pub(crate) fn receipt(path: &Path, text: &str, check: &Check) -> Option<String> {
    let (faults, here, other) = match check {
        Check::Skipped | Check::Clean => return None,
        Check::TimedOut => {
            return Some(format!(
                "--- [Check] cargo check did not finish in {} ms ---",
                gate::CHECK_LIMIT.as_millis()
            ));
        }
        Check::Unchecked => {
            return Some("--- [Check] not type-checked: the turn was stopped ---".to_string());
        }
        Check::Faults { faults, here, other } => (faults, *here, *other),
    };
    let root = gate::crate_root(path)?;
    let mine = gate::shown(&root, path);
    let mut out: Vec<String> = vec![format!("--- [Check] {here} error(s) here, {other} other ---")];
    if here + other > faults.len() {
        out.push(format!("(the first {} are listed)", faults.len()));
    }
    for fault in faults {
        let head = if fault.file == mine {
            format!("--- {}:{} ---", fault.file, fault.line)
        } else {
            format!("--- other: {}:{} ---", fault.file, fault.line)
        };
        out.push(head);
        let source = if fault.file == mine {
            Some(text.to_string())
        } else {
            std::fs::read_to_string(root.join(&fault.file)).ok()
        };
        if let Some(source) = source {
            out.extend(seam(&source, fault.line));
        }
        out.push(format!("! {}", fault.message));
        for help in &fault.helps {
            out.push(format!("! {help}"));
        }
    }
    Some(out.join("\n"))
}

/// A file that did not parse: the formatter's verdict is the whole report.
pub(crate) fn unparsed(message: &str) -> String {
    format!("--- [Check] not type-checked: this file does not parse ---\n! {message}")
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

