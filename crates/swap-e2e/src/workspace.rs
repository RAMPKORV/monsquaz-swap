//! An isolated directory for one end-to-end run.

use std::io;
use std::path::{Path, PathBuf};

/// A temporary directory that holds the database, files, clock, and reports
/// for one run. The directory is deleted when the value drops.
#[derive(Debug)]
pub struct TempWorkspace {
    directory: tempfile::TempDir,
    label: String,
}

impl TempWorkspace {
    pub fn new(label: &str) -> io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(&format!("msw-e2e-{label}-"))
            .tempdir()?;
        Ok(Self {
            directory,
            label: label.to_string(),
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn root(&self) -> &Path {
        self.directory.path()
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    pub fn db(&self) -> PathBuf {
        self.path("monsquaz.db")
    }

    pub fn files(&self) -> PathBuf {
        self.path("files")
    }

    pub fn clock(&self) -> PathBuf {
        self.path("clock")
    }

    pub fn report(&self, name: &str) -> PathBuf {
        self.path(name)
    }

    /// Keep the directory after the value drops. Returns the root path.
    pub fn keep(self) -> PathBuf {
        self.directory.keep()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workspace_owns_its_files() {
        let workspace = TempWorkspace::new("unit").unwrap();
        let db = workspace.db();
        std::fs::write(&db, b"db").unwrap();
        assert!(db.exists());
        assert!(workspace.root().exists());
        assert!(workspace.files().ends_with("files"));
    }

    #[test]
    fn a_kept_workspace_survives_the_drop() {
        let workspace = TempWorkspace::new("keep").unwrap();
        let root = workspace.root().to_path_buf();
        let kept = workspace.keep();
        assert_eq!(root, kept);
        assert!(kept.exists());
        let _ = std::fs::remove_dir_all(kept);
    }
}
