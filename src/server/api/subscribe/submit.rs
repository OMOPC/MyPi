// `submit` — open a turn and take the session's writer slot.

use anyhow::Result;
use serde_json::{Value, json};

use crate::server::agent::r#loop::Message;
use crate::server::api::route::{i64_of, opt_str_of, str_of};
use crate::server::api::{Api, Refuse};
use crate::server::storage::Reader;

pub(crate) fn apply(api: &Api, conn: u64, viewer: &str, ask: &Value) -> Result<Value, Refuse> {
    let session = i64_of(ask, "session")?;
    let message = Message::Submit {
        prefix: opt_str_of(ask, "prefix")?.map(str::to_string),
        text: str_of(ask, "text")?.to_string(),
        suffix: opt_str_of(ask, "suffix")?.map(str::to_string),
    };
    open(api, conn, viewer, session, message, "submit")
}

// Shared by `submit` and `compact`: one turn, one writer.
pub(crate) fn open(
    api: &Api,
    conn: u64,
    viewer: &str,
    session: i64,
    message: Message,
    door: &'static str,
) -> Result<Value, Refuse> {
    if api.relay().busy(session) {
        return Err(Refuse::Busy("the session is already in a turn".into()));
    }
    let reader = Reader::open(api.db())?;
    let row = reader
        .session(session)?
        .ok_or_else(|| Refuse::Bad(format!("no session {session}")))?;
    drop(reader);
    if row["anchor"].is_i64() {
        return Err(Refuse::Permission(
            "a child session takes no messages".into(),
        ));
    }
    // A compaction opens with no words of the user's: nobody else has a block to draw for it.
    let opening = match &message {
        Message::Submit { text, .. } => text.clone(),
        Message::Compact { .. } => String::new(),
    };
    api.ensure_booked(session)?;
    let turn = api.ledger().deliver(session, message)?;
    api.relay()
        .open(conn, session, viewer, turn, door, &opening);
    Ok(json!({"turn": turn}))
}
