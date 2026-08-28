//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! URL composition for forge-hosted repositories.
//!
//! Pure functions over [`ForgeConfig`]: given the configured `base_url` /
//! `api_url` and an `(owner, name)` pair, produce the canonical clone, web,
//! and API URLs. No HTTP, no client state. Useful for `migrate --dry-run`
//! and for the gix `clone_into` / `mirror_into` call sites.
//!
//! ## Host template resolution
//!
//! The SSH clone URL needs the bare host (`codeberg.org`), not the full
//! `base_url` (`https://codeberg.org`). [`host_of`] strips the scheme and
//! any trailing path. This avoids a separate `ssh_host` field on
//! [`ForgeConfig`] that callers would have to keep in sync with `base_url`.

use crate::config::ForgeConfig;

/// HTTPS clone URL: `{base_url}/{owner}/{name}.git`.
pub fn clone_https(forge: &ForgeConfig, owner: &str, name: &str) -> String {
    format!(
        "{base}/{owner}/{name}.git",
        base = trim_trailing_slash(&forge.base_url),
    )
}

/// SSH clone URL: `git@{host}:{owner}/{name}.git`.
///
/// The host is derived from `forge.base_url` via [`host_of`]. Assumes the
/// SSH endpoint sits at the same host as the HTTP one and at the root of
/// the namespace. Self-hosted installations behind an HTTP path prefix
/// (`https://example.com/gitlab/...`) are not supported by this composer;
/// such installations typically need a separately-configured SSH host
/// anyway, which is out of scope for the v0 abstract layer.
pub fn clone_ssh(forge: &ForgeConfig, owner: &str, name: &str) -> String {
    let host = host_of(&forge.base_url);
    format!("git@{host}:{owner}/{name}.git")
}

/// Web URL for the repo's landing page: `{base_url}/{owner}/{name}`.
pub fn web(forge: &ForgeConfig, owner: &str, name: &str) -> String {
    format!(
        "{base}/{owner}/{name}",
        base = trim_trailing_slash(&forge.base_url),
    )
}

/// API URL for the repo resource: `{api_url}/repos/{owner}/{name}`.
///
/// GitHub (`https://api.github.com`) and Forgejo / Gitea
/// (`https://codeberg.org/api/v1`) both expose the per-repo resource at
/// `/repos/{owner}/{name}`, so a single composer covers both.
pub fn api_repo(forge: &ForgeConfig, owner: &str, name: &str) -> String {
    format!(
        "{api}/repos/{owner}/{name}",
        api = trim_trailing_slash(&forge.api_url),
    )
}

/// API URL for the create-repo endpoint, scoped to the owner namespace.
///
/// - GitHub: `/user/repos` (always; the owner is implied by the token).
/// - Forgejo / Gitea: `/orgs/{owner}/repos` for orgs, `/user/repos` for users.
///   The kind alone cannot distinguish; clients pick at call time.
///
/// This helper returns the api-host prefix only (`{api_url}` trimmed). The
/// concrete client appends the per-kind path because the choice is
/// kind-and-owner-aware. Kept here so all api-path composition lives in one file.
pub fn api_root(forge: &ForgeConfig) -> String {
    trim_trailing_slash(&forge.api_url).to_string()
}

/// Extract the bare host from a URL of shape `scheme://host[:port][/path]`.
///
/// Best-effort string slicing. Returns the input unchanged if no scheme
/// separator is present (already a bare host). Strips trailing path
/// segments and trailing slashes.
///
/// Assumes its input is a forge `base_url` as carried in `homma.toml`, i.e.
/// a `scheme://host` shape. Garbage in produces garbage out; the config
/// layer is the right place to validate URL shape.
///
/// Path-prefixed installations (`https://example.com/gitlab`) have the
/// path prefix dropped. Such installations are rare for Forgejo / Gitea
/// (which typically run at host root) and are not supported by the
/// derived SSH URL anyway; see [`clone_ssh`].
///
/// ```ignore
/// assert_eq!(host_of("https://codeberg.org"), "codeberg.org");
/// assert_eq!(host_of("https://codeberg.org/"), "codeberg.org");
/// assert_eq!(host_of("https://codeberg.org/api/v1"), "codeberg.org");
/// assert_eq!(host_of("codeberg.org"), "codeberg.org");
/// assert_eq!(host_of("https://user:pw@codeberg.org/x"), "codeberg.org");
/// ```
///
/// Userinfo is dropped rather than carried. A host is what this names, and a
/// url that happens to carry credentials would otherwise hand them to whatever
/// reads the result. The token command takes the host as an argument, and an
/// argument is visible in `ps` to every process on the machine.
///
/// The split is on the **last** `@` in the authority, which is where RFC 3986
/// puts the boundary: a `@` inside userinfo is percent-encoded, so the last one
/// is the separator.
pub fn host_of(url: &str) -> &str {
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3 ..],
        None => url,
    };
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    match authority.rfind('@') {
        Some(i) => &authority[i + 1 ..],
        None => authority,
    }
}

