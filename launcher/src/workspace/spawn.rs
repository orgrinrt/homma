//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Making a workspace: the content repository cloned on its working trunk,
//! the repositories the settings and the command line name cloned beside it,
//! and a branch switched to in each where one was asked for.
//!
//! Everything comes from the remote. No clone on this machine is read, since
//! whatever the person keeps in their own tree is theirs and may be mid-edit,
//! and a workspace built from it is a workspace built on somebody's working
//! state.

use std::io::Write;
use std::path::Path;

use super::git;
use crate::settings::{CONTENT_REPO, Prefs};

/// Spawn into the cwd, which has to be empty and outside any repository.
pub fn in_place(
    prefs: &Prefs,
    home: Option<&Path>,
    cwd: &Path,
    out: &mut dyn Write,
) -> Result<(), String> {
    preflight(prefs, home, cwd)?;
    if let Some(above) = repository_above(cwd) {
        return Err(format!(
            "{} is inside the repository at {}, and a workspace is not made inside another \
             repository; `homma workspace spawn <slug>` makes one under {}",
            cwd.display(),
            above.display(),
            prefs.workspaces_root.display()
        ));
    }
    let mut entries =
        std::fs::read_dir(cwd).map_err(|e| format!("cannot read {}: {e}", cwd.display()))?;
    if entries.next().is_some() {
        return Err(format!(
            "{} is not empty, and a workspace made in place takes an empty directory",
            cwd.display()
        ));
    }
    spawn(prefs, home, cwd, &[], None, out)
}

/// Spawn under the person's workspaces directory, creating that directory
/// itself where it is absent and refusing a slug already taken.
pub fn under_root(
    prefs: &Prefs,
    home: Option<&Path>,
    slug: &str,
    extra: &[String],
    branch: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), String> {
    let dest = prefs.workspaces_root.join(slug);
    preflight(prefs, home, &dest)?;
    if dest.exists() {
        return Err(format!(
            "{} already exists; `homma workspace reap {slug}` removes it",
            dest.display()
        ));
    }
    spawn(prefs, home, &dest, extra, branch, out)
}

/// The two refusals that come before anything is looked at on disk: no
/// content repository to clone, and a destination the settings deny.
fn preflight(prefs: &Prefs, home: Option<&Path>, dest: &Path) -> Result<(), String> {
    if prefs.content_repo.is_empty() {
        return Err(format!(
            "{CONTENT_REPO} is not set, so there is nothing to clone a workspace from; \
             `homma config set {CONTENT_REPO} <git url>` names it"
        ));
    }
    if let Some(why) = prefs.refusal_for(dest, home)? {
        return Err(why);
    }
    Ok(())
}

/// The common half: the content repository, then the members.
fn spawn(
    prefs: &Prefs,
    home: Option<&Path>,
    dest: &Path,
    extra: &[String],
    branch: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), String> {
    preflight(prefs, home, dest)?;
    git::is_available()?;
    let w = |out: &mut dyn Write, s: String| out.write_all(s.as_bytes()).map_err(|e| e.to_string());

    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    w(out, format!("spawning {}\n", dest.display()))?;
    let trunk = git::trunk_of(&prefs.content_repo)?;
    git::clone(&prefs.content_repo, &trunk, dest)?;
    w(
        out,
        format!(
            "  {}  {trunk}  {}\n",
            name_of(&prefs.content_repo),
            git::short_head(dest)?
        ),
    )?;

    for repo in prefs.repos.iter().chain(extra) {
        let url = url_of(&prefs.content_repo, repo);
        let name = name_of(&url);
        let into = dest.join(&name);
        if into.exists() {
            return Err(format!(
                "{} is already there, so {repo} was named twice or the content repository \
                 carries a directory of that name",
                into.display()
            ));
        }
        let trunk = git::trunk_of(&url)?;
        git::clone(&url, &trunk, &into)?;
        let mut line = format!("  {name}  {trunk}");
        if let Some(b) = branch {
            git::switch_new(&into, b)?;
            line.push_str(&format!(" -> {b}"));
        }
        w(out, format!("{line}  {}\n", git::short_head(&into)?))?;
    }
    w(out, format!("\nwork here: {}\n", dest.display()))?;
    Ok(())
}

/// The repository `dir` or any ancestor is inside, by the `.git` directory,
/// or `None`.
pub fn repository_above(dir: &Path) -> Option<std::path::PathBuf> {
    dir.ancestors()
        .find(|d| git::is_clone(d))
        .map(Path::to_path_buf)
}

/// A repository as `owner/name` becomes a url on the content repository's
/// host, in its spelling: `git@host:owner/name.git` or `https://host/owner/name.git`.
/// Anything carrying a scheme or an `@` is already a url and is left alone.
pub fn url_of(content_repo: &str, repo: &str) -> String {
    if repo.contains("://") || repo.contains('@') || repo.starts_with('/') || repo.starts_with('.')
    {
        return repo.to_owned();
    }
    let with_git = |s: &str| if s.ends_with(".git") { s.to_owned() } else { format!("{s}.git") };
    if let Some((host, _)) = content_repo.split_once(':')
        && host.contains('@')
        && !content_repo.contains("://")
    {
        return with_git(&format!("{host}:{repo}"));
    }
    if let Some((scheme, rest)) = content_repo.split_once("://")
        && let Some((host, _)) = rest.split_once('/')
    {
        return with_git(&format!("{scheme}://{host}/{repo}"));
    }
    with_git(repo)
}

/// The directory a clone of `url` lands in: the last path segment with any
/// `.git` taken off, which is what `git clone` would name it.
pub fn name_of(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_name_takes_the_content_repositorys_host_and_spelling() {
        let ssh = "git@github.com:orgrinrt/clause-dev.git";
        assert_eq!(
            url_of(ssh, "orgrinrt/notko"),
            "git@github.com:orgrinrt/notko.git"
        );
        assert_eq!(
            url_of(ssh, "hiisi-digital/viola.git"),
            "git@github.com:hiisi-digital/viola.git"
        );
        let https = "https://github.com/orgrinrt/clause-dev.git";
        assert_eq!(
            url_of(https, "orgrinrt/notko"),
            "https://github.com/orgrinrt/notko.git"
        );
        let sshurl = "ssh://git@github.com/orgrinrt/clause-dev.git";
        assert_eq!(
            url_of(sshurl, "orgrinrt/notko"),
            "ssh://git@github.com/orgrinrt/notko.git"
        );
        // a full url, a path and a relative path are left alone
        for already in [
            "git@codeberg.org:x/y.git",
            "https://codeberg.org/x/y.git",
            "/srv/git/y.git",
            "../y",
        ] {
            assert_eq!(url_of(ssh, already), already);
        }
        // and a content repository with no host, a local path, gives the
        // short name back with the suffix, which is the honest answer
        assert_eq!(url_of("/srv/git/ws.git", "o/n"), "o/n.git");
    }

    #[test]
    fn the_clone_directory_is_the_last_segment_without_the_suffix() {
        assert_eq!(
            name_of("git@github.com:orgrinrt/clause-dev.git"),
            "clause-dev"
        );
        assert_eq!(name_of("https://github.com/orgrinrt/Loru.git"), "Loru");
        assert_eq!(name_of("https://github.com/orgrinrt/notko"), "notko");
        assert_eq!(name_of("/srv/git/ws.git/"), "ws");
        assert_eq!(name_of("ws"), "ws");
    }
}
