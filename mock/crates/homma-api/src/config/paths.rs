//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where homma keeps things in a workspace.

use serde::{Deserialize, Serialize};

use crate::path::RelPath;

/// Where homma keeps things in a workspace.
///
/// Every path has a default homma will create and an override for a workspace
/// whose convention differs. Nothing about the workspace homma was born in is
/// compiled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub hands:          RelPath,
    pub experts:        RelPath,
    pub channels:       RelPath,
    pub agents:         RelPath,
    pub index:          RelPath,
    /// Where `homma capture` writes what the person said to a session.
    pub captures:       RelPath,
    /// The command every tag, release and remote branch deletion of the
    /// workspace goes through, which `homma release run` asks for a release's
    /// tag and its forge release. Unset, the release tags with git and
    /// releases through the forge client.
    pub forge_route:    Option<RelPath>,
    /// The command that reads a repository's whole history for attribution
    /// the workspace forbids, which `homma release run` runs over each repo
    /// with its root as the one argument, refusing the release on any exit
    /// but 0. Unset, a release scans nothing.
    pub history_scan:   Option<RelPath>,
    /// The command that renders the workspace's own rules, skills and
    /// personas, which `homma agent regen` runs as `<command> regen` from the
    /// workspace root after its aggregate. Unset, the regen renders none of
    /// them and says so.
    pub agent_surfaces: Option<RelPath>,
    /// The git-shook manifest declaring the workspace's own git hooks, which
    /// `homma hook install` places in each member for git-shook to register
    /// beside homma's release gate. homma reads its `name` and nothing more.
    pub git_hooks:      RelPath,
    /// The rendered index of every agent address, one line each: the address,
    /// the handle and the role, tab-separated.
    pub identities:     RelPath,
    /// The directory of lease records, one `<handle>.toml` per held identity.
    pub leases:         RelPath,
}

impl Paths {
    /// The keys of a raw `[paths]` table that this type does not read, sorted.
    ///
    /// The keys it reads are the ones the key probe lists, so a field added
    /// here stops being reported with no other edit. A value that is no table
    /// has none.
    ///
    /// # Errors
    ///
    /// When the probe cannot read this type, which it reads with no input, so
    /// on every call alike.
    pub fn unread(raw: &toml::Value) -> Result<Vec<String>, serde::de::value::Error> {
        let known = crate::keys_of::<Paths>()?;
        let Some(table) = raw.as_table() else { return Ok(Vec::new()) };
        let mut unread: Vec<String> = table
            .keys()
            .filter(|k| !known.read().contains(k.as_str()))
            .cloned()
            .collect();
        unread.sort_unstable();
        Ok(unread)
    }
}

impl Default for Paths {
    fn default() -> Self {
        // Every one of these is a literal in this file, so `expect` here says
        // "this source is wrong" rather than "the configuration is wrong",
        // which is the only way it can fail.
        let rel = |s: &str| RelPath::new(s).expect("a default path is relative and contained");
        Self {
            hands:          rel(".shared/hands"),
            experts:        rel(".shared/experts"),
            channels:       rel(".shared/channels"),
            agents:         rel(".claude/agents"),
            index:          rel(".shared/.index"),
            captures:       rel(".data/op-responses"),
            forge_route:    None,
            history_scan:   None,
            agent_surfaces: None,
            git_hooks:      rel("shook.toml"),
            identities:     rel(".shared/.index/identities.tsv"),
            leases:         rel(".shared/state/leases"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MINIMAL, WITH_ORG, Workspace};

    #[test]
    fn every_path_defaults_and_creating_none_of_them_is_valid() {
        let w = Workspace::parse(MINIMAL).unwrap();
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new(".shared/hands")
        );
        assert_eq!(
            w.paths.channels.as_path(),
            std::path::Path::new(".shared/channels")
        );
        assert_eq!(
            w.paths.captures.as_path(),
            std::path::Path::new(".data/op-responses")
        );
    }

    #[test]
    fn the_capture_store_is_a_configured_path_like_the_others() {
        let toml = "content_repo = \"local\"\n\n[paths]\ncaptures = \"notes/said\"\n";
        let w = Workspace::parse(toml).expect("a contained store is fine");
        assert_eq!(
            w.paths.captures.as_path(),
            std::path::Path::new("notes/said")
        );
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new(".shared/hands")
        );
        // And it is refused where it leaves, like every other one.
        let bad = "content_repo = \"local\"\n\n[paths]\ncaptures = \"../elsewhere\"\n";
        assert!(Workspace::parse(bad).is_err());
    }

