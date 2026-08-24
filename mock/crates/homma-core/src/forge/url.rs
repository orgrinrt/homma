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

fn trim_trailing_slash(s: &str) -> &str {
    s.strip_suffix('/').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ForgeKind;

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
