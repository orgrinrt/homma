//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The verbs: check all and walk through what needs it, check only, fix
//! one, hand a token or an npmrc to a publish, correct an expiry, and show
//! what a registry answered. The terminal is behind `Prompt`, so the order
//! of paste, prove and store is tested without one.

use std::path::Path;

use super::{Entry, Kind, Probe, State, Status, Store, Stored, Token, check, judge, line, masked};
use crate::release::day::Day;
use crate::release::publish::npmrc_line;

/// The person at the terminal.
pub trait Prompt {
    fn say(&self, text: &str);
    /// A line read without echo.
    fn secret(&self, ask: &str) -> String;
    /// A line read as typed.
    fn line(&self, ask: &str) -> String;
    /// Open `url` in a browser; `false` where none opened.
    fn open(&self, url: &str) -> bool;
    /// Hand the terminal to `argv` and wait for it.
    fn hand_over(&self, argv: &[&str]) -> Result<(), String>;
}

/// The three capabilities a walk is handed: where a token is kept, how a
/// registry is asked, and the person at the terminal.
#[derive(Clone, Copy)]
pub struct Ports<'a> {
    pub store:  &'a dyn Store,
    pub probe:  &'a dyn Probe,
    pub prompt: &'a dyn Prompt,
}

/// How many pastes a fix takes before it gives up.
pub const ATTEMPTS: usize = 3;

/// Mint, paste, prove, then store, in that order: a pasted token is judged
/// as the candidate before it reaches the store, so one the registry
/// rejects is never stored at all.
pub fn fix_token(entry: &Entry, token: &Token, ports: &Ports, today: Day, browser: bool) -> Status {
    let Ports {
        store,
        probe,
        prompt,
    } = *ports;
    if let Err(why) = store.writable(entry.key) {
        prompt.say(&format!(
            "\n  {}: {}\n    no token is asked for: {why}",
            entry.key, entry.what
        ));
        return Status::new(State::Refused, why.to_string());
    }
    prompt.say(&format!(
        "\n  {}: {}\n    {}",
        entry.key, entry.what, token.mint_url
    ));
    if !token.mint_hint.is_empty() {
        prompt.say(&format!("    {}", token.mint_hint));
    }
    if browser && prompt.open(token.mint_url) {
        prompt.say("    (opened in your browser)");
    }
    for attempt in 0 .. ATTEMPTS {
        let secret = prompt.secret("    paste the token (input hidden): ");
        let secret = secret.trim();
        if secret.is_empty() {
            prompt.say("    nothing pasted, giving up on this one");
            return Status::new(State::Missing, "skipped");
        }
        let raw = prompt.line("    expiry date, if you set one (YYYY-MM-DD, blank for none): ");
        let expires = Day::parse(&raw);
        if expires.is_none() && !raw.trim().is_empty() {
            prompt.say("    not a date this understands, recording no expiry");
        }
        let candidate = Stored {
            secret: secret.to_string(),
            expires,
        };
        let st = judge(token, Some(&candidate), probe, today);
        if matches!(st.state, State::Ok | State::Expiring) {
            if let Err(why) = store.put(entry.key, &candidate.secret, expires) {
                prompt.say(&format!("    accepted, and not stored: {why}"));
                return Status::new(State::Refused, why.to_string());
            }
            prompt.say(&format!("    stored and confirmed: {}", st.detail));
            return st;
        }
        prompt.say(&format!("    {}. not stored.", st.detail));
        if attempt + 1 < ATTEMPTS {
            prompt.say("    try again.");
        }
    }
    Status::new(State::Invalid, "three attempts, none accepted")
}

/// Fix one entry, whatever it reads as now.
pub fn fix(entry: &Entry, ports: &Ports, today: Day, browser: bool) -> Status {
    match &entry.kind {
        Kind::Token(t) => fix_token(entry, t, ports, today, browser),
        Kind::Cli(cli) => {
            ports.prompt.say(&format!(
                "\n  {}: {}\n    running: {}\n    this is that tool's own flow. answer its prompts.\n",
                entry.key,
                entry.what,
                cli.fix.join(" ")
            ));
            if let Err(why) = ports.prompt.hand_over(cli.fix) {
                return Status::new(State::Unreachable, why);
            }
            check(entry, ports.store, ports.probe, today)
        },
    }
}

/// Every entry checked, its lines, and whether all are fine: anything that
/// needs work or did not answer makes it not fine.
pub fn status(
    entries: &[Entry],
    store: &dyn Store,
    probe: &dyn Probe,
    today: Day,
) -> (Vec<String>, bool) {
    let mut lines = vec!["publish credentials\n".to_string()];
    let mut ok = true;
    for e in entries {
        let st = check(e, store, probe, today);
        lines.push(line(e, &st));
        if st.state != State::Ok {
            ok = false;
        }
    }
    (lines, ok)
}

