//! `homma org`: who exists, and standing them up.
//!
//! Reads the registry from the workspace configuration. Nothing here knows what
//! a Hand is beyond what a role says, because homma runs against workspaces it
//! has never seen.

use anyhow::{Context, Result};
use homma_api::{Identity, Role, Staffing, Workspace};
use homma_org::Registry;
use std::path::Path;

/// The standing instruction every generated definition carries.
///
/// One source, so it cannot drift between participants: none of them owns it.
pub(crate) const DISCIPLINE: &str = "\
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn line_for<'a>(lines: &'a [Line], handle: &str) -> &'a Line {
        lines.iter().find(|l| l.handle == handle).expect(handle)
    }

    fn ws() -> Workspace {
        Workspace::parse(ORG).unwrap()
    }

    /// Enough git to exercise `stand_up`'s own logic, tracking which URL each
    /// path was cloned from. What a real repository does with an identity is
    /// tested where the real implementation lives.
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
}
