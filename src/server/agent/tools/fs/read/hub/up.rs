//! The upward scan: who imports this module, one whole level at a time, under a deadline.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::edges::{Forward, names_module, trace, written};
use super::entries::entries;
use super::locate::{Module, chain_of, rel};
use super::syntax::uses;
use super::text::clip;

/// How long the upward walk may spend past its first level. The first level always finishes.
const BUDGET: Duration = Duration::from_millis(100);

pub(super) struct Hit {
    file: PathBuf,
    line: usize,
    symbols: Vec<String>,
    /// The entry file whose `pub use` made this consumer visible under another name.
    via: Option<PathBuf>,
}

pub(super) struct Radar {
    pub(super) hits: Vec<Hit>,
    pub(super) boundary: Boundary,
}

pub(super) enum Boundary {
    CrateRoot,
    Stopped(PathBuf),
    /// The module is the crate root: there is nothing above it to scan.
    NoAbove,
}

/// One line for both directions: how far up the walk went, and how deep the listing went.
pub(super) fn scan_line(module: &Module, boundary: &Boundary, by_name: bool) -> String {
    let up = match boundary {
        Boundary::CrateRoot => "crate root (complete)".to_string(),
        Boundary::Stopped(dir) => format!("{} (budget)", rel(module, dir)),
        Boundary::NoAbove => "none (this is the crate root)".to_string(),
    };
    let down = if by_name {
        "this directory (by file name)"
    } else {
        "this directory (nothing deeper)"
    };
    format!("[scan: up→{up} | down→{down}]")
}

pub(super) fn radar(module: &Module, forwards: &[Forward]) -> Radar {
    let mut hits: Vec<Hit> = Vec::new();
    if module.chain.is_empty() {
        return Radar {
            hits,
            boundary: Boundary::NoAbove,
        };
    }
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(module.file.clone());
    if let Some(hub) = &module.hub {
        seen.insert(hub.clone());
    }
    // Inside a module's own directory is not "outside": those edges are the directory graph.
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
            if !walk(
                root, &skip_dir, &mut seen, module, forwards, &mut found, deadline, first,
            ) {
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
    forwards: &[Forward],
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
            if let Some(hit) = hit(&path, module, forwards) {
                hits.push(hit);
            }
        }
    }
    true
}

fn hit(path: &Path, module: &Module, forwards: &[Forward]) -> Option<Hit> {
    let text = std::fs::read_to_string(path).ok()?;
    let chain = chain_of(path, &module.src)?;
    let dir = path.parent()?;
    let mut symbols: Vec<String> = Vec::new();
    let mut line = 0;
    let mut via: Option<PathBuf> = None;
    for (at, paths, public) in uses(&text) {
        // A `pub use` is a forwarding edge, not a consumer: the index already holds it.
        if public {
            continue;
        }
        let mut found = false;
        for path in paths {
            let Some(full) = written(&path, &chain, dir) else {
                continue;
            };
            let (resolved, through) = if names_module(&full, module) {
                (full, None)
            } else {
                match trace(&full, forwards) {
                    Some((resolved, from)) => (resolved, Some(from)),
                    None => continue,
                }
            };
            if !names_module(&resolved, module) {
                continue;
            }
            found = true;
            let symbol = if resolved.len() == module.chain.len() {
                module.chain.last().cloned().unwrap_or_default()
            } else {
                resolved.last().cloned().unwrap_or_default()
            };
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
            if through.is_some() {
                via = through;
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
        via,
    })
}

pub(super) fn hit_line(module: &Module, hit: &Hit) -> String {
    let mut line = format!(
        "{}:{} imports {}",
        rel(module, &hit.file),
        hit.line,
        hit.symbols.join(", ")
    );
    if let Some(via) = &hit.via {
        line.push_str(&format!(" via {}", rel(module, via)));
    }
    clip(line)
}

pub(super) fn grouped(module: &Module, hits: &[Hit]) -> Vec<String> {
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
