//! The module graph: what a module exports, who sits beside it, and who imports it.
//! A text scan over a crate's `src/`: no compiler, no LSP; the walk stops at the crate root.

mod edges;
mod entries;
mod locate;
mod syntax;
mod text;
mod up;
mod view;

pub(crate) use entries::entries_all;
pub(crate) use locate::{canonical, declares_modules, locate, src_of};
pub(crate) use text::plural;
pub(crate) use view::view;
