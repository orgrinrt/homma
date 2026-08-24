//! `homma verify`: sanity-check `homma.toml` and the local environment.
//!
//! Checks performed:
//!
//! - Workspace root path exists (when configured to anything other than `.`).
//! - Each repo references a forge that is declared in `[forges.*]`.
//! - Each forge `token_env`, when set, resolves to a non-empty value.
//! - With `--forge`, each repo exists on its forge under the owner and name
//!   the manifest gives it. Off by default: it is a round-trip per repo where
//!   everything else is offline.
//!
//! A repo whose `local_path` is absent is **not** a finding. A workspace clones
//! the repos its work touches and leaves the rest, so most of the manifest is
//! absent from any given one and reporting that buries the findings that mean
//! something.
//!
//! Exits 0 with `ok: true` and an empty findings list when every check
//! passes, 1 with the failing findings otherwise. JSON mode emits the full
//! report regardless of pass/fail; the exit code is the contract for
//! pass/fail tooling consumers.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use homma_core::Config;
use homma_core::config::ForgeConfig;
use homma_core::forge::Forge;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::Outcome;
use crate::output::{HumanRender, emit};

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok:       bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub level:   Level,
    pub kind:    String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Warn,
    Error,
}

pub fn run(cfg: &Config, forge: bool, format: OutputFormat) -> Result<Outcome> {
    let mut report = check(cfg);
    if forge {
        report.findings.extend(resolve_on_their_forges(cfg));
        report.ok = !report
            .findings
            .iter()
            .any(|f| matches!(f.level, Level::Error));
    }
    let ok = report.ok;
    emit(&report, format)?;
    Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
}

/// Ask each forge whether the repo is there under the owner and name the
/// manifest gives it.
///
/// This is the only check that sees a wrong `owner`. The field builds both the
/// API path and the clone URL, so a repo recorded under the wrong organisation
/// parses, verifies, and then 404s on the first forge operation, which may be
/// months later. Four entries in this workspace's own manifest sat wrong long
/// enough for the rules table to grow the same four errors, and neither caught
/// the other.
///
/// Separate from [`check`], and behind a flag, because that function is offline
/// and pure and is worth keeping so.
fn resolve_on_their_forges(cfg: &Config) -> Vec<Finding> {
    resolve_with(cfg, &|fc| crate::cmd::forge::client_from_config(fc))
}

