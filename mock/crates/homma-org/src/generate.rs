//! Generating the definition an agent runs under.
//!
//! Three parts, and the split is the point. The identity block comes from the
//! registry, so it cannot disagree with what homma believes. The standing
//! discipline comes from one shared template, so it cannot drift between
//! participants, because none of them owns it. The character is hand-written and
//! composed in verbatim, because voice is the part a template cannot supply.
//!
//! The twin's definition is generated from the same identity and deliberately
//! omits the memory key. That restriction has to be structural: a definition
//! carrying the key grants the write path whatever its prose says.

use crate::workspace::Layout;
use homma_api::Identity;
use std::fs;
use std::io;

/// What a generated definition is made of.
pub struct Generated {
    pub frontmatter: String,
    pub body: String,
}

impl Generated {
    pub fn render(&self) -> String {
        format!("---\n{}---\n\n{}", self.frontmatter, self.body)
    }
}

/// Whether the definition is for the participant itself or for a dispatched
/// copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Runs as a session's own agent. Carries memory.
    Prime,
    /// Dispatched as a subagent. Carries no memory, only notes.
    Twin,
}

/// Build the definition for an identity.
///
/// `character` is the hand-written fragment, composed verbatim. `discipline` is
/// the shared standing instruction. Neither is invented here: this function
/// assembles, it does not author.
pub fn definition(id: &Identity, form: Form, discipline: &str, character: &str) -> Generated {
    let mut fm = String::new();
    let name = match form {
        Form::Prime => id.handle.clone(),
        Form::Twin => format!("{}-twin", id.handle),
    };
    fm.push_str(&format!("name: {name}\n"));
    fm.push_str(&format!("description: {}\n", describe(id, form)));
    // The memory key is present for a prime and absent for a twin. This is the
    // whole mechanism behind "a twin may not write memory".
    if form == Form::Prime {
        fm.push_str("memory: project\n");
    }

    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", display_name(id)));
    if let Some(domain) = &id.domain {
        body.push_str(&format!("Domain: {domain}.\n\n"));
    }
    // Tell it who it is. Without this a Hand answers questions about its own
    // identity from the global git config it can see, which is the human's, and
    // is wrong in the one place being wrong matters.
    if let (Some(name), Some(email)) = (&id.git_name, &id.git_email) {
        body.push_str(&format!(
            "You commit as `{name} <{email}>`, set in this workspace's own git \
             config and nowhere else. The global identity on this machine belongs \
             to someone else and is not yours.\n\n"
        ));
    }
    match form {
        Form::Prime => {
            body.push_str(
                "You keep your own memory. Read it before you start and write to it \
                 what will still be true next week.\n\n",
            );
        }
        Form::Twin => {
            body.push_str(
                "You are a dispatched copy. **You may not write memory.** What you \
                 learn goes to notes, which your prime reads and folds or drops. You \
                 do not write to channels either: answer whoever dispatched you, and \
                 they speak in their own name.\n\n",
            );
        }
    }
    body.push_str(discipline.trim_end());
    body.push('\n');
    if !character.trim().is_empty() {
        body.push('\n');
        body.push_str(character.trim_end());
        body.push('\n');
    }

    Generated {
        frontmatter: fm,
        body,
    }
}

fn display_name(id: &Identity) -> String {
    id.full_name
        .clone()
        .or_else(|| id.nickname.clone())
        .unwrap_or_else(|| id.handle.clone())
}

fn describe(id: &Identity, form: Form) -> String {
    let who = id.nickname.clone().unwrap_or_else(|| id.handle.clone());
    match (form, &id.domain) {
        (Form::Prime, Some(d)) => format!("{who}, who works on {d}."),
        (Form::Prime, None) => format!("{who}."),
        (Form::Twin, Some(d)) => {
            format!("{who} dispatched as a subagent, without session history. Works on {d}.")
        }
        (Form::Twin, None) => format!("{who} dispatched as a subagent, without session history."),
    }
}

