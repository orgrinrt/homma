//! The vocabulary every other homma crate speaks.
//!
//! Types and the traits over them. No I/O of any kind lives here, which is what
//! lets the store, the registry and the command surface all depend on it without
//! depending on each other.

pub mod config;
pub mod contained;
pub mod denied;
pub mod git;
pub mod path;
pub mod record;
pub mod reference;

pub use config::{Identity, Paths, Role, Staffing, Workspace};
pub use contained::{ContainedPath, Escapes, Root};
pub use denied::{Denied, Forbidden};
pub use git::Git;
pub use path::{AbsPath, NotAbsolute};
pub use record::{Attr, AttrType, Invalid, Kind, Mutability, Record};
pub use reference::{Namespace, NotAReference, Reference, Rung};
