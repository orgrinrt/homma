//! The one-shot model as a command: one print-mode run, isolated from every session.
//!
//! The run gets no tools, safe mode, no setting sources, no MCP servers, no slash commands and no
//! session kept, the rules and the brief as its whole system prompt and the content on stdin, in a
//! fresh empty directory so no instruction file beside it is read. The system prompt goes in a file
//! outside that directory, named by `--system-prompt-file`, so it is not in the command's arguments.
//! Its answer is read from its JSON result. homma reads no credential: the command finds its own in
//! the environment it inherits. The run is the engine's bounded run of a child.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use homma_api::SummaryConfig;
use serde::Deserialize;

use super::OneShot;
use crate::cmd::util::bounded;

/// How long one call may run.
pub const LIMIT: Duration = Duration::from_secs(300);

/// How many characters of a failed call's words its reason keeps.
pub const REASON: usize = 200;

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
    /// The arguments of one call whose whole system prompt is in the file at `system`.
    #[must_use]
    pub fn args(&self, system: &Path) -> Vec<String> {
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
            "--system-prompt-file",
            &system.display().to_string(),
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
        // Two directories: the run's own stays empty, and the prompt's file sits in the other.
        let dir = tempfile::tempdir().map_err(|e| format!("no empty directory to run in: {e}"))?;
        let held =
            tempfile::tempdir().map_err(|e| format!("no place for the system prompt: {e}"))?;
        let prompt = held.path().join("system-prompt");
        std::fs::write(&prompt, system)
            .map_err(|e| format!("the system prompt could not be written: {e}"))?;
        let child = bounded::spawn(
            std::process::Command::new(&self.program)
                .args(self.args(&prompt))
                .current_dir(dir.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .map_err(|e| format!("`{}` did not start: {e}", self.program))?;
        let ran = bounded::run(child, content, self.limit)
            .map_err(|s| format!("the call: {}", s.reason(self.limit)))?;
        let (stdout, stderr) = ran.text();
        let status = ran.status.success();
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

/// The first line that says anything, at most [`REASON`] characters.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("the command said nothing");
    line.chars().take(REASON).collect()
}
