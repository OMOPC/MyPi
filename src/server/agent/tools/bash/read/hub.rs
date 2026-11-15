//! The module graph: what a module exports, who sits beside it, and who imports it.
//! A text scan over a crate's `src/`: no compiler, no LSP; the walk stops at the crate root.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::server::agent::tools::bash::change;
use crate::server::agent::tools::workspace::canonicalize_lenient;

/// How long the upward walk may spend past its first level. The first level always finishes.
const BUDGET: Duration = Duration::from_millis(100);

/// A view longer than these is cut; the whole crate is 456 files today, so nothing real hits it.
const VIEW_LINES: usize = 800;
const VIEW_BYTES: usize = 256 * 1024;

/// Past this many importers, the upstream list is grouped by directory.
const GROUP_AT: usize = 200;

/// One module, as the file the model just read sees it.
pub(crate) struct Module {
    /// The module the file belongs to: `web::cdp` for `web/cdp/client.rs`, the crate for `lib.rs`.
    pub chain: Vec<String>,
    /// Where the module's own files live.
    pub dir: PathBuf,
    /// The module's entry file: `mod.rs`, the dual-track `x.rs`, or `lib.rs`/`main.rs`.
    pub hub: Option<PathBuf>,
    /// The crate root, the directory holding `Cargo.toml`.
    pub root: PathBuf,
    /// The crate's `src/`.
    pub src: PathBuf,
    /// The file that was read.
    pub file: PathBuf,
}

/// The module `file` belongs to, when it sits under a crate's `src/`.
pub(crate) fn locate(file: &Path) -> Option<Module> {
    if file.extension().and_then(|e| e.to_str()) != Some("rs") {
        return None;
    }
    let file = canonicalize_lenient(file);
    let root = crate_root(&file)?;
    let src = root.join("src");
    let rel = file.strip_prefix(&src).ok()?;
    let stem = rel.file_stem()?.to_string_lossy().into_owned();
    let parent = rel.parent().unwrap_or_else(|| Path::new(""));
    let mut chain: Vec<String> = parent
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let mut dir = src.join(parent);
    let root_hub = chain.is_empty() && (stem == "lib" || stem == "main");
    if stem != "mod" && !root_hub {
        // A dual-track hub: `x.rs` beside an `x/` directory owns the directory's files.
        let child = dir.join(&stem);
        if child.is_dir() {
            chain.push(stem.clone());
            dir = child;
        }
    }
    let hub = hub_of(&dir, &src);
    Some(Module {
        chain,
        dir,
        hub,
        root,
        src,
        file,
    })
}

/// The crate's `src/` when `dir` sits inside one.
pub(crate) fn src_of(dir: &Path) -> Option<PathBuf> {
    for root in dir.ancestors() {
        if root.join("Cargo.toml").is_file() {
            let src = root.join("src");
            return dir.starts_with(&src).then_some(src);
        }
    }
    None
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
    canonicalize_lenient(path)
}

/// One directory's entries: names, paths, and which are directories. Junk and dot entries are
/// dropped, directories come first, and the order is case-insensitive.
pub(crate) fn entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, bool)> = read
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if junk(&name) {
                return None;
            }
            let is_dir = entry.file_type().ok()?.is_dir();
            Some((name, entry.path(), is_dir))
        })
        .collect();
    out.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    out
}

fn junk(name: &str) -> bool {
    matches!(name, "target" | "node_modules") || name.starts_with('.')
}

/// Does this text declare a module that lives in its own file? An inline `mod x { … }` and a
/// `#[cfg]`-hidden declaration both stay out: the first has no file, the second no answer.
pub(crate) fn declares_modules(text: &str) -> bool {
    text.lines().any(|line| {
        let line = strip_vis(strip_attrs(line.trim_start()));
        line.strip_prefix("mod ").is_some_and(|rest| {
            let rest = rest.trim_end();
            rest.ends_with(';') && !rest.contains('{')
        })
    })
}

