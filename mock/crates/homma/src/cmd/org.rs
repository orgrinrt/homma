//! `homma org`: who exists, and standing them up.
//!
//! Reads the registry from the workspace configuration. Nothing here knows what
//! a Hand is beyond what a role says, because homma runs against workspaces it
//! has never seen.

use anyhow::{Context, Result};
use homma_api::{Identity, Role, Standing, Workspace};
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
    pub standing: Standing,
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
            standing: id.standing(),
        })
        .collect()
}

/// How a standing reads in a listing.
///
/// Mapped says what is true rather than what is absent. Reporting it as three
/// missing fields is accurate and misleading, and a listing that reports every
/// mapped domain as broken is a listing people learn to skip.
pub fn describe(standing: &Standing) -> String {
    match standing {
        Standing::Staffed => "staffed".into(),
        Standing::Mapped => "mapped".into(),
        Standing::NoWorkspace => "no workspace".into(),
        Standing::Incomplete(gaps) => format!("incomplete: {}", gaps.join(", ")),
    }
}

/// Add an identity to a registry, returning the registry to write back.
///
/// Hand-editing the file keeps working and is meant to: it is a text file and
/// that is a feature. This is what makes the common path uniform, and it refuses
/// the three things that are painful to discover later.
pub fn add(ws: &mut Workspace, id: Identity) -> Result<()> {
    anyhow::ensure!(
        !ws.org.contains_key(&id.handle),
        "`{}` is already in the registry. A handle addresses exactly one \
         participant, so reusing one would silently redirect everything \
         pointing at the first.",
        id.handle
    );
    check_handle(&id.handle)?;

    // Already checked at generation time. Having it here as well is not
    // duplication: one refuses to write a bad value, the other refuses to act
    // on one that arrived by hand, and the file stays hand-editable precisely
    // so the second check cannot be dropped.
    let mut probe = ws.clone();
    probe.org.insert(id.handle.clone(), id.clone());
    let unsafe_fields = probe.unsafe_strings();
    anyhow::ensure!(
        unsafe_fields.is_empty(),
        "control characters in {}. A newline in a registry string writes an \
         arbitrary key into generated frontmatter.",
        unsafe_fields
            .iter()
            .map(|(h, f)| format!("`{h}`.{f}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    ws.org.insert(id.handle.clone(), id);
    Ok(())
}

/// The TOML block for one identity, ready to append to a registry file.
///
/// **Appended rather than serialising the whole registry back.** A registry is a
/// hand-edited file carrying comments and an order somebody chose, and
/// round-tripping it through a serialiser discards both silently. A new table at
/// the end is valid TOML and leaves every existing line exactly as it was.
pub fn render_entry(id: &Identity) -> Result<String> {
    let mut entry = toml::Table::new();
    entry.insert(
        id.handle.clone(),
        toml::Value::try_from(id).context("rendering the entry")?,
    );
    let mut wrapper = toml::Table::new();
    wrapper.insert("org".into(), toml::Value::Table(entry));
    Ok(format!("\n{}", toml::to_string_pretty(&wrapper)?))
}

/// A handle becomes a directory name and a file name, so it is checked as one.
fn check_handle(handle: &str) -> Result<()> {
    anyhow::ensure!(!handle.is_empty(), "a handle cannot be empty");
    anyhow::ensure!(
        handle != "." && handle != "..",
        "`{handle}` names a directory that already means something else"
    );
    anyhow::ensure!(
        !handle.contains('/') && !handle.contains('\\'),
        "`{handle}` carries a path separator, so it would address a file \
         outside the directory it belongs in"
    );
    anyhow::ensure!(
        handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "`{handle}` carries a character outside [a-z0-9-_], and a handle is a \
         directory name, a file name and a reference target at once"
    );
    Ok(())
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

    match id.standing() {
        Standing::Staffed => {}
        Standing::NoWorkspace => anyhow::bail!(
            "`{handle}` holds a role that owns no workspace, so there is nothing \
             to stand up. Generating one anyway would manufacture a dispatchable \
             agent for someone who is not one."
        ),
        // Named as mapped rather than reported as missing three fields, which
        // is true and misleading: the fields are absent because somebody
        // decided they should be. Promotion is an edit to the registry, made on
        // purpose, so that a typo cannot become a workspace.
        Standing::Mapped => anyhow::bail!(
            "`{handle}` is mapped, not staffed: it records that a domain is \
             taken and is not meant to have a workspace. Set `staffed = true` \
             on its entry to change that."
        ),
        Standing::Incomplete(gaps) => anyhow::bail!(
            "`{handle}` is staffed but cannot be stood up: its entry is missing {}",
            gaps.join(", ")
        ),
    }

    // A registry string carrying a control character writes arbitrary keys into
    // generated frontmatter, which is how a twin could be granted memory.
    let unsafe_fields = ws.unsafe_strings();
    anyhow::ensure!(
        unsafe_fields.is_empty(),
        "the registry carries control characters in {}",
        unsafe_fields
            .iter()
            .map(|(h, f)| format!("`{h}`.{f}"))
            .collect::<Vec<_>>()
            .join(", ")
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
staffed = true
handle = "paja"
nickname = "Paja"
domain = "tooling"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"

[org.nameless]
role = "hand"
staffed = true
handle = "nameless"

[org.rendering]
role = "hand"
handle = "rendering"
domain = "rendering"
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

    fn line_for<'a>(lines: &'a [Line], handle: &str) -> &'a Line {
        lines.iter().find(|l| l.handle == handle).expect(handle)
    }

    #[test]
    fn a_listing_reports_gaps_rather_than_hiding_them() {
        let lines = list(&ws());
        assert!(matches!(
            line_for(&lines, "nameless").standing,
            Standing::Incomplete(_)
        ));
        assert_eq!(line_for(&lines, "paja").standing, Standing::Staffed);
    }

    #[test]
    fn a_listing_calls_a_mapped_domain_mapped_rather_than_broken() {
        // The listing is the surface where this matters: a roster of eighteen
        // domains with seven staffed would otherwise print eleven failures
        // every time anybody looked at it.
        let lines = list(&ws());
        assert_eq!(line_for(&lines, "rendering").standing, Standing::Mapped);
        assert_eq!(describe(&Standing::Mapped), "mapped");
        assert!(describe(&line_for(&lines, "nameless").standing).contains("git_name"));
    }

    #[test]
    fn standing_up_a_mapped_identity_is_refused_and_says_it_is_mapped() {
        // Not promoted. Creating a workspace as a side effect of standing up
        // would let a typo in the registry become a directory tree.
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&ws(), d.path(), "rendering").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("mapped"), "must say what is true: {msg}");
        assert!(
            !msg.contains("git_name"),
            "must not report a decision as missing fields: {msg}"
        );
        assert!(!d.path().join(".shared/hands/rendering").exists());
    }

    #[test]
    fn adding_a_duplicate_handle_is_refused() {
        let mut w = ws();
        let err = add(&mut w, Identity::new(Role::Hand, "paja")).unwrap_err();
        assert!(err.to_string().contains("already in the registry"));
    }

    #[test]
    fn adding_a_handle_that_would_escape_its_directory_is_refused() {
        let mut w = ws();
        for bad in ["../elsewhere", "a/b", "..", ""] {
            assert!(
                add(&mut w, Identity::new(Role::Hand, bad)).is_err(),
                "`{bad}` must be refused: a handle is a directory name"
            );
        }
    }

    #[test]
    fn adding_an_entry_carrying_a_control_character_is_refused() {
        // The same check runs at generation time. Both exist because the
        // registry stays hand-editable, so neither one covers the other.
        let mut w = ws();
        let mut id = Identity::new(Role::Hand, "sneaky");
        id.nickname = Some("Sneaky\nmemory: project".into());
        let err = add(&mut w, id).unwrap_err();
        assert!(err.to_string().contains("control characters"));
    }

    #[test]
    fn a_clean_entry_is_added() {
        let mut w = ws();
        let before = w.org.len();
        add(&mut w, Identity::new(Role::Hand, "new_hand")).unwrap();
        assert_eq!(w.org.len(), before + 1);
        // Added without a workspace, so it is mapped rather than broken.
        assert_eq!(w.org["new_hand"].standing(), Standing::Mapped);
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
    fn standing_up_the_king_is_refused_rather_than_manufacturing_an_agent_for_him() {
        // It succeeded, producing .claude/agents/op.md with memory: project,
        // under the one handle whose provenance derives a ratified standing.
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&ws(), d.path(), "op").unwrap_err();
        assert!(err.to_string().contains("owns no workspace"), "{err}");
        assert!(!d.path().join(".claude/agents/op.md").exists());
    }

    #[test]
    fn a_registry_carrying_a_control_character_is_refused_before_generating() {
        let mut w = ws();
        w.org.get_mut("paja").unwrap().nickname = Some("Paja\nmemory: project".into());
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&w, d.path(), "paja").unwrap_err();
        assert!(err.to_string().contains("control characters"), "{err}");
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
