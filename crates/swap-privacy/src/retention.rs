use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::PrivacyError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub kept_files: usize,
}

/// Remove regular backup files older than `cutoff`. Directories and symlinks
/// are skipped; the current event data is never touched by this function.
pub fn prune_backups(
    directory: &Path,
    cutoff: SystemTime,
) -> Result<RetentionReport, PrivacyError> {
    let mut report = RetentionReport::default();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() {
            continue;
        }
        if metadata.modified()? < cutoff {
            let size = metadata.len();
            fs::remove_file(entry.path())?;
            report.removed_files += 1;
            report.removed_bytes += size;
        } else {
            report.kept_files += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn old_files_are_removed_and_new_files_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("old.backup"), b"old").unwrap();
        fs::write(directory.path().join("new.backup"), b"new").unwrap();
        let report = prune_backups(
            directory.path(),
            SystemTime::now() + Duration::from_secs(3_600),
        )
        .unwrap();
        assert_eq!(report.removed_files, 2);
        assert_eq!(report.removed_bytes, 6);
        assert_eq!(report.kept_files, 0);
    }

    #[test]
    fn a_past_cutoff_keeps_everything() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("old.backup"), b"old").unwrap();
        let report = prune_backups(
            directory.path(),
            SystemTime::now() - Duration::from_secs(3_600),
        )
        .unwrap();
        assert_eq!(report.removed_files, 0);
        assert_eq!(report.kept_files, 1);
    }
}
