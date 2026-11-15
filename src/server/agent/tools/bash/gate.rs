//! The compiler gate: rustfmt formats, `cargo check` types, and neither outlives its deadline.
//! Every child leads its own process group, so a stuck formatter dies with its whole tree instead
//! of holding the call it belongs to.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::server::platform::proc::{self, Signal};
use crate::server::platform::stop::Stop;

/// The comment a change is wrapped in, so the formatter's output can be walked back to it.
pub(crate) const MARK: &str = "mypi:splice";

/// How long one formatter round trip may take before its tree is killed.
const FORMAT_LIMIT: Duration = Duration::from_secs(5);

/// How long `cargo check` gets; past this the call goes ahead without it.
pub(crate) const CHECK_LIMIT: Duration = Duration::from_millis(1500);

/// How often a waiting caller looks at its child.
const TICK: Duration = Duration::from_millis(5);
const CACHE: usize = 32;

/// How many bytes of one stream a child may hand back.
const CAP: usize = 8 << 20;

/// How many faults one answer lists.
pub(crate) const FAULTS: usize = 12;

/// What the formatter could not make sense of.
#[derive(Debug, Clone)]
pub(crate) struct Broken {
    /// The line it pointed at, 1-based, in the text it was given.
    pub line: Option<usize>,

    /// What it said, in one line.
    pub message: String,
}

/// One text's turn through the formatter.
#[derive(Debug, Clone)]
pub(crate) enum Outcome {
    /// The text as rustfmt left it.
    Formatted(String),
    Broken(Broken),
    /// The turn was stopped: the formatter was killed, and its answer means nothing.
    Stopped,
}

/// What `cargo check` had to say about one file after a change.
#[derive(Debug)]
pub(crate) enum Check {
    /// Not this call's business: outside the workspace, or not a crate's own file.
    Skipped,
    /// No errors.
    Clean,
    /// The compiler was still working when the deadline came.
    TimedOut,
    /// The turn was stopped: the file is written and unchecked, whether the compiler ran or not.
    Unchecked,
    /// Errors, the changed file's first; the counts describe all of them, listed or not.
    Faults {
        faults: Vec<Fault>,
        here: usize,
        other: usize,
    },
}

/// One compiler error, cut to what a model acts on.
#[derive(Debug, Clone)]
pub(crate) struct Fault {
    /// The file, relative to the workspace root.
    pub file: String,

    /// The line it points at, 1-based.
    pub line: usize,

    /// What went wrong.
    pub message: String,

    /// What the compiler suggests, its own words.
    pub helps: Vec<String>,
}

/// Format one text, or say what stopped it. A turn that was stopped remembers nothing: its
/// answer depends on a kill, and a retry of the same text must go through the formatter again.
pub(crate) fn format(text: &str, edition: &str, stop: &Stop) -> Outcome {
    let key = (edition.to_string(), hash(text));
    if let Some(hit) = remembered(&key) {
        return hit;
    }
    let outcome = formatter(text, edition, stop);
    if !matches!(outcome, Outcome::Stopped) {
        remember(key, &outcome);
    }
    outcome
}

/// The crate's edition, from the nearest `Cargo.toml` above the file. Read once per directory.
pub(crate) fn edition(path: &Path) -> String {
    let Some(dir) = path.parent() else {
        return DEFAULT_EDITION.to_string();
    };
    static EDITIONS: LazyLock<Mutex<HashMap<PathBuf, String>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Some(held) = lock(&EDITIONS).get(dir) {
        return held.clone();
    }
    let found = edition_at(dir);
    lock(&EDITIONS).insert(dir.to_path_buf(), found.clone());
    found
}

/// The edition of the crate rooted at `dir`, one entry per ancestor until a manifest appears.
fn edition_at(dir: &Path) -> String {
    for at in dir.ancestors() {
        let manifest = at.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        return std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|text| edition_of(&text))
            .unwrap_or_else(|| DEFAULT_EDITION.to_string());
    }
    DEFAULT_EDITION.to_string()
}

/// The edition an older manifest still accepts.
const DEFAULT_EDITION: &str = "2021";

/// The first `edition = "…"` in a manifest.
fn edition_of(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("edition") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if matches!(value, "2015" | "2018" | "2021" | "2024") {
            return Some(value.to_string());
        }
    }
    None
}