/// What a clone's `origin` remote says about where it came from.
///
/// The other direction from the composers above, and detection needs it: a
/// member repository's forge and owner are properties of its remote rather
/// than of anything anybody wrote down, because the remote is what decides
/// where a push actually lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteOrigin {
    /// The bare host, comparable with [`host_of`] over a forge's `base_url`.
    pub host:  String,
    /// The namespace the repository sits in.
    pub owner: String,
    /// The repository's own name, with any `.git` suffix removed.
    pub name:  String,
}

/// Read a clone URL back into its host, owner and name.
///
/// The two spellings git actually writes are both accepted:
/// `https://host/owner/name.git` and `git@host:owner/name.git`. A scheme other
/// than those still parses when it has the same shape, which covers `ssh://`
/// and `git://`, because what is being read is the tail rather than the
/// protocol.
///
/// `None` for anything that does not carry both an owner and a name. That is a
/// real answer rather than a failure: a clone can have a remote that is a local
/// path, and such a repository is still a member of the workspace, with no
/// forge and no owner. Guessing either would send a push somewhere nobody asked
/// for.
pub fn read_origin(url: &str) -> Option<RemoteOrigin> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    // `git@host:owner/name`, which has no `://` and separates the host with a
    // colon. Checked first, because the scheme test below would read the whole
    // thing as a path.
    let (host, path) = match url.find("://") {
        None => {
            let (authority, path) = url.split_once(':')?;
            let host = authority.rsplit('@').next().unwrap_or(authority);
            (host, path)
        },
        Some(i) => {
            let after = &url[i + 3 ..];
            let (authority, path) = after.split_once('/')?;
            let host = authority.rsplit('@').next().unwrap_or(authority);
            (host, path)
        },
    };
    if host.is_empty() {
        return None;
    }
    // The last two segments, so a namespace nested deeper than one level still
    // yields the owner directly above the repository.
    let path = path.trim_matches('/');
    let mut segments = path.rsplit('/');
    let last = segments.next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    let owner = segments.next()?;
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(RemoteOrigin {
        host:  host.to_string(),
        owner: owner.to_string(),
        name:  name.to_string(),
    })
}

