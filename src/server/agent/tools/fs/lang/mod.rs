// The language sorters: one file per language, picked by suffix, each answering one line table.

use std::path::Path;

mod blank;
mod rust;
mod table;

pub(crate) use rust::{binds, shape};
pub(crate) use table::{Line, Table};

/// One file's line table: the sorter its suffix names, or every line as its own piece.
pub(crate) fn table(path: &Path, text: &str) -> Table {
    let suffix = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if rust::SUFFIXES.contains(&suffix) {
        return rust::table(text);
    }
    Table::plain(text)
}
