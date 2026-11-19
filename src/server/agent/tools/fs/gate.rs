//! The process gate: every external tool the file tools run goes through here. A child leads its
//! own process group, so a stuck child dies with its whole tree instead of holding the call it
//! belongs to.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::server::platform::proc;
use crate::server::platform::stop::Stop;

/// How often a waiting caller looks at its child.
const TICK: Duration = Duration::from_millis(5);

/// How long the platform gets to finish a stop before the caller moves on.
const KILL_LIMIT: Duration = Duration::from_millis(50);

/// How many bytes of one stream a child may hand back.
const CAP: usize = 8 << 20;

/// What one child handed back.
pub(crate) struct Ran {
    /// Its exit code; `None` when a signal ended it or it was killed.
    pub(crate) code: Option<i32>,
    pub(crate) out: String,
    pub(crate) err: String,
    /// Whether the deadline killed it.
    pub(crate) timed_out: bool,
    /// Whether the turn's stop killed it.
    pub(crate) stopped: bool,
}

/// Run one child to its end, or kill its whole group when the deadline passes or the turn is
/// stopped. Every tick looks at both: a stop must never wait out a deadlock in someone else's tree.
pub(crate) fn run(
    mut cmd: Command,
    input: Option<&[u8]>,
    limit: Option<Duration>,
    stop: &Stop,
) -> std::io::Result<Ran> {
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let process = proc::Process::spawn(proc::Origin::Tool, &mut cmd)?;

    // The input goes down a thread of its own: a child that reads late must not hold this call.
    if let Some(bytes) = input {
        let mut stdin = process.take_stdin().expect("stdin is piped");
        let owned = bytes.to_vec();
        let _ = std::thread::Builder::new()
            .name("mypi-gate-in".into())
            .spawn(move || {
                let _ = stdin.write_all(&owned);
            });
    }
    let out = drain(process.take_stdout().expect("stdout is piped"));
    let err = drain(process.take_stderr().expect("stderr is piped"));

    let deadline = limit.map(|limit| Instant::now() + limit);
    let mut timed_out = false;
    let mut stopped = false;
    let status = loop {
        if stop.is_stopped() {
            stopped = true;
            process.stop();
            let _ = process.wait_timeout(KILL_LIMIT);
            break None;
        }
        let now = Instant::now();
        if deadline.is_some_and(|at| now >= at) {
            timed_out = true;
            process.stop();
            let _ = process.wait_timeout(KILL_LIMIT);
            break None;
        }
        let wait = deadline
            .map(|at| at.saturating_duration_since(now))
            .unwrap_or(TICK)
            .min(TICK);
        match process.wait_timeout(wait) {
            Err(e) => {
                process.stop();
                let _ = process.wait();
                return Err(e);
            }
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
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

/// A program of a toolchain: next to this executable first, then whatever PATH holds.
pub(crate) fn beside(name: &str) -> PathBuf {
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
