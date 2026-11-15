//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which way a repo's release writes go: the workspace's forge route for a
//! repo on GitHub where the manifest names one, git and the forge client for
//! every other, and an error where the manifest cannot be read at all.

use super::*;

/// A manifest at `homma.toml` in a scratch directory, holding `text`.
fn manifest(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("homma.toml");
    std::fs::write(&at, text).unwrap();
    (dir, at)
}

#[test]
fn a_manifest_naming_the_route_gives_it_from_the_manifests_directory() {
    let (dir, at) = manifest(
        "[workspace]\nname = \"w\"\n\n[paths]\nforge_route = \".shared/scripts/remote/forge-route\"\n",
    );
    let route = forge_route_of(&at).unwrap().unwrap();
    assert_eq!(
        route,
        std::path::absolute(dir.path().join(".shared/scripts/remote/forge-route")).unwrap()
    );
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
