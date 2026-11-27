//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a registration names its hook: the first word of its command reaching
//! into `.claude/hooks/`, the program before it and the arguments after.

/// What a registration's command runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookCall {
    /// The hook file, by its name under `.claude/hooks/`.
    pub name:   String,
    /// The words before the hook's path, which the host runs it with. Empty
    /// where the host runs the file itself.
    pub runner: String,
    /// Whatever followed the hook's path, as it was written.
    pub args:   String,
}

/// The hook a command runs: the first word reaching into `.claude/hooks/`,
/// whatever prefix it reaches it through, a relative path, the host's
/// project-root placeholder or an absolute one. None for a command running
/// nothing there, or something in a directory below it.
pub(crate) fn hook_call(cmd: &str) -> Option<HookCall> {
    let mut rest = cmd.trim_start();
    let mut before: Vec<&str> = Vec::new();
    while !rest.is_empty() {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        let after = after.trim_start();
        if word.contains(".claude/hooks/") {
            let bare: String = word.chars().filter(|c| *c != '"' && *c != '\'').collect();
            let (_, name) = bare.rsplit_once(".claude/hooks/")?;
            if name.is_empty() || name.contains('/') {
                return None;
            }
            return Some(HookCall {
                name:   name.to_string(),
                runner: before.join(" "),
                args:   after.trim_end().to_string(),
            });
        }
        before.push(word);
        rest = after;
    }
    None
}

/// The file a hook command runs, by its name under `.claude/hooks/`.
pub(crate) fn hook_file_named(cmd: &str) -> Option<String> {
    hook_call(cmd).map(|c| c.name)
}

/// Escape a string for embedding inside a bash single-quoted literal.
/// Replaces each `'` with `'\''` so the generated script is safe for
/// paths or names that happen to contain a single quote.
pub(crate) fn sh_single_quote_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}
