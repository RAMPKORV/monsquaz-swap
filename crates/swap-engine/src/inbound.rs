//! Inbound Discord handler for the classic delivery loop.
//!
//! The handler downloads attachments immediately and submits them through the
//! engine. It wakes the outbox dispatcher after each event. It never stores or
//! forwards the Discord CDN URL.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Notify, mpsc, watch};

use async_trait::async_trait;
use swap_db::{AssignmentRepo, AuditRepo, Db, DmCursorRepo, EventRepo, ProcessedAttachmentRepo};
use swap_discord::{
    Action, AttachmentFetcher, DiscordChannelId, DiscordError, DiscordPort, DiscordUserId,
    DmAttachment, InboundEvent, InboundHandler, MessageId,
};
use swap_files::BlobStore;

use crate::clock::Clock;
use crate::delivery::{DeliveryReport, DeliveryWorker};
use crate::engine::{
    EngineError, JoinRequest, SubmissionSource, SubmitRequest, claim, join_with_seed, signup, skip,
    submit_attachment,
};
use crate::prepare::DeliveryPreparer;
use crate::settings::EventSettings;

type InboundQueue = (
    mpsc::UnboundedSender<InboundEvent>,
    tokio::task::JoinHandle<()>,
);

#[derive(Clone)]
pub struct InboundService {
    db: Arc<Mutex<Db>>,
    store: BlobStore,
    preparer: Arc<dyn DeliveryPreparer>,
    discord: Arc<dyn DiscordPort>,
    fetcher: Arc<dyn AttachmentFetcher>,
    clock: Arc<dyn Clock>,
    worker: DeliveryWorker,
    wake: Arc<Notify>,
    queues: Arc<Mutex<HashMap<String, InboundQueue>>>,
    queue_stop: watch::Sender<bool>,
    stopped: Arc<AtomicBool>,
    backfill_pending: Arc<Mutex<HashSet<String>>>,
    backfill_notify: Arc<Notify>,
    backfill_warned: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackfillReport {
    pub handled: usize,
    pub failed: usize,
}

impl BackfillReport {
    pub fn complete(&self) -> bool {
        self.failed == 0
    }
}

impl InboundService {
    pub fn new(
        db: Arc<Mutex<Db>>,
        store: BlobStore,
        preparer: Arc<dyn DeliveryPreparer>,
        discord: Arc<dyn DiscordPort>,
        fetcher: Arc<dyn AttachmentFetcher>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let worker = DeliveryWorker::new(db.clone(), store.clone());
        Self {
            db,
            store,
            preparer,
            discord,
            fetcher,
            clock,
            worker,
            wake: Arc::new(Notify::new()),
            queues: Arc::new(Mutex::new(HashMap::new())),
            queue_stop: watch::channel(false).0,
            stopped: Arc::new(AtomicBool::new(false)),
            backfill_pending: Arc::new(Mutex::new(HashSet::new())),
            backfill_notify: Arc::new(Notify::new()),
            backfill_warned: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Gate live events for every participant with a saved DM cursor until the
    /// startup backfill for that participant succeeds.
    pub fn mark_backfill_pending(&self) {
        let cursors = {
            let db = self.lock();
            DmCursorRepo::list(db.conn())
        };
        match cursors {
            Ok(cursors) => {
                let mut pending = self
                    .backfill_pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                for cursor in cursors {
                    pending.insert(cursor.user_id);
                }
            }
            Err(error) => eprintln!("DM backfill cursor list failed: {error}"),
        }
    }

    fn finish_backfill_user(&self, user_id: &str) {
        let removed = self
            .backfill_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(user_id);
        if removed {
            self.backfill_notify.notify_waiters();
        }
    }

    async fn wait_for_backfill(&self, user_id: &str) {
        loop {
            let pending = self
                .backfill_pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .contains(user_id);
            if !pending {
                return;
            }
            let notified = self.backfill_notify.notified();
            let still_pending = self
                .backfill_pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .contains(user_id);
            if !still_pending {
                return;
            }
            notified.await;
        }
    }

    fn queue_sender(&self, user_id: &str) -> mpsc::UnboundedSender<InboundEvent> {
        let mut queues = self
            .queues
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((sender, _)) = queues.get(user_id) {
            return sender.clone();
        }
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let service = self.clone();
        let mut stop = self.queue_stop.subscribe();
        let task = tokio::spawn(async move {
            loop {
                if *stop.borrow() {
                    break;
                }
                tokio::select! {
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            break;
                        }
                    }
                    event = receiver.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        if let Err(error) = service.process(event).await {
                            eprintln!("inbound event failed: {error}");
                        }
                    }
                }
            }
        });
        queues.insert(user_id.to_string(), (sender.clone(), task));
        sender
    }

