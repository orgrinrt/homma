//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The publish credentials a release needs, each proven live against the
//! registry with the credential held, and walked through where one is
//! missing, rejected or close to its recorded expiry.
//!
//! Two kinds. A token registry hands out a string the store holds and the
//! registry's own identity endpoint proves. A cli registry keeps its own
//! credential and gives a command to ask and a command to fix, so nothing
//! is copied out of it. A token is proven before it is stored, never after.

use std::fmt;

use super::day::Day;

pub mod flow;
pub mod store;

pub use store::{Store, Stored, Unavailable};

/// What a check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Missing,
    Invalid,
    /// Valid, and inside two weeks of its recorded expiry.
    Expiring,
    /// The registry or the store could not be asked, which says nothing
    /// about the credential and so prompts for nothing.
    Unreachable,
}

impl State {
    /// Whether the credential wants a person to act: never for a registry
    /// that did not answer, since a new token cannot fix a network.
    pub const fn needs_work(self) -> bool {
        matches!(self, State::Missing | State::Invalid | State::Expiring)
    }

    /// The seven-column mark a status line opens with.
    pub const fn mark(self) -> &'static str {
        match self {
            State::Ok => "   ok  ",
            State::Missing => "MISSING",
            State::Invalid => "INVALID",
            State::Expiring => "EXPIRES",
            State::Unreachable => "   ??  ",
        }
    }
}

/// A state and the words that go with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state:  State,
    pub detail: String,
}

impl Status {
    pub fn new(state: State, detail: impl Into<String>) -> Self {
        Self {
            state,
            detail: detail.into(),
        }
    }
}

/// A recorded expiry more than this many days out is fine.
pub const EXPIRY_WARNING_DAYS: i64 = 14;

/// A token the registry accepted, graded by the life its recorded expiry
/// leaves: none recorded is fine, past is invalid, two weeks or less is
/// expiring.
pub fn grade(stored: &Stored, today: Day, who: &str) -> Status {
    match stored.days_left(today) {
        None => Status::new(State::Ok, format!("{who}, no expiry recorded")),
        Some(left) if left < 0 => {
            Status::new(
                State::Invalid,
                format!("{who}, but the recorded expiry passed {} days ago", -left),
            )
        },
        Some(left) if left <= EXPIRY_WARNING_DAYS => {
            Status::new(State::Expiring, format!("{who}, expires in {left} days"))
        },
        Some(left) => Status::new(State::Ok, format!("{who}, {left} days left")),
    }
}

/// How a token registry is proven and minted.
#[derive(Debug)]
pub struct Token {
    /// Where a person mints one.
    pub mint_url:   &'static str,
    /// What to mint, said at the prompt.
    pub mint_hint:  &'static str,
    /// The variable a publish reads it from.
    pub env_var:    &'static str,
    /// The endpoint naming the token's holder.
    pub whoami_url: &'static str,
    /// Whether that endpoint wants `Bearer <token>`; crates.io takes the
    /// token bare and answers a Bearer with 401.
    pub bearer:     bool,
    /// Whether a 403 is a real token scoped too narrowly to read the
    /// endpoint, which is what crates.io answers a publish-scoped token.
    pub scoped_403: bool,
    /// The holder's name in the endpoint's answer.
    pub holder:     fn(&serde_json::Value) -> Option<String>,
}

/// How a cli registry is asked and fixed.
#[derive(Debug)]
pub struct Cli {
    pub check: &'static [&'static str],
    pub fix:   &'static [&'static str],
}

/// The two kinds.
#[derive(Debug)]
pub enum Kind {
    Token(Token),
    Cli(Cli),
}

/// One publish credential.
#[derive(Debug)]
pub struct Entry {
    /// The name the verbs take.
    pub key:  &'static str,
    pub what: &'static str,
    pub kind: Kind,
}

impl Entry {
    pub fn token(&self) -> Option<&Token> {
        match &self.kind {
            Kind::Token(t) => Some(t),
            Kind::Cli(_) => None,
        }
    }
}

