//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a repository is, read off the manifests at its root.

use std::fmt;
use std::path::Path;

use homma_api::RepoKind;

/// Neither manifest is at the root, so there is nothing to gate or release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoManifest(pub String);

impl fmt::Display for NoManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no Cargo.toml or deno.json at {}", self.0)
    }
}

impl std::error::Error for NoManifest {}

/// A `Cargo.toml` makes it a crate, a `deno.json` a deno package, both makes
/// it both, and neither is an error rather than a guess.
pub fn detect(root: &Path) -> Result<RepoKind, NoManifest> {
    let cargo = root.join("Cargo.toml").is_file();
    let deno = root.join("deno.json").is_file();
    match (cargo, deno) {
        (true, true) => Ok(RepoKind::Both),
        (true, false) => Ok(RepoKind::Crate),
        (false, true) => Ok(RepoKind::Deno),
        (false, false) => Err(NoManifest(root.display().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_manifest_names_its_kind_and_both_is_both() {
        let d = tempfile::tempdir().unwrap();
        assert!(detect(d.path()).is_err());
        std::fs::write(d.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(detect(d.path()).unwrap(), RepoKind::Crate);
        std::fs::write(d.path().join("deno.json"), "{}").unwrap();
        assert_eq!(detect(d.path()).unwrap(), RepoKind::Both);
        std::fs::remove_file(d.path().join("Cargo.toml")).unwrap();
        assert_eq!(detect(d.path()).unwrap(), RepoKind::Deno);
    }

    #[test]
    #[ignore = "catalogue: a repo keeping its manifests under mock/ is read as having none; tracked homma-release-reads-a-manifest-under-mock"]
    fn a_manifest_under_mock_names_the_repo_s_kind() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("mock")).unwrap();
        std::fs::write(d.path().join("mock/Cargo.toml"), "[workspace]\n").unwrap();
        assert_eq!(detect(d.path()).unwrap(), RepoKind::Crate);
    }
}
