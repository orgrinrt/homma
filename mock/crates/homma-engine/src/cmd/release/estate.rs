//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release estate`: every package the workspace publishes, against
//! its tags and its registry, asked live each time. The logic is
//! `homma_core::release::estate`.

use std::path::Path;

use anyhow::Result;
use homma_core::Config;
use homma_core::release::estate::{self, Row};
use homma_core::release::registry;

use super::{Report, finish};
use crate::cli::{Cli, EstateOp};
use crate::cmd::Outcome;

fn table(rows: &[Row]) -> Vec<String> {
    let mut out = vec![format!(
        "  {:<24}{:<11}{:<14}{:<6}{:<6}{:<9}{:<7}{:<9}dirty",
        "package", "registry", "current", "tags", "live", "missing", "jumps", "bad tags"
    )];
    for r in rows {
        out.push(format!(
            "  {:<24}{:<11}{:<14}{:<6}{:<6}{:<9}{:<7}{:<9}{}",
            r.package.name,
            r.package.registry.key(),
            r.package.version.as_deref().unwrap_or("none"),
            r.tags.len(),
            r.published.len(),
            r.missing.len(),
            r.jumps.len(),
            r.disagreeing.len(),
            r.dirty
        ));
    }
    out
}

pub(crate) fn run(
    cli: &Cli,
    cfg: &Config,
    op: Option<&EstateOp>,
    json: Option<&Path>,
) -> Result<Outcome> {
    let scope = cfg.registry("jsr").and_then(|r| r.scope.as_deref());
    let rows = estate::measure_all(&cfg.workspace.path, scope, registry::holding)?;
    if let Some(path) = json {
        std::fs::write(path, serde_json::to_string_pretty(&rows)?)?;
        return finish(cli, Report {
            ok:    true,
            lines: vec![format!("wrote {} package(s) to {}", rows.len(), path.display())],
        });
    }
    let (ok, lines) = match op {
        None => (true, table(&rows)),
        Some(EstateOp::Jumps) => {
            let lines: Vec<String> = rows
                .iter()
                .filter(|r| !r.jumps.is_empty())
                .map(|r| format!("  {:<22} {}", r.package.name, r.jumps.join("; ")))
                .collect();
            if lines.is_empty() {
                (true, vec!["  no jumps".into()])
            } else {
                (false, lines)
            }
        },
        Some(EstateOp::Missing) => {
            let lines: Vec<String> = rows
                .iter()
                .filter(|r| !r.missing.is_empty())
                .map(|r| format!("  {:<22} {}", r.package.name, r.missing.join(" ")))
                .collect();
            (
                true,
                if lines.is_empty() { vec!["  nothing missing".into()] } else { lines },
            )
        },
    };
    finish(cli, Report {
        ok,
        lines,
    })
}
