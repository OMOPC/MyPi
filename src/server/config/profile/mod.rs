//! Profile discovery and resolution. A profile is a directory under `profiles/`.

mod discover;
mod resolve;
mod types;

pub use discover::{list, locate};
pub use resolve::resolve;
pub use types::{BUILTINS, Builtin, DEFAULT_PROFILE, ResolvedProfile, builtin};
