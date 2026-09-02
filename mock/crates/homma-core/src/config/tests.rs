//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tests for [`super`], beside it rather than inside it.
//!
//! Three modules, one per thing being asserted: what a token command resolves
//! to, what the schema accepts and rejects, and where a relative `deny` entry
//! lands. They were in `config.rs` until it passed the file-size limit, and the
//! seam was obvious because they are the only part of it that is not the schema.

#[cfg(test)]
mod token_command_tests {
    use crate::config::*;

    fn cfg(body: &str) -> Config {
        let mut c = Config::parse(body).unwrap();
        c.settle_token_commands(Path::new("/ws"));
        c
    }

    const TWO_FORGES: &str = r#"
[workspace]
name = "w"
[auth]
token_cmd = [".shared/scripts/release/auth", "token", "{forge}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
"#;

    #[test]
    fn one_line_serves_every_forge_because_the_placeholder_carries_the_name() {
        // The whole point of the default: the operator writes it once and each
        // profile asks about itself. A fixture with one forge cannot tell a
        // working substitution from a constant, so there are two.
        let c = cfg(TWO_FORGES);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap()[2], "github");
        assert_eq!(
            c.forges["codeberg"].token_cmd.as_ref().unwrap()[2],
            "codeberg"
        );
    }

    #[test]
    fn the_three_registries_exist_and_take_the_inherited_command_with_their_own_key() {
        let c = cfg(TWO_FORGES);
        for (key, host) in RegistryConfig::KNOWN {
            let reg = c
                .registry(key)
                .unwrap_or_else(|| panic!("{key} is missing"));
            let argv = reg.token_cmd.as_ref().unwrap();
            assert_eq!(argv[0], "/ws/.shared/scripts/release/auth");
            assert_eq!(argv[2], *key);
            assert!(
                !argv
                    .iter()
                    .any(|a| a.contains("{host}") || a.contains(host))
            );
            assert_eq!(reg.token_env, None);
        }
    }

    #[test]
    fn a_registrys_own_fields_are_kept_and_a_wrong_field_is_refused() {
        let c = cfg(&format!(
            "{TWO_FORGES}\n[registries.npm]\ntoken_env = \"NPM_TOKEN\"\ntoken_cmd = [\"op\", \"read\", \"{{forge}}\", \"{{host}}\"]\n"
        ));
        let npm = c.registry("npm").unwrap();
        assert_eq!(npm.token_env.as_deref(), Some("NPM_TOKEN"));
        assert_eq!(npm.token_cmd.as_ref().unwrap(), &[
            "op",
            "read",
            "npm",
            "registry.npmjs.org"
        ]);
        assert_eq!(
            c.registry("jsr").unwrap().token_cmd.as_ref().unwrap()[2],
            "jsr"
        );
        assert!(
            Config::parse(&format!("{TWO_FORGES}\n[registries.npm]\ntoken = \"x\"\n")).is_err()
        );
    }

    #[test]
    fn a_relative_program_path_is_anchored_to_the_workspace_root() {
        // Not to the working directory. `homma` is meant to run from inside a
        // member clone, where a path relative to cwd names nothing.
        let c = cfg(TWO_FORGES);
        assert_eq!(
            c.forges["github"].token_cmd.as_ref().unwrap()[0],
            "/ws/.shared/scripts/release/auth"
        );
    }