/// Type-check the crate this file belongs to. Errors in the file itself come first; every other
/// file's follow. A crate still compiling when the deadline comes is dropped; one the user stopped
/// is killed and answers nothing.
pub(crate) fn check(path: &Path, stop: &Stop) -> Check {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return Check::Skipped;
    }
    let Some(root) = crate_root(path) else {
        return Check::Skipped;
    };
    let mut cmd = Command::new(cargo());
    cmd.args(["check", "--message-format=json", "--quiet"])
        .current_dir(&root);
    let ran = match run(cmd, None, CHECK_LIMIT, stop) {
        Ok(ran) => ran,
        Err(_) => return Check::Skipped,
    };
    if ran.stopped {
        return Check::Unchecked;
    }
    if ran.timed_out {
        return Check::TimedOut;
    }
    let mut faults = faults(&root, &ran.out);
    if faults.is_empty() {
        return Check::Clean;
    }
    let mine = shown(&root, path);
    faults.sort_by_key(|fault| (fault.file != mine, fault.line));
    faults.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.message == b.message);
    let here = faults.iter().filter(|fault| fault.file == mine).count();
    let other = faults.len() - here;
    faults.truncate(FAULTS);
    Check::Faults {
        faults,
        here,
        other,
    }
}

/// The crate this file belongs to: the nearest directory above it that holds a manifest.
pub(crate) fn crate_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

/// A path as an answer names it: relative to the crate root the compiler reports against.
pub(crate) fn shown(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The errors one `cargo check` reported, whatever file they sit in.
fn faults(root: &Path, out: &str) -> Vec<Fault> {
    let mut list: Vec<Fault> = Vec::new();
    for line in out.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        if message.get("level").and_then(|l| l.as_str()) != Some("error") {
            continue;
        }
        let Some(spans) = message.get("spans").and_then(|s| s.as_array()) else {
            continue;
        };
        let span = spans
            .iter()
            .find(|span| span.get("is_primary").and_then(|p| p.as_bool()) == Some(true))
            .or_else(|| spans.first());
        let Some(span) = span else {
            continue;
        };
        let Some(file) = span.get("file_name").and_then(|f| f.as_str()) else {
            continue;
        };
        let line = span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(1) as usize;
        let text = message
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("error")
            .trim()
            .to_string();
        let mut helps: Vec<String> = Vec::new();
        for child in message
            .get("children")
            .and_then(|c| c.as_array())
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if child.get("level").and_then(|l| l.as_str()) != Some("help") {
                continue;
            }
            let Some(said) = child.get("message").and_then(|m| m.as_str()) else {
                continue;
            };
            // A suggestion names its replacement in a span, not in its own sentence.
            let named = child
                .get("spans")
                .and_then(|s| s.as_array())
                .and_then(|spans| spans.first())
                .and_then(|span| span.get("suggested_replacement"))
                .and_then(|name| name.as_str())
                .filter(|name| !name.contains('\n'));
            let said = match named {
                Some(name) => format!("{}: {name}", said.trim()),
                None => said.trim().to_string(),
            };
            if !said.is_empty() && !helps.contains(&said) {
                helps.push(said);
            }
        }
        helps.truncate(2);
        let file = Path::new(file);
        let named = if file.is_absolute() {
            file.to_path_buf()
        } else {
            root.join(file)
        };
        list.push(Fault {
            file: shown(root, &named),
            line,
            message: text,
            helps,
        });
    }
    list
}

/// One formatter round trip.
fn formatter(text: &str, edition: &str, stop: &Stop) -> Outcome {
    let bin = rustfmt();
    let mut cmd = Command::new(&bin);
    cmd.args(["--edition", edition, "--emit", "stdout", "--quiet"]);
    match run(cmd, Some(text.as_bytes()), FORMAT_LIMIT, stop) {
        Ok(ran) if ran.stopped => Outcome::Stopped,
        Ok(ran) if ran.timed_out => Outcome::Broken(Broken {
            line: None,
            message: format!(
                "rustfmt did not finish in {} s, so it was killed",
                FORMAT_LIMIT.as_secs()
            ),
        }),
        Ok(ran) if ran.code == Some(0) => Outcome::Formatted(ran.out),
        Ok(ran) => Outcome::Broken(parse(&ran.err)),
        Err(e) => Outcome::Broken(Broken {
            line: None,
            message: format!(
                "failed to run `{}`: {e}; put rustfmt beside this executable or on PATH",
                bin.display()
            ),
        }),
    }
}

/// What one child handed back.
struct Ran {
    /// Its exit code; `None` when a signal ended it or it was killed.
    code: Option<i32>,
    out: String,
    err: String,
    /// Whether the deadline killed it.
    timed_out: bool,
    /// Whether the turn's stop killed it.
    stopped: bool,
}

