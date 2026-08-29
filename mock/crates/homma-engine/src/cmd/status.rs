//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma status`: report the workspace state captured by `homma.toml`.
//!
//! No network calls, no git ops. Just reads the config and summarises what
//! the workspace is. Git working-tree status for individual repos lives
//! under `homma repo status <name>` for now.

use std::io::Write;

use anyhow::Result;
use homma_core::{Config, ForgeKind, Injected};
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Top-level success payload for `homma status`.
#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub workspace: WorkspaceLine,
    pub forges:    Vec<ForgeLine>,
    pub repos:     Vec<RepoLine>,
    /// What the workspace's own tools said, in the order `[[status.inject]]`
    /// declares them. Empty where the manifest declares none, which is most of
    /// them.
    pub injected:  Vec<Injected>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceLine {
    pub name:                   String,
    pub path:                   String,
    pub default_public_branch:  String,
    pub default_working_branch: String,
}

#[derive(Debug, Serialize)]
pub struct ForgeLine {
    pub name:      String,
    pub kind:      String,
    pub base_url:  String,
    pub api_url:   String,
    pub token_env: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RepoLine {
    pub name:           String,
    /// Absent where the clone's remote names no configured forge, which is a
    /// real state rather than a gap: a member with a local remote has none.
    pub forge:          Option<String>,
    /// Absent on the same terms.
    pub owner:          Option<String>,
    pub local_path:     String,
    pub public_branch:  String,
    pub working_branch: String,
}

pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
    // Run before the report is built rather than inside it, so `build_report`
    // stays a pure function of the config and can be tested without spawning
    // anything. A workspace declaring no injections spawns nothing either way.
    let injected = homma_core::inject::run_all(&cfg.status, &cfg.workspace.path);
    let report = build_report(cfg, injected);
    emit(&report, format)?;
    Ok(())
}

fn build_report(cfg: &Config, injected: Vec<Injected>) -> StatusReport {
    let workspace = WorkspaceLine {
        name:                   cfg.workspace.name.clone(),
        path:                   cfg.workspace.path.display().to_string(),
        default_public_branch:  cfg.defaults.public_branch.clone(),
        default_working_branch: cfg.defaults.working_branch.clone(),
    };
    let forges = cfg
        .forges
        .iter()
        .map(|(name, f)| {
            ForgeLine {
                name:      name.clone(),
                kind:      forge_kind_str(f.kind).to_string(),
                base_url:  f.base_url.clone(),
                api_url:   f.api_url.clone(),
                token_env: f.token_env.clone(),
            }
        })
        .collect();
    let repos = cfg
        .repos
        .iter()
        .map(|(name, r)| {
            RepoLine {
                name:           name.clone(),
                forge:          r.forge.clone(),
                owner:          r.owner.clone(),
                local_path:     r.local_path.display().to_string(),
                public_branch:  cfg.defaults.public_branch.clone(),
                working_branch: cfg.defaults.working_branch.clone(),
            }
        })
        .collect();
    StatusReport {
        workspace,
        forges,
        repos,
        injected,
    }
}

fn forge_kind_str(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::Github => "github",
        ForgeKind::Forgejo => "forgejo",
    }
}

