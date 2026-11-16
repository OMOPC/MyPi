//! What one command reads, and what it answers.

use crate::ui::tui::zone::Request;
use crate::ui::tui::zone::main::desk::Hire;
use crate::ui::tui::zone::main::input::{ExitRequest, Notice};

/// One command's input.
pub struct Call<'a> {
    /// The text after the command name, trimmed.
    pub args: &'a str,
    /// The session this front end follows; `None` = a draft.
    pub session: Option<i64>,

    /// Where the conversation stands.
    pub cwd: &'a str,

    /// Whether the model this front end shows reaches a wire that carries an effort.
    pub takes_effort: bool,
}

/// What one command answers.
pub enum Done {
    /// One write, and what to say once it lands.
    Write { request: Request, notice: Notice },
    /// One message, carrying the context the command built.
    Submit {
        prefix: Option<String>,
        text: String,
    },
    /// Something only this front end does.
    Exit(ExitRequest),
    /// One temp hand.
    Hire(Hire),
    /// The profile a session will open with.
    Profile(Option<String>),
    /// A refusal: one line for the history zone, and the box is cleared.
    Notice(Notice),
    /// An argument is missing or unusable: the line stays in the box.
    Hold,
}
