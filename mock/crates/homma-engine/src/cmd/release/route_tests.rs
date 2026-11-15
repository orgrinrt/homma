//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which way a repo's release writes go: the workspace's forge route for a
//! repo on GitHub where the manifest names one, git and the forge client for
//! every other, and an error where the manifest cannot be read at all.

use std::cell::RefCell;

use homma_core::forge::{CommitStatus, CreateRepoSpec, ForgeError, RepoMetadata};
use homma_core::release::sh;

use super::*;

/// A manifest at `homma.toml` in a scratch directory, holding `text`.
fn manifest(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("homma.toml");
    std::fs::write(&at, text).unwrap();
    (dir, at)
}

const NAMES_THE_ROUTE: &str =
    "[workspace]\nname = \"w\"\n\n[paths]\nforge_route = \".shared/scripts/remote/forge-route\"\n";

#[test]
fn a_manifest_naming_the_route_gives_it_from_the_manifests_directory() {
    let (dir, at) = manifest(NAMES_THE_ROUTE);
    let script = dir.path().join(".shared/scripts/remote/forge-route");
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, "#!/bin/sh\n").unwrap();
    let route = forge_route_of(&at).unwrap().unwrap();
    assert_eq!(route, std::path::absolute(&script).unwrap());
    assert!(route.is_absolute());
}

#[test]
fn a_manifest_naming_no_route_gives_none() {
    let (_dir, at) = manifest("[workspace]\nname = \"w\"\n");
    assert_eq!(forge_route_of(&at).unwrap(), None);
}

#[test]
fn a_manifest_that_does_not_read_as_a_workspaces_is_an_error_and_not_no_route() {
    // Mutation: the read's failure taken as naming no route, which sends the
    // tag down the git push a container's proxy refuses, after `main` moved.
    let (_dir, at) = manifest("[workspace\nname = ");
    assert!(forge_route_of(&at).is_err());
    let (dir, _) = manifest("");
    assert!(forge_route_of(&dir.path().join("nowhere.toml")).is_err());
}

#[test]
fn the_route_is_taken_for_a_github_repo_alone() {
    // Mutation: the route taken whatever the repo's forge, which asks GitHub
    // for a Forgejo repo's tag under a slug read off a Forgejo url.
    let route = Path::new("/ws/route");
    assert_eq!(route_for(ForgeKind::Github, Some(route)), Some(route));
    assert_eq!(route_for(ForgeKind::Forgejo, Some(route)), None);
    assert_eq!(route_for(ForgeKind::Github, None), None);
    assert_eq!(route_for(ForgeKind::Forgejo, None), None);
}

/// A runner that records the programs it is asked to run and answers each done.
#[derive(Default)]
struct Calls(RefCell<Vec<String>>);

impl Runner for Calls {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> std::result::Result<sh::Output, sh::Spawn> {
        self.0
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        Ok(sh::Output {
            program: program.into(),
            args:    args.iter().map(|a| a.to_string()).collect(),
            status:  Some(0),
            stdout:  String::new(),
            stderr:  String::new(),
        })
    }
}

/// A forge client that records the releases it is asked for and is asked
/// nothing else.
#[derive(Default)]
struct Releases(RefCell<Vec<String>>);

impl Forge for Releases {
    fn fetch_repo(&self, _: &str, _: &str) -> std::result::Result<RepoMetadata, ForgeError> {
        unreachable!()
    }

    fn repo_exists(&self, _: &str, _: &str) -> std::result::Result<bool, ForgeError> {
        unreachable!()
    }

    fn create_repo(
        &self,
        _: &str,
        _: &CreateRepoSpec,
    ) -> std::result::Result<RepoMetadata, ForgeError> {
        unreachable!()
    }

    fn archive_repo(&self, _: &str, _: &str) -> std::result::Result<(), ForgeError> {
        unreachable!()
    }

    fn delete_repo(&self, _: &str, _: &str) -> std::result::Result<(), ForgeError> {
        unreachable!()
    }

    fn credential_works(&self) -> std::result::Result<bool, ForgeError> {
        unreachable!()
    }

    fn set_commit_status(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &CommitStatus,
    ) -> std::result::Result<(), ForgeError> {
        unreachable!()
    }

    fn commit_known(&self, _: &str, _: &str, _: &str) -> std::result::Result<bool, ForgeError> {
        unreachable!()
    }

    fn create_release(
        &self,
        owner: &str,
        name: &str,
        tag: &str,
        _: &str,
    ) -> std::result::Result<(), ForgeError> {
        self.0.borrow_mut().push(format!("{owner}/{name} {tag}"));
        Ok(())
    }
}

/// Which of the two the release forge built for `kind` and `route` asks for a
/// release: the runner's calls and the client's releases.
fn ask_a_release(kind: ForgeKind, route: Option<&Path>) -> (Vec<String>, Vec<String>) {
    let runner = Calls::default();
    let client = Releases::default();
    let writes = release_forge(kind, route, &runner, ByClient {
        forge:  &client,
        owner:  "o",
        name:   "x",
        remote: "origin",
    });
    writes
        .release(Path::new("/clone"), "v0.1.1", "## 0.1.1")
        .unwrap();
    drop(writes);
    (runner.0.into_inner(), client.0.into_inner())
}

#[test]
fn a_forgejo_repo_releases_through_its_client_even_where_the_route_is_named() {
    // Mutation: the release forge built from the named route without asking
    // which forge the repo is on.
    let (ran, released) = ask_a_release(ForgeKind::Forgejo, Some(Path::new("/ws/route")));
    assert!(ran.is_empty(), "the route was asked: {ran:?}");
    assert_eq!(released, ["o/x v0.1.1"]);
}

#[test]
fn a_github_repo_releases_through_the_route_where_it_is_named_and_the_client_where_not() {
    let (ran, released) = ask_a_release(ForgeKind::Github, Some(Path::new("/ws/route")));
    assert_eq!(ran, ["/ws/route release /clone v0.1.1"]);
    assert!(
        released.is_empty(),
        "the client made a release: {released:?}"
    );
    let (ran, released) = ask_a_release(ForgeKind::Github, None);
    assert!(ran.is_empty(), "{ran:?}");
    assert_eq!(released, ["o/x v0.1.1"]);
}
