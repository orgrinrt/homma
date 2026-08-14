//! `homma org`: who exists, and standing them up.
//!
//! Reads the registry from the workspace configuration. Nothing here knows what
//! a Hand is beyond what a role says, because homma runs against workspaces it
//! has never seen.

use anyhow::{Context, Result};
use homma_api::{Git, Identity, Role, Staffing, Workspace};
use homma_org::{prepare, provision, write_definitions, Layout, Registry};
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
    pub staffing: Staffing,
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
            staffing: id.staffing(),
        })
        .collect()
}

/// How a staffing state reads in a listing.
///
/// Mapped says what is true rather than what is absent. Reporting it as three
/// missing fields is accurate and misleading, and a listing that reports every
/// mapped domain as broken is a listing people learn to skip.
pub fn describe(staffing: &Staffing) -> String {
    match staffing {
        Staffing::Staffed => "staffed".into(),
        Staffing::Mapped => "mapped".into(),
        Staffing::NoWorkspace => "no workspace".into(),
        Staffing::Incomplete(gaps) => format!("incomplete: {}", gaps.join(", ")),
    }
}

/// Add an identity to a registry, in memory.
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
    /// The workspace clone itself, which is what the identity commits in.
    pub workspace: PathBuf,
    /// False when the workspace already held the content repository.
    pub cloned: bool,
    pub definition: PathBuf,
    pub twin_definition: PathBuf,
}

