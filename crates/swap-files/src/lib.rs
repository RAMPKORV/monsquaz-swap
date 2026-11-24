//! File intake policy, content hashing, and the content-addressed store.
//!
//! This crate does not parse module internals yet. It validates the outer
//! container, enforces the allowlist and size cap, and stores bytes by hash.
//! Metadata scrubbing and module parsing land in the next slice.

pub mod pack;
pub mod policy;
pub mod store;

pub use pack::{PackManifest, PackReport, SampleInfo, check_samples};
pub use policy::{
    DEFAULT_EXTENSIONS, DEFAULT_MAX_SIZE, FileError, FileKind, FilePolicy, ValidatedFile,
    sha256_hex,
};
pub use store::{BlobStore, StoreError, StoredBlob};
