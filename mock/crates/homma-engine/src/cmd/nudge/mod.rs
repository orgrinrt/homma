//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma agent nudge <event>`: the rows under `[agent.nudges]` run on one
//! host event, against the payload the host hands a hook on stdin.
//!
//! It prints nothing where nothing applies, the host's deny where a row stops
//! the call, and added context where a reminder is due. It never fails the
//! call: what it cannot read or keep is a line of the log.

pub(crate) mod record;
mod wrapper;

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};

use homma_api::{Nudge, Nudges, Strength};
use regex::Regex;
pub(crate) use wrapper::{NUDGES_PREFIX, install_wrappers};

use self::record::{Answered, Entry, Record, Stopped, Store};

/// The environment variable an evidence program reads the answer's reason in.
pub(crate) const REASON_VAR: &str = "HOMMA_NUDGE_REASON";

/// The one tool whose call can carry an answer.
const SHELL_TOOL: &str = "Bash";

/// What the verb runs against: the workspace its programs are under, where it
/// keeps its record, and the moment it is asked, in seconds.
pub(crate) struct Ctx<'a> {
    pub root:  &'a Path,
    pub store: Store,
    pub now:   u64,
}

/// The call as the payload gives it.
struct Call {
    session: String,
    tool:    String,
    text:    String,
    payload: serde_json::Value,
}

impl Call {
    fn read(payload: &str) -> Option<Call> {
        let payload: serde_json::Value = serde_json::from_str(payload).ok()?;
        let field = |k: &str| {
            payload
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let input = payload.get("tool_input");
        let text = match input {
            None => String::new(),
            Some(i) => {
                match i.get("command").and_then(|c| c.as_str()) {
                    Some(c) => c.to_string(),
                    None => i.to_string(),
                }
            },
        };
        Some(Call {
            session: field("session_id"),
            tool: field("tool_name"),
            text,
            payload,
        })
    }
}

/// What a program said.
enum Ran {
    /// Exit 0, with what it printed.
    Yes(String),
    /// Exit 1.
    No,
    /// Anything else, with why.
    Failed(String),
}

fn run_program(root: &Path, path: &str, stdin: &str, reason: Option<&str>) -> Ran {
    let mut cmd = Command::new(root.join(path));
    cmd.current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(r) = reason {
        cmd.env(REASON_VAR, r);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Ran::Failed(format!("`{path}` did not start: {e}")),
    };
    if let Some(mut input) = child.stdin.take() {
        // a program that reads none of its input closes the pipe early, which is its right
        let _ = input.write_all(stdin.as_bytes());
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return Ran::Failed(format!("`{path}`: {e}")),
    };
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.code() {
        Some(0) => Ran::Yes(said),
        Some(1) => Ran::No,
        other => {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let code = other.map_or_else(|| "a signal".to_string(), |c| format!("exit {c}"));
            Ran::Failed(format!("`{path}` ended with {code}: {why}"))
        },
    }
}

/// Where the row's marker sits in `text` as a word, and the reason after it.
fn marker_in<'t>(row: &Nudge, text: &'t str) -> Option<(usize, &'t str)> {
    let marker = row.answer()?;
    let re = Regex::new(&format!(
        r"(?m)(?:^|[\s#:;])({})(?:[ \t]+(.*))?$",
        regex::escape(marker)
    ))
    .ok()?;
    let caps = re.captures(text)?;
    let at = caps.get(1)?.start();
    Some((at, caps.get(2).map_or("", |m| m.as_str().trim())))
}

/// The call's text with the marker, its reason and the `#` or `:` before it
/// taken off the end, which is the call the agent means when it carries both.
fn without_marker(text: &str, at: usize) -> String {
    let head = text[.. at].trim_end();
    let head = head
        .strip_suffix('#')
        .or_else(|| head.strip_suffix(':'))
        .unwrap_or(head);
    head.trim_end().to_string()
}

fn within(at: u64, now: u64, row: &Nudge) -> bool {
    now.saturating_sub(at) < row.ttl().seconds()
}

/// The verb's decision on one payload, as the text to print, or nothing.
pub(crate) fn answer(ctx: &Ctx<'_>, nudges: &Nudges, event: &str, payload: &str) -> Option<String> {
    let rows: Vec<&Nudge> = nudges.of_event(event).collect();
    if rows.is_empty() {
        return None;
    }
    let Some(call) = Call::read(payload) else {
        log(
            ctx,
            "",
            "",
            "payload-unreadable",
            "",
            "the payload is not JSON",
        );
        return None;
    };
    let (mut record, broken) = ctx.store.load(&call.session);
    if let Some(why) = broken {
        log(ctx, &call.session, "", "record-unreadable", "", &why);
    }
    let before = record.clone();
    let said = decide(ctx, &rows, event, &call, &mut record);
    if record != before {
        if let Err(why) = ctx.store.keep(&call.session, &record) {
            log(ctx, &call.session, "", "record-not-kept", "", &why);
        }
    }
    said
}

