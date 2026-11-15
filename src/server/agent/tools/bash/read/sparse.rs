//! Sparse line numbers: the lines a `splice` may start or end on.
//! Comments and literals are blanked before any brace is counted.

use crate::server::agent::tools::bash::change;

/// One boolean per line: does this line carry a number?
pub(crate) fn anchors(text: &str) -> Vec<bool> {
    let raw: Vec<&str> = text.lines().collect();
    let n = raw.len();
    let mut out = vec![false; n];
    if n == 0 {
        return out;
    }
    let code = stripped(text);

    // A declaration's first line.
    let head: Vec<Option<&'static str>> = code
        .iter()
        .map(|line| change::head_of(line.trim_start()))
        .collect();

    // A run of `use` statements is one block: its first and its last line, nothing between.
    let mut in_use = vec![false; n];
    let mut i = 0;
    while i < n {
        if head[i] == Some("use") && !in_use[i] {
            let (end, next) = use_run(&code, &head, i);
            for line in in_use.iter_mut().take(end + 1).skip(i) {
                *line = true;
            }
            out[i] = true;
            out[end] = true;
            i = next;
            continue;
        }
        i += 1;
    }
    for i in 0..n {
        if head[i].is_some() && !in_use[i] {
            out[i] = true;
        }
    }

    // A hat — attributes and doc comments — is one block: its first and its last line, whatever
    // it holds and however many attributes are stacked.
    let mut i = 0;
    while i < n {
        if !is_hat(raw[i].trim_start()) {
            i += 1;
            continue;
        }
        let start = i;
        let mut depth = bracket_delta(&code[i]);
        let mut end = i;
        let mut j = i + 1;
        while j < n {
            let line = raw[j].trim_start();
            if depth > 0 || is_hat(line) {
                depth += bracket_delta(&code[j]);
                end = j;
                j += 1;
                continue;
            }
            break;
        }
        // A hat that owns nothing is prose, and prose gets no number — except at the file's own
        // head, which is always worth a line a change can name.
        if start == 0 || (end + 1 < n && !raw[end + 1].trim().is_empty()) {
            out[start] = true;
            out[end] = true;
        }
        i = end + 1;
    }

    // A long function hands out numbers at the control-flow blocks inside it, whose own ends are
    // stamped by the closing-brace pass below.
    let frames = frames(&code);
    for (open, close) in &frames {
        if head[*open] != Some("fn") || *close <= *open + LONG {
            continue;
        }
        for (at, line) in code.iter().enumerate().take(*close).skip(open + 1) {
            if control_head(line.trim()) {
                out[at] = true;
            }
        }
    }

    // Every block that starts with a number ends with one: the closing brace, twice, so a block
    // opened by the pass above is closed too.
    for _ in 0..2 {
        for (open, close) in &frames {
            if out[*open] {
                out[*close] = true;
            }
        }
    }
    out
}

/// A function at least this long hands out numbers inside its body.
const LONG: usize = 15;

