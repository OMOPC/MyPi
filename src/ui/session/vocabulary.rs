//! The words the front end and the api both use: a model address, a compaction budget, an effort.

/// A model address: the provider and the id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelId {
    pub provider: String,
    pub model: String,
}

/// How much of the history a compaction keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// Characters of the tail.
    Chars(u64),

    /// Newest blocks.
    Blocks(u64),
}

/// How hard the session asks the model to think. Memory-only: the server never stores it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Effort {
    /// Ask for nothing and take the host's default.
    #[default]
    Auto,
    Low,
    High,
    Max,
}

impl Effort {
    /// The ladder in the order Ctrl+Tab cycles through it.
    pub const LADDER: [Self; 4] = [Self::Auto, Self::Low, Self::High, Self::Max];

    /// The one spelling the server takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase();
        Self::LADDER
            .into_iter()
            .find(|effort| effort.as_str() == raw.as_str())
    }

    /// The next rung, wrapping back to `Auto`.
    pub fn next(self) -> Self {
        let at = Self::LADDER
            .iter()
            .position(|rung| *rung == self)
            .unwrap_or(0);
        Self::LADDER[(at + 1) % Self::LADDER.len()]
    }
}
