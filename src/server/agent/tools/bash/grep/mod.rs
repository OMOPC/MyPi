//! grep: a word's hits, with the Rust structure around each one.

mod outline;
mod render;
mod rust;
mod scan;
mod tool;

pub(crate) use tool::TOOL;
