//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release gate`: the gate on the checkout, or on every tip a
//! pre-push hook is handed, each run recorded and its status posted.

use anyhow::{Context, Result, anyhow};
use homma_api::Verdict;
use homma_core::Config;
use homma_core::release::gate::{self, Real};
use homma_core::release::status;

use super::{Outcome, Report, clock, finish, forge_for, record, resolve_repo, store};
use crate::cli::Cli;

pub(super) fn gate_cmd(
    cli: &Cli,
    cfg: &Config,
    repo: Option<&str>,
    sha: Option<&str>,
    hook: bool,
    post: Option<&str>,
) -> Result<Outcome> {
    let (name, r, root) = resolve_repo(cfg, repo)?;
    let root = &root;
    let store = store(cli);
    let (forge, owner) = forge_for(cfg, r)?;
    if let Some(sha) = post {
        let run = record::newest_for(&store, name, sha)?
            .ok_or_else(|| anyhow!("no gate run recorded on {sha}"))?;
        status::post(forge.as_ref(), &owner, name, &run)
            .with_context(|| format!("posting the status on {sha}"))?;
        return finish(cli, Report {
            ok:    true,
            lines: vec![format!(
                "posted {} on {sha}: {}",
                status::CONTEXT,
                status::description(&run)
            )],
        });
    }
    let head = homma_core::release::git::head(root)?;
    // the hook gates every tip being pushed, the head in place and any other
    // in a worktree of its own; a plain run gates the checkout
    let tips: Vec<String> = if hook {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
        let tips = pushed_tips(&text);
        if tips.is_empty() {
            return finish(cli, Report {
                ok:    true,
                lines: vec!["no ref is being pushed; nothing to gate".into()],
            });
        }
        tips
    } else {
        // a commit named by hand is gated the way a pushed tip is: the head
        // in place, any other in a worktree of its own
        match sha {
            Some(want) => {
                let full = homma_core::release::git::sha(root, want)
                    .map_err(|_| anyhow!("`{want}` is not a commit in this repository"))?;
                vec![full]
            },
            None => vec![head],
        }
    };
    let mut lines = Vec::new();
    let mut ok = true;
    for tip in &tips {
        let run = gate::run_gate_at(&Real, root, tip, name, &clock::now())?;
        record::append(&store, &run).context("recording the run")?;
        lines.push(run.summary());
        match status::post(forge.as_ref(), &owner, name, &run) {
            Ok(()) => lines.push(format!("posted {} on {}", status::CONTEXT, &run.sha[.. 7])),
            Err(e) => {
                lines.push(format!(
                    "the status was not posted ({e}); the record is kept and `homma release gate --post {}` posts it",
                    &run.sha[.. 7]
                ))
            },
        }
        ok &= run.verdict == Verdict::Green;
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// The distinct local shas a pre-push hook is handed on stdin, one
/// `<local ref> <local sha> <remote ref> <remote sha>` per line, skipping a
/// deletion, whose local sha is all zeros.
fn pushed_tips(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sha in text.lines().filter_map(|l| l.split_whitespace().nth(1)) {
        if sha.chars().all(|c| c == '0') || out.iter().any(|s| s == sha) {
            continue;
        }
        out.push(sha.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pushed_tips;

    #[test]
    fn the_tips_are_the_distinct_local_shas_and_a_deletion_is_not_one() {
        let text = "refs/heads/dev aaa1 refs/heads/dev bbb1\n\
                    refs/heads/topic ccc1 refs/heads/topic 0000\n\
                    refs/heads/again aaa1 refs/heads/again bbb1\n\
                    refs/heads/gone 0000000000000000000000000000000000000000 refs/heads/gone ddd1\n";
        assert_eq!(pushed_tips(text), vec![
            "aaa1".to_string(),
            "ccc1".to_string()
        ]);
        assert!(pushed_tips("").is_empty());
    }
}
