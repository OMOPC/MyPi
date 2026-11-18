//! Process stop signals: the one fact the loop cannot read from the terminal.
//!
//! `SIGINT`, `SIGTERM`, `SIGQUIT` and `SIGHUP` end the front end. The loop restores the screen
//! and leaves; nothing tries to survive them.
//!
//! Windows has neither `SIGHUP` nor `SIGQUIT`, and no signal iterator: the console's `Ctrl-C`
//! and `Ctrl-Break` are the stop signals there, read out of a flag.

#[cfg(unix)]
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
#[cfg(unix)]
use signal_hook::iterator::Signals;

/// The stop signals, registered once at startup.
#[cfg(unix)]
pub struct Stop {
    signals: Signals,
}

#[cfg(unix)]
impl Stop {
    pub fn new() -> std::io::Result<Stop> {
        Ok(Stop {
            signals: Signals::new([SIGINT, SIGTERM, SIGQUIT, SIGHUP])?,
        })
    }

    /// The first stop signal seen since the last call.
    pub fn poll(&mut self) -> Option<i32> {
        self.signals.pending().next()
    }
}

#[cfg(windows)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
use signal_hook::consts::{SIGBREAK, SIGINT, SIGTERM};

/// The stop signals, registered once at startup.
#[cfg(windows)]
pub struct Stop {
    seen: Arc<AtomicUsize>,
}

#[cfg(windows)]
impl Stop {
    pub fn new() -> std::io::Result<Stop> {
        let seen = Arc::new(AtomicUsize::new(0));
        for signal in [SIGINT, SIGTERM, SIGBREAK] {
            signal_hook::flag::register_usize(signal, Arc::clone(&seen), signal as usize)?;
        }
        Ok(Stop { seen })
    }

    /// The stop signal seen since the last call; the last one wins.
    pub fn poll(&mut self) -> Option<i32> {
        match self.seen.swap(0, Ordering::Acquire) {
            0 => None,
            signal => Some(signal as i32),
        }
    }
}