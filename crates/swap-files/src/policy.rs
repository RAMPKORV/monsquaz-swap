use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const DEFAULT_MAX_SIZE: u64 = 20 * 1024 * 1024;

pub const DEFAULT_EXTENSIONS: &[&str] = &[
    "it", "mptm", "xm", "mod", "s3m", "mtm", "669", "far", "wav", "flac", "mp3", "ogg", "zip",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    It,
    Mptm,
    Xm,
    Mod,
    S3m,
    Mtm,
    SixSixNine,
    Far,
    Wav,
    Flac,
    Mp3,
    Ogg,
    Zip,
    Unknown,
}

impl FileKind {
    pub fn is_module(self) -> bool {
        matches!(
            self,
            Self::It
                | Self::Mptm
                | Self::Xm
                | Self::Mod
                | Self::S3m
                | Self::Mtm
                | Self::SixSixNine
                | Self::Far
        )
    }

    fn matches_extension(self, extension: &str) -> bool {
        matches!(
            (extension, self),
            ("it", Self::It)
                | ("mptm", Self::Mptm)
                | ("xm", Self::Xm)
                | ("mod", Self::Mod)
                | ("s3m", Self::S3m)
                | ("mtm", Self::Mtm)
                | ("669", Self::SixSixNine)
                | ("far", Self::Far)
                | ("wav", Self::Wav)
                | ("flac", Self::Flac)
                | ("mp3", Self::Mp3)
                | ("ogg", Self::Ogg)
                | ("zip", Self::Zip)
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FileError {
    #[error("the file is empty")]
    Empty,
    #[error("the file is {size} bytes; the maximum is {max} bytes")]
    TooLarge { size: u64, max: u64 },
    #[error("the file has no extension")]
    MissingExtension,
    #[error("the file extension is not allowed: {0}")]
    UnsupportedExtension(String),
    #[error("the file type is not recognized")]
    UnknownType,
    #[error("the extension .{extension} does not match the detected type {detected:?}")]
    TypeMismatch {
        extension: String,
        detected: FileKind,
    },
    #[error("the file name is not a plain file name")]
    InvalidName,
    #[error("the archive is unsafe: {0}")]
    UnsafeArchive(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedFile {
    pub extension: String,
    pub kind: FileKind,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct FilePolicy {
    allowed_extensions: BTreeSet<String>,
    max_size: u64,
}

impl Default for FilePolicy {
    fn default() -> Self {
        Self {
            allowed_extensions: DEFAULT_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            max_size: DEFAULT_MAX_SIZE,
        }
    }
}

impl FilePolicy {
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_size = max_size;
        self
    }

    pub fn allow_extension(&mut self, extension: &str) -> &mut Self {
        self.allowed_extensions
            .insert(extension.trim_start_matches('.').to_ascii_lowercase());
        self
    }

    pub fn is_extension_allowed(&self, extension: &str) -> bool {
        self.allowed_extensions.contains(extension)
    }

    /// Validate the outer container. The caller never uses the original name
    /// for storage; only the extension and the bytes matter.
    pub fn validate(&self, name: &str, bytes: &[u8]) -> Result<ValidatedFile, FileError> {
        let extension = extension_of(name)?;
        if !self.is_extension_allowed(&extension) {
            return Err(FileError::UnsupportedExtension(extension));
        }
        let size = bytes.len() as u64;
        if size == 0 {
            return Err(FileError::Empty);
        }
        if size > self.max_size {
            return Err(FileError::TooLarge {
                size,
                max: self.max_size,
            });
        }
        let kind = detect_kind(bytes, &extension);
        if kind == FileKind::Unknown {
            return Err(FileError::UnknownType);
        }
        if !kind.matches_extension(&extension) {
            return Err(FileError::TypeMismatch {
                extension,
                detected: kind,
            });
        }
        if kind == FileKind::Zip {
            scan_zip(bytes)?;
        }
        Ok(ValidatedFile {
            extension,
            kind,
            sha256: sha256_hex(bytes),
            size,
        })
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

const MAX_ARCHIVE_ENTRIES: usize = 100;
const MAX_ARCHIVE_UNCOMPRESSED: u64 = 200 * 1024 * 1024;
const MAX_ARCHIVE_RATIO: u64 = 100;

/// Bound the archive before anyone extracts it. A zip is not a trust boundary;
/// it is a container that can hide a bomb, a traversal name, or a symlink.
fn scan_zip(bytes: &[u8]) -> Result<(), FileError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| FileError::UnsafeArchive(format!("invalid zip: {error}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(FileError::UnsafeArchive("too many entries".to_string()));
    }
    let mut uncompressed_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| FileError::UnsafeArchive(format!("bad entry: {error}")))?;
        if entry.enclosed_name().is_none() {
            return Err(FileError::UnsafeArchive(
                "entry path escapes the archive".to_string(),
            ));
        }
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(FileError::UnsafeArchive("symlink entry".to_string()));
        }
        let size = entry.size();
        uncompressed_total = uncompressed_total.saturating_add(size);
        if uncompressed_total > MAX_ARCHIVE_UNCOMPRESSED {
            return Err(FileError::UnsafeArchive(
                "uncompressed size is too large".to_string(),
            ));
        }
        let compressed = entry.compressed_size();
        if compressed > 0 && size / compressed > MAX_ARCHIVE_RATIO {
            return Err(FileError::UnsafeArchive(
                "compression ratio is too high".to_string(),
            ));
        }
    }
    Ok(())
}

fn extension_of(name: &str) -> Result<String, FileError> {
    if name.is_empty() || name.contains('\0') || name.contains('/') || name.contains('\\') {
        return Err(FileError::InvalidName);
    }
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .ok_or(FileError::MissingExtension)
}

const MOD_SIGNATURES: &[&[u8; 4]] = &[
    b"M.K.", b"M!K!", b"M&K!", b"N.T.", b"FLT4", b"FLT8", b"4CHN", b"6CHN", b"8CHN", b"CD81",
    b"OKTA", b"OCTA", b"FA08",
];

pub fn detect_kind(bytes: &[u8], extension: &str) -> FileKind {
    if bytes.len() >= 4 && &bytes[..4] == b"IMPM" {
        return if extension == "mptm" {
            FileKind::Mptm
        } else {
            FileKind::It
        };
    }
    if bytes.len() >= 17 && &bytes[..17] == b"Extended Module: " {
        return FileKind::Xm;
    }
    if bytes.len() >= 0x30 && &bytes[0x2C..0x30] == b"SCRM" {
        return FileKind::S3m;
    }
    if bytes.len() >= 3 && &bytes[..3] == b"MTM" {
        return FileKind::Mtm;
    }
    if bytes.len() >= 4 && &bytes[..4] == b"FAR\xFE" {
        return FileKind::Far;
    }
    if bytes.len() >= 2 && extension == "669" && (&bytes[..2] == b"if" || &bytes[..2] == b"JN") {
        return FileKind::SixSixNine;
    }
    if bytes.len() >= 1084 {
        let sig = &bytes[1080..1084];
        if MOD_SIGNATURES
            .iter()
            .any(|candidate| sig == candidate.as_slice())
        {
            return FileKind::Mod;
        }
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return FileKind::Wav;
    }
    if bytes.len() >= 4 && &bytes[..4] == b"fLaC" {
        return FileKind::Flac;
    }
    if bytes.len() >= 4 && &bytes[..4] == b"OggS" {
        return FileKind::Ogg;
    }
    if (bytes.len() >= 3 && &bytes[..3] == b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0)
    {
        return FileKind::Mp3;
    }
    if bytes.len() >= 4
        && (&bytes[..4] == b"PK\x03\x04"
            || &bytes[..4] == b"PK\x05\x06"
            || &bytes[..4] == b"PK\x07\x08")
    {
        return FileKind::Zip;
    }
    FileKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; 256];
        bytes[..4].copy_from_slice(b"IMPM");
        bytes
    }

    fn xm_bytes() -> Vec<u8> {
        let mut bytes = b"Extended Module: ".to_vec();
        bytes.resize(256, 0);
        bytes
    }

    fn mod_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; 1200];
        bytes[1080..1084].copy_from_slice(b"M.K.");
        bytes
    }