impl HumanRender for StatusReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        writeln!(
            out,
            "workspace: {} ({})",
            self.workspace.name, self.workspace.path
        )?;
        writeln!(
            out,
            "  default public={}  working={}",
            self.workspace.default_public_branch, self.workspace.default_working_branch
        )?;
        if !self.forges.is_empty() {
            writeln!(out)?;
            writeln!(out, "forges:")?;
            for f in &self.forges {
                let tok = f.token_env.as_deref().unwrap_or("<none>");
                writeln!(
                    out,
                    "  {} [{}] {} (api={}, token_env={})",
                    f.name, f.kind, f.base_url, f.api_url, tok
                )?;
            }
        }
        if !self.repos.is_empty() {
            writeln!(out)?;
            writeln!(out, "repos:")?;
            for r in &self.repos {
                // A member with no forge prints as one rather than as a blank
                // where a name should be, so the line says which state it is
                // in instead of leaving the reader to guess at an empty field.
                let on = match (&r.owner, &r.forge) {
                    (Some(owner), Some(forge)) => format!("{owner}/{} on {forge}", r.name),
                    (Some(owner), None) => format!("{owner}/{}, forge unknown", r.name),
                    (None, _) => format!("{}, no forge remote", r.name),
                };
                writeln!(
                    out,
                    "  {} -> {on} (path={}, public={}, working={})",
                    r.name, r.local_path, r.public_branch, r.working_branch
                )?;
            }
        }
        for block in &self.injected {
            writeln!(out)?;
            match &block.failed {
                // The failure goes on the heading rather than under it, so a
                // block with nothing in it does not read as a tool that had
                // nothing to say.
                Some(why) => writeln!(out, "{}: {why}", block.title)?,
                None => {
                    writeln!(out, "{}:", block.title)?;
                    // Indented here rather than by the tool, which does not
                    // know it is being embedded and prints the same either way
                    // when run by hand. A blank line stays blank; two spaces on
                    // it would be trailing whitespace in somebody's terminal.
                    for line in block.text.lines() {
                        if line.is_empty() {
                            writeln!(out)?;
                        } else {
                            writeln!(out, "  {line}")?;
                        }
                    }
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(title: &str, text: &str, failed: Option<&str>) -> Injected {
        Injected {
            title:  title.into(),
            text:   text.into(),
            failed: failed.map(str::to_string),
        }
    }

    fn report(injected: Vec<Injected>) -> StatusReport {
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        build_report(&cfg, injected)
    }

    fn rendered(report: &StatusReport) -> String {
        let mut out = Vec::new();
        report
            .render_human(&mut out)
            .expect("writing to a vec cannot fail");
        String::from_utf8(out).expect("the render is utf-8")
    }

    #[test]
    fn a_block_prints_its_title_and_its_text_indented() {
        let text = rendered(&report(vec![block("context", "450733 of 1000000", None)]));
        assert!(text.contains("context:\n  450733 of 1000000\n"), "{text}");
    }

    #[test]
    fn a_blank_line_inside_a_block_stays_blank() {
        // Two spaces on an otherwise empty line is trailing whitespace in
        // somebody's terminal and in every diff of a captured status.
        let text = rendered(&report(vec![block("t", "a\n\nb", None)]));
        assert!(text.contains("  a\n\n  b\n"), "{text:?}");
    }

    #[test]
    fn a_failed_block_says_so_on_the_heading() {
        // On the heading rather than under it, so a block with nothing in it
        // does not read as a tool that had nothing to say.
        let text = rendered(&report(vec![block("agenda", "", Some("agenda exited 1"))]));
        assert!(text.contains("agenda: agenda exited 1\n"), "{text}");
        assert!(
            !text.contains("agenda:\n"),
            "the empty-body shape must not appear: {text}"
        );
    }

    #[test]
    fn blocks_print_in_the_order_they_arrived() {
        let text = rendered(&report(vec![
            block("first", "1", None),
            block("second", "2", None),
            block("third", "3", None),
        ]));
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("missing {needle} in {text}"))
        };
        assert!(at("first:") < at("second:"));
        assert!(at("second:") < at("third:"));
    }

    #[test]
    fn a_report_with_no_injections_prints_nothing_extra() {
        // The control. Every assertion above would hold against a render that
        // printed blocks unconditionally, and a workspace declaring none is
        // every workspace but one.
        let bare = rendered(&report(vec![]));
        let with = rendered(&report(vec![block("t", "x", None)]));
        assert!(!bare.contains("t:"), "{bare}");
        assert_eq!(with.len() - bare.len(), "\nt:\n  x\n".len(), "{with:?}");
    }

    #[test]
    fn the_blocks_reach_the_json_document_too() {
        let doc = serde_json::to_value(report(vec![block("context", "45%", None)]))
            .expect("the report serialises");
        let blocks = doc["injected"].as_array().expect("an injected array");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["title"], "context");
        assert_eq!(blocks[0]["text"], "45%");
        assert!(
            blocks[0].get("failed").is_none(),
            "a block that worked carries no failure key: {blocks:?}"
        );
    }

    #[test]
    fn a_failed_block_carries_its_reason_into_the_json_too() {
        let doc = serde_json::to_value(report(vec![block("agenda", "", Some("exited 1"))]))
            .expect("the report serialises");
        assert_eq!(doc["injected"][0]["failed"], "exited 1");
    }

    #[test]
    fn building_a_report_spawns_nothing() {
        // `build_report` is a pure function of the config, which is what lets
        // it be tested at all. Injections run in `run`, before it.
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        assert!(build_report(&cfg, Vec::new()).injected.is_empty());
    }
}
