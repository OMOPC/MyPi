// shell: the command, then its output.

use super::super::super::row::{Lead, Row, Run};
use super::frame::wrap_rows;
use super::{ToolBlock, ToolRenderer, partial};
use crate::ui::tui::theme::fixed;

pub(crate) struct Shell;

impl ToolRenderer for Shell {
    fn call_body(&self, b: &ToolBlock<'_>) -> Vec<Row> {
        wrap_rows(shell_call(b.args), b.inner_width())
    }
}

fn shell_call(args: &str) -> Vec<Row> {
    let cmd = partial::text(args, "command").unwrap_or_default();
    if cmd.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, line) in cmd.lines().enumerate() {
        let mut runs = Vec::new();
        if i == 0 {
            runs.push(Run::fixed("$ ", fixed::DIM));
        }
        runs.push(Run::plain(line));
        out.push(Row::of(Lead::None, runs));
    }
    out
}
