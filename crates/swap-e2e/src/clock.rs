//! A file-backed clock for end-to-end tests.
//!
//! The test harness writes an epoch second value to one file. The engine, the
//! delivery worker, and the host commands read the same file through
//! `MSW_E2E_CLOCK`. Time moves only when the test moves it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use swap_engine::Clock;

/// An epoch-second value that lives on disk.
#[derive(Debug, Clone)]
pub struct FileClock {
    path: PathBuf,
}

impl FileClock {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write a new epoch second value. The write is atomic.
    pub fn set(&self, epoch_seconds: i64) -> io::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("the clock path has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        {
            let mut file = fs::File::create(&temporary)?;
            writeln!(file, "{epoch_seconds}")?;
            file.sync_all()?;
        }
        fs::rename(&temporary, &self.path)
    }

    /// Read the stored value. A missing or malformed file returns `None`.
    pub fn read(&self) -> Option<i64> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|value| value.trim().parse().ok())
    }

    /// Add a duration and return the new value.
    pub fn advance(&self, seconds: i64) -> io::Result<i64> {
        let next = self.now_seconds().saturating_add(seconds);
        self.set(next)?;
        Ok(next)
    }

    /// Read the stored value, or the system time when the file is absent.
    pub fn now_seconds(&self) -> i64 {
        self.read().unwrap_or_else(system_now)
    }
}

impl Clock for FileClock {
    fn now(&self) -> i64 {
        self.now_seconds()
    }
}

/// The system time in epoch seconds. This is the fallback for a missing file.
pub fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Read the clock path from `MSW_E2E_CLOCK` when `MSW_E2E=1`.
///
/// This is the same rule that the host command module uses. Tests call this to
/// point the harness and the host at one clock.
pub fn clock_from_env() -> Option<FileClock> {
    if std::env::var("MSW_E2E").ok().as_deref() != Some("1") {
        return None;
    }
    let path = std::env::var("MSW_E2E_CLOCK").ok()?;
    if path.trim().is_empty() {
        return None;
    }
    Some(FileClock::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_round_trips_and_advances() {
        let directory = tempfile::tempdir().unwrap();
        let clock = FileClock::new(directory.path().join("clock"));
        clock.set(1_700_000_000).unwrap();
        assert_eq!(clock.now(), 1_700_000_000);
        assert_eq!(clock.advance(3600).unwrap(), 1_700_003_600);
        assert_eq!(clock.read(), Some(1_700_003_600));
    }

    #[test]
    fn a_missing_file_falls_back_to_the_system_time() {
        let directory = tempfile::tempdir().unwrap();
        let clock = FileClock::new(directory.path().join("absent"));
        let now = clock.now_seconds();
        assert!((system_now() - now).abs() < 5);
    }

    #[test]
    fn a_malformed_file_falls_back_to_the_system_time() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("clock");
        fs::write(&path, "not a number").unwrap();
        let clock = FileClock::new(path);
        assert_eq!(clock.read(), None);
        assert!((system_now() - clock.now_seconds()).abs() < 5);
    }
}
