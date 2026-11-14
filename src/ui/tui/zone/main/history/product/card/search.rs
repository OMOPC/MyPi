//! search: the query as a title, then one branch per hit.

use super::super::super::row::{Lead, Row, Run};
use super::tree;
use super::{
    Section, ToolBlock, ToolOutcome, ToolRenderer, arg_str, error_body, one_line, truncate,
};
use crate::ui::tui::text::display_width;
use crate::ui::tui::theme::fixed;

pub(crate) struct Search;

impl ToolRenderer for Search {
    fn label(&self, _b: &ToolBlock<'_>) -> Option<String> {
        Some("Search".to_string())
    }

    /// The query, and how much it turned up.
    fn title(&self, b: &ToolBlock<'_>) -> Option<String> {
        let query = arg_str(b.args, "query").filter(|q| !q.trim().is_empty())?;
        let mut parts = vec![truncate(&one_line(&query), 60)];
        let count = hits(b.result.map(|o| o.text).unwrap_or_default()).len();
        if count > 0 {
            parts.push(format!("{count} results"));
        }
        Some(parts.join(" · "))
    }

    /// The engine, on the header's second line.
    fn call_body(&self, b: &ToolBlock<'_>) -> Vec<Row> {
        let Some(provider) = arg_str(b.args, "provider").filter(|p| !p.trim().is_empty()) else {
            return Vec::new();
        };
        vec![Row::of(
            Lead::None,
            vec![
                Run::fixed("From provider: ", fixed::DIM),
                Run::fixed(provider, fixed::MUTED),
            ],
        )]
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
            lines: search_lines(hits(o.text), b.inner_width()),
        }]
    }
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

fn domain_of(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = rest.split('/').next().unwrap_or_default();
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

/// One branch per hit: the title, then the domain it came from.
fn search_lines(hits: Vec<(String, String)>, inner: usize) -> Vec<Row> {
    let last = hits.len().saturating_sub(1);
    hits.iter()
        .enumerate()
        .map(|(at, (title, url))| {
            let domain = domain_of(url);
            let tail = if domain.is_empty() {
                String::new()
            } else {
                format!(" ({domain})")
            };
            let budget = inner.saturating_sub(4 + display_width(&tail) + 1).max(12);
            let mut runs = vec![Run::fixed(truncate(title, budget), fixed::TOOL_OUTPUT)];
            if !tail.is_empty() {
                runs.push(tree::value(tail));
            }
            tree::entry(at == last, runs)
        })
        .collect()
}

