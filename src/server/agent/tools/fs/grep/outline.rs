// The skeleton a folded answer draws: every hit's own chain of pieces opened level by level,
// the pieces beside each chain folded to one label row.

use std::collections::BTreeSet;

use super::scan;
use crate::server::agent::tools::contract::env::GrepLimits;
use crate::server::agent::tools::fs::lang::Table;
use crate::server::agent::tools::summary;

// Columns a line number takes beside its neighbour.
const MIN_WIDTH: usize = 3;

// One drawn source line's clip.
const CLIP: usize = 160;

// One file's skeleton: the trees its hits hang in, each drawn whole, one after another.
pub(crate) fn file(file: &scan::File, raw: &[&str], label: &str, caps: GrepLimits) -> String {
    let width = raw.len().to_string().len().max(MIN_WIDTH);
    let mut forest = Forest::default();
    for &line in file.hits.iter().take(caps.file) {
        forest.hang(&file.table, line);
    }
    let mut out = format!("{label}\n");
    let mut drawn = BTreeSet::new();
    for root in &forest.roots {
        paint(root, raw, &file.table, width, &mut out, &mut drawn);
        unwind(root, raw, width, &mut out, &mut drawn);
    }
    if file.hits.len() > caps.file {
        out.push_str(&format!(
            "  {label} [+{} more in this file]\n",
            file.hits.len() - caps.file
        ));
    }
    out
}

// One piece on a hit's chain: the lines it opens and closes on, the chain below it, its own hits.
struct Node {
    open: usize,
    close: usize,
    children: Vec<Node>,
    hits: Vec<usize>,
}

// One file's trees, one per topmost piece a hit hangs under, kept in line order.
#[derive(Default)]
struct Forest {
    roots: Vec<Node>,
}

// The node opening on `open`, or a fresh one inserted in line order.
fn slot(nodes: &mut Vec<Node>, open: usize, close: usize) -> &mut Node {
    match nodes.iter().position(|node| node.open == open) {
        Some(at) => &mut nodes[at],
        None => {
            let at = nodes.partition_point(|node| node.open < open);
            nodes.insert(
                at,
                Node {
                    open,
                    close,
                    children: Vec::new(),
                    hits: Vec::new(),
                },
            );
            &mut nodes[at]
        }
    }
}

impl Forest {
    // Threads one hit in: its whole chain of pieces, outermost first; a hit no piece holds
    // becomes its own one-row tree.
    fn hang(&mut self, table: &Table, line: usize) {
        let chain: Vec<(usize, usize)> = table
            .around(line)
            .into_iter()
            .map(|(at, piece)| (at, piece.mate.unwrap_or(at)))
            .collect();
        let Some((&(open, close), rest)) = chain.split_first() else {
            slot(&mut self.roots, line, line).hits.push(line);
            return;
        };
        slot(&mut self.roots, open, close).grow(rest, line);
    }
}

impl Node {
    fn grow(&mut self, chain: &[(usize, usize)], line: usize) {
        let Some(&(open, close)) = chain.first() else {
            self.hits.push(line);
            return;
        };
        slot(&mut self.children, open, close).grow(&chain[1..], line);
    }
}

// One tree's content rows: the openings and hits top-down, then the folds beside the chain.
fn paint(
    node: &Node,
    raw: &[&str],
    table: &Table,
    width: usize,
    out: &mut String,
    drawn: &mut BTreeSet<usize>,
) {
    row(node.open, raw, width, out, drawn);
    for &line in &node.hits {
        if line != node.open && line != node.close {
            row(line, raw, width, out, drawn);
        }
    }
    for child in &node.children {
        paint(child, raw, table, width, out, drawn);
    }
    folds(node, table, raw, width, out);
}

// One tree's closing rows, innermost first, after all its content rows are down.
fn unwind(node: &Node, raw: &[&str], width: usize, out: &mut String, drawn: &mut BTreeSet<usize>) {
    for child in &node.children {
        unwind(child, raw, width, out, drawn);
    }
    if node.close > node.open {
        row(node.close, raw, width, out, drawn);
    }
}

// One drawn line: its number, then the file's own text; a line is drawn once.
fn row(line: usize, raw: &[&str], width: usize, out: &mut String, drawn: &mut BTreeSet<usize>) {
    if drawn.insert(line) {
        out.push_str(&format!(
            "{:>width$} |{}\n",
            line + 1,
            summary::clip(raw[line], CLIP)
        ));
    }
}

// The pieces beside one node's chain, folded to one row per run of one label; a chain child or
// an unlabeled piece ends the run.
fn folds(node: &Node, table: &Table, raw: &[&str], width: usize, out: &mut String) {
    let chain: BTreeSet<usize> = node.children.iter().map(|child| child.open).collect();
    let end = table.end(node.open).unwrap_or(node.open);
    let mut run: Option<(usize, usize, &str)> = None;
    for at in table.items(node.open + 1, end) {
        let Some(line) = table.line(at) else { continue };
        if chain.contains(&at) {
            flush(run.take(), raw, width, out);
            continue;
        }
        match line.label.as_deref() {
            Some(label) => match run.take() {
                Some((start, _, same)) if same == label => {
                    run = Some((start, line.mate.unwrap_or(at), label));
                }
                past => {
                    flush(past, raw, width, out);
                    run = Some((at, line.mate.unwrap_or(at), label));
                }
            },
            None => flush(run.take(), raw, width, out),
        }
    }
    flush(run.take(), raw, width, out);
}

// One fold row: its range where its first sibling's own text starts, then label and ellipsis.
fn flush(run: Option<(usize, usize, &str)>, raw: &[&str], width: usize, out: &mut String) {
    if let Some((start, end, label)) = run {
        let pad = width + 2 + raw[start].len() - raw[start].trim_start().len();
        out.push_str(&format!(
            "{}{}::{} {label} …\n",
            " ".repeat(pad),
            start + 1,
            end + 1,
        ));
    }
}
