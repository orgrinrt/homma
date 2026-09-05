//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a workspace holds, repository by repository, and what would stop it
//! being removed.

use std::io::Write;
use std::path::Path;

use super::git;

/// One repository in a workspace, as the survey found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The directory name, or `.` for the workspace's own clone.
    pub name:     String,
    pub branch:   String,
    pub head:     String,
    /// Uncommitted or untracked changes.
    pub dirty:    bool,
    /// Commits on a local branch and on no remote, one line each.
    pub unpushed: Vec<String>,
}

impl Member {
    /// Whether removing the workspace would lose something only it holds.
    pub fn holds_work(&self) -> bool {
        self.dirty || !self.unpushed.is_empty()
    }
}

/// The workspace's own clone first, then every directory one level down
/// holding a `.git` directory, in name order. A directory that is not a
/// clone is not a member, the same rule the manifest applies.
pub fn survey(root: &Path) -> Result<Vec<Member>, String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map_err(|e| format!("cannot read {}: {e}", root.display()))?
        .filter_map(Result::ok)
        .filter(|e| git::is_clone(&e.path()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut out = Vec::new();
    if git::is_clone(root) {
        // The members sit inside this clone as untracked directories unless
        // it ignores them, and that is the workspace's shape rather than
        // dirt, so those entries are taken out before the rest is counted.
        let dirty = git::status_lines(root)?
            .iter()
            .any(|line| !names.iter().any(|n| line == &format!("?? {n}/")));
        out.push(member(".", root, dirty)?);
    }
    for name in names {
        let dir = root.join(&name);
        let dirty = !git::status_lines(&dir)?.is_empty();
        out.push(member(&name, &dir, dirty)?);
    }
    Ok(out)
}

fn member(name: &str, dir: &Path, dirty: bool) -> Result<Member, String> {
    Ok(Member {
        name: name.to_owned(),
        branch: git::current_branch(dir)?,
        head: git::short_head(dir)?,
        dirty,
        unpushed: git::unpushed(dir)?,
    })
}

/// The status, one line per member: name, branch, head, and what would be
/// lost. Members with nothing to lose say so in one word.
pub fn render(root: &Path, members: &[Member], out: &mut dyn Write) -> Result<(), String> {
    let w = |out: &mut dyn Write, s: String| out.write_all(s.as_bytes()).map_err(|e| e.to_string());
    w(out, format!("workspace {}\n", root.display()))?;
    if members.is_empty() {
        return w(out, "  no repositories\n".into());
    }
    for m in members {
        let mut state = Vec::new();
        if m.dirty {
            state.push("dirty".to_owned());
        }
        if !m.unpushed.is_empty() {
            state.push(format!("{} on no remote", m.unpushed.len()));
        }
        let state = if state.is_empty() { "clean".to_owned() } else { state.join(", ") };
        w(
            out,
            format!("  {}  {}  {}  {state}\n", m.name, m.branch, m.head),
        )?;
        for line in &m.unpushed {
            w(out, format!("      {line}\n"))?;
        }
    }
    Ok(())
}

/// Survey and render, which is what the bare command does inside one.
pub fn report(root: &Path, out: &mut dyn Write) -> Result<(), String> {
    let members = survey(root)?;
    render(root, &members, out)
}
