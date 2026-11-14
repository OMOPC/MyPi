//! web: one line per operation, except `eval`, whose script is its own body.

use super::super::super::prefetch::color::code;
use super::super::super::row::{Lead, Row, Run};
use super::{
    Section, State, ToolBlock, ToolOutcome, ToolRenderer, arg_bool, arg_scalar, arg_str, error_body,
    error_under, head, non_empty_lines, one_line, output_lines, partial, short_url, truncate,
};
use crate::ui::tui::text::glyphs as g;
use crate::ui::tui::theme::fixed;

/// The one-line form every operation but `eval` takes.
pub(crate) fn inline(args: &str, result: Option<ToolOutcome<'_>>) -> Option<Vec<Row>> {
    let op = op_of(args);
    // A script is a body, not a label: `eval` opens the card and shows the code.
    if op == "eval" {
        return None;
    }
    let state = State::of(result);
    let mut runs = head(state, &label_of(&op));
    let mut meta: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    match op.as_str() {
        "read" => {
            let target = arg_str(args, "url")
                .filter(|u| !u.trim().is_empty())
                .map(|u| short_url(&u))
                .unwrap_or_else(|| "page".to_string());
            values.push(target);
            if arg_bool(args, "raw") == Some(true) {
                meta.push("raw".to_string());
            }
            if let Some(o) = result.filter(|o| o.ok == Some(true)) {
                meta.push(format!("{} lines", non_empty_lines(o.text)));
            }
        }
        "open" => {
            if let Some(u) = arg_str(args, "url").filter(|u| !u.trim().is_empty()) {
                values.push(short_url(&u));
            }
        }
        "click" | "fill" | "select" => {
            if let Some(sel) = arg_str(args, "selector").filter(|s| !s.trim().is_empty()) {
                values.push(truncate(&one_line(&sel), 48));
            }
        }
        "scroll" => {
            values.push(format!(
                "{}px",
                arg_scalar(args, "pixels").unwrap_or_else(|| "600".to_string())
            ));
        }
        "screenshot" => {
            if arg_bool(args, "full_page") == Some(true) {
                meta.push("full page".to_string());
            }
            if let Some(o) = result.filter(|o| o.ok == Some(true))
                && let Some(path) = o.text.split(" -> ").nth(1)
            {
                values.push(truncate(&one_line(path), 60));
            }
        }
        other if !other.is_empty() => values.push(other.to_string()),
        _ => {}
    }
    if !values.is_empty() {
        runs.push(Run::fixed(format!(" {}", values.join(" ")), fixed::MUTED));
    }
    for m in meta {
        runs.push(Run::fixed(format!(" {} {m}", g::SEP), fixed::DIM));
    }
    let mut out = vec![Row::of(Lead::None, runs)];
    out.extend(error_under(result));
    Some(out)
}

/// `eval` alone opens a card: the script in the body, its answer below.
pub(crate) struct Web;

impl ToolRenderer for Web {
    fn label(&self, b: &ToolBlock<'_>) -> Option<String> {
        (op_of(b.args) == "eval").then(|| "Web Eval".to_string())
    }

    fn call_body(&self, b: &ToolBlock<'_>) -> Vec<Row> {
        let Some(script) = partial::text(b.args, "value") else {
            return Vec::new();
        };
        script
            .lines()
            .map(|line| code::line(line, Some("js")))
            .collect()
    }

    fn result_sections(&self, b: &ToolBlock<'_>, o: ToolOutcome<'_>) -> Vec<Section> {
        if o.ok == Some(false) {
            return vec![Section {
                label: None,
                lines: error_body(&o.failure_text()),
            }];
        }
        match op_of(b.args).as_str() {
            "eval" => vec![Section {
                label: None,
                lines: output_lines(o.text),
            }],
            _ => Vec::new(),
        }
    }
}

fn op_of(args: &str) -> String {
    arg_str(args, "type").unwrap_or_default()
}

fn label_of(op: &str) -> String {
    let mut chars = op.chars();
    match chars.next() {
        Some(c) => format!("Web {}{}", c.to_uppercase(), chars.as_str()),
        None => "Web".to_string(),
    }
}

