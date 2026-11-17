//! The edges: what one file reaches inside its own directory, and the names `pub use` forwards.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::locate::{Module, chain_of, hub_of};
use super::syntax::{expand_named, uses};
use super::text::{strip_attrs, strip_vis};

/// Which modules of this file's own directory it reaches into, and for which symbols.
///
/// `chain` is the file's own module, `dir_chain` the directory that holds it, and `place` that
/// directory: only names living there count, and the file's own module never does.
pub(super) fn deps(
    text: &str,
    chain: &[String],
    dir_chain: &[String],
    place: &Path,
) -> Vec<String> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, paths, _) in uses(text) {
        for path in paths {
            let Some(full) = absolute(&path, chain) else {
                continue;
            };
            if full.len() <= dir_chain.len() || full[..dir_chain.len()] != dir_chain[..] {
                continue;
            }
            if full.starts_with(chain) {
                continue;
            }
            let rest = &full[dir_chain.len()..];
            let sibling = rest[0].clone();
            if !place.join(format!("{sibling}.rs")).is_file() && !place.join(&sibling).is_dir() {
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

/// One forwarded name: where a consumer writes it, where it really lives, and who forwards it.
pub(super) struct Forward {
    /// The path a consumer writes, from the crate root: `a::read::TOOL`.
    at: Vec<String>,
    /// The path it stands for: `a::read::tool::TOOL`.
    to: Vec<String>,
    /// The entry file holding the `pub use`.
    from: PathBuf,
}

/// Every `pub use` on the way from this module up to the crate: each is a name a consumer may
/// write instead of the real one.
pub(super) fn forwards(module: &Module) -> Vec<Forward> {
    let mut out = Vec::new();
    let mut place = Some(module.dir.clone());
    while let Some(dir) = place {
        if !dir.starts_with(&module.src) {
            break;
        }
        if let Some(hub) = hub_of(&dir, &module.src)
            && let Some(chain) = chain_of(&hub, &module.src)
            && let Ok(text) = std::fs::read_to_string(&hub)
        {
            for (path, name) in pub_uses(&text) {
                let Some(to) = written(&path, &chain, &dir) else {
                    continue;
                };
                let mut at = chain.clone();
                at.push(name);
                out.push(Forward {
                    at,
                    to,
                    from: hub.clone(),
                });
            }
        }
        if dir == module.src {
            break;
        }
        place = dir.parent().map(Path::to_path_buf);
    }
    out
}

/// A file's `pub use` statements: the target path and the name it is visible under there.
fn pub_uses(text: &str) -> Vec<(Vec<String>, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = strip_attrs(lines[i].trim_start());
        if !raw.starts_with("pub") || !strip_vis(raw).starts_with("use ") {
            i += 1;
            continue;
        }
        let mut buf = strip_vis(raw).trim_start().to_string();
        while !buf.contains(';') && i + 1 < lines.len() {
            i += 1;
            buf.push(' ');
            buf.push_str(lines[i].trim());
        }
        out.extend(expand_named(buf.split(';').next().unwrap_or("")));
        i += 1;
    }
    out
}

/// A consumer's path as it really points, through any forwarding on the way.
pub(super) fn trace(path: &[String], forwards: &[Forward]) -> Option<(Vec<String>, PathBuf)> {
    let mut current = path.to_vec();
    let mut via: Option<PathBuf> = None;
    for _ in 0..8 {
        let mut found: Option<&Forward> = None;
        for forward in forwards {
            if current.len() < forward.at.len() || current[..forward.at.len()] != forward.at[..] {
                continue;
            }
            if found.is_none_or(|old| old.at.len() < forward.at.len()) {
                found = Some(forward);
            }
        }
        let Some(forward) = found else {
            break;
        };
        let mut next = forward.to.clone();
        next.extend(current[forward.at.len()..].iter().cloned());
        if next == current {
            break;
        }
        current = next;
        via = Some(forward.from.clone());
    }
    Some((current, via?))
}

/// Does this path name the module, or something inside it?
pub(super) fn names_module(path: &[String], module: &Module) -> bool {
    path.len() >= module.chain.len() && path[..module.chain.len()] == module.chain[..]
}

/// A use path as written inside `dir`: `crate`/`self`/`super`, or a module sitting beside it.
pub(super) fn written(path: &[String], chain: &[String], dir: &Path) -> Option<Vec<String>> {
    if let Some(absolute) = absolute(path, chain) {
        return Some(absolute);
    }
    let first = path.first()?;
    let beside = dir.join(format!("{first}.rs")).is_file() || dir.join(first).is_dir();
    beside.then(|| {
        let mut base = chain.to_vec();
        base.extend(path.iter().cloned());
        base
    })
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
