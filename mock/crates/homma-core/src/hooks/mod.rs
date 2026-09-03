//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The git hooks homma has a hand in: one entrypoint per event written into
//! the repository's own hooks directory, and the entries the table runs when
//! git calls it. The table itself is `homma_api::Hooks`.

pub mod install;
pub mod run;

pub use install::{HookError, Installed, Reach, hooks_dir, install, is_installed, script};
pub use run::{Ran, run, touched};