    #[test]
    fn accepts_a_valid_it_file() {
        let validated = FilePolicy::default()
            .validate("song.it", &it_bytes())
            .unwrap();
        assert_eq!(validated.kind, FileKind::It);
        assert_eq!(validated.extension, "it");
        assert_eq!(validated.size, 256);
        assert_eq!(validated.sha256.len(), 64);
        assert!(validated.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn accepts_a_mod_file() {
        let validated = FilePolicy::default()
            .validate("song.mod", &mod_bytes())
            .unwrap();
        assert_eq!(validated.kind, FileKind::Mod);
    }

    #[test]
    fn rejects_an_empty_file() {
        assert_eq!(
            FilePolicy::default().validate("song.it", &[]),
            Err(FileError::Empty)
        );
    }

    #[test]
    fn rejects_a_disallowed_extension() {
        assert_eq!(
            FilePolicy::default().validate("song.exe", b"MZ"),
            Err(FileError::UnsupportedExtension("exe".to_string()))
        );
    }

    #[test]
    fn rejects_a_file_over_the_cap() {
        let policy = FilePolicy::default().with_max_size(4);
        assert_eq!(
            policy.validate("song.it", &[0u8; 5]),
            Err(FileError::TooLarge { size: 5, max: 4 })
        );
    }

    #[test]
    fn rejects_an_extension_mismatch() {
        assert_eq!(
            FilePolicy::default().validate("song.it", &xm_bytes()),
            Err(FileError::TypeMismatch {
                extension: "it".to_string(),
                detected: FileKind::Xm
            })
        );
    }

    #[test]
    fn rejects_an_unknown_type() {
        assert_eq!(
            FilePolicy::default().validate("song.it", b"not a module"),
            Err(FileError::UnknownType)
        );
    }

    #[test]
    fn rejects_a_path_in_the_name() {
        assert_eq!(
            FilePolicy::default().validate("../song.it", &it_bytes()),
            Err(FileError::InvalidName)
        );
    }

    #[test]
    fn the_hash_is_stable() {
        assert_eq!(sha256_hex(b"hello"), sha256_hex(b"hello"));
        assert_ne!(sha256_hex(b"hello"), sha256_hex(b"world"));
    }

    use std::io::{Cursor, Write};

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn accepts_a_safe_zip() {
        let bytes = zip_bytes(&[("song.it", &it_bytes())]);
        let validated = FilePolicy::default().validate("pack.zip", &bytes).unwrap();
        assert_eq!(validated.kind, FileKind::Zip);
    }

    #[test]
    fn rejects_a_malformed_zip() {
        let bytes = b"PK\x03\x04not really a zip".to_vec();
        assert!(matches!(
            FilePolicy::default().validate("pack.zip", &bytes),
            Err(FileError::UnsafeArchive(_))
        ));
    }

    #[test]
    fn rejects_a_zip_with_too_many_entries() {
        let names: Vec<String> = (0..101).map(|index| format!("file{index}.it")).collect();
        let data = it_bytes();
        let entries: Vec<(&str, &[u8])> = names
            .iter()
            .map(|name| (name.as_str(), data.as_slice()))
            .collect();
        let bytes = zip_bytes(&entries);
        assert!(matches!(
            FilePolicy::default().validate("pack.zip", &bytes),
            Err(FileError::UnsafeArchive(message)) if message == "too many entries"
        ));
    }

    #[test]
    fn rejects_a_zip_with_a_traversal_entry() {
        let bytes = zip_bytes(&[("../evil.it", &it_bytes())]);
        assert!(matches!(
            FilePolicy::default().validate("pack.zip", &bytes),
            Err(FileError::UnsafeArchive(message)) if message.contains("escapes")
        ));
    }
}
