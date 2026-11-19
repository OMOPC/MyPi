// `effort` — how hard this session's laps ask the model to think.

use anyhow::Result;
use serde_json::{Value, json};

use crate::server::agent::r#loop::Change;
use crate::server::agent::request::Effort;
use crate::server::api::route::i64_of;
use crate::server::api::{Api, Refuse};

use super::emit;

// The one refusal a front end reads as "this model takes no effort".
pub(crate) const NO_EFFORT: &str = "this model takes no thinking effort";

pub(crate) fn apply(api: &Api, viewer: &str, ask: &Value) -> Result<Value, Refuse> {
    let session = i64_of(ask, "session")?;
    let raw = ask
        .get("effort")
        .and_then(Value::as_str)
        .ok_or_else(|| Refuse::Bad("missing or bad `effort`".into()))?;
    let effort = Effort::parse(raw).ok_or_else(|| Refuse::Bad(format!("no such effort: {raw}")))?;
    emit::prepare(api, viewer, session)?;
    // The rungs a model takes are its own declaration; a front end hears the refusal here.
    let declared = declared(api, session)?;
    if declared.is_empty() {
        return Err(Refuse::Bad(NO_EFFORT.into()));
    }
    if effort != Effort::Auto && !declared.contains(&effort) {
        let rungs = declared
            .iter()
            .map(|rung| rung.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Refuse::Bad(format!(
            "this model takes no `{}`; it declares {rungs}",
            effort.as_str()
        )));
    }
    api.ledger().change(session, Change::Effort(effort))?;
    Ok(json!({"effort": effort.as_str()}))
}

// The rungs this session's model declares; empty takes none.
fn declared(api: &Api, session: i64) -> Result<Vec<Effort>, Refuse> {
    let (provider, model) = api.ledger().model(session)?;
    Ok(api
        .cfg()
        .model_by_id(&format!("{provider}:{model}"))
        .map(|resolved| resolved.entry.efforts)
        .unwrap_or_default())
}
