// What a language's formatter and checker answer: the neutral shapes every tool reads.

/// What a formatter said about a text it could not parse.
#[derive(Debug, Clone)]
pub(crate) struct Unparsed {
    /// The line it pointed at, 1-based, in the text it was given.
    pub line: Option<usize>,

    /// What it said, in one line.
    pub message: String,
}

/// One text's turn through a formatter.
#[derive(Debug, Clone)]
pub(crate) enum Outcome {
    /// The text as the formatter left it.
    Formatted(String),

    /// The formatter ran and said the text does not parse.
    Unparsed(Unparsed),

    /// The formatter did not finish or could not run: nothing is known about the text.
    Undecided(String),

    /// The turn was stopped: the formatter was killed, and its answer means nothing.
    Stopped,
}

/// What a checker had to say about one unit after a batch wrote into it.
#[derive(Debug)]
pub(crate) enum Check {
    /// Nothing was verified: the checker did not run, or the turn was stopped.
    Unchecked(String),

    /// No errors and no warnings.
    Clean,

    /// Every error first, then every warning, in the checker's own order.
    Faults {
        faults: Vec<Fault>,
        warns: Vec<Fault>,
    },
}

/// One diagnostic, cut to what a model acts on.
#[derive(Debug, Clone)]
pub(crate) struct Fault {
    /// The file, named the way its own unit names it.
    pub file: String,

    /// The line it points at, 1-based.
    pub line: usize,

    /// What went wrong.
    pub message: String,

    /// What the checker suggests, its own words.
    pub helps: Vec<String>,
}
