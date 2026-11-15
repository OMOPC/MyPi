//! Rust structure from text: which items a line sits in, and what each item binds.
//! Braces are counted on comment- and literal-blanked code; a header runs until its own `{` or `;`.

use std::collections::BTreeMap;

use crate::server::agent::tools::bash::change::head_of;
use crate::server::agent::tools::bash::read::sparse;
use crate::server::agent::tools::summary;

/// One declaration: its header, its body's ends, and the name it binds.
pub(crate) struct Item {
    /// The header's first line, 0-based.
    pub head: usize,
    /// The line that opens the body; the header's last line when there is none.
    pub open: usize,
    /// The line that closes the body; `open` when there is none.
    pub close: usize,
    /// The head word: `fn`, `impl`, `struct`, …
    pub kind: &'static str,
    /// What the header binds: `impl CdpSession` → `CdpSession`, `fn send` → `send`.
    pub name: Option<String>,
    /// The trait an `impl` block implements, when it names one.
    pub trait_name: Option<String>,
    /// The whole header on one line.
    pub sig: String,
}

impl Item {
    /// How many lines the body spans; zero for `struct S;` and a trait's `fn f();`.
    pub fn body(&self) -> usize {
        self.close.saturating_sub(self.open)
    }

    /// `impl CdpSession`, `fn send_cmd`, `struct Effect`.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!("{} {name}", self.kind),
            None => self.kind.to_string(),
        }
    }
}

/// One file's items, and who holds whom.
pub(crate) struct Layout {
    pub items: Vec<Item>,
    /// Each item's innermost container, by index.
    parent: Vec<Option<usize>>,
}

impl Layout {
    /// The innermost item whose own lines cover `line`.
    pub fn innermost(&self, line: usize) -> Option<usize> {
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

    /// Every item around one line, outermost first.
    pub fn around(&self, line: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut at = self.innermost(line);
        while let Some(index) = at {
            chain.push(index);
            at = self.parent[index];
        }
        chain.reverse();
        chain
    }

    /// The items this one directly holds.
    pub fn inner(&self, outer: usize) -> Vec<&Item> {
        self.parent
            .iter()
            .enumerate()
            .filter(|(_, parent)| **parent == Some(outer))
            .map(|(at, _)| &self.items[at])
            .collect()
    }

}

/// One file's items, in the order their headers appear.
pub(crate) fn analyse(text: &str) -> Layout {
    let raw: Vec<&str> = text.lines().collect();
    let code = sparse::stripped(text);
    // The block a `{` opens: two openers on one line close in order, so the last close wins.
    let mut closes: BTreeMap<usize, usize> = BTreeMap::new();
    for (open, close) in sparse::frames(&code) {
        let slot = closes.entry(open).or_insert(close);
        if close > *slot {
            *slot = close;
        }
    }

    let mut items = Vec::new();
    for (at, line) in code.iter().enumerate() {
        let Some(kind) = head_of(line.trim_start()) else {
            continue;
        };
        let Some((open, body)) = span(&code, at) else {
            continue;
        };
        let close = if body {
            closes.get(&open).copied().unwrap_or(open)
        } else {
            open
        };
        let header = header_text(&raw, at, open);
        let (name, trait_name) = names_of(kind, &header);
        items.push(Item {
            head: at,
            open,
            close,
            kind,
            name,
            trait_name,
            sig: summary::clip(&header, 200),
        });
    }
    let parent = parents(&items);
    Layout { items, parent }
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

/// What a header binds: the item's own name, and the trait an `impl` names.
fn names_of(kind: &'static str, header: &str) -> (Option<String>, Option<String>) {
    if kind == "impl" {
        return impl_names(header);
    }
    let name = match kind {
        "use" | "extern" | "macro" => None,
        "macro_rules!" => after_word(header, "macro_rules!").and_then(bare_name),
        _ => after_word(header, kind).and_then(bare_name),
    };
    (name, None)
}

/// `impl Tr for A` → `(A, Tr)`; `impl A` → `(A, None)`. A higher-ranked bound writes `for<'a>`,
/// with no space before its angle, so only the spaced `for` splits a header.
fn impl_names(header: &str) -> (Option<String>, Option<String>) {
    match header.rfind(" for ") {
        Some(at) => (type_name(&header[at + 5..]), own_target(&header[..at])),
        None => (own_target(header), None),
    }
}

/// The text after the head word, once its modifiers are behind it.
fn after_word<'a>(header: &'a str, word: &str) -> Option<&'a str> {
    let at = header.find(word)?;
    Some(&header[at + word.len()..])
}

/// `impl A` → `A`; `impl<T> A<T>` → `A`.
fn own_target(header: &str) -> Option<String> {
    type_name(after_word(header, "impl")?)
}

/// The bare type a header names: references, `!`, `mut`, `dyn`, lifetimes and generics all go.
fn type_name(text: &str) -> Option<String> {
    let mut rest = text.trim();
    loop {
        let trimmed = rest.trim_start();
        if let Some(next) = trimmed.strip_prefix('&') {
            rest = next;
            continue;
        }
        if let Some(next) = trimmed.strip_prefix('!') {
            rest = next;
            continue;
        }
        if let Some(next) = trimmed.strip_prefix('<') {
            rest = skip_angle(next)?;
            continue;
        }
        if let Some(next) = trimmed.strip_prefix("mut ") {
            rest = next;
            continue;
        }
        if let Some(next) = trimmed.strip_prefix("dyn ") {
            rest = next;
            continue;
        }
        if let Some(next) = trimmed.strip_prefix("for ") {
            rest = next;
            continue;
        }
        if trimmed.starts_with('\'') {
            let after = &trimmed[1..];
            let len = after
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            rest = &after[len..];
            continue;
        }
        rest = trimmed;
        break;
    }
    let head = rest
        .split(['<', '(', '[', '+', ' ', '\n'])
        .next()
        .unwrap_or("")
        .trim();
    let segment = head.rsplit("::").next().unwrap_or("").trim();
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => Some(segment.to_string()),
        _ => None,
    }
}

/// Past a generic list: the text after its matching `>`.
fn skip_angle(text: &str) -> Option<&str> {
    let mut depth = 1i64;
    let chars: Vec<char> = text.chars().collect();
    for (k, c) in chars.iter().enumerate() {
        match c {
            '<' => depth += 1,
            '>' => {
                if !matches!(chars.get(k.wrapping_sub(1)), Some('-') | Some('=')) {
                    depth -= 1;
                }
                if depth == 0 {
                    let offset: usize = chars[..=k].iter().map(|c| c.len_utf8()).sum();
                    return Some(&text[offset..]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The first identifier word of a header's tail: `MyStruct<T>` → `MyStruct`, `a;` → `a`.
fn bare_name(tail: &str) -> Option<String> {
    let word: String = tail
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!word.is_empty()).then_some(word)
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

/// Lines inside one item's body whose own line binds `name`: a variant, a field, a const.
pub(crate) fn members(raw: &[&str], item: &Item, name: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for at in item.open + 1..item.close.min(raw.len()) {
        let line = raw[at].trim();
        let Some(rest) = line.strip_prefix(name) else {
            continue;
        };
        let next = rest.chars().next();
        if matches!(
            next,
            None | Some(' ')
                | Some(',')
                | Some('{')
                | Some('(')
                | Some('=')
                | Some(':')
                | Some(';')
                | Some('<')
        ) {
            out.push(at);
        }
    }
    out
}
