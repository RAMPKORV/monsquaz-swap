//! Host commands for backups, the restore wizard, and the single-writer guard.
//!
//! The backup folder path stays in the main process (`backup.json` in the app
//! data folder). The UI receives only the folder display name and opaque
//! backup ids. Error messages pass through `redact` so that no absolute path
//! crosses IPC.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use swap_db::backup::{
    self, BackupKind, BackupOutcome, BackupTarget, DAILY_INTERVAL_SECS, Manifest, Retention,
    Staleness,
};
use swap_db::writer::{self, WriterState};
use swap_db::{Db, TransitionHook};
use tauri::{AppHandle, Manager, State};

const CONFIG_FILE: &str = "backup.json";
const MACHINE_ID_FILE: &str = "machine-id";
const DB_FILE: &str = "monsquaz.db";
const BLOB_DIR: &str = "files";

/// Process state: the folder that the restore wizard reads from, and the
/// message after this machine lost the writer lock.
#[derive(Default)]
pub struct BackupRuntime {
    restore_source: Mutex<Option<PathBuf>>,
    lost_lock: Mutex<Option<String>>,
}

impl BackupRuntime {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct BackupConfig {
    folder: Option<PathBuf>,
    keep_transition: usize,
    keep_daily: usize,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            folder: None,
            keep_transition: backup::DEFAULT_KEEP_TRANSITION,
            keep_daily: backup::DEFAULT_KEEP_DAILY,
        }
    }
}

impl BackupConfig {
    fn root(&self) -> Option<PathBuf> {
        self.folder.as_deref().map(backup::backup_root)
    }

