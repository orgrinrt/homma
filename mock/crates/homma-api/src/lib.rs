//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The vocabulary every other homma crate speaks.
//!
//! Types and the traits over them. No I/O of any kind lives here, which is what
//! lets the store, the registry and the command surface all depend on it without
//! depending on each other.

pub mod agent_hooks;
pub mod agent_nudges;
pub mod catalogue;
pub mod config;
pub mod contained;
pub mod denied;
pub mod deny_entry;
pub mod effort;
pub mod frontmatter;
pub mod git;
pub mod hooks;
pub mod keys;
pub mod layers;
pub mod mode;
pub mod path;
pub mod poster;
pub mod record;
pub mod reference;
pub mod release;
pub mod rule;
pub mod session;
pub mod skill;
pub mod version;
pub mod workspace_name;

pub use agent_hooks::{AGENT_EVENTS, AgentHook, AgentHooks, AgentTable, InvalidAgentHooks};
pub use agent_nudges::{Fault, InvalidNudge, Nudge, NudgeRow, Nudges, Strength, Ttl};
pub use catalogue::{LeaseConfig, Office};
pub use config::Workspace;
pub use config::identity::{Identity, Role, Staffing};
pub use config::paths::Paths;
pub use config::tables::{AskConfig, SearchConfig, SummaryConfig, WhatConfig};
pub use config::unreadable::{UNREADABLE, Unreadable};
pub use contained::{ContainedPath, Escapes, Root};
pub use denied::{Denied, Forbidden, NoHome, Standing};
pub use deny_entry::DenyEntry;
pub use effort::{Effort, NotAnEffort};
pub use frontmatter::{Block, FrontmatterError};
pub use git::{CommitIdentity, EmptyPart, Git, Part};
pub use hooks::{HookEntry, Hooks, InvalidHooks};
pub use keys::{Keys, keys_of};
pub use layers::{DEFAULT_LAYER, InvalidLayers, Layers};
pub use mode::{ModeMeta, ParsedMode};
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
    Target,
    UnknownLevel,
    Verdict,
    Version,
};
pub use rule::{MetaError, Parsed, RuleKind, RuleMeta};
pub use session::{Place, Session, SessionConfig, SessionRole, UnknownWord, role_file};
pub use skill::{SkillError, SkillMeta};
pub use workspace_name::{NotAWorkspaceName, WorkspaceName};
