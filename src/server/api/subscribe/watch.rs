// `watch` — this connection follows one session, or nothing. Nothing is asked, nothing is answered.

use anyhow::Result;

use crate::server::api::{Api, Refuse};

pub(crate) fn apply(api: &Api, conn: u64, session: Option<i64>) -> Result<(), Refuse> {
    api.relay().follow(conn, session);
    Ok(())
}
