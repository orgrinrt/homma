//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Workspace-level git-ops validation gate.
//!
//! Generates a workspace `.claude/hooks/` shell script
//! that intercepts Bash tool calls running `git commit` or `git push`
//! from within a member repo's subtree and verifies that repo has a
//! healthy mockspace bootstrap before allowing the operation through.
//!
//! The gate is workspace-authored (not aggregated): one script registered
//! against the Bash matcher, embedded with the list of member-repo
//! absolute paths at generation time. When fired, it:
//!
//! 1. Reads the Claude Code tool-input JSON from stdin.
//! 2. Checks the `tool_input.command` field; exits 0 silently if it does
//!    not look like `git commit` or `git push` (out-of-scope call).
//! 3. Resolves the active repo from `tool_input.cwd` (or `$PWD` as
//!    fallback); exits 0 silently if no member repo's tree covers it.
//! 4. Verifies the repo has `mock/`, a config marker (a launcher pin in
//!    `mockspace.toml`, or the legacy `cargo mock` alias in
//!    `.cargo/config.toml`), and a `core.hooksPath` set so per-repo git
//!    hooks fire.
//! 5. If healthy: silent allow. If anything is missing: structured deny
//!    with a hookSpecificOutput payload pointing at the fix.
//!
//! Defence-in-depth: per-repo git hooks (installed by mockspace
//! bootstrap) are the primary enforcement. This workspace gate covers
//! the case where the per-repo hook would not fire because the bootstrap
//! was never run or `core.hooksPath` got reset.

use anyhow::{Context, Result};

use crate::cmd::aggregate::HookEntry;

/// File name for the generated workspace gate script.
const GATE_SCRIPT_NAME: &str = "_workspace--mockspace-gate.sh";

/// Generate the workspace mockspace gate script and return the
/// `settings.json` entry registering it.
///
/// The script is written to `<workspace>/.claude/hooks/`. The returned
/// entry is appended to `hooks.PreToolUse[]` with matcher `Bash` so it
/// fires on every Bash tool call; the script itself decides which
/// commands warrant inspection.
///
/// `repos` is the list of `(repo_name, workspace_relative_path)` pairs that
/// the gate should consider as "member repos" worth validating. Relative,
/// because the gate script is tracked and a path naming the workspace that
/// generated it matches nothing in any other clone. A repo declared outside
/// the workspace has no relative form and keeps its absolute one; the script
/// handles both.
pub(crate) fn install_workspace_gate(
    root: &homma_api::Root,
    repos: &[(String, String)],
) -> Result<HookEntry> {
    // Proven, like every other write this pass performs. This one installs an
    // executable the agent harness then runs on every tool call, which is the
    // write with the largest consequence in the program.
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    root.create_dir_all(&hooks_dir).ok();
    let target = root
        .contain_under(&hooks_dir, GATE_SCRIPT_NAME)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Normalised here rather than by the caller, because a caller that
    // forgets is a caller nothing can catch: the resulting table entry is a
    // plausible-looking string that simply never matches, and the gate then
    // finds no repo covering any path. The function that owns the contract is
    // the one place a test can hold it.
    let repos: Vec<(String, String)> = repos
        .iter()
        .map(|(n, p)| {
            (
                n.clone(),
                crate::cmd::util::relative_str(std::path::Path::new(p)),
            )
        })
        .collect();
    let body = gate_script(&repos);
    root.write(&target, body)
        .with_context(|| format!("writing {}", target.as_path().display()))?;
    #[cfg(unix)]
    root.set_executable(&target)?;
    // `${CLAUDE_PROJECT_DIR}`, for the same reason the per-repo wrappers use
    // it: the host substitutes the project root regardless of the working
    // directory the hook runs in, so one tracked `settings.json` names this
    // workspace's gate in every clone.
    Ok(HookEntry {
        matcher: "Bash".to_string(),
        command: format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{GATE_SCRIPT_NAME}"),
    })
}

/// True if a single hook command string points at the workspace-gate
/// script. Used by `merge_settings` when filtering hooks individually
/// within a multi-hook entry. Identification is by command-path
/// basename so absolute and relative path forms both match cleanly
/// across regens.
pub(crate) fn is_workspace_gate_command(cmd: &str) -> bool {
    cmd.rsplit('/').next().unwrap_or(cmd) == GATE_SCRIPT_NAME
}

