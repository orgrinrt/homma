//! The session's checks, put to the command `homma ask` runs before it runs it: the `Bash` deny
//! rules and the `PreToolUse` hooks for `Bash` of the user's and the workspace's settings.
//!
//! A session sees `homma ask ...` and not the command inside it, so this is where that command meets
//! the same guards a `Bash` call of it would.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::cmd::aggregate::registrations;
use crate::cmd::util::bounded::{self, spawn};

/// How long one hook may run.
pub const HOOK_LIMIT: Duration = Duration::from_secs(60);

/// What the settings say about a `Bash` call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guards {
    /// Each `Bash` deny rule: `None` for a bare `Bash`, else the text inside its parentheses.
    pub deny:  Vec<Option<String>>,
    /// Each `PreToolUse` hook command registered for `Bash`, in the order the files give them.
    pub hooks: Vec<String>,
}

/// The settings files whose rules govern a session at `root`: the user's, then the workspace's
/// shared and local ones.
#[must_use]
pub fn files(home: Option<&Path>, root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = home
        .map(|h| h.join(".claude/settings.json"))
        .into_iter()
        .collect();
    out.push(root.join(".claude/settings.json"));
    out.push(root.join(".claude/settings.local.json"));
    out
}

/// The guards `files` hold. A file that is absent or does not parse holds none.
#[must_use]
pub fn read(files: &[PathBuf]) -> Guards {
    let mut g = Guards::default();
    for f in files {
        let Some(v) = std::fs::read_to_string(f)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            continue;
        };
        let deny = v
            .get("permissions")
            .and_then(|p| p.get("deny"))
            .and_then(|d| d.as_array());
        for rule in deny.into_iter().flatten().filter_map(|r| r.as_str()) {
            if rule == "Bash" {
                g.deny.push(None);
            } else if let Some(body) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')'))
            {
                g.deny.push(Some(body.to_string()));
            }
        }
        for (event, matcher, cmd) in registrations(&v) {
            if event == "PreToolUse" && matches_bash(matcher) && !g.hooks.iter().any(|h| h == cmd) {
                g.hooks.push(cmd.to_string());
            }
        }
    }
    g
}

/// Whether a hook registered under `matcher` sees a `Bash` call: empty and `*` see every call, and
/// anything else is a regular expression that must match `Bash` whole. One that does not compile
/// is read as the literal words it is made of, `|`-separated.
fn matches_bash(matcher: &str) -> bool {
    if matcher.is_empty() || matcher == "*" {
        return true;
    }
    match regex::Regex::new(&format!("^(?:{matcher})$")) {
        Ok(re) => re.is_match("Bash"),
        Err(_) => matcher.split('|').any(|w| w.trim() == "Bash"),
    }
}

/// The command and each command of it split at a newline, `&&`, `||`, `;`, `|` and a lone `&`,
/// trimmed and non-empty.
#[must_use]
pub fn commands(command: &str) -> Vec<&str> {
    let mut out = vec![command.trim()];
    let mut rest = command;
    while !rest.is_empty() {
        let cut = ["\n", "&&", "||", ";", "|", "&"]
            .iter()
            .filter_map(|sep| rest.find(sep).map(|i| (i, sep.len())))
            .min_by_key(|(i, len)| (*i, std::cmp::Reverse(*len)));
        let (piece, after) = match cut {
            Some((i, len)) => (&rest[.. i], &rest[i + len ..]),
            None => (rest, ""),
        };
        out.push(piece.trim());
        rest = after;
    }
    out.retain(|c| !c.is_empty());
    out.dedup();
    out
}

/// The deny rule that refuses `command`, written as it stands in the settings, where one does.
#[must_use]
pub fn denied(deny: &[Option<String>], command: &str) -> Option<String> {
    let parts = commands(command);
    deny.iter().find_map(|rule| {
        let hit = match rule {
            None => true,
            Some(body) => parts.iter().any(|c| rule_matches(body, c)),
        };
        hit.then(|| {
            rule.as_ref()
                .map_or("Bash".to_string(), |b| format!("Bash({b})"))
        })
    })
}

fn rule_matches(body: &str, command: &str) -> bool {
    match body.strip_suffix(":*") {
        Some(prefix) => command.starts_with(prefix),
        None => glob(body, command),
    }
}

/// `pattern` against `text`, a `*` matching any run of characters.
fn glob(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let tail: Vec<&str> = parts.collect();
    let Some((last, middle)) = tail.split_last() else {
        return rest.is_empty();
    };
    for p in middle {
        match rest.find(p) {
            Some(i) => rest = &rest[i + p.len() ..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// Where a call is made, as the hooks are told.
#[derive(Clone, Debug)]
pub struct Place<'a> {
    pub cwd:     &'a Path,
    pub root:    &'a Path,
    pub session: Option<&'a str>,
}

/// What the guards made of the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It may run; the hooks that could not run are named.
    Allowed {
        passed_over: Vec<String>,
    },
    /// It may not, and why.
    Refused(String),
}

/// Put `command` to the deny rules, then to every hook, stopping at the first that refuses.
#[must_use]
pub fn check(guards: &Guards, command: &str, at: &Place<'_>) -> Verdict {
    if let Some(rule) = denied(&guards.deny, command) {
        return Verdict::Refused(format!("the settings deny it by `{rule}`"));
    }
    let payload = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command },
        "cwd": at.cwd.display().to_string(),
        "session_id": at.session.unwrap_or(""),
    })
    .to_string();
    let mut passed_over = Vec::new();
    for hook in &guards.hooks {
        match run_hook(hook, &payload, at) {
            Ok(None) => {},
            Ok(Some(why)) => return Verdict::Refused(format!("a hook refused it: {why}")),
            Err(why) => passed_over.push(format!("{hook}: {why}")),
        }
    }
    Verdict::Allowed {
        passed_over,
    }
}

/// Run one hook as the host does: `Ok(Some(reason))` where it refuses, `Ok(None)` where it lets the
/// call through, `Err` where it could not be run.
fn run_hook(hook: &str, payload: &str, at: &Place<'_>) -> Result<Option<String>, String> {
    let child = spawn(
        std::process::Command::new("sh")
            .args(["-c", hook])
            .current_dir(at.cwd)
            .env("CLAUDE_PROJECT_DIR", at.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .map_err(|e| format!("did not start: {e}"))?;
    let ran = bounded::run(child, payload, HOOK_LIMIT).map_err(|s| s.reason(HOOK_LIMIT))?;
    let status = ran.status;
    let (stdout, stderr) = ran.text();
    let said = |s: &str| {
        let s = s.trim();
        if s.is_empty() {
            "no reason given".to_string()
        } else {
            s.lines().next().unwrap_or(s).to_string()
        }
    };
    if status.code() == Some(2) {
        return Ok(Some(said(&stderr)));
    }
    // The host reads the whole of stdout; a hook printing other lines before its JSON is read by
    // its last line that parses.
    let json = serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .or_else(|| {
            stdout
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        });
    if let Some(v) = json {
        let specific = v.get("hookSpecificOutput");
        // `ask` would ask a person, and none is there to answer.
        let deny = matches!(
            specific
                .and_then(|s| s.get("permissionDecision"))
                .and_then(|d| d.as_str()),
            Some("deny" | "ask")
        );
        let block = v.get("decision").and_then(|d| d.as_str()) == Some("block");
        if deny || block {
            let reason = specific
                .and_then(|s| s.get("permissionDecisionReason"))
                .or_else(|| v.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or(&stderr);
            return Ok(Some(said(reason)));
        }
    }
    Ok(None)
}
