// The index: language facts turned into one unit's neighbourhood. No language is named here.

use std::path::{Path, PathBuf};

use super::walk;
use super::{Around, Boundary, Child, Decl, Edge, Loader, RawEdge, Size, Unit};
use crate::server::agent::tools::fs::lang::Hub;

// One file's unit when the language does not say better.
fn file_unit(file: &Path) -> Unit {
    Unit {
        name: file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        place: file.to_path_buf(),
        entry: Some(file.to_path_buf()),
    }
}

fn resolve_unit(facts: &Hub, file: &Path) -> Unit {
    match facts.unit_of {
        Some(unit_of) => unit_of(file).unwrap_or_else(|| file_unit(file)),
        None => file_unit(file),
    }
}

// Whether `place` sits at or under `root`, path-component by path-component.
fn within(place: &Path, root: &Path) -> bool {
    place == root
        || place
            .strip_prefix(root)
            .is_ok_and(|rest| !rest.as_os_str().is_empty())
}

// The files whose text speaks for a unit: its entry, its file, or every file under its directory.
pub(crate) fn unit_texts(unit: &Unit, loader: &Loader) -> Vec<(PathBuf, String)> {
    if let Some(entry) = &unit.entry {
        return loader
            .read(entry)
            .map(|text| vec![(entry.clone(), text)])
            .unwrap_or_default();
    }
    if unit.place.is_file() {
        return loader
            .read(&unit.place)
            .map(|text| vec![(unit.place.clone(), text)])
            .unwrap_or_default();
    }
    loader
        .entries(&unit.place)
        .into_iter()
        .filter(|entry| !entry.dir)
        .filter_map(|entry| {
            let text = loader.read(&entry.path)?;
            Some((entry.path, text))
        })
        .collect()
}

fn unit_interface(facts: &Hub, unit: &Unit, loader: &Loader) -> Vec<String> {
    let Some(interface) = facts.interface else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for (path, text) in unit_texts(unit, loader) {
        for item in interface(&path, &text) {
            if !out.contains(&item) {
                out.push(item);
            }
        }
    }
    out
}

fn size_of(unit: &Unit, loader: &Loader) -> Size {
    if unit.place.is_dir() {
        let files = loader
            .entries(&unit.place)
            .into_iter()
            .filter(|entry| !entry.dir)
            .count();
        return Size::Files(files);
    }
    loader
        .read(&unit.place)
        .map(|text| Size::Lines(text.lines().count()))
        .unwrap_or(Size::Unknown)
}

fn has_children(facts: &Hub, unit: &Unit, loader: &Loader) -> bool {
    match facts.territory {
        Some(territory) => !territory(unit, loader).is_empty(),
        None => false,
    }
}

// Resolve one raw edge to the unit and the file it lands on.
fn land(facts: &Hub, raw: &RawEdge) -> Option<(Unit, std::path::PathBuf)> {
    let file = raw.targets.iter().find(|path| path.is_file())?;
    Some((resolve_unit(facts, file), file.clone()))
}

fn same_target(a: &Edge, b: &Edge) -> bool {
    match (&a.to_file, &b.to_file) {
        (Some(one), Some(other)) => one == other,
        (None, None) => a.wrote == b.wrote,
        _ => false,
    }
}

// Append an edge, merging symbols into an earlier one with the same target.
fn merge(out: &mut Vec<Edge>, edge: Edge) {
    if let Some(old) = out.iter_mut().find(|one| same_target(one, &edge)) {
        for symbol in edge.symbols {
            if !old.symbols.contains(&symbol) {
                old.symbols.push(symbol);
            }
        }
        if old.via.is_empty() {
            old.via = edge.via;
        }
        return;
    }
    out.push(edge);
}

fn edges_of(facts: &Hub, unit: &Unit, loader: &Loader) -> Vec<Edge> {
    let mut out: Vec<Edge> = Vec::new();
    for (file, text) in unit_texts(unit, loader) {
        for raw in (facts.edges)(&file, &text, loader) {
            let landed = land(facts, &raw);
            merge(
                &mut out,
                Edge {
                    at: file.clone(),
                    to: landed.as_ref().map(|(unit, _)| unit.clone()),
                    to_file: landed.map(|(_, file)| file),
                    wrote: raw.wrote,
                    line: raw.line,
                    symbols: raw.symbols,
                    re_export: raw.re_export,
                    via: raw.via,
                },
            );
        }
    }
    out
}

