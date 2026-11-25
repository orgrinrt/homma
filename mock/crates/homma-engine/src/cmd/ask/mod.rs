//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `<command> | homma ask <question>`: the output piped in read through the summarising pass with
//! the question, and only a short summary framed by the question and the answer printed, every line
//! naming its place. The ask runs no command: the command is the caller's, under the session's own
//! guards.
//!
//! The output is cut into chunks by
//! content ([`chunks`]); a question-neutral partial is kept per chunk and an answer per
//! question over a set of chunks ([`cache`]), so unchanged chunks are not read again.

use std::io::{IsTerminal, Read};
use std::path::Path;

use anyhow::{Context, Result};
use homma_api::AskConfig;
use homma_core::Config;

use crate::cmd::Outcome;
use crate::cmd::summary::{self, Budget, OneShot, call, version_of};

pub mod cache;
pub mod chunks;

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;

use cache::{Asker, Cache};
use chunks::Chunk;

/// The rules of a call that makes partials: one question-neutral section per chunk.
pub const PARTIAL_RULES: &str = "\
You compact text for a reader who will not see it. You have no tools and no other context: only \
these rules and the text, which arrives as the user message.

The text is a run of chunks. Each starts with a line `=== chunk <i> ===`, and every line of a chunk \
starts with its line number within that chunk and a tab. For every chunk, in order, write the same \
header line `=== chunk <i> ===` and under it lines that each start with `- `, then the place the line \
summarises: the `path:line` or key the chunk itself gives where it gives one, or else `line <n>` with \
the chunk's own number of that line. Then a colon and what is there, in one sentence. Summarise \
everything a later question might need, since you do not know the question: keep names, keys, \
numbers and commands exactly as written. Never add what the text does not say. Write a section for \
every chunk, even one that says little, and nothing outside the sections.
";

/// The rules of the call that answers: a summary framed by the question, then the answer.
pub const ANSWER_RULES: &str = "\
You answer one question about a text the reader will not see. You have no tools and no other \
context: only these rules, the question after them, and the text, which arrives as the user message.

The text is a summary of a command's output, each line starting with `- ` and the place it \
summarises. Write first a short summary of what in the text bears on the question, as lines that \
each start with `- ` and the place, copied exactly from the text's lines, then a colon and what is \
there. Leave out what does not bear on the question. Then a line `Answer:` and the answer in a few \
sentences, naming in it the places it rests on. Where the text does not answer the question, say so \
and say what it holds instead. Never add what the text does not say.

The question:
";

/// The version of everything that shapes an answer, which both caches key on.
#[must_use]
pub fn rules_version() -> String {
    version_of(&[&summary::rules_version(), PARTIAL_RULES, ANSWER_RULES])
}

/// An answer and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub answer: String,
    pub chunks: usize,
    /// How many partials were read from the cache rather than made.
    pub cached: usize,
    /// Whether the whole answer was.
    pub whole:  bool,
    /// What could not be kept, one line each.
    pub notes:  Vec<String>,
}

