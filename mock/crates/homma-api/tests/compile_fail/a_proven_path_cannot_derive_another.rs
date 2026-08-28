//! A `ContainedPath` does not itself carry a way to build a new path.
//!
//! It implemented `Deref<Target = Path>` for one round, which gave every
//! consumer `Path::join` and `Path::parent` on a proven path. Neither preserves
//! the proof, and `join` with an absolute argument discards the receiver
//! outright, so the guarantee was voidable by accident by anybody who did not
//! know to avoid it.
//!
//! **This pins the absence of that impl and nothing more, which is narrower than
//! the file's own title first read.** `ContainedPath::as_abs` is a declared
//! unwrap door, so a caller wanting `join` reaches it in two steps, and a review
//! found this file claiming a property the two-step route walks around. What the
//! missing `Deref` buys is that deriving a path is something somebody wrote down
//! rather than something that happened, and `Root::contain_under` exists so the
//! ordinary case never writes it at all.

use homma_api::{AbsPath, Denied, Root};

fn main() {
    let denied = Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap());
    let root = Root::new(&AbsPath::new("/srv/ws").unwrap(), denied).unwrap();
    let proven = root
        .contain(&AbsPath::new("/srv/ws/hands").unwrap())
        .unwrap();

    // Each of these compiled through `Deref` and produced an unproven path.
    let _ = proven.join("escape");
    let _ = proven.parent();
}
