// The module graph machine: what one unit's neighbourhood looks like, and how it is printed. It knows no language: a `Hub` supplies every fact, this layer only aggregates.

mod entry;
pub(crate) mod index;
pub(crate) mod loader;
pub(crate) mod parts;
mod types;
pub(crate) mod view;
pub(crate) mod walk;

pub(crate) use entry::{build, render, unit};
pub(crate) use loader::Loader;
pub(crate) use types::{Around, Boundary, Child, Decl, Domain, Edge, RawEdge, Size, Unit};