    /// Stop every per-participant queue. In-flight work finishes; queued work is
    /// dropped. The service refuses new events after this call.
    pub async fn stop_queues(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.queue_stop.send(true);
        let tasks = {
            let mut queues = self
                .queues
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            queues
                .drain()
                .map(|(_, (_, task))| task)
                .collect::<Vec<_>>()
        };
        for task in tasks {
            let _ = task.await;
        }
    }

    pub fn wake_notifier(&self) -> Arc<Notify> {
        self.wake.clone()
    }

    pub fn with_alert_channel(mut self, channel_id: Option<String>) -> Self {
        if let Some(channel_id) = channel_id.filter(|value| !value.trim().is_empty()) {
            self.worker = self.worker.with_alert_channel(channel_id);
        }
        self
    }

    pub fn with_fallback_channel(mut self, channel_id: Option<String>) -> Self {
        if let Some(channel_id) = channel_id.filter(|value| !value.trim().is_empty()) {
            self.worker = self.worker.with_fallback_channel(channel_id);
        }
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Db> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn now(&self) -> i64 {
        self.clock.now()
    }

    pub async fn drain_pending(&self) -> Result<DeliveryReport, String> {
        self.worker
            .drain(self.discord.as_ref(), self.now(), 100)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn backfill(&self) -> Result<BackfillReport, String> {
        self.backfill_users(None).await
    }

    /// Retry only the participants whose startup backfill has not succeeded.
    pub async fn retry_backfill(&self) -> Result<BackfillReport, String> {
        let pending = self
            .backfill_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if pending.is_empty() {
            return Ok(BackfillReport::default());
        }
        self.backfill_users(Some(&pending)).await
    }

    async fn backfill_users(
        &self,
        only: Option<&HashSet<String>>,
    ) -> Result<BackfillReport, String> {
        let cursors = {
            let db = self.lock();
            DmCursorRepo::list(db.conn()).map_err(|error| error.to_string())?
        };
        let mut report = BackfillReport::default();
        let mut failure_event: Option<String> = None;
        for cursor in cursors {
            if only.is_some_and(|only| !only.contains(&cursor.user_id)) {
                continue;
            }
            let user = DiscordUserId::new(cursor.user_id.clone());
            let after = cursor.last_seen_message_id.as_ref().map(MessageId::new);
            let messages = match self.discord.fetch_dm_after(&user, after.as_ref()).await {
                Ok(messages) => messages,
                Err(error) => {
                    eprintln!("DM backfill failed for one participant: {error}");
                    report.failed += 1;
                    if failure_event.is_none() {
                        let db = self.lock();
                        failure_event = EventRepo::active_for_user(db.conn(), &cursor.user_id)
                            .ok()
                            .flatten()
                            .map(|event| event.id);
                    }
                    if permanent_backfill_error(&error) {
                        self.finish_backfill_user(&cursor.user_id);
                    }
                    continue;
                }
            };
            let mut user_failed = false;
            for message in messages {
                let mut complete = true;
                for attachment in &message.attachments {
                    match self
                        .attachment(
                            &cursor.user_id,
                            &cursor.channel_id,
                            &message.message_id.0,
                            None,
                            attachment,
                        )
                        .await
                    {
                        Ok(()) => report.handled += 1,
                        Err(error) => {
                            eprintln!("DM backfill attachment failed: {error}");
                            complete = false;
                        }
                    }
                }
                if !complete {
                    user_failed = true;
                    break;
                }
                let now = self.now();
                let db = self.lock();
                if let Err(error) = DmCursorRepo::upsert(
                    db.conn(),
                    &cursor.user_id,
                    &cursor.channel_id,
                    Some(&message.message_id.0),
                    now,
                ) {
                    eprintln!("DM backfill cursor update failed: {error}");
                    user_failed = true;
                    break;
                }
            }
            if user_failed {
                report.failed += 1;
            } else {
                self.finish_backfill_user(&cursor.user_id);
            }
            if user_failed && failure_event.is_none() {
                let db = self.lock();
                failure_event = EventRepo::active_for_user(db.conn(), &cursor.user_id)
                    .ok()
                    .flatten()
                    .map(|event| event.id);
            }
        }
        if report.failed > 0 {
            if !self.backfill_warned.swap(true, Ordering::SeqCst) {
                self.worker
                    .notify_host(
                        self.discord.as_ref(),
                        &format!(
                            "DM backfill failed for {} participant(s). The app will retry.",
                            report.failed
                        ),
                    )
                    .await;
                if let Some(event_id) = failure_event {
                    let db = self.lock();
                    if let Err(error) = AuditRepo::insert(
                        db.conn(),
                        &event_id,
                        self.now(),
                        "worker",
                        "backfill.failed",
                        None,
                        &format!("{{\"failed\":{}}}", report.failed),
                    ) {
                        eprintln!("backfill audit entry failed: {error}");
                    }
                }
            }
        } else {
            self.backfill_warned.store(false, Ordering::SeqCst);
        }
        self.wake.notify_one();
        Ok(report)
    }

    async fn join_seed(
        &self,
        event: &swap_db::EventRecord,
        user_id: &str,
        channel_id: &str,
        message_id: &str,
        author_name: Option<&str>,
        attachment: &DmAttachment,
    ) -> Result<(), String> {
        let settings: EventSettings =
            serde_json::from_str(&event.settings_json).map_err(|error| error.to_string())?;
        if attachment.size > settings.max_file_bytes {
            self.notify(
                user_id,
                "That seed is larger than this event allows. Send a smaller file.",
            )
            .await;
            self.mark_processed(user_id, message_id, &attachment.id)?;
            self.update_cursor(user_id, channel_id, message_id)?;
            return Ok(());
        }
        let bytes = match self
            .fetcher
            .fetch(&attachment.url, settings.max_file_bytes)
            .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.notify(
                    user_id,
                    "The seed download failed. Please send the file again.",
                )
                .await;
                return Err(error.to_string());
            }
        };
        let participant = {
            let db = self.lock();
            swap_db::ParticipantRepo::get(db.conn(), &event.id, user_id)
                .map_err(|error| error.to_string())?
        };
        let display_name = author_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                participant
                    .as_ref()
                    .map(|participant| participant.display_name.clone())
                    .unwrap_or_else(|| user_id.to_string())
            });
        let timezone = participant
            .as_ref()
            .map(|participant| participant.timezone.clone())
            .unwrap_or_else(|| "UTC".to_string());
        let max_handoffs = participant
            .as_ref()
            .map(|participant| participant.max_handoffs)
            .unwrap_or(4);
        let db = self.db.clone();
        let store = self.store.clone();
        let preparer = self.preparer.clone();
        let event_id = event.id.clone();
        let user = user_id.to_string();
        let filename = attachment.filename.clone();
        let now = self.now();
        let outcome = tokio::task::spawn_blocking(move || {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            join_with_seed(
                &db,
                &store,
                preparer.as_ref(),
                &JoinRequest {
                    event_id: &event_id,
                    user_id: &user,
                    display_name: &display_name,
                    timezone: &timezone,
                    max_handoffs,
                    filename: &filename,
                    bytes: &bytes,
                },
                now,
            )
        })
        .await
        .map_err(|error| error.to_string())?;
        match outcome {
            Ok(_) => {
                self.mark_processed(user_id, message_id, &attachment.id)?;
                self.update_cursor(user_id, channel_id, message_id)?;
                Ok(())
            }
            Err(error) if terminal_join(&error) => {
                self.notify(user_id, &join_message(&error)).await;
                self.mark_processed(user_id, message_id, &attachment.id)?;
                self.update_cursor(user_id, channel_id, message_id)?;
                Ok(())
            }
            Err(error) => {
                self.notify(
                    user_id,
                    "The seed was not stored. Please send the file again or ask the host.",
                )
                .await;
                Err(error.to_string())
            }
        }
    }

    async fn handle_attachment(
        &self,
        user_id: &DiscordUserId,
        channel_id: &DiscordChannelId,
        message_id: &str,
        author_name: Option<&str>,
        attachment: &DmAttachment,
    ) -> Result<(), String> {
        self.attachment(
            &user_id.0,
            &channel_id.0,
            message_id,
            author_name,
            attachment,
        )
        .await
    }

    async fn attachment(
        &self,
        user_id: &str,
        channel_id: &str,
        message_id: &str,
        author_name: Option<&str>,
        attachment: &DmAttachment,
    ) -> Result<(), String> {
        {
            let db = self.lock();
            if ProcessedAttachmentRepo::is_processed(db.conn(), user_id, message_id, &attachment.id)
                .map_err(|error| error.to_string())?
            {
                return Ok(());
            }
        }

        let signup = {
            let db = self.lock();
            EventRepo::signup_event(db.conn()).map_err(|error| error.to_string())?
        };
        if let Some(event) = signup {
            let participant = {
                let db = self.lock();
                swap_db::ParticipantRepo::get(db.conn(), &event.id, user_id)
                    .map_err(|error| error.to_string())?
            };
            match participant
                .as_ref()
                .map(|participant| participant.membership)
            {
                None | Some(swap_core::Membership::SignedUp) => {
                    return self
                        .join_seed(
                            &event,
                            user_id,
                            channel_id,
                            message_id,
                            author_name,
                            attachment,
                        )
                        .await;
                }
                Some(swap_core::Membership::Seeded | swap_core::Membership::Active) => {
                    self.notify(
                        user_id,
                        "You already sent a seed. Wait for the host to start the event.",
                    )
                    .await;
                    self.mark_processed(user_id, message_id, &attachment.id)?;
                    self.update_cursor(user_id, channel_id, message_id)?;
                    return Ok(());
                }
                Some(swap_core::Membership::Left | swap_core::Membership::Removed) => {
                    self.notify(
                        user_id,
                        "You left this event. Ask the host to return as a fill-in.",
                    )
                    .await;
                    self.mark_processed(user_id, message_id, &attachment.id)?;
                    self.update_cursor(user_id, channel_id, message_id)?;
                    return Ok(());
                }
            }
        }

        let event = {
            let db = self.lock();
            EventRepo::active_for_user(db.conn(), user_id).map_err(|error| error.to_string())?
        };
        let Some(event) = event else {
            self.notify(user_id, "There is no active swap event for you.")
                .await;
            return Ok(());
        };
        let assignment = {
            let db = self.lock();
            AssignmentRepo::latest_for_participant(db.conn(), &event.id, user_id)
                .map_err(|error| error.to_string())?
        };
        let Some(assignment) = assignment.filter(|assignment| {
            matches!(
                assignment.status,
                swap_core::AssignmentStatus::Claimed
                    | swap_core::AssignmentStatus::Overdue
                    | swap_core::AssignmentStatus::Submitted
            )
        }) else {
            self.notify(
                user_id,
                "You have no active track. Claim an offer before you send a file.",
            )
            .await;
            self.mark_processed(user_id, message_id, &attachment.id)?;
            return Ok(());
        };
        let cutoff = assignment.claimed_at.unwrap_or(assignment.offered_at);
        if !message_is_current(message_id, cutoff) {
            self.notify(
                user_id,
                "That file is older than your current track. Send the file again if it is your current edit.",
            )
            .await;
            self.mark_processed(user_id, message_id, &attachment.id)?;
            return Ok(());
        }
        let settings: EventSettings =
            serde_json::from_str(&event.settings_json).map_err(|error| error.to_string())?;
        if attachment.size > settings.max_file_bytes {
            self.notify(
                user_id,
                "That file is larger than this event allows. Send a smaller file.",
            )
            .await;
            self.mark_processed(user_id, message_id, &attachment.id)?;
            return Ok(());
        }

        let bytes = match self
            .fetcher
            .fetch(&attachment.url, settings.max_file_bytes)
            .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                self.notify(
                    user_id,
                    "The file download failed. Please send the file again.",
                )
                .await;
                return Err(error.to_string());
            }
        };

        let db = self.db.clone();
        let store = self.store.clone();
        let preparer = self.preparer.clone();
        let event_id = event.id.clone();
        let user = user_id.to_string();
        let filename = attachment.filename.clone();
        let source_message = message_id.to_string();
        let source_attachment = attachment.id.clone();
        let now = self.now();
        let outcome = tokio::task::spawn_blocking(move || {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            submit_attachment(
                &db,
                &store,
                preparer.as_ref(),
                &SubmitRequest {
                    event_id: &event_id,
                    user_id: &user,
                    filename: &filename,
                    bytes: &bytes,
                },
                SubmissionSource {
                    message_id: &source_message,
                    attachment_id: &source_attachment,
                },
                now,
            )
        })
        .await
        .map_err(|error| error.to_string())?;

        match outcome {
            Ok(_) | Err(EngineError::AttachmentAlreadyProcessed) => {
                self.update_cursor(user_id, channel_id, message_id)?;
                Ok(())
            }
            Err(error) if terminal_submit(&error) => {
                self.notify(user_id, &submit_message(&error)).await;
                self.mark_processed(user_id, message_id, &attachment.id)?;
                self.update_cursor(user_id, channel_id, message_id)?;
                Ok(())
            }
            Err(error) => {
                self.notify(
                    user_id,
                    "The file was not stored. Please try again or ask the host.",
                )
                .await;
                Err(error.to_string())
            }
        }
    }

    async fn handle_join(
        &self,
        user_id: &str,
        options: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        let event = {
            let db = self.lock();
            EventRepo::signup_event(db.conn()).map_err(|error| error.to_string())?
        };
        let Some(event) = event else {
            self.notify(user_id, "There is no signup event open now.")
                .await;
            return Ok(());
        };
        let participant = {
            let db = self.lock();
            swap_db::ParticipantRepo::get(db.conn(), &event.id, user_id)
                .map_err(|error| error.to_string())?
        };
        match participant
            .as_ref()
            .map(|participant| participant.membership)
        {
            Some(swap_core::Membership::Seeded | swap_core::Membership::Active) => {
                self.notify(
                    user_id,
                    "You already sent a seed. Wait for the host to start.",
                )
                .await;
                return Ok(());
            }
            Some(swap_core::Membership::Left | swap_core::Membership::Removed) => {
                self.notify(
                    user_id,
                    "You left this event. Ask the host to return as a fill-in.",
                )
                .await;
                return Ok(());
            }
            Some(swap_core::Membership::SignedUp) | None => {}
        }
        let has_options = options.contains_key("timezone") || options.contains_key("max_handoffs");
        if participant.is_none() || has_options {
            let timezone = options
                .get("timezone")
                .map(String::as_str)
                .filter(|value| {
                    !value.trim().is_empty()
                        && value.len() <= 64
                        && value.contains('/')
                        && !value.contains(' ')
                })
                .or_else(|| {
                    participant
                        .as_ref()
                        .map(|participant| participant.timezone.as_str())
                })
                .unwrap_or("UTC")
                .to_string();
            let max_handoffs = options
                .get("max_handoffs")
                .and_then(|value| value.parse::<u32>().ok())
                .or_else(|| {
                    participant
                        .as_ref()
                        .map(|participant| participant.max_handoffs)
                })
                .unwrap_or(4)
                .clamp(1, 20);
            let db = self.lock();
            signup(
                &db,
                &event.id,
                user_id,
                user_id,
                &timezone,
                max_handoffs,
                self.now(),
            )
            .map_err(|error| error.to_string())?;
        }
        let settings: EventSettings =
            serde_json::from_str(&event.settings_json).unwrap_or_default();
        let formats = settings
            .allowed_extensions
            .iter()
            .map(|extension| format!(".{extension}"))
            .collect::<Vec<_>>()
            .join(", ");
        let max_mib = settings.max_file_bytes / (1024 * 1024);
        self.notify(
            user_id,
            &format!(
                "You are signed up. Send one seed file ({formats}) no larger than {max_mib} MiB to this direct message. The bot stores the file locally."
            ),
        )
        .await;
        Ok(())
    }

    async fn handle_claim(
        &self,
        event_id: &str,
        user_id: &str,
        assignment_id: Option<&str>,
    ) -> Result<(), String> {
        let assignment_id = match assignment_id.filter(|value| !value.is_empty()) {
            Some(value) => value.to_string(),
            None => {
                let assignment = {
                    let db = self.lock();
                    AssignmentRepo::latest_for_participant(db.conn(), event_id, user_id)
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "no active assignment".to_string())?
                };
                if assignment.status != swap_core::AssignmentStatus::Offered {
                    self.notify(user_id, "There is no open offer to claim.")
                        .await;
                    return Ok(());
                }
                assignment.id
            }
        };
        let db = self.db.clone();
        let store = self.store.clone();
        let event = event_id.to_string();
        let user = user_id.to_string();
        let now = self.now();
        let outcome = tokio::task::spawn_blocking(move || {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            claim(&db, &store, &event, &assignment_id, &user, now)
        })
        .await
        .map_err(|error| error.to_string())?;
        match outcome {
            Ok(_) => Ok(()),
            Err(error) if terminal_claim(&error) => {
                self.notify(user_id, &claim_message(&error)).await;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    async fn handle_skip(&self, event_id: &str, user_id: &str) -> Result<(), String> {
        let db = self.db.clone();
        let event = event_id.to_string();
        let user = user_id.to_string();
        let now = self.now();
        let outcome = tokio::task::spawn_blocking(move || {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            skip(&db, &event, &user, now)
        })
        .await
        .map_err(|error| error.to_string())?;
        match outcome {
            Ok(_) => Ok(()),
            Err(error) => {
                self.notify(user_id, &format!("The skip failed: {error}"))
                    .await;
                Ok(())
            }
        }
    }

    async fn active_event(&self, user_id: &str) -> Result<Option<String>, String> {
        let db = self.lock();
        Ok(EventRepo::active_for_user(db.conn(), user_id)
            .map_err(|error| error.to_string())?
            .map(|event| event.id))
    }

    async fn notify(&self, user_id: &str, content: &str) {
        let _ = self
            .discord
            .send_dm(
                &DiscordUserId::new(user_id),
                swap_discord::OutboundMessage {
                    content: content.to_string(),
                },
            )
            .await;
    }

    fn mark_processed(
        &self,
        user_id: &str,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<(), String> {
        let db = self.lock();
        ProcessedAttachmentRepo::mark(db.conn(), user_id, message_id, attachment_id, self.now())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn update_cursor(
        &self,
        user_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), String> {
        let db = self.lock();
        DmCursorRepo::upsert(db.conn(), user_id, channel_id, Some(message_id), self.now())
            .map_err(|error| error.to_string())
    }
}

impl InboundService {
    pub async fn process(&self, event: InboundEvent) -> Result<(), String> {
        self.wait_for_backfill(&event.user_id().0).await;
        let result = self.process_inner(event).await;
        self.wake.notify_one();
        result
    }

    async fn process_inner(&self, event: InboundEvent) -> Result<(), String> {
        match event {
            InboundEvent::DmAttachment {
                user_id,
                channel_id,
                message_id,
                author_name,
                attachment,
            } => {
                self.handle_attachment(
                    &user_id,
                    &channel_id,
                    &message_id.0,
                    author_name.as_deref(),
                    &attachment,
                )
                .await
            }
            InboundEvent::Button {
                user_id, custom_id, ..
            } => match custom_id.action() {
                Action::Claim => {
                    self.handle_claim(custom_id.event_id(), &user_id.0, custom_id.token())
                        .await
                }
                Action::Skip => self.handle_skip(custom_id.event_id(), &user_id.0).await,
                _ => Ok(()),
            },
            InboundEvent::SlashCommand {
                user_id,
                command,
                options,
                ..
            } => match command {
                swap_discord::SlashCommand::Claim => {
                    let Some(event_id) = self.active_event(&user_id.0).await? else {
                        self.notify(&user_id.0, "There is no active swap event for you.")
                            .await;
                        return Ok(());
                    };
                    self.handle_claim(&event_id, &user_id.0, None).await
                }
                swap_discord::SlashCommand::Skip => {
                    let Some(event_id) = self.active_event(&user_id.0).await? else {
                        self.notify(&user_id.0, "There is no active swap event for you.")
                            .await;
                        return Ok(());
                    };
                    self.handle_skip(&event_id, &user_id.0).await
                }
                swap_discord::SlashCommand::Join => self.handle_join(&user_id.0, &options).await,
                swap_discord::SlashCommand::Status => {
                    self.notify(&user_id.0, "Use the offer in your direct messages.")
                        .await;
                    Ok(())
                }
                swap_discord::SlashCommand::Help => {
                    self.notify(
                        &user_id.0,
                        "Send a file to submit your claimed track. Use /swap claim or /swap skip.",
                    )
                    .await;
                    Ok(())
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

#[async_trait]
impl InboundHandler for InboundService {
    async fn handle(&self, event: InboundEvent) -> Result<(), String> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err("the inbound service is stopped".to_string());
        }
        let user_id = event.user_id().0.clone();
        self.queue_sender(&user_id)
            .send(event)
            .map_err(|_| "the inbound queue is closed".to_string())
    }
}

const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;
/// The host clock and the Discord clock can differ. Allow a short window so a
/// real submission right after a claim is not treated as old.
const CLOCK_SKEW_SECS: i64 = 600;

/// Discord message ids carry a millisecond timestamp. A backfill can return an
/// attachment from before the current claim. Accept it only when the id is
/// numeric and not more than the skew window older than the claim.
fn message_is_current(message_id: &str, cutoff_secs: i64) -> bool {
    let Ok(id) = message_id.parse::<u64>() else {
        return true;
    };
    let millis = (id >> 22) as i64 + DISCORD_EPOCH_MS;
    millis / 1000 + CLOCK_SKEW_SECS >= cutoff_secs
}

fn terminal_join(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::AlreadySeeded
            | EngineError::ParticipantLeft
            | EngineError::WrongEventState { .. }
            | EngineError::File(_)
            | EngineError::Prepare(_)
            | EngineError::Settings(_)
    )
}

fn join_message(error: &EngineError) -> String {
    match error {
        EngineError::AlreadySeeded => "You already sent a seed.".to_string(),
        EngineError::ParticipantLeft => "You left this event. Ask the host to return.".to_string(),
        EngineError::WrongEventState { .. } => {
            "Seed intake is closed. Ask the host about the fill-in path.".to_string()
        }
        EngineError::File(_) | EngineError::Prepare(_) => {
            "That seed file is not allowed. Check the format and size.".to_string()
        }
        _ => "The seed was rejected. Ask the host for help.".to_string(),
    }
}

fn permanent_backfill_error(error: &DiscordError) -> bool {
    matches!(
        error,
        DiscordError::DmClosed
            | DiscordError::NoMutualGuild
            | DiscordError::UnknownUser
            | DiscordError::MissingAccess
    )
}

fn terminal_submit(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::NoActiveAssignment
            | EngineError::FormatChange { .. }
            | EngineError::File(_)
            | EngineError::Prepare(_)
            | EngineError::OfferExpired
            | EngineError::WrongAssignmentState(_)
    )
}

fn terminal_claim(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::OfferExpired
            | EngineError::WrongAssignmentState(_)
            | EngineError::NotAssignmentOwner
            | EngineError::AssignmentNotFound(_)
            | EngineError::NoActiveAssignment
    )
}

fn submit_message(error: &EngineError) -> String {
    match error {
        EngineError::NoActiveAssignment => {
            "You have no active track. Claim an offer first.".to_string()
        }
        EngineError::FormatChange { current, submitted } => {
            format!("Keep the same file format. This track uses .{current}, not .{submitted}.")
        }
        EngineError::OfferExpired => "The offer expired. Wait for the next offer.".to_string(),
        EngineError::WrongAssignmentState(state) => {
            format!("That track is in state {state:?} and cannot accept a file now.")
        }
        _ => "The file was rejected. Check the file and try again.".to_string(),
    }
}

fn claim_message(error: &EngineError) -> String {
    match error {
        EngineError::OfferExpired => "The offer expired.".to_string(),
        EngineError::NotAssignmentOwner | EngineError::AssignmentNotFound(_) => {
            "That offer does not belong to you.".to_string()
        }
        EngineError::WrongAssignmentState(_) => "That offer is no longer open.".to_string(),
        _ => "The claim failed. Try again or ask the host.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snowflake(seconds: i64) -> String {
        ((((seconds * 1000) - DISCORD_EPOCH_MS) as u64) << 22).to_string()
    }

    #[test]
    fn a_message_from_before_the_claim_is_not_current() {
        let claimed_at = 1_700_000_000;
        assert!(!message_is_current(
            &snowflake(claimed_at - CLOCK_SKEW_SECS - 1),
            claimed_at
        ));
        assert!(message_is_current(
            &snowflake(claimed_at - CLOCK_SKEW_SECS),
            claimed_at
        ));
        assert!(message_is_current(&snowflake(claimed_at), claimed_at));
        assert!(message_is_current(&snowflake(claimed_at + 100), claimed_at));
    }

    #[test]
    fn a_non_snowflake_test_id_is_current() {
        assert!(message_is_current("m-backfill", 2_000));
    }
}