/// The whole view: exports, submodules, and who imports this module.
pub(crate) fn view(module: &Module, batch: &BTreeSet<PathBuf>) -> String {
    let title = if module.chain.is_empty() {
        "crate".to_string()
    } else {
        module.chain.join("::")
    };
    let mut out = format!("--- module {title} ({}/) ---\n", rel(module, &module.dir));
    let hub_text = module
        .hub
        .as_ref()
        .and_then(|hub| std::fs::read_to_string(hub).ok())
        .unwrap_or_default();

    let exports = re_exports(&hub_text);
    out.push_str("exports: ");
    out.push_str(&if exports.is_empty() {
        "none".to_string()
    } else {
        exports.join("; ")
    });
    out.push('\n');

    let children = children(module, &hub_text);
    if children.is_empty() {
        out.push_str("submodules: none\n");
    } else {
        out.push_str("submodules:\n");
        for child in &children {
            out.push_str(&child_line(child, batch));
            out.push('\n');
        }
    }

    out.push_str("upstream:\n");
    let mut any = false;
    if let Some(line) = parent_decl(module) {
        out.push_str(&format!("  {line}\n"));
        any = true;
    }
    let radar = radar(module);
    let lines = if radar.hits.len() > GROUP_AT {
        grouped(module, &radar.hits)
    } else {
        radar.hits.iter().map(|hit| hit_line(module, hit)).collect()
    };
    for line in &lines {
        out.push_str(&format!("  {line}\n"));
        any = true;
    }
    if !any {
        out.push_str("  none\n");
    }
    out.push_str(&radar.boundary.line(module));
    out.push('\n');
    cut(out)
}

// ---- the module's own files ----

struct Child {
    label: String,
    path: PathBuf,
    /// The directory a sub-module owns, for the label and the role.
    lines: usize,
    vis: String,
    hub: bool,
    exports: Vec<String>,
    deps: Vec<String>,
}

fn children(module: &Module, hub_text: &str) -> Vec<Child> {
    let mut declared = declarations(hub_text);
    if declared.is_empty() {
        declared = entries(&module.dir)
            .into_iter()
            .filter_map(|(name, _, is_dir)| {
                let stem = name.strip_suffix(".rs")?;
                (!is_dir && stem != "mod").then(|| (stem.to_string(), "?".to_string()))
            })
            .collect();
    }
    let mut out = Vec::new();
    for (name, vis) in declared {
        let Some((path, dir)) = resolve_child(&module.dir, &name) else {
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let hub = dir || declares_modules(&text);
        let exports = if hub {
            let re = re_exports(&text);
            if re.is_empty() {
                export_items(&text)
            } else {
                re
            }
        } else {
            export_items(&text)
        };
        let mut chain = module.chain.clone();
        chain.push(name.clone());
        let deps = deps(&text, &chain, module);
        out.push(Child {
            label: if dir {
                format!("{name}/")
            } else {
                format!("{name}.rs")
            },
            path,
            lines: text.lines().count(),
            vis,
            hub,
            exports,
            deps,
        });
    }
    out
}

fn resolve_child(dir: &Path, name: &str) -> Option<(PathBuf, bool)> {
    let sub = dir.join(name);
    let module = sub.join("mod.rs");
    if module.is_file() {
        return Some((module, true));
    }
    let file = dir.join(format!("{name}.rs"));
    file.is_file().then_some((file, false))
}

fn child_line(child: &Child, batch: &BTreeSet<PathBuf>) -> String {
    let mut line = format!(
        "  {} ({}L, {}, {})",
        child.label,
        child.lines,
        child.vis,
        if child.hub { "sub-hub" } else { "leaf" }
    );
    if batch.contains(&child.path) {
        line.push_str(" [body in this batch]");
    } else if !child.exports.is_empty() {
        line.push_str(&format!(" exports: {}", child.exports.join("; ")));
    }
    if !child.deps.is_empty() {
        line.push_str(&format!("; depends on: {}", child.deps.join(", ")));
    }
    clip(line)
}

/// `pub mod x;` lines, in the order the hub writes them.
fn declarations(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let raw = strip_attrs(line.trim_start());
        let vis = if raw.starts_with("pub") {
            let rest = strip_vis(raw);
            if rest.len() < raw.len() {
                let end = raw.find(')').map(|i| i + 1).unwrap_or_else(|| {
                    raw.find(' ').unwrap_or(raw.len())
                });
                raw[..end].trim().to_string()
            } else {
                "pub".to_string()
            }
        } else {
            "private".to_string()
        };
        let rest = strip_vis(raw);
        let Some(rest) = rest.strip_prefix("mod ") else {
            continue;
        };
        let rest = rest.trim_end();
        if !rest.ends_with(';') || rest.contains('{') {
            continue;
        }
        let name = rest.trim_end_matches(';').trim();
        if name.is_empty() || name.contains(' ') {
            continue;
        }
        out.push((name.to_string(), vis));
    }
    out
}

