//! `/effort` — how hard this session asks the model to think.

use crate::ui::command::{Arg, CommandMeta};
use crate::ui::lang::Text;
use crate::ui::session::Effort;
use crate::ui::tui::zone::Request;

use super::super::Notice;
use super::{Call, Done};

pub const META: CommandMeta = CommandMeta {
    name: "/effort",
    aliases: &[],
    args: &[Arg::Effort],
    detail: Text {
        en: "how hard to think: auto, low, high, max (this session only, never stored)",
        zh: "思考强度：auto、low、high、max（只在本会话内，不落库）",
    },
};

pub fn run(call: &Call<'_>) -> Done {
    if call.session.is_none() {
        return Done::Notice(Notice::NoSession);
    }
    let Some(effort) = Effort::parse(call.args) else {
        return Done::Hold;
    };
    Done::Write {
        request: Request::SetEffort(effort),
        notice: Notice::Effort(effort),
    }
}
