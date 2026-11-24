//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The summarising pass: a large text read by one isolated call of a small model, and a summary
//! whose every line names the place it summarises.
//!
//! [`summarise`] is what a verb calls, generic over the [`OneShot`] model that reads the text;
//! [`pass`] is the same with the model the manifest's `[summary]` names, or the reason there is no
//! summary. Where there is none the caller prints its own output whole, with that reason as one
//! line, so a summary is never the only way to the output.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use homma_api::SummaryConfig;
use homma_core::Config;
use serde::Serialize;

use crate::cmd::Outcome;

pub mod call;
pub mod parts;

#[cfg(test)]
mod tests;

/// The compaction rules, the first half of every call's system prompt. Their digest is their
/// version, [`rules_version`].
pub const RULES: &str = "\
You compact one text for a reader who will not see it. You have no tools and no other context: \
only these rules, the brief after them, and the text, which arrives as the user message.

Every line of the text starts with its line number and a tab. Write the summary as lines that each \
start with `- ` and then the place the line summarises: the `path:line` or the key the text itself \
gives for it where it gives one, or else `line <n>` with the number of the text's line. Then a \
colon and what is there, in one sentence. Keep every place that bears on the brief; group places \
saying the same thing on one line, naming each place. Leave out what does not bear on the brief. \
Quote names, keys, numbers and commands exactly as the text spells them. Never add what the text \
does not say. Write nothing before the first line or after the last one.

The brief:
";

/// A token, counted as this many bytes. Ordinary text runs nearer four, so this overcounts and errs
/// toward the smaller request.
pub const BYTES_PER_TOKEN: usize = 3;

/// What one call may carry, in tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The rules and the brief together.
    pub system:  usize,
    /// The whole request: the system prompt and the content.
    pub request: usize,
    /// Kept out of the request for the answer.
    pub reserve: usize,
}

impl Budget {
    /// Rules and brief under 15k tokens, a request under 100k, 8k kept for the answer.
    pub const DEFAULT: Self = Self {
        system:  15_000,
        request: 100_000,
        reserve: 8_000,
    };

    /// How many bytes of content fit one call beside a system prompt of `system` bytes.
    #[must_use]
    pub const fn part_bytes(&self, system: usize) -> usize {
        self.request
            .saturating_sub(self.reserve)
            .saturating_mul(BYTES_PER_TOKEN)
            .saturating_sub(system)
    }
}

/// A model asked once: the system prompt and the content in, the answer or why there is none out.
pub trait OneShot {
    /// One call, with nothing of the caller's session in it.
    ///
    /// # Errors
    /// Why there is no answer, in a line.
    fn ask(&self, system: &str, content: &str) -> Result<String, String>;
}

/// What the pass gives back: a summary, or the reason there is none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Pass {
    /// The summary, where there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Why there is no summary, in one line, where there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note:    Option<String>,
}

impl Pass {
    pub(crate) const fn none(note: String) -> Self {
        Self {
            summary: None,
            note:    Some(note),
        }
    }
}

/// The first sixteen hex digits of the rules' BLAKE3 digest.
#[must_use]
pub fn rules_version() -> String {
    blake3::hash(RULES.as_bytes()).to_hex()[.. 16].to_string()
}

/// The first sixteen hex digits of the BLAKE3 digest of `parts`, each followed by a NUL so no two
/// lists of parts run together into the same bytes.
#[must_use]
pub fn version_of(parts: &[&str]) -> String {
    let mut h = blake3::Hasher::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(&[0]);
    }
    h.finalize().to_hex()[.. 16].to_string()
}

/// The `[summary]` table, with its defaults where it is absent.
///
/// # Errors
/// A table that does not read as one.
pub fn settings(cfg: &Config) -> Result<SummaryConfig> {
    Ok(match &cfg.summary {
        Some(v) => v.clone().try_into().context("reading `[summary]`")?,
        None => SummaryConfig::default(),
    })
}

/// The pass with the model `[summary]` names, or the reason there is none where it is off.
#[must_use]
pub fn pass(settings: &SummaryConfig, brief: &str, text: &str) -> Pass {
    pass_with(settings, &call::Command::from(settings), brief, text)
}

