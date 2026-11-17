// The Rust language: its sorter, its own scan, its formatter and its checker.

use crate::server::agent::tools::fs::lang::Language;

mod check;
mod format;
mod scan;
mod table;

pub(crate) static LANGUAGE: Language = Language {
    suffixes: table::SUFFIXES,
    table: table::table,
    shape: table::shape,
    binds: Some(table::binds),
    source: true,
    format: Some(format::format),
    check: Some(check::CHECKER),
};