fn decide(
    ctx: &Ctx<'_>,
    rows: &[&Nudge],
    event: &str,
    call: &Call,
    record: &mut Record,
) -> Option<String> {
    // the call that is the answer is taken first, so the stopped call it may be passes below
    let mut bare = call.text.clone();
    if call.tool == SHELL_TOOL {
        for row in rows.iter().filter(|r| r.strength() == Strength::Once) {
            let Some((at, reason)) = marker_in(row, &call.text) else {
                continue;
            };
            bare = without_marker(&call.text, at);
            let state = record.rows.entry(row.name().to_string()).or_default();
            let Some(stopped) = state.stopped.clone().filter(|s| within(s.at, ctx.now, row)) else {
                continue;
            };
            if let Some(evidence) = row.evidence() {
                let refused = match run_program(
                    ctx.root,
                    evidence,
                    &stopped.payload.to_string(),
                    Some(reason),
                ) {
                    Ran::Yes(_) => None,
                    Ran::No => Some(String::new()),
                    Ran::Failed(why) => Some(why),
                };
                if let Some(why) = refused {
                    log(
                        ctx,
                        &call.session,
                        row.name(),
                        "evidence-refused",
                        &stopped.text,
                        &why,
                    );
                    let mut said = format!("{}\nThe answer was not taken.", row.says());
                    if !why.is_empty() {
                        said.push_str(&format!(" {why}"));
                    }
                    return Some(deny(&said));
                }
            }
            log(
                ctx,
                &call.session,
                row.name(),
                "answered",
                &stopped.text,
                reason,
            );
            state.stopped = None;
            state.answered = Some(Answered {
                at:   ctx.now,
                text: stopped.text,
            });
        }
    }

    let mut informs: Vec<&Nudge> = Vec::new();
    for row in rows {
        if !row.takes_tool(&call.tool) || !row.takes_text(&bare) {
            continue;
        }
        let state = record.rows.entry(row.name().to_string()).or_default();
        match row.strength() {
            Strength::Inform => {
                if state.said.is_some_and(|at| within(at, ctx.now, row)) {
                    continue;
                }
                if applies(ctx, row, call).is_some() {
                    informs.push(row);
                }
            },
            Strength::Once => {
                if state
                    .answered
                    .as_ref()
                    .is_some_and(|a| a.text == bare && within(a.at, ctx.now, row))
                {
                    continue;
                }
                let again = state.stopped.as_ref().is_some_and(|s| {
                    s.text == bare && s.tool == call.tool && within(s.at, ctx.now, row)
                });
                if row.answer().is_none() && again {
                    log(
                        ctx,
                        &call.session,
                        row.name(),
                        "answered",
                        &bare,
                        "made again",
                    );
                    state.stopped = None;
                    state.answered = Some(Answered {
                        at:   ctx.now,
                        text: bare.clone(),
                    });
                    continue;
                }
                let Some(out) = applies(ctx, row, call) else {
                    continue;
                };
                let state = record.rows.entry(row.name().to_string()).or_default();
                state.stopped = Some(Stopped {
                    at:      ctx.now,
                    tool:    call.tool.clone(),
                    text:    bare.clone(),
                    payload: call.payload.clone(),
                });
                log(ctx, &call.session, row.name(), "stopped", &bare, "");
                return Some(deny(&stop_text(row, &out, &call.tool)));
            },
            Strength::Refuse => {
                if let Some(out) = applies(ctx, row, call) {
                    log(ctx, &call.session, row.name(), "refused", &bare, "");
                    return Some(deny(&joined(row.says(), &out)));
                }
            },
        }
    }
    if informs.is_empty() {
        return None;
    }
    let mut text = Vec::new();
    for row in informs {
        record.rows.entry(row.name().to_string()).or_default().said = Some(ctx.now);
        text.push(row.says().to_string());
    }
    Some(context(event, &text.join("\n")))
}

/// Whether the row's predicate says it applies, with what it printed. A row
/// with none applies.
fn applies(ctx: &Ctx<'_>, row: &Nudge, call: &Call) -> Option<String> {
    let Some(when) = row.when() else {
        return Some(String::new());
    };
    match run_program(ctx.root, when, &call.payload.to_string(), None) {
        Ran::Yes(out) => Some(out),
        Ran::No => None,
        Ran::Failed(why) => {
            log(
                ctx,
                &call.session,
                row.name(),
                "predicate-failed",
                &call.text,
                &why,
            );
            None
        },
    }
}

fn joined(says: &str, out: &str) -> String {
    if out.is_empty() { says.to_string() } else { format!("{says}\n{out}") }
}

fn stop_text(row: &Nudge, out: &str, tool: &str) -> String {
    let mut said = joined(row.says(), out);
    match row.answer() {
        None => said.push_str("\nTo go on, make the same call again."),
        Some(marker) if tool == SHELL_TOOL => {
            said.push_str(&format!(
                "\nTo go on, run the same command with `# {marker} <why>` after it, or answer with \
                 `: {marker} <why>` and then make the call again."
            ));
        },
        Some(marker) => {
            said.push_str(&format!(
                "\nTo go on, answer with the shell command `: {marker} <why>`, then make the call \
                 again."
            ));
        },
    }
    said
}

fn deny(reason: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

fn context(event: &str, text: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": text,
        }
    })
    .to_string()
}

fn log(ctx: &Ctx<'_>, session: &str, row: &str, what: &str, call: &str, reason: &str) {
    ctx.store.log(&Entry {
        at: ctx.now,
        session,
        row,
        what,
        call,
        reason,
    });
}

/// The verb: the payload from stdin, the rows from the loaded manifest, the
/// decision on stdout. A manifest that does not load is said as added context,
/// since every row it would have run is missing and only a person can fix it.
pub(crate) fn run(cfg: Result<homma_core::Config, String>, event: &str) {
    let mut payload = String::new();
    if std::io::stdin().read_to_string(&mut payload).is_err() {
        return;
    }
    let cfg = match cfg {
        Ok(c) => c,
        Err(why) => {
            println!(
                "{}",
                context(
                    event,
                    &format!("The nudges were not read, so none of them ran: {why}")
                )
            );
            return;
        },
    };
    let root = cfg.workspace.path.clone();
    let Ok(dir) = crate::cmd::util::homma_dir(&root, "nudges") else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let ctx = Ctx {
        root: &root,
        store: Store::at(dir),
        now,
    };
    if let Some(out) = answer(&ctx, &cfg.agent.nudges, event, &payload) {
        println!("{out}");
    }
}

#[cfg(test)]
mod tests;