/// The body, over a factory rather than a hardwired client.
///
/// A parameter so the branches below can be driven from a test. Constructing
/// the client inline left the two that matter, a repo reported absent and a
/// credential the forge rejects, reachable only against the live API, which
/// means in practice never.
fn resolve_with(cfg: &Config, make: &dyn Fn(&ForgeConfig) -> Box<dyn Forge>) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Only forges some repo actually names. A profile the manifest declares and
    // nothing references is not a gap: warning about its token would reproduce,
    // on another axis, the noise the `local_path` warning was removed for.
    let mut in_use: Vec<&String> = cfg.repos.values().map(|r| &r.forge).collect();
    in_use.sort();
    in_use.dedup();

    // Which forges can be believed. A forge is usable only when its credential
    // is one the forge accepts, and every reason it might not be gets its own
    // finding, because they are different things to go and fix.
    let mut usable: Vec<&String> = Vec::new();
    for forge in in_use {
        let Some(forge_cfg) = cfg.forges.get(forge) else {
            // `check` already reported this as `repo_forge_undeclared`.
            continue;
        };
        if !has_a_token(forge_cfg) {
            findings.push(Finding {
                level:   Level::Warn,
                kind:    "forge_answers_are_not_evidence".into(),
                message: format!(
                    "no credential for forge `{forge}`: {}. A private repo there is \
                     indistinguishable from one that does not exist, so its repos were not \
                     asked about",
                    where_it_looked(forge_cfg)
                ),
            });
            continue;
        }
        // A token being SET is not a token that WORKS. An expired, revoked or
        // under-scoped credential produces exactly the same 404 as no
        // credential at all, so without this probe a bad token turns every
        // private repo into a reported absence and the report reads as a
        // manifest defect.
        match make(forge_cfg).credential_works() {
            Ok(true) => usable.push(forge),
            Ok(false) => {
                findings.push(Finding {
                    level:   Level::Warn,
                    kind:    "forge_credential_rejected".into(),
                    message: format!(
                        "forge `{forge}` rejected the token in {}; a private repo there is \
                     indistinguishable from one that does not exist, so its repos were not \
                     asked about",
                        forge_cfg.token_env.as_deref().unwrap_or("the environment")
                    ),
                })
            },
            Err(e) => {
                findings.push(Finding {
                    level:   Level::Warn,
                    kind:    "forge_unreachable".into(),
                    message: format!(
                        "could not check the token for forge `{forge}`: {e}; its repos were not \
                     asked about"
                    ),
                })
            },
        }
    }

    for (name, repo) in &cfg.repos {
        if !usable.contains(&&repo.forge) {
            continue;
        }
        let Some(forge_cfg) = cfg.forges.get(&repo.forge) else {
            continue;
        };
        match make(forge_cfg).repo_exists(&repo.owner, name) {
            Ok(true) => {},
            Ok(false) => {
                findings.push(Finding {
                    level:   Level::Error,
                    kind:    "repo_not_on_forge".into(),
                    message: format!(
                        "repo `{name}` is not at {}/{name} on forge `{}`; every forge \
                         operation against it will fail",
                        repo.owner, repo.forge
                    ),
                });
            },
            Err(e) => {
                // A network problem is not evidence about the repo, so it is a
                // warning rather than a verdict on the manifest.
                findings.push(Finding {
                    level:   Level::Warn,
                    kind:    "repo_forge_unreachable".into(),
                    message: format!(
                        "could not ask forge `{}` about {}/{name}: {e}",
                        repo.forge, repo.owner
                    ),
                });
            },
        }
    }
    findings
}

pub(crate) fn check(cfg: &Config) -> VerifyReport {
    let mut findings = Vec::new();

    let workspace_root = &cfg.workspace.path;
    if workspace_root != Path::new(".") && !workspace_root.exists() {
        findings.push(Finding {
            level:   Level::Error,
            kind:    "workspace_path_missing".into(),
            message: format!(
                "workspace.path = {} does not exist",
                workspace_root.display()
            ),
        });
    }

    for (name, repo) in &cfg.repos {
        if !cfg.forges.contains_key(&repo.forge) {
            findings.push(Finding {
                level:   Level::Error,
                kind:    "repo_forge_undeclared".into(),
                message: format!(
                    "repo `{name}` references forge `{}` which is not declared in [forges.*]",
                    repo.forge
                ),
            });
        }
    }

    for (name, forge) in &cfg.forges {
        if let Some(var) = forge.token_env.as_deref() {
            match std::env::var(var) {
                Ok(v) if !v.is_empty() => {}
                Ok(_) => findings.push(Finding {
                    level: Level::Warn,
                    kind: "forge_token_empty".into(),
                    message: format!(
                        "forge `{name}` token_env={var} is set but empty; mutating ops will fail unauthorized"
                    ),
                }),
                Err(_) => findings.push(Finding {
                    level: Level::Warn,
                    kind: "forge_token_unset".into(),
                    message: format!(
                        "forge `{name}` token_env={var} is not set in the environment; mutating ops will fail unauthorized"
                    ),
                }),
            }
        }
    }

    let ok = !findings.iter().any(|f| matches!(f.level, Level::Error));
    VerifyReport {
        ok,
        findings,
    }
}

impl HumanRender for VerifyReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.ok && self.findings.is_empty() {
            writeln!(out, "OK")?;
            return Ok(());
        }
        writeln!(out, "{}", if self.ok { "OK with warnings" } else { "FAIL" })?;
        for f in &self.findings {
            let tag = match f.level {
                Level::Error => "error",
                Level::Warn => "warn",
            };
            writeln!(out, "  [{tag}] {}: {}", f.kind, f.message)?;
        }
        Ok(())
    }
}