/// A file's `pub` items, signature only: no bodies, no private items.
fn export_items(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        if !raw.starts_with("pub") {
            i += 1;
            continue;
        }
        let rest = strip_vis(raw);
        let Some(head) = change::head_of(rest) else {
            i += 1;
            continue;
        };
        if matches!(head, "use" | "impl" | "mod") {
            i += 1;
            continue;
        }
        let mut buf = raw.to_string();
        let mut j = i;
        while !buf.contains('{') && !buf.contains(';') && j + 1 < lines.len() && j - i < 6 {
            j += 1;
            buf.push(' ');
            buf.push_str(lines[j].trim());
        }
        let cut = buf.find(|c| c == '{' || c == ';').unwrap_or(buf.len());
        out.push(clip(buf[..cut].trim().to_string()));
        i = j + 1;
    }
    out
}

/// The hub's `pub use` statements, each on one line.
fn re_exports(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        if !raw.starts_with("pub") || !strip_vis(raw).starts_with("use ") {
            i += 1;
            continue;
        }
        let mut buf = raw.to_string();
        while !buf.contains(';') && i + 1 < lines.len() {
            i += 1;
            buf.push(' ');
            buf.push_str(lines[i].trim());
        }
        let statement = buf.split(';').next().unwrap_or("").trim().to_string();
        out.push(clip(statement));
        i += 1;
    }
    out
}

// ---- the sibling edges ----

/// Which siblings this file reaches into, and for which symbols.
fn deps(text: &str, chain: &[String], module: &Module) -> Vec<String> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, paths) in uses(text) {
        for path in paths {
            let Some(full) = absolute(&path, chain) else {
                continue;
            };
            if full.len() <= module.chain.len() || full[..module.chain.len()] != module.chain[..] {
                continue;
            }
            let rest = &full[module.chain.len()..];
            let sibling = rest[0].clone();
            let sibling_dir = module.dir.join(&sibling);
            if !module.dir.join(format!("{sibling}.rs")).is_file() && !sibling_dir.is_dir() {
                continue;
            }
            let symbol = if rest.len() == 1 {
                sibling.clone()
            } else {
                rest.last().cloned().unwrap_or_default()
            };
            let list = map.entry(sibling).or_default();
            if !list.contains(&symbol) {
                list.push(symbol);
            }
        }
    }
    map.into_iter()
        .map(|(name, symbols)| {
            if symbols.len() == 1 && symbols[0] == name {
                name
            } else {
                format!("{} ({})", name, symbols.join(", "))
            }
        })
        .collect()
}

/// Every `use` statement in a file: the line it starts on and the full paths it names.
fn uses(text: &str) -> Vec<(usize, Vec<Vec<String>>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        let stripped = strip_vis(raw);
        let Some(rest) = stripped.strip_prefix("use ") else {
            i += 1;
            continue;
        };
        let start = i;
        let mut buf = rest.trim().to_string();
        while !buf.contains(';') && i + 1 < lines.len() {
            i += 1;
            buf.push(' ');
            buf.push_str(lines[i].trim());
        }
        let body = buf.split(';').next().unwrap_or("");
        out.push((start + 1, expand(body)));
        i += 1;
    }
    out
}

/// One use tree → the full paths it names. `crate::a::{B, c::D}` → `crate::a::B`, `crate::a::c::D`.
fn expand(tree: &str) -> Vec<Vec<String>> {
    let tree = tree.trim();
    if tree.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    match split_group(tree) {
        Some((head, inner)) => {
            for part in split_top(inner) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let mut prefix: Vec<String> = segments(head);
                match split_group(part) {
                    Some((nested, inner)) => {
                        prefix.extend(segments(nested));
                        out.extend(expand(&format!("{}::{{{inner}}}", prefix.join("::"))));
                    }
                    None => {
                        prefix.extend(segments(part));
                        out.push(prefix);
                    }
                }
            }
        }
        None => out.push(segments(tree)),
    }
    out
}

