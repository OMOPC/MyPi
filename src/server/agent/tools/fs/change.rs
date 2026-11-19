// One line-addressed change: the piece around it, its line numbers, and the one-line receipt.

use serde_json::{Value, json};

use super::lang::Table;

// Lines a window keeps above and below the change.
const WINDOW: usize = 5;

// The context of one change: `{"scope": [...], "excerpt": {"from": n, "lines": [...]}}`. `at` is the first changed line, 1-based; `span` is how many old lines the change covers; `cap` is how long a piece may be and still be handed over whole. The excerpt is the piece holding the change, or a window around it when no piece holds it or the piece is longer than `cap`.
pub(crate) fn context(table: &Table, text: &str, at: usize, span: usize, cap: usize) -> Value {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return json!({ "scope": [], "excerpt": { "from": 1, "lines": [] } });
    }
    let at = at.clamp(1, lines.len());
    let end = (at + span.max(1) - 1).min(lines.len());
    let piece = table.innermost(at - 1);
    let holds = piece.is_some_and(|(first, line)| {
        let last = line.mate.unwrap_or(first);
        first < at && last + 1 >= end && last - first < cap.max(1)
    });

    // The breadcrumb, nearest first: the pieces holding the change, and the piece itself once a window stands in for it.
    let mut scope: Vec<String> = match piece {
        Some((first, _)) => {
            let mut above = table.around(first);
            above.pop();
            above
                .iter()
                .rev()
                .filter_map(|(_, line)| line.exact.clone())
                .collect()
        }
        None => Vec::new(),
    };
    let (from, to) = match piece {
        Some((first, line)) if holds => (first, line.mate.unwrap_or(first)),
        Some((_, line)) => {
            if let Some(exact) = &line.exact {
                scope.insert(0, exact.clone());
            }
            window(at, end, lines.len())
        }
        None => window(at, end, lines.len()),
    };

    let cut = scope.len() > 3;
    scope.truncate(3);
    scope.reverse();
    if cut {
        scope.insert(0, "…".to_string());
    }

    json!({
        "scope": scope,
        "excerpt": { "from": from + 1, "lines": lines[from..=to] },
    })
}

// The window around a change: `WINDOW` lines above it and below it, 0-based and inclusive.
fn window(at: usize, end: usize, total: usize) -> (usize, usize) {
    (
        at.saturating_sub(WINDOW).max(1) - 1,
        (end + WINDOW).min(total) - 1,
    )
}

// The one line a model reads back: the lines that went, the lines that came, what moved below.
pub(crate) fn receipt(from: usize, old_lines: usize, new_lines: usize, total: usize) -> String {
    let to = from + old_lines.saturating_sub(1);
    let below = new_lines as i64 - old_lines as i64;
    format!(
        "replaced lines {from}-{to} ({old_lines}) with {new_lines} lines; below {below:+}; \
         file now {total} lines"
    )
}

// Which line (1-based) a byte position sits on.
pub(crate) fn line_of(text: &str, pos: usize) -> usize {
    text[..pos.min(text.len())].matches('\n').count() + 1
}

// A text's lines, with no empty line for the newline that ends it.
pub(crate) fn lines_of(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
}
