//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Publishing: one `cargo publish` per member in dependency order, `deno
//! publish`, and the npm build and publish where the package ships there.
//! Tokens reach the tools through the environment of the one call and never
//! the shell that ran homma.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use homma_api::Version;

use super::gate::Runner;
use super::registry::{self, Registry};

#[derive(Debug)]
pub enum PublishError {
    /// The tool ran and refused; the command line and its log.
    Failed { command: String, log: String },
    Spawn(super::sh::Spawn),
    /// The credential tool gave no token for this registry.
    NoToken(Registry, String),
    /// The registry never served the version within the wait.
    NotServed(Registry, String, Version),
    Unreachable(registry::Unreachable),
    Io(std::io::Error),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishError::Failed { command, log } => write!(f, "`{command}` failed:\n{log}"),
            PublishError::Spawn(e) => write!(f, "{e}"),
            PublishError::NoToken(r, why) => write!(f, "no token for {r}: {why}"),
            PublishError::NotServed(r, p, v) => write!(f, "{r} never served {p} {v}"),
            PublishError::Unreachable(e) => write!(f, "{e}"),
            PublishError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PublishError {}

impl From<super::sh::Spawn> for PublishError {
    fn from(e: super::sh::Spawn) -> Self {
        PublishError::Spawn(e)
    }
}

impl From<std::io::Error> for PublishError {
    fn from(e: std::io::Error) -> Self {
        PublishError::Io(e)
    }
}

/// Where a token comes from: the credential tool with the registry as its
/// argument, or whatever a test hands in.
pub type TokenSource<'a> = &'a dyn Fn(Registry) -> Result<String, String>;

/// How the publish waits for a registry to serve a version: the real one
/// polls the registry, a test answers at once.
pub type Served<'a> = &'a dyn Fn(Registry, &str, &Version) -> Result<bool, registry::Unreachable>;

/// The publishable crates under `root` in dependency order, each with the
/// directory its manifest is in. A crate names another as a dependency by
/// its package name, with or without a `path`.
pub fn crate_order(root: &Path, names: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    let dirs = crate_dirs(root);
    let mut deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in names {
        let Some(dir) = dirs.get(name) else {
            deps.insert(name.clone(), Vec::new());
            continue;
        };
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        let doc: toml::Value = toml::from_str(&text).unwrap_or(toml::Value::Table(Default::default()));
        let mut on = Vec::new();
        for table in ["dependencies", "build-dependencies"] {
            if let Some(t) = doc.get(table).and_then(|d| d.as_table()) {
                for (key, value) in t {
                    let dep_name = value
                        .get("package")
                        .and_then(|p| p.as_str())
                        .unwrap_or(key.as_str())
                        .to_string();
                    if names.contains(&dep_name) && dep_name != *name {
                        on.push(dep_name);
                    }
                }
            }
        }
        deps.insert(name.clone(), on);
    }
    // kahn's, taking the alphabetically first ready crate so the order is
    // stable across runs
    let mut placed: Vec<String> = Vec::new();
    while placed.len() < deps.len() {
        let ready = deps
            .iter()
            .find(|(n, on)| !placed.contains(n) && on.iter().all(|d| placed.contains(d)))
            .map(|(n, _)| n.clone());
        match ready {
            Some(n) => placed.push(n),
            None => {
                let stuck: Vec<&str> =
                    deps.keys().filter(|n| !placed.contains(n)).map(String::as_str).collect();
                return Err(stuck.join(", "));
            },
        }
    }
    Ok(placed
        .into_iter()
        .map(|n| {
            let dir = dirs.get(&n).cloned().unwrap_or_else(|| root.to_path_buf());
            (n, dir)
        })
        .collect())
}

/// Every crate directory under `root`: the root itself and each workspace
/// member, keyed by package name.
fn crate_dirs(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    let read_name = |dir: &Path| -> Option<String> {
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
        let doc: toml::Value = toml::from_str(&text).ok()?;
        doc.get("package")?.get("name")?.as_str().map(str::to_string)
    };
    if let Some(n) = read_name(root) {
        out.insert(n, root.to_path_buf());
    }
    let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let doc: toml::Value = toml::from_str(&text).unwrap_or(toml::Value::Table(Default::default()));
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for m in members.iter().filter_map(|m| m.as_str()) {
        let dirs: Vec<PathBuf> = match m.strip_suffix("/*") {
            Some(parent) => std::fs::read_dir(root.join(parent))
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default(),
            None => vec![root.join(m)],
        };
        for dir in dirs {
            if let Some(n) = read_name(&dir) {
                out.insert(n, dir);
            }
        }
    }
    out
}

/// Publish one crate from `dir` and wait until the registry serves it.
pub fn publish_crate(
    runner: &dyn Runner,
    root: &Path,
    name: &str,
    version: &Version,
    token: TokenSource<'_>,
    served: Served<'_>,
) -> Result<(), PublishError> {
    let t = token(Registry::CratesIo).map_err(|e| PublishError::NoToken(Registry::CratesIo, e))?;
    let out = runner.run(root, "cargo", &["publish", "-p", name, "--locked"], &[("CARGO_REGISTRY_TOKEN", &t)])?;
    if !out.ok() {
        return Err(PublishError::Failed {
            command: out.command_line(),
            log:     out.log(),
        });
    }
    wait_until_served(Registry::CratesIo, name, version, served)
}

/// Publish the deno package at `root` to jsr.
pub fn publish_jsr(
    runner: &dyn Runner,
    root: &Path,
    name: &str,
    version: &Version,
    token: TokenSource<'_>,
    served: Served<'_>,
) -> Result<(), PublishError> {
    let t = token(Registry::Jsr).map_err(|e| PublishError::NoToken(Registry::Jsr, e))?;
    let out = runner.run(root, "deno", &["publish", "--allow-dirty", "--token", &t], &[])?;
    if !out.ok() {
        return Err(PublishError::Failed {
            command: out.command_line(),
            log:     out.log(),
        });
    }
    wait_until_served(Registry::Jsr, name, version, served)
}

/// Build the npm package where `deno.json` has a `build:npm` task, then
/// publish from `npm/` when that directory exists and from the root
/// otherwise, with the token in a user config written for the call.
pub fn publish_npm(
    runner: &dyn Runner,
    root: &Path,
    name: &str,
    version: &Version,
    token: TokenSource<'_>,
    served: Served<'_>,
) -> Result<(), PublishError> {
    if has_deno_task(root, "build:npm") {
        let out = runner.run(root, "deno", &["task", "build:npm"], &[])?;
        if !out.ok() {
            return Err(PublishError::Failed {
                command: out.command_line(),
                log:     out.log(),
            });
        }
    }
    let dir = if root.join("npm/package.json").is_file() { root.join("npm") } else { root.to_path_buf() };
    let t = token(Registry::Npm).map_err(|e| PublishError::NoToken(Registry::Npm, e))?;
    let npmrc = std::env::temp_dir().join(format!("homma-npmrc-{}", std::process::id()));
    std::fs::write(&npmrc, format!("//registry.npmjs.org/:_authToken={t}\n"))?;
    let out = runner.run(&dir, "npm", &["publish", "--access", "public"], &[(
        "NPM_CONFIG_USERCONFIG",
        npmrc.to_str().unwrap_or_default(),
    )]);
    let _ = std::fs::remove_file(&npmrc);
    let out = out?;
    if !out.ok() {
        return Err(PublishError::Failed {
            command: out.command_line(),
            log:     out.log(),
        });
    }
    wait_until_served(Registry::Npm, name, version, served)
}

fn has_deno_task(root: &Path, task: &str) -> bool {
    std::fs::read_to_string(root.join("deno.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|d| d.get("tasks")?.get(task).cloned())
        .is_some()
}

/// Ask `served` until it says yes, for a bounded number of tries.
pub fn wait_until_served(
    registry: Registry,
    package: &str,
    version: &Version,
    served: Served<'_>,
) -> Result<(), PublishError> {
    for attempt in 0 .. 30 {
        if served(registry, package, version).map_err(PublishError::Unreachable)? {
            return Ok(());
        }
        if attempt < 29 {
            std::thread::sleep(poll_interval());
        }
    }
    Err(PublishError::NotServed(registry, package.to_string(), version.clone()))
}

/// The real answer to "is it served yet", off the registry.
pub fn registry_serves(registry: Registry, package: &str, version: &Version) -> Result<bool, registry::Unreachable> {
    Ok(registry::published_versions(registry, package)?.contains(version))
}

#[cfg(not(test))]
fn poll_interval() -> std::time::Duration {
    std::time::Duration::from_secs(10)
}

#[cfg(test)]
fn poll_interval() -> std::time::Duration {
    std::time::Duration::from_millis(1)
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
