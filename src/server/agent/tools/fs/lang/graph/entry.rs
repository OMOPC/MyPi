// The graph's doors: the unit a file belongs to, its neighbourhood, and the rows a view reads.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{Around, Loader, Unit};
use super::{index, view};
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
