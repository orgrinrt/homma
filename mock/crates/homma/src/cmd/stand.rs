//! Standing an identity up, and writing the registry back.
//!
//! Split from `org.rs`, which reached 703 lines taking the same split that took
//! `workspace.rs` under the limit. Listing who exists and creating a workspace
//! for one of them are different jobs against different things: one reads
//! configuration, the other clones repositories and writes into trees.

use super::org::DISCIPLINE;
use anyhow::{Context, Result};
use homma_api::{AbsPath, ContainedPath, Git, Staffing, Workspace};
use homma_org::{prepare, provision, write_definitions, Layout, Registry};

/// Refuse a path that lies inside a repository which is not itself.
///
/// Called on **the root and the workspace**, which are the two paths a caller
/// hands in. It is not what guards the paths derived from them: those are
/// guarded by [`homma_api::Root`], which proves containment against the
/// filesystem and hands back a type that the write functions are the only
/// consumers of.
///
/// The split matters because a previous version of this comment claimed every
/// written path passed through here. It did not, and the claim survived a round
/// after being reported, which is how a guard gets trusted for work it is not
/// doing. This one answers "is this path inside somebody else's repository";
/// the other answers "does this path resolve inside our root". Neither implies
/// the other.
fn refuse_if_nested<G: Git>(git: &G, path: &AbsPath, what: &str) -> Result<()> {
    let enclosing = git
        .enclosing_repo(path)
        .map_err(|e| anyhow::anyhow!("looking for a repository above {path}: {e}"))?;
    if let Some(enclosing) = enclosing {
        anyhow::bail!(
            "{path} sits inside the repository at {enclosing}, and it is the \
             {what}. Writing there would put files in a tree that is not ours."
        );
    }
    Ok(())
}

