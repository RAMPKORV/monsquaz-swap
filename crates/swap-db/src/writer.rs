//! Single-writer guard: one machine runs the bot for the data at a time.
//!
//! The lock is a machine id plus a heartbeat time in UTC seconds. The app keeps
//! one copy in the database (`meta`) and one copy in the backup folder. The
//! database copy travels with a restore; the folder copy is visible to every
//! machine that uses the same backup folder.
//!
//! A heartbeat from another machine is fresh when it is less than
//! [`STALE_AFTER_SECS`] old. A heartbeat in the future is fresh, so a clock
//! skew never lets two machines run. The host can always take over with a
//! typed confirmation.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::DbError;
use crate::repos::{AuditRepo, MetaRepo};

/// The bot writes a heartbeat at this interval, in seconds.
pub const HEARTBEAT_SECS: i64 = 60;
/// A heartbeat that is this old or older does not block another machine.
pub const STALE_AFTER_SECS: i64 = 5 * 60;
/// The typed words that confirm a takeover.
pub const TAKEOVER_CONFIRMATION: &str = "TAKE OVER";
/// The lock file name in the backup root.
pub const LOCK_FILE: &str = "writer-lock.json";
const META_KEY: &str = "writer.lock";

#[derive(Debug, Error)]
pub enum WriterError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("file system error: {0}")]
    Io(#[from] io::Error),
    #[error("type {TAKEOVER_CONFIRMATION} to confirm")]
    Confirmation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriterLock {
    pub machine_id: String,
    pub heartbeat_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriterState {
    /// No lock, or only stale locks from other machines.
    Free,
    /// The newest lock belongs to this machine.
    Own,
    /// Another machine has a fresh heartbeat.
    Foreign(WriterLock),
}

impl WriterLock {
    /// A short machine label for the host. The full id never leaves the app.
    pub fn machine_label(&self) -> String {
        self.machine_id.chars().take(8).collect()
    }
}

/// Decide the writer state from every lock copy. Pure; the clock is injected.
pub fn evaluate(locks: &[Option<WriterLock>], me: &str, now: i64) -> WriterState {
    let foreign = locks
        .iter()
        .flatten()
        .filter(|lock| lock.machine_id != me && now - lock.heartbeat_at < STALE_AFTER_SECS)
        .max_by_key(|lock| lock.heartbeat_at);
    if let Some(lock) = foreign {
        return WriterState::Foreign(lock.clone());
    }
    if locks.iter().flatten().any(|lock| lock.machine_id == me) {
        WriterState::Own
    } else {
        WriterState::Free
    }
}

pub fn read_db_lock(conn: &Connection) -> Result<Option<WriterLock>, WriterError> {
    Ok(MetaRepo::get(conn, META_KEY)?.and_then(|value| serde_json::from_str(&value).ok()))
}

/// Read the folder copy. A missing or unreadable file counts as no lock.
pub fn read_folder_lock(root: &Path) -> Option<WriterLock> {
    let path = root.join(LOCK_FILE);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn check(
    conn: &Connection,
    root: Option<&Path>,
    me: &str,
    now: i64,
) -> Result<WriterState, WriterError> {
    let locks = [read_db_lock(conn)?, root.and_then(read_folder_lock)];
    Ok(evaluate(&locks, me, now))
}

/// Write this machine's heartbeat unless another machine has a fresh one.
/// Returns the state before the write; `Foreign` means this machine must stop.
pub fn heartbeat(
    conn: &Connection,
    root: Option<&Path>,
    me: &str,
    now: i64,
) -> Result<WriterState, WriterError> {
    let state = check(conn, root, me, now)?;
    if matches!(state, WriterState::Foreign(_)) {
        return Ok(state);
    }
    write_lock(conn, root, me, now)?;
    Ok(state)
}

/// Record this machine as the writer after a typed confirmation, and write an
/// audit entry for every event in the database.
pub fn take_over(
    conn: &Connection,
    root: Option<&Path>,
    me: &str,
    now: i64,
    confirmation: &str,
) -> Result<(), WriterError> {
    if confirmation.trim() != TAKEOVER_CONFIRMATION {
        return Err(WriterError::Confirmation);
    }
    let previous = check(conn, root, me, now)?;
    write_lock(conn, root, me, now)?;
    let detail = serde_json::json!({
        "machine": &me[..me.len().min(8)],
        "previousMachine": match &previous {
            WriterState::Foreign(lock) => Some(lock.machine_label()),
            _ => None,
        },
    })
    .to_string();
    let mut statement = conn.prepare("SELECT id FROM events ORDER BY id")?;
    let events: Vec<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for event in events {
        AuditRepo::insert(conn, &event, now, "Host", "Writer takeover", None, &detail)?;
    }
    Ok(())
}

fn write_lock(
    conn: &Connection,
    root: Option<&Path>,
    me: &str,
    now: i64,
) -> Result<(), WriterError> {
    let lock = WriterLock {
        machine_id: me.to_string(),
        heartbeat_at: now,
    };
    let json = serde_json::to_string(&lock).map_err(DbError::from)?;
    MetaRepo::set(conn, META_KEY, &json)?;
    if let Some(root) = root {
        fs::create_dir_all(root)?;
        let path = root.join(LOCK_FILE);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(WriterError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the writer lock is a symbolic link",
            )));
        }
        let partial = root.join(format!("{LOCK_FILE}.partial"));
        let mut file = File::create(&partial)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        fs::rename(partial, path)?;
    }
    Ok(())
}

