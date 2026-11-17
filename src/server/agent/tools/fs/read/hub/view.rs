//! The module view: what the entry exports, the modules it declares, and the importers above it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::edges::{deps, forwards};
use super::entries::entries;
use super::locate::{Module, declares_modules, hub_of, rel};
use super::syntax::{declarations, export_items, re_exports};
use super::text::{clip, cut, joined, plural, strip_attrs, strip_vis};
use super::up::{grouped, hit_line, radar, scan_line};

/// Past this many importers, the upstream list is grouped by directory.
const GROUP_AT: usize = 200;

/// The whole view: exports, the modules the entry declares, and who imports this module.
pub(crate) fn view(module: &Module, batch: &BTreeSet<PathBuf>, primer: bool) -> String {
    let title = if module.chain.is_empty() {
        "crate".to_string()
    } else {
        module.chain.join("::")
    };
    let tag = if primer { "primer" } else { "entry file" };
    let mut out = format!(
        "--- module {title} ({}/) · {tag} ---\n",
        rel(module, &module.dir)
    );
    let hub_text = module
        .hub
        .as_ref()
        .and_then(|hub| std::fs::read_to_string(hub).ok())
        .unwrap_or_default();

    let exports = re_exports(&hub_text);
    out.push_str(&format!(
        "exports ({}): {}\n",
        exports.len(),
        joined(&exports)
    ));

    let (children, by_name) = children(module, &hub_text);
    if children.is_empty() {
        out.push_str("down (0): none\n");
    } else {
        out.push_str(&format!("down ({}):\n", children.len()));
        for child in &children {
            out.push_str(&child_line(child, batch));
            out.push('\n');
        }
    }

    let forwards = forwards(module);
    let radar = radar(module, &forwards);
    let mut up: Vec<String> = Vec::new();
    if let Some(line) = parent_decl(module) {
        up.push(line);
    }
    if radar.hits.len() > GROUP_AT {
        up.extend(grouped(module, &radar.hits));
    } else {
        up.extend(radar.hits.iter().map(|hit| hit_line(module, hit)));
    }
    if up.is_empty() {
        out.push_str("up (0): none\n");
    } else {
        out.push_str(&format!("up ({}):\n", up.len()));
        for line in &up {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out.push_str(&scan_line(module, &radar.boundary, by_name));
    out.push('\n');
    cut(out)
}

struct Child {
    label: String,
    path: PathBuf,
    /// How big it is: a directory counts its files, a leaf its own lines.
    size: String,
    vis: String,
    hub: bool,
    exports: Vec<String>,
    deps: Vec<String>,
}

fn children(module: &Module, hub_text: &str) -> (Vec<Child>, bool) {
    let mut declared = declarations(hub_text);
    let by_name = declared.is_empty();
    if by_name {
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
                export_items(&path, &text)
            } else {
                re
            }
        } else {
            export_items(&path, &text)
        };
        let mut chain = module.chain.clone();
        chain.push(name.clone());
        let (dir_chain, place) = if dir {
            (chain.as_slice(), module.dir.join(&name))
        } else {
            (&chain[..chain.len() - 1], module.dir.clone())
        };
        let deps = deps(&text, &chain, dir_chain, &place);
        let size = if dir {
            let kids = entries(&module.dir.join(&name));
            plural(kids.iter().filter(|kid| !kid.2).count(), "file")
        } else {
            format!("{}L", text.lines().count())
        };
        out.push(Child {
            label: if dir {
                format!("{name}/")
            } else {
                format!("{name}.rs")
            },
            path,
            size,
            vis,
            hub,
            exports,
            deps,
        });
    }
    (out, by_name)
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
        "  {} {} {} {} exports ({}): {}",
        child.label,
        child.size,
        child.vis,
        if child.hub { "hub" } else { "leaf" },
        child.exports.len(),
        joined(&child.exports),
    );
    if batch.contains(&child.path) {
        line.push_str(" [body in this batch]");
    }
    if !child.deps.is_empty() {
        line.push_str(&format!(" deps: {}", child.deps.join(", ")));
    }
    clip(line)
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
