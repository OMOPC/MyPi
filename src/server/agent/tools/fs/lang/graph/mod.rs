// The module graph machine: what one unit's neighbourhood looks like, and how it is printed. It knows no language: a `Hub` supplies every fact, this layer only aggregates.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) mod index;
pub(crate) mod loader;
pub(crate) mod parts;
pub(crate) mod view;
pub(crate) mod walk;

pub(crate) use loader::Loader;

use crate::server::agent::tools::fs::lang::{Hub, Row};

// The unit a file belongs to, without building anything else near it.
pub(crate) fn unit(facts: &Hub, file: &Path) -> Option<Unit> {
    (facts.unit_of?)(file)
}

// One unit's neighbourhood.
pub(crate) fn build(facts: &Hub, loader: &Loader, file: &Path) -> Around {
    index::build(facts, loader, file)
}

// One neighbourhood as rows: the language's narration, or the machine's default.
pub(crate) fn render(
    facts: &Hub,
    around: &Around,
    loader: &Loader,
    batch: &BTreeSet<PathBuf>,
) -> Vec<Row> {
    match facts.view {
        Some(view) => view(around, loader, batch),
        None => view::default_view(around, loader, batch),
    }
}

// One unit of a language: a file, a directory, or a named thing with no entry file at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    // The name the view prints; `a::b` for Rust, the note's stem for Markdown.
    pub(crate) name: String,
    // What identifies it: a canonical path.
    pub(crate) place: PathBuf,
    // The file that carries its text, when it has one.
    pub(crate) entry: Option<PathBuf>,
}

// One unit a parent declares, and how that declaration reads.
#[derive(Clone, Debug)]
pub(crate) struct Decl {
    pub(crate) unit: Unit,
    // The line the parent writes it on, if the parent has a line to point at.
    pub(crate) line: Option<usize>,
    // The file that line lives in.
    pub(crate) from: Option<PathBuf>,
    // The language's own words; the machine prints them and never interprets them.
    pub(crate) tags: Vec<String>,
}

// One reference as a language reads it: where it is written, what it names, where it lands.
#[derive(Clone, Debug)]
pub(crate) struct RawEdge {
    // The 0-based line the reference starts on. A view prints it 1-based.
    pub(crate) line: usize,
    // The reference as written, for a target that lands outside this tree.
    pub(crate) wrote: String,
    // The names this edge carries; empty when only the relation is known.
    pub(crate) symbols: Vec<String>,
    // Candidate landing files, in order; empty when it lands outside the tree.
    pub(crate) targets: Vec<PathBuf>,
    // The forwarding entry files passed on the way, in order.
    pub(crate) via: Vec<PathBuf>,
    // This edge is itself a forwarding declaration, not a consumer.
    pub(crate) re_export: bool,
}

// Where a unit may find who points at it: the tree's root, and the language's own source directory under it.
#[derive(Clone, Debug)]
pub(crate) struct Domain {
    pub(crate) root: PathBuf,
    pub(crate) tree: PathBuf,
}

// How big a child is. The machine counts; the language does not say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Size {
    Lines(usize),
    Files(usize),
    // Nothing was readable.
    Unknown,
}

// One resolved reference, from a unit to a unit or to nothing.
#[derive(Clone, Debug)]
pub(crate) struct Edge {
    // The file this line is written in; a unit may hold several.
    pub(crate) at: PathBuf,
    // The unit it lands on; `None` when it lands outside the tree.
    pub(crate) to: Option<Unit>,
    // The file it lands on, one step finer than the unit.
    pub(crate) to_file: Option<PathBuf>,
    pub(crate) wrote: String,
    pub(crate) line: usize,
    pub(crate) symbols: Vec<String>,
    pub(crate) re_export: bool,
    pub(crate) via: Vec<PathBuf>,
}

// One unit under the one being viewed.
#[derive(Clone, Debug)]
pub(crate) struct Child {
    pub(crate) unit: Unit,
    pub(crate) size: Size,
    pub(crate) has_children: bool,
    pub(crate) tags: Vec<String>,
    pub(crate) interface: Vec<String>,
    // Its references that land inside the viewed unit's territory.
    pub(crate) outgoing: Vec<Edge>,
}

// Where the upward walk stopped, and how much it saw.
#[derive(Clone, Debug)]
pub(crate) struct Boundary {
    pub(crate) levels: usize,
    pub(crate) files: usize,
    // The directory the walk stopped at, when it stopped before the domain root.
    pub(crate) stop: Option<PathBuf>,
    // Whether the walk reached the domain root.
    pub(crate) complete: bool,
    // Whether the unit is itself the domain root: there is nothing above it.
    pub(crate) no_above: bool,
}

// Everything the machine knows about one unit's neighbourhood.
#[derive(Clone, Debug)]
pub(crate) struct Around {
    pub(crate) unit: Unit,
    // The file the model just read: the neighbourhood is that file's.
    pub(crate) file: PathBuf,
    // The base every printed path is relative to.
    pub(crate) base: PathBuf,
    pub(crate) interface: Vec<String>,
    pub(crate) children: Vec<Child>,
    pub(crate) declared_by: Option<Decl>,
    pub(crate) outgoing: Vec<Edge>,
    pub(crate) incoming: Vec<Edge>,
    pub(crate) boundary: Boundary,
}
