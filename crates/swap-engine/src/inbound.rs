//! Inbound Discord handler for the classic delivery loop.
//!
//! The handler downloads attachments immediately, submits them through the
//! engine, and drains the durable outbox. It never stores or forwards the
//! Discord CDN URL.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use swap_db::{AssignmentRepo, Db, DmCursorRepo, EventRepo, ProcessedAttachmentRepo};
use swap_discord::{
    Action, AttachmentFetcher, DiscordChannelId, DiscordPort, DiscordUserId, DmAttachment,
    InboundEvent, InboundHandler, MessageId,
};
use swap_files::BlobStore;

use crate::clock::Clock;
use crate::delivery::DeliveryWorker;
use crate::engine::{EngineError, SubmitRequest, claim, skip, submit};
use crate::prepare::DeliveryPreparer;
use crate::settings::EventSettings;

pub struct InboundService {
    db: Arc<Mutex<Db>>,
    store: BlobStore,
    preparer: Arc<dyn DeliveryPreparer>,
    discord: Arc<dyn DiscordPort>,
    fetcher: Arc<dyn AttachmentFetcher>,
    clock: Arc<dyn Clock>,
    worker: DeliveryWorker,
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
        }
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

    pub async fn backfill(&self) -> Result<usize, String> {
        let cursors = {
            let db = self.lock();
            DmCursorRepo::list(db.conn()).map_err(|error| error.to_string())?
        };
        let mut handled = 0;
        for cursor in cursors {
            let user = DiscordUserId::new(cursor.user_id.clone());
            let after = cursor.last_seen_message_id.as_ref().map(MessageId::new);
            let messages = self
                .discord
                .fetch_dm_after(&user, after.as_ref())
                .await
                .map_err(|error| error.to_string())?;
            for message in messages {
                for attachment in &message.attachments {
                    self.attachment(
                        &cursor.user_id,
                        &cursor.channel_id,
                        &message.message_id.0,
                        attachment,
                    )
                    .await?;
                    handled += 1;
                }
                let now = self.now();
                let db = self.lock();
                DmCursorRepo::upsert(
                    db.conn(),
                    &cursor.user_id,
                    &cursor.channel_id,
                    Some(&message.message_id.0),
                    now,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        Ok(handled)
    }

    async fn handle_attachment(
        &self,
        user_id: &DiscordUserId,
        channel_id: &DiscordChannelId,
        message_id: &str,
        attachment: &DmAttachment,
    ) -> Result<(), String> {
        self.attachment(&user_id.0, &channel_id.0, message_id, attachment)
            .await
    }

    async fn attachment(
        &self,
        user_id: &str,
        channel_id: &str,
        message_id: &str,
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

        let event = {
            let db = self.lock();
            EventRepo::active_for_user(db.conn(), user_id).map_err(|error| error.to_string())?
        };
        let Some(event) = event else {
            self.notify(user_id, "There is no active swap event for you.")
                .await;
            return Ok(());
        };
        let active = {
            let db = self.lock();
            AssignmentRepo::latest_for_participant(db.conn(), &event.id, user_id)
                .map_err(|error| error.to_string())?
                .is_some_and(|assignment| {
                    matches!(
                        assignment.status,
                        swap_core::AssignmentStatus::Claimed
                            | swap_core::AssignmentStatus::Overdue
                            | swap_core::AssignmentStatus::Submitted
                    )
                })
        };
        if !active {
            self.notify(
                user_id,
                "You have no active track. Claim an offer before you send a file.",
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
        let now = self.now();
        let outcome = tokio::task::spawn_blocking(move || {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            submit(
                &db,
                &store,
                preparer.as_ref(),
                &SubmitRequest {
                    event_id: &event_id,
                    user_id: &user,
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
                self.worker
                    .drain(self.discord.as_ref(), self.now(), 100)
                    .await
                    .map_err(|error| error.to_string())?;
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
            Ok(_) => {
                self.worker
                    .drain(self.discord.as_ref(), self.now(), 100)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
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
            Ok(_) => {
                self.worker
                    .drain(self.discord.as_ref(), self.now(), 100)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
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

#[async_trait]
impl InboundHandler for InboundService {
    async fn handle(&self, event: InboundEvent) -> Result<(), String> {
        match event {
            InboundEvent::DmAttachment {
                user_id,
                channel_id,
                message_id,
                attachment,
            } => {
                self.handle_attachment(&user_id, &channel_id, &message_id.0, &attachment)
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
                user_id, command, ..
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