/// Whether a credential can be obtained for this forge at all.
///
/// Both sources, because a forge configured only with a `token_cmd` has a
/// perfectly good credential and reporting it as tokenless would send the
/// operator to set a variable the manifest never asked for.
///
/// A forge that declares neither is treated the same as one whose variable is
/// unset: no credential either way.
/// The sources this forge declares, named in the order they are tried, so a
/// reader is told where to go rather than being told a credential is missing.
fn where_it_looked(forge: &ForgeConfig) -> String {
    let mut tried = Vec::new();
    if let Some(var) = &forge.token_env {
        tried.push(format!("`{var}` is unset or empty"));
    }
    if let Some(argv) = &forge.token_cmd {
        tried.push(format!("`{}` produced none", argv.join(" ")));
    }
    if tried.is_empty() {
        return "it declares neither `token_env` nor `token_cmd`".to_string();
    }
    tried.join(", and ")
}

fn has_a_token(forge: &ForgeConfig) -> bool {
    homma_core::forge::token::resolve(forge).is_some()
}

#[cfg(test)]
mod tests {
    use homma_core::forge::{CreateRepoSpec, ForgeError, RepoMetadata};

    use super::*;

    /// A forge that answers exactly what a test tells it to, so every branch of
    /// [`resolve_with`] is reachable without the network. The two that matter
    /// most, a repo reported absent and a credential the forge rejects, were
    /// previously reachable only against the live API.
    struct Stub {
        credential: Result<bool, ForgeError>,
        exists:     bool,
    }

    impl Forge for Stub {
        fn credential_works(&self) -> Result<bool, ForgeError> {
            match &self.credential {
                Ok(b) => Ok(*b),
                Err(_) => {
                    Err(ForgeError::UnexpectedStatus {
                        status: 500,
                        body:   "stub".into(),
                    })
                },
            }
        }

        fn repo_exists(&self, _owner: &str, _name: &str) -> Result<bool, ForgeError> {
            Ok(self.exists)
        }

        fn fetch_repo(&self, _o: &str, _n: &str) -> Result<RepoMetadata, ForgeError> {
            unreachable!("verify never fetches metadata")
        }

        fn create_repo(&self, _o: &str, _s: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
            unreachable!("verify never creates")
        }

        fn archive_repo(&self, _o: &str, _n: &str) -> Result<(), ForgeError> {
            unreachable!("verify never archives")
        }

        fn delete_repo(&self, _o: &str, _n: &str) -> Result<(), ForgeError> {
            unreachable!("verify never deletes")
        }
    }

    /// A manifest with one repo on `gh`, plus an unused `cb` profile, so the
    /// used-forge filter has something to filter. `token_var` is threaded in so
    /// each test can own a distinct environment variable and the suite stays
    /// safe to run in parallel.
    fn cfg(token_var: &str) -> Config {
        Config::parse(&format!(
            r#"
content_repo = "c"
[workspace]
name = "w"
[forges.gh]
kind = "github"
base_url = "https://example.invalid"
api_url = "https://example.invalid/api"
token_env = "{token_var}"
[forges.cb]
kind = "forgejo"
base_url = "https://other.invalid"
api_url = "https://other.invalid/api"
token_env = "HOMMA_TEST_NEVER_SET"
[repos.somerepo]
forge = "gh"
owner = "someone"
local_path = "somerepo"
"#
        ))
        .unwrap()
    }

    fn run(token_var: &str, stub: Stub) -> Vec<Finding> {
        resolve_with(&cfg(token_var), &|_fc| {
            Box::new(Stub {
                credential: match &stub.credential {
                    Ok(b) => Ok(*b),
                    Err(_) => {
                        Err(ForgeError::UnexpectedStatus {
                            status: 500,
                            body:   "stub".into(),
                        })
                    },
                },
                exists:     stub.exists,
            })
        })
    }

