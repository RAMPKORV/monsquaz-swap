//! Executable acceptance tests for the core invariants.
//!
//! This crate intentionally contains no production code. It links the pure
//! domain crate and runs the hand-written Gherkin features in `specs/features`.

pub const ACCEPTANCE_CRATE: &str = "swap-acceptance";