    #[test]
    fn a_bare_program_name_is_left_for_path_to_find() {
        // The control on the anchoring above: `gh` must stay `gh`, or the one
        // case that needs no configuration at all stops working.
        let c = cfg(r#"
[workspace]
name = "w"
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_cmd = ["gh", "auth", "token"]
"#);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap(), &[
            "gh", "auth", "token"
        ]);
    }

    #[test]
    fn a_forges_own_command_is_not_replaced_by_the_default() {
        let c = cfg(r#"
[workspace]
name = "w"
[auth]
token_cmd = ["shared", "{forge}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_cmd = ["gh", "auth", "token"]
[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
"#);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap(), &[
            "gh", "auth", "token"
        ]);
        // and the other one still inherits, which is what makes this a test
        // about precedence rather than about the default never applying
        assert_eq!(c.forges["codeberg"].token_cmd.as_ref().unwrap(), &[
            "shared", "codeberg"
        ]);
    }

    #[test]
    fn the_host_placeholder_is_the_api_host_and_not_the_public_one() {
        // They differ on GitHub, which is the case worth pinning: `github.com`
        // against `api.github.com`.
        let c = cfg(r#"
[workspace]
name = "w"
[auth]
token_cmd = ["t", "{host}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
"#);
        assert_eq!(
            c.forges["github"].token_cmd.as_ref().unwrap()[1],
            "api.github.com"
        );
    }

    #[test]
    fn a_manifest_naming_no_command_anywhere_gets_none() {
        // The control on all of the above: nothing is invented for a manifest
        // that asked for nothing, so an operator who never opts in never has a
        // subprocess run on their behalf.
        let c = cfg(r#"
[workspace]
name = "w"
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_env = "SOMETHING"
"#);
        assert!(c.forges["github"].token_cmd.is_none());
    }

    #[test]
    fn settling_twice_changes_nothing() {
        // `from_path` settles once, and a caller that parsed a string may
        // settle again. A second pass must not re-anchor an already absolute
        // path or substitute into a name that legitimately contains braces.
        let mut c = Config::parse(TWO_FORGES).unwrap();
        c.settle_token_commands(Path::new("/ws"));
        let once = c.forges["github"].token_cmd.clone();
        c.settle_token_commands(Path::new("/elsewhere"));
        assert_eq!(c.forges["github"].token_cmd, once);
    }
}

#[cfg(test)]
mod tests {
    use crate::config::*;

    #[test]
    fn a_relative_workspace_path_resolves_beside_the_config_not_beside_the_caller() {
        // The failure this exists to stop: the tracked `homma.toml` used to
        // carry an absolute path to one particular clone, so every repo lookup
        // from any other workspace resolved into that one, and the configs
        // stage would have written files into it.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("kamu-canon");
        std::fs::create_dir_all(&ws).unwrap();
        let at = ws.join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\n").unwrap();

        let cfg = Config::from_path(&at).unwrap();
        assert_eq!(
            cfg.workspace.path, ws,
            "the default did not anchor on the config"
        );
        assert!(cfg.workspace.path.is_absolute());
    }

    #[test]
    fn a_relative_config_path_still_yields_an_absolute_workspace_path() {
        // The half of the matrix the sibling test above does not reach: it
        // passes an absolute config path, so it never exercises the anchoring
        // it asserts. Every path in the program hangs off this one, and a
        // relative result is what made the aggregated hooks inert.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("kamu-canon");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();

        let relative = pathdiff_from(&std::env::current_dir().unwrap(), &ws.join("homma.toml"));
        let cfg = Config::from_path(&relative).unwrap();
        assert!(
            cfg.workspace.path.is_absolute(),
            "a relative config path left the workspace relative: {}",
            cfg.workspace.path.display()
        );
        assert_eq!(cfg.workspace.path, normalise(&ws));
    }

    #[test]
    fn a_bare_filename_config_path_is_the_case_the_fallback_never_covered() {
        // Named so a later reader does not restore the `unwrap_or(".")` as
        // sufficient. `Path::new("homma.toml").parent()` is `Some("")`, not
        // `None`, so that fallback is unreachable for the one spelling that
        // needs it.
        assert_eq!(Path::new("homma.toml").parent(), Some(Path::new("")));
        // And the control: the fallback does fire for a path with no filename
        // at all, which is the case it was written for.
        assert_eq!(Path::new("").parent(), None);
    }

    #[test]
    fn an_absolute_config_path_is_unaffected_by_the_working_directory() {
        // The control on the change: absolutising the config path must not
        // move a config that already named itself absolutely.
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\npath = \"repos\"\n").unwrap();
        assert_eq!(
            Config::from_path(&at).unwrap().workspace.path,
            normalise(&dir.path().join("repos"))
        );
    }

    /// A path to `target` expressed relative to `base`, for the one test that
    /// needs a relative config path and cannot change the working directory
    /// without racing every other test in the binary.
    fn pathdiff_from(base: &Path, target: &Path) -> PathBuf {
        let base = normalise(base);
        let target = normalise(target);
        let mut up = PathBuf::new();
        let mut probe = base.as_path();
        loop {
            if let Ok(rest) = target.strip_prefix(probe) {
                return up.join(rest);
            }
            match probe.parent() {
                Some(p) => {
                    up.push("..");
                    probe = p;
                },
                None => return target,
            }
        }
    }

    #[test]
    fn an_absolute_workspace_path_is_left_exactly_as_written() {
        // The control: naming a path explicitly still means that path. The
        // resolution is for the relative case only.
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(
            &at,
            "[workspace]\nname = \"w\"\npath = \"/somewhere/else\"\n",
        )
        .unwrap();
        assert_eq!(
            Config::from_path(&at).unwrap().workspace.path,
            PathBuf::from("/somewhere/else")
        );
    }

    #[test]
    fn a_relative_path_that_climbs_lands_where_it_reads() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("a").join("b");
        std::fs::create_dir_all(&inner).unwrap();
        let at = inner.join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\npath = \"../..\"\n").unwrap();
        assert_eq!(Config::from_path(&at).unwrap().workspace.path, dir.path());
    }

    #[test]
    fn parsing_a_string_leaves_the_path_alone_because_there_is_nothing_to_anchor_on() {
        // `parse` has no file, so it cannot resolve, and inventing the working
        // directory as an anchor would be the guess this whole change removes.
        let cfg = Config::parse("[workspace]\nname = \"w\"\npath = \"repos\"\n").unwrap();
        assert_eq!(cfg.workspace.path, PathBuf::from("repos"));
    }

    #[test]
    fn normalising_a_path_keeps_what_it_names() {
        assert_eq!(normalise(Path::new("/a/./b")), PathBuf::from("/a/b"));
        assert_eq!(normalise(Path::new("/a/b/..")), PathBuf::from("/a"));
        assert_eq!(normalise(Path::new("/a/b/../..")), PathBuf::from("/"));
        assert_eq!(normalise(Path::new("a/b/../c")), PathBuf::from("a/c"));
        // a leading climb has nothing to cancel against and is kept, rather
        // than silently becoming the relative root
        assert_eq!(normalise(Path::new("../a")), PathBuf::from("../a"));
        assert_eq!(normalise(Path::new("../../a")), PathBuf::from("../../a"));
        // and a path that cancels to nothing is still a path
        assert_eq!(normalise(Path::new("a/..")), PathBuf::from("."));
        assert_eq!(normalise(Path::new(".")), PathBuf::from("."));
    }
}

