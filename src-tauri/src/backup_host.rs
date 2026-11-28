//! Host commands for backups, the restore wizard, and the single-writer guard.
//!
//! The backup folder path stays in the main process (`backup.json` in the app
//! data folder). The UI receives only the folder display name and opaque
//! backup ids. Error messages pass through `redact` so that no absolute path
//! crosses IPC.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use swap_db::backup::{
    self, BackupKind, BackupOutcome, BackupTarget, DAILY_INTERVAL_SECS, Manifest, Retention,
    Staleness,
};
use swap_db::writer::{self, WriterState};
use swap_db::{AuditRepo, Db, MetaRepo, TransitionHook};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

const CONFIG_FILE: &str = "backup.json";
const MACHINE_ID_FILE: &str = "machine-id";
const DB_FILE: &str = "monsquaz.db";
const BLOB_DIR: &str = "files";
const PRE_RESTORE_DIR: &str = "pre-restore";

pub(crate) const CODE_FOLDER: &str = "MSW-BACKUP-001";
pub(crate) const CODE_DAMAGED: &str = "MSW-BACKUP-002";
pub(crate) const CODE_WRITE: &str = "MSW-BACKUP-003";
pub(crate) const CODE_CONFIRM: &str = "MSW-BACKUP-004";
pub(crate) const CODE_BOT_LOCK: &str = "MSW-BACKUP-005";

/// A command error for the host UI: host copy, a stable code, and technical
/// detail that stays secondary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl HostError {
    fn folder(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_FOLDER.to_string(),
            message: "The app cannot read the backup folder. Connect the drive, then choose Retry. If the folder moved, choose Change folder. The app does not change the event state while the backup fails.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn damaged(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_DAMAGED.to_string(),
            message: "The app cannot use this backup. One file is damaged. The app did not change the event data. Choose another backup, then choose Check backup.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn write(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_WRITE.to_string(),
            message: "The app could not write the backup. The app did not change the event state. Connect the drive, then choose Back up now.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn restore(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_WRITE.to_string(),
            message: "The app could not restore the backup. The app did not change the event data. Check the free space, then choose Restore again.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn purge(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_WRITE.to_string(),
            message: "The app could not delete every backup. Connect the drive, then choose Delete backups again.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn confirmation(word: &str) -> Self {
        Self {
            code: CODE_CONFIRM.to_string(),
            message: format!("Type {word} in capital letters to continue."),
            detail: None,
        }
    }

    fn bot_lock(detail: impl Into<String>) -> Self {
        Self {
            code: CODE_BOT_LOCK.to_string(),
            message: "The app could not use the bot lock. Stop the bot on the other computer, then choose Take over.".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn no_folder() -> Self {
        Self {
            code: CODE_FOLDER.to_string(),
            message: "No backup folder is selected. Choose a folder first.".to_string(),
            detail: None,
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// The last automatic or manual backup failure, kept in process memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFailureDto {
    pub at: i64,
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

type FailureSink = Arc<Mutex<Option<BackupFailureDto>>>;

/// Process state: the folder that the restore wizard reads from, the time this
/// computer lost the bot lock, and the last backup failure.
#[derive(Default)]
pub struct BackupRuntime {
    restore_source: Mutex<Option<PathBuf>>,
    lost_lock_at: Mutex<Option<i64>>,
    last_failure: FailureSink,
}

impl BackupRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn failure_sink(&self) -> FailureSink {
        self.last_failure.clone()
    }
}

fn record_failure(sink: &FailureSink, at: i64, error: &HostError) {
    *sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(BackupFailureDto {
        at,
        code: error.code.clone(),
        message: error.message.clone(),
        detail: error.detail.clone(),
    });
}

fn clear_failure(sink: &FailureSink) {
    *sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
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
    heartbeat_at: Option<i64>,
    lost_lock_at: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatusDto {
    folder_name: Option<String>,
    folder_error: Option<HostError>,
    last_failure: Option<BackupFailureDto>,
    checked_at: i64,
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
        .map_err(|_| "the app data folder is not writable".to_string())?;
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

fn map_folder_error(error: backup::BackupError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        backup::BackupError::Confirmation(word) => HostError::confirmation(word),
        other => HostError::folder(redact(other, data, folder)),
    }
}

fn map_create_error(error: backup::BackupError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        backup::BackupError::Confirmation(word) => HostError::confirmation(word),
        other => HostError::write(redact(other, data, folder)),
    }
}

fn map_preview_error(error: backup::BackupError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        backup::BackupError::Confirmation(word) => HostError::confirmation(word),
        backup::BackupError::NotFound | backup::BackupError::InvalidId => {
            HostError::folder("The backup is no longer in the folder.")
        }
        backup::BackupError::Io(error) => HostError::folder(redact(error, data, folder)),
        other => HostError::damaged(redact(other, data, folder)),
    }
}

fn map_restore_error(error: backup::BackupError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        backup::BackupError::Confirmation(word) => HostError::confirmation(word),
        backup::BackupError::NotFound | backup::BackupError::InvalidId => {
            HostError::folder("The backup is no longer in the folder.")
        }
        backup::BackupError::Io(error) => HostError::restore(redact(error, data, folder)),
        backup::BackupError::Db(error) => HostError::restore(redact(error, data, folder)),
        backup::BackupError::Sqlite(error) => HostError::restore(redact(error, data, folder)),
        other => HostError::damaged(redact(other, data, folder)),
    }
}

fn map_purge_error(error: backup::BackupError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        backup::BackupError::Confirmation(word) => HostError::confirmation(word),
        other => HostError::purge(redact(other, data, folder)),
    }
}