fn crates_holder(v: &serde_json::Value) -> Option<String> {
    v.get("user")?.get("login")?.as_str().map(str::to_string)
}

fn jsr_holder(v: &serde_json::Value) -> Option<String> {
    v.get("name")
        .and_then(|n| n.as_str())
        .map(str::to_string)
        .or_else(|| v.get("githubId").map(|g| g.to_string()))
}

fn npm_holder(v: &serde_json::Value) -> Option<String> {
    v.get("username")?.as_str().map(str::to_string)
}

/// The four credentials a release here needs, a fixed set the engine names:
/// a fifth is a row here, its own check reading the same fields.
pub const ENTRIES: &[Entry] = &[
    Entry {
        key:  "crates-io",
        what: "crates.io, for the rust crates",
        kind: Kind::Token(Token {
            mint_url:   "https://crates.io/settings/tokens/new",
            mint_hint:  "scope it to publish-new and publish-update. that scope cannot read the \
                         identity endpoint, so the check confirms the token is real without naming \
                         you, which is fine",
            env_var:    "CARGO_REGISTRY_TOKEN",
            whoami_url: "https://crates.io/api/v1/me",
            bearer:     false,
            scoped_403: true,
            holder:     crates_holder,
        }),
    },
    Entry {
        key:  "jsr",
        what: "jsr.io, for the deno-side packages",
        kind: Kind::Token(Token {
            mint_url:   "https://jsr.io/account/tokens/create",
            mint_hint:  "it needs the publish permission; an expiry is fine, the check nags before \
                         it lapses",
            env_var:    "JSR_AUTH_TOKEN",
            whoami_url: "https://api.jsr.io/user",
            bearer:     true,
            scoped_403: false,
            holder:     jsr_holder,
        }),
    },
    Entry {
        key:  "npm",
        what: "npm, for the node distributions",
        kind: Kind::Token(Token {
            mint_url:   "https://www.npmjs.com/settings/~/tokens/new",
            mint_hint:  "a granular access token, with read and write on the packages you publish. \
                         the expiry is a date picker whose only limit is a day out, so give it a \
                         long one",
            env_var:    "NPM_CONFIG_USERCONFIG",
            whoami_url: "https://registry.npmjs.org/-/whoami",
            bearer:     true,
            scoped_403: false,
            holder:     npm_holder,
        }),
    },
    Entry {
        key:  "github",
        what: "github, for pull requests and releases",
        kind: Kind::Cli(Cli {
            check: &["gh", "auth", "status"],
            fix:   &["gh", "auth", "login", "--web", "--git-protocol", "ssh"],
        }),
    },
];

/// The entry a name names.
pub fn entry(key: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.key == key)
}

/// What a registry answered: an http status, `0` where nothing answered,
/// and the body where it parsed.
pub type Answer = (u16, Option<serde_json::Value>);

/// The live side of a check: the registry's identity endpoint, and a cli.
pub trait Probe {
    /// `GET url` carrying `token`, as a Bearer or bare.
    fn whoami(&self, url: &str, token: &str, bearer: bool) -> Answer;
    /// Run `argv`, `None` where it could not start, else whether it exited
    /// zero and everything it printed.
    fn cli(&self, argv: &[&str]) -> Option<(bool, String)>;
}

/// The host of a url, for saying who did not answer.
fn host(url: &str) -> &str {
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
}

