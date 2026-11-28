use std::fs;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::policy::sha256_hex;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("the hash is not a SHA-256 hex string")]
    InvalidHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlob {
    pub sha256: String,
    pub size: u64,
    pub relative_path: String,
}

/// Content-addressed blob store. The original file name is never used on disk.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn store(&self, bytes: &[u8]) -> Result<StoredBlob, StoreError> {
        let sha256 = sha256_hex(bytes);
        let prefix = &sha256[..2];
        let directory = self.root.join(prefix);
        fs::create_dir_all(&directory)?;
        let path = directory.join(&sha256);
        let relative_path = format!("{prefix}/{sha256}");

        if path.exists() {
            return Ok(StoredBlob {
                sha256,
                size: bytes.len() as u64,
                relative_path,
            });
        }

        let temporary = directory.join(format!("{sha256}.tmp"));
        fs::write(&temporary, bytes)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(StoreError::Io(error));
        }

        Ok(StoredBlob {
            sha256,
            size: bytes.len() as u64,
            relative_path,
        })
    }

    pub fn read(&self, sha256: &str) -> Result<Vec<u8>, StoreError> {
        if !is_sha256_hex(sha256) {
            return Err(StoreError::InvalidHash);
        }
        Ok(fs::read(self.root.join(&sha256[..2]).join(sha256))?)
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_bytes_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let blob = store.store(b"module bytes").unwrap();
        assert_eq!(blob.size, 12);
        assert_eq!(store.read(&blob.sha256).unwrap(), b"module bytes");
    }

    #[test]
    fn storing_the_same_bytes_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let first = store.store(b"same").unwrap();
        let second = store.store(b"same").unwrap();
        assert_eq!(first, second);
        let files = std::fs::read_dir(dir.path().join(&first.sha256[..2]))
            .unwrap()
            .count();
        assert_eq!(files, 1);
    }

    // scenario: Path traversal is blocked
    #[test]
    fn the_original_name_is_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let blob = store.store(b"name test").unwrap();
        assert!(!blob.relative_path.contains("song.it"));
        assert_eq!(
            blob.relative_path,
            format!("{}/{}", &blob.sha256[..2], blob.sha256)
        );
    }

    #[test]
    fn a_traversal_hash_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        assert!(matches!(
            store.read("../../etc/passwd"),
            Err(StoreError::InvalidHash)
        ));
    }
}
