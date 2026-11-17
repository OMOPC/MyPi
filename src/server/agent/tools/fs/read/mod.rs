//! read: files by line numbers, directories by name.

mod dir;
mod hub;
mod sniff;
pub(crate) mod sparse;
mod tool;

pub(crate) use tool::TOOL;
