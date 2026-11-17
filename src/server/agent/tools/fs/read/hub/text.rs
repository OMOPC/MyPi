//! Line-level text: visibility and attribute stripping, plurals, and the view's clipping.

/// A view longer than these is cut; the whole crate is 456 files today, so nothing real hits it.
const VIEW_LINES: usize = 800;
const VIEW_BYTES: usize = 256 * 1024;

/// The words before an item, dropped: `pub(crate) ` and consecutive `#[…]` hats.
pub(super) fn strip_vis(line: &str) -> &str {
    let line = line.trim_start();
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let rest = rest.trim_start();
    if let Some(inner) = rest.strip_prefix('(')
        && let Some(end) = inner.find(')')
    {
        return inner[end + 1..].trim_start();
    }
    rest
}

pub(super) fn strip_attrs(mut line: &str) -> &str {
    loop {
        let line_trimmed = line.trim_start();
        if !line_trimmed.starts_with("#[") && !line_trimmed.starts_with("#![") {
            return line_trimmed;
        }
        match line_trimmed.find(']') {
            Some(end) => line = &line_trimmed[end + 1..],
            None => return line_trimmed,
        }
    }
}

/// A count and its word: `1 file`, `3 files`.
pub(crate) fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

/// A list on one line, cut at a character budget with the rest counted.
pub(super) fn joined(items: &[String]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    const MAX: usize = 300;
    let mut out = String::new();
    let mut used = 0usize;
    for (at, item) in items.iter().enumerate() {
        let add = item.chars().count() + usize::from(at > 0) * 2;
        if used + add > MAX {
            out.push_str(&format!(" [+{} more]", items.len() - at));
            break;
        }
        if at > 0 {
            out.push_str("; ");
        }
        out.push_str(item);
        used += add;
    }
    out
}

/// A line over the view's own width: its tail counted, never a silent ellipsis.
pub(super) fn clip(text: String) -> String {
    const MAX: usize = 400;
    let count = text.chars().count();
    if count <= MAX {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(MAX)
        .map(|(at, _)| at)
        .unwrap_or(text.len());
    format!("{}[+{}]", &text[..cut], count - MAX)
}

pub(super) fn cut(text: String) -> String {
    let lines = text.lines().count();
    if lines <= VIEW_LINES && text.len() <= VIEW_BYTES {
        return text;
    }
    let mut out = String::new();
    for line in text.lines().take(VIEW_LINES) {
        if out.len() + line.len() + 1 > VIEW_BYTES {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    let kept = out.lines().count();
    out.push_str(&format!("[+{} lines]\n", lines.saturating_sub(kept)));
    out
}
