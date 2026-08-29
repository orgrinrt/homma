//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `[[status.inject]]`: workspace tools whose output `homma status` carries.
//!
//! A workspace accumulates tools homma has no business owning. What fills a
//! session's context window, what the rule corpus costs to load, what is
//! outstanding on the agenda: each is real, each belongs to whoever wrote it,
//! and none of them is a thing a workspace orchestrator should grow an opinion
//! about. So homma runs them and prints what they say, in the order they are
//! declared, and knows nothing else about them.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// `[status]`, which today holds only the injections.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusConfig {
    /// In declaration order, which is print order. TOML keeps the order of an
    /// array of tables, so the manifest reads the way the output does.
    #[serde(default)]
    pub inject: Vec<Inject>,
}

/// One `[[status.inject]]` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inject {
    /// What to run. Its stdout is the payload.
    ///
    /// An argument list rather than a shell line, for the reason
    /// [`AuthConfig::token_cmd`] gives: nothing here needs a pipeline, and a
    /// shell turns every argument into something that can be quoted out of. A
    /// bare string is read as a one-element list, so a tool taking no arguments
    /// is written the way anybody would write it.
    ///
    /// A relative program path holding a separator resolves against the
    /// workspace root; a bare name is left alone so `PATH` finds it. Same rule
    /// as a token command, and the root is where a workspace's own tools live.
    ///
    /// [`AuthConfig::token_cmd`]: crate::config::AuthConfig::token_cmd
    pub tool: Argv,

    /// What to call the block. Defaults to the program's file name.
    ///
    /// Optional because a tool named `context` needs no second name, and
    /// deriving one means the minimal entry is a single key and still labels
    /// itself rather than printing an anonymous paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// A shell pipeline over the tool's stdout, whose own stdout replaces it.
    ///
    /// A shell line here and an argument list above, which looks inconsistent
    /// and is not: the two are different things. `tool` names a program to run,
    /// and this is a pipeline over text, which is what a shell is for and what
    /// this is nearly always going to be. `head -3`, `sed`, `grep -v`.
    ///
    /// **One transform rather than two.** The shape this came from carried a
    /// `format` and a `process`, and nothing could say which was which. Two
    /// keys whose difference nobody can state get used interchangeably and then
    /// disagree, so there is one, and homma owns the title and the indentation
    /// around it. A tool emitting structured output for a machine and prose for
    /// a person is a real thing to want and is a real design question, with a
    /// use case behind it rather than a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// A command, written either as a bare program name or as an argument list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argv {
    One(String),
    Many(Vec<String>),
}

impl Argv {
    /// The words, in order.
    pub fn words(&self) -> &[String] {
        match self {
            Self::One(s) => std::slice::from_ref(s),
            Self::Many(v) => v,
        }
    }

    fn program_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::One(s) => Some(s),
            Self::Many(v) => v.first_mut(),
        }
    }
}

