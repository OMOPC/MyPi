//! The model pill — `[M] [<effort>] <display name> (<provider>)`.

use ratatui::text::Span;

use super::super::Deco;
use super::super::theme::{Slot, StatusTheme};
use crate::ui::tui::zone::main::input::Ctx;

#[derive(Debug, Default)]
pub struct Model;

pub(super) fn make() -> Box<dyn Deco> {
    Box::new(Model)
}

impl Deco for Model {
    fn render(&self, t: &StatusTheme, _budget: Option<usize>, ctx: &Ctx) -> Vec<Span<'static>> {
        let Some(name) = ctx.data.model.as_deref() else {
            return Vec::new();
        };
        // `auto` names no rung: the pill stays as short as the host's own default.
        let rung = match ctx.data.effort {
            crate::ui::session::Effort::Auto => String::new(),
            effort => format!("[{}] ", effort.as_str()),
        };
        let text = match ctx.data.provider.as_deref() {
            Some(provider) => format!("[M] {rung}{name} ({provider})"),
            None => format!("[M] {rung}{name}"),
        };
        vec![t.capsule(text, t.get(Slot::Accent))]
    }
}
