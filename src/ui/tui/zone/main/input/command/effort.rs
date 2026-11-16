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
        en: "how hard to think: auto, or a rung the model declares (this session only, never stored)",
        zh: "思考强度：auto，或该模型声明的档位（只在本会话内，不落库）",
    },
};

pub fn run(call: &Call<'_>) -> Done {
    if call.session.is_none() {
        return Done::Notice(Notice::NoSession);
    }
    // A model that declares no rung cannot be asked for one, and a rung it never declared is
    // refused here, before the request reaches a host that would answer with a 400.
    if call.efforts.is_empty() {
        return Done::Notice(Notice::NoEffort);
    }
    let Some(effort) = Effort::parse(call.args) else {
        return Done::Hold;
    };
    if effort != Effort::Auto && !call.efforts.contains(&effort) {
        return Done::Notice(Notice::NotDeclared(effort));
    }
    Done::Write {
        request: Request::SetEffort(effort),
        notice: Notice::Effort(effort),
    }
}
