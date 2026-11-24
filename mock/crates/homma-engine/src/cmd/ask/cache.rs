//! What `homma ask` paid for, kept as files under the workspace's git directory: a partial per
//! chunk and an answer per question over a set of chunks. A file that cannot be read is a miss; one
//! that cannot be written is said and does not fail the ask.

use std::path::{Path, PathBuf};

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

    /// A chunk's partial, where one is kept for this rules version and model.
    #[must_use]
    pub fn partial(&self, key: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join("partials").join(key)).ok()
    }

    /// The answer kept for `key`, where there is one.
    #[must_use]
    pub fn answer(&self, key: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join("answers").join(key)).ok()
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

fn write(dir: &Path, key: &str, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Written beside and renamed over, so a reader never meets half a file.
    let tmp = dir.join(format!(".{key}.{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, dir.join(key)).map_err(|e| format!("{}: {e}", dir.display()))
}

/// A partial's key: the chunk's digest, the rules' version and the model.
#[must_use]
pub fn partial_key(digest: &str, rules: &str, model: &str) -> String {
    version_of(&[digest, rules, model])
}

/// An answer's key: the question, the rules' version, the model and the chunk digests in order.
#[must_use]
pub fn answer_key(question: &str, rules: &str, model: &str, digests: &[&str]) -> String {
    let mut parts = vec![question, rules, model];
    parts.extend_from_slice(digests);
    version_of(&parts)
}
