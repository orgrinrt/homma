//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! One changelog block per release: the version and the date, then the
//! commits since the previous tag grouped by prefix in a fixed order, in the
//! commit's own words. The same text is the GitHub release body.

use std::path::Path;

use homma_api::Version;

use super::git::Subject;

/// The groups, in the order they print. `other` is last and takes anything
/// without a recognised prefix.
pub const GROUPS: [&str; 6] = ["feat", "fix", "docs", "refactor", "chore", "test"];

/// The prefix of a conventional subject, `feat` off `feat: ...` or
/// `feat!: ...` or `feat(scope): ...`, or none.
pub fn prefix(subject: &str) -> Option<&str> {
    let head = subject.split(':').next()?;
    let head = head.trim_end_matches('!');
    let head = head.split('(').next()?;
    GROUPS.contains(&head).then_some(head)
}

/// The block for `version` released on `date`, over `subjects` in the order
/// git gave them, which is newest first.
pub fn block(version: &Version, date: &str, subjects: &[Subject]) -> String {
    let mut out = format!("## {version} ({date})\n");
    let mut groups: Vec<(&str, Vec<&Subject>)> = GROUPS.iter().map(|g| (*g, Vec::new())).collect();
    let mut other = Vec::new();
    for s in subjects {
        match prefix(&s.subject) {
            Some(p) => {
                groups
                    .iter_mut()
                    .find(|(g, _)| *g == p)
                    .expect("prefix is one of the groups")
                    .1
                    .push(s)
            },
            None => other.push(s),
        }
    }
    groups.push(("other", other));
    for (name, items) in groups {
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {name}\n\n"));
        for s in items {
            out.push_str(&line(s));
            out.push('\n');
        }
    }
    out
}

fn line(s: &Subject) -> String {
    match s.pr {
        Some(pr) => format!("- {} `{}` #{pr}", s.subject, s.sha),
        None => format!("- {} `{}`", s.subject, s.sha),
    }
}

/// Prepend `block` to `CHANGELOG.md` at `root`, creating the file when it is
/// not there. A block already opening the file is not added twice.
pub fn prepend(root: &Path, block: &str) -> std::io::Result<()> {
    let path = root.join("CHANGELOG.md");
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let body = existing
        .strip_prefix("# Changelog\n")
        .map(|s| s.trim_start_matches('\n'))
        .unwrap_or(&existing);
    if body.starts_with(block.trim_end()) {
        return Ok(());
    }
    let mut text = String::from("# Changelog\n\n");
    text.push_str(block.trim_end());
    text.push('\n');
    if !body.trim().is_empty() {
        text.push('\n');
        text.push_str(body.trim_start_matches('\n'));
    }
    std::fs::write(&path, text)
}

/// The newest block in `CHANGELOG.md`, for the release body when the file
/// already carries it.
pub fn newest_block(text: &str) -> Option<&str> {
    let start = text.find("\n## ")? + 1;
    let rest = &text[start ..];
    let end = rest[3 ..]
        .find("\n## ")
        .map(|i| i + 3)
        .unwrap_or(rest.len());
    Some(rest[.. end].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(sha: &str, subject: &str, pr: Option<u64>) -> Subject {
        Subject {
            sha: sha.into(),
            subject: subject.into(),
            pr,
        }
    }

    #[test]
    fn prefixes_are_read_with_scope_and_bang_and_anything_else_is_none() {
        assert_eq!(prefix("feat: x"), Some("feat"));
        assert_eq!(prefix("feat!: x"), Some("feat"));
        assert_eq!(prefix("fix(core): x"), Some("fix"));
        assert_eq!(prefix("docs: x"), Some("docs"));
        assert_eq!(prefix("state: x"), None);
        assert_eq!(prefix("Merge pull request #3 from a/b"), None);
        assert_eq!(prefix("features are not feat"), None);
    }

    #[test]
    fn the_block_groups_in_the_fixed_order_and_other_is_last() {
        let subjects = vec![
            s("aaa", "chore: tidy", None),
            s("bbb", "rework the thing (#12)", Some(12)),
            s("ccc", "feat: add x", None),
            s("ddd", "fix: y", None),
            s("eee", "feat!: break z", None),
        ];
        let v = Version::new(0, 2, 0);
        let b = block(&v, "2026-09-02", &subjects);
        let expected = "## 0.2.0 (2026-09-02)\n\n### feat\n\n- feat: add x `ccc`\n- feat!: break z `eee`\n\n### fix\n\n- fix: y `ddd`\n\n### chore\n\n- chore: tidy `aaa`\n\n### other\n\n- rework the thing (#12) `bbb` #12\n";
        assert_eq!(b, expected);
    }

    #[test]
    fn an_empty_range_is_a_heading_alone() {
        assert_eq!(block(&Version::new(1, 0, 0), "d", &[]), "## 1.0.0 (d)\n");
    }

    #[test]
    fn prepend_creates_then_stacks_newest_first_and_does_not_duplicate() {
        let d = tempfile::tempdir().unwrap();
        let one = block(&Version::new(0, 1, 0), "d1", &[s("a", "feat: a", None)]);
        prepend(d.path(), &one).unwrap();
        let text = std::fs::read_to_string(d.path().join("CHANGELOG.md")).unwrap();
        assert_eq!(
            text,
            "# Changelog\n\n## 0.1.0 (d1)\n\n### feat\n\n- feat: a `a`\n"
        );
        let two = block(&Version::new(0, 1, 1), "d2", &[s("b", "fix: b", None)]);
        prepend(d.path(), &two).unwrap();
        let text = std::fs::read_to_string(d.path().join("CHANGELOG.md")).unwrap();
        assert!(text.starts_with("# Changelog\n\n## 0.1.1 (d2)\n"), "{text}");
        assert!(text.contains("\n## 0.1.0 (d1)\n"), "{text}");
        assert!(text.find("0.1.1").unwrap() < text.find("0.1.0").unwrap());
        prepend(d.path(), &two).unwrap();
        let again = std::fs::read_to_string(d.path().join("CHANGELOG.md")).unwrap();
        assert_eq!(again, text, "a block already at the top is not added twice");
        assert_eq!(newest_block(&again), Some(two.trim_end()));
    }

    #[test]
    fn newest_block_is_none_on_a_file_with_no_release() {
        assert_eq!(newest_block("# Changelog\n"), None);
        assert_eq!(newest_block(""), None);
    }
}
