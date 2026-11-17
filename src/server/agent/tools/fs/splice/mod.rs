// splice: replace whole lines by number, many ranges at once.

mod edit;
mod fmt;
mod receipt;
mod tool;

pub(crate) use tool::{TOOL, mass_text, parse_range};
