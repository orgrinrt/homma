//! `homma org`: who exists, and standing them up.
//!
//! Reads the registry from the workspace configuration. Nothing here knows what
//! a Hand is beyond what a role says, because homma runs against workspaces it
//! has never seen.

use anyhow::{Context, Result};
use homma_api::{Role, Workspace};
use homma_org::{prepare, write_definitions, Layout, Registry};
use std::path::{Path, PathBuf};

/// The standing instruction every generated definition carries.
///
/// One source, so it cannot drift between participants: none of them owns it.
const DISCIPLINE: &str = "\
## Standing

Read the workspace rules before touching anything, and prefer the precedent \
already in the tree over inventing a convention.

Work happens in your own workspace. The central clone is for reading.

Never commit to a trunk. Branch first, always: `dev` and `main` are landed \
through review, never written to directly. If you find yourself on one, you are \
one command away from a mistake nobody can undo cleanly.

Name the paths on a commit rather than relying on what you staged, because a \
commit takes the whole index.

When something needs the human, say what the options are. An answer to options \
nobody wrote down cannot be acted on later.";

/// Load a workspace's configuration.
pub fn load(path: &Path) -> Result<Workspace> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Workspace::parse(&text).with_context(|| format!("parsing {}", path.display()))
}

/// One line per participant, for a listing.
#[derive(Debug)]
pub struct Line {
    pub handle: String,
    pub role: Role,
    pub domain: String,
    pub gaps: Vec<&'static str>,
}

pub fn list(ws: &Workspace) -> Vec<Line> {
    let registry = Registry::new(ws);
    registry
        .all()
        .into_iter()
        .map(|id| Line {
            handle: id.handle.clone(),
            role: id.role,
            domain: id.domain.clone().unwrap_or_default(),
            gaps: id.missing(),
        })
        .collect()
}

/// What standing an identity up produced.
#[derive(Debug)]
pub struct StoodUp {
    pub handle: String,
    pub home: PathBuf,
    pub definition: PathBuf,
    pub twin_definition: PathBuf,
}

/// Create the directories, link the memory, and write both definitions.
///
/// Refuses an identity whose entry cannot support a workspace, naming what is
/// missing, rather than creating half of one and failing later.
pub fn stand_up(ws: &Workspace, root: &Path, handle: &str) -> Result<StoodUp> {
    let registry = Registry::new(ws);
    let id = registry
        .get(handle)
        .with_context(|| format!("no identity `{handle}` in the registry"))?;

    let gaps = id.missing();
    anyhow::ensure!(
        gaps.is_empty(),
        "`{handle}` cannot be stood up: its entry is missing {}",
        gaps.join(", ")
    );

    let layout = Layout::new(root, &ws.paths);
    let prepared =
        prepare(&layout, id).with_context(|| format!("preparing the workspace for `{handle}`"))?;
    let (prime, twin) = write_definitions(&layout, id, DISCIPLINE)
        .with_context(|| format!("generating definitions for `{handle}`"))?;

    Ok(StoodUp {
        handle: id.handle.clone(),
        home: prepared.home,
        definition: prime,
        twin_definition: twin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG: &str = r#"
content_repo = "clause-dev"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
handle = "paja"
nickname = "Paja"
domain = "tooling"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"

[org.nameless]
role = "hand"
handle = "nameless"
"#;

    fn ws() -> Workspace {
        Workspace::parse(ORG).unwrap()
    }

    #[test]
    fn the_standing_instruction_forbids_committing_to_a_trunk() {
        // Found end to end: the first Hand stood up committed straight to main,
        // because nothing in what it was given said not to. It had not pushed,
        // so nothing was lost, and the gap was in the instruction rather than
        // in the Hand.
        assert!(DISCIPLINE.contains("Never commit to a trunk"));
        assert!(DISCIPLINE.contains("Branch first"));
    }

    #[test]
    fn a_listing_reports_gaps_rather_than_hiding_them() {
        let lines = list(&ws());
        let nameless = lines.iter().find(|l| l.handle == "nameless").unwrap();
        assert!(!nameless.gaps.is_empty());
        let paja = lines.iter().find(|l| l.handle == "paja").unwrap();
        assert!(paja.gaps.is_empty());
    }

    #[test]
    fn standing_up_an_incomplete_identity_is_refused_with_the_reason() {
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&ws(), d.path(), "nameless").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("git_name"), "must say what is missing: {msg}");
        assert!(
            !d.path().join(".shared/hands/nameless").exists(),
            "a refusal must not leave half a workspace behind"
        );
    }

    #[test]
    fn standing_up_an_unknown_handle_is_refused() {
        let d = tempfile::tempdir().unwrap();
        assert!(stand_up(&ws(), d.path(), "stranger").is_err());
    }

    #[test]
    fn standing_up_produces_both_definitions_and_a_linked_memory() {
        let d = tempfile::tempdir().unwrap();
        let out = stand_up(&ws(), d.path(), "paja").unwrap();
        assert!(out.definition.exists());
        assert!(out.twin_definition.exists());

        let prime = std::fs::read_to_string(&out.definition).unwrap();
        let twin = std::fs::read_to_string(&out.twin_definition).unwrap();
        assert!(prime.contains("memory: project"));
        assert!(!twin.contains("memory:"));
        assert!(prime.contains(DISCIPLINE.lines().next().unwrap()));

        let link = d.path().join(".claude/agent-memory/paja");
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::write(link.join("MEMORY.md"), "x").unwrap();
        assert!(out.home.join("memory/MEMORY.md").exists());
    }

    #[test]
    fn standing_up_twice_is_the_same_answer() {
        let d = tempfile::tempdir().unwrap();
        let a = stand_up(&ws(), d.path(), "paja").unwrap();
        std::fs::write(a.home.join("memory/MEMORY.md"), "kept").unwrap();
        let b = stand_up(&ws(), d.path(), "paja").unwrap();
        assert_eq!(a.home, b.home);
        assert_eq!(
            std::fs::read_to_string(b.home.join("memory/MEMORY.md")).unwrap(),
            "kept",
            "standing up again must not clear what is remembered"
        );
    }
}
