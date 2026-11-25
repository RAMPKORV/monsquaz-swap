//! File intake policy, module metadata scrub, A/B render validation, and the
//! content-addressed store.
//!
//! The scrub is conservative and format-aware. It removes identity text from
//! IT/MPTM/XM/S3M/MOD and refuses a malformed header. The renderer runs in a
//! child process; a scrub is accepted only when the WAV audio data matches.

pub mod pack;
pub mod policy;
pub mod render;
pub mod scrub;
pub mod store;

pub use pack::{PackManifest, PackReport, SampleInfo, check_samples};
pub use policy::{
    DEFAULT_EXTENSIONS, DEFAULT_MAX_SIZE, FileError, FileKind, FilePolicy, ValidatedFile,
    sha256_hex,
};
pub use render::{
    AbResult, ModuleRenderer, OpenMptRenderer, PREVIEW_SECONDS, PrepareError, PreparedDelivery,
    RenderError, compare_audio, prepare_module, render_preview, wav_audio_data,
};
pub use scrub::{ScrubError, ScrubSummary, Scrubbed, scrub_metadata};
pub use store::{BlobStore, StoreError, StoredBlob};
