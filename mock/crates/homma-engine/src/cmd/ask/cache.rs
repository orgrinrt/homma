//! What `homma ask` paid for, kept as files under the workspace's git directory: a partial per
//! chunk and an answer per question over a set of chunks. A file that cannot be read is a miss; one
//! that cannot be written is said and does not fail the ask.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::cmd::summary::version_of;

/// The two caches, under one directory.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// The caches under `dir`, made as they are first written.
    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self {
            dir,
        }
    }

    /// `homma/ask` under the git directory of the repository at `root`.
    ///
    /// # Errors
    /// Git's words where `root` is not in a repository.
    pub fn of_workspace(root: &Path) -> Result<Self, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--absolute-git-dir"])
            .output()
            .map_err(|e| format!("git did not start: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let git = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Self::at(Path::new(&git).join("homma/ask")))
    }

    /// The directory the two caches sit under.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A chunk's partial, where one is kept for this rules version and model.
    #[must_use]
    pub fn partial(&self, key: &str) -> Option<String> {
        read(&self.dir.join("partials").join(key))
    }

    /// The answer kept for `key`, where there is one.
    #[must_use]
    pub fn answer(&self, key: &str) -> Option<String> {
        read(&self.dir.join("answers").join(key))
    }

    /// Take out every entry last read more than `expire` before `now`, then the
    /// least recently read until the whole is at most `max_bytes`.
    ///
    /// Silent: an entry that cannot be read or removed is left where it is, and
    /// the ask goes on.
    pub fn sweep(&self, now: SystemTime, expire: Duration, max_bytes: u64) {
        let mut kept: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for sub in ["partials", "answers"] {
            let Ok(entries) = std::fs::read_dir(self.dir.join(sub)) else {
                continue;
            };
            for e in entries.flatten() {
                let Ok(meta) = e.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let Ok(read) = meta.modified() else { continue };
                if now.duration_since(read).is_ok_and(|age| age > expire) {
                    let _ = std::fs::remove_file(e.path());
                } else {
                    kept.push((read, meta.len(), e.path()));
                }
            }
        }
        let mut total: u64 = kept.iter().map(|k| k.1).sum();
        kept.sort();
        for (_, len, path) in kept {
            if total <= max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }

    /// Keep a chunk's partial.
    ///
    /// # Errors
    /// Why it could not be written.
    pub fn keep_partial(&self, key: &str, text: &str) -> Result<(), String> {
        write(&self.dir.join("partials"), key, text)
    }

    /// Keep an answer.
    ///
    /// # Errors
    /// Why it could not be written.
    pub fn keep_answer(&self, key: &str, text: &str) -> Result<(), String> {
        write(&self.dir.join("answers"), key, text)
    }
}

/// A cache file's text, its modification time stamped now as the moment it was
/// last read, which is what the sweep expires and orders by. A stamp that fails
/// leaves the hit a hit.
fn read(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if let Ok(f) = std::fs::File::options().append(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
    Some(text)
}

fn write(dir: &Path, key: &str, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Written beside and renamed over, so a reader never meets half a file.
    let tmp = dir.join(format!(".{key}.{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, dir.join(key)).map_err(|e| format!("{}: {e}", dir.display()))
}

/// What answers: the command `[summary]` names and the model it asks that command for. Two commands
/// asked for the same model can answer differently, so both are in every key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asker<'a> {
    pub command: &'a str,
    pub model:   &'a str,
}

/// A partial's key: the chunk's digest, the rules' version, the command and the model.
#[must_use]
pub fn partial_key(digest: &str, rules: &str, asker: Asker<'_>) -> String {
    version_of(&[digest, rules, asker.command, asker.model])
}

/// An answer's key: the question, the rules' version, the command, the model and the chunk digests
/// in order.
#[must_use]
pub fn answer_key(question: &str, rules: &str, asker: Asker<'_>, digests: &[&str]) -> String {
    let mut parts = vec![question, rules, asker.command, asker.model];
    parts.extend_from_slice(digests);
    version_of(&parts)
}