/// Clone the content repository, set the identity in that clone, create the
/// directories, link the memory, and write both definitions.
///
/// Refuses an identity whose entry cannot support a workspace, naming what is
/// missing, rather than creating half of one and failing later.
///
/// **The clone and the identity were built and left unwired for one round**, so
/// `up` reported success having created directories and no workspace, while the
/// definition it generated told the Hand it committed under an identity nothing
/// had set. Every test passed, because each tested a function nothing called.
pub fn stand_up<G: Git>(ws: &Workspace, root: &Path, handle: &str, git: &G) -> Result<StoodUp> {
    let registry = Registry::new(ws);
    let id = registry
        .get(handle)
        .with_context(|| format!("no identity `{handle}` in the registry"))?;

    match id.staffing() {
        Staffing::Staffed => {}
        Staffing::NoWorkspace => anyhow::bail!(
            "`{handle}` holds a role that owns no workspace, so there is nothing \
             to stand up. Generating one anyway would manufacture a dispatchable \
             agent for someone who is not one."
        ),
        // Named as mapped rather than reported as missing three fields, which
        // is true and misleading: the fields are absent because somebody
        // decided they should be. Promotion is an edit to the registry, made on
        // purpose, so that a typo cannot become a workspace.
        Staffing::Mapped => anyhow::bail!(
            "`{handle}` is mapped, not staffed: it records that a domain is \
             taken and is not meant to have a workspace. Set `staffed = true` \
             on its entry to change that."
        ),
        Staffing::Incomplete(gaps) => anyhow::bail!(
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

    // The content repository's URL is the workspace root's own `origin`. homma
    // runs inside a clone of it, so the URL is already on disk and correct, and
    // a configuration key would be a second place for one fact to disagree.
    let url = git
        .origin_url(root)
        .map_err(|e| anyhow::anyhow!("reading the origin of {}: {e}", root.display()))?
        .with_context(|| {
            format!(
                "{} has no `origin`, so there is nothing to clone `{handle}`'s \
                 workspace from. homma stands identities up from inside a clone \
                 of the content repository.",
                root.display()
            )
        })?;

    // Before the directories, so a refusal here leaves nothing half-built.
    let provisioned =
        provision(id, &url, git).map_err(|e| anyhow::anyhow!("provisioning `{handle}`: {e}"))?;

    let layout = Layout::new(root, &ws.paths);
    let prepared =
        prepare(&layout, id).with_context(|| format!("preparing the workspace for `{handle}`"))?;
    let (prime, twin) = write_definitions(&layout, id, DISCIPLINE)
        .with_context(|| format!("generating definitions for `{handle}`"))?;

    Ok(StoodUp {
        handle: id.handle.clone(),
        home: prepared.home,
        workspace: provisioned.root,
        cloned: provisioned.cloned,
        definition: prime,
        twin_definition: twin,
    })
}

/// Write a registry back, through a temporary file and a rename.
///
/// Appending directly leaves a truncated table when a write is short, and every
/// later invocation then fails to parse a registry with no backup. Re-parsed
/// afterwards, because a file homma writes and cannot read is worse than one it
/// refuses to write.
pub fn append_entry(path: &Path, id: &Identity) -> Result<()> {
    let existing =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let next = format!("{existing}{}", render_entry(id)?);

    Workspace::parse(&next).with_context(|| {
        format!(
            "the registry would not parse after adding `{}`, so nothing was written",
            id.handle
        )
    })?;

    let temp = path.with_extension("toml.writing");
    std::fs::write(&temp, &next).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
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

    /// Enough git to exercise `stand_up`'s own logic. What a real repository
    /// does with an identity is tested where the real implementation lives.
    struct FakeGit {
        origin: Option<String>,
        cloned: std::cell::RefCell<Vec<PathBuf>>,
        identities: std::cell::RefCell<Vec<(PathBuf, String, String)>>,
    }

    impl FakeGit {
        fn with_origin() -> Self {
            Self {
                origin: Some("git@example.invalid:orgrinrt/content.git".into()),
                cloned: Default::default(),
                identities: Default::default(),
            }
        }
        fn without_origin() -> Self {
            Self {
                origin: None,
                cloned: Default::default(),
                identities: Default::default(),
            }
        }
    }

    #[derive(Debug)]
    struct Never;
    impl std::fmt::Display for Never {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "never")
        }
    }
    impl std::error::Error for Never {}

    impl Git for FakeGit {
        type Error = Never;
        fn is_repo(&self, path: &Path) -> bool {
            self.cloned.borrow().iter().any(|p| p == path)
        }
        fn clone_repo(&self, _url: &str, dest: &Path) -> Result<(), Never> {
            self.cloned.borrow_mut().push(dest.to_path_buf());
            Ok(())
        }
        fn set_identity(&self, path: &Path, name: &str, email: &str) -> Result<(), Never> {
            self.identities.borrow_mut().push((
                path.to_path_buf(),
                name.to_string(),
                email.to_string(),
            ));
            Ok(())
        }
        fn origin_url(&self, _path: &Path) -> Result<Option<String>, Never> {
            Ok(self.origin.clone())
        }
        fn identity(&self, path: &Path) -> Result<Option<(String, String)>, Never> {
            Ok(self
                .identities
                .borrow()
                .iter()
                .rev()
                .find(|(p, _, _)| p == path)
                .map(|(_, n, e)| (n.clone(), e.clone())))
        }
    }

    #[test]
    fn standing_up_clones_the_workspace_and_sets_its_identity() {
        // The defect the previous round shipped: `provision` existed, was
        // tested, and had no caller, so `up` reported success having created
        // directories and no workspace at all.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::with_origin();
        let out = stand_up(&ws(), d.path(), "paja", &git).unwrap();

        assert!(out.cloned, "a fresh workspace must be cloned");
        assert_eq!(git.cloned.borrow().len(), 1);
        assert_eq!(
            git.identity(&out.workspace).unwrap(),
            Some(("paja".into(), "paja@example.invalid".into())),
            "the identity must be set in the clone, or the Hand commits as the machine's owner"
        );
    }

    #[test]
    fn standing_up_twice_does_not_clone_over_the_workspace() {
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::with_origin();
        stand_up(&ws(), d.path(), "paja", &git).unwrap();
        let second = stand_up(&ws(), d.path(), "paja", &git).unwrap();
        assert!(!second.cloned);
        assert_eq!(git.cloned.borrow().len(), 1);
    }

    #[test]
    fn standing_up_from_a_root_with_no_origin_is_refused_and_says_why() {
        // The URL comes from the root's own remote, so a root that is not a
        // clone of the content repository has nothing to derive it from.
        // Guessing would clone the wrong thing quietly.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::without_origin();
        let err = stand_up(&ws(), d.path(), "paja", &git).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("origin"), "{msg}");
        assert!(git.cloned.borrow().is_empty());
    }

    #[test]
    fn a_registry_that_would_not_parse_is_never_written() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("homma.toml");
        std::fs::write(&path, "content_repo = \"clause-dev\"\n").unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        // A handle that is a valid TOML key but collides with the table it
        // would be written into.
        let mut id = Identity::new(Role::Hand, "ok_handle");
        id.domain = Some("fine".into());
        append_entry(&path, &id).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().len() > before.len());

        // And nothing is left behind by the write.
        let leftovers: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("writing"))
            .collect();
        assert!(leftovers.is_empty(), "temporary left behind: {leftovers:?}");
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
            line_for(&lines, "nameless").staffing,
            Staffing::Incomplete(_)
        ));
        assert_eq!(line_for(&lines, "paja").staffing, Staffing::Staffed);
    }

    #[test]
    fn a_listing_calls_a_mapped_domain_mapped_rather_than_broken() {
        // The listing is the surface where this matters: a roster of eighteen
        // domains with seven staffed would otherwise print eleven failures
        // every time anybody looked at it.
        let lines = list(&ws());
        assert_eq!(line_for(&lines, "rendering").staffing, Staffing::Mapped);
        assert_eq!(describe(&Staffing::Mapped), "mapped");
        assert!(describe(&line_for(&lines, "nameless").staffing).contains("git_name"));
    }

    #[test]
    fn standing_up_a_mapped_identity_is_refused_and_says_it_is_mapped() {
        // Not promoted. Creating a workspace as a side effect of standing up
        // would let a typo in the registry become a directory tree.
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&ws(), d.path(), "rendering", &FakeGit::with_origin()).unwrap_err();
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
        assert_eq!(w.org["new_hand"].staffing(), Staffing::Mapped);
    }

    #[test]
    fn standing_up_an_incomplete_identity_is_refused_with_the_reason() {
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&ws(), d.path(), "nameless", &FakeGit::with_origin()).unwrap_err();
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
        let err = stand_up(&ws(), d.path(), "op", &FakeGit::with_origin()).unwrap_err();
        assert!(err.to_string().contains("owns no workspace"), "{err}");
        assert!(!d.path().join(".claude/agents/op.md").exists());
    }

    #[test]
    fn a_registry_carrying_a_control_character_is_refused_before_generating() {
        let mut w = ws();
        w.org.get_mut("paja").unwrap().nickname = Some("Paja\nmemory: project".into());
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(&w, d.path(), "paja", &FakeGit::with_origin()).unwrap_err();
        assert!(err.to_string().contains("control characters"), "{err}");
    }

    #[test]
    fn standing_up_an_unknown_handle_is_refused() {
        let d = tempfile::tempdir().unwrap();
        assert!(stand_up(&ws(), d.path(), "stranger", &FakeGit::with_origin()).is_err());
    }

    #[test]
    fn standing_up_produces_both_definitions_and_a_linked_memory() {
        let d = tempfile::tempdir().unwrap();
        let out = stand_up(&ws(), d.path(), "paja", &FakeGit::with_origin()).unwrap();
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
        let a = stand_up(&ws(), d.path(), "paja", &FakeGit::with_origin()).unwrap();
        std::fs::write(a.home.join("memory/MEMORY.md"), "kept").unwrap();
        let b = stand_up(&ws(), d.path(), "paja", &FakeGit::with_origin()).unwrap();
        assert_eq!(a.home, b.home);
        assert_eq!(
            std::fs::read_to_string(b.home.join("memory/MEMORY.md")).unwrap(),
            "kept",
            "standing up again must not clear what is remembered"
        );
    }
}
