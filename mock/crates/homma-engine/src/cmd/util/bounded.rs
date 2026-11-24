//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The one way the engine runs a child it may have to end.
//!
//! The child leads a process group of its own, is fed on one thread and read on one thread per
//! pipe, so neither side waits on the other, and gets its limit to exit and then [`GRACE`] for its
//! pipes to close. Past either, the whole group is ended by a kill signal sent to it from here, so
//! nothing it started outlives it.

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// How long the pipes may stay open once the child has exited: a process it left running that
/// holds one gets this long and is then ended.
pub(crate) const GRACE: Duration = Duration::from_secs(1);

/// What a bounded run gives back once the child exited.
#[derive(Debug)]
pub(crate) struct Ran {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Ran {
    /// Both pipes as text, any byte that is not UTF-8 replaced rather than losing the whole text.
    #[must_use]
    pub fn text(&self) -> (String, String) {
        (
            String::from_utf8_lossy(&self.stdout).into_owned(),
            String::from_utf8_lossy(&self.stderr).into_owned(),
        )
    }
}

/// Why a bounded run gave nothing back.
#[derive(Debug)]
pub(crate) enum Stopped {
    /// The child ran past its limit, and its group was ended.
    RanPast,
    /// Waiting on the child failed, and its group was ended.
    Wait(std::io::Error),
}

impl Stopped {
    /// Why, in a line, for a run whose limit was `limit`.
    #[must_use]
    pub fn reason(&self, limit: Duration) -> String {
        match self {
            Self::RanPast if limit.as_secs() == 0 => {
                format!("it ran past {} ms and was ended", limit.as_millis())
            },
            Self::RanPast if limit.subsec_nanos() == 0 => {
                format!("it ran past {} s and was ended", limit.as_secs())
            },
            Self::RanPast => format!("it ran past {} s and was ended", limit.as_secs_f64()),
            Self::Wait(e) => format!("waiting on it failed, and it was ended: {e}"),
        }
    }
}

/// Make `command` lead a process group of its own where the platform has them, so that ending the
/// group ends whatever it started too, and a group the caller leaves does not take it along.
pub(crate) fn in_own_group(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
}

/// Spawn `command` in a group of its own.
///
/// # Errors
/// The command did not start.
pub(crate) fn spawn(command: &mut Command) -> std::io::Result<Child> {
    in_own_group(command).spawn()
}

/// End the child and, where it leads a process group, everything in the group.
pub(crate) fn end(child: &mut Child) {
    end_group(child);
    let _ = child.kill();
    let _ = child.wait();
}

/// Signal the group the child leads, where the platform has groups.
fn end_group(child: &Child) {
    #[cfg(unix)]
    {
        // The group's id is the child's pid, since it was spawned to lead one. A group already gone
        // has nothing left to end, so the signal's failure says nothing.
        if let Some(pid) = pid(child) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
    }
}

#[cfg(unix)]
fn pid(child: &Child) -> Option<rustix::process::Pid> {
    i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
}

/// Whether the child has exited, read without reaping it, so its pid and its group's id stay its
/// own until it is waited on.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd"
))]
fn exited(child: &mut Child) -> std::io::Result<bool> {
    use rustix::process::{WaitId, WaitIdOptions, waitid};
    let Some(pid) = pid(child) else {
        return child.try_wait().map(|s| s.is_some());
    };
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    Ok(waitid(WaitId::Pid(pid), options)?.is_some())
}

/// Whether the child has exited, where the platform cannot read that without reaping it.
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd"
)))]
fn exited(child: &mut Child) -> std::io::Result<bool> {
    child.try_wait().map(|s| s.is_some())
}

/// Feed `input` to the child where it was spawned with a piped stdin, read both its pipes, and wait
/// at most `limit` for it to exit, then at most [`GRACE`] for both pipes to close, however little
/// of `limit` was left. A pipe still open after that is held by something the child left running,
/// so the group is ended and what arrived is all there is. Where the platform can read the exit
/// without reaping it, the child is reaped only after that, so the signal never reaches a group
/// that took its number.
///
/// # Errors
/// The child ran past `limit`, or waiting on it failed; either way its group is ended.
pub(crate) fn run(mut child: Child, input: &str, limit: Duration) -> Result<Ran, Stopped> {
    if let Some(mut stdin) = child.stdin.take() {
        let body = input.to_owned();
        // Not joined: a child that never reads leaves the write blocked until the child is ended.
        // A child that exits before reading closes the pipe, and its exit says why.
        std::thread::spawn(move || {
            let _ = stdin.write_all(body.as_bytes());
        });
    }
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    loop {
        match exited(&mut child) {
            Ok(true) => break,
            Ok(false) if start.elapsed() >= limit => {
                end(&mut child);
                return Err(Stopped::RanPast);
            },
            Ok(false) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => {
                end(&mut child);
                return Err(Stopped::Wait(e));
            },
        }
    }
    let until = Instant::now() + GRACE;
    let (stdout, out_ended) = out.take(until);
    let (stderr, err_ended) = err.take(until);
    if !(out_ended && err_ended) {
        end_group(&child);
    }
    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            end(&mut child);
            return Err(Stopped::Wait(e));
        },
    };
    Ok(Ran {
        status,
        stdout,
        stderr,
    })
}

/// What a pipe has delivered so far, and the signal that it reached its end.
struct Pipe {
    bytes: Arc<Mutex<Vec<u8>>>,
    done:  Receiver<()>,
}

impl Pipe {
    /// The bytes read, after waiting until `until` for the end of the pipe, and whether it ended.
    fn take(&self, until: Instant) -> (Vec<u8>, bool) {
        let ended = self
            .done
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .is_ok();
        (
            self.bytes.lock().map(|b| b.clone()).unwrap_or_default(),
            ended,
        )
    }
}

/// Read a pipe on a thread of its own, so a child that writes more than the pipe holds is never left
/// waiting for a reader that is waiting for the child, and what arrived is there even if the pipe
/// never ends.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Pipe {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let (tx, done) = std::sync::mpsc::channel();
    let shared = Arc::clone(&bytes);
    std::thread::spawn(move || {
        if let Some(mut r) = pipe {
            let mut chunk = [0_u8; 8192];
            loop {
                let n = match r.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => n,
                    // An interrupted read is not the end of the pipe.
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                };
                if let Ok(mut b) = shared.lock() {
                    b.extend_from_slice(&chunk[.. n]);
                }
            }
        }
        let _ = tx.send(());
    });
    Pipe {
        bytes,
        done,
    }
}
