//! Writing the registry back.
//!
//! Split from `stand.rs`, which crossed the file-size limit. Rendering an entry
//! and appending it to a text file is a different job from cloning a repository
//! and generating definitions, and the two share nothing but the identity type.

use std::path::Path;

use anyhow::{Context, Result};
use homma_api::{Denied, Identity, Workspace};

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

/// Write a registry back, through a temporary file and a rename.
///
/// Appending directly leaves a truncated table when a write is short, and every
/// later invocation then fails to parse a registry with no backup. Re-parsed
/// afterwards, because a file homma writes and cannot read is worse than one it
/// refuses to write.
pub fn append_entry(path: &Path, id: &Identity, denied: &Denied) -> Result<()> {
    // **Before anything is read or written**, because this writes two files at a
    // path the operator names and had no deny check at all: `org add` against a
    // `--config` inside `~/.claude` exited 0 having rewritten a registry there.
    //
    // The list is a parameter rather than read here, for the same reason it is
    // one everywhere else: a default would be the empty list, which is the
    // answer that silently reintroduces the defect.
    let abs =
        homma_api::AbsPath::new(std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    denied.check(&abs, "registry")?;

    let existing =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let next = format!("{existing}{}", render_entry(id)?);

    Workspace::parse(&next).with_context(|| {
        format!(
            "the registry would not parse after adding `{}`, so nothing was written",
            id.handle
        )
    })?;

    // Resolved first, because renaming over a symlink replaces the link with a
    // regular file: the entry lands on the link, the file it pointed at never
    // sees it, and the operator is left maintaining a registry that is silently
    // stale beside a divergent copy.
    let path = &std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    // Write beside and rename. **This is not pinned by a test and cannot be at
    // this level**: what the rename buys is that a crash between the two leaves
    // the original intact, and a unit test cannot observe a crash. Reverting it
    // to a plain write leaves the suite green, which was checked rather than
    // assumed. Stated here because a mechanism whose absence nothing detects is
    // a mechanism the next reader may delete believing it covered.
    let temp = path.with_extension("toml.writing");
    std::fs::write(&temp, &next).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use homma_api::Role;

    use super::*;

    /// A list denying somewhere no test writes, so these exercise the write
    /// rather than the refusal. The refusal is asserted end to end, where the
    /// path actually comes from an operator.
    fn nothing_denied() -> Denied {
        Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap())
    }

    #[test]
    fn a_registry_that_would_not_parse_is_never_written() {
        // Deleting the re-validation left the whole suite green, because the
        // only test naming it asserted the success path. This asserts the
        // refusal, which is the branch the mechanism exists for.
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("homma.toml");
        // A registry that is already broken. Appending to it produces something
        // that still will not parse, which is exactly when nothing may be
        // written.
        std::fs::write(&path, "content_repo = \n").unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let err = append_entry(
            &path,
            &Identity::new(Role::Hand, "victim"),
            &nothing_denied(),
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("would not parse"),
            "must say why nothing was written: {err:#}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "a refused write must leave the registry exactly as it was"
        );
    }

    #[test]
    fn a_registry_write_leaves_no_stray_files_and_parses_back() {
        // Note what this does NOT test: reverting the rename to a plain write
        // keeps it green, because atomicity under a crash is not observable
        // from here. It tests that the write completes, leaves nothing behind,
        // and produces something readable.
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("homma.toml");
        std::fs::write(
            &path,
            "content_repo = \"git@example.invalid:orgrinrt/clause-dev.git\"\n",
        )
        .unwrap();

        append_entry(
            &path,
            &Identity::new(Role::Hand, "ok_handle"),
            &nothing_denied(),
        )
        .unwrap();

        let strays: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "homma.toml")
            .collect();
        assert!(strays.is_empty(), "left behind: {strays:?}");
        // And the result is readable, which is the whole point of re-parsing.
        let text = std::fs::read_to_string(&path).unwrap();
        Workspace::parse(&text).expect("what was written must parse");
    }
}