/// Write both definitions into a workspace, reading the character fragment from
/// the identity's own directory if it has one.
pub fn write_definitions(
    layout: &Layout<'_>,
    id: &Identity,
    discipline: &str,
) -> io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    // Only absence is absence. Swallowing every error shipped a Hand without
    // its voice and reported nothing.
    let character = match fs::read_to_string(layout.character(id)) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let prime = layout.definition(id);
    let twin = layout.twin_definition(id);
    if let Some(parent) = prime.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &prime,
        definition(id, Form::Prime, discipline, &character).render(),
    )?;
    fs::write(
        &twin,
        definition(id, Form::Twin, discipline, &character).render(),
    )?;
    Ok((prime, twin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Layout;
    use homma_api::{Paths, Role};

    fn hand() -> Identity {
        let mut i = Identity::new(Role::Hand, "paja");
        i.nickname = Some("Paja".into());
        i.full_name = Some("Urho \"Paja\" Alasinen".into());
        i.domain = Some("tooling".into());
        i.git_name = Some("paja".into());
        i.git_email = Some("paja@example.invalid".into());
        i
    }

    const DISCIPLINE: &str = "## Standing\n\nRead the rules before you touch anything.";

    #[test]
    fn a_prime_carries_memory_and_a_twin_does_not() {
        let id = hand();
        let prime = definition(&id, Form::Prime, DISCIPLINE, "");
        let twin = definition(&id, Form::Twin, DISCIPLINE, "");
        assert!(prime.frontmatter.contains("memory: project"));
        assert!(
            !twin.frontmatter.contains("memory:"),
            "the twin's inability to write memory is the absence of this key, \
             not a sentence asking it not to"
        );
    }

    #[test]
    fn the_twin_is_a_different_agent_name() {
        let id = hand();
        assert!(definition(&id, Form::Prime, DISCIPLINE, "")
            .frontmatter
            .contains("name: paja\n"));
        assert!(definition(&id, Form::Twin, DISCIPLINE, "")
            .frontmatter
            .contains("name: paja-twin\n"));
    }

    #[test]
    fn the_character_is_composed_verbatim() {
        let id = hand();
        let character = "He is short with anyone who says a thing is impossible\nbefore trying it.";
        let out = definition(&id, Form::Prime, DISCIPLINE, character).render();
        assert!(
            out.contains(character),
            "the character must survive unedited"
        );
    }

    #[test]
    fn a_missing_character_leaves_the_definition_valid() {
        let id = hand();
        let out = definition(&id, Form::Prime, DISCIPLINE, "").render();
        assert!(out.starts_with("---\n"));
        assert!(out.contains(DISCIPLINE));
        assert!(
            !out.contains("\n\n\n\n"),
            "no hole where the character would be"
        );
    }

    #[test]
    fn the_shared_discipline_is_in_both_forms() {
        let id = hand();
        for form in [Form::Prime, Form::Twin] {
            assert!(
                definition(&id, form, DISCIPLINE, "")
                    .body
                    .contains(DISCIPLINE),
                "the shared part cannot drift between forms because neither owns it"
            );
        }
    }

    #[test]
    fn the_twin_is_told_it_does_not_speak_in_channels() {
        let id = hand();
        let twin = definition(&id, Form::Twin, DISCIPLINE, "").body;
        assert!(twin.contains("do not write to channels"));
    }

    #[test]
    fn both_definitions_land_on_disk() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::default();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        crate::workspace::prepare(&l, &id).unwrap();
        std::fs::write(l.character(&id), "Terse.").unwrap();

        let (prime, twin) = write_definitions(&l, &id, DISCIPLINE).unwrap();
        let prime_text = std::fs::read_to_string(&prime).unwrap();
        let twin_text = std::fs::read_to_string(&twin).unwrap();
        assert!(prime_text.contains("memory: project"));
        assert!(!twin_text.contains("memory:"));
        assert!(prime_text.contains("Terse."));
        assert!(twin_text.contains("Terse."), "a twin is the same colleague");
    }

    #[test]
    fn a_hand_is_told_which_git_identity_is_its_own() {
        // Found end to end: a Hand asked for its git email answered with the
        // machine's global one, because nothing had told it otherwise. The
        // commits were correct; the Hand's belief about itself was not.
        let id = hand();
        let body = definition(&id, Form::Prime, DISCIPLINE, "").body;
        assert!(body.contains("paja@example.invalid"));
        assert!(
            body.contains("belongs"),
            "and that the global one is not its own"
        );
    }

    #[test]
    fn an_identity_with_no_git_name_gets_no_claim_about_one() {
        let mut id = hand();
        id.git_name = None;
        id.git_email = None;
        let body = definition(&id, Form::Prime, DISCIPLINE, "").body;
        assert!(!body.contains("You commit as"));
    }

    #[test]
    fn a_character_that_cannot_be_read_is_reported_rather_than_swallowed() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::default();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        crate::workspace::prepare(&l, &id).unwrap();
        // A directory where the file belongs is not absence.
        std::fs::create_dir_all(l.character(&id)).unwrap();
        assert!(
            write_definitions(&l, &id, DISCIPLINE).is_err(),
            "shipping a voiceless Hand silently is the failure this prevents"
        );
    }

    #[test]
    fn the_identity_block_comes_from_the_registry_rather_than_being_typed() {
        // If this drifts, the definition and homma disagree about who someone is.
        let id = hand();
        let out = definition(&id, Form::Prime, DISCIPLINE, "").render();
        assert!(out.contains("Urho \"Paja\" Alasinen"));
        assert!(out.contains("tooling"));
    }
}