/// Check all, leave the fine ones alone, and walk through the rest.
pub fn all(entries: &[Entry], ports: &Ports, today: Day, browser: bool) -> bool {
    let Ports {
        store,
        probe,
        prompt,
    } = *ports;
    prompt.say("publish credentials\n");
    let mut pending = Vec::new();
    for e in entries {
        let st = check(e, store, probe, today);
        prompt.say(&line(e, &st));
        if st.state.needs_work() {
            pending.push(e);
        }
    }
    if pending.is_empty() {
        prompt.say("\nall are good. nothing to do.");
        return true;
    }
    prompt.say(&format!(
        "\n{} to sort out. the rest are left alone.",
        pending.len()
    ));
    let failed: Vec<&str> = pending
        .into_iter()
        .filter(|e| fix(e, ports, today, browser).state.needs_work())
        .map(|e| e.key)
        .collect();
    if failed.is_empty() {
        prompt.say("\nall are good now.");
        true
    } else {
        prompt.say(&format!(
            "\nstill not right: {}. run it again once that is sorted.",
            failed.join(", ")
        ));
        false
    }
}

/// The entry `key` names where it holds a token, or why not.
fn token_entry<'a>(entries: &'a [Entry], key: &str) -> Result<&'a Entry, String> {
    entries
        .iter()
        .find(|e| e.key == key && e.token().is_some())
        .ok_or_else(|| format!("no token is held for {key}"))
}

fn held(store: &dyn Store, key: &str) -> Result<Stored, String> {
    store
        .get(key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("nothing stored for {key}. run `homma release auth` first."))
}

/// The token stored for `key`, for a publish to read.
pub fn token(entries: &[Entry], store: &dyn Store, key: &str) -> Result<String, String> {
    let e = token_entry(entries, key)?;
    Ok(held(store, e.key)?.secret)
}

/// Correct a recorded expiry without pasting the token again: `none`,
/// `never` or `-` drops it. The date is a note about one the registry
/// decides, so getting it wrong costs a nag, never access.
pub fn expiry(
    entries: &[Entry],
    store: &dyn Store,
    key: &str,
    raw: &str,
    today: Day,
) -> Result<String, String> {
    let e = token_entry(entries, key)?;
    let stored = held(store, e.key)?;
    let clear = ["none", "never", "-"].contains(&raw.to_ascii_lowercase().as_str());
    let when = if clear {
        None
    } else {
        Some(
            Day::parse(raw)
                .ok_or_else(|| format!("{raw} is not a date this understands. want YYYY-MM-DD."))?,
        )
    };
    store
        .put(e.key, &stored.secret, when)
        .map_err(|e| e.to_string())?;
    let back = held(store, e.key)?;
    if back.secret != stored.secret {
        return Err("the token did not survive the rewrite".into());
    }
    let shown = back
        .expires
        .map(|d| d.to_string())
        .unwrap_or_else(|| "none".into());
    Ok(match back.days_left(today) {
        Some(left) => format!("  {key}: expiry {shown}, {left} days left"),
        None => format!("  {key}: expiry {shown}"),
    })
}

/// What the store holds for `key` and what the registry answered, so a
/// rejection can be read.
pub fn diagnose(
    entries: &[Entry],
    store: &dyn Store,
    probe: &dyn Probe,
    key: &str,
    today: Day,
) -> Result<Vec<String>, String> {
    let e = entries
        .iter()
        .find(|e| e.key == key)
        .ok_or_else(|| format!("no registry called {key}"))?;
    let mut out = vec![format!("  {}: {}", e.key, e.what)];
    if let Some(t) = e.token() {
        out.push("  kind: token".into());
        match store.get(e.key) {
            Err(why) => out.push(format!("  {}: {why}", store.name())),
            Ok(None) => out.push(format!("  {}: nothing stored", store.name())),
            Ok(Some(s)) => {
                let shown = s
                    .expires
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "none".into());
                out.push(format!(
                    "  {}: {}, expiry {shown}",
                    store.name(),
                    masked(&s.secret)
                ));
                let (code, body) = probe.whoami(t.whoami_url, &s.secret, t.bearer);
                let body: String = body
                    .map(|b| b.to_string())
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect();
                out.push(format!("  {}", t.whoami_url));
                out.push(format!("  http: {code}"));
                out.push(format!("  body: {body}"));
            },
        }
    } else {
        out.push("  kind: cli".into());
    }
    let st = check(e, store, probe, today);
    out.push(format!("  verdict: {}, {}", st.state, st.detail));
    Ok(out)
}

/// Write the npmrc npm reads a token out of at `path`, rewritten whole so a
/// rotated token leaves nothing under it, created readable by its owner
/// alone before anything is in it.
pub fn write_npmrc(path: &Path, store: &dyn Store) -> Result<(), String> {
    let stored = held(store, "npm")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    crate::release::publish::write_private(path, &npmrc_line(&stored.secret))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The export lines a publish script wants, each reading the token through
/// `program`, the command line that reaches these verbs.
pub fn env_lines(entries: &[Entry], store: &dyn Store, program: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        let Some(t) = e.token() else { continue };
        if !matches!(store.get(e.key), Ok(Some(_))) {
            out.push(format!(
                "# {} unset: nothing stored for {}",
                t.env_var, e.key
            ));
            continue;
        }
        let verb = if e.key == "npm" { "npmrc".to_string() } else { format!("token {}", e.key) };
        out.push(format!("export {}=\"$({program} {verb})\"", t.env_var));
    }
    out
}