/// One run of consecutive `use` statements: where it ends, and where the next line starts.
fn use_run(code: &[String], head: &[Option<&'static str>], start: usize) -> (usize, usize) {
    let n = code.len();
    let mut i = start;
    loop {
        let mut depth = 0i32;
        let end = loop {
            for ch in code[i].chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if (depth <= 0 && code[i].contains(';')) || i + 1 == n {
                break i;
            }
            i += 1;
        };
        if end + 1 < n && head[end + 1] == Some("use") {
            i = end + 1;
            continue;
        }
        return (end, end + 1);
    }
}

fn is_hat(line: &str) -> bool {
    // Every comment counts, not only `///` and `//!`: a file's own head is a block like any other.
    line.starts_with("//") || line.starts_with("#[") || line.starts_with("#![")
}

fn bracket_delta(code: &str) -> i32 {
    let mut delta = 0;
    for ch in code.chars() {
        match ch {
            '[' => delta += 1,
            ']' => delta -= 1,
            _ => {}
        }
    }
    delta
}

/// A line that opens a multi-line block inside a long function, in three passes: a keyword, the
/// same keyword behind an expression prefix, then a macro call or a closure.
fn control_head(line: &str) -> bool {
    let line = line.trim();
    // A line may close the block above and open the next one: `} else if cond {`.
    let head = line.strip_prefix('}').map(str::trim_start).unwrap_or(line);

    if control_word(head) {
        return true;
    }
    if control_word(expression_tail(head)) {
        return true;
    }
    // A macro or a closure counts only when its `{` ends the line: `.map(|x| x + 1)` is one line,
    // and a folded `|` is not a block.
    line.ends_with('{') && (line.contains("!{") || line.contains("! {") || closure(line))
}

/// The words that open a block, whole: `elsewhere()` and `loop_count` are not two of them.
fn control_word(line: &str) -> bool {
    if line.starts_with("if ") || line.starts_with("match ") || line.starts_with("while ") {
        return true;
    }
    if line.starts_with("for ") {
        return true;
    }
    for word in ["else", "loop", "unsafe", "async"] {
        if let Some(rest) = line.strip_prefix(word)
            && (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('{'))
        {
            return true;
        }
    }
    false
}

/// The expression a block may open behind: after `let x =`, after an assignment, after a jump.
fn expression_tail(line: &str) -> &str {
    // `let Some(x) = foo() else {` opens its block at the `else`, not at the call.
    if line.starts_with("let ") && line.trim_end().ends_with("else {") {
        return "else {";
    }
    if let Some(rest) = line.strip_prefix("return ") {
        return rest.trim_start();
    }
    if let Some(rest) = line.strip_prefix("break ") {
        return rest.trim_start();
    }
    match assignment(line) {
        Some(right) => right.trim_start(),
        None => "",
    }
}

/// The right side of an assignment, and only an assignment: a comparison is not one.
fn assignment(line: &str) -> Option<&str> {
    let (left, right) = line.split_once('=')?;
    let left = left.trim_end();
    if right.starts_with('=') || left.ends_with(['=', '!', '<', '>']) {
        return None;
    }
    Some(right)
}

/// `spawn(move || {` and `.map(|x| {`: the line opens with a closure's brace.
fn closure(line: &str) -> bool {
    line.strip_suffix('{')
        .map(str::trim_end)
        .is_some_and(|head| head.ends_with('|'))
}

/// Every `{`-`}` pair, in the order the braces open.
pub(crate) fn frames(code: &[String]) -> Vec<(usize, usize)> {
    let mut stack: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => stack.push(i),
                '}' => {
                    if let Some(open) = stack.pop() {
                        out.push((open, i));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The text with comments and literals blanked: braces survive only where code opened them.
pub(crate) fn stripped(text: &str) -> Vec<String> {
    Lex::new(text).run()
}

struct Lex {
    chars: Vec<char>,
    i: usize,
    line: String,
    out: Vec<String>,
    ended: bool,
}

impl Lex {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            i: 0,
            line: String::new(),
            out: Vec::new(),
            ended: text.ends_with('\n'),
        }
    }

    fn at(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    fn space(&mut self) {
        if !self.line.ends_with(' ') {
            self.line.push(' ');
        }
    }

    fn newline(&mut self) {
        self.out.push(std::mem::take(&mut self.line));
        self.i += 1;
    }

    fn run(mut self) -> Vec<String> {
        while self.i < self.chars.len() {
            let c = self.chars[self.i];
            match c {
                '\n' => self.newline(),
                '/' if self.at(1) == Some('/') => {
                    self.i += 2;
                    self.line_comment();
                    self.space();
                }
                '/' if self.at(1) == Some('*') => {
                    self.i += 2;
                    self.block_comment();
                    self.space();
                }
                '"' => {
                    self.i += 1;
                    self.space();
                    self.string();
                }
                '\'' => self.quote(),
                'r' | 'b' => self.prefixed(c),
                _ => {
                    self.line.push(c);
                    self.i += 1;
                }
            }
        }
        if !self.ended && !self.chars.is_empty() {
            self.out.push(self.line);
        }
        self.out
    }

    fn line_comment(&mut self) {
        while self.i < self.chars.len() && self.chars[self.i] != '\n' {
            self.i += 1;
        }
    }

    fn block_comment(&mut self) {
        let mut depth = 1;
        while self.i < self.chars.len() {
            match (self.at(0), self.at(1)) {
                (Some('\n'), _) => self.newline(),
                (Some('/'), Some('*')) => {
                    depth += 1;
                    self.i += 2;
                }
                (Some('*'), Some('/')) => {
                    depth -= 1;
                    self.i += 2;
                    if depth == 0 {
                        return;
                    }
                }
                _ => self.i += 1,
            }
        }
    }

    fn string(&mut self) {
        while self.i < self.chars.len() {
            match self.at(0) {
                Some('\n') => self.newline(),
                Some('\\') => {
                    self.i += 1;
                    if self.at(0) == Some('\n') {
                        self.newline();
                    } else {
                        self.i += 1;
                    }
                }
                Some('"') => {
                    self.i += 1;
                    return;
                }
                _ => self.i += 1,
            }
        }
    }

    fn raw_string(&mut self, hashes: usize) {
        while self.i < self.chars.len() {
            match self.at(0) {
                Some('\n') => self.newline(),
                Some('"') => {
                    let mut k = 1;
                    while k <= hashes && self.at(k) == Some('#') {
                        k += 1;
                    }
                    if k == hashes + 1 {
                        self.i += k;
                        return;
                    }
                    self.i += 1;
                }
                _ => self.i += 1,
            }
        }
    }

    fn quote(&mut self) {
        // A char literal closes within a few characters; a lifetime never closes.
        if self.at(1) == Some('\\') {
            let mut j = self.i + 2;
            let mut hops = 0;
            while j < self.chars.len() && hops < 12 {
                if self.chars[j] == '\'' {
                    self.i = j + 1;
                    self.space();
                    return;
                }
                if self.chars[j] == '\n' {
                    break;
                }
                j += 1;
                hops += 1;
            }
        } else if self.at(2) == Some('\'') {
            self.i += 3;
            self.space();
            return;
        }
        self.line.push('\'');
        self.i += 1;
    }

    fn prefixed(&mut self, c: char) {
        if c == 'r' {
            let mut k = 1;
            while self.at(k) == Some('#') {
                k += 1;
            }
            if self.at(k) == Some('"') {
                let hashes = k - 1;
                self.i += k + 1;
                self.space();
                self.raw_string(hashes);
                return;
            }
        }
        if c == 'b' {
            match self.at(1) {
                Some('"') => {
                    self.i += 2;
                    self.space();
                    self.string();
                    return;
                }
                Some('\'') => {
                    self.i += 1;
                    self.quote();
                    return;
                }
                Some('r') => {
                    let mut k = 2;
                    while self.at(k) == Some('#') {
                        k += 1;
                    }
                    if self.at(k) == Some('"') {
                        let hashes = k - 2;
                        self.i += k + 1;
                        self.space();
                        self.raw_string(hashes);
                        return;
                    }
                }
                _ => {}
            }
        }
        self.line.push(c);
        self.i += 1;
    }
}
