// The skeleton a folded answer draws: every hit's own chain of pieces opened level by level,
// the pieces beside each chain folded to one label row, every row in the file's own order.

use std::collections::BTreeSet;

use super::scan;
use crate::server::agent::tools::fs::lang::Table;
use crate::server::agent::tools::summary;

// Columns a line number takes beside its neighbour.
const MIN_WIDTH: usize = 3;

// One drawn source line's clip.
const CLIP: usize = 160;

pub(crate) fn file(file: &scan::File, raw: &[&str], label: &str, want: &[usize]) -> String {
    let width = raw.len().to_string().len().max(MIN_WIDTH);
    let mut forest = Forest::default();
    for &line in want {
        forest.hang(&file.table, line);
    }
    let mut pool: Vec<(usize, String)> = Vec::new();
    for root in &forest.roots {
        paint(root, raw, &file.table, width, &mut pool);
    }
    pool.sort_by_key(|(line, _)| *line);
    pool.dedup_by(|a, b| a.0 == b.0);
    let mut out = format!("{label}\n");
    for (_, text) in &pool {
        out.push_str(text);
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

// One tree's rows into the pool: its opening and closing lines, its own hits, the children's
// trees, and the folds beside its chain; the pool's sort puts every row where the file has it.
fn paint(node: &Node, raw: &[&str], table: &Table, width: usize, pool: &mut Vec<(usize, String)>) {
    pool.push((node.open, line_text(node.open, raw, width)));
    for &line in &node.hits {
        if line != node.open && line != node.close {
            pool.push((line, line_text(line, raw, width)));
        }
    }
    for child in &node.children {
        paint(child, raw, table, width, pool);
    }
    folds(node, table, raw, width, pool);
    if node.close > node.open {
        pool.push((node.close, line_text(node.close, raw, width)));
    }
}

// One drawn source line: its number, then the file's own text.
fn line_text(line: usize, raw: &[&str], width: usize) -> String {
    format!("{:>width$} |{}\n", line + 1, summary::clip(raw[line], CLIP))
}

// The pieces beside one node's chain, folded to one row per run of one label; a chain child or
// an unlabeled piece ends the run.
fn folds(node: &Node, table: &Table, raw: &[&str], width: usize, pool: &mut Vec<(usize, String)>) {
    let chain: BTreeSet<usize> = node.children.iter().map(|child| child.open).collect();
    let end = table.end(node.open).unwrap_or(node.open);
    let mut run: Option<(usize, usize, &str)> = None;
    for at in table.items(node.open + 1, end) {
        let Some(line) = table.line(at) else { continue };
        if chain.contains(&at) {
            flush(run.take(), raw, width, pool);
            continue;
        }
        match line.label.as_deref() {
            Some(label) => match run.take() {
                Some((start, _, same)) if same == label => {
                    run = Some((start, line.mate.unwrap_or(at), label));
                }
                past => {
                    flush(past, raw, width, pool);
                    run = Some((at, line.mate.unwrap_or(at), label));
                }
            },
            None => flush(run.take(), raw, width, pool),
        }
    }
    flush(run.take(), raw, width, pool);
}

// One fold row, sorted by the line its range starts on: its range where its first sibling's own
// text starts, then label and ellipsis.
fn flush(
    run: Option<(usize, usize, &str)>,
    raw: &[&str],
    width: usize,
    pool: &mut Vec<(usize, String)>,
) {
    if let Some((start, end, label)) = run {
        let pad = width + 2 + raw[start].len() - raw[start].trim_start().len();
        pool.push((
            start,
            format!("{}{}::{} {label} …\n", " ".repeat(pad), start + 1, end + 1,),
        ));
    }
}
