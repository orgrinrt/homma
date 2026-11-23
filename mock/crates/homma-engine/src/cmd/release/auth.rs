//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release auth`: the publish credentials, wired to the terminal,
//! the store this machine holds them in, and the live registries. The
//! logic is `homma_core::release::auth`.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use homma_core::Config;
use homma_core::release::auth::flow::{self, Prompt};
use homma_core::release::auth::store::default_store;
use homma_core::release::auth::{ENTRIES, Live, Store, entry};
use homma_core::release::day::Day;

use super::{Report, finish};
use crate::cli::{AuthOp, Cli};
use crate::cmd::Outcome;

/// The terminal: prompts on stderr, so a verb's own stdout stays the
/// answer, and a secret read with echo off where the terminal allows it.
struct Terminal;

impl Terminal {
    fn read_line(&self, ask: &str) -> String {
        eprint!("{ask}");
        let _ = std::io::stderr().flush();
        let mut s = String::new();
        let _ = std::io::stdin().lock().read_line(&mut s);
        s.trim_end_matches(['\r', '\n']).to_string()
    }

    fn stty(arg: &str) -> bool {
        std::process::Command::new("stty")
            .arg(arg)
            .stdin(std::process::Stdio::inherit())
            .status()
            .is_ok_and(|s| s.success())
    }
}

impl Prompt for Terminal {
    fn say(&self, text: &str) {
        eprintln!("{text}");
    }

    fn secret(&self, ask: &str) -> String {
        let hidden = Self::stty("-echo");
        let s = self.read_line(ask);
        if hidden {
            Self::stty("echo");
            eprintln!();
        }
        s
    }

    fn line(&self, ask: &str) -> String {
        self.read_line(ask)
    }

