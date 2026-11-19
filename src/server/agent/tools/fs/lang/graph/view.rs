// The default narration: the machine's own arrangement of the parts, for a language that has none.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::parts;
use super::{Around, Edge, Loader};
use crate::server::agent::tools::fs::lang::Row;

// Fold this many importers by directory; below it, print them one by one.
const GROUP_AT: usize = 200;

pub(crate) fn default_view(
    around: &Around,
    _loader: &Loader,
    batch: &BTreeSet<PathBuf>,
) -> Vec<Row> {
    let base = &around.base;
    let mut rows: Vec<Row> = Vec::new();
    rows.push(Row {
        heading: Some(around.unit.name.clone()),
        text: format!("  {}", parts::rel(base, around.unit.place.as_path())),
    });
    if !around.interface.is_empty() {
        rows.push(Row {
            heading: Some(parts::heading("interface", around.interface.len())),
            text: String::new(),
        });
        for item in &around.interface {
            rows.push(Row {
                heading: None,
                text: parts::item_line(item),
            });
        }
    }
    if !around.children.is_empty() {
        rows.push(Row {
            heading: Some(parts::heading("children", around.children.len())),
            text: String::new(),
        });
        for child in &around.children {
            let line = format!(
                "{}  {}  {}",
                parts::node_line(&child.unit.name, child.size),
                child.tags.join(" "),
                parts::plural(child.interface.len(), "export")
            );
            rows.push(Row {
                heading: None,
                text: parts::clip(format!(
                    "{}{}",
                    line,
                    if batch.contains(&child.unit.place) {
                        "  [body in this batch]"
                    } else {
                        ""
                    }
                )),
            });
        }
    }
    if let Some(decl) = &around.declared_by {
        rows.push(Row {
            heading: Some("declared by".to_string()),
            text: format!(
                "  {}:{}",
                parts::rel(base, decl.unit.entry.as_deref().unwrap_or(&decl.unit.place)),
                decl.line.map_or(0, |line| line + 1)
            ),
        });
    }
    section(base, "out", &around.outgoing, &mut rows);
    if around.incoming.len() > GROUP_AT {
        rows.push(Row {
            heading: Some(parts::heading("up", around.incoming.len())),
            text: String::new(),
        });
        for line in parts::grouped(base, &around.incoming) {
            rows.push(Row {
                heading: None,
                text: format!("  {line}"),
            });
        }
    } else {
        section(base, "up", &around.incoming, &mut rows);
    }
    rows.push(Row {
        heading: None,
        text: parts::boundary_line(base, &around.boundary),
    });
    rows
}

fn section(base: &std::path::Path, label: &str, edges: &[Edge], rows: &mut Vec<Row>) {
    if edges.is_empty() {
        rows.push(Row {
            heading: Some(format!("{label} (0)")),
            text: "  none".to_string(),
        });
        return;
    }
    rows.push(Row {
        heading: Some(parts::heading(label, edges.len())),
        text: String::new(),
    });
    for edge in edges {
        rows.push(Row {
            heading: None,
            text: parts::edge_line(base, edge),
        });
    }
}
