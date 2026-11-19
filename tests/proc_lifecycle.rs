// Lifecycle checks for one process tree: birth, waiting, stopping, and what the logs say.

use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use mypi::server::platform::log;
use mypi::server::platform::proc::{Origin, Process};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
const SHORT: &str = "exit 7";
#[cfg(unix)]
const SHORT: &str = "exit 7";

#[cfg(windows)]
const LONG: &str = "ping -n 30 127.0.0.1";
#[cfg(unix)]
const LONG: &str = "sleep 30";

fn command_of(line: &str) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", line]);
        cmd
    };
    #[cfg(unix)]
    let mut cmd = {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", line]);
        cmd
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

fn records(pid: u32) -> Vec<log::Record> {
    let needle = format!("pid={pid} ");
    log::by_scope("proc")
        .into_iter()
        .filter(|record| record.text.contains(&needle))
        .collect()
}

fn await_record(pid: u32, needle: &str) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        if let Some(found) = records(pid)
            .into_iter()
            .find(|record| record.text.contains(needle))
        {
            return Some(found.text);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn code_of(text: &str) -> Option<String> {
    text.split_whitespace()
        .find_map(|field| field.strip_prefix("code="))
        .map(str::to_string)
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt as _;
    let mut cmd = Command::new("tasklist");
    cmd.args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let Ok(output) = cmd.output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let wanted = pid.to_string();
    text.split_whitespace().any(|field| field == wanted)
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn external_kill(pid: u32) {
    use std::os::windows::process::CommandExt as _;
    let mut cmd = Command::new("taskkill");
    cmd.args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.status();
}

#[cfg(unix)]
fn external_kill(pid: u32) {
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
}

fn spawn(origin: Origin, line: &str) -> Process {
    let mut cmd = command_of(line);
    Process::spawn(origin, &mut cmd).expect("a tree is created")
}

#[test]
fn spawn_and_natural_exit() {
    let process = spawn(Origin::Tool, SHORT);
    let pid = process.id();
    let status = process.wait().expect("the leader exits");
    assert_eq!(status.code(), Some(7));
    assert_eq!(process.wait().expect("a second read").code(), Some(7));
    assert!(await_record(pid, "spawn origin=").is_some());
    let exit = await_record(pid, "exit origin=").expect("one exit record");
    assert!(exit.contains("method=natural"), "{exit}");
    assert_eq!(code_of(&exit).as_deref(), Some("7"));
}

#[test]
fn drop_does_not_kill() {
    let process = spawn(Origin::Tool, LONG);
    let pid = process.id();
    drop(process);
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(pid), "a dropped handle must not take the tree down");
    external_kill(pid);
    assert!(await_record(pid, "exit origin=").is_some());
}

#[test]
fn stop_is_idempotent() {
    let process = spawn(Origin::Tool, LONG);
    let pid = process.id();
    let started = Instant::now();
    process.stop();
    process.stop();
    assert!(started.elapsed() < Duration::from_secs(3));
    await_record(pid, "exit origin=").expect("one exit record");
    let exits = records(pid)
        .iter()
        .filter(|record| record.text.contains("exit origin="))
        .count();
    assert_eq!(exits, 1);
}

#[test]
fn stop_while_waiting() {
    let process = spawn(Origin::Tool, LONG);
    let waiter = process.clone();
    let handle = std::thread::spawn(move || waiter.wait());
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    process.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
    handle.join().expect("the waiter thread").expect("a status");
}

#[test]
fn concurrent_spawn_serialized() {
    let barrier = Arc::new(Barrier::new(8));
    let mut joins = Vec::new();
    for _ in 0..8 {
        let barrier = barrier.clone();
        joins.push(std::thread::spawn(move || {
            barrier.wait();
            let mut cmd = command_of(SHORT);
            Process::spawn(Origin::Tool, &mut cmd).map(|process| {
                let pid = process.id();
                let _ = process.wait();
                pid
            })
        }));
    }
    let mut ids = Vec::new();
    for join in joins {
        ids.push(join.join().expect("a creator thread").expect("a tree"));
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 8);
    for pid in ids {
        assert!(await_record(pid, "spawn origin=").is_some());
    }
}

#[test]
fn detached_survives_handle_release() {
    let mut cmd = command_of(LONG);
    let process = Process::spawn_detached(Origin::Frontend, &mut cmd).expect("a detached tree");
    let pid = process.id();
    drop(process);
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(pid), "a detached tree outlives its handle");
    external_kill(pid);
    assert!(await_record(pid, "exit origin=").is_some());
}

#[test]
fn detached_rejects_other_origins() {
    let mut cmd = command_of(SHORT);
    let error = Process::spawn_detached(Origin::Tool, &mut cmd).expect_err("only the front end");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(windows)]
#[test]
fn windows_stop_is_one_step() {
    let process = spawn(Origin::Tool, LONG);
    let pid = process.id();
    let started = Instant::now();
    process.stop();
    assert!(started.elapsed() <= Duration::from_millis(100));
    let exit = await_record(pid, "exit origin=").expect("one exit record");
    assert!(exit.contains("grace=none"), "{exit}");
}

#[cfg(unix)]
#[test]
fn unix_grace_then_kill() {
    let process = spawn(Origin::Tool, "trap '' TERM; sleep 30");
    let pid = process.id();
    let started = Instant::now();
    process.stop();
    let took = started.elapsed();
    assert!(took >= Duration::from_secs(2), "{took:?}");
    assert!(took <= Duration::from_millis(3000), "{took:?}");
    let exit = await_record(pid, "exit origin=").expect("one exit record");
    assert!(exit.contains("method=kill_tree"), "{exit}");
    assert!(exit.contains("grace=ignored"), "{exit}");
    assert_eq!(code_of(&exit).as_deref(), Some("none"));
}

#[cfg(unix)]
#[test]
fn unix_term_within_grace() {
    let process = spawn(Origin::Tool, "sleep 30");
    let pid = process.id();
    let started = Instant::now();
    process.stop();
    assert!(started.elapsed() < Duration::from_secs(2));
    let exit = await_record(pid, "exit origin=").expect("one exit record");
    assert!(exit.contains("method=request_stop"), "{exit}");
    assert!(exit.contains("grace=within"), "{exit}");
}