fn map_writer_error(error: writer::WriterError, data: &Path, folder: Option<&Path>) -> HostError {
    match error {
        writer::WriterError::Confirmation => HostError::confirmation(writer::TAKEOVER_CONFIRMATION),
        other => HostError::bot_lock(redact(other, data, folder)),
    }
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
) -> Result<Option<BackupOutcome>, HostError> {
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
    .map_err(|error| map_create_error(error, data, config.folder.as_deref()))
}

/// The pre-transition backup hook for a host command database handle. `None`
/// when the host has not selected a backup folder.
pub(crate) fn transition_hook(data: &Path, failures: FailureSink) -> Option<TransitionHook> {
    read_config(data).folder.as_ref()?;
    let data = data.to_path_buf();
    Some(Box::new(move |db: &Db, _event_id: &str| {
        let now = crate::host::now_secs();
        match run_backup(&data, db, BackupKind::Transition, now) {
            Ok(_) => {
                clear_failure(&failures);
                Ok(())
            }
            Err(error) => {
                record_failure(&failures, now, &error);
                Err(format!("{}: {}", error.code, error.message))
            }
        }
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
        WriterState::Foreign(_) => Err(foreign_message()),
        WriterState::Free | WriterState::Own => Ok(()),
    }
}

fn foreign_message() -> String {
    format!(
        "{CODE_BOT_LOCK}: Another computer holds the bot lock. The app keeps the bot off on this computer. The lock clears 5 minutes after the last check. Choose Take over in Settings only if you stopped the bot on that computer. Then choose Reconnect bot to start the bot."
    )
}

/// What the periodic timer did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TickOutcome {
    pub daily: bool,
    pub lost_lock_at: Option<i64>,
}

/// One timer tick: write the heartbeat while the bot runs, and take the daily
/// backup when the newest daily backup is 24 hours old or older.
pub(crate) fn tick(
    data: &Path,
    bot_running: bool,
    now: i64,
    failures: &FailureSink,
) -> Result<TickOutcome, HostError> {
    let mut outcome = TickOutcome::default();
    let config = read_config(data);
    let db_path = data.join(DB_FILE);
    if !db_path.exists() {
        return Ok(outcome);
    }
    let db = Db::open(&db_path).map_err(|error| HostError::write(redact(error, data, None)))?;
    if bot_running {
        let me = machine_id(data).map_err(HostError::bot_lock)?;
        let root = config.root();
        if let WriterState::Foreign(_) = writer::heartbeat(db.conn(), root.as_deref(), &me, now)
            .map_err(|error| map_writer_error(error, data, config.folder.as_deref()))?
        {
            outcome.lost_lock_at = Some(now);
        }
    }
    if let Some(root) = config.root() {
        let last = match backup::last_backup_at(&root, BackupKind::Daily) {
            Ok(last) => last,
            Err(error) => {
                let error = map_create_error(error, data, config.folder.as_deref());
                record_failure(failures, now, &error);
                return Err(error);
            }
        };
        if last.is_none_or(|last| now - last >= DAILY_INTERVAL_SECS) {
            match run_backup(data, &db, BackupKind::Daily, now) {
                Ok(_) => {
                    clear_failure(failures);
                    outcome.daily = true;
                }
                Err(error) => {
                    record_failure(failures, now, &error);
                    return Err(error);
                }
            }
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
            let failures = app.state::<BackupRuntime>().failure_sink();
            let result =
                tauri::async_runtime::spawn_blocking(move || tick(&data, running, now, &failures))
                    .await;
            match result {
                Ok(Ok(outcome)) => {
                    if let Some(at) = outcome.lost_lock_at {
                        #[cfg(feature = "live")]
                        {
                            let live = app.state::<crate::live_runtime::LiveRuntime>();
                            if let Err(error) = live.stop().await {
                                eprintln!("the bot did not stop after a takeover: {error}");
                            }
                        }
                        *app.state::<BackupRuntime>()
                            .lost_lock_at
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(at);
                    }
                }
                Ok(Err(error)) => eprintln!("backup timer: {error}"),
                Err(error) => eprintln!("backup timer task failed: {error}"),
            }
        }
    });
}

