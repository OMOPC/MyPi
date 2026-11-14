//! Profile types.

use super::super::prompts;

pub const DEFAULT_PROFILE: &str = "default";

/// A profile compiled into the binary: what a machine without a directory of that name runs. A
/// directory of the same name, in any root, is the user's and wins over this.
pub struct Builtin {
    pub name: &'static str,
    /// The prompt, trimmed; a function pointer because a `static` table cannot call one.
    pub system: fn() -> &'static str,
    /// The compaction instruction, trimmed the same way.
    pub compact: fn() -> &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: DEFAULT_PROFILE,
        system: prompts::system,
        compact: prompts::compaction,
    },
    Builtin {
        name: "ENGINEER",
        system: prompts::engineer,
        compact: prompts::engineer_compaction,
    },
];

/// The built-in profile with this name, if the name is one.
pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProfile {
    pub system: String,
    pub tools: Option<Vec<String>>,
    pub compact_instruction: String,
}