    #[test]
    fn the_forge_route_is_unset_by_default_and_a_contained_path_where_named() {
        assert!(
            Workspace::parse(MINIMAL)
                .unwrap()
                .paths
                .forge_route
                .is_none()
        );
        // the workspace's own manifest names its stale lists beside it, which homma does not read
        let toml = "content_repo = \"local\"\n\n[paths]\nforge_route = \".shared/scripts/remote/forge-route\"\nstale_branches = \"x.txt\"\n";
        let w = Workspace::parse(toml).expect("a contained route is fine");
        assert_eq!(
            w.paths.forge_route.as_ref().map(|p| p.as_path()),
            Some(std::path::Path::new(".shared/scripts/remote/forge-route"))
        );
        let bad = "content_repo = \"local\"\n\n[paths]\nforge_route = \"/usr/bin/route\"\n";
        assert!(
            Workspace::parse(bad).is_err(),
            "a route outside the workspace is refused"
        );
    }

    #[test]
    fn the_history_scan_is_unset_by_default_and_a_contained_path_where_named() {
        // Mutation: a default scanner compiled in, which is a copy of the
        // workspace's policy homma does not own.
        assert!(
            Workspace::parse(MINIMAL)
                .unwrap()
                .paths
                .history_scan
                .is_none()
        );
        let toml = "content_repo = \"local\"\n\n[paths]\nhistory_scan = \".shared/skills/pr-review/scripts/scan-attribution.sh\"\n";
        let w = Workspace::parse(toml).expect("a contained scanner is fine");
        assert_eq!(
            w.paths.history_scan.as_ref().map(|p| p.as_path()),
            Some(std::path::Path::new(
                ".shared/skills/pr-review/scripts/scan-attribution.sh"
            ))
        );
        let bad = "content_repo = \"local\"\n\n[paths]\nhistory_scan = \"/usr/bin/scan\"\n";
        assert!(
            Workspace::parse(bad).is_err(),
            "a scanner outside the workspace is refused"
        );
    }

    #[test]
    fn an_override_replaces_only_what_it_names() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new("custom/hands")
        );
        // The others keep their defaults rather than vanishing.
        assert_eq!(
            w.paths.experts.as_path(),
            std::path::Path::new(".shared/experts")
        );
    }

    #[test]
    fn a_configured_path_that_leaves_the_workspace_is_refused_when_parsed() {
        // The seventh instance of one class: `hands` and `agents` are joined
        // onto the workspace root and were unvalidated strings, so both an
        // escaping and an absolute value wrote a Hand's directories and
        // definitions into an unrelated repository's tree, exit 0.
        for bad in ["../victim/stolen", "/etc/passwd-ish", "a/../../out"] {
            let toml = format!("content_repo = \"local\"\n\n[paths]\nhands = \"{bad}\"\n");
            assert!(
                Workspace::parse(&toml).is_err(),
                "`{bad}` must be refused where somebody can act on it"
            );
        }
    }

    #[test]
    fn a_configured_path_that_stays_inside_is_accepted() {
        // Including one that climbs and comes back, which is contained.
        let toml = "content_repo = \"local\"\n\n[paths]\nhands = \"a/../b/hands\"\n";
        let w = Workspace::parse(toml).expect("contained paths are fine");
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new("a/../b/hands")
        );
    }
}
