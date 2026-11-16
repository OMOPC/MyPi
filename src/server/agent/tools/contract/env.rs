//! One session's conditions for its whole life: budgets, shell, folders, and the reads it holds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::view::{Export, Source};
use crate::server::agent::tools::workspace::Workspace;
use crate::server::config::rules::Rules;

/// What a tool reads: one session's conditions, built when it was booked.
pub struct Env {
    /// The working directory handle every call reads.
    pub workspace: Workspace,
    pub limits: Limits,
    /// What `grep` expands and what it folds.
    pub grep: GrepLimits,
    pub bash: BashEnv,
    /// The session's stop signal; a tool that blocks registers what stopping it means.
    pub stop: Arc<crate::server::platform::stop::Stop>,
    pub session: i64,
    pub batch_len: usize,
    pub tools: BTreeSet<String>,
    /// The model roster the config knew when the loop was built: address → display name.
    pub models: ModelNames,
    pub aliases: Aliases,
    /// The budget a spawned sub-session gets when the call names none.
    pub default_budget: u32,
    /// Lines an edited item may take and still be handed over whole.
    pub full_item_budget: usize,
    /// Whether the session's model takes image parts on the wire.
    pub vision: bool,
    pub export: Arc<dyn Export>,
    pub paths: Paths,
    pub source: Arc<dyn Source>,
    /// The site rules `web` narrows a page with: one read, when the config was loaded.
    pub rules: Arc<Rules>,
    /// What this conversation's reads have already shown the model.
    pub seen: Seen,
    /// The batch now running, so a tool can merge the siblings that touch its own file.
    pub batch: Vec<crate::server::agent::request::contract::ToolCall>,
    /// The call now running; empty outside a run.
    pub current: String,
    /// What a merged run has already produced for the siblings after it, by call id.
    pub handed: BTreeMap<String, Answer>,
}

/// One finished call's answer, kept for its own turn in the batch.
#[derive(Debug, Clone)]
pub struct Answer {
    pub text: String,
    pub memo: Option<serde_json::Value>,
    /// Why the call was refused before any of its ranges could be read.
    pub refusal: Option<String>,
}

/// The session's reading memory: one module card per directory, and the stamp of every file it
/// has read. A compaction clears it, because the conversation no longer holds what was shown.
#[derive(Debug, Default)]
pub struct Seen {
    /// Module directories whose context card has been shown.
    pub dirs: std::collections::HashSet<PathBuf>,
    /// Every file read in this conversation: its line count and content hash.
    pub files: BTreeMap<PathBuf, Stamp>,
}

/// One file as it stood when it was read.
#[derive(Debug, Clone, Copy)]
pub struct Stamp {
    pub lines: usize,
    pub hash: u64,
}

impl Env {
    /// Is this tool in the session's list? The profile gate reads the same list.
    pub fn allows(&self, name: &str) -> bool {
        self.tools.contains(name)
    }
}

/// Aliases in two layers: the user's, from config, above the session's, from the database.
pub struct Aliases {
    user: BTreeMap<String, String>,
    session: BTreeMap<String, String>,
}

/// Which layer an alias came from.
#[derive(Clone, Copy)]
pub enum AliasSource {
    Config,
    Session,
}

impl AliasSource {
    pub fn tag(self) -> &'static str {
        match self {
            AliasSource::Config => "config",
            AliasSource::Session => "session",
        }
    }
}

impl Aliases {
    pub fn new(user: BTreeMap<String, String>, session: BTreeMap<String, String>) -> Self {
        Self { user, session }
    }

    /// The config layer wins: a session row under a config name is never read.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.user
            .get(name)
            .or_else(|| self.session.get(name))
            .map(String::as_str)
    }

    /// Every alias, config layer first; session rows under a config name are dropped.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, AliasSource)> {
        let user = self
            .user
            .iter()
            .map(|(name, command)| (name.as_str(), command.as_str(), AliasSource::Config));
        let session = self
            .session
            .iter()
            .filter(|(name, _)| !self.user.contains_key(name.as_str()))
            .map(|(name, command)| (name.as_str(), command.as_str(), AliasSource::Session));
        user.chain(session)
    }

    /// Is this name fixed by the config? A session write cannot take it.
    pub fn fixed(&self, name: &str) -> bool {
        self.user.contains_key(name)
    }

    /// A write lands in the session layer only.
    pub fn set(&mut self, name: String, command: String) {
        self.session.insert(name, command);
    }

    /// Removing only reaches the session layer; a config name stays.
    pub fn unset(&mut self, name: &str) -> Option<String> {
        self.session.remove(name)
    }
}

/// Inline budget for one tool result.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub lines: usize,
    pub bytes: usize,
}

/// What `grep` folds by: five counts, three for the hits and two for the x-ray.
#[derive(Debug, Clone, Copy)]
pub struct GrepLimits {
    /// Hits an answer expands whole, item and all.
    pub deep: usize,
    /// Hits it lists one line each; more than this folds to per-file counts.
    pub fold: usize,
    /// Hits one file may show.
    pub file: usize,
    /// `impl` blocks one x-ray may list.
    pub impls: usize,
    /// Members one block may list.
    pub items: usize,
}

/// The model roster: address → display name, as the config knew it when the loop was built.
pub type ModelNames = std::collections::BTreeMap<String, String>;

/// What every session's [`Env`] starts from: the process-level facts, read once.
#[derive(Clone)]
pub struct Defaults {
    pub limits: Limits,
    pub grep: GrepLimits,
    pub bash: BashEnv,
    pub aliases: BTreeMap<String, String>,
    pub models: ModelNames,
    pub default_budget: u32,
    pub full_item_budget: usize,
    pub export: Arc<dyn Export>,
    pub paths: Paths,
    pub source: Arc<dyn Source>,
    pub rules: Arc<Rules>,
}

impl Defaults {
    /// One session's conditions: its own directory, tools, aliases and stop flag.
    pub fn session(
        &self,
        session: i64,
        cwd: super::call::Cwd,
        tools: Vec<String>,
        stop: Arc<crate::server::platform::stop::Stop>,
        vision: bool,
    ) -> anyhow::Result<Env> {
        Ok(Env {
            workspace: Workspace::new(cwd),
            limits: self.limits,
            grep: self.grep,
            bash: self.bash.clone(),
            stop,
            session,
            batch_len: 0,
            tools: tools.into_iter().collect(),
            models: self.models.clone(),
            aliases: Aliases::new(self.aliases.clone(), self.source.aliases(session)?),
            default_budget: self.default_budget,
            full_item_budget: self.full_item_budget,
            vision,
            export: self.export.clone(),
            paths: self.paths.clone(),
            source: self.source.clone(),
            rules: Arc::clone(&self.rules),
            seen: Seen::default(),
            batch: Vec::new(),
            current: String::new(),
            handed: BTreeMap::new(),
        })
    }
}

/// What `bash` runs with.
#[derive(Clone)]
pub struct BashEnv {
    pub timeout: Duration,
    pub shell: crate::server::platform::shell::Shell,
}

/// The one folder a sub-session reads from that is not the project's own: the user's templates.
/// The project's `.mypi` is taken from the live working directory at call time.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The templates that belong to the user, not to this project.
    pub global_agents: PathBuf,
}
