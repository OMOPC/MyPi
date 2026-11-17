//! One line-addressed change: the item around it, its line numbers, and the one-line receipt.

use serde_json::{Value, json};

/// Words that begin an item once its modifiers are behind it.
const HEADS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "union",
    "trait",
    "impl",
    "mod",
    "use",
    "const",
    "static",
    "type",
    "extern",
    "macro_rules!",
    "macro",
];

/// Words that may stand before a head, or between two of them (`pub async unsafe fn`).
const MODIFIERS: &[&str] = &[
    "pub", "const", "unsafe", "async", "default", "auto", "gen", "extern",
];

/// Columns of leading whitespace; a tab counts as four.
fn indent(line: &str) -> usize {
    let mut width = 0;
    for ch in line.chars() {
        match ch {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}

/// Whether this line declares an item rather than opening a block.
fn is_item(line: &str) -> bool {
    head_of(line).is_some()
}

/// The head word of a declaration, once its modifiers are behind it; `None` when the line opens
/// no item. `pub async fn` → `fn`, `pub(crate) mod` → `mod`.
pub(crate) fn head_of(line: &str) -> Option<&'static str> {
    for raw in line.split_whitespace() {
        if raw.starts_with('#') || raw.starts_with('!') {
            continue;
        }
        let word: String = raw
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '!')
            .collect();
        if word.is_empty() {
            continue;
        }
        if let Some(head) = HEADS.iter().find(|head| **head == word) {
            return Some(head);
        }
        if MODIFIERS.contains(&word.as_str()) {
            continue;
        }
        return None;
    }
    None
}

/// Whether this line is still the item's own header, a signature wrapped over several lines.
fn continues(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with(')')
        || trimmed.starts_with(']')
        || trimmed.starts_with("->")
        || trimmed.starts_with("where")
}

/// A declaration that follows a blank line, a comment, or an attribute.
fn after_gap(lines: &[&str], at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let above = lines[at - 1].trim_start();
    above.is_empty()
        || above.starts_with("//")
        || above.starts_with("/*")
        || above.starts_with('*')
        || above.starts_with('#')
}

/// The line a wrapped signature starts on: `at` itself unless it is a continuation.
fn header_start(lines: &[&str], mut at: usize) -> usize {
    while at > 0 {
        let above = lines[at - 1].trim_end();
        let current = lines[at].trim_start();
        let joins = current.starts_with(')')
            || current.starts_with(']')
            || current.starts_with("->")
            || current.starts_with("where")
            || above.ends_with('(')
            || above.ends_with(',')
            || above.ends_with("->")
            || above.ends_with('<');
        if !joins {
            break;
        }
        at -= 1;
    }
    at
}

/// Lines a window keeps above and below the change.
const WINDOW: usize = 5;

/// The context of one change: `{"scope": [...], "excerpt": {"from": n, "lines": [...]}}`.
///
/// `at` is the first changed line, 1-based; `span` is how many old lines the change covers; `cap`
/// is how long an item may be and still be handed over whole. The excerpt is the enclosing item, or
/// a window around the change when the item does not hold it or is longer than `cap`.
pub(crate) fn context(text: &str, at: usize, span: usize, cap: usize) -> Value {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return json!({ "scope": [], "excerpt": { "from": 1, "lines": [] } });
    }
    let at = at.clamp(1, lines.len());

    // The blocks above the change, nearest first: every line with a smaller indent than the one
    // already found. Blank lines take no part.
    let mut chain: Vec<usize> = Vec::new();
    let mut width = indent(lines[at - 1]);
    let mut probe = at - 1;
    while probe > 0 {
        probe -= 1;
        let line = lines[probe];
        if line.trim().is_empty() {
            continue;
        }
        let w = indent(line);
        if w < width {
            chain.push(probe);
            width = w;
        }
    }

    // The item is the nearest block that is a declaration; failing that, the nearest one that
    // follows a gap; failing that, the nearest block.
    let first = chain
        .iter()
        .copied()
        .find(|i| is_item(lines[*i]))
        .or_else(|| chain.iter().copied().find(|i| after_gap(&lines, *i)))
        .or_else(|| chain.first().copied())
        .unwrap_or(at - 1);

    // A signature wrapped over several lines puts its item head above the continuation line
    // the indent chain landed on.
    let first = header_start(&lines, first);

    // Where the item ends: the line before the next one at its own indent or shallower.
    let width = indent(lines[first]);
    let mut opened = lines[first].contains('{');
    let mut last = lines.len() - 1;
    for i in first + 1..lines.len() {
        let line = lines[i];
        // A blank line holds no indent of its own: it never ends a block.
        if line.trim().is_empty() {
            continue;
        }
        if !opened {
            if line.contains('{') {
                opened = true;
                continue;
            }
            if continues(line) || indent(line) > width {
                continue;
            }
            last = i - 1;
            break;
        }
        if indent(line) <= width {
            // The item's own closing brace belongs to it; a sibling that starts here does not.
            last = if line.trim_start().starts_with('}') {
                i
            } else {
                i - 1
            };
            break;
        }
    }
    let mut last = last.max(first);
    // Trailing blank lines belong to the gap after the item, not to the item.
    while last > first && lines[last].trim().is_empty() {
        last -= 1;
    }

    // The item holds the change: its first line is at or above it, its last line at or below the
    // change's end. An item that fails either test lost the change, and no diff would show.
    let end = (at + span.max(1) - 1).min(lines.len());
    let holds = first + 1 <= at && last + 1 >= end && last - first + 1 <= cap.max(1);
    let (from, to) = if holds {
        (first, last)
    } else {
        (
            at.saturating_sub(WINDOW).max(1) - 1,
            (end + WINDOW).min(lines.len()) - 1,
        )
    };

    // The breadcrumb: the blocks above the item, outermost first, the nearest three of them.
    // A window is not the item, so the item's own head joins the breadcrumb.
    let mut scope: Vec<String> = chain
        .iter()
        .filter(|i| **i < first)
        .map(|i| lines[*i].trim().trim_end_matches('{').trim().to_string())
        .collect();
    if !holds {
        scope.insert(
            0,
            lines[first].trim().trim_end_matches('{').trim().to_string(),
        );
    }
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

/// The one line a model reads back: the lines that went, the lines that came, what moved below.
pub(crate) fn receipt(from: usize, old_lines: usize, new_lines: usize, total: usize) -> String {
    let to = from + old_lines.saturating_sub(1);
    let below = new_lines as i64 - old_lines as i64;
    format!(
        "replaced lines {from}-{to} ({old_lines}) with {new_lines} lines; below {below:+}; \
         file now {total} lines"
    )
}

/// Which line (1-based) a byte position sits on.
pub(crate) fn line_of(text: &str, pos: usize) -> usize {
    text[..pos.min(text.len())].matches('\n').count() + 1
}

/// A text's lines, with no empty line for the newline that ends it.
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