/// The pass through `model`, or the reason there is none where `settings` has it off.
#[must_use]
pub fn pass_with<M: OneShot>(settings: &SummaryConfig, model: &M, brief: &str, text: &str) -> Pass {
    if !settings.enabled {
        return Pass::none(
            "summary: off; `enabled = true` under `[summary]` in homma.toml turns it on, and the output is whole"
                .to_string(),
        );
    }
    summarise(model, &Budget::DEFAULT, brief, text)
}

/// Summarise `text` for `brief` through `model`, in parts where it does not fit one call.
#[must_use]
pub fn summarise<M: OneShot>(model: &M, budget: &Budget, brief: &str, text: &str) -> Pass {
    let system = format!("{RULES}{}\n", brief.trim());
    if system.len() > budget.system.saturating_mul(BYTES_PER_TOKEN) {
        return Pass::none(format!(
            "summary: the rules and the brief come to about {} tokens, over the {} a call's rules may take, so nothing was sent and the output is whole",
            system.len() / BYTES_PER_TOKEN,
            budget.system
        ));
    }
    if text.trim().is_empty() {
        return Pass::none(
            "summary: the text is empty, so there was nothing to summarise and no call was made"
                .to_string(),
        );
    }
    let cut = parts::cut(text, budget.part_bytes(system.len()));
    let whole = cut.len();
    let mut read = Vec::with_capacity(whole);
    for (i, part) in cut.iter().enumerate() {
        match model.ask(&system, &part.body).and_then(|a| checked(&a)) {
            Ok(answer) => read.push(answer),
            Err(why) => {
                return Pass::none(format!(
                    "summary: the call for part {} of {whole} failed: {why}; the output is whole",
                    i + 1
                ));
            },
        }
    }
    let mut summary = if whole == 1 {
        read.remove(0)
    } else {
        cut.iter()
            .zip(&read)
            .enumerate()
            .map(|(i, (p, s))| {
                format!(
                    "part {} of {whole}, lines {}-{}\n{s}",
                    i + 1,
                    p.first,
                    p.last
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let placeless = placeless(&summary);
    if placeless > 0 {
        summary.push_str(&format!(
            "\n\n{placeless} summary line(s) above name no place; open the output for those"
        ));
    }
    Pass {
        summary: Some(summary),
        note:    None,
    }
}

/// `answer` trimmed, where it holds a summary: refused where it is empty once trimmed or has no line
/// starting `- `, since taken as a summary it would put nothing in the output's place.
///
/// # Errors
/// Which of the two it was.
pub fn checked(answer: &str) -> Result<String, String> {
    let answer = answer.trim();
    if answer.is_empty() {
        return Err("the answer was empty".to_string());
    }
    if !answer.lines().any(|l| l.trim_start().starts_with("- ")) {
        return Err("the answer had no summary line, none starting `- `".to_string());
    }
    Ok(answer.to_string())
}

/// How many summary lines, those starting `- `, do not begin with a place: `line <n>`, or a word
/// holding a `:`.
#[must_use]
pub fn placeless(summary: &str) -> usize {
    summary
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("- "))
        .filter(|rest| !has_place(rest))
        .count()
}

fn has_place(rest: &str) -> bool {
    let mut words = rest.split_whitespace();
    match words.next() {
        Some("line") => {
            words.next().is_some_and(|n| {
                n.trim_end_matches([':', ',', '.'])
                    .split('-')
                    .all(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            })
        },
        Some(word) => word.contains(':'),
        None => false,
    }
}

/// `homma summarise`: the file or stdin through the pass, the summary printed, or the text whole and
/// the reason on stderr. Exits 0 either way, since the output is there.
///
/// # Errors
/// The text could not be read, or the table does not read.
pub fn run(cfg: &Config, brief: &str, file: Option<&Path>) -> Result<Outcome> {
    let settings = settings(cfg)?;
    let text = match file {
        Some(f) => {
            std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?
        },
        None => std::io::read_to_string(std::io::stdin()).context("reading stdin")?,
    };
    let got = pass(&settings, brief, &text);
    let mut out = std::io::stdout().lock();
    match (&got.summary, &got.note) {
        (Some(s), _) => writeln!(out, "{s}")?,
        (None, note) => {
            out.write_all(text.as_bytes())?;
            if let Some(n) = note {
                eprintln!("{n}");
            }
        },
    }
    Ok(Outcome::Ok)
}