/// Read the machine id from `path`, or create a new random id there. The file
/// lives in the app data folder and is never part of a backup.
pub fn load_or_create_machine_id(path: &Path) -> io::Result<String> {
    if let Ok(value) = fs::read_to_string(path) {
        let value = value.trim();
        if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(value.to_string());
        }
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    let id = hex::encode(bytes);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const NOW: i64 = 1_800_000_000;

    fn lock(machine: &str, at: i64) -> Option<WriterLock> {
        Some(WriterLock {
            machine_id: machine.to_string(),
            heartbeat_at: at,
        })
    }

    #[test]
    fn no_lock_is_free() {
        assert_eq!(evaluate(&[None, None], ME, NOW), WriterState::Free);
    }

    #[test]
    fn a_foreign_heartbeat_one_second_before_the_limit_blocks() {
        let state = evaluate(&[lock(OTHER, NOW - STALE_AFTER_SECS + 1)], ME, NOW);
        assert!(matches!(state, WriterState::Foreign(_)));
    }

    #[test]
    fn a_foreign_heartbeat_at_the_limit_is_stale() {
        let state = evaluate(&[lock(OTHER, NOW - STALE_AFTER_SECS)], ME, NOW);
        assert_eq!(state, WriterState::Free);
    }

    #[test]
    fn a_foreign_heartbeat_in_the_future_blocks() {
        let state = evaluate(&[lock(OTHER, NOW + 86_400)], ME, NOW);
        assert!(matches!(state, WriterState::Foreign(_)));
    }

    #[test]
    fn a_fresh_folder_lock_wins_over_an_own_database_lock() {
        let state = evaluate(&[lock(ME, NOW - 10), lock(OTHER, NOW - 5)], ME, NOW);
        assert_eq!(state, WriterState::Foreign(lock(OTHER, NOW - 5).unwrap()));
    }

    #[test]
    fn an_own_lock_is_own_even_when_old() {
        let state = evaluate(&[lock(ME, NOW - 86_400), None], ME, NOW);
        assert_eq!(state, WriterState::Own);
    }

    #[test]
    fn the_freshest_foreign_lock_is_reported() {
        let state = evaluate(
            &[
                lock(OTHER, NOW - 100),
                lock("c".repeat(32).as_str(), NOW - 1),
            ],
            ME,
            NOW,
        );
        match state {
            WriterState::Foreign(lock) => assert_eq!(lock.heartbeat_at, NOW - 1),
            other => panic!("expected a foreign lock, got {other:?}"),
        }
    }

    #[test]
    fn the_machine_id_is_stable_and_random() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machine-id");
        let first = load_or_create_machine_id(&path).unwrap();
        assert_eq!(first.len(), 32);
        assert_eq!(load_or_create_machine_id(&path).unwrap(), first);
        let other = load_or_create_machine_id(&dir.path().join("other")).unwrap();
        assert_ne!(first, other);
    }
}