/// Render the gate script body with the workspace's member-repo list
/// baked in. The script is portable bash (>=3.2, the macOS default).
fn gate_script(repos: &[(String, String)]) -> String {
    let mut repo_table = String::new();
    for (name, path) in repos {
        // Escape any embedded single quotes so the bash literal stays
        // well-formed for paths or names containing them.
        let entry = crate::cmd::aggregate::sh_single_quote_escape(&format!("{name}|{path}"));
        repo_table.push_str(&format!("    '{entry}'\n"));
    }

    format!(
        r##"#!/usr/bin/env bash
# Generated by `homma agent regen` (#454 phase 3).
# Workspace-level mockspace bootstrap gate.
#
# Intercepts Bash tool calls running git commit / git push from inside a
# member repo's tree and denies the call if the repo's mockspace
# bootstrap is missing or broken. Per-repo git hooks remain the primary
# enforcement; this gate covers the case where they would not fire.

set -u

# The workspace this file sits in, found from its own location: a gate script
# lives at `<workspace>/.claude/hooks/`, a fixed depth, so it needs no baked
# prefix and no environment variable to know where the member repos are.
WS=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")/../.." && pwd) || exit 0

INPUT=$(cat)

# `jq` reads the input below and encodes the reason at the bottom, so without it
# this script cannot tell an out-of-scope call from one it should stop.
#
# **It said nothing and allowed everything.** Both reads carried `2>/dev/null`,
# so a missing `jq` produced an empty command line, the out-of-scope branch
# below took it, and every commit and every push went through at exit 0. That is
# indistinguishable from the gate having looked and found nothing wrong, which
# is the one thing a gate may never be.
#
# So it denies instead. A denial is visible and is one `brew install jq` from
# gone; a silent allow is neither.
if ! command -v jq >/dev/null 2>&1; then
    printf '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}}}\n' \
        "the mockspace adoption gate needs jq and cannot find it on PATH, so it cannot read this call. Install jq, or remove this hook."
    exit 0
fi

command_line=$(printf '%s' "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null)

# Out of scope: not a Bash command call.
if [ -z "$command_line" ]; then
    exit 0
fi

# Out of scope: command does not start with `git commit` or `git push`.
# Tolerate a leading subdir cd (`cd foo && git commit ...`).
case "$command_line" in
    *"git commit"*) ;;
    *"git push"*) ;;
    *) exit 0 ;;
esac

# Determine active directory: prefer tool_input.cwd, fall back to PWD.
target_dir=$(printf '%s' "$INPUT" | jq -r '.tool_input.cwd // empty' 2>/dev/null)
if [ -z "$target_dir" ]; then
    target_dir="$PWD"
fi

# Member-repo table: each entry is `<name>|<path>`, where the path is
# relative to the workspace unless the manifest declared the repo outside it.
REPOS=(
{repo_table})

