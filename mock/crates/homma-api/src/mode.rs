//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a mode declares about itself, and reading it off its file.
//!
//! A mode is a skill whose point is the effort it sets and the few lines it says
//! about a kind of work. Its skill's frontmatter is built from this meta, never
//! written by hand, so the meta is the whole of what a mode declares.

use serde::Serialize;

use crate::Effort;
use crate::frontmatter::{self, FrontmatterError};

/// Every field a mode declares, all of them required.
const KNOWN: &[&str] = &["type", "effort", "when", "skills", "tools", "ends_turn"];

/// What a mode declares about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeMeta {
    /// The kind of work, which is also the file's name.
    #[serde(rename = "type")]
    pub kind:      String,
    /// The effort the mode's skill sets.
    pub effort:    Effort,
    /// When it is reached for, the clause the skill's description is built on.
    pub when:      String,
    /// The skills the mode names.
    pub skills:    Vec<String>,
    /// The tools the mode names.
    pub tools:     Vec<String>,
    /// Whether the work ends the turn.
    pub ends_turn: bool,
}

/// A mode's meta and the body after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMode {
    pub meta: ModeMeta,
    pub body: String,
}

/// Read a mode off the text of its file.
pub fn parse(source: &str) -> Result<ParsedMode, FrontmatterError> {
    let block = frontmatter::split(source, KNOWN)?;
    let kind = block.scalar("type")?;
    let effort = block.scalar("effort")?.parse::<Effort>().map_err(|e| {
        FrontmatterError::BadValue {
            key:    "effort".into(),
            reason: e.to_string(),
        }
    })?;
    let when = block.scalar("when")?;
    let skills = block.required_list("skills")?;
    let tools = block.required_list("tools")?;
    let ends_turn = match block.scalar("ends_turn")?.as_str() {
        "true" => true,
        "false" => false,
        other => {
            return Err(FrontmatterError::BadValue {
                key:    "ends_turn".into(),
                reason: format!("`{other}` is neither `true` nor `false`"),
            });
        },
    };
    Ok(ParsedMode {
        meta: ModeMeta {
            kind,
            effort,
            when,
            skills,
            tools,
            ends_turn,
        },
        body: block.body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODE: &str = "---\ntype: review\neffort: high\nwhen: \"reviewing a diff\"\nskills: [a, b]\ntools: [x y, z]\nends_turn: false\n---\n\n- Read the design first.\n";

    #[test]
    fn a_mode_reads_its_six_keys_and_its_body() {
        let m = parse(MODE).unwrap();
        assert_eq!(m.meta.kind, "review");
        assert_eq!(m.meta.effort, Effort::High);
        assert_eq!(m.meta.when, "reviewing a diff");
        assert_eq!(m.meta.skills, ["a", "b"]);
        assert_eq!(m.meta.tools, ["x y", "z"]);
        assert!(!m.meta.ends_turn);
        assert_eq!(m.body.trim(), "- Read the design first.");
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let src = MODE.replace("ends_turn: false\n", "ends_turn: false\nmodel: x\n");
        assert!(matches!(
            parse(&src),
            Err(FrontmatterError::UnknownKey { .. })
        ));
    }

    #[test]
    fn a_missing_key_is_refused() {
        let src = MODE.replace("tools: [x y, z]\n", "");
        assert_eq!(
            parse(&src),
            Err(FrontmatterError::Missing {
                key: "tools",
            })
        );
    }

    #[test]
    fn an_ends_turn_that_is_no_bool_and_an_effort_the_host_lacks_are_refused() {
        let src = MODE.replace("ends_turn: false", "ends_turn: yes");
        assert!(
            matches!(parse(&src), Err(FrontmatterError::BadValue { key, .. }) if key == "ends_turn")
        );
        let src = MODE.replace("effort: high", "effort: huge");
        assert!(
            matches!(parse(&src), Err(FrontmatterError::BadValue { key, .. }) if key == "effort")
        );
    }

    #[test]
    fn a_list_not_written_inline_is_refused() {
        let src = MODE.replace("skills: [a, b]", "skills: a");
        assert!(
            matches!(parse(&src), Err(FrontmatterError::BadValue { key, .. }) if key == "skills")
        );
    }
}