    fn open(&self, url: &str) -> bool {
        let program = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(program)
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn hand_over(&self, argv: &[&str]) -> Result<(), String> {
        let (program, args) = argv.split_first().ok_or("no command")?;
        std::process::Command::new(program)
            .args(args)
            .status()
            .map(|_| ())
            .map_err(|e| format!("{program} did not start: {e}"))
    }
}

/// The store, given each registry's `token_env` for a machine with no
/// keychain.
fn store(cfg: &Config) -> Box<dyn Store> {
    let vars = ENTRIES
        .iter()
        .filter_map(|e| {
            cfg.registry(e.key)
                .and_then(|r| r.token_env.clone())
                .map(|v| (e.key.to_string(), v))
        })
        .collect();
    default_store(vars)
}

/// Where the npmrc goes: `[auth] npmrc`, against the workspace root where
/// relative, else under the cache directory, never inside a repository.
pub(crate) fn npmrc_path(cfg: &Config) -> Result<PathBuf> {
    if let Some(p) = &cfg.auth.npmrc {
        return Ok(if p.is_relative() { cfg.workspace.path.join(p) } else { p.clone() });
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .ok_or_else(|| {
            anyhow!("no cache directory: set `[auth] npmrc`, or XDG_CACHE_HOME or HOME")
        })?;
    Ok(cache.join("homma").join("npmrc"))
}

/// Print one bare answer for a pipe: a token or a path, never wrapped.
fn bare(text: &str) -> Result<Outcome> {
    println!("{text}");
    Ok(Outcome::Ok)
}

fn refused(why: String) -> Result<Outcome> {
    eprintln!("{why}");
    Ok(Outcome::ReportedFailure)
}

pub(crate) fn run(
    cli: &Cli,
    cfg: &Config,
    op: Option<&AuthOp>,
    no_browser: bool,
) -> Result<Outcome> {
    let store = store(cfg);
    let store = store.as_ref();
    let today = Day::today();
    let browser = !no_browser;
    match op {
        None => {
            let ok = flow::all(ENTRIES, store, &Live, &Terminal, today, browser);
            Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
        },
        Some(AuthOp::Status) => {
            let (lines, ok) = flow::status(ENTRIES, store, &Live, today);
            finish(cli, Report {
                ok,
                lines,
            })
        },
        Some(AuthOp::Fix {
            name,
        }) => {
            let Some(e) = entry(name) else {
                return refused(format!("no registry called {name}"));
            };
            let st = flow::fix(e, store, &Live, &Terminal, today, browser);
            Ok(if st.state.needs_work() { Outcome::ReportedFailure } else { Outcome::Ok })
        },
        Some(AuthOp::Forget {
            name,
        }) => {
            match store.forget(name) {
                Ok(gone) => {
                    finish(cli, Report {
                        ok:    true,
                        lines: vec![format!(
                            "{} {name}",
                            if gone { "dropped" } else { "nothing stored for" }
                        )],
                    })
                },
                Err(why) => refused(why.to_string()),
            }
        },
        Some(AuthOp::Expiry {
            name,
            date,
        }) => {
            match flow::expiry(ENTRIES, store, name, date, today) {
                Ok(line) => {
                    finish(cli, Report {
                        ok:    true,
                        lines: vec![line],
                    })
                },
                Err(why) => refused(why),
            }
        },
        Some(AuthOp::Token {
            name,
        }) => {
            match flow::token(ENTRIES, store, name) {
                Ok(t) => bare(&t),
                Err(why) => refused(why),
            }
        },
        Some(AuthOp::Npmrc) => {
            let at = npmrc_path(cfg)?;
            match flow::write_npmrc(&at, store) {
                Ok(()) => bare(&at.display().to_string()),
                Err(why) => refused(why),
            }
        },
        Some(AuthOp::Env) => {
            finish(cli, Report {
                ok:    true,
                lines: flow::env_lines(ENTRIES, store, "homma release auth"),
            })
        },
        Some(AuthOp::Diagnose {
            name,
        }) => {
            match flow::diagnose(ENTRIES, store, &Live, name, today) {
                Ok(lines) => {
                    finish(cli, Report {
                        ok: true,
                        lines,
                    })
                },
                Err(why) => refused(why),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Command, EstateOp, ReleaseOp, VersionsOp};

    fn op(args: &[&str]) -> ReleaseOp {
        let mut argv = vec!["homma-engine", "release"];
        argv.extend_from_slice(args);
        match Cli::try_parse_from(argv)
            .expect("these arguments should parse")
            .command
        {
            Command::Release {
                op,
            } => op,
            _ => panic!("a release command line parsed as something else"),
        }
    }

    #[test]
    fn the_npmrc_is_the_configured_path_against_the_root_or_under_the_cache() {
        let mut cfg = Config::parse(
            "[workspace]\nname = \"w\"\npath = \"/ws\"\n\n[auth]\nnpmrc = \"keep/npmrc\"\n",
        )
        .unwrap();
        assert_eq!(npmrc_path(&cfg).unwrap(), PathBuf::from("/ws/keep/npmrc"));
        cfg.auth.npmrc = None;
        let default = npmrc_path(&cfg).unwrap();
        assert!(default.ends_with("homma/npmrc"), "{}", default.display());
        // a token inside a working tree is one `git add -A` from published
        let here = std::env::current_dir().unwrap();
        assert!(!default.starts_with(&here), "{}", default.display());
    }

    #[test]
    fn every_verb_of_the_python_parses_under_release() {
        assert!(matches!(op(&["auth"]), ReleaseOp::Auth {
            op:         None,
            no_browser: false,
        }));
        assert!(matches!(op(&["auth", "status"]), ReleaseOp::Auth {
            op: Some(AuthOp::Status),
            ..
        }));
        assert!(matches!(
            op(&["auth", "fix", "jsr", "--no-browser"]),
            ReleaseOp::Auth {
                op:         Some(AuthOp::Fix { .. }),
                no_browser: true,
            }
        ));
        for verb in [&["auth", "token", "crates-io"][..], &["auth", "npmrc"], &["auth", "env"]] {
            assert!(
                matches!(op(verb), ReleaseOp::Auth {
                    op: Some(_),
                    ..
                }),
                "{verb:?}"
            );
        }
        assert!(matches!(op(&["versions", "homma"]), ReleaseOp::Versions {
            op: None,
            repo: Some(_),
            all: false,
            ..
        }));
        assert!(matches!(op(&["versions", "--all"]), ReleaseOp::Versions {
            all: true,
            ..
        }));
        assert!(matches!(
            op(&["versions", "plan", "homma"]),
            ReleaseOp::Versions {
                op: Some(VersionsOp::Plan { .. }),
                ..
            }
        ));
        assert!(matches!(
            op(&["versions", "retag", "--apply"]),
            ReleaseOp::Versions {
                op: Some(VersionsOp::Retag {
                    repo:  None,
                    apply: true,
                }),
                ..
            }
        ));
        assert!(matches!(
            op(&["versions", "tag", "homma", "--manifest", "x/Cargo.toml"]),
            ReleaseOp::Versions {
                op: Some(VersionsOp::Tag { .. }),
                manifest: Some(_),
                ..
            }
        ));
        assert!(matches!(op(&["estate"]), ReleaseOp::Estate {
            op:   None,
            json: None,
        }));
        assert!(matches!(op(&["estate", "jumps"]), ReleaseOp::Estate {
            op: Some(EstateOp::Jumps),
            ..
        }));
        assert!(matches!(
            op(&["estate", "--json", "out.json"]),
            ReleaseOp::Estate {
                json: Some(_),
                ..
            }
        ));
    }
}
