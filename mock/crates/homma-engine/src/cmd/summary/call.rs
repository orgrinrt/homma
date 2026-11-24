//! The one-shot model as a command: one print-mode run, isolated from every session.
//!
//! The run gets no tools, safe mode, no setting sources, no MCP servers, no slash commands and no
//! session kept, the rules and the brief as its whole system prompt and the content on stdin, in a
//! fresh empty directory so no instruction file beside it is read. Its answer is read from its JSON
//! result. homma reads no credential: the command finds its own in the environment it inherits.

use std::io::{Read, Write};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use homma_api::SummaryConfig;
use serde::Deserialize;

use super::OneShot;

/// How long one call may run.
pub const LIMIT: Duration = Duration::from_secs(300);

/// How many characters of a failed call's words its reason keeps.
const REASON: usize = 200;

/// The command and the model it is asked to use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub model:   String,
    pub limit:   Duration,
}

impl From<&SummaryConfig> for Command {
    fn from(s: &SummaryConfig) -> Self {
        Self {
            program: s.command.clone(),
            model:   s.model.clone(),
            limit:   LIMIT,
        }
    }
}

impl Command {
    /// The arguments of one call with `system` as its whole system prompt.
    #[must_use]
    pub fn args(&self, system: &str) -> Vec<String> {
        [
            "-p",
            "--model",
            &self.model,
            "--tools",
            "",
            "--safe-mode",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--output-format",
            "json",
            "--system-prompt",
            system,
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }
}

/// The part of the JSON result read.
#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    is_error: bool,
    result:   Option<String>,
}

impl OneShot for Command {
    fn ask(&self, system: &str, content: &str) -> Result<String, String> {
        let dir = tempfile::tempdir().map_err(|e| format!("no empty directory to run in: {e}"))?;
        let child = spawn(
            std::process::Command::new(&self.program)
                .args(self.args(system))
                .current_dir(dir.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .map_err(|e| format!("`{}` did not start: {e}", self.program))?;
        let (status, stdout, stderr) = drive(child, content, self.limit)?;
        let status = status.success();
        let answer: Option<Answer> = stdout
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str(l.trim()).ok());
        match answer {
            Some(Answer {
                is_error: false,
                result: Some(r),
            }) if status => Ok(r),
            Some(Answer {
                result: Some(r),
                ..
            }) => Err(first_line(&r)),
            _ if !status => {
                Err(first_line(if stderr.trim().is_empty() {
                    &stdout
                } else {
                    &stderr
                }))
            },
            _ => Err("the command printed no JSON result".to_string()),
        }
    }
}

/// Spawn `command` alone in a process group of its own where the platform has them, so that ending
/// the call ends whatever it started too.
pub(crate) fn spawn(command: &mut std::process::Command) -> std::io::Result<Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// End the child and, where it leads a process group, everything in the group.
fn end(child: &mut Child) {
    #[cfg(unix)]
    {
        // The group's id is the child's pid, since it was spawned to lead one.
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Feed `content` to the child, read both its pipes, and wait at most `limit` for it to exit and
/// for both pipes to close, ending it and its group past that, so a process it left holding a pipe
/// cannot keep the call waiting. Gives whether it exited 0, and what it printed.
pub(crate) fn drive(
    mut child: Child,
    content: &str,
    limit: Duration,
) -> Result<(ExitStatus, String, String), String> {
    let ran_past = || format!("the call ran past {} s and was ended", limit.as_secs());
    let mut stdin = child.stdin.take();
    let body = content.to_owned();
    // Not joined: a command that never reads leaves the write blocked until the call is ended.
    std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            // A command that exits before reading closes the pipe; its exit says why.
            let _ = s.write_all(body.as_bytes());
        }
    });
    let out = reader(child.stdout.take());
    let err = reader(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() >= limit => {
                end(&mut child);
                return Err(ran_past());
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                end(&mut child);
                return Err(format!("waiting on the call: {e}"));
            },
        }
    };
    let read = |pipe: &Receiver<String>| pipe.recv_timeout(limit.saturating_sub(start.elapsed()));
    match (read(&out), read(&err)) {
        (Ok(stdout), Ok(stderr)) => Ok((status, stdout, stderr)),
        _ => {
            end(&mut child);
            Err(ran_past())
        },
    }
}

/// Read `pipe` to its end on a thread of its own, the text arriving on the channel returned.
fn reader<R: Read + Send + 'static>(pipe: Option<R>) -> Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_string(&mut s);
        }
        let _ = tx.send(s);
    });
    rx
}

/// The first line that says anything, at most [`REASON`] characters.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("the command said nothing");
    line.chars().take(REASON).collect()
}