fn trim_trailing_slash(s: &str) -> &str {
    s.strip_suffix('/').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ForgeKind;

    #[test]
    fn a_remote_reads_back_into_a_host_an_owner_and_a_name() {
        // Both spellings git writes, with and without the suffix, plus the two
        // schemes that share the https shape.
        for url in [
            "https://github.com/orgrinrt/notko.git",
            "https://github.com/orgrinrt/notko",
            "git@github.com:orgrinrt/notko.git",
            "git@github.com:orgrinrt/notko",
            "ssh://git@github.com/orgrinrt/notko.git",
            "  https://github.com/orgrinrt/notko.git  ",
        ] {
            let got = read_origin(url).unwrap_or_else(|| panic!("did not parse: {url}"));
            assert_eq!(got.host, "github.com", "{url}");
            assert_eq!(got.owner, "orgrinrt", "{url}");
            assert_eq!(got.name, "notko", "{url}");
        }
    }

    #[test]
    fn a_deeper_namespace_gives_the_owner_directly_above_the_repository() {
        let got = read_origin("https://codeberg.org/a/b/c.git").expect("parses");
        assert_eq!((got.owner.as_str(), got.name.as_str()), ("b", "c"));
    }

    #[test]
    fn a_remote_that_is_not_a_forge_url_reads_as_nothing_rather_than_as_a_guess() {
        // Each of these is a real thing a clone's origin can be, and none of
        // them names an owner. A workspace member with a local remote is still
        // a member; inventing a forge for it would send a push somewhere
        // nobody asked for.
        for url in [
            "",
            "   ",
            "/srv/git/notko.git",
            "../sibling",
            "https://github.com/notko.git",
            "git@github.com:notko.git",
            "https:///orgrinrt/notko.git",
        ] {
            assert_eq!(read_origin(url), None, "read a forge out of {url:?}");
        }
    }

    #[test]
    fn the_two_directions_agree_on_the_host() {
        // `host_of` reads a forge's base_url and `read_origin` reads a clone's
        // remote, and detection matches one against the other. Two spellings
        // of the same rule is how they would drift.
        let forge = ForgeConfig {
            kind:      ForgeKind::Github,
            base_url:  "https://github.com".into(),
            api_url:   "https://api.github.com".into(),
            token_env: None,
            token_cmd: None,
        };
        let composed = clone_https(&forge, "orgrinrt", "notko");
        let read = read_origin(&composed).expect("what we composed reads back");
        assert_eq!(read.host, host_of(&forge.base_url));
        assert_eq!(
            (read.owner.as_str(), read.name.as_str()),
            ("orgrinrt", "notko")
        );

        let over_ssh = read_origin(&clone_ssh(&forge, "orgrinrt", "notko")).expect("parses");
        assert_eq!(over_ssh, read, "the two clone spellings disagree");
    }

    fn cfg(base: &str, api: &str) -> ForgeConfig {
        ForgeConfig {
            kind:      ForgeKind::Forgejo,
            base_url:  base.into(),
            api_url:   api.into(),
            token_env: None,
            token_cmd: None,
        }
    }

    #[test]
    fn host_of_strips_scheme_and_path() {
        assert_eq!(host_of("https://codeberg.org"), "codeberg.org");
        assert_eq!(host_of("https://codeberg.org/"), "codeberg.org");
        assert_eq!(host_of("https://codeberg.org/api/v1"), "codeberg.org");
        assert_eq!(host_of("http://localhost:3000"), "localhost:3000");
        assert_eq!(host_of("codeberg.org"), "codeberg.org");
    }

    #[test]
    fn host_of_drops_userinfo_rather_than_handing_it_on() {
        // The token command takes the host as an argument, and an argument is
        // in `ps` for every process on the machine. A url carrying credentials
        // must not reach it.
        assert_eq!(host_of("https://user:pw@codeberg.org"), "codeberg.org");
        assert_eq!(
            host_of("https://user:pw@codeberg.org/api/v1"),
            "codeberg.org"
        );
        assert_eq!(host_of("https://user@codeberg.org"), "codeberg.org");
        assert_eq!(host_of("https://user:pw@localhost:3000"), "localhost:3000");
        // The last `@` is the separator, since one inside userinfo is
        // percent-encoded. A naive split on the first would keep half a
        // password in the host.
        assert_eq!(host_of("https://us%40er:pw@codeberg.org"), "codeberg.org");
        // Without a scheme it is still an authority and still stripped.
        assert_eq!(host_of("user:pw@codeberg.org/x"), "codeberg.org");
        // And a path that contains an `@` is not userinfo.
        assert_eq!(host_of("https://codeberg.org/a@b"), "codeberg.org");
    }

    #[test]
    fn clone_https_uses_base_url() {
        let f = cfg("https://codeberg.org", "https://codeberg.org/api/v1");
        assert_eq!(
            clone_https(&f, "orgrinrt", "homma"),
            "https://codeberg.org/orgrinrt/homma.git"
        );
    }

    #[test]
    fn clone_https_tolerates_trailing_slash() {
        let f = cfg("https://codeberg.org/", "https://codeberg.org/api/v1");
        assert_eq!(
            clone_https(&f, "orgrinrt", "homma"),
            "https://codeberg.org/orgrinrt/homma.git"
        );
    }

    #[test]
    fn clone_ssh_strips_scheme_to_host() {
        let f = cfg("https://codeberg.org", "https://codeberg.org/api/v1");
        assert_eq!(
            clone_ssh(&f, "orgrinrt", "homma"),
            "git@codeberg.org:orgrinrt/homma.git"
        );
    }

    #[test]
    fn web_url_omits_dot_git() {
        let f = cfg("https://codeberg.org", "https://codeberg.org/api/v1");
        assert_eq!(
            web(&f, "orgrinrt", "homma"),
            "https://codeberg.org/orgrinrt/homma"
        );
    }

    #[test]
    fn api_repo_appends_repos_segment() {
        let f = cfg("https://codeberg.org", "https://codeberg.org/api/v1");
        assert_eq!(
            api_repo(&f, "orgrinrt", "homma"),
            "https://codeberg.org/api/v1/repos/orgrinrt/homma"
        );
    }

    #[test]
    fn api_root_trims_trailing_slash() {
        let f = cfg("https://api.github.com/", "https://api.github.com/");
        assert_eq!(api_root(&f), "https://api.github.com");
    }
}