/// Run one child to its end, or kill its whole group when the deadline passes or the turn is
/// stopped. Every tick looks at both: a stop must never wait out a deadlock in someone else's tree.
fn run(
    mut cmd: Command,
    input: Option<&[u8]>,
    limit: Duration,
    stop: &Stop,
) -> std::io::Result<Ran> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = proc::spawn_grouped(&mut cmd)?;

    // The input goes down a thread of its own: a child that reads late must not hold this call.
    if let Some(bytes) = input {
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let owned = bytes.to_vec();
        let _ = std::thread::Builder::new()
            .name("mypi-gate-in".into())
            .spawn(move || {
                let _ = stdin.write_all(&owned);
            });
    }
    let out = drain(child.stdout.take().expect("stdout is piped"));
    let err = drain(child.stderr.take().expect("stderr is piped"));

    let deadline = Instant::now() + limit;
    let mut timed_out = false;
    let mut stopped = false;
    let status = loop {
        match child.try_wait() {
            Err(e) => {
                proc::signal_group(child.id(), Signal::Kill);
                let _ = child.wait();
                return Err(e);
            }
            Ok(Some(status)) => break Some(status),
            Ok(None) if stop.is_stopped() => {
                stopped = true;
                proc::signal_group(child.id(), Signal::Kill);
                let _ = child.wait();
                break None;
            }
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                proc::signal_group(child.id(), Signal::Kill);
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(TICK),
        }
    };
    Ok(Ran {
        code: status.and_then(|status| status.code()),
        out: out.join().unwrap_or_default(),
        err: err.join().unwrap_or_default(),
        timed_out,
        stopped,
    })
}

/// Read one stream to its end on a thread of its own: a child that talks past the pipe's buffer
/// must never block on a reader that is not there yet.
fn drain(pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::Builder::new()
        .name("mypi-gate-out".into())
        .spawn(move || {
            use std::io::Read as _;
            let mut rdr = std::io::BufReader::new(pipe);
            let mut raw = [0u8; 8192];
            let mut text: Vec<u8> = Vec::new();
            while let Ok(n) = rdr.read(&mut raw) {
                if n == 0 {
                    break;
                }
                if text.len() < CAP {
                    text.extend_from_slice(&raw[..n]);
                }
            }
            String::from_utf8_lossy(&text).to_string()
        })
        .expect("a reader thread")
}

/// The `rustfmt` to run: the one beside this executable first, then whatever PATH holds.
fn rustfmt() -> PathBuf {
    static FOUND: LazyLock<PathBuf> = LazyLock::new(|| beside("rustfmt"));
    FOUND.clone()
}

/// The `cargo` to run: the one beside this executable first, then whatever PATH holds.
fn cargo() -> PathBuf {
    static FOUND: LazyLock<PathBuf> = LazyLock::new(|| beside("cargo"));
    FOUND.clone()
}

/// A program of the toolchain: next to this executable, else on PATH.
fn beside(name: &str) -> PathBuf {
    let named = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let near = dir.join(&named);
        if near.is_file() {
            return near;
        }
    }
    PathBuf::from(named)
}

/// rustfmt's stderr, cut to the one line and the line number a model can act on.
fn parse(stderr: &str) -> Broken {
    let mut message: Option<String> = None;
    let mut line = None;
    for raw in stderr.lines() {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            line = rest.split(':').nth(1).and_then(|spot| spot.parse().ok());
            continue;
        }
        if message.is_some() {
            continue;
        }
        let cleaned = trimmed.strip_prefix("error: ").or_else(|| {
            trimmed
                .strip_prefix("error[")
                .and_then(|rest| rest.split_once("]: ").map(|(_, tail)| tail))
        });
        if let Some(cleaned) = cleaned {
            message = Some(cleaned.trim_end_matches(':').to_string());
        }
    }
    Broken {
        line,
        message: message.unwrap_or_else(|| "rustfmt refused the text".to_string()),
    }
}

type Key = (String, u64);

/// The formatted texts this process has already produced, oldest first.
static MEMO: LazyLock<Mutex<Vec<(Key, Outcome)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn remembered(key: &Key) -> Option<Outcome> {
    lock(&MEMO)
        .iter()
        .find(|(held, _)| held == key)
        .map(|(_, outcome)| outcome.clone())
}

fn remember(key: Key, outcome: &Outcome) {
    // A text too large to cache is simply not cached: the retries that follow pay one process.
    let size = match outcome {
        Outcome::Formatted(text) => text.len(),
        Outcome::Broken(_) | Outcome::Stopped => 0,
    };
    if size > CACHE {
        return;
    }
    let mut held = lock(&MEMO);
    while held.iter().map(|(_, held)| weigh(held)).sum::<usize>() + size > CACHE {
        held.remove(0);
    }
    held.push((key, outcome.clone()));
}

/// How many bytes one cached outcome holds.
fn weigh(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Formatted(text) => text.len(),
        Outcome::Broken(_) | Outcome::Stopped => 0,
    }
}

/// A lock that survives a panicking holder: the state behind it is a cache, never a promise.
fn lock<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|e| e.into_inner())
}

/// A text's content hash.
fn hash(text: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
