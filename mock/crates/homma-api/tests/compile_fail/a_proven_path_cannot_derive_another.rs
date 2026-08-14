//! A `ContainedPath` must not hand out a way to build a new path from itself.
//!
//! It implemented `Deref<Target = Path>` for one round, which gave every
//! consumer `Path::join` and `Path::parent` on a proven path. Neither preserves
//! the proof, and `join` with an absolute argument discards the receiver
//! outright, so the guarantee was voidable by accident by anybody who did not
//! know to avoid it.
//!
//! Removing the impl is the fix. This file is what stops a later round adding it
//! back for convenience, since nothing else in the suite names the absence.

use homma_api::{AbsPath, Root};

fn main() {
    let root = Root::new(&AbsPath::new("/srv/ws").unwrap()).unwrap();
    let proven = root
        .contain(&AbsPath::new("/srv/ws/hands").unwrap())
        .unwrap();

    // Each of these compiled through `Deref` and produced an unproven path.
    let _ = proven.join("escape");
    let _ = proven.parent();
}
