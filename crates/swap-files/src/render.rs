//! A/B render validation and WAV preview generation through openmpt123.
//!
//! The renderer is a child process. It has a hard wall-clock limit and never
//! writes into the event library. A scrub is accepted only when the rendered
//! audio data matches exactly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use thiserror::Error;
use wait_timeout::ChildExt;

use crate::policy::{FileKind, sha256_hex};
use crate::scrub::{ScrubError, ScrubSummary, scrub_metadata};

pub const PREVIEW_SECONDS: u32 = 15;
const RENDER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("the renderer is not available: {0}")]
    Unavailable(String),
    #[error("the renderer did not finish within the time limit")]
    Timeout,
    #[error("the renderer failed: {0}")]
    Failed(String),
    #[error("the renderer produced invalid WAV data")]
    InvalidWav,
    #[error("the file extension is not safe")]
    InvalidExtension,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub trait ModuleRenderer: Send + Sync {
    fn render(
        &self,
        bytes: &[u8],
        extension: &str,
        max_seconds: u32,
    ) -> Result<Vec<u8>, RenderError>;
}

#[derive(Debug, Clone)]
pub struct OpenMptRenderer {
    binary: PathBuf,
}

impl Default for OpenMptRenderer {
    fn default() -> Self {
        Self::new("openmpt123")
    }
}

impl OpenMptRenderer {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Prefer the binary shipped next to the app, then the system PATH.
    pub fn discover() -> Result<Self, RenderError> {
        if let Ok(executable) = std::env::current_exe()
            && let Some(directory) = executable.parent()
        {
            let name = if cfg!(windows) {
                "openmpt123.exe"
            } else {
                "openmpt123"
            };
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(Self::new(candidate));
            }
        }
        let probe = Command::new("openmpt123")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if probe.map(|status| status.success()).unwrap_or(false) {
            return Ok(Self::default());
        }
        Err(RenderError::Unavailable(
            "openmpt123 was not found next to the app or on PATH".to_string(),
        ))
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }
}

impl ModuleRenderer for OpenMptRenderer {
    fn render(
        &self,
        bytes: &[u8],
        extension: &str,
        max_seconds: u32,
    ) -> Result<Vec<u8>, RenderError> {
        if !is_safe_extension(extension) {
            return Err(RenderError::InvalidExtension);
        }
        let max_seconds = max_seconds.clamp(1, 120);
        let directory = tempfile::tempdir()?;
        let input_name = format!("input.{extension}");
        let input_path = directory.path().join(&input_name);
        fs::write(&input_path, bytes)?;

        let mut child = Command::new(&self.binary)
            .current_dir(directory.path())
            .args([
                "--render",
                "--output-type",
                "wav",
                "--end-time",
                &max_seconds.to_string(),
                "--samplerate",
                "44100",
                "--channels",
                "2",
                "--no-float",
                "--dither",
                "0",
                "--banner",
                "0",
                "--no-progress",
                "--no-details",
                "--quiet",
                &input_name,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| RenderError::Unavailable(error.to_string()))?;

        match child.wait_timeout(RENDER_TIMEOUT)? {
            Some(status) if status.success() => {}
            Some(_) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(RenderError::Failed(stderr.trim().to_string()));
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RenderError::Timeout);
            }
        }