#[cfg(test)]
mod deny_anchor_tests {
    use crate::config::*;

    /// Write a manifest into a fresh directory and load it the way a run does.
    ///
    /// Through the file rather than through `parse`, because the anchoring is
    /// what `from_path` adds and a string has no directory to be anchored to.
    fn loaded(body: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(&at, body).unwrap();
        let cfg = Config::from_path(&at).unwrap();
        (dir, cfg)
    }

    #[test]
    fn a_relative_entry_is_anchored_to_the_manifest_rather_than_the_caller() {
        let (dir, cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
"#,
        );
        assert_eq!(cfg.deny[0].path, dir.path().join("scratch"));
        // The control: the working directory is a different place, and an entry
        // anchored there would name it instead.
        assert_ne!(
            cfg.deny[0].path,
            std::env::current_dir().unwrap().join("scratch")
        );
    }

    #[test]
    fn the_anchor_holds_when_the_workspace_points_somewhere_else() {
        // The case the two anchors diverged on. The registry resolved a relative
        // entry against the manifest's directory and the aggregation resolved it
        // against the workspace root, so one manifest denied two different
        // places depending on which command read it.
        let (dir, cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
path = "elsewhere"
"#,
        );
        assert_eq!(cfg.deny[0].path, dir.path().join("scratch"));
        assert_ne!(cfg.deny[0].path, cfg.workspace.path.join("scratch"));
    }

    #[test]
    fn a_home_entry_and_an_absolute_one_come_through_untouched() {
        // `~/` belongs to the home rather than the manifest, and resolving it
        // here as well as in `DenyEntry::resolve` is how the two come to
        // disagree. An absolute entry names its place already.
        let (_dir, cfg) = loaded(
            r#"
deny = ["~/work/someone-elses", "/var/tmp/nope"]
[workspace]
name = "w"
"#,
        );
        assert_eq!(cfg.deny[0].path, Path::new("~/work/someone-elses"));
        assert_eq!(cfg.deny[1].path, Path::new("/var/tmp/nope"));
    }

    #[test]
    fn settling_twice_lands_in_the_same_place() {
        // `from_path` has already settled it, so a caller that settles again
        // against a different directory must not push it further. Idempotence is
        // what makes the public method safe to call without knowing.
        let (dir, mut cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
"#,
        );
        let once = cfg.deny[0].path.clone();
        cfg.settle_deny(Path::new("/somewhere/else"));
        assert_eq!(cfg.deny[0].path, once);
        assert_eq!(once, dir.path().join("scratch"));
    }
}
