// `mailbox` — the front end's one pending message: written whole, or taken back.

use anyhow::Result;
use serde_json::{Value, json};

use crate::server::agent::r#loop::Letter;
use crate::server::api::post::emit;
use crate::server::api::route::{i64_of, opt_str_of};
use crate::server::api::{Api, Refuse};

pub(crate) fn apply(api: &Api, conn: u64, viewer: &str, ask: &Value) -> Result<Value, Refuse> {
    let session = i64_of(ask, "session")?;
    emit::require_writer(api, viewer, session)?;
    let text = opt_str_of(ask, "text")?.unwrap_or_default();
    let prefix = opt_str_of(ask, "prefix")?.map(str::to_string);
    let suffix = opt_str_of(ask, "suffix")?.map(str::to_string);
    // A letter may be all context: `/prepare` with no words of its own.
    if text.trim().is_empty() && prefix.is_none() && suffix.is_none() {
        api.ledger().clear_mail(session)?;
        api.relay().mail(conn, session, None);
        return Ok(json!({}));
    }
    let letter = Letter {
        prefix,
        text: text.to_string(),
        suffix,
    };
    api.ensure_booked(session)?;
    api.ledger().write_mail(session, letter)?;
    api.relay().mail(conn, session, Some(viewer));
    Ok(json!({}))
}
