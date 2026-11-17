//! Tools. The root declares modules; everything is reached by its own path.

pub mod contract;
pub(crate) mod engine;
mod fs;
mod kairos;
pub(crate) mod mcp;
pub(crate) mod memo;
pub(crate) mod registry;
pub(crate) mod search;
mod shell;
pub(crate) mod spec;
pub(crate) mod subagent;
mod summary;
pub(crate) mod workspace;

pub(crate) mod web;