fn status(app: &AppHandle, runtime: &BackupRuntime) -> Result<BackupStatusDto, HostError> {
    let data = data_dir(app).map_err(HostError::write)?;
    let config = read_config(&data);
    let now = crate::host::now_secs();
    let mut folder_error = None;
    let backups = match config.root() {
        Some(root) => match items(&root) {
            Ok(backups) => backups,
            Err(error) => {
                folder_error = Some(map_folder_error(error, &data, config.folder.as_deref()));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let me = machine_id(&data).map_err(HostError::bot_lock)?;
    let db = Db::open(data.join(DB_FILE))
        .map_err(|error| HostError::write(redact(error, &data, None)))?;
    let state = writer::check(db.conn(), config.root().as_deref(), &me, now)
        .map_err(|error| map_writer_error(error, &data, config.folder.as_deref()))?;
    let mut lost_lock_at = *runtime
        .lost_lock_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let writer = match state {
        WriterState::Free => {
            lost_lock_at = None;
            WriterDto {
                state: "free".to_string(),
                heartbeat_at: None,
                lost_lock_at: None,
            }
        }
        WriterState::Own => {
            lost_lock_at = None;
            WriterDto {
                state: "own".to_string(),
                heartbeat_at: None,
                lost_lock_at: None,
            }
        }
        WriterState::Foreign(lock) => WriterDto {
            state: "foreign".to_string(),
            heartbeat_at: Some(lock.heartbeat_at),
            lost_lock_at,
        },
    };
    *runtime
        .lost_lock_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = lost_lock_at;
    let last_failure = runtime
        .last_failure
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Ok(BackupStatusDto {
        folder_name: config.folder.as_deref().map(folder_name),
        folder_error,
        last_failure,
        checked_at: now,
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
/// survive the loss of this computer's data.
fn check_folder(data: &Path, folder: &Path) -> Result<(), HostError> {
    let canonical = folder
        .canonicalize()
        .map_err(|_| HostError::folder("The selected folder is not readable."))?;
    let data = data.canonicalize().unwrap_or_else(|_| data.to_path_buf());
    if canonical.starts_with(&data) {
        return Err(HostError::folder(
            "The app data folder does not survive a disk loss. Choose another folder.",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn host_backup_status(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, HostError> {
    status(&app, &runtime)
}

#[tauri::command]
pub async fn host_backup_choose_folder(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, HostError> {
    let folder = pick_folder(&app).await.map_err(HostError::folder)?;
    if let Some(folder) = folder {
        let data = data_dir(&app).map_err(HostError::write)?;
        check_folder(&data, &folder)?;
        let mut config = read_config(&data);
        config.folder = Some(folder);
        write_config(&data, &config).map_err(HostError::folder)?;
        clear_failure(&runtime.failure_sink());
    }
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_backup_set_retention(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    keep_transition: usize,
    keep_daily: usize,
) -> Result<BackupStatusDto, HostError> {
    if !(1..=365).contains(&keep_transition) || !(1..=365).contains(&keep_daily) {
        return Err(HostError {
            code: CODE_FOLDER.to_string(),
            message: "Keep between 1 and 365 backups of each kind.".to_string(),
            detail: None,
        });
    }
    let data = data_dir(&app).map_err(HostError::write)?;
    let mut config = read_config(&data);
    config.keep_transition = keep_transition;
    config.keep_daily = keep_daily;
    write_config(&data, &config).map_err(HostError::write)?;
    if let Some(root) = config.root() {
        backup::prune(&root, config.retention())
            .map_err(|error| map_create_error(error, &data, config.folder.as_deref()))?;
    }
    status(&app, &runtime)
}

#[tauri::command]
pub async fn host_backup_now(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<BackupStatusDto, HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    if read_config(&data).folder.is_none() {
        return Err(HostError::no_folder());
    }
    let failures = runtime.failure_sink();
    let worker = data.clone();
    let now = crate::host::now_secs();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let db = Db::open(worker.join(DB_FILE))
            .map_err(|error| HostError::write(redact(error, &worker, None)))?;
        match run_backup(&worker, &db, BackupKind::Manual, now)? {
            Some(_) => {
                clear_failure(&failures);
                Ok(())
            }
            None => Err(HostError::no_folder()),
        }
    })
    .await
    .map_err(|error| HostError::write(error.to_string()))?;
    if let Err(error) = &result {
        record_failure(&runtime.failure_sink(), now, error);
    }
    result?;
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_backup_purge(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    confirmation: String,
) -> Result<BackupStatusDto, HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let config = read_config(&data);
    let root = config.root().ok_or_else(HostError::no_folder)?;
    let count = items(&root)
        .map_err(|error| map_folder_error(error, &data, config.folder.as_deref()))?
        .len();
    backup::purge(&root, &confirmation)
        .map_err(|error| map_purge_error(error, &data, config.folder.as_deref()))?;
    if let Ok(db) = Db::open(data.join(DB_FILE))
        && let Ok(Some(event_id)) = MetaRepo::get(db.conn(), "host.current_event")
    {
        let label = config
            .folder
            .as_deref()
            .map(folder_name)
            .unwrap_or_else(|| "the backup folder".to_string());
        let _ = AuditRepo::insert(
            db.conn(),
            &event_id,
            crate::host::now_secs(),
            "Host",
            "Every backup deleted",
            Some(&label),
            &serde_json::json!({ "backups": count }).to_string(),
        );
    }
    clear_failure(&runtime.failure_sink());
    status(&app, &runtime)
}

#[tauri::command]
pub fn host_writer_takeover(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    confirmation: String,
) -> Result<BackupStatusDto, HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let config = read_config(&data);
    let me = machine_id(&data).map_err(HostError::bot_lock)?;
    let db = Db::open(data.join(DB_FILE))
        .map_err(|error| HostError::bot_lock(redact(error, &data, None)))?;
    writer::take_over(
        db.conn(),
        config.root().as_deref(),
        &me,
        crate::host::now_secs(),
        &confirmation,
    )
    .map_err(|error| map_writer_error(error, &data, config.folder.as_deref()))?;
    *runtime
        .lost_lock_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    status(&app, &runtime)
}

fn restore_root(data: &Path, runtime: &BackupRuntime) -> Result<(PathBuf, PathBuf), HostError> {
    let chosen = runtime
        .restore_source
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let folder = chosen
        .or_else(|| read_config(data).folder)
        .ok_or_else(HostError::no_folder)?;
    Ok((backup::backup_root(&folder), folder))
}

#[tauri::command]
pub async fn host_restore_choose_folder(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
) -> Result<RestoreSourceDto, HostError> {
    let folder = pick_folder(&app).await.map_err(HostError::folder)?;
    if let Some(folder) = folder {
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
) -> Result<RestoreSourceDto, HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let Ok((root, folder)) = restore_root(&data, &runtime) else {
        return Ok(RestoreSourceDto {
            folder_name: None,
            backups: Vec::new(),
        });
    };
    let backups = items(&root).map_err(|error| map_folder_error(error, &data, Some(&folder)))?;
    Ok(RestoreSourceDto {
        folder_name: Some(folder_name(&folder)),
        backups,
    })
}

fn newest_recovery_dir(data: &Path) -> Option<PathBuf> {
    let parent = data.join(PRE_RESTORE_DIR);
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    dirs.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    dirs.into_iter().next()
}

#[tauri::command]
pub fn host_backup_reveal_folder(app: AppHandle) -> Result<(), HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let folder = read_config(&data).folder.ok_or_else(HostError::no_folder)?;
    app.opener()
        .reveal_item_in_dir(&folder)
        .map_err(|error| HostError::folder(redact(error, &data, Some(&folder))))
}

#[tauri::command]
pub fn host_open_recovery_folder(app: AppHandle) -> Result<(), HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let folder = newest_recovery_dir(&data).ok_or_else(|| HostError {
        code: CODE_FOLDER.to_string(),
        message: "There is no recovery folder yet. Restore a backup first.".to_string(),
        detail: None,
    })?;
    app.opener()
        .open_path(folder.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| HostError::folder(redact(error, &data, Some(&folder))))
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
) -> Result<RestorePreviewDto, HostError> {
    let data = data_dir(&app).map_err(HostError::write)?;
    let (root, folder) = restore_root(&data, &runtime)?;
    tauri::async_runtime::spawn_blocking(move || {
        preview(&data, &root, &id).map_err(|error| map_preview_error(error, &data, Some(&folder)))
    })
    .await
    .map_err(|error| HostError::write(error.to_string()))?
}

#[tauri::command]
pub async fn host_restore(
    app: AppHandle,
    runtime: State<'_, BackupRuntime>,
    id: String,
    confirmation: String,
) -> Result<RestoreDoneDto, HostError> {
    if confirmation.trim() != backup::RESTORE_CONFIRMATION {
        return Err(HostError::confirmation(backup::RESTORE_CONFIRMATION));
    }
    let data = data_dir(&app).map_err(HostError::write)?;
    let (root, folder) = restore_root(&data, &runtime)?;
    // The restore replaces the database file: stop the only long-lived
    // connection first. The token stays in the keychain, and the bot starts
    // again only after the host reconnects and the bot lock passes.
    #[cfg(feature = "live")]
    app.state::<crate::live_runtime::LiveRuntime>()
        .stop()
        .await
        .map_err(|error| HostError::restore(error.to_string()))?;
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
        .map_err(|error| map_restore_error(error, &data, Some(&folder)))
    })
    .await
    .map_err(|error| HostError::restore(error.to_string()))??;
    *runtime
        .lost_lock_at
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
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

    fn failure_sink() -> FailureSink {
        Arc::new(Mutex::new(None))
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
        assert!(transition_hook(&data, failure_sink()).is_none());
        let db = Db::open(data.join(DB_FILE)).unwrap();
        assert!(
            run_backup(&data, &db, BackupKind::Manual, T0)
                .unwrap()
                .is_none()
        );
    }

    // scenario: The app makes a daily backup
    #[test]
    fn the_daily_tick_backs_up_once_per_24_hours() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        select_folder(&data, &dir.path().join("usb"));
        assert!(tick(&data, false, T0, &failure_sink()).unwrap().daily);
        assert!(
            !tick(&data, false, T0 + DAILY_INTERVAL_SECS - 1, &failure_sink(),)
                .unwrap()
                .daily
        );
        assert!(
            tick(&data, false, T0 + DAILY_INTERVAL_SECS, &failure_sink(),)
                .unwrap()
                .daily
        );
    }

    // scenario: A computer that lost the bot lock stops the bot
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
        let outcome = tick(&data, true, T0 + 60, &failure_sink()).unwrap();
        assert_eq!(outcome.lost_lock_at, Some(T0 + 60));
        let error = claim_writer(&data, T0 + 61).unwrap_err();
        assert!(error.contains("Take over"));
        // After the stale window the guard lets this machine start again.
        claim_writer(&data, T0 + 30 + writer::STALE_AFTER_SECS).unwrap();
    }

    fn select_file_as_folder(data: &Path, folder: &Path) {
        std::fs::write(folder, b"not a folder").unwrap();
        write_config(
            data,
            &BackupConfig {
                folder: Some(folder.to_path_buf()),
                ..BackupConfig::default()
            },
        )
        .unwrap();
    }

    // scenario: A failed backup stops the transition
    #[test]
    fn a_failed_transition_backup_is_kept_for_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        let sink = failure_sink();
        select_file_as_folder(&data, &dir.path().join("usb"));
        let hook = transition_hook(&data, sink.clone()).unwrap();
        let db = Db::open(data.join(DB_FILE)).unwrap();
        let error = hook(&db, "evt-1").unwrap_err();
        assert!(error.starts_with("MSW-BACKUP-003"), "{error}");
        let failure = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("a failure is recorded");
        assert_eq!(failure.code, CODE_WRITE);
        assert!(failure.message.contains("did not change the event state"));

        // A later successful transition clears the failure.
        let usb = dir.path().join("usb");
        std::fs::remove_file(&usb).unwrap();
        select_folder(&data, &usb);
        hook(&db, "evt-1").unwrap();
        assert!(
            sink.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_none()
        );
    }

    #[test]
    fn a_failed_daily_backup_is_kept_for_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let data = data_with_event(dir.path());
        let sink = failure_sink();
        select_file_as_folder(&data, &dir.path().join("usb"));
        let error = tick(&data, false, T0, &sink).unwrap_err();
        assert_eq!(error.code, CODE_WRITE);
        assert_eq!(
            sink.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .map(|failure| failure.code.as_str()),
            Some(CODE_WRITE)
        );
    }

    // scenario: The host chooses a backup folder
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
    fn command_errors_use_stable_codes_and_host_copy() {
        let data = Path::new("/home/host/data");
        let folder = Path::new("/media/usb");
        let confirmation = map_preview_error(
            backup::BackupError::Confirmation("RESTORE"),
            data,
            Some(folder),
        );
        assert_eq!(confirmation.code, CODE_CONFIRM);
        assert!(confirmation.message.contains("capital letters"));
        assert!(!confirmation.message.contains('/'));

        let damaged = map_preview_error(
            backup::BackupError::BlobMismatch("S7F3A.it".to_string()),
            data,
            Some(folder),
        );
        assert_eq!(damaged.code, CODE_DAMAGED);
        assert!(damaged.message.contains("One file is damaged"));
        assert!(damaged.detail.unwrap_or_default().contains("S7F3A.it"));

        let write = map_create_error(
            backup::BackupError::Io(std::io::Error::other("no space")),
            data,
            Some(folder),
        );
        assert_eq!(write.code, CODE_WRITE);
        assert!(write.message.contains("did not change the event state"));
        assert!(!write.detail.unwrap_or_default().contains("/home/host"));

        let json = serde_json::to_value(HostError::damaged("detail")).unwrap();
        assert_eq!(json["code"], CODE_DAMAGED);
        assert!(json["message"].as_str().unwrap().contains("One file"));
        assert_eq!(json["detail"], "detail");
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
    fn the_newest_recovery_folder_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let earlier = data.join(PRE_RESTORE_DIR).join("100");
        let later = data.join(PRE_RESTORE_DIR).join("200");
        std::fs::create_dir_all(&earlier).unwrap();
        std::fs::create_dir_all(&later).unwrap();
        assert_eq!(newest_recovery_dir(&data), Some(later));
    }

    // scenario: A backup never contains the bot token
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
        assert!(
            tick(&data, true, T0 + DAILY_INTERVAL_SECS, &failure_sink())
                .unwrap()
                .daily
        );

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