    fn kinds(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|f| f.kind.as_str()).collect()
    }

    #[test]
    fn a_repo_the_forge_does_not_have_is_reported_as_an_error() {
        // The branch the whole flag exists for, and the one nothing reached.
        let var = "HOMMA_TEST_TOKEN_ABSENT_REPO";
        unsafe { std::env::set_var(var, "t") };
        let f = run(var, Stub {
            credential: Ok(true),
            exists:     false,
        });
        assert_eq!(kinds(&f), ["repo_not_on_forge"]);
        assert!(matches!(f[0].level, Level::Error));
        assert!(
            f[0].message.contains("someone/somerepo"),
            "{}",
            f[0].message
        );
    }

    #[test]
    fn a_repo_the_forge_has_produces_nothing() {
        // The control. Without it the test above passes for an implementation
        // that reports every repo absent.
        let var = "HOMMA_TEST_TOKEN_PRESENT_REPO";
        unsafe { std::env::set_var(var, "t") };
        let f = run(var, Stub {
            credential: Ok(true),
            exists:     true,
        });
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn a_credential_the_forge_rejects_stops_the_questions_rather_than_answering_them() {
        // The finding the token-is-set gate could not make. The stub would say
        // the repo is absent, and that answer must not be reported, because an
        // expired or under-scoped token produces exactly the 404 an absent repo
        // does.
        let var = "HOMMA_TEST_TOKEN_REJECTED";
        unsafe { std::env::set_var(var, "a-token-that-does-not-work") };
        let f = run(var, Stub {
            credential: Ok(false),
            exists:     false,
        });
        assert_eq!(kinds(&f), ["forge_credential_rejected"]);
        assert!(matches!(f[0].level, Level::Warn));
        assert!(
            f[0].message.contains(var),
            "names the variable: {}",
            f[0].message
        );
        assert!(
            !kinds(&f).contains(&"repo_not_on_forge"),
            "a rejected credential's 404 was reported as a missing repo"
        );
    }

    #[test]
    fn a_forge_that_cannot_be_reached_stops_the_questions_too() {
        let var = "HOMMA_TEST_TOKEN_UNREACHABLE";
        unsafe { std::env::set_var(var, "t") };
        let f = run(var, Stub {
            credential: Err(ForgeError::UnexpectedStatus {
                status: 500,
                body:   "x".into(),
            }),
            exists:     false,
        });
        assert_eq!(kinds(&f), ["forge_unreachable"]);
        assert!(matches!(f[0].level, Level::Warn));
    }

    #[test]
    fn a_forge_with_no_token_is_warned_about_once_and_its_repos_left_alone() {
        let var = "HOMMA_TEST_TOKEN_NEVER_SET_AT_ALL";
        unsafe { std::env::remove_var(var) };
        let f = run(var, Stub {
            credential: Ok(true),
            exists:     false,
        });
        assert_eq!(kinds(&f), ["forge_answers_are_not_evidence"]);
        assert!(matches!(f[0].level, Level::Warn));
    }

    #[test]
    fn an_empty_token_is_no_token() {
        // The subtler half of the same case: `is_some()` on the variable is
        // true and the credential is worthless.
        let var = "HOMMA_TEST_TOKEN_EMPTY";
        unsafe { std::env::set_var(var, "") };
        let f = run(var, Stub {
            credential: Ok(true),
            exists:     false,
        });
        assert_eq!(kinds(&f), ["forge_answers_are_not_evidence"]);
    }

    #[test]
    fn a_forge_no_repo_uses_is_never_mentioned() {
        // `cb` is declared, has a token_env nothing sets, and no repo names it.
        // Warning about it would reintroduce, on another axis, the noise the
        // `local_path` warning was removed for.
        let var = "HOMMA_TEST_TOKEN_UNUSED_FORGE";
        unsafe { std::env::set_var(var, "t") };
        let f = run(var, Stub {
            credential: Ok(true),
            exists:     true,
        });
        assert!(
            !f.iter().any(|f| f.message.contains("`cb`")),
            "a forge nothing references was reported: {f:?}"
        );
    }
}