    fn retention(&self) -> Retention {
        Retention {
            keep_transition: self.keep_transition,
            keep_daily: self.keep_daily,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEventDto {
    name: String,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupItemDto {
    id: String,
    kind: String,
    created_at: i64,
    complete: bool,
    events: Vec<BackupEventDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterDto {
    /// `free`, `own`, or `foreign`.
    state: String,
    machine: Option<String>,
    heartbeat_at: Option<i64>,
    lost_lock: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatusDto {
    folder_name: Option<String>,
    keep_transition: usize,
    keep_daily: usize,
    backups: Vec<BackupItemDto>,
    writer: WriterDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSourceDto {
    folder_name: Option<String>,
    backups: Vec<BackupItemDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreviewDto {
    id: String,
    kind: String,
    created_at: i64,
    app_version: String,
    files: usize,
    events: Vec<BackupEventDto>,
    /// `none`, `same`, `older`, `newer`, or `diverged`.
    staleness: String,
    newer_entries: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreDoneDto {
    events: Vec<BackupEventDto>,
    kept_previous: bool,
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "the app data folder is not available".to_string())?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("the app data folder is not writable: {error}"))?;
    Ok(directory)
}

fn read_config(data: &Path) -> BackupConfig {
    std::fs::read(data.join(CONFIG_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_config(data: &Path, config: &BackupConfig) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
    let partial = data.join(format!("{CONFIG_FILE}.partial"));
    std::fs::write(&partial, json).map_err(|error| error.to_string())?;
    std::fs::rename(&partial, data.join(CONFIG_FILE)).map_err(|error| error.to_string())
}

fn machine_id(data: &Path) -> Result<String, String> {
    writer::load_or_create_machine_id(&data.join(MACHINE_ID_FILE))
        .map_err(|error| format!("the machine id is not available: {error}"))
}

/// Replace every known absolute path in a message with a neutral label.
fn redact(message: impl ToString, data: &Path, folder: Option<&Path>) -> String {
    let mut text = message.to_string();
    let mut paths: Vec<(String, &str)> = vec![(data.display().to_string(), "the app data folder")];
    if let Some(folder) = folder {
        paths.push((folder.display().to_string(), "the backup folder"));
    }
    // Replace the longest path first, so a nested path is not cut in half.
    paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    for (path, label) in paths {
        if !path.is_empty() {
            text = text.replace(&path, label);
        }
    }
    text
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "Selected folder".to_string())
}

fn events(manifest: &Manifest) -> Vec<BackupEventDto> {
    manifest
        .events
        .iter()
        .map(|event| BackupEventDto {
            name: event.name.clone(),
            state: event.state.clone(),
        })
        .collect()
}

fn items(root: &Path) -> Result<Vec<BackupItemDto>, backup::BackupError> {
    Ok(backup::list_backups(root)?
        .into_iter()
        .map(|entry| match entry.manifest {
            Some(manifest) => BackupItemDto {
                id: entry.id,
                kind: manifest.kind.as_str().to_string(),
                created_at: manifest.created_at,
                complete: true,
                events: events(&manifest),
            },
            None => BackupItemDto {
                kind: entry.id.split('-').nth(1).unwrap_or("unknown").to_string(),
                id: entry.id,
                created_at: 0,
                complete: false,
                events: Vec::new(),
            },
        })
        .collect())
}

/// Take one backup now. Returns `None` when no backup folder is selected.
fn run_backup(
    data: &Path,
    db: &Db,
    kind: BackupKind,
    now: i64,
) -> Result<Option<BackupOutcome>, String> {
    let config = read_config(data);
    let Some(root) = config.root() else {
        return Ok(None);
    };
    let blob_root = data.join(BLOB_DIR);
    backup::create_backup(
        db,
        &BackupTarget {
            root: &root,
            blob_root: &blob_root,
            app_version: env!("CARGO_PKG_VERSION"),
        },
        kind,
        now,
        config.retention(),
    )
    .map(Some)
    .map_err(|error| redact(error, data, config.folder.as_deref()))
}

/// The pre-transition backup hook for a host command database handle. `None`
/// when the host has not selected a backup folder.
pub(crate) fn transition_hook(data: &Path) -> Option<TransitionHook> {
    read_config(data).folder.as_ref()?;
    let data = data.to_path_buf();
    Some(Box::new(move |db: &Db, _event_id: &str| {
        run_backup(&data, db, BackupKind::Transition, crate::host::now_secs()).map(|_| ())
    }))
}

/// Refuse to start the bot when another machine has a fresh heartbeat.
/// Otherwise record this machine as the writer.
pub(crate) fn claim_writer(data: &Path, now: i64) -> Result<(), String> {
    let config = read_config(data);
    let root = config.root();
    let me = machine_id(data)?;
    let db = Db::open(data.join(DB_FILE)).map_err(|error| redact(error, data, None))?;
    match writer::heartbeat(db.conn(), root.as_deref(), &me, now)
        .map_err(|error| redact(error, data, config.folder.as_deref()))?
    {
        WriterState::Foreign(lock) => Err(foreign_message(&lock, now)),
        WriterState::Free | WriterState::Own => Ok(()),
    }
}

fn foreign_message(lock: &writer::WriterLock, now: i64) -> String {
    let minutes = ((now - lock.heartbeat_at).max(0) + 59) / 60;
    format!(
        "Another machine ({}) ran this event {} minute{} ago. The bot stays off on this machine. Type TAKE OVER in Settings to run the event here.",
        lock.machine_label(),
        minutes,
        if minutes == 1 { "" } else { "s" }
    )
}

/// What the periodic timer did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TickOutcome {
    pub daily: bool,
    pub lost_lock: Option<String>,
}

/// One timer tick: write the heartbeat while the bot runs, and take the daily
/// backup when the newest daily backup is 24 hours old or older.
pub(crate) fn tick(data: &Path, bot_running: bool, now: i64) -> Result<TickOutcome, String> {
    let mut outcome = TickOutcome::default();
    let config = read_config(data);
    let db_path = data.join(DB_FILE);
    if !db_path.exists() {
        return Ok(outcome);
    }
    let db = Db::open(&db_path).map_err(|error| redact(error, data, None))?;
    if bot_running {
        let me = machine_id(data)?;
        let root = config.root();
        if let WriterState::Foreign(lock) = writer::heartbeat(db.conn(), root.as_deref(), &me, now)
            .map_err(|error| redact(error, data, config.folder.as_deref()))?
        {
            outcome.lost_lock = Some(format!(
                "Machine {} took over this event. The bot stopped on this machine.",
                lock.machine_label()
            ));
        }
    }
    if let Some(root) = config.root() {
        let last = backup::last_backup_at(&root, BackupKind::Daily)
            .map_err(|error| redact(error, data, config.folder.as_deref()))?;
        if last.is_none_or(|last| now - last >= DAILY_INTERVAL_SECS) {
            run_backup(data, &db, BackupKind::Daily, now)?;
            outcome.daily = true;
        }
    }
    Ok(outcome)
}

/// Start the timer: one tick every heartbeat interval.
pub fn spawn_timer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let period = std::time::Duration::from_secs(writer::HEARTBEAT_SECS as u64);
        loop {
            tokio::time::sleep(period).await;
            let Ok(data) = data_dir(&app) else {
                continue;
            };
            #[cfg(feature = "live")]
            let running = app.state::<crate::live_runtime::LiveRuntime>().is_running();
            #[cfg(not(feature = "live"))]
            let running = false;
            let now = crate::host::now_secs();
            let result =
                tauri::async_runtime::spawn_blocking(move || tick(&data, running, now)).await;
            match result {
                Ok(Ok(outcome)) => {
                    if let Some(message) = outcome.lost_lock {
                        #[cfg(feature = "live")]
                        {
                            let live = app.state::<crate::live_runtime::LiveRuntime>();
                            if let Err(error) = live.stop().await {
                                eprintln!("the bot did not stop after a takeover: {error}");
                            }
                        }
                        eprintln!("{message}");
                        *app.state::<BackupRuntime>()
                            .lost_lock
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(message);
                    }
                }
                Ok(Err(error)) => eprintln!("backup timer: {error}"),
                Err(error) => eprintln!("backup timer task failed: {error}"),
            }
        }
    });
}

fn status(app: &AppHandle, runtime: &BackupRuntime) -> Result<BackupStatusDto, String> {
    let data = data_dir(app)?;
    let config = read_config(&data);
    let redact_error = |error: backup::BackupError| redact(error, &data, config.folder.as_deref());
    let backups = match config.root() {
        Some(root) => items(&root).map_err(redact_error)?,
        None => Vec::new(),
    };
    let me = machine_id(&data)?;
    let db = Db::open(data.join(DB_FILE)).map_err(|error| redact(error, &data, None))?;
    let state = writer::check(
        db.conn(),
        config.root().as_deref(),
        &me,
        crate::host::now_secs(),
    )
    .map_err(|error| redact(error, &data, config.folder.as_deref()))?;
    let lost_lock = runtime
        .lost_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let writer = match state {
        WriterState::Free => WriterDto {
            state: "free".to_string(),
            machine: None,
            heartbeat_at: None,
            lost_lock,
        },
        WriterState::Own => WriterDto {
            state: "own".to_string(),
            machine: Some(me.chars().take(8).collect()),
            heartbeat_at: None,
            lost_lock,
        },
        WriterState::Foreign(lock) => WriterDto {
            state: "foreign".to_string(),
            machine: Some(lock.machine_label()),
            heartbeat_at: Some(lock.heartbeat_at),
            lost_lock,
        },
    };
    Ok(BackupStatusDto {
        folder_name: config.folder.as_deref().map(folder_name),
        keep_transition: config.keep_transition,
        keep_daily: config.keep_daily,
        backups,
        writer,
    })
}

async fn pick_folder(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = sender.send(folder);
    });
    let Some(folder) = receiver
        .await
        .map_err(|_| "the folder dialog closed".to_string())?
    else {
        return Ok(None);
    };
    let path = folder
        .into_path()
        .map_err(|_| "the selected folder is not a local folder".to_string())?;
    Ok(Some(path))
}

/// Refuse a backup folder inside the app data folder: a backup there does not
/// survive the loss of this machine's data.
fn check_folder(data: &Path, folder: &Path) -> Result<(), String> {
    let canonical = folder
        .canonicalize()
        .map_err(|_| "the selected folder is not readable".to_string())?;
    let data = data.canonicalize().unwrap_or_else(|_| data.to_path_buf());
    if canonical.starts_with(&data) {
        return Err("Select a folder outside the app data folder.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn host_backup_status(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, String> {
    status(&app, &runtime)
}

#[tauri::command]
pub async fn host_backup_choose_folder(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, String> {
    if let Some(folder) = pick_folder(&app).await? {
        let data = data_dir(&app)?;
        check_folder(&data, &folder)?;
        let mut config = read_config(&data);
        config.folder = Some(folder);
        write_config(&data, &config)?;
    }
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_backup_set_retention(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    keep_transition: usize,
    keep_daily: usize,
) -> Result<BackupStatusDto, String> {
    if !(1..=365).contains(&keep_transition) || !(1..=365).contains(&keep_daily) {
        return Err("Keep between 1 and 365 backups of each kind.".to_string());
    }
    let data = data_dir(&app)?;
    let mut config = read_config(&data);
    config.keep_transition = keep_transition;
    config.keep_daily = keep_daily;
    write_config(&data, &config)?;
    if let Some(root) = config.root() {
        backup::prune(&root, config.retention())
            .map_err(|error| redact(error, &data, config.folder.as_deref()))?;
    }
    status(&app, &runtime)
}

#[tauri::command]
pub async fn host_backup_now(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, String> {
    let data = data_dir(&app)?;
    let worker = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db = Db::open(worker.join(DB_FILE)).map_err(|error| redact(error, &worker, None))?;
        run_backup(&worker, &db, BackupKind::Manual, crate::host::now_secs())?
            .ok_or_else(|| "Select a backup folder first.".to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_backup_purge(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    confirmation: String,
) -> Result<BackupStatusDto, String> {
    let data = data_dir(&app)?;
    let config = read_config(&data);
    let root = config
        .root()
        .ok_or_else(|| "Select a backup folder first.".to_string())?;
    backup::purge(&root, &confirmation)
        .map_err(|error| redact(error, &data, config.folder.as_deref()))?;
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_writer_takeover(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    confirmation: String,
) -> Result<BackupStatusDto, String> {
    let data = data_dir(&app)?;
    let config = read_config(&data);
    let me = machine_id(&data)?;
    let db = Db::open(data.join(DB_FILE)).map_err(|error| redact(error, &data, None))?;
    writer::take_over(
        db.conn(),
        config.root().as_deref(),
        &me,
        crate::host::now_secs(),
        &confirmation,
    )
    .map_err(|error| redact(error, &data, config.folder.as_deref()))?;
    *runtime
        .lost_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    status(&app, &runtime)
}

fn restore_root(data: &Path, runtime: &BackupRuntime) -> Result<(PathBuf, PathBuf), String> {
    let chosen = runtime
        .restore_source
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let folder = chosen
        .or_else(|| read_config(data).folder)
        .ok_or_else(|| "Select the folder that holds the backups.".to_string())?;
    Ok((backup::backup_root(&folder), folder))
}

#[tauri::command]
pub async fn host_restore_choose_folder(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<RestoreSourceDto, String> {
    if let Some(folder) = pick_folder(&app).await? {
        *runtime
            .restore_source
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(folder);
    }
    host_restore_source(app, runtime)
}

#[tauri::command]
pub fn host_restore_source(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<RestoreSourceDto, String> {
    let data = data_dir(&app)?;
    let Ok((root, folder)) = restore_root(&data, &runtime) else {
        return Ok(RestoreSourceDto {
            folder_name: None,
            backups: Vec::new(),
        });
    };
    let backups = items(&root).map_err(|error| redact(error, &data, Some(&folder)))?;
    Ok(RestoreSourceDto {
        folder_name: Some(folder_name(&folder)),
        backups,
    })
}

fn preview(data: &Path, root: &Path, id: &str) -> Result<RestorePreviewDto, backup::BackupError> {
    let manifest = backup::verify_backup(root, id)?;
    let current = data.join(DB_FILE);
    let staleness = if current.exists() {
        let db = Db::open(&current)?;
        backup::compare_with_current(&manifest, db.conn())?
    } else {
        Staleness::NoCurrentData
    };
    let (label, newer_entries) = match staleness {
        Staleness::NoCurrentData => ("none", 0),
        Staleness::Same => ("same", 0),
        Staleness::BackupOlder {
            newer_audit_entries,
        } => ("older", newer_audit_entries),
        Staleness::BackupNewer => ("newer", 0),
        Staleness::Diverged => ("diverged", 0),
    };
    Ok(RestorePreviewDto {
        id: id.to_string(),
        kind: manifest.kind.as_str().to_string(),
        created_at: manifest.created_at,
        app_version: manifest.app_version.clone(),
        files: manifest.blobs.len(),
        events: events(&manifest),
        staleness: label.to_string(),
        newer_entries,
    })
}

#[tauri::command]
pub async fn host_restore_preview(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    id: String,
) -> Result<RestorePreviewDto, String> {
    let data = data_dir(&app)?;
    let (root, folder) = restore_root(&data, &runtime)?;
    tauri::async_runtime::spawn_blocking(move || {
        preview(&data, &root, &id).map_err(|error| redact(error, &data, Some(&folder)))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn host_restore(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    id: String,
    confirmation: String,
) -> Result<RestoreDoneDto, String> {
    if confirmation.trim() != backup::RESTORE_CONFIRMATION {
        return Err(format!("Type {} to confirm.", backup::RESTORE_CONFIRMATION));
    }
    let data = data_dir(&app)?;
    let (root, folder) = restore_root(&data, &runtime)?;
    // The restore replaces the database file: stop the only long-lived
    // connection first. The bot starts again only after the host enters the
    // token and the writer guard passes.
    #[cfg(feature = "live")]
    app.state::<crate::live_runtime::LiveRuntime>()
        .stop()
        .await
        .map_err(|error| format!("the bot did not stop, so the restore did not start: {error}"))?;
    let now = crate::host::now_secs();
    let report = tauri::async_runtime::spawn_blocking(move || {
        backup::restore_backup(
            &root,
            &id,
            &confirmation,
            &data,
            DB_FILE,
            &data.join(BLOB_DIR),
            now,
        )
        .map_err(|error| redact(error, &data, Some(&folder)))
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(RestoreDoneDto {
        events: events(&report.manifest),
        kept_previous: report.pre_restore_dir.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use swap_core::EventState;
    use swap_db::{EventRecord, EventRepo};

    const T0: i64 = 1_800_000_000;
    // Split so that the canary is not a secret-shaped literal in the source.
    const TOKEN: &str = concat!(
        "MTIzNDU2",
        "Nzg5MDEy",
        "MzQ1Njc4",
        ".GhIjKl.",
        "planted-token-canary-0123456789",
    );

    fn data_with_event(dir: &Path) -> PathBuf {
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let db = Db::open(data.join(DB_FILE)).unwrap();
        EventRepo::insert(
            db.conn(),
            &EventRecord {
                id: "evt-1".to_string(),
                name: "Night Shift Swap".to_string(),
                state: EventState::Signup,
                seed: 1,
                settings_json: "{}".to_string(),
                created_at: T0,
                updated_at: T0,
            },
        )
        .unwrap();
        data
    }

    fn select_folder(data: &Path, folder: &Path) {
        std::fs::create_dir_all(folder).unwrap();
        write_config(
            data,
            &BackupConfig {
                folder: Some(folder.to_path_buf()),
                ..BackupConfig::default()
            },
        )
        .unwrap();
    }

    fn files_under(path: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path).unwrap().flatten() {
            if entry.path().is_dir() {
                out.extend(files_under(&entry.path()));
            } else {
                out.push(entry.path());
            }
        }
        out
    }

    #[test]
    fn no_folder_means_no_hook_and_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        assert!(transition_hook(&data).is_none());
        let db = Db::open(data.join(DB_FILE)).unwrap();
        assert!(
            run_backup(&data, &db, BackupKind::Manual, T0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn the_daily_tick_backs_up_once_per_24_hours() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        select_folder(&data, &dir.path().join("usb"));
        assert!(tick(&data, false, T0).unwrap().daily);
        assert!(
            !tick(&data, false, T0 + DAILY_INTERVAL_SECS - 1)
                .unwrap()
                .daily
        );
        assert!(tick(&data, false, T0 + DAILY_INTERVAL_SECS).unwrap().daily);
    }

    #[test]
    fn the_tick_reports_a_lost_lock_and_does_not_overwrite_it() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        let usb = dir.path().join("usb");
        select_folder(&data, &usb);
        claim_writer(&data, T0).unwrap();
        // Another machine takes over through the shared backup folder.
        let other = tempfile::tempdir().unwrap();
        let other_db = Db::open(other.path().join("other.db")).unwrap();
        writer::take_over(
            other_db.conn(),
            Some(&backup::backup_root(&usb)),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            T0 + 30,
            writer::TAKEOVER_CONFIRMATION,
        )
        .unwrap();
        let outcome = tick(&data, true, T0 + 60).unwrap();
        assert!(outcome.lost_lock.unwrap().contains("bbbbbbbb"));
        let error = claim_writer(&data, T0 + 61).unwrap_err();
        assert!(error.contains("TAKE OVER"));
        // After the stale window the guard lets this machine start again.
        claim_writer(&data, T0 + 30 + writer::STALE_AFTER_SECS).unwrap();
    }

    #[test]
    fn a_folder_inside_the_app_data_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        let inside = data.join("backups");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(check_folder(&data, &inside).is_err());
        let outside = dir.path().join("usb");
        std::fs::create_dir_all(&outside).unwrap();
        check_folder(&data, &outside).unwrap();
    }

    #[test]
    fn errors_do_not_show_absolute_paths() {
        let data = Path::new("/home/host/.local/share/monsquaz");
        let folder = Path::new("/media/usb");
        let message = redact(
            "unable to open database file: /home/host/.local/share/monsquaz/monsquaz.db and /media/usb/x",
            data,
            Some(folder),
        );
        assert!(!message.contains("/home/host"));
        assert!(!message.contains("/media/usb"));
        assert!(message.contains("the app data folder"));
    }

    #[test]
    fn a_backup_never_contains_the_planted_bot_token() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        let usb = dir.path().join("usb");
        select_folder(&data, &usb);
        // Plant the token everywhere near the data that is not the keychain:
        // the settings file, a log file, and the process environment.
        std::fs::write(
            data.join("host-settings.json"),
            format!("{{\"botIdentity\":\"{TOKEN}\"}}"),
        )
        .unwrap();
        std::fs::write(data.join("app.log"), format!("Authorization: Bot {TOKEN}")).unwrap();
        let session = crate::setup_host::SessionTokenStore::default();
        crate::setup_host::TokenStore::set(&session, TOKEN).unwrap();

        let db = Db::open(data.join(DB_FILE)).unwrap();
        run_backup(&data, &db, BackupKind::Manual, T0)
            .unwrap()
            .unwrap();
        claim_writer(&data, T0).unwrap();
        assert!(tick(&data, true, T0 + DAILY_INTERVAL_SECS).unwrap().daily);

        let files = files_under(&usb);
        assert!(files.len() >= 4, "{files:?}");
        for path in files {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(TOKEN.len())
                    .any(|window| window == TOKEN.as_bytes()),
                "{} holds the bot token",
                path.display()
            );
        }
    }
}
