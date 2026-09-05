//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a repository is, read off the marker files at its root.

use std::fmt;
use std::path::Path;

use homma_api::{Markers, RepoKind};

/// No declared marker is at the root, so this is a repository the workspace
/// has not been told about, and there is nothing to gate or release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoManifest {
    pub root:    String,
    pub markers: Vec<String>,
}

impl fmt::Display for NoManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no root marker at {}; the workspace declares {} under [markers] in homma.toml",
            self.root,
            self.markers.join(", ")
        )
    }
}

impl std::error::Error for NoManifest {}

/// The kind the root's markers add up to. Every declared file that is present
/// contributes its signal, and the set of those is the kind: cargo, deno, both,
/// or content alone. No marker present is an error rather than a guess.
pub fn detect(root: &Path, markers: &Markers) -> Result<RepoKind, NoManifest> {
    let present = markers
        .iter()
        .filter(|(file, _)| root.join(file).is_file())
        .map(|(_, signal)| signal);
    RepoKind::from_signals(present).ok_or_else(|| {
        NoManifest {
            root:    root.display().to_string(),
            markers: markers.iter().map(|(f, _)| f.to_string()).collect(),
        }
    })
}

#[cfg(test)]
mod tests {
    use homma_api::Signal;

    use super::*;

    #[test]
    fn each_manifest_names_its_kind_and_both_is_both() {
        let d = tempfile::tempdir().unwrap();
        let m = Markers::default();
        assert!(detect(d.path(), &m).is_err());
        std::fs::write(d.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(detect(d.path(), &m).unwrap(), RepoKind::Crate);
        std::fs::write(d.path().join("deno.json"), "{}").unwrap();
        assert_eq!(detect(d.path(), &m).unwrap(), RepoKind::Both);
        std::fs::remove_file(d.path().join("Cargo.toml")).unwrap();
        assert_eq!(detect(d.path(), &m).unwrap(), RepoKind::Deno);
    }

    #[test]
    fn a_declared_content_marker_makes_a_content_repo_and_an_undeclared_file_makes_none() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("polka.toml"), "").unwrap();
        let bare = Markers::default();
        let err = detect(d.path(), &bare).unwrap_err();
        assert_eq!(err.markers, vec![
            "Cargo.toml".to_string(),
            "deno.json".to_string()
        ]);
        assert!(err.to_string().contains("[markers]"), "{err}");
        let m = Markers::new([("polka.toml".to_string(), Signal::Content)]);
        assert_eq!(detect(d.path(), &m).unwrap(), RepoKind::Content);
        // a manifest beside the content marker makes it that manifest's kind
        std::fs::write(d.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(detect(d.path(), &m).unwrap(), RepoKind::Crate);
    }

    #[test]
    fn a_directory_named_like_a_marker_is_not_one() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("Cargo.toml")).unwrap();
        assert!(detect(d.path(), &Markers::default()).is_err());
    }

    #[test]
    #[ignore = "catalogue: a repo keeping its manifests under mock/ is read as having none; tracked homma-release-reads-a-manifest-under-mock"]
    fn a_manifest_under_mock_names_the_repo_s_kind() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("mock")).unwrap();
        std::fs::write(d.path().join("mock/Cargo.toml"), "[workspace]\n").unwrap();
        assert_eq!(
            detect(d.path(), &Markers::default()).unwrap(),
            RepoKind::Crate
        );
    }
}
