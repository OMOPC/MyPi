//! The built-in prompts: one folder per profile compiled into the binary.

/// A text file ends with a newline the original text did not have; drop it on the way out.
fn text(raw: &'static str) -> &'static str {
    raw.strip_suffix('\n').unwrap_or(raw)
}

/// The `default` system prompt.
pub fn system() -> &'static str {
    text(include_str!("DEFAULT/SYSTEM.md"))
}

/// The `default` compaction instruction.
pub fn compaction() -> &'static str {
    text(include_str!("DEFAULT/COMPACTION.md"))
}

/// The `ENGINEER` system prompt.
pub fn engineer() -> &'static str {
    text(include_str!("ENGINEER/SYSTEM.md"))
}

/// The `ENGINEER` compaction instruction.
pub fn engineer_compaction() -> &'static str {
    text(include_str!("ENGINEER/COMPACTION.md"))
}
