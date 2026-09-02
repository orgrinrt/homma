//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! One way to run a program: every gate step and every git call goes through
//! here, so what was run, where, and what came back is captured once.

use std::{fmt, path::Path, process::Command};

/// What a program produced: its exit status and both streams, decoded lossily
/// since a build tool's output is for reading rather than for parsing bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub program: String,
    pub args:    Vec<String>,
    pub status:  Option<i32>,
    pub stdout:  String,
    pub stderr:  String,
}

impl Output {
    /// Whether the program exited zero.
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }

    /// Both streams, stdout first, the way a person reads a log.
    pub fn log(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.stdout);
        if !self.stderr.is_empty() {
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&self.stderr);
        }
        s
    }

    /// The command line as it was run, for a message.
    pub fn command_line(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// The program could not be started at all, which is different from it
/// running and failing.
#[derive(Debug)]
pub struct Spawn {
    pub program: String,
    pub source:  std::io::Error,
}

impl fmt::Display for Spawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not run `{}`: {}", self.program, self.source)
    }
}

impl std::error::Error for Spawn {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Run `program` with `args` in `cwd`, capturing everything. A non-zero exit
/// is an `Ok` output whose `ok()` is false; only failing to start is an error.
pub fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<Output, Spawn> {
    let out = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|source| Spawn {
            program: program.to_string(),
            source,
        })?;
    Ok(Output {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run `program` with `args` in `cwd` and an environment variable set for
/// that call alone, which is how a token reaches `cargo publish` without
/// reaching the shell that ran homma.
pub fn run_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<Output, Spawn> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|source| Spawn {
        program: program.to_string(),
        source,
    })?;
    Ok(Output {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run `program` with `args` in `cwd`, feeding `stdin` to it, for the git
/// plumbing that reads an object or a tree listing from its input.
pub fn run_stdin(cwd: &Path, program: &str, args: &[&str], stdin: &str) -> Result<Output, Spawn> {
    use std::{io::Write, process::Stdio};
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Spawn {
            program: program.to_string(),
            source,
        })?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin.as_bytes()).map_err(|source| Spawn {
            program: program.to_string(),
            source,
        })?;
    }
    let out = child.wait_with_output().map_err(|source| Spawn {
        program: program.to_string(),
        source,
    })?;
    Ok(Output {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_reaches_the_child() {
        let out = run_stdin(&std::env::temp_dir(), "cat", &[], "fed\n").unwrap();
        assert_eq!(out.stdout, "fed\n");
    }

    #[test]
    fn a_zero_exit_is_ok_and_a_nonzero_one_is_not() {
        let dir = std::env::temp_dir();
        assert!(run(&dir, "true", &[]).unwrap().ok());
        assert!(!run(&dir, "false", &[]).unwrap().ok());
    }

    #[test]
    fn stdout_is_captured_and_the_command_line_is_reproduced() {
        let out = run(&std::env::temp_dir(), "echo", &["hello", "there"]).unwrap();
        assert_eq!(out.stdout, "hello there\n");
        assert_eq!(out.command_line(), "echo hello there");
        assert_eq!(out.log(), "hello there\n");
    }

    #[test]
    fn a_program_that_does_not_exist_is_a_spawn_error_not_a_failed_run() {
        let err = run(&std::env::temp_dir(), "homma-no-such-program-xyz", &[]).unwrap_err();
        assert!(err.to_string().contains("homma-no-such-program-xyz"));
    }

    #[test]
    fn an_environment_variable_reaches_the_child_and_only_that_child() {
        let out = run_with_env(&std::env::temp_dir(), "sh", &["-c", "echo $HOMMA_SH_T"], &[(
            "HOMMA_SH_T",
            "set",
        )])
        .unwrap();
        assert_eq!(out.stdout, "set\n");
        assert!(std::env::var("HOMMA_SH_T").is_err());
    }
}
