//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A repository outside the instance's scope is never opened.
//!
//! Measured on the work done, the clones git was asked about, and not on the
//! members that came out: a detector that opened every clone and then dropped
//! the ones out of scope returns the same members and fails these.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;

use homma_api::{AbsPath, CommitIdentity, Git};
use homma_core::config::Config;
use homma_core::repo::GixGit;
use homma_core::scope::Scope;

/// Delegates to the real git and writes down every path it was asked an origin of.
#[derive(Default)]
struct Spy {
    asked: RefCell<Vec<PathBuf>>,
}

impl Spy {
    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .asked
            .borrow()
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            .collect();
        v.sort();
        v
    }
}

impl Git for Spy {
    type Error = <GixGit as Git>::Error;

    fn is_repo(&self, path: &AbsPath) -> bool {
        GixGit.is_repo(path)
    }

    fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Self::Error> {
        GixGit.clone_repo(url, dest)
    }

    fn set_identity(&self, path: &AbsPath, id: &CommitIdentity) -> Result<(), Self::Error> {
        GixGit.set_identity(path, id)
    }

    fn init(&self, path: &AbsPath) -> Result<(), Self::Error> {
        GixGit.init(path)
    }

    fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Self::Error> {
        GixGit.enclosing_repo(path)
    }

    fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Self::Error> {
        self.asked.borrow_mut().push(path.as_path().to_path_buf());
        GixGit.origin_url(path)
    }

    fn identity(&self, path: &AbsPath) -> Result<Option<CommitIdentity>, Self::Error> {
        GixGit.identity(path)
    }
}

fn config() -> Config {
    Config::parse("[workspace]\nname = \"demo\"\n").expect("the fixture manifest parses")
}

fn clone_at(root: &Path, name: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let out = Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for n in ["kaski", "homma", "arvo", "notko", "viola"] {
        clone_at(root.path(), n);
    }
    root
}

#[test]
fn a_clone_out_of_scope_is_never_asked_about() {
    let root = workspace();
    let spy = Spy::default();
    let mut cfg = config();
    cfg.detect_members(root.path(), &spy, &Scope::named(["kaski", "homma"]));

    assert_eq!(
        spy.names(),
        ["homma", "kaski"],
        "git was asked only about the scope"
    );
    assert_eq!(cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(), [
        "homma", "kaski"
    ]);
}

#[test]
fn without_a_scope_every_clone_is_asked() {
    // the case that must fail the one above if the filter were absent
    let root = workspace();
    let spy = Spy::default();
    config().detect_members(root.path(), &spy, &Scope::everything());
    assert_eq!(spy.names(), ["arvo", "homma", "kaski", "notko", "viola"]);
}

#[test]
fn a_named_repository_that_is_not_cloned_is_not_a_member_and_not_an_error() {
    let root = workspace();
    let mut cfg = config();
    let scope = Scope::named(["kaski", "letkajenkka"]);
    cfg.detect_members(root.path(), &GixGit, &scope);

    assert_eq!(cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(), [
        "kaski"
    ]);
    assert_eq!(scope.unmet(cfg.repos.keys().map(String::as_str)), [
        "letkajenkka"
    ]);
}

#[test]
fn loading_a_manifest_applies_the_local_file_beside_it() {
    let root = workspace();
    std::fs::write(
        root.path().join("homma.toml"),
        "[workspace]\nname = \"demo\"\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("homma.local.toml"),
        "[instance]\nwork = \"w\"\nrepos = [\"kaski\"]\n",
    )
    .unwrap();

    let cfg = Config::from_path(&root.path().join("homma.toml")).unwrap();
    assert_eq!(cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(), [
        "kaski"
    ]);
}

#[test]
fn a_broken_local_file_loads_everything() {
    let root = workspace();
    std::fs::write(
        root.path().join("homma.toml"),
        "[workspace]\nname = \"demo\"\n",
    )
    .unwrap();
    std::fs::write(root.path().join("homma.local.toml"), "[instance\n").unwrap();

    let cfg = Config::from_path(&root.path().join("homma.toml")).unwrap();
    assert_eq!(cfg.repos.len(), 5);
}