/// What standing an identity up produced.
#[derive(Debug)]
pub struct StoodUp {
    pub handle: String,
    pub home: ContainedPath,
    /// The workspace clone itself, which is what the identity commits in.
    pub workspace: AbsPath,
    /// False when the workspace already held the content repository.
    pub cloned: bool,
    pub definition: ContainedPath,
    pub twin_definition: ContainedPath,
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
pub fn stand_up<G: Git>(ws: &Workspace, root: &AbsPath, handle: &str, git: &G) -> Result<StoodUp> {
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

    // The content repository is configuration, because that is what a workspace
    // is cloned from. An earlier round derived it from the root's own `origin`
    // and justified that by saying a configuration key would be a second place
    // for one fact to disagree. The key already existed and the derivation
    // consulted neither, so standing up from an unrelated clone cloned that
    // unrelated repository and wrote a participant's directories into it.
    // Before anything is created, and on the root rather than only on the
    // workspace: `prepare` and `write_definitions` both write here.
    refuse_if_nested(git, root, "workspace root")?;

    // Lexically, and before `git.init` rather than after. A workspace is a
    // clone of the content repository and cannot live inside it, and with
    // `content_repo = "local"` the root becomes a repository partway through:
    // checking afterwards refused correctly, having already created the `.git`
    // that made the refusal true, against the comment below saying a refusal
    // leaves nothing half-built.
    let workspace = AbsPath::resolve(
        root,
        id.workspace
            .as_ref()
            .expect("a staffed identity carries a workspace"),
    );
    // Both sides resolved, because one of them is canonical and the other is
    // whatever the registry said. Comparing those directly is what let this
    // check fall through to the repository check, which only fires after the
    // root has been initialised.
    let resolved_workspace = workspace
        .resolved()
        .with_context(|| format!("resolving {workspace}"))?;
    anyhow::ensure!(
        !resolved_workspace.starts_with(root.as_path()),
        "{workspace} is inside the workspace root at {root}. A workspace is a \
         clone of the content repository and cannot live in its tree; name one \
         outside it."
    );

    // And the workspace against the filesystem, not only lexically, and still
    // before anything is initialised. The lexical check above only catches a
    // workspace inside the root; a workspace nested in some *other* repository
    // fell through it, `git.init(root)` ran, and `provision` refused afterwards,
    // leaving behind the `.git` the comment above promises a refusal does not
    // create. `provision` still makes the same check, and keeps it: it is the
    // one that holds when provision is called from anywhere else.
    refuse_if_nested(git, &workspace, "workspace")?;

    let url = if ws.content_repo == homma_api::config::LOCAL {
        // The root itself is the content repository. On a fresh workspace it is
        // not a repository yet, and there is nothing to clone from until it is.
        if !git.is_repo(root) {
            // `is_repo` answers only whether this path is a repository root, so
            // a directory nested in somebody else's checkout looks free.
            // Initialising there puts a repository inside a repository and
            // lands a participant's directories in a tree that is not ours,
            // which is what every guard here exists to stop.
            git.init(root)
                .map_err(|e| anyhow::anyhow!("initialising {}: {e}", root.display()))?;
        }
        root.to_string_lossy().into_owned()
    } else {
        // The root's own remote is a cross-check of two independent facts rather
        // than a source for one. A root that is a clone of something else is not
        // the workspace this configuration describes, and its tree is not ours
        // to write in.
        if let Some(origin) = git
            .origin_url(root)
            .map_err(|e| anyhow::anyhow!("reading the origin of {}: {e}", root.display()))?
        {
            anyhow::ensure!(
                homma_org::same_repo(&origin, &ws.content_repo),
                "{} is a clone of `{}`, and this configuration describes the \
                 workspace for `{}`. Standing `{handle}` up here would write into \
                 the wrong tree. Run against the content repository's own clone, \
                 or pass --root.",
                root.display(),
                homma_org::repo_name(&origin),
                homma_org::repo_name(&ws.content_repo)
            );
        }
        ws.content_repo.clone()
    };

    // Before the directories, so a refusal here leaves nothing half-built.
    let provisioned = provision(id, &workspace, &url, git)
        .map_err(|e| anyhow::anyhow!("provisioning `{handle}`: {e}"))?;

    let layout = Layout::new(root, &ws.paths)
        .with_context(|| format!("resolving the workspace root at {root}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::fake_git::{abs, FakeGit, CONTENT};

    const ORG: &str = r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"

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
    fn standing_up_clones_the_workspace_and_sets_its_identity() {
        // The defect the previous round shipped: `provision` existed, was
        // tested, and had no caller, so `up` reported success having created
        // directories and no workspace at all.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::at_the_content_repo();
        let out = stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap();

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
        let git = FakeGit::at_the_content_repo();
        stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap();
        let second = stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap();
        assert!(!second.cloned);
        assert_eq!(git.cloned.borrow().len(), 1);
    }

    #[test]
    fn standing_up_from_a_root_cloned_from_something_else_is_refused() {
        // Reproduced before this existed: standing up from an unrelated member
        // clone cloned that member repository into the Hand's workspace and
        // wrote the Hand's directories into the member clone's own tree, which
        // every guard here exists to stop. It exited 0.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::somewhere_else();
        let err = stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("clause-dev"),
            "must name what it expected: {msg}"
        );
        assert!(msg.contains("member"), "and what it found: {msg}");
        assert!(
            git.cloned.borrow().is_empty(),
            "refusing must happen before anything is cloned"
        );
    }

    #[test]
    fn standing_up_from_a_root_that_is_not_a_repository_still_works() {
        // The root's remote is a cross-check, not a source. A root with no
        // remote cannot contradict the configuration, so there is nothing to
        // refuse; the URI came from the configuration either way.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::no_origin();
        let out = stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap();
        assert!(out.cloned);
        assert_eq!(git.cloned.borrow()[0].0, CONTENT);
    }

    #[test]
    fn the_clone_url_is_the_configured_uri_and_not_the_roots_remote() {
        // The whole defect in one assertion.
        let d = tempfile::tempdir().unwrap();
        let git = FakeGit::at_the_content_repo();
        stand_up(&ws(), &abs(d.path()), "paja", &git).unwrap();
        assert_eq!(git.cloned.borrow()[0].0, CONTENT);
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
    fn standing_up_a_mapped_identity_is_refused_and_says_it_is_mapped() {
        // Not promoted. Creating a workspace as a side effect of standing up
        // would let a typo in the registry become a directory tree.
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(
            &ws(),
            &abs(d.path()),
            "rendering",
            &FakeGit::at_the_content_repo(),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("mapped"), "must say what is true: {msg}");
        assert!(
            !msg.contains("git_name"),
            "must not report a decision as missing fields: {msg}"
        );
        assert!(!d.path().join(".shared/hands/rendering").exists());
    }

    #[test]
    fn standing_up_an_incomplete_identity_is_refused_with_the_reason() {
        let d = tempfile::tempdir().unwrap();
        let err = stand_up(
            &ws(),
            &abs(d.path()),
            "nameless",
            &FakeGit::at_the_content_repo(),
        )
        .unwrap_err();
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
        let err =
            stand_up(&ws(), &abs(d.path()), "op", &FakeGit::at_the_content_repo()).unwrap_err();
        assert!(err.to_string().contains("owns no workspace"), "{err}");
        assert!(!d.path().join(".claude/agents/op.md").exists());
    }

    #[test]
    fn a_registry_carrying_a_control_character_is_refused_before_generating() {
        let mut w = ws();
        w.org.get_mut("paja").unwrap().nickname = Some("Paja\nmemory: project".into());
        let d = tempfile::tempdir().unwrap();
        let err =
            stand_up(&w, &abs(d.path()), "paja", &FakeGit::at_the_content_repo()).unwrap_err();
        assert!(err.to_string().contains("control characters"), "{err}");
    }

    #[test]
    fn standing_up_an_unknown_handle_is_refused() {
        let d = tempfile::tempdir().unwrap();
        assert!(stand_up(
            &ws(),
            &abs(d.path()),
            "stranger",
            &FakeGit::at_the_content_repo()
        )
        .is_err());
    }

    #[test]
    fn standing_up_produces_both_definitions_and_a_linked_memory() {
        let d = tempfile::tempdir().unwrap();
        let out = stand_up(
            &ws(),
            &abs(d.path()),
            "paja",
            &FakeGit::at_the_content_repo(),
        )
        .unwrap();
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
        let a = stand_up(
            &ws(),
            &abs(d.path()),
            "paja",
            &FakeGit::at_the_content_repo(),
        )
        .unwrap();
        std::fs::write(a.home.join("memory/MEMORY.md"), "kept").unwrap();
        let b = stand_up(
            &ws(),
            &abs(d.path()),
            "paja",
            &FakeGit::at_the_content_repo(),
        )
        .unwrap();
        assert_eq!(a.home, b.home);
        assert_eq!(
            std::fs::read_to_string(b.home.join("memory/MEMORY.md")).unwrap(),
            "kept",
            "standing up again must not clear what is remembered"
        );
    }
}
