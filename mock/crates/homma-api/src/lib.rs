//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The vocabulary every other homma crate speaks.
//!
//! Types and the traits over them. No I/O of any kind lives here, which is what
//! lets the store, the registry and the command surface all depend on it without
//! depending on each other.

pub mod config;
pub mod contained;
pub mod denied;
pub mod deny_entry;
pub mod git;
pub mod hooks;
pub mod path;
pub mod poster;
pub mod record;
pub mod reference;
pub mod release;
pub mod version;

pub use config::{Identity, Paths, Role, Staffing, UNREADABLE, Unreadable, Workspace};
pub use contained::{ContainedPath, Escapes, Root};
pub use denied::{Denied, Forbidden, NoHome, Standing};
pub use deny_entry::DenyEntry;
pub use git::{CommitIdentity, EmptyPart, Git, Part};
pub use hooks::{HookEntry, Hooks, InvalidHooks};
pub use path::{AbsPath, NotAbsolute};
pub use poster::{NotAGiveUp, POSTER_GAVE_UP_KIND, PosterGaveUp};
pub use record::{Attr, AttrType, Invalid, Kind, Mutability, Record};
pub use reference::{Namespace, NotAReference, Reference, Rung};
pub use release::{
    Badge,
    CheckSeverity,
    Finding,
    GATE_RUN_KIND,
    GateRun,
    Level,
    Markers,
    NotAGateRun,
    NotAVersion,
    RepoKind,
    Signal,
    Step,
    StepOutcome,
    UnknownLevel,
    Verdict,
    Version,
};
