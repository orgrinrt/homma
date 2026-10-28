//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The file mockspace consults before it writes a row: `.mockspace/hooks/registry-append` in the
//! workspace root. It is local, nothing tracks it, and a repository cloned anywhere else has none.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Where the hook sits for a workspace.
pub fn path(root: &Path) -> PathBuf {
    root.join(".mockspace/hooks/registry-append")
}

fn quoted(p: &Path) -> Result<String> {
    let s = p
        .to_str()
        .with_context(|| format!("{} is not text", p.display()))?;
    if s.contains('\'') {
        bail!("{s} holds a quote, which the hook cannot spell");
    }
    Ok(format!("'{s}'"))
}

/// Places the hook, replacing one that is there. It runs `exe` against the workspace at `root`.
pub fn place(root: &Path, exe: &Path) -> Result<PathBuf> {
    let file = path(root);
    let dir = file.parent().context("the hook has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    let body = format!(
        "#!/bin/sh\n# Placed by `homma answers hook`; `homma answers hook --remove` takes it away.\nexec {} --dir {} answers delegate \"$@\"\n",
        quoted(exe)?,
        quoted(root)?
    );
    std::fs::write(&file, body).with_context(|| format!("writing {}", file.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("making {} executable", file.display()))?;
    }
    Ok(file)
}

/// Takes the hook away, and says nothing where there was none.
pub fn remove(root: &Path) -> Result<()> {
    match std::fs::remove_file(path(root)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("removing {}", path(root).display())),
    }
}
