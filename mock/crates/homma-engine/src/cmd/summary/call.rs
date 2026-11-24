//! The one-shot model as a command: one print-mode run, isolated from every session.
//!
//! The run gets no tools, safe mode, no setting sources, no MCP servers, no slash commands and no
//! session kept, the rules and the brief as its whole system prompt and the content on stdin, in a
//! fresh empty directory so no instruction file beside it is read. Its answer is read from its JSON
//! result. homma reads no credential: the command finds its own in the environment it inherits.

use std::io::{Read, Write};
use std::process::{Child, Stdio};
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
        let child = std::process::Command::new(&self.program)
            .args(self.args(system))
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("`{}` did not start: {e}", self.program))?;
        let (status, stdout, stderr) = drive(child, content, self.limit)?;
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

/// Feed `content` to the child, read both its pipes, and wait for it at most `limit`, ending it past
/// that. Gives whether it exited 0, and what it printed.
fn drive(
    mut child: Child,
    content: &str,
    limit: Duration,
) -> Result<(bool, String, String), String> {
    let mut stdin = child.stdin.take();
    let body = content.to_owned();
    let feed = std::thread::spawn(move || {
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
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "the call ran past {} s and was ended",
                    limit.as_secs()
                ));
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("waiting on the call: {e}")),
        }
    };
    let _ = feed.join();
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    Ok((status.success(), stdout, stderr))
}

fn reader<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_string(&mut s);
        }
        s
    })
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
