//! A turn the user stopped: the one sentence the tools and the front door share.

use std::path::Path;

use crate::server::agent::tools::contract::output::ToolOutput;

/// Why a call cut short by the user says so; the front door repeats it as the result's reason.
pub(crate) const INTERRUPTED: &str = "interrupted: the user stopped the turn";

/// What a tool hands back when the turn was stopped before it could touch anything.
pub(crate) fn stopped(path: &Path) -> ToolOutput {
    ToolOutput::text(format!(
        "nothing was written: {} is unchanged",
        path.display()
    ))
    .failure(INTERRUPTED)
}
