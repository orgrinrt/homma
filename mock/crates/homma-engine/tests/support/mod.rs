//! Shared test support.
//!
//! Re-exported rather than redefined. This helper existed in two crates, was
//! fixed in one, and the round that fixed it added a third copy. It now has one
//! definition, in `homma_core::testing`, and this module exists only so the
//! integration tests can write `support::` instead of naming the crate.

pub use homma_core::testing::global_configs_now;
