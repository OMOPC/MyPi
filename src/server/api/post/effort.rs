//! `effort` — how hard this session's laps ask the model to think.

use anyhow::Result;
use serde_json::{Value, json};

use crate::server::agent::r#loop::Change;
use crate::server::agent::request::Effort;
use crate::server::api::route::i64_of;
use crate::server::api::{Api, Refuse};

use super::emit;

/// The one refusal a front end reads as "this model takes no effort".
pub(crate) const NO_EFFORT: &str = "this model takes no thinking effort";

pub(crate) fn apply(api: &Api, viewer: &str, ask: &Value) -> Result<Value, Refuse> {
    let session = i64_of(ask, "session")?;
    let raw = ask
        .get("effort")
        .and_then(Value::as_str)
        .ok_or_else(|| Refuse::Bad("missing or bad `effort`".into()))?;
    let effort = Effort::parse(raw).ok_or_else(|| Refuse::Bad(format!("no such effort: {raw}")))?;
    emit::require_writer(api, viewer, session)?;
    api.ensure_booked(session)?;
    // A host whose wire names no effort cannot be asked for one; a front end hears it here.
    let provider = api.ledger().provider(session)?;
    if !names_effort(api, &provider) {
        return Err(Refuse::Bad(NO_EFFORT.into()));
    }
    api.ledger().change(session, Change::Effort(effort))?;
    emit::changed(api, viewer, session, json!({"effort": effort.as_str()}));
    Ok(serde_json::json!({"effort": effort.as_str()}))
}

/// Whether this provider's `api` field names a wire that carries a thinking effort.
fn names_effort(api: &Api, provider: &str) -> bool {
    let Ok(value) = api
        .cfg()
        .get_json(&format!("models.provider.{provider}.api"), None)
    else {
        return false;
    };
    let Some(raw) = value.as_str() else {
        return false;
    };
    crate::server::agent::request::ProtocolId::parse(raw)
        .map(|protocol| protocol.build().supports_effort())
        .unwrap_or(false)
}
