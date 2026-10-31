//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma milestone`: where the current milestone began, the steps that record
//! it, and the reports over the range since. The logic lives in
//! `homma_core::milestone`; this wires it to the manifest and the terminal.

use std::io::Write;

use anyhow::Result;
use homma_core::milestone::branches::{Branch, branches};
use homma_core::milestone::range::{Partition, StandIn, partition, stand_ins};
use homma_core::milestone::record::{Mark, Row, Source, base};
use homma_core::milestone::steps::{Shown, freeze, mark, pin, show};
use serde::Serialize;

use crate::cli::{Cli, MilestoneOp, OutputFormat};
use crate::cmd::util::{branches_for, resolve_repo};
use crate::cmd::{Outcome, load_config};
use crate::output::{HumanRender, emit};

/// The base: the sha, and whether the record or a tag answered.
#[derive(Debug, Serialize)]
struct BaseReport {
    repo:   String,
    sha:    String,
    source: Source,
}

impl HumanRender for BaseReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        write!(out, "{}", self.sha)
    }
}

/// A mark made; a person is told nothing, a pipe gets the mark.
#[derive(Debug, Serialize)]
struct MarkReport {
    repo: String,
    sha:  String,
    at:   String,
}

impl HumanRender for MarkReport {
    fn render_human(&self, _out: &mut dyn Write) -> std::io::Result<()> {
        Ok(())
    }
}

/// A row written by `freeze` or `pin`; a person is told nothing, a pipe gets
/// the row.
#[derive(Debug, Serialize)]
struct RowReport {
    repo: String,
    row:  Row,
}

impl HumanRender for RowReport {
    fn render_human(&self, _out: &mut dyn Write) -> std::io::Result<()> {
        Ok(())
    }
}

/// The recorded milestones with their ranges.
#[derive(Debug, Serialize)]
struct ShowReport {
    repo:       String,
    milestones: Vec<Shown>,
}

impl HumanRender for ShowReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let blocks: Vec<String> = self.milestones.iter().map(block).collect();
        write!(out, "{}", blocks.join("\n\n"))
    }
}

/// One milestone as lines, each revision named and each unset one left out.
fn block(s: &Shown) -> String {
    let mut lines = vec![format!("milestone {}", s.n)];
    let mut line = |name: &str, value: &Option<String>, date: &Option<String>| {
        if let Some(v) = value {
            let day = date.as_ref().map_or(String::new(), |d| format!("  {d}"));
            lines.push(format!("  {name:<7} {v}{day}"));
        }
    };
    line("base", &s.base, &None);
    line("freeze", &s.freeze, &s.freeze_at);
    line("head", &s.head, &None);
    line("tip", &s.tip, &s.at);
    line("epoch", &s.epoch, &None);
    line("review", &s.review, &None);
    lines.join("\n")
}

#[derive(Debug, Serialize)]
struct PartitionReport {
    repo:       String,
    partitions: Vec<Partition>,
}

impl HumanRender for PartitionReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let rows: Vec<String> = self
            .partitions
            .iter()
            .map(|p| {
                format!(
                    "{:>7}  {} {}  ({} {}, +{} -{})",
                    p.changed(),
                    p.unit,
                    p.kind.word(),
                    p.files,
                    if p.files == 1 { "file" } else { "files" },
                    p.added,
                    p.deleted
                )
            })
            .collect();
        write!(out, "{}", rows.join("\n"))
    }
}

#[derive(Debug, Serialize)]
struct StandInReport {
    repo:      String,
    stand_ins: Vec<StandIn>,
}

impl HumanRender for StandInReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let rows: Vec<String> = self
            .stand_ins
            .iter()
            .map(|s| format!("{}:{}  {}  {}", s.file, s.line, s.marker, s.text))
            .collect();
        write!(out, "{}", rows.join("\n"))
    }
}

#[derive(Debug, Serialize)]
struct BranchReport {
    repo:     String,
    branches: Vec<Branch>,
}

impl HumanRender for BranchReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let rows: Vec<String> = self
            .branches
            .iter()
            .map(|b| {
                let mut row = format!(
                    "{}  {}  {} lines  {} commits",
                    b.name,
                    b.call.word(),
                    b.changed,
                    b.commits
                );
                if !b.conflicts.is_empty() {
                    row.push_str(&format!("  conflicts: {}", b.conflicts.join(", ")));
                }
                if let Some(why) = &b.reason {
                    row.push_str(&format!("  reason: {why}"));
                }
                row
            })
            .collect();
        write!(out, "{}", rows.join("\n"))
    }
}

pub fn run(cli: &Cli, op: &MilestoneOp) -> Result<Outcome> {
    let cfg = load_config(cli)?;
    let repo_of = |name: &str| resolve_repo(&cfg, Some(name));
    match op {
        MilestoneOp::Base {
            repo,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, _) = branches_for(&cfg, &root);
            let (sha, source) = base(&root, trunk)?;
            emit(
                &BaseReport {
                    repo: name.to_string(),
                    sha,
                    source,
                },
                cli.output,
            )?;
        },
        MilestoneOp::Freeze {
            repo,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, _) = branches_for(&cfg, &root);
            let row = freeze(&root, trunk)?;
            if matches!(cli.output, OutputFormat::Json) {
                emit(
                    &RowReport {
                        repo: name.to_string(),
                        row,
                    },
                    cli.output,
                )?;
            }
        },
        MilestoneOp::Pin {
            repo,
            sha,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, _) = branches_for(&cfg, &root);
            let row = pin(&root, trunk, sha.as_deref())?;
            if matches!(cli.output, OutputFormat::Json) {
                emit(
                    &RowReport {
                        repo: name.to_string(),
                        row,
                    },
                    cli.output,
                )?;
            }
        },
        MilestoneOp::Show {
            repo,
            n,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, _) = branches_for(&cfg, &root);
            let milestones = show(&root, trunk, *n)?;
            emit(
                &ShowReport {
                    repo: name.to_string(),
                    milestones,
                },
                cli.output,
            )?;
        },
        MilestoneOp::Mark {
            repo,
            sha,
            tag,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, _) = branches_for(&cfg, &root);
            let Mark {
                sha,
                at,
            } = mark(&root, trunk, sha, *tag)?;
            // quiet on success for a person; a pipe is handed the mark
            if matches!(cli.output, OutputFormat::Json) {
                emit(
                    &MarkReport {
                        repo: name.to_string(),
                        sha,
                        at,
                    },
                    cli.output,
                )?;
            }
        },
        MilestoneOp::Partition {
            repo,
            base,
            head,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let partitions = partition(&root, base, head)?;
            emit(
                &PartitionReport {
                    repo: name.to_string(),
                    partitions,
                },
                cli.output,
            )?;
        },
        MilestoneOp::StandIns {
            repo,
            base,
            head,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let stand_ins = stand_ins(&root, base, head)?;
            emit(
                &StandInReport {
                    repo: name.to_string(),
                    stand_ins,
                },
                cli.output,
            )?;
        },
        MilestoneOp::Branches {
            repo,
        } => {
            let (name, _, root) = repo_of(repo)?;
            let (trunk, release) = branches_for(&cfg, &root);
            let found = branches(&root, trunk, release)?;
            emit(
                &BranchReport {
                    repo:     name.to_string(),
                    branches: found,
                },
                cli.output,
            )?;
        },
    }
    Ok(Outcome::Ok)
}
