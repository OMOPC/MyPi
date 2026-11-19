// `close` — a session is gone: its buffer, its turn, and its watchers go with it.

use crate::server::api::Api;

pub(crate) fn apply(api: &Api, session: i64) {
    api.relay().close(session);
}