/// A path's segments, minus an `as` alias.
fn segments(text: &str) -> Vec<String> {
    let name = text.split(" as ").next().unwrap_or(text).trim();
    name.split("::")
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// The first `{…}` group in a tree, and what stands before it.
fn split_group(tree: &str) -> Option<(&str, &str)> {
    let open = tree.find('{')?;
    let mut depth = 0;
    let mut close = None;
    for (at, ch) in tree.char_indices().skip(open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(at);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let head = tree[..open].trim_end_matches("::").trim();
    Some((head, &tree[open + 1..close]))
}

/// Split a group's body on the commas that are not inside a nested group.
fn split_top(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (at, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// A use path → the path from the crate root, when it names one.
fn absolute(path: &[String], chain: &[String]) -> Option<Vec<String>> {
    let (mut base, rest) = match path.first().map(String::as_str)? {
        "crate" => (Vec::new(), &path[1..]),
        "self" => (chain.to_vec(), &path[1..]),
        "super" => {
            let mut up = 0;
            while path.get(up).map(String::as_str) == Some("super") {
                up += 1;
            }
            if up > chain.len() {
                return None;
            }
            (chain[..chain.len() - up].to_vec(), &path[up..])
        }
        _ => return None,
    };
    base.extend(rest.iter().filter(|part| part.as_str() != "self").cloned());
    Some(base)
}

// ---- the upward scan ----

struct Hit {
    file: PathBuf,
    line: usize,
    symbols: Vec<String>,
}

struct Radar {
    hits: Vec<Hit>,
    boundary: Boundary,
}

enum Boundary {
    CrateRoot,
    Stopped(PathBuf),
}

impl Boundary {
    fn line(&self, module: &Module) -> String {
        match self {
            Boundary::CrateRoot => "[scan: crate root]".to_string(),
            Boundary::Stopped(dir) => format!("[scan: stopped at {}]", rel(module, dir)),
        }
    }
}

fn radar(module: &Module) -> Radar {
    let mut hits: Vec<Hit> = Vec::new();
    if module.chain.is_empty() {
        return Radar {
            hits,
            boundary: Boundary::CrateRoot,
        };
    }
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(module.file.clone());
    if let Some(hub) = &module.hub {
        seen.insert(hub.clone());
    }
    // Inside a module's own directory is not "outside": those edges are the sibling graph.
    let skip_dir = (module.dir != module.src).then(|| module.dir.clone());
    let deadline = Instant::now() + BUDGET;
    let mut boundary = Boundary::Stopped(module.dir.clone());
    let mut level = 1;
    loop {
        let Some(base) = module.dir.ancestors().nth(level) else {
            break;
        };
        if !base.starts_with(&module.root) {
            break;
        }
        let mut roots = vec![base.to_path_buf()];
        if let Some(up) = base.parent() {
            if up.starts_with(&module.root) {
                for (_, path, _) in entries(up) {
                    if path != base {
                        roots.push(path);
                    }
                }
            }
        }
        let first = level == 1;
        let mut found: Vec<Hit> = Vec::new();
        let mut complete = true;
        for root in &roots {
            if !walk(root, &skip_dir, &mut seen, module, &mut found, deadline, first) {
                complete = false;
                break;
            }
        }
        if !complete {
            break;
        }
        hits.extend(found);
        // One level covers its base and the base's peers, so the walk has reached the parent.
        let reached = match base.parent() {
            Some(up) if up.starts_with(&module.root) => up,
            _ => base,
        };
        boundary = if reached == module.root {
            Boundary::CrateRoot
        } else {
            Boundary::Stopped(reached.to_path_buf())
        };
        if base == module.root {
            break;
        }
        level += 1;
        if Instant::now() >= deadline {
            break;
        }
    }
    Radar { hits, boundary }
}

/// One level's tree. A level past the first that runs out of time is dropped whole: half a level
/// reads as "nobody else imports this", which is a lie.
fn walk(
    root: &Path,
    skip_dir: &Option<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    module: &Module,
    hits: &mut Vec<Hit>,
    deadline: Instant,
    first: bool,
) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if !first && Instant::now() >= deadline {
            return false;
        }
        if skip_dir.as_ref().is_some_and(|skip| dir == *skip) {
            continue;
        }
        for (_, path, is_dir) in entries(&dir) {
            if is_dir {
                if skip_dir.as_ref().is_some_and(|skip| path == *skip) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            if let Some(hit) = hit(&path, module) {
                hits.push(hit);
            }
        }
    }
    true
}

fn hit(path: &Path, module: &Module) -> Option<Hit> {
    let text = std::fs::read_to_string(path).ok()?;
    let chain = chain_of(path, &module.src)?;
    let mut symbols: Vec<String> = Vec::new();
    let mut line = 0;
    for (at, paths) in uses(&text) {
        let mut found = false;
        for path in paths {
            let Some(full) = absolute(&path, &chain) else {
                continue;
            };
            if full.len() < module.chain.len() || full[..module.chain.len()] != module.chain[..] {
                continue;
            }
            found = true;
            let symbol = if full.len() == module.chain.len() {
                module.chain.last().cloned().unwrap_or_default()
            } else {
                full.last().cloned().unwrap_or_default()
            };
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
        }
        if found && line == 0 {
            line = at;
        }
    }
    (line > 0).then(|| Hit {
        file: path.to_path_buf(),
        line,
        symbols,
    })
}

fn chain_of(file: &Path, src: &Path) -> Option<Vec<String>> {
    let rel = file.strip_prefix(src).ok()?;
    let stem = rel.file_stem()?.to_string_lossy().into_owned();
    let mut chain: Vec<String> = rel
        .parent()
        .map(|parent| {
            parent
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if stem != "mod" && !(chain.is_empty() && (stem == "lib" || stem == "main")) {
        chain.push(stem);
    }
    Some(chain)
}

fn hit_line(module: &Module, hit: &Hit) -> String {
    clip(format!(
        "{}:{} imports {}",
        rel(module, &hit.file),
        hit.line,
        hit.symbols.join(", ")
    ))
}

fn grouped(module: &Module, hits: &[Hit]) -> Vec<String> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for hit in hits {
        let full = rel(module, &hit.file);
        let (dir, name) = match full.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), full.clone()),
        };
        map.entry(dir)
            .or_default()
            .push(format!("{name}:{}", hit.line));
    }
    map.into_iter()
        .map(|(dir, files)| {
            let count = files.len();
            clip(format!("{dir} ({count} files): {}", files.join(", ")))
        })
        .collect()
}

/// The parent module's declaration of this module, with its line number.
fn parent_decl(module: &Module) -> Option<String> {
    let name = module.chain.last()?.clone();
    let mut parent = module.src.clone();
    for part in &module.chain[..module.chain.len() - 1] {
        parent.push(part);
    }
    let hub = hub_of(&parent, &module.src)?;
    let text = std::fs::read_to_string(&hub).ok()?;
    let tail = format!("mod {name};");
    for (at, line) in text.lines().enumerate() {
        let raw = strip_attrs(line.trim_start());
        if !strip_vis(raw).starts_with(&tail) {
            continue;
        }
        return Some(clip(format!(
            "{}:{} declares {}",
            rel(module, &hub),
            at + 1,
            raw.trim()
        )));
    }
    None
}

// ---- helpers ----

fn crate_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

fn hub_of(dir: &Path, src: &Path) -> Option<PathBuf> {
    if dir == src {
        let lib = src.join("lib.rs");
        if lib.is_file() {
            return Some(lib);
        }
        let main = src.join("main.rs");
        return main.is_file().then_some(main);
    }
    let module = dir.join("mod.rs");
    if module.is_file() {
        return Some(module);
    }
    let sibling = dir.with_extension("rs");
    sibling.is_file().then_some(sibling)
}

/// The words before an item, dropped: `pub(crate) ` and consecutive `#[…]` hats.
fn strip_vis(line: &str) -> &str {
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

fn strip_attrs(mut line: &str) -> &str {
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

fn rel(module: &Module, path: &Path) -> String {
    let path = path.strip_prefix(&module.root).unwrap_or(path);
    let text = path.to_string_lossy().replace('\\', "/");
    if text.is_empty() { ".".to_string() } else { text }
}

fn clip(text: String) -> String {
    if text.chars().count() <= 400 {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(400)
        .map(|(at, _)| at)
        .unwrap_or(text.len());
    format!("{}…", &text[..cut])
}

fn cut(text: String) -> String {
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
    out.push_str(&format!("[truncated: {} lines]\n", lines.saturating_sub(kept)));
    out
}