        let output_path = directory.path().join(format!("{input_name}.wav"));
        Ok(fs::read(output_path)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbResult {
    pub original_audio_sha256: String,
    pub scrubbed_audio_sha256: String,
    pub audio_matches: bool,
}

#[derive(Debug, Error)]
pub enum PrepareError {
    #[error(transparent)]
    Scrub(#[from] ScrubError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("the scrub changed the rendered audio")]
    AudioChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDelivery {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub original_sha256: String,
    pub summary: ScrubSummary,
    pub audio_matches: bool,
}

/// Scrub a module, render both versions, and return the scrubbed bytes only
/// when the audio data matches. The caller keeps the original bytes.
pub fn prepare_module(
    renderer: &dyn ModuleRenderer,
    original: &[u8],
    kind: FileKind,
    extension: &str,
    max_seconds: u32,
) -> Result<PreparedDelivery, PrepareError> {
    let scrubbed = scrub_metadata(original, kind)?;
    if !scrubbed.summary.changed() {
        return Ok(PreparedDelivery {
            bytes: original.to_vec(),
            sha256: sha256_hex(original),
            original_sha256: sha256_hex(original),
            summary: scrubbed.summary,
            audio_matches: true,
        });
    }

    let ab = compare_audio(renderer, original, &scrubbed.bytes, extension, max_seconds)?;
    if !ab.audio_matches {
        return Err(PrepareError::AudioChanged);
    }

    Ok(PreparedDelivery {
        sha256: sha256_hex(&scrubbed.bytes),
        original_sha256: sha256_hex(original),
        bytes: scrubbed.bytes,
        summary: scrubbed.summary,
        audio_matches: true,
    })
}

pub fn compare_audio(
    renderer: &dyn ModuleRenderer,
    original: &[u8],
    scrubbed: &[u8],
    extension: &str,
    max_seconds: u32,
) -> Result<AbResult, RenderError> {
    let original_wav = renderer.render(original, extension, max_seconds)?;
    let scrubbed_wav = renderer.render(scrubbed, extension, max_seconds)?;
    let original_audio = wav_audio_data(&original_wav)?;
    let scrubbed_audio = wav_audio_data(&scrubbed_wav)?;
    let original_audio_sha256 = sha256_hex(original_audio);
    let scrubbed_audio_sha256 = sha256_hex(scrubbed_audio);
    Ok(AbResult {
        audio_matches: original_audio == scrubbed_audio,
        original_audio_sha256,
        scrubbed_audio_sha256,
    })
}

pub fn render_preview(
    renderer: &dyn ModuleRenderer,
    bytes: &[u8],
    extension: &str,
    max_seconds: u32,
) -> Result<Vec<u8>, RenderError> {
    let wav = renderer.render(bytes, extension, max_seconds)?;
    wav_audio_data(&wav)?;
    Ok(wav)
}

/// Return the `data` chunk payload. WAV metadata before the audio (including a
/// title copied from the module) must not affect A/B validation.
pub fn wav_audio_data(wav: &[u8]) -> Result<&[u8], RenderError> {
    if wav.len() < 12 || &wav[..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err(RenderError::InvalidWav);
    }
    let mut cursor = 12usize;
    while cursor + 8 <= wav.len() {
        let id = &wav[cursor..cursor + 4];
        let size = u32::from_le_bytes([
            wav[cursor + 4],
            wav[cursor + 5],
            wav[cursor + 6],
            wav[cursor + 7],
        ]) as usize;
        let start = cursor + 8;
        let end = start.checked_add(size).ok_or(RenderError::InvalidWav)?;
        if end > wav.len() {
            return Err(RenderError::InvalidWav);
        }
        if id == b"data" {
            return Ok(&wav[start..end]);
        }
        cursor = end + (size % 2);
    }
    Err(RenderError::InvalidWav)
}

fn is_safe_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= 16
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeRenderer {
        audio: Vec<u8>,
    }

    impl ModuleRenderer for FakeRenderer {
        fn render(
            &self,
            _bytes: &[u8],
            _extension: &str,
            _max_seconds: u32,
        ) -> Result<Vec<u8>, RenderError> {
            Ok(wav_with_data(&self.audio))
        }
    }

    fn wav_with_data(data: &[u8]) -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&176_400u32.to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(data);
        wav
    }

    #[test]
    fn the_audio_chunk_is_extracted() {
        let wav = wav_with_data(&[1, 2, 3, 4]);
        assert_eq!(wav_audio_data(&wav).unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn a_bad_wav_is_refused() {
        assert!(matches!(
            wav_audio_data(b"not a wav"),
            Err(RenderError::InvalidWav)
        ));
    }

    #[test]
    fn a_matching_audio_render_is_accepted() {
        let renderer = FakeRenderer {
            audio: vec![1, 2, 3, 4],
        };
        let mut bytes = vec![0u8; 1084];
        bytes[..8].copy_from_slice(b"LeakyMOD");
        bytes[1080..1084].copy_from_slice(b"M.K.");
        let prepared = prepare_module(&renderer, &bytes, FileKind::Mod, "mod", 2).unwrap();
        assert!(prepared.audio_matches);
        assert!(prepared.summary.song_name);
        assert_ne!(prepared.sha256, prepared.original_sha256);
    }

    struct EchoRenderer;

    impl ModuleRenderer for EchoRenderer {
        fn render(
            &self,
            bytes: &[u8],
            _extension: &str,
            _max_seconds: u32,
        ) -> Result<Vec<u8>, RenderError> {
            Ok(wav_with_data(&bytes[..bytes.len().min(8)]))
        }
    }

    // scenario: Metadata is scrubbed and checked
    #[test]
    fn different_audio_is_rejected() {
        let renderer = EchoRenderer;
        let ab = compare_audio(&renderer, b"original", b"scrubbed", "mod", 2).unwrap();
        assert!(!ab.audio_matches);
        assert_ne!(ab.original_audio_sha256, ab.scrubbed_audio_sha256);
    }

    #[test]
    fn a_preview_must_be_a_valid_wav() {
        let renderer = FakeRenderer {
            audio: vec![1, 2, 3],
        };
        let wav = render_preview(&renderer, b"module", "mod", 2).unwrap();
        assert_eq!(wav_audio_data(&wav).unwrap(), &[1, 2, 3]);
    }
}
