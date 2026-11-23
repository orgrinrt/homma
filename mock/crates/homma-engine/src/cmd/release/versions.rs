//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release versions`: the version timeline, what it owes, and the
//! repairs, which write local tags and push nothing. The logic is
//! `homma_core::release::history`.

use std::path::Path;

use anyhow::{Result, anyhow};
use homma_api::Version;
use homma_core::Config;
use homma_core::release::history::{self, Matched};
use homma_core::release::registry::{self, Registry};
use homma_core::release::{git, packages};

use super::{Report, clock, finish};
use crate::cli::{Cli, VersionsOp};
use crate::cmd::Outcome;
use crate::cmd::util::{branches_for, resolve_repo};

fn trunks<'a>(cfg: &'a Config, root: &Path) -> [&'a str; 2] {
    let (t, r) = branches_for(cfg, root);
    [t, r]
}

fn manifest(root: &Path, named: Option<&str>) -> Result<String> {
    match history::manifest_for(root, named) {
        Ok(Some(m)) => Ok(m),
        Ok(None) => {
            Err(anyhow!(
                "{} has no manifest this understands",
                root.display()
            ))
        },
        Err(several) => {
            Err(anyhow!(
                "{} members carry a version ({}); name one with --manifest",
                several.len(),
                several.join(", ")
            ))
        },
    }
}

/// The timeline, one line per change, `!!` where a step is not clean.
fn show(name: &str, root: &Path, m: &str, trunks: &[&str]) -> Result<Vec<String>> {
    let steps = history::timeline(root, m, trunks)?;
    if steps.is_empty() {
        return Ok(vec![format!("  {name}: no version ever recorded in {m}")]);
    }
    let tags = history::released_at(root, &steps, trunks)?;
    let mut out = vec![format!("\n  {name}  ({m}, {} version changes)", steps.len())];
    for s in &steps {
        let (ok, why) = history::sensible(s.from.as_deref(), &s.to);
        let t = tags
            .get(&s.sha)
            .filter(|t| !t.is_empty())
            .map(|t| t.join(","));
        out.push(format!(
            "   {} {} {} {:9} -> {:9} {:26}{}",
            if ok { "  " } else { "!!" },
            s.date,
            s.sha,
            s.from.as_deref().unwrap_or("(new)"),
            s.to,
            t.unwrap_or_else(|| "untagged".into()),
            if ok { String::new() } else { why }
        ));
    }
    Ok(out)
}

/// What crates.io holds for each crate the repo publishes.
fn registry_report(root: &Path) -> Vec<String> {
    let crates: Vec<(String, String)> = packages::crates(root)
        .into_iter()
        .filter_map(|c| c.version.map(|v| (c.name, v)))
        .collect();
    if crates.is_empty() {
        return Vec::new();
    }
    let mut out = vec![format!("    {} crate(s) publish from here:", crates.len())];
    for (name, want) in crates {
        let line = match registry::holding(Registry::CratesIo, &name) {
            Err(e) => format!("could not ask: {e}"),
            Ok(h) if h.live.is_empty() && h.yanked.is_empty() => {
                "nothing published under this name".into()
            },
            Ok(h) => {
                let held: Vec<String> = h.live.iter().map(Version::to_string).collect();
                if held.contains(&want) {
                    "published".into()
                } else {
                    let tail = held[held.len().saturating_sub(3) ..].join(", ");
                    format!("not published; registry has {tail}")
                }
            },
        };
        out.push(format!("      {name:24} {want:10} {line}"));
    }
    out
}

fn plan(name: &str, root: &Path, m: &str, trunks: &[&str]) -> Result<Vec<String>> {
    let steps = history::timeline(root, m, trunks)?;
    let mut out = vec![format!("\n  {name}")];
    if steps.is_empty() {
        out.push("    nothing to do, no version recorded".into());
        out.extend(registry_report(root));
        return Ok(out);
    }
    let tags = history::released_at(root, &steps, trunks)?;
    let missing: Vec<history::Step> = history::releasable(&steps)
        .into_iter()
        .filter(|s| tags.get(&s.sha).is_none_or(Vec::is_empty))
        .collect();
    out.push(format!(
        "    now at {}",
        steps.last().map(|s| s.to.as_str()).unwrap_or("")
    ));
    if missing.is_empty() {
        out.push("    every version change carries a tag".into());
    } else {
        out.push(format!("    {} version(s) never tagged:", missing.len()));
        for s in &missing {
            out.push(format!(
                "      {:10} would tag {}  ({}  {})",
                s.to, s.sha, s.date, s.subject
            ));
        }
    }
    let flagged: Vec<(&history::Step, String)> = steps
        .iter()
        .filter_map(|s| {
            let (ok, why) = history::sensible(s.from.as_deref(), &s.to);
            (!ok).then_some((s, why))
        })
        .collect();
    if !flagged.is_empty() {
        out.push(format!(
            "    {} step(s) are not a clean increment:",
            flagged.len()
        ));
        for (s, why) in flagged {
            out.push(format!(
                "      {} -> {}  {why}",
                s.from.as_deref().unwrap_or("(new)"),
                s.to
            ));
        }
    }
    out.extend(registry_report(root));
    Ok(out)
}