/// Answer `question` over `output` through `model`, keeping and reusing what it pays for in
/// `cache`, keyed with `asker`.
///
/// # Errors
/// Why there is no answer: the output is empty or cannot be cut, or a call failed or left a chunk
/// out.
pub fn answer<M: OneShot>(
    model: &M,
    asker: Asker<'_>,
    cache: &Cache,
    question: &str,
    output: &str,
) -> Result<Asked, String> {
    let chunks = chunks::cut(output);
    if output.trim().is_empty() || chunks.is_empty() {
        return Err(
            "the input was empty, so there was nothing to answer from and no call was made"
                .to_string(),
        );
    }
    let rules = rules_version();
    // Each chunk's first line beside its digest: the answer cites output lines, so the same chunks
    // on other lines are another answer.
    let firsts: Vec<String> = chunks.iter().map(|c| c.first.to_string()).collect();
    let placed_digests: Vec<&str> = chunks
        .iter()
        .zip(&firsts)
        .flat_map(|(c, f)| [c.digest.as_str(), f.as_str()])
        .collect();
    let akey = cache::answer_key(question, &rules, asker, &placed_digests);
    if let Some(answer) = cache.answer(&akey) {
        return Ok(Asked {
            answer,
            chunks: chunks.len(),
            cached: chunks.len(),
            whole: true,
            notes: Vec::new(),
        });
    }
    let mut notes = Vec::new();
    let keys: Vec<String> = chunks
        .iter()
        .map(|c| cache::partial_key(&c.digest, &rules, asker))
        .collect();
    let mut partials: Vec<Option<String>> = keys.iter().map(|k| cache.partial(k)).collect();
    let cached = partials.iter().filter(|p| p.is_some()).count();
    let missing: Vec<usize> = (0 .. chunks.len())
        .filter(|i| partials[*i].is_none())
        .collect();
    for batch in batches(&chunks, &missing) {
        let (read, left_out) = read_batch(model, &chunks, &batch)?;
        // What came back is kept even where a section is missing, so a retry pays only for the rest.
        for (i, text) in read {
            if let Err(e) = cache.keep_partial(&keys[i], &text) {
                notes.push(format!("a partial was not kept: {e}"));
            }
            partials[i] = Some(text);
        }
        if let Some(i) = left_out {
            return Err(format!(
                "the call making partials gave no section for chunk {} (lines {}-{})",
                i + 1,
                chunks[i].first,
                chunks[i].last
            ));
        }
    }
    let placed: String = chunks
        .iter()
        .zip(&partials)
        .map(|(c, p)| chunks::moved(p.as_deref().unwrap_or(""), c.first.saturating_sub(1)))
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!("{ANSWER_RULES}{}\n", question.trim());
    let budget = Budget::DEFAULT;
    let input = if placed.len() <= budget.part_bytes(system.len()) {
        placed
    } else {
        let brief = format!(
            "what bears on the question \"{}\"; every line of the text begins with the place it \
             summarises, and each summary line names that place copied exactly as the text's line \
             gives it, never a line number of this text itself",
            question.trim()
        );
        let pass = summary::summarise(model, &budget, &brief, &placed);
        let summary = pass.summary.ok_or_else(|| {
            format!(
                "the partials did not fit one call, and summarising them failed: {}",
                pass.note.unwrap_or_default()
            )
        })?;
        without_part_headings(&summary)
    };
    let answer = model
        .ask(&system, &input)
        .and_then(|a| summary::checked(&a))
        .and_then(|a| {
            if a.lines().any(|l| l.trim_start().starts_with("Answer:")) {
                Ok(a)
            } else {
                Err("the answer had no line starting `Answer:`".to_string())
            }
        })
        .map_err(|why| format!("the answering call failed: {why}"))?;
    if let Err(e) = cache.keep_answer(&akey, &answer) {
        notes.push(format!("the answer was not kept: {e}"));
    }
    Ok(Asked {
        answer,
        chunks: chunks.len(),
        cached,
        whole: false,
        notes,
    })
}

