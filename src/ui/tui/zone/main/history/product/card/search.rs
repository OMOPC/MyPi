//! search: the engine in the header, the words below it, one branch per hit.

use super::super::super::row::{Lead, Row, Run};
use super::frame::wrap_rows;
use super::tree;
use super::{
    Section, ToolBlock, ToolOutcome, ToolRenderer, arg_str, error_body, one_line,
};
use crate::ui::tui::theme::fixed;

pub(crate) struct Search;

impl ToolRenderer for Search {
    fn label(&self, _b: &ToolBlock<'_>) -> Option<String> {
        Some("Search".to_string())
    }

    fn folds(&self) -> bool {
        false
    }

    /// The engine this call ran against, and how much it turned up.
    fn title(&self, b: &ToolBlock<'_>) -> Option<String> {
        let provider = arg_str(b.args, "provider").filter(|p| !p.trim().is_empty())?;
        let count = hits(b.result.map(|o| o.text).unwrap_or_default()).len();
        Some(match count {
            0 => provider,
            n => format!("{provider} · {n} results"),
        })
    }

    /// The words it searched for, on the header's second line.
    fn call_body(&self, b: &ToolBlock<'_>) -> Vec<Row> {
        let Some(query) = arg_str(b.args, "query").filter(|q| !q.trim().is_empty()) else {
            return Vec::new();
        };
        let mut runs = vec![Run::fixed("Query: ", fixed::DIM)];
        runs.push(body(one_line(&query)));
        wrap_rows(vec![Row::of(Lead::None, runs)], b.inner_width())
    }

    fn result_sections(&self, b: &ToolBlock<'_>, o: ToolOutcome<'_>) -> Vec<Section> {
        if o.ok == Some(false) {
            return vec![Section {
                label: None,
                lines: error_body(&o.failure_text()),
            }];
        }
        vec![Section {
            label: None,
            lines: wrap_rows(branches(hits(o.text)), b.inner_width()),
        }]
    }
}

/// Body text: the terminal's own foreground.
fn body(text: impl Into<String>) -> Run {
    Run::fixed(text, fixed::TOOL_TITLE)
}

/// A hit's title and its note: everything after the first `|`.
fn split(title: &str) -> (&str, Option<&str>) {
    match title.split_once('|') {
        Some((head, note)) => (head.trim_end(), Some(note.trim())),
        None => (title.trim(), None),
    }
}

/// One branch per hit: the title, then the page it opens.
fn branches(hits: Vec<(String, String)>) -> Vec<Row> {
    let last = hits.len().saturating_sub(1);
    let mut rows = Vec::new();
    for (at, (title, url)) in hits.iter().enumerate() {
        let (head, note) = split(title);
        let mut runs = vec![body(head)];
        if let Some(note) = note {
            runs.push(Run::fixed(format!(" | {note}"), fixed::MUTED));
        }
        rows.push(tree::entry(at == last, runs));
        if !url.is_empty() {
            rows.push(tree::child(
                at == last,
                vec![Run::fixed(url, fixed::TOOL_PATH)],
            ));
        }
    }
    rows
}

/// A hit's title and URL, in the text a search prints.
fn hits(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let Some(title) = numbered(line) else {
            continue;
        };
        let url = lines[at + 1..]
            .iter()
            .take_while(|next| numbered(next).is_none())
            .map(|next| next.trim())
            .find(|next| next.starts_with("http"))
            .unwrap_or_default();
        out.push((title, url.to_string()));
    }
    out
}

/// The text of a `N. text` line; a line without a number answers nothing.
fn numbered(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || !trimmed[digits..].starts_with(". ") {
        return None;
    }
    Some(trimmed[digits + 2..].to_string())
}
