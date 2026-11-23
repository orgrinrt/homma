//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a registration names its hook and the program it runs it with, and the
//! scope check the dispatcher uses to decide whether a call lands in a hook's
//! repository.

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

/// The one program every registration of a hook runs it with, or the problem
/// to report for `shown` where there is not one.
///
/// Two registrations running one hook two ways cannot share a wrapper, since it
/// runs the hook one way. A runner that is a line of shell rather than a
/// program is refused too: the wrapper puts it in front of the hook in an
/// `exec`, which runs a program with arguments, so an assignment in front of
/// the hook fails there and a separator before it runs something else. `cd dir
/// && .claude/hooks/x.sh` would `exec` a `cd` binary, which exits 0, and that
/// would read as the hook's approval.
pub(crate) fn one_runner<'a>(
    runners: impl Iterator<Item = &'a str>,
    shown: &str,
) -> Result<String, String> {
    let runners: std::collections::BTreeSet<&str> = runners.collect();
    if runners.len() > 1 {
        let ways: Vec<String> = runners
            .iter()
            .map(|r| if r.is_empty() { "itself".to_string() } else { format!("`{r}`") })
            .collect();
        return Err(format!(
            "`{shown}` is run through {} by different registrations, and one wrapper can \
             only run it one way; not carried",
            ways.join(" and ")
        ));
    }
    let runner = runners.first().copied().unwrap_or("");
    let assignment = runner
        .split_whitespace()
        .next()
        .is_some_and(|w| w.contains('='));
    let shell = ['&', ';', '|', '(', ')', '`', '<', '>'];
    if assignment || runner.contains(shell) || runner.contains("$(") {
        return Err(format!(
            "`{shown}` is run through `{runner}`, which is a line of shell rather than a \
             program, and the wrapper can only run a program with arguments; not carried"
        ));
    }
    Ok(runner.to_string())
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

/// The scope check the dispatcher carries, as bash reading `$FILE`, `$CWD` and
/// `$CMD`, which the dispatcher has already read out of the host's JSON, `$CWD`
/// being the dispatcher's own directory where the host names none. It defines
/// `lands_in <dir>`, true when the call lands anywhere under that directory: a
/// file tool on its file, a shell call in its directory and on every path its
/// command names, read relative to that directory.
///
/// A command is split on everything that cannot be part of a path, which takes
/// quotes, separators, redirections and the `=` of `--flag=path` apart. A file
/// or a piece climbing with `..` lands everywhere, and a directory's absolute path anywhere
/// in the command counts, which is what catches one with a space in it. Not
/// seen: a relative path with a space in it, a path spelled through a variable,
/// and what a script the command runs does next.
pub(crate) const LANDS_IN_SH: &str = r#"under() {
    case "$2" in
        "$1"|"$1"/*) return 0 ;;
    esac
    return 1
}

# A path or piece climbing with `..` lands everywhere, since a guard skipped
# costs what the guard is for. Running one too often is not free either: a
# guard that denies can deny a call that never touched its repository.
lands_in() {
    local root=$1 w
    if [ -n "$FILE" ]; then
        case "$FILE" in
            ..|../*|*/..|*/../*) return 0 ;;
        esac
        case "$FILE" in
            /*) under "$root" "$FILE" ;;
            *)  under "$root" "$CWD/$FILE" ;;
        esac
        return
    fi
    under "$root" "$CWD" && return 0
    [ -n "$CMD" ] || return 1
    # The directory's own absolute path, ended by anything a name cannot carry
    # on with, so a neighbour whose name only starts the same is not it.
    case "$CMD" in
        *"$root"|*"$root"[!A-Za-z0-9_.@%+,:~-]*) return 0 ;;
    esac
    local IFS=$' \t\n'
    set -f
    for w in $(printf '%s' "$CMD" | tr -c 'A-Za-z0-9_./~@%+,:-' ' '); do
        case "$w" in
            '~/'*) w=${HOME:-}/${w#'~/'} ;;
            /*) ;;
            *) w=$CWD/$w ;;
        esac
        # A `.` component names the directory it sits in, so `./engine` is
        # `engine`.
        while [ "${w//\/.\//\/}" != "$w" ]; do
            w=${w//\/.\//\/}
        done
        w=${w%/.}
        case "$w" in
            */..|*/../*) set +f; return 0 ;;
        esac
        if under "$root" "$w"; then
            set +f
            return 0
        fi
    done
    set +f
    return 1
}
"#;
