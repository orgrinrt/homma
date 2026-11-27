//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//! The git hooks homma has a hand in, every one of them registered with
//! git-shook: homma's own release gate, and the workspace's git hooks manifest
//! carried into each member. homma declares no git hook in its config and runs
//! no chain of them; git-shook runs every event's entries on its base.

mod error;
pub mod legacy;
pub mod shook;

pub use error::HookError;
pub use shook::{
    GATE,
    GATE_EVENT,
    Installed,
    NAME,
    WorkspaceHooks,
    gate_manifest,
    holds_path,
    install,
    install_with,
    workspace_hooks,
};
