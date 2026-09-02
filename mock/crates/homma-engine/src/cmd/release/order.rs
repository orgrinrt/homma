//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Which repos a workspace-wide release takes, in what order, and the edges
//! between them read off their manifests.

use std::path::Path;

use homma_core::Config;
use homma_core::release::publish;

/// The order the repos release in when none is named.
///
/// FIXME: name order. The deep dive asks for dependency order across repos,
/// so a member lands before the one that depends on it. `sibling_dependency`
/// below already reads the edges out of each manifest; what is left is the
/// ordering over them, the way `publish::crate_order` orders the crates of
/// one workspace. Until then `run_cmd` refuses a workspace-wide run that has
/// such an edge in it. The gap is a red test below, tracked in the workspace
/// agenda as `homma-release-orders-repos-by-dependency`.
pub(super) fn release_order(cfg: &Config) -> Vec<String> {
    cfg.repos.keys().cloned().collect()
}

/// The first sibling member `root`'s manifests depend on, if any: a
/// `Cargo.toml` dependency in the root or any workspace member whose git url
/// ends in the sibling's name or whose package name is the sibling's, or a
/// `deno.json` import whose specifier carries the sibling's name as a path
/// segment.
pub(super) fn sibling_dependency(root: &Path, siblings: &[String]) -> Option<String> {
    let names_git = |url: &str, s: &str| {
        let url = url.trim_end_matches('/').trim_end_matches(".git");
        url.rsplit('/').next() == Some(s)
    };
    // the same member listing the publish walks, so an edge declared in a
    // member crate is seen; the root goes first, since it is not listed
    // where it names no package of its own
    let mut dirs: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    dirs.extend(publish::crate_dirs(root).into_values());
    dirs.dedup();
    for dir in dirs {
        let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let Ok(doc) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        for table in ["dependencies", "build-dependencies", "dev-dependencies"] {
            let Some(t) = doc.get(table).and_then(|d| d.as_table()) else {
                continue;
            };
            for (key, value) in t {
                let package = value
                    .get("package")
                    .and_then(|p| p.as_str())
                    .unwrap_or(key.as_str());
                let git = value.get("git").and_then(|g| g.as_str()).unwrap_or("");
                if let Some(s) = siblings.iter().find(|s| *s == package || names_git(git, s)) {
                    return Some(s.clone());
                }
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("deno.json")) {
        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) {
            let imports = doc
                .get("imports")
                .and_then(|i| i.as_object())
                .cloned()
                .unwrap_or_default();
            for spec in imports.values().filter_map(|v| v.as_str()) {
                let segments: Vec<&str> = spec
                    .split(['/', '@', ':'])
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(s) = siblings.iter().find(|s| segments.contains(&s.as_str())) {
                    return Some(s.clone());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn git(path: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// A member clone at `root/name` whose manifest names `deps` as git
    /// dependencies on sibling members.
    fn member(root: &Path, name: &str, deps: &[&str]) {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        git(&path, &["init", "-q"]);
        git(&path, &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/orgrinrt/{name}.git"),
        ]);
        let mut manifest =
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
        for d in deps {
            manifest.push_str(&format!(
                "{d} = {{ git = \"https://github.com/orgrinrt/{d}.git\" }}\n"
            ));
        }
        std::fs::write(path.join("Cargo.toml"), manifest).unwrap();
    }

    fn workspace(dir: &Path) -> Config {
        std::fs::write(
            dir.join("homma.toml"),
            "[workspace]\nname = \"t\"\npath = \".\"\n\n[forges.github]\nkind = \"github\"\nbase_url = \"https://github.com\"\napi_url = \"https://api.github.com\"\n",
        )
        .unwrap();
        Config::from_path(&dir.join("homma.toml")).unwrap()
    }

    #[test]
    #[ignore = "catalogue: repos release in name order where the deep dive asks for dependency order across repos; tracked homma-release-orders-repos-by-dependency"]
    fn a_repo_releases_after_the_sibling_it_depends_on_whatever_their_names() {
        let dir = tempfile::tempdir().unwrap();
        // `alpha` depends on `zed`, so `zed` lands first, against the
        // alphabet
        member(dir.path(), "zed", &[]);
        member(dir.path(), "alpha", &["zed"]);
        let cfg = workspace(dir.path());
        assert_eq!(release_order(&cfg), vec![
            "zed".to_string(),
            "alpha".to_string()
        ]);
    }

    #[test]
    fn a_sibling_dependency_is_read_off_a_git_url_a_package_name_or_a_deno_import() {
        let dir = tempfile::tempdir().unwrap();
        let siblings = vec!["zed".to_string(), "alpha".to_string()];
        // a git url naming the sibling
        member(dir.path(), "alpha", &["zed"]);
        assert_eq!(
            sibling_dependency(&dir.path().join("alpha"), &siblings),
            Some("zed".into())
        );
        // no edge at all
        member(dir.path(), "zed", &[]);
        assert_eq!(sibling_dependency(&dir.path().join("zed"), &siblings), None);
        // a renamed dependency counts by its package name, from any table
        std::fs::write(
            dir.path().join("zed/Cargo.toml"),
            "[package]\nname = \"zed\"\nversion = \"0.1.0\"\n[dev-dependencies]\nother = { package = \"alpha\", version = \"0.1\" }\n",
        )
        .unwrap();
        assert_eq!(
            sibling_dependency(&dir.path().join("zed"), &siblings),
            Some("alpha".into())
        );
        // a foreign git url that merely contains the name as a substring does not
        std::fs::write(
            dir.path().join("zed/Cargo.toml"),
            "[package]\nname = \"zed\"\nversion = \"0.1.0\"\n[dependencies]\nx = { git = \"https://github.com/someone/alphabet.git\" }\n",
        )
        .unwrap();
        assert_eq!(sibling_dependency(&dir.path().join("zed"), &siblings), None);
        // a deno import carrying the sibling's name as a segment
        let d = dir.path().join("deno-one");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("deno.json"),
            r#"{"name": "@h/deno-one", "imports": {"zed": "jsr:@orgrinrt/zed@^0.1"}}"#,
        )
        .unwrap();
        assert_eq!(sibling_dependency(&d, &siblings), Some("zed".into()));
        std::fs::write(
            d.join("deno.json"),
            r#"{"name": "@h/deno-one", "imports": {"std": "jsr:@std/path@^1"}}"#,
        )
        .unwrap();
        assert_eq!(sibling_dependency(&d, &siblings), None);
        // an edge declared in a workspace member, with a root that names no
        // package of its own, is seen through the member listing
        let w = dir.path().join("ws");
        std::fs::create_dir_all(w.join("crates/inner")).unwrap();
        std::fs::write(
            w.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        std::fs::write(
            w.join("crates/inner/Cargo.toml"),
            "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n[dependencies]\nzed = { git = \"https://github.com/orgrinrt/zed.git\" }\n",
        )
        .unwrap();
        assert_eq!(sibling_dependency(&w, &siblings), Some("zed".into()));
        std::fs::write(
            w.join("crates/inner/Cargo.toml"),
            "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(sibling_dependency(&w, &siblings), None);
    }

    #[test]
    fn a_member_crate_named_for_its_own_repo_is_not_a_sibling_edge() {
        let dir = tempfile::tempdir().unwrap();
        // the repo `notko` is a workspace whose members include a crate
        // called `notko`, which `notko-hlist` depends on
        let w = dir.path().join("notko");
        std::fs::create_dir_all(w.join("crates/notko")).unwrap();
        std::fs::create_dir_all(w.join("crates/notko-hlist")).unwrap();
        std::fs::write(
            w.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        std::fs::write(
            w.join("crates/notko/Cargo.toml"),
            "[package]\nname = \"notko\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            w.join("crates/notko-hlist/Cargo.toml"),
            "[package]\nname = \"notko-hlist\"\nversion = \"0.1.0\"\n[dependencies]\nnotko = { path = \"../notko\" }\n",
        )
        .unwrap();
        // with its own name in the slice it matches itself, which is why the
        // caller takes the name out first
        assert_eq!(
            sibling_dependency(&w, &["notko".to_string(), "zed".to_string()]),
            Some("notko".into())
        );
        assert_eq!(sibling_dependency(&w, &["zed".to_string()]), None);
    }

    #[test]
    fn independent_repos_release_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        member(dir.path(), "zed", &[]);
        member(dir.path(), "alpha", &[]);
        let cfg = workspace(dir.path());
        assert_eq!(release_order(&cfg), vec![
            "alpha".to_string(),
            "zed".to_string()
        ]);
    }
}