# Find which repo's tree covers target_dir (longest-prefix match).
match_name=""
match_path=""
for entry in "${{REPOS[@]}}"; do
    name="${{entry%%|*}}"
    raw="${{entry#*|}}"
    case "$raw" in
        /*) path="$raw" ;;
        *)  path="$WS/$raw" ;;
    esac
    case "$target_dir" in
        "$path"|"$path"/*)
            # Prefer the most specific (longest) path.
            if [ "${{#path}}" -gt "${{#match_path}}" ]; then
                match_name="$name"
                match_path="$path"
            fi
            ;;
    esac
done

# Out of scope: target is not under any member repo.
if [ -z "$match_path" ]; then
    exit 0
fi

# The mockspace tool repo is not a mockspace *consumer*. Its mock/ is v2
# self-hosting and its v1 src is intentionally ungated (no core.hooksPath),
# so it always scores partial adoption by design, not drift. Skip the gate
# for it, identified by its own Cargo package name.
if [ "$match_name" = "mockspace" ] \
    && grep -qE '^name[[:space:]]*=[[:space:]]*"mockspace"[[:space:]]*$' "$match_path/Cargo.toml" 2>/dev/null; then
    exit 0
fi

# Decide whether the repo has *partially* adopted mockspace.
# A repo with NO mockspace surface (no mock/, no alias, no
# core.hooksPath) has not adopted mockspace at all; the gate stays
# out of the way to avoid enforcing adoption on unrelated repos.
# Only flag repos where some surfaces exist but others are missing,
# i.e. actual drift.
has_mock_dir=0
has_alias=0
has_pin=0
has_hooks_path=0

[ -d "$match_path/mock" ] && has_mock_dir=1

if [ -f "$match_path/.cargo/config.toml" ]; then
    # Section-aware: only count `mock = ...` lines that appear under
    # the `[alias]` table header. A bare `mock = ...` under any other
    # section (e.g. `[envs]`) is not the cargo alias the bootstrap
    # writes and should not satisfy the adoption check. awk's state
    # machine handles the section tracking cleanly without pulling
    # in a TOML parser.
    alias_hit=$(awk '
        /^\[alias\][[:space:]]*$/ {{ in_alias=1; next }}
        /^\[[^]]+\][[:space:]]*$/ {{ in_alias=0; next }}
        in_alias && /^[[:space:]]*mock[[:space:]]*=/ {{ print "1"; exit }}
    ' "$match_path/.cargo/config.toml")
    [ "$alias_hit" = "1" ] && has_alias=1
fi

if [ -n "$(git -C "$match_path" config --get core.hooksPath 2>/dev/null)" ]; then
    has_hooks_path=1
fi

# The launcher+pin model has no `[alias] mock`; its config marker is a
# top-level `mockspace_*` pin in a root or mock/ mockspace.toml. Either the
# legacy alias OR a launcher pin satisfies the "config present" surface.
for cfg in "$match_path/mockspace.toml" "$match_path/mock/mockspace.toml"; do
    [ -f "$cfg" ] || continue
    pin_hit=$(awk '
        /^\[/ {{ exit }}
        /^[[:space:]]*mockspace_(version|branch|rev|tag)[[:space:]]*=/ {{ print "1"; exit }}
    ' "$cfg")
    [ "$pin_hit" = "1" ] && {{ has_pin=1; break; }}
done
if [ "$has_alias" -eq 1 ] || [ "$has_pin" -eq 1 ]; then has_config=1; else has_config=0; fi

adoption=$((has_mock_dir + has_config + has_hooks_path))

problems=()
fix=""

# Zero adoption is a repo that never opted in and full adoption is one with
# nothing to say. Anything between the two is drift.
if [ "$adoption" -ne 0 ] && [ "$adoption" -ne 3 ]; then
    [ "$has_mock_dir" -eq 0 ] && \
        problems+=("$match_path/mock/ is missing but other mockspace surfaces are present")
    [ "$has_config" -eq 0 ] && \
        problems+=("$match_path: no launcher pin (mockspace_* in mockspace.toml) and no legacy [alias] mock entry")
    [ "$has_hooks_path" -eq 0 ] && \
        problems+=("$match_path: git config core.hooksPath is not set; per-repo git hooks will not fire")
    fix="Run \`homma agent regen --repo $match_name\` to fix."
fi

# The shared tool configs, where there is a workspace to ask about.
#
# No manifest means no workspace: the repo table this script carries came from
# one, and without it there is nothing for homma to resolve `$match_name`
# against. Skipping is the honest answer rather than refusing, because a
# refusal would be this gate reporting the absence of its own input as a fault
# in somebody's repo.
#
# Asked of homma rather than decided here, so the rule
# saying what a repo owes exists once. The templates directory decides which
# repos want which config, and a second copy of that decision in bash would
# start disagreeing with the first the day somebody adds a template, which is
# the exact thing the directory exists to prevent.
#
# It refuses when it cannot ask at all, which is what this script already does
# for a missing `jq`, for the reason argued there: a check that could not run
# must never be indistinguishable from one that ran and found nothing.
if [ ! -f "$WS/homma.toml" ]; then
    :
elif ! command -v homma >/dev/null 2>&1; then
    problems+=("homma is not on PATH, so the shared tool configs could not be checked")
else
    config_out=$(homma --dir "$WS" repo config check --repo "$match_name" 2>&1)
    config_rc=$?
    # `1` is the check having run and found the repo owing something, which is
    # what the advice below is for. Anything else non-zero is the check not
    # having run at all, and the commonest one is this workspace's own manifest
    # failing to parse. Sending somebody to `init` for that is sending them to a
    # command that reads the same manifest and fails the same way, so the two
    # cases say different things.
    if [ "$config_rc" -eq 1 ]; then
        while IFS= read -r line; do
            [ -n "$line" ] && problems+=("$line")
        done <<<"$config_out"
        fix="Run \`homma repo config init --repo $match_name\` to place what is missing."
    elif [ "$config_rc" -ne 0 ]; then
        problems+=("the shared tool configs could not be checked: $config_out")
        fix="Read \`$WS/homma.toml\`; homma could not run against it, so this says nothing about repo \`$match_name\`."
    fi
fi

# Nothing wrong on either count, which is nearly every call.
if [ "${{#problems[@]}}" -eq 0 ]; then
    exit 0
fi

# Build a multi-line reason for the deny payload.
reason="repo \`$match_name\` ($match_path) is not ready to commit:"
for p in "${{problems[@]}}"; do
    reason+=$'\n  - '"$p"
done
[ -n "$fix" ] && reason+=$'\n'"$fix"

# Emit a structured deny so Claude Code surfaces it cleanly.
# `jq -Rs .` JSON-encodes stdin as a quoted string; trim the outer
# quotes since our format string already supplies them.
escaped=$(printf '%s' "$reason" | jq -Rs . | sed 's/^"//;s/"$//')
printf '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}}}\n' \
    "$escaped"
exit 0
"##,
        repo_table = repo_table.trim_end(),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs;
    use std::path::Path;

    /// A `Root` over a test workspace, denying nothing that a test uses.
    ///
    /// The real code path, not a variant of it: these go through
    /// `Root::contain` exactly as production does, which is what makes the
    /// containment they assert mean anything.
    fn test_root(workspace: &Path) -> homma_api::Root {
        homma_api::Root::new(
            &homma_api::AbsPath::new(workspace).expect("a tempdir path is absolute"),
            homma_api::Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap()),
        )
        .expect("a tempdir is a legitimate root")
    }
    use super::*;

    #[test]
    fn install_writes_executable_script_and_returns_entry() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let repos = vec![
            ("arvo".to_string(), "arvo".to_string()),
            ("notko".to_string(), "notko".to_string()),
        ];
        let entry = install_workspace_gate(&test_root(workspace), &repos).unwrap();
        assert_eq!(entry.matcher, "Bash");
        assert_eq!(
            entry.command, "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_workspace--mockspace-gate.sh",
            "the registered command must name the host's project-root placeholder",
        );
        assert!(
            !entry.command.contains(workspace.to_str().unwrap()),
            "the command named the workspace that generated it: {}",
            entry.command,
        );

        let script = workspace.join(".claude/hooks/_workspace--mockspace-gate.sh");
        assert!(script.exists());
        let body = fs::read_to_string(&script).unwrap();
        assert!(body.starts_with("#!/usr/bin/env bash"));
        assert!(body.contains("'arvo|arvo'"));
        assert!(body.contains("'notko|notko'"));
        assert!(body.contains("git commit"));
        assert!(body.contains("git push"));
        assert!(
            !body.contains(workspace.to_str().unwrap()),
            "the gate baked the generating workspace's path"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&script).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "script not executable: mode {mode:o}");
        }
    }

    #[test]
    fn is_workspace_gate_command_matches_managed_path() {
        assert!(is_workspace_gate_command(
            ".claude/hooks/_workspace--mockspace-gate.sh"
        ));
    }

    #[test]
    fn is_workspace_gate_command_rejects_other_paths() {
        assert!(!is_workspace_gate_command(
            ".claude/hooks/arvo--no-alloc.sh"
        ));
    }

    #[test]
    fn a_relative_repo_entry_resolves_against_the_workspace_the_gate_sits_in() {
        // The gate is tracked, so its repo table travels to every clone. A
        // relative entry has to be joined to the workspace the script is
        // running from rather than to whichever one wrote it, and an absolute
        // entry, which is what a repo declared outside the workspace leaves,
        // has to be left alone.
        let probe = |raw: &str, ws: &str| -> String {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(format!(
                    r#"WS='{ws}'; raw='{raw}'; case "$raw" in /*) path="$raw";; *) path="$WS/$raw";; esac; printf '%s' "$path""#
                ))
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert_eq!(probe("arvo", "/here"), "/here/arvo");
        // The control on the other arm, so the join is not unconditional.
        assert_eq!(probe("/elsewhere/arvo", "/here"), "/elsewhere/arvo");
    }

    #[test]
    fn a_manifest_path_carrying_a_curdir_component_resolves_to_the_repo() {
        // The gate's repo table is the one place a manifest path reaches an
        // emitted script without passing through a `strip_prefix` that would
        // have normalised it: `agent.rs` reads `local_path` straight off the
        // manifest. So `local_path = "./arvo"` is the reachable shape, and
        // `/ws/./arvo` is not a textual prefix of `/ws/arvo/src/lib.rs`, which
        // is how the longest-prefix match silently finds nothing.
        //
        // Through `install_workspace_gate` rather than through the helper, so
        // this holds the wiring. An earlier draft called `relative_str`
        // directly and passed with the wiring removed, which is what moved the
        // normalisation inside the function.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        install_workspace_gate(&test_root(ws), &[(
            "arvo".to_string(),
            "./arvo".to_string(),
        )])
        .unwrap();
        let installed =
            fs::read_to_string(ws.join(".claude/hooks/_workspace--mockspace-gate.sh")).unwrap();
        assert!(
            installed.contains("'arvo|arvo'"),
            "the gate kept a no-op path component from the manifest: {installed}"
        );
        assert!(!installed.contains("/./"));

        // And the resolution the script performs on it, run, so this is not a
        // claim about a string. The control is the un-normalised form, which is
        // what the same table held before and which does not match.
        let matched = |table_entry: &str| -> bool {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(format!(
                    r#"WS=/ws; raw='{table_entry}'; case "$raw" in /*) p="$raw";; *) p="$WS/$raw";; esac
                       case "/ws/arvo/src/lib.rs" in "$p"|"$p"/*) exit 0;; *) exit 1;; esac"#
                ))
                .output()
                .unwrap();
            out.status.success()
        };
        assert!(matched("arvo"));
        assert!(!matched("./arvo"));
    }

    #[test]
    fn script_body_includes_workspace_repo_paths() {
        let repos = vec![
            ("arvo".to_string(), "/abs/arvo".to_string()),
            ("hilavitkutin".to_string(), "/abs/hilavitkutin".to_string()),
        ];
        let body = gate_script(&repos);
        assert!(body.contains("'arvo|/abs/arvo'"));
        assert!(body.contains("'hilavitkutin|/abs/hilavitkutin'"));
    }

    #[test]
    fn script_body_handles_empty_repo_list() {
        let body = gate_script(&[]);
        assert!(body.contains("REPOS=("));
        assert!(body.contains("git commit"));
    }

    // -------------------------------------------------------------------
    // End-to-end bash-execution tests for the rendered gate script. The
    // script reads JSON from stdin via `jq`, walks the workspace's
    // member-repo table to find a covering repo, then probes that
    // repo's mockspace adoption surfaces (`mock/` dir, `cargo mock`
    // alias under `[alias]` in `.cargo/config.toml`, and
    // `core.hooksPath` git config). All exit codes are 0; the
    // allow/deny signal travels through stdout's JSON payload.
    //
    // Each test renders the script via gate_script(), writes it to a
    // temp file, builds a synthetic repo tree with whichever surfaces
    // the case wants present, then invokes `bash <script>` with
    // synthesised INPUT JSON.
    //
    // **They used to skip when bash or jq were missing, and a skip here is
    // indistinguishable from a pass.** Six tests returned early and reported
    // green having run nothing, on the same machine where the script they
    // cover allowed every commit for the same reason. `bash` is required
    // outright now, since a bash script cannot be covered without it. `jq` is
    // a case rather than an excuse: `run_gate` takes the PATH to run under, so
    // its absence is one more thing the suite asserts about.
    // -------------------------------------------------------------------

    /// Synthesise an INPUT JSON payload matching the Claude Code Bash
    /// tool-call shape the gate script parses.
    fn input_json(command: &str, cwd: &str) -> String {
        serde_json::json!({
            "tool_input": {
                "command": command,
                "cwd": cwd,
            }
        })
        .to_string()
    }

    /// Run the gate script with the given INPUT, under the inherited PATH,
    /// and return stdout.
    fn run_gate(repos: &[(String, String)], input: &str) -> String {
        run_gate_on_path(repos, input, None)
    }

    /// The same, with `path` replacing `PATH` for the child.
    ///
    /// The parameter exists so the missing-`jq` case is a test rather than a
    /// reason to skip five others. Passing a directory holding only `bash` and
    /// its dependencies reproduces a machine without `jq` on one that has it.
    fn run_gate_on_path(repos: &[(String, String)], input: &str, path: Option<&str>) -> String {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Required rather than skipped. This suite covers a bash script, so
        // without bash it covers nothing, and saying so is the only honest
        // report available.
        assert!(
            Command::new("bash").arg("--version").output().is_ok(),
            "these tests cover a bash script and bash is not on PATH"
        );

        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("gate.sh");
        fs::write(&script_path, gate_script(repos)).unwrap();

        let mut cmd = Command::new("bash");
        cmd.arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(p) = path {
            cmd.env("PATH", p);
        }
        let mut child = cmd.spawn().expect("bash would not start");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("bash would not finish");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Build a fake repo tree with a tunable set of mockspace surfaces.
    /// Returns the repo's absolute path.
    fn fake_repo(
        parent: &Path,
        name: &str,
        with_mock_dir: bool,
        alias_shape: AliasShape,
        with_hooks_path: bool,
    ) -> std::path::PathBuf {
        let repo = parent.join(name);
        fs::create_dir_all(&repo).unwrap();

        if with_mock_dir {
            fs::create_dir_all(repo.join("mock")).unwrap();
        }

        match alias_shape {
            AliasShape::None => {},
            AliasShape::UnderAlias => {
                let cfg = repo.join(".cargo/config.toml");
                fs::create_dir_all(cfg.parent().unwrap()).unwrap();
                fs::write(&cfg, "[alias]\nmock = \"run -p mock-cli --\"\n").unwrap();
            },
            AliasShape::UnderOtherSection => {
                let cfg = repo.join(".cargo/config.toml");
                fs::create_dir_all(cfg.parent().unwrap()).unwrap();
                // `mock = ...` at top-level outside any section AND
                // `mock = ...` under an unrelated section. Neither
                // satisfies the section-aware alias check.
                fs::write(&cfg, "[envs]\nmock = \"some-other-value\"\n").unwrap();
            },
        }

        if with_hooks_path {
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&repo)
                .output()
                .unwrap();
            std::process::Command::new("git")
                .args(["config", "core.hooksPath", "mock/target/hooks"])
                .current_dir(&repo)
                .output()
                .unwrap();
        }

        repo
    }

    enum AliasShape {
        None,
        UnderAlias,
        UnderOtherSection,
    }

    /// A PATH holding what these scripts need and no `jq`.
    ///
    /// Built by symlinking each program rather than by trimming the real PATH,
    /// because there is no way to un-find something on a PATH and `jq` sits in a
    /// different directory on every machine.
    ///
    /// The list is what the generated scripts actually shell out to. It is short
    /// because most of what they use is a bash builtin, and it is explicit
    /// because a PATH missing one of these fails somewhere unrelated: the first
    /// version of this held `bash` alone, and `dirname` came back empty, so the
    /// script resolved its workspace to `/` and reached the check under test by
    /// accident.
    pub(crate) fn a_path_without_jq(dir: &Path) -> String {
        use std::process::Command;
        let bin = dir.join("bin-without-jq");
        fs::create_dir_all(&bin).unwrap();
        for program in ["bash", "dirname", "cat"] {
            let found = Command::new("sh")
                .args(["-c", &format!("command -v {program}")])
                .output()
                .expect("no shell");
            let found = String::from_utf8_lossy(&found.stdout).trim().to_string();
            assert!(!found.is_empty(), "{program} is not on PATH");
            std::os::unix::fs::symlink(&found, bin.join(program)).unwrap();
        }
        let path = bin.to_string_lossy().into_owned();
        // That it really has no jq, rather than being assumed to. A PATH that
        // happened to reach one would leave every caller testing the ordinary
        // path twice and reporting green.
        assert!(
            !Command::new(bin.join("bash"))
                .args(["-c", "command -v jq"])
                .env("PATH", &path)
                .status()
                .unwrap()
                .success(),
            "the restricted PATH still reaches jq"
        );
        path
    }

    /// **The gate denies rather than allowing silently when `jq` is missing.**
    ///
    /// It exited 0 with no output instead: both reads of the input carried
    /// `2>/dev/null`, so the command line came back empty and the script took
    /// its own out-of-scope branch, which is the same exit 0 it produces when
    /// it has looked and found nothing wrong. Nothing outside could tell the
    /// two apart, and every commit and every push in such a workspace went
    /// through unchecked.
    #[test]
    fn gate_e2e_without_jq_denies_rather_than_allowing_silently() {
        let dir = tempfile::tempdir().unwrap();
        let path = a_path_without_jq(dir.path());
        // Full adoption: the case that is *supposed* to be allowed, so a deny
        // here can only be the missing tool and not the drift check.
        let repo = fake_repo(dir.path(), "arvo", true, AliasShape::UnderAlias, true);
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());

        // The control, on the inherited PATH: this exact call is allowed when
        // `jq` is there. Without it the assertion below cannot tell a
        // missing-tool deny from a gate that denies everything.
        let with_jq = run_gate(&repos, &input);
        assert!(
            !with_jq.contains("\"permissionDecision\":\"deny\""),
            "control: full adoption is allowed when jq is present; got: {with_jq}"
        );

        let out = run_gate_on_path(&repos, &input, Some(&path));
        assert!(
            out.contains("\"permissionDecision\":\"deny\""),
            "a workspace without jq allowed a commit the gate could not read; got: {out}"
        );
        assert!(
            out.contains("jq"),
            "the refusal has to name the missing tool, or nobody can fix it: {out}"
        );
    }

    #[test]
    fn gate_e2e_full_adoption_emits_no_deny() {
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(dir.path(), "arvo", true, AliasShape::UnderAlias, true);
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "full adoption should not deny; got: {out}",
        );
    }

    #[test]
    fn gate_e2e_launcher_pin_counts_as_config_marker() {
        // launcher+pin model: no `[alias] mock`, but a root mockspace.toml with
        // a `mockspace_branch` pin. With mock/ + hooksPath present, that is full
        // adoption and must not deny.
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(dir.path(), "arvo", true, AliasShape::None, true);
        fs::write(
            repo.join("mockspace.toml"),
            "mock_dir = \"mock\"\nmockspace_branch = \"dev\"\n",
        )
        .unwrap();
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "launcher pin + mock/ + hooks is full adoption; should not deny; got: {out}",
        );
    }

    #[test]
    fn gate_e2e_partial_adoption_emits_deny() {
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(dir.path(), "arvo", true, AliasShape::None, false);
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            out.contains("\"permissionDecision\":\"deny\""),
            "partial adoption (mock/ only) should deny; got: {out}",
        );
        assert!(
            out.contains("[alias] mock entry"),
            "deny reason should name the missing config marker; got: {out}",
        );
    }

    #[test]
    fn gate_e2e_zero_adoption_emits_no_deny() {
        // No mock/, no alias, no hooksPath. The gate stays out of the
        // way to avoid enforcing adoption on unrelated repos.
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(dir.path(), "arvo", false, AliasShape::None, false);
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "zero adoption should be a silent allow; got: {out}",
        );
    }

    #[test]
    fn gate_e2e_alias_under_other_section_does_not_satisfy_check() {
        // The `.cargo/config.toml` has a `mock = ...` line, but it
        // appears under `[envs]` rather than `[alias]`. The
        // section-aware check should NOT count this as the cargo
        // alias the bootstrap writes. With mock/ + hooksPath both
        // present, this leaves the alias slot empty (adoption=2/3,
        // partial) and the gate should deny.
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(
            dir.path(),
            "arvo",
            true,
            AliasShape::UnderOtherSection,
            true,
        );
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("git commit -m hi", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            out.contains("\"permissionDecision\":\"deny\""),
            "alias under non-[alias] section should not count; got: {out}",
        );
        assert!(
            out.contains("[alias] mock entry"),
            "deny reason should name the missing config marker; got: {out}",
        );
    }

    #[test]
    fn gate_e2e_non_git_command_is_silent_allow() {
        // Out of scope: command is not git commit / git push.
        let dir = tempfile::tempdir().unwrap();
        let repo = fake_repo(dir.path(), "arvo", true, AliasShape::None, false);
        let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
        let input = input_json("ls -la", &repo.to_string_lossy());
        let out = run_gate(&repos, &input);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "non-git command should be silent allow; got: {out}",
        );
    }

    // -------------------------------------------------------------------
    // The shared-config arm.
    //
    // Every case below puts the repo at zero mockspace adoption, so the
    // bootstrap arm contributes nothing and whatever the gate says comes from
    // the config check alone. Without that isolation a pass could be the
    // bootstrap arm firing and the config arm never running at all.
    // -------------------------------------------------------------------

    /// A PATH holding what the script shells out to, plus a `homma` behaving
    /// as `homma_body` says.
    ///
    /// `None` leaves `homma` off the PATH entirely, which is the case where
    /// the gate cannot ask at all.
    fn a_path_with_homma(dir: &Path, homma_body: Option<&str>) -> String {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let bin = dir.join("bin-with-homma");
        fs::create_dir_all(&bin).unwrap();
        // Everything the generated script shells out to, not only the pieces
        // this case is about. A PATH missing one of them fails somewhere
        // unrelated and quietly: without `sed` the deny payload is built and
        // then emitted with an empty reason, which still looks like a refusal
        // and tells the reader nothing.
        for program in ["bash", "dirname", "cat", "jq", "sed", "grep", "awk", "git"] {
            let found = Command::new("sh")
                .args(["-c", &format!("command -v {program}")])
                .output()
                .expect("no shell");
            let found = String::from_utf8_lossy(&found.stdout).trim().to_string();
            assert!(!found.is_empty(), "{program} is not on PATH");
            std::os::unix::fs::symlink(&found, bin.join(program)).unwrap();
        }
        if let Some(body) = homma_body {
            let at = bin.join("homma");
            fs::write(&at, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
            fs::set_permissions(&at, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = bin.to_string_lossy().into_owned();
        // That the stub is reachable, or absent, as the case intends. Assuming
        // either is how a test ends up covering the other branch in silence.
        let reachable = Command::new(bin.join("bash"))
            .args(["-c", "command -v homma"])
            .env("PATH", &path)
            .status()
            .unwrap()
            .success();
        assert_eq!(
            reachable,
            homma_body.is_some(),
            "the restricted PATH does not have the homma this case wants"
        );
        path
    }

    /// Run the gate from inside a real-shaped workspace.
    ///
    /// The script goes where the generated one goes, `<ws>/.claude/hooks/`,
    /// because it finds its workspace from its own location and a script
    /// anywhere else resolves to the wrong one. A `homma.toml` is written
    /// unless `with_manifest` is false, since that file is what tells the
    /// script there is a workspace to ask about at all.
    fn run_gate_in_a_workspace(homma_body: Option<&str>, with_manifest: bool) -> String {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        let hooks = ws.join(".claude").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        if with_manifest {
            fs::write(ws.join("homma.toml"), "content_repo = \"x\"\n").unwrap();
        }
        // Zero adoption on purpose: no mock/, no alias, no hooksPath.
        let repo = fake_repo(&ws, "arvo", false, AliasShape::None, false);

        let script = hooks.join("gate.sh");
        fs::write(
            &script,
            gate_script(&[("arvo".to_string(), "arvo".to_string())]),
        )
        .unwrap();

        let input = input_json("git commit -m x", &repo.to_string_lossy());
        let path = a_path_with_homma(dir.path(), homma_body);
        let mut child = Command::new("bash")
            .arg(&script)
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bash would not start");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("bash would not finish");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn gate_allows_when_the_shared_configs_are_in_order() {
        // `exit 0` is what `repo config check` does when nothing blocks.
        let out = run_gate_in_a_workspace(Some("exit 0"), true);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "a repo owing nothing was refused: {out}"
        );
    }

    #[test]
    fn gate_denies_when_a_required_config_is_missing_and_names_the_fix() {
        // Non-zero is what it does when something blocks, and what it printed
        // is what the person needs to see.
        let out = run_gate_in_a_workspace(
            Some("echo 'arvo'; echo '  deny.toml is missing, and is required here'; exit 1"),
            true,
        );
        assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
        assert!(out.contains("deny.toml is missing"), "{out}");
        assert!(
            out.contains("homma repo config init --repo arvo"),
            "the refusal did not name the command that fixes it: {out}"
        );
    }

    #[test]
    fn gate_denies_when_the_check_could_not_run_and_does_not_send_you_to_init() {
        // Exit 2 is homma saying it could not run, and the commonest reason on
        // this path is this workspace's own manifest failing to parse, which is
        // nothing the repo being committed to can fix. It still denies, for the
        // reason the arms around it deny. What it must not do is name `init`,
        // which reads the same manifest through the same function and fails
        // identically.
        let out = run_gate_in_a_workspace(
            Some("echo 'error: loading config from /w/homma.toml'; exit 2"),
            true,
        );
        assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
        assert!(
            out.contains("could not be checked"),
            "the refusal did not say the check never ran: {out}"
        );
        assert!(
            out.contains("loading config from"),
            "and did not carry what homma printed, which is the only clue: {out}"
        );
        assert!(
            out.contains("homma.toml"),
            "and did not name the manifest to read: {out}"
        );
        assert!(
            !out.contains("repo config init"),
            "it sent somebody to a command that fails on the same manifest: {out}"
        );
    }

    #[test]
    fn gate_denies_when_it_cannot_ask_at_all() {
        // The same reasoning as the missing-`jq` case: a check that could not
        // run must never be indistinguishable from one that ran and found
        // nothing. Allowing here would mean a workspace with no homma installed
        // silently stops enforcing anything.
        let out = run_gate_in_a_workspace(None, true);
        assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
        assert!(out.contains("homma is not on PATH"), "{out}");
    }

    #[test]
    fn gate_skips_the_config_check_where_there_is_no_workspace_to_ask_about() {
        // No manifest means no workspace, so there is nothing to resolve the
        // repo against. Refusing would be the gate reporting the absence of its
        // own input as a fault in somebody's repo.
        let out = run_gate_in_a_workspace(None, false);
        assert!(
            !out.contains("\"permissionDecision\":\"deny\""),
            "a repo was refused over the gate's own missing input: {out}"
        );
    }

    #[test]
    fn the_two_arms_are_reported_together_rather_than_one_hiding_the_other() {
        // A repo can be wrong on both counts and should hear about both. The
        // bootstrap arm needs partial adoption to fire, which is what this one
        // builds instead of the zero-adoption tree the others use.
        use std::io::Write;
        use std::process::{Command, Stdio};

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("workspace");
        let hooks = ws.join(".claude").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::write(ws.join("homma.toml"), "content_repo = \"x\"\n").unwrap();
        // Partial adoption: mock/ present, no alias, no hooksPath.
        let repo = fake_repo(&ws, "arvo", true, AliasShape::None, false);

        let script = hooks.join("gate.sh");
        fs::write(
            &script,
            gate_script(&[("arvo".to_string(), "arvo".to_string())]),
        )
        .unwrap();
        let path = a_path_with_homma(
            dir.path(),
            Some("echo '  deny.toml is missing, and is required here'; exit 1"),
        );
        let input = input_json("git commit -m x", &repo.to_string_lossy());
        let mut child = Command::new("bash")
            .arg(&script)
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bash would not start");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("bash would not finish");
        let out = String::from_utf8_lossy(&out.stdout).into_owned();

        assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
        assert!(
            out.contains("core.hooksPath is not set"),
            "the bootstrap arm is missing: {out}"
        );
        assert!(
            out.contains("deny.toml is missing"),
            "the config arm is missing: {out}"
        );
    }
}