fn children(facts: &Hub, loader: &Loader, unit: &Unit) -> Vec<Child> {
    let decls = match facts.territory {
        Some(territory) => territory(unit, loader),
        None => loader
            .entries(&unit.place)
            .into_iter()
            .filter(|entry| entry.dir)
            .map(|entry| Decl {
                unit: Unit {
                    name: entry.name.clone(),
                    place: entry.path,
                    entry: None,
                },
                line: None,
                from: None,
                tags: Vec::new(),
            })
            .collect(),
    };
    let mut out = Vec::new();
    for decl in decls {
        let interface = unit_interface(facts, &decl.unit, loader);
        // The child's own text is not a reference to itself.
        let own = decl
            .unit
            .entry
            .clone()
            .unwrap_or_else(|| decl.unit.place.clone());
        let outgoing = edges_of(facts, &decl.unit, loader)
            .into_iter()
            .filter(|edge| {
                edge.to_file
                    .as_ref()
                    .is_some_and(|file| file != &own && within(file, &unit.place))
            })
            .collect();
        out.push(Child {
            size: size_of(&decl.unit, loader),
            has_children: has_children(facts, &decl.unit, loader),
            tags: decl.tags,
            interface,
            outgoing,
            unit: decl.unit,
        });
    }
    out
}

fn declared_by(facts: &Hub, loader: &Loader, unit: &Unit) -> Option<Decl> {
    let territory = facts.territory?;
    let parent = facts.unit_of?(unit.place.parent()?)?;
    territory(&parent, loader)
        .into_iter()
        .find(|decl| decl.unit.entry == unit.entry || decl.unit.place == unit.place)
}

// Keep the raw edges that point at the unit or into it, normalized to the unit itself.
fn incoming(facts: &Hub, unit: &Unit, found: Vec<(PathBuf, Vec<RawEdge>)>) -> Vec<Edge> {
    let mut out: Vec<Edge> = Vec::new();
    for (at, raws) in found {
        for raw in raws {
            if raw.re_export {
                continue;
            }
            let Some((to, _file)) = land(facts, &raw) else {
                continue;
            };
            if !within(&to.place, &unit.place) {
                continue;
            }
            if let Some(old) = out.iter_mut().find(|edge| edge.at == at) {
                for symbol in raw.symbols {
                    if !old.symbols.contains(&symbol) {
                        old.symbols.push(symbol);
                    }
                }
                if !raw.via.is_empty() {
                    old.via = raw.via;
                }
                continue;
            }
            out.push(Edge {
                at: at.clone(),
                to: Some(unit.clone()),
                to_file: None,
                wrote: raw.wrote,
                line: raw.line,
                symbols: raw.symbols,
                re_export: false,
                via: raw.via,
            });
        }
    }
    out
}

// Build one unit's neighbourhood from a language's facts.
pub(crate) fn build(facts: &Hub, loader: &Loader, file: &Path) -> Option<Around> {
    let unit = resolve_unit(facts, file);
    let domain = facts.domain.and_then(|domain| domain(file));
    let base = domain
        .as_ref()
        .map(|domain| domain.root.clone())
        .unwrap_or_else(|| unit.place.clone());
    let outgoing = edges_of(facts, &unit, loader);
    let (incoming, boundary) = match &domain {
        Some(domain) => {
            let (found, mut boundary) = walk::scan(facts, loader, &unit, domain);
            boundary.no_above = match (facts.unit_of, unit.place.parent()) {
                (Some(unit_of), Some(parent)) => unit_of(parent).is_none(),
                _ => false,
            };
            (incoming(facts, &unit, found), boundary)
        }
        None => (
            Vec::new(),
            Boundary {
                levels: 0,
                files: 0,
                stop: None,
                complete: false,
                no_above: true,
            },
        ),
    };
    Some(Around {
        interface: unit_interface(facts, &unit, loader),
        children: children(facts, loader, &unit),
        declared_by: declared_by(facts, loader, &unit),
        outgoing,
        incoming,
        boundary,
        base,
        file: file.to_path_buf(),
        unit,
    })
}