/// `summary` without the `part <i> of <n>, lines <a>-<b>` headings the summarising pass puts over
/// each part, whose lines count the text it was given and not the output.
fn without_part_headings(summary: &str) -> String {
    summary
        .lines()
        .filter(|l| {
            let l = l.trim();
            !(l.starts_with("part ") && l.contains(" of ") && l.contains(", lines "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The chunk at each index of `missing`, packed in order into batches whose numbered text fits one
/// call beside the partial rules. A chunk too large for a call alone is a batch of its own.
fn batches(chunks: &[Chunk], missing: &[usize]) -> Vec<Vec<usize>> {
    let room = Budget::DEFAULT.part_bytes(PARTIAL_RULES.len());
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut used = 0;
    for &i in missing {
        let size = section(i, &chunks[i]).len();
        match out.last_mut() {
            Some(b) if used + size <= room => {
                b.push(i);
                used += size;
            },
            _ => {
                out.push(vec![i]);
                used = size;
            },
        }
    }
    out
}

fn section(i: usize, chunk: &Chunk) -> String {
    format!("=== chunk {} ===\n{}", i + 1, chunks::numbered(&chunk.text))
}

/// One call making the partials of the chunks in `batch`: each read back from its section, and the
/// first chunk the answer gave no section, where one is missing.
///
/// # Errors
/// Why the call failed.
fn read_batch<M: OneShot>(
    model: &M,
    chunks: &[Chunk],
    batch: &[usize],
) -> Result<(Vec<(usize, String)>, Option<usize>), String> {
    let content: String = batch.iter().map(|&i| section(i, &chunks[i])).collect();
    let said = model
        .ask(PARTIAL_RULES, &content)
        .map_err(|why| format!("a call making partials failed: {why}"))?;
    let sections = sections(&said);
    let mut read = Vec::new();
    let mut left_out = None;
    for &i in batch {
        // A section with no line is as good as none: its chunk would add nothing ever after.
        match sections
            .iter()
            .find(|(n, text)| *n == i + 1 && !text.trim().is_empty())
        {
            Some((_, text)) => read.push((i, text.clone())),
            None => left_out = left_out.or(Some(i)),
        }
    }
    Ok((read, left_out))
}

/// The sections of a partials answer, by chunk number, each the lines under its header.
fn sections(said: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for line in said.lines() {
        let header = line
            .trim()
            .strip_prefix("=== chunk ")
            .and_then(|r| r.strip_suffix(" ==="))
            .and_then(|n| n.trim().parse::<usize>().ok());
        match (header, out.last_mut()) {
            (Some(n), _) => out.push((n, String::new())),
            (None, Some((_, text))) if !line.trim().is_empty() => {
                text.push_str(line.trim_end());
                text.push('\n');
            },
            _ => {},
        }
    }
    out
}

/// `homma ask`: the output on stdin read and the answer printed with a line saying where it came
/// from. Exits 1 with one line and no answer where the pass is off or a call fails.
///
/// # Errors
/// The `[summary]` table does not read, or stdin cannot be read.
pub fn run(cfg: &Config, root: &Path, question: &[String]) -> Result<Outcome> {
    let settings = summary::settings(cfg)?;
    let question = question.join(" ");
    let refuse = |why: String| {
        eprintln!("ask: {why}; no answer");
        Ok(Outcome::ReportedFailure)
    };
    // Swept on every ask, a refused one too, before anything is answered.
    let kept: AskConfig = match &cfg.ask {
        Some(v) => v.clone().try_into().context("reading `[ask]`")?,
        None => AskConfig::default(),
    };
    let cache = Cache::of_workspace(root);
    if let Ok(c) = &cache {
        c.sweep(
            std::time::SystemTime::now(),
            std::time::Duration::from_secs(kept.cache_expire_hours.saturating_mul(3600)),
            kept.cache_max_bytes,
        );
    }
    if !settings.enabled {
        return refuse(
            "the summarising pass is off; `enabled = true` under `[summary]` in homma.toml turns it on"
                .to_string(),
        );
    }
    let stdin = std::io::stdin();
    let Some(output) = read_piped(stdin.is_terminal(), stdin.lock())? else {
        return refuse(
            "nothing was piped to it; give it a command's output, `<command> | homma ask <question>`"
                .to_string(),
        );
    };
    let cache = match cache {
        Ok(c) => c,
        Err(why) => return refuse(format!("no place for the cache: {why}")),
    };
    let model = call::Command::from(&settings);
    let asker = Asker {
        command: &settings.command,
        model:   &settings.model,
    };
    match answer(&model, asker, &cache, &question, &output) {
        Ok(asked) => {
            for n in &asked.notes {
                eprintln!("ask: {n}");
            }
            println!("{}", asked.answer);
            println!();
            let from = if asked.whole {
                "the whole answer came from the cache".to_string()
            } else {
                format!("{} of them from the cache", asked.cached)
            };
            println!("{} chunk(s) of input, {from}", asked.chunks);
            Ok(Outcome::Ok)
        },
        Err(why) => refuse(why),
    }
}

/// The input, where it was piped: none where it is a terminal, which is left unread, since reading
/// a terminal to its end waits on a person who is not typing.
///
/// # Errors
/// `input` cannot be read.
pub fn read_piped<R: Read>(terminal: bool, input: R) -> std::io::Result<Option<String>> {
    if terminal {
        return Ok(None);
    }
    read_input(input).map(Some)
}

/// All of `input`, read as UTF-8 with any byte that is not replaced, so output that is not text is
/// still asked about and never dropped.
///
/// # Errors
/// `input` cannot be read.
pub fn read_input<R: Read>(mut input: R) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    input.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
