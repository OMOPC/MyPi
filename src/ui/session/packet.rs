// The normalized unit: one frame's three parts, forwarded unchanged.

use serde_json::Value;

// One frame as it crosses the session: `kind`, `name`, `content`. The session reads `content` only for the blocks it answers and the writes it keeps its windows by.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub kind: String,
    pub name: String,
    pub content: Value,
}

impl Packet {
    pub fn new(kind: impl Into<String>, name: impl Into<String>, content: Value) -> Packet {
        Packet {
            kind: kind.into(),
            name: name.into(),
            content,
        }
    }

    // A fact the session itself produced.
    pub fn local(name: impl Into<String>, content: Value) -> Packet {
        Packet::new("local", name, content)
    }
}
