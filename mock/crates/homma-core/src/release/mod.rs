//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release: a gate that measures a commit, the record and the commit
//! status a run leaves, and the merge, tag, changelog, GitHub release and
//! registry publish that carry `dev` onto `main`. `DEEPDIVE_release.md` in
//! this crate's design is the whole of it; each module here is one section.

pub mod badges;
pub mod changelog;
pub mod check;
pub mod gate;
pub mod git;
pub mod hook;
pub mod kind;
pub mod numbers;
pub mod plan;
pub mod publish;
pub mod registry;
pub mod sh;
pub mod status;
pub mod version;