/// What one injection produced.
#[derive(Debug, Clone, Serialize)]
pub struct Injected {
    pub title:  String,
    /// The tool's output, after `format` if one was given. Empty where the
    /// tool printed nothing, and empty where it failed.
    pub text:   String,
    /// Why this block has no output, when that is the reason it has none.
    ///
    /// A failure is carried rather than raised. `homma status` is the cheapest
    /// sanity check in the workspace and is run to find out what state things
    /// are in; a foreign script exiting non-zero is one of the things worth
    /// finding out, and it is not a reason to refuse to print the forges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// Anchor every relative program path against the workspace root, once.
///
/// The root rather than the manifest's own directory, because a workspace's
/// tools are addressed from the root and that is the directory the child runs
/// in. The two are the same wherever `workspace.path` is left at `.`, and where
/// it is not, one anchor that is also the working directory beats two that
/// disagree about which `tools/` was meant.
///
/// Public, and idempotent, for the reason [`Config::settle_token_commands`]
/// is: a caller that parsed a string rather than read a file is the only thing
/// that knows which directory the text belongs to.
///
/// [`Config::settle_token_commands`]: crate::config::Config::settle_token_commands
pub fn settle(status: &mut StatusConfig, root: &Path) {
    for entry in &mut status.inject {
        let Some(program) = entry.tool.program_mut() else {
            continue;
        };
        let p = Path::new(program.as_str());
        if p.is_relative() && p.components().count() > 1 {
            *program = root.join(p).display().to_string();
        }
    }
}

/// Run every injection, in order, and collect what each said.
///
/// `cwd` is the workspace root, so a tool's own relative paths mean what they
/// mean when it is run by hand from there.
pub fn run_all(status: &StatusConfig, cwd: &Path) -> Vec<Injected> {
    status
        .inject
        .iter()
        .map(|entry| run_one(entry, cwd))
        .collect()
}

fn run_one(entry: &Inject, cwd: &Path) -> Injected {
    let words = entry.tool.words();
    let title = entry
        .title
        .clone()
        .unwrap_or_else(|| derive_title(words.first().map(String::as_str)));
    let Some((program, args)) = words.split_first() else {
        return Injected {
            title,
            text: String::new(),
            // An empty `tool` is a manifest that declares an injection and
            // names nothing to run. Saying so beats printing an empty block.
            failed: Some("names no command to run".into()),
        };
    };

    let out = match Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) => out,
        Err(e) => {
            return Injected {
                title,
                text: String::new(),
                failed: Some(format!("{program} did not run: {e}")),
            };
        },
    };
    if !out.status.success() {
        // The tool's own stderr, because it is what says what went wrong and
        // homma has nothing to add to it. Trimmed to one line: a block in a
        // status report is not the place for a backtrace, and the operator can
        // run the thing themselves.
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let code = match out.status.code() {
            Some(c) => c.to_string(),
            None => "a signal".into(),
        };
        return Injected {
            title,
            text: String::new(),
            failed: Some(if why.is_empty() {
                format!("{program} exited {code}")
            } else {
                format!("{program} exited {code}: {why}")
            }),
        };
    }

    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    match &entry.format {
        None => {
            Injected {
                title,
                text: text.trim_end().to_string(),
                failed: None,
            }
        },
        Some(pipeline) => {
            match pipe_through(pipeline, &text, cwd) {
                Ok(text) => {
                    Injected {
                        title,
                        text: text.trim_end().to_string(),
                        failed: None,
                    }
                },
                // The tool ran and the pipeline over it did not, which is a
                // different fault from the tool failing and is reported as one.
                // Reporting it as the tool's failure sends whoever reads it to
                // debug a program that worked.
                Err(e) => {
                    Injected {
                        title,
                        text: String::new(),
                        failed: Some(format!("its format did not run: {e}")),
                    }
                },
            }
        },
    }
}

/// Feed `input` to `sh -c <pipeline>` and take its stdout.
fn pipe_through(pipeline: &str, input: &str, cwd: &Path) -> Result<String, String> {
    use std::io::Write;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(pipeline)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Taken out of the child so the handle drops and the pipe closes. A
    // pipeline whose first stage reads to end of input never sees one
    // otherwise, and both sides wait forever.
    if let Some(mut stdin) = child.stdin.take() {
        // A pipeline that reads none of its input closes the pipe early, and
        // writing into a closed pipe is `EPIPE` rather than a fault: `head -1`
        // is the ordinary case and it must not read as a failure.
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let code = match out.status.code() {
            Some(c) => c.to_string(),
            None => "a signal".into(),
        };
        return Err(if why.is_empty() {
            format!("exited {code}")
        } else {
            format!("exited {code}: {why}")
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A name for a block whose entry gave none: the program's file name.
fn derive_title(program: Option<&str>) -> String {
    program
        .map(|p| {
            PathBuf::from(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string())
        })
        .unwrap_or_else(|| "inject".to_string())
}

#[cfg(test)]
#[path = "inject_tests.rs"]
mod tests;
