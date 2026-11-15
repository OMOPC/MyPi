//! Rust structure from text: which item a line sits in, and the header each item reads by.
//! Braces are counted on comment- and literal-blanked code; a header runs until its own `{` or `;`.

use crate::server::agent::tools::bash::change::head_of;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::summary;

/// One declaration: its header, its body's ends, and the header as one line.
pub(crate) struct Item {
    /// The header's first line, 0-based.
    pub head: usize,
    /// The line that opens the body; the header's last line when there is none.
    pub open: usize,
    /// The line that closes the body; `open` when there is none.
    pub close: usize,
    /// The whole header on one line, its brace dropped.
    pub sig: String,
}

/// One file's items, and who holds whom.
pub(crate) struct Layout {
    items: Vec<Item>,
    /// Each item's innermost container, by index.
    parent: Vec<Option<usize>>,
}

impl Layout {
    /// One file's items, in the order their headers appear.
    pub(crate) fn analyse(text: &str) -> Self {
        let raw: Vec<&str> = text.lines().collect();
        let code = sparse::stripped(text);
        // The block a `{` opens: two openers on one line close in order, so the last close wins.
        let mut closes: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        for (open, close) in sparse::frames(&code) {
            let slot = closes.entry(open).or_insert(close);
            if close > *slot {
                *slot = close;
            }
        }

        let mut items = Vec::new();
        for (at, line) in code.iter().enumerate() {
            if head_of(line.trim_start()).is_none() {
                continue;
            }
            let Some((open, body)) = span(&code, at) else {
                continue;
            };
            let close = if body {
                closes.get(&open).copied().unwrap_or(open)
            } else {
                open
            };
            let header = header_text(&raw, at, open);
            items.push(Item {
                head: at,
                open,
                close,
                sig: summary::clip(header.trim_end_matches('{').trim_end(), 80),
            });
        }
        let parent = parents(&items);
        Layout { items, parent }
    }

    /// Every item around one 0-based line, outermost first.
    pub(crate) fn around(&self, line: usize) -> Vec<&Item> {
        let mut chain = Vec::new();
        let mut at = self.innermost(line);
        while let Some(index) = at {
            chain.push(&self.items[index]);
            at = self.parent[index];
        }
        chain.reverse();
        chain
    }

    /// The innermost item whose own lines cover `line`.
    fn innermost(&self, line: usize) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (at, item) in self.items.iter().enumerate() {
            if item.head <= line && line <= item.close {
                match best {
                    Some(seen) if self.items[seen].head >= item.head => {}
                    _ => best = Some(at),
                }
            }
        }
        best
    }
}

/// A header's ends: the line the body opens on, and whether there is a body at all.
///
/// Brackets are counted, so a wrapped `where` clause and nested generics stay inside the header;
/// `->` and `=>` contribute no closing angle.
fn span(code: &[String], start: usize) -> Option<(usize, bool)> {
    let mut paren = 0i64;
    let mut square = 0i64;
    let mut angle = 0i64;
    for (at, line) in code.iter().enumerate().skip(start) {
        let chars: Vec<char> = line.chars().collect();
        for (k, c) in chars.iter().enumerate() {
            match c {
                '(' => paren += 1,
                ')' => paren -= 1,
                '[' => square += 1,
                ']' => square -= 1,
                '<' => angle += 1,
                '>' => {
                    if !matches!(chars.get(k.wrapping_sub(1)), Some('-') | Some('=')) {
                        angle -= 1;
                    }
                }
                '{' if paren <= 0 && square <= 0 && angle <= 0 => return Some((at, true)),
                ';' if paren <= 0 && square <= 0 && angle <= 0 => return Some((at, false)),
                _ => {}
            }
        }
    }
    None
}

/// A header's lines joined into one, so a wrapped signature reads as one line.
fn header_text(raw: &[&str], head: usize, open: usize) -> String {
    let mut text = String::new();
    for line in raw.iter().take(open + 1).skip(head) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

/// Each item's innermost container.
fn parents(items: &[Item]) -> Vec<Option<usize>> {
    let mut out = vec![None; items.len()];
    for (at, item) in items.iter().enumerate() {
        let mut best: Option<usize> = None;
        for (other, holder) in items.iter().enumerate() {
            if other == at {
                continue;
            }
            if holder.open < item.head && item.close <= holder.close {
                match best {
                    Some(seen) if items[seen].head >= holder.head => {}
                    _ => best = Some(other),
                }
            }
        }
        out[at] = best;
    }
    out
}
