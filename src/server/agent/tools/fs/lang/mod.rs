// The sorter registry: a suffix names a language, and a language answers one line table.

use std::path::{Path, PathBuf};

use crate::server::platform::stop::Stop;

mod guages;
mod table;
pub(crate) mod verdict;

pub(crate) use table::{Line, Table};
use verdict::{Check, Outcome};

/// One text's turn through the formatter a language names.
pub(crate) type Format = fn(&str, &Path, &Stop) -> Outcome;

/// What every language owes the tools: the suffixes it answers to, its table, the answers about a
/// piece, and the two external tools it knows how to drive.
pub(crate) struct Language {
    pub(crate) suffixes: &'static [&'static str],
    pub(crate) table: fn(&str) -> Table,
    pub(crate) shape: fn(&Line) -> Option<&str>,
    /// Whether a piece can say what it binds; `None` when the language names nothing.
    pub(crate) binds: Option<fn(&Line, &str) -> bool>,
    /// Whether its files are source: they take the source budget and are read whole.
    pub(crate) source: bool,
    pub(crate) format: Option<Format>,
    pub(crate) check: Option<Checker>,
}

/// How one language's checker finds a build unit and asks it for its diagnostics.
pub(crate) struct Checker {
    /// The unit a file belongs to; `None` when no unit holds it.
    pub(crate) unit: fn(&Path) -> Option<PathBuf>,
    /// What a report calls that unit.
    pub(crate) title: fn(&Path) -> String,
    /// How that unit names one of its files, so a diagnostic can be matched to a write.
    pub(crate) shown: fn(&Path, &Path) -> String,
    /// Check one unit, given the files this batch wrote into it.
    pub(crate) run: fn(&Path, &[PathBuf], &Stop) -> Check,
}

/// Every sorted language, in the order a suffix meets them.
static LANGUAGES: &[&Language] = &[&guages::rust::LANGUAGE, &guages::markdown::LANGUAGE];

/// The language that claims this path, by its suffix.
pub(crate) fn owner(path: &Path) -> Option<&'static Language> {
    let suffix = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    LANGUAGES
        .iter()
        .copied()
        .find(|one| one.suffixes.contains(&suffix))
}

/// One file's line table: the sorter its suffix names, or every line as its own piece.
pub(crate) fn table(path: &Path, text: &str) -> Table {
    match owner(path) {
        Some(one) => (one.table)(text),
        None => Table::plain(text),
    }
}

/// Whether this path is source, which decides how much of it a read may show.
pub(crate) fn source(path: &Path) -> bool {
    owner(path).is_some_and(|one| one.source)
}

/// The formatter this path's language names, if it names one.
pub(crate) fn format(path: &Path) -> Option<Format> {
    owner(path)?.format
}

/// The checker this path's language names, if it names one.
pub(crate) fn checker(path: &Path) -> Option<&'static Checker> {
    owner(path)?.check.as_ref()
}

/// The kind of piece this line opens, in its own language's word: `fn`, `impl`, `heading`, `fence`.
pub(crate) fn shape<'a>(table: &Table, line: &'a Line) -> Option<&'a str> {
    (table.language()?.shape)(line)
}

/// Whether this piece binds `word`; a language whose pieces name nothing answers no.
pub(crate) fn binds(table: &Table, line: &Line, word: &str) -> bool {
    table
        .language()
        .and_then(|one| one.binds)
        .is_some_and(|binds| binds(line, word))
}

/// Whether any piece of this file can be asked what it binds: only then is a name worth searching.
pub(crate) fn named(table: &Table) -> bool {
    table.language().is_some_and(|one| one.binds.is_some())
}