/// Tag each releasable change no tag released, at its commit, named the way
/// the repo names its tags; nothing is pushed.
fn tag(root: &Path, m: &str, trunks: &[&str]) -> Result<Vec<String>> {
    let steps = history::timeline(root, m, trunks)?;
    let tags = history::released_at(root, &steps, trunks)?;
    let existing = git::tags(root)?;
    let mut out = Vec::new();
    let mut made = Vec::new();
    for s in history::releasable(&steps) {
        if tags.get(&s.sha).is_some_and(|t| !t.is_empty()) {
            continue;
        }
        let Ok(v) = s.to.parse::<Version>() else {
            out.push(format!("    {} is not a version, left alone", s.to));
            continue;
        };
        let name = packages::tag_name(&existing, &v);
        if existing.contains(&name) || made.contains(&name) {
            out.push(format!("    {name} already exists elsewhere, left alone"));
            continue;
        }
        let msg = format!(
            "{}\n\nTagged after the fact, at the commit that set the version.",
            s.to
        );
        match git::tag_annotated(root, &name, &s.sha, &msg) {
            Ok(()) => {
                out.push(format!("    {name} -> {}", s.sha));
                made.push(name);
            },
            Err(e) => out.push(format!("    {name} refused: {e}")),
        }
    }
    if made.is_empty() {
        out.push("    nothing to tag".into());
    } else {
        out.push(format!(
            "    {} tag(s) made here and pushed nowhere; push them through the forge route",
            made.len()
        ));
    }
    Ok(out)
}

fn retag(name: &str, root: &Path, apply: bool, trunks: &[&str]) -> Result<Vec<String>> {
    let found = history::orphans(root, trunks)?;
    if found.is_empty() {
        return Ok(vec![format!("  {name}: every tag is on a trunk")]);
    }
    let (ok, stuck): (Vec<_>, Vec<_>) = found.iter().partition(|o| o.recoverable());
    let ambiguous = ok.iter().filter(|o| o.candidates.len() > 1).count();
    let mut out = vec![format!(
        "  {name}: {} off every trunk, {} recoverable{}{}",
        found.len(),
        ok.len(),
        if ambiguous > 0 { format!(", {ambiguous} ambiguous") } else { String::new() },
        if stuck.is_empty() {
            String::new()
        } else {
            format!(", {} with no match", stuck.len())
        }
    )];
    for o in &stuck {
        let side = if o.side_branch_only() {
            format!(", on {}", o.held_by.join(", "))
        } else {
            String::new()
        };
        out.push(format!(
            "    {:26} tree {} is on no trunk commit{side}",
            o.tag, o.tree
        ));
    }
    let basis = |o: &history::Orphan| {
        match o.matched {
            Matched::Tree => "by tree",
            Matched::Metadata => "by author date and subject",
            Matched::Nothing => "",
        }
    };
    if !apply {
        for o in &ok {
            let extra = if o.candidates.len() > 1 {
                format!("  ({} candidates, refused on apply)", o.candidates.len())
            } else {
                String::new()
            };
            out.push(format!(
                "    {:26} {} -> {} {}{extra}",
                o.tag,
                o.old_sha,
                o.candidates.join(", "),
                basis(o)
            ));
        }
        return Ok(out);
    }
    let now = clock::now();
    for o in &ok {
        match history::retarget(root, o, &now) {
            Ok(to) => out.push(format!("    {:26} -> {to} {}", o.tag, basis(o))),
            Err(why) => out.push(format!("    {:26} refused: {why}", o.tag)),
        }
    }
    out.push("    moved here and pushed nowhere; push them through the forge route".into());
    Ok(out)
}

pub(crate) fn run(
    cli: &Cli,
    cfg: &Config,
    op: Option<&VersionsOp>,
    repo: Option<&str>,
    all: bool,
    named: Option<&str>,
) -> Result<Outcome> {
    let each = |names: Vec<String>| -> Result<Vec<(String, std::path::PathBuf)>> {
        names
            .into_iter()
            .map(|n| {
                let (_, _, root) = resolve_repo(cfg, Some(&n))?;
                Ok((n, root))
            })
            .collect()
    };
    let one = |r: Option<&str>| -> Result<(String, std::path::PathBuf)> {
        let (n, _, root) = resolve_repo(cfg, r)?;
        Ok((n.to_string(), root))
    };
    let mut lines = Vec::new();
    match op {
        None if all => {
            for (n, root) in each(cfg.repos.keys().cloned().collect())? {
                match manifest(&root, named) {
                    Ok(m) => lines.extend(show(&n, &root, &m, &trunks(cfg, &root))?),
                    Err(_) => lines.push(format!("  {n}: no manifest this understands")),
                }
            }
        },
        None => {
            let (n, root) = one(repo)?;
            lines.extend(show(
                &n,
                &root,
                &manifest(&root, named)?,
                &trunks(cfg, &root),
            )?);
        },
        Some(VersionsOp::Plan {
            repo,
        }) => {
            let (n, root) = one(repo.as_deref())?;
            lines.extend(plan(
                &n,
                &root,
                &manifest(&root, named)?,
                &trunks(cfg, &root),
            )?);
        },
        Some(VersionsOp::Tag {
            repo,
        }) => {
            let (_, root) = one(repo.as_deref())?;
            lines.extend(tag(&root, &manifest(&root, named)?, &trunks(cfg, &root))?);
        },
        Some(VersionsOp::Retag {
            repo,
            apply,
        }) => {
            let targets = match repo {
                Some(r) => vec![one(Some(r))?],
                None => each(cfg.repos.keys().cloned().collect())?,
            };
            for (n, root) in targets {
                lines.extend(retag(&n, &root, *apply, &trunks(cfg, &root))?);
            }
        },
    }
    finish(cli, Report {
        ok: true,
        lines,
    })
}