/// Judge a token against its registry, `stored` being the candidate: the
/// one held, or one just pasted and not yet stored.
pub fn judge(token: &Token, stored: Option<&Stored>, probe: &dyn Probe, today: Day) -> Status {
    let Some(stored) = stored else {
        return Status::new(State::Missing, "no token stored");
    };
    let h = host(token.whoami_url);
    let (code, body) = probe.whoami(token.whoami_url, &stored.secret, token.bearer);
    match code {
        0 => Status::new(State::Unreachable, format!("{h} did not answer")),
        401 => Status::new(State::Invalid, format!("{h} does not recognise the token")),
        403 if token.scoped_403 => grade(stored, today, "stored, scoped too narrowly to name you"),
        200 => {
            let who = body
                .as_ref()
                .and_then(token.holder)
                .unwrap_or_else(|| "authenticated".into());
            grade(stored, today, &format!("as {who}"))
        },
        other => Status::new(State::Unreachable, format!("{h} answered {other}")),
    }
}

/// Check a cli registry by its own status command.
pub fn judge_cli(cli: &Cli, probe: &dyn Probe) -> Status {
    let program = cli.check.first().copied().unwrap_or("?");
    match probe.cli(cli.check) {
        None => {
            Status::new(
                State::Unreachable,
                format!("{program} is not installed or did not answer"),
            )
        },
        Some((false, _)) => Status::new(State::Missing, format!("{program} is not logged in")),
        Some((true, text)) => {
            let who = text
                .lines()
                .find(|l| l.contains("Logged in to"))
                .and_then(|l| l.rsplit("account").next())
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
                .filter(|w| !w.is_empty())
                .unwrap_or_else(|| "authenticated".into());
            Status::new(State::Ok, format!("as {who}"))
        },
    }
}

/// Check one entry: what the store holds, proven live. A store that cannot
/// be read is a reason on the line, never a failure of the whole status.
pub fn check(entry: &Entry, store: &dyn Store, probe: &dyn Probe, today: Day) -> Status {
    match &entry.kind {
        Kind::Cli(cli) => judge_cli(cli, probe),
        Kind::Token(token) => {
            match store.get(entry.key) {
                Err(why) => Status::new(State::Unreachable, why.to_string()),
                Ok(None) => Status::new(State::Missing, format!("no token in {}", store.name())),
                Ok(Some(stored)) => judge(token, Some(&stored), probe, today),
            }
        },
    }
}

/// One status line, and its detail under it.
pub fn line(entry: &Entry, st: &Status) -> String {
    let mut out = format!("  [{}] {:10} {}", st.state.mark(), entry.key, entry.what);
    if !st.detail.is_empty() {
        out.push_str(&format!("\n             {}", st.detail));
    }
    out
}

/// A token shown without being shown: its length and its two ends.
pub fn masked(secret: &str) -> String {
    let n = secret.chars().count();
    if n > 12 {
        let head: String = secret.chars().take(4).collect();
        let tail: String = secret.chars().skip(n - 4).collect();
        format!("{n} chars, {head}...{tail}")
    } else {
        format!("{n} chars, (short)")
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            State::Ok => "ok",
            State::Missing => "missing",
            State::Invalid => "invalid",
            State::Expiring => "expiring",
            State::Unreachable => "unreachable",
        })
    }
}

/// The real probe: the registry agent, and the program run where it is.
pub struct Live;

impl Probe for Live {
    fn whoami(&self, url: &str, token: &str, bearer: bool) -> Answer {
        match request(url, token, bearer).call() {
            Ok(resp) => {
                let code = resp.status();
                (code, resp.into_json().ok())
            },
            Err(ureq::Error::Status(code, _)) => (code, None),
            Err(_) => (0, None),
        }
    }

    fn cli(&self, argv: &[&str]) -> Option<(bool, String)> {
        let (program, args) = argv.split_first()?;
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Some((out.status.success(), text))
    }
}

/// The identity request, built apart from the call so what it carries is
/// tested without a network: the agent's own `User-Agent`, which crates.io
/// refuses a request without, and the token in the form the registry reads.
pub fn request(url: &str, token: &str, bearer: bool) -> ureq::Request {
    let auth = if bearer { format!("Bearer {token}") } else { token.to_string() };
    super::registry::agent()
        .get(url)
        .set("Authorization", &auth)
}

#[cfg(test)]
mod tests;
