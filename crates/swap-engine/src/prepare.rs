use swap_files::{FileKind, OpenMptRenderer, PrepareError, prepare_module, sha256_hex};

/// Seconds rendered for the A/B audio comparison. A short window is enough to
/// detect a changed arrangement and keeps submission latency bounded.
pub const AB_SECONDS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBytes {
    pub delivery: Vec<u8>,
    pub original_sha256: String,
    pub delivery_sha256: String,
    pub audio_verified: bool,
    pub scrub_summary_json: String,
}

pub trait DeliveryPreparer: Send + Sync {
    fn prepare(
        &self,
        bytes: &[u8],
        kind: FileKind,
        extension: &str,
    ) -> Result<PreparedBytes, PrepareError>;
}

/// Use in tests and for kinds that have no module scrub. The delivery bytes
/// and the original bytes are the same.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughPreparer;

impl DeliveryPreparer for PassthroughPreparer {
    fn prepare(
        &self,
        bytes: &[u8],
        _kind: FileKind,
        _extension: &str,
    ) -> Result<PreparedBytes, PrepareError> {
        let sha256 = sha256_hex(bytes);
        Ok(PreparedBytes {
            delivery: bytes.to_vec(),
            original_sha256: sha256.clone(),
            delivery_sha256: sha256,
            audio_verified: false,
            scrub_summary_json: "{}".to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenMptPreparer {
    renderer: OpenMptRenderer,
}

impl Default for OpenMptPreparer {
    fn default() -> Self {
        Self::new(OpenMptRenderer::default())
    }
}

impl OpenMptPreparer {
    pub fn new(renderer: OpenMptRenderer) -> Self {
        Self { renderer }
    }

    pub fn discover() -> Result<Self, PrepareError> {
        Ok(Self::new(OpenMptRenderer::discover()?))
    }
}

impl DeliveryPreparer for OpenMptPreparer {
    fn prepare(
        &self,
        bytes: &[u8],
        kind: FileKind,
        extension: &str,
    ) -> Result<PreparedBytes, PrepareError> {
        if !kind.is_module() {
            return PassthroughPreparer.prepare(bytes, kind, extension);
        }
        let prepared = prepare_module(&self.renderer, bytes, kind, extension, AB_SECONDS)?;
        let summary = serde_json::json!({
            "song_name": prepared.summary.song_name,
            "tracker_name": prepared.summary.tracker_name,
            "message_bytes": prepared.summary.message_bytes,
            "instrument_names": prepared.summary.instrument_names,
            "sample_names": prepared.summary.sample_names,
            "file_names": prepared.summary.file_names,
        })
        .to_string();
        Ok(PreparedBytes {
            delivery: prepared.bytes,
            original_sha256: prepared.original_sha256,
            delivery_sha256: prepared.sha256,
            audio_verified: prepared.audio_matches,
            scrub_summary_json: summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passthrough_keeps_the_bytes() {
        let prepared = PassthroughPreparer
            .prepare(b"bytes", FileKind::Wav, "wav")
            .unwrap();
        assert_eq!(prepared.delivery, b"bytes");
        assert_eq!(prepared.original_sha256, prepared.delivery_sha256);
        assert!(!prepared.audio_verified);
    }

    #[test]
    fn a_non_module_uses_the_passthrough() {
        let prepared = OpenMptPreparer::default()
            .prepare(b"audio", FileKind::Wav, "wav")
            .unwrap();
        assert_eq!(prepared.delivery, b"audio");
        assert_eq!(prepared.scrub_summary_json, "{}");
    }

    // scenario: Metadata is scrubbed and checked
    #[test]
    fn a_module_is_scrubbed_and_audio_verified() {
        let Ok(renderer) = OpenMptRenderer::discover() else {
            return;
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Leaky preparation title");
        bytes.resize(20, 0);
        for index in 0..31 {
            bytes.extend_from_slice(format!("Sample {index} secret").as_bytes());
            bytes.resize(20 + (index + 1) * 30, 0);
        }
        bytes.extend_from_slice(&[1, 0]);
        bytes.resize(20 + 31 * 30 + 2 + 128, 0);
        bytes.extend_from_slice(b"M.K.");
        bytes.resize(1084 + 1024, 0);
        let prepared = OpenMptPreparer::new(renderer)
            .prepare(&bytes, FileKind::Mod, "mod")
            .unwrap();
        assert!(prepared.audio_verified);
        assert_ne!(prepared.original_sha256, prepared.delivery_sha256);
        assert!(prepared.scrub_summary_json.contains("\"song_name\":true"));
        assert!(prepared.delivery[..20].iter().all(|byte| *byte == 0));
    }
}
