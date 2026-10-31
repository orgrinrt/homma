//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The commands `cargo mock tools` lists, in each mockspace repository of the workspace.
//!
//! The one source that starts a process. It is run for the repositories the instance's scope names
//! where it names any, and a repository where it fails is said in the footer. The builtin subcommands
//! are the same in every repository and are taken once.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use homma_core::scope::Scope;

use super::super::doc::{Doc, Gap, Gathered, Kind};

/// How long one repository's listing may take.
pub(super) const LIMIT: Duration = Duration::from_secs(60);

/// The repositories to ask: those directly under the root with a `mockspace.toml`, within `scope`
/// where it is not empty.
pub fn repos(root: &Path, scope: &Scope) -> Vec<String> {
    repos_probing(root, scope, &mut |dir| dir.join("mockspace.toml").is_file())
}

/// [`repos`] with the probe for a repository passed in, so a test can record which directories
/// were probed: the work done, which printing the right rows does not show.
pub fn repos_probing(
    root: &Path,
    scope: &Scope,
    probe: &mut dyn FnMut(&Path) -> bool,
) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    // the scope is asked on the name, before the directory is probed
    let mut out: Vec<String> = read
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| scope.admits(n))
        .filter(|n| probe(&root.join(n)))
        .collect();
    out.sort();
    out
}

pub fn gather(root: &Path, scope: &Scope) -> Gathered {
    let names = repos(root, scope);
    if names.is_empty() {
        return Gathered {
            docs: Vec::new(),
            gaps: vec![Gap {
                source: "mock",
                count:  0,
                what:   "no mockspace repository here: mockspace commands not listed".to_string(),
            }],
        };
    }
    // One listing for each repository, side by side: the slowest decides how long the source takes.
    let listed: Vec<(String, Result<String, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = names
            .iter()
            .map(|repo| {
                let dir = root.join(repo);
                s.spawn(move || list("cargo", &dir, LIMIT))
            })
            .collect();
        names
            .iter()
            .cloned()
            .zip(handles.into_iter().map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("the listing panicked".to_string()))
            }))
            .collect()
    });
    merge(listed)
}

/// The listings of the repositories as one source: the builtins, which every repository lists alike, once,
/// and a repository where the listing failed named in the footer.
pub fn merge(listed: Vec<(String, Result<String, String>)>) -> Gathered {
    let mut out = Gathered::default();
    let mut builtins_done = false;
    for (repo, result) in listed {
        match result {
            Ok(text) => {
                for mut d in parse(&text, &repo) {
                    if d.tags.iter().any(|t| t == "builtin") {
                        if builtins_done {
                            continue;
                        }
                        d.place.clear();
                    }
                    out.docs.push(d);
                }
                builtins_done = true;
            },
            Err(why) => {
                out.gaps.push(Gap {
                    source: "mock",
                    count:  1,
                    what:   format!("mockspace commands not listed in {repo}: {why}"),
                });
            },
        }
    }
    out
}

/// What a pipe has delivered so far, and whether it reached its end.
struct Pipe {
    bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    done:  std::sync::mpsc::Receiver<()>,
}

impl Pipe {
    /// The bytes read, after waiting up to `wait` for the end of the pipe: a grandchild that kept it
    /// open leaves the end out, and what arrived is then all there is.
    fn take(&self, wait: Duration) -> (Vec<u8>, bool) {
        let ended = self.done.recv_timeout(wait).is_ok();
        (
            self.bytes.lock().map(|b| b.clone()).unwrap_or_default(),
            ended,
        )
    }
}

/// Read a pipe on a thread of its own, so a child that writes more than the pipe holds is never left
/// waiting for a reader that is waiting for the child.
fn drain<R: std::io::Read + Send + 'static>(r: Option<R>) -> Pipe {
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (tx, done) = std::sync::mpsc::channel();
    let shared = std::sync::Arc::clone(&bytes);
    std::thread::spawn(move || {
        if let Some(mut r) = r {
            let mut chunk = [0_u8; 8192];
            while let Ok(n) = r.read(&mut chunk) {
                if n == 0 {
                    break;
                }
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

/// End the child and what it started: its process group where there is one.
fn end(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// What `cargo mock tools` prints in `dir`.
pub fn list(program: &str, dir: &Path, limit: Duration) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(["mock", "tools"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cargo mock could not start: {e}"))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() > limit => {
                end(&mut child);
                return Err(format!(
                    "cargo mock tools took longer than {} seconds",
                    limit.as_secs()
                ));
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    };
    // A grandchild that kept the pipe open outlives the child, so the reads get a second and no more.
    let grace = Duration::from_secs(1).min(limit.saturating_sub(start.elapsed()));
    let until = Instant::now() + grace;
    let (out, out_ended) = out.take(grace);
    let (err, err_ended) = err.take(until.saturating_duration_since(Instant::now()));
    // A pipe still open after the grace is held by something the child left running, which is ended.
    if !(out_ended && err_ended) {
        end(&mut child);
    }
    if !status.success() {
        let err = String::from_utf8_lossy(&err);
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it exited non-zero");
        return Err(format!("cargo mock tools failed: {}", line.trim()));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// The commands in a listing: `    mock <usage>  [<kind>  ]<summary>` under a heading in capitals.
pub fn parse(text: &str, repo: &str) -> Vec<Doc> {
    let mut docs = Vec::new();
    let mut project = false;
    for l in text.lines() {
        if !l.starts_with(' ') && !l.trim().is_empty() {
            project = l.trim().starts_with("PROJECT");
            continue;
        }
        let Some(rest) = l.trim_start().strip_prefix("mock ") else {
            continue;
        };
        let parts: Vec<&str> = rest
            .split("  ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let Some(usage) = parts.first() else {
            continue;
        };
        let name = usage.split_whitespace().next().unwrap_or(usage);
        let (kind, summary) = match (project, parts.len()) {
            (true, n) if n >= 3 => (parts[1], parts[2 ..].join(" ")),
            (_, n) if n >= 2 => ("", parts[1 ..].join(" ")),
            _ => ("", String::new()),
        };
        let mut d = Doc::new(Kind::Mock, name, &format!("cargo mock {usage}"), &summary);
        if project {
            d.command = format!("cd {repo} && cargo mock {usage}");
            d.place = vec![repo.to_string()];
            if !kind.is_empty() {
                d.tags.push(kind.to_string());
            }
        } else {
            d.tags.push("builtin".to_string());
        }
        docs.push(d);
    }
    docs
}
