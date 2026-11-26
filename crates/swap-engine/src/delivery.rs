//! Durable delivery outbox and dispatcher.
//!
//! The engine enqueues a delivery in the same transaction as the state change.
//! The caller dispatches pending rows after the command and again on startup.
//! The outbox id is a deterministic dedupe key, so a retry never creates a
//! second row.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use swap_db::{Connection, Db, DbError, DeliveryRepo};
use swap_discord::{
    DiscordChannelId, DiscordError, DiscordPort, DiscordUserId, OutboundFile, OutboundMessage,
};
use swap_files::{BlobStore, StoreError, sha256_hex};
use thiserror::Error;

use crate::effects::Effect;

const MAX_DELIVERY_ATTEMPTS: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingDelivery {
    Dm {
        user_id: String,
        content: String,
    },
    Offer {
        user_id: String,
        event_id: String,
        assignment_id: String,
        track_code: String,
        due_at: i64,
        claim_window_secs: i64,
    },
    File {
        user_id: String,
        track_code: String,
        filename: String,
        sha256: String,
        caption: String,
    },
    Channel {
        channel_id: String,
        content: String,
    },
}

impl PendingDelivery {
    pub fn from_effect(effect: &Effect) -> Self {
        match effect {
            Effect::Dm { user_id, content } => Self::Dm {
                user_id: user_id.clone(),
                content: content.clone(),
            },
            Effect::Offer {
                user_id,
                event_id,
                assignment_id,
                track_code,
                due_at,
                claim_window_secs,
            } => Self::Offer {
                user_id: user_id.clone(),
                event_id: event_id.clone(),
                assignment_id: assignment_id.clone(),
                track_code: track_code.clone(),
                due_at: *due_at,
                claim_window_secs: *claim_window_secs,
            },
            Effect::TrackFile {
                user_id,
                track_code,
                filename,
                bytes,
                caption,
            } => Self::File {
                user_id: user_id.clone(),
                track_code: track_code.clone(),
                filename: filename.clone(),
                sha256: sha256_hex(bytes),
                caption: caption.clone(),
            },
            Effect::Channel {
                channel_id,
                content,
            } => Self::Channel {
                channel_id: channel_id.clone(),
                content: content.clone(),
            },
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Dm { .. } => "dm",
            Self::Offer { .. } => "offer",
            Self::File { .. } => "file",
            Self::Channel { .. } => "channel",
        }
    }

    pub fn dedupe_key(&self) -> String {
        match self {
            Self::Dm { user_id, content } => {
                format!("dm:{user_id}:{}", sha256_hex(content.as_bytes()))
            }
            Self::Offer {
                event_id,
                assignment_id,
                ..
            } => format!("offer:{event_id}:{assignment_id}"),
            Self::File {
                user_id,
                track_code,
                sha256,
                ..
            } => format!("file:{track_code}:{user_id}:{sha256}"),
            Self::Channel {
                channel_id,
                content,
            } => format!("channel:{channel_id}:{}", sha256_hex(content.as_bytes())),
        }
    }

    fn encode(&self) -> Result<Vec<u8>, DeliveryError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|error| DeliveryError::Codec(error.to_string()))
    }

    fn decode(bytes: &[u8]) -> Result<Self, DeliveryError> {
        let (value, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map_err(|error| DeliveryError::Codec(error.to_string()))?;
        Ok(value)
    }
}

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("delivery payload is not valid: {0}")]
    Codec(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    pub sent: usize,
    pub retried: usize,
    pub failed: usize,
}

pub(crate) fn enqueue_effect(
    conn: &Connection,
    event_id: &str,
    effect: &Effect,
    now: i64,
) -> Result<bool, DeliveryError> {
    enqueue_pending(conn, event_id, &PendingDelivery::from_effect(effect), now)
}

pub(crate) fn enqueue_pending(
    conn: &Connection,
    event_id: &str,
    delivery: &PendingDelivery,
    now: i64,
) -> Result<bool, DeliveryError> {
    let payload = delivery.encode()?;
    let id = format!("{event_id}:{}", delivery.dedupe_key());
    Ok(DeliveryRepo::enqueue(
        conn,
        &id,
        event_id,
        delivery.kind(),
        &payload,
        now,
    )?)
}

/// Production dispatcher. The database sits behind a mutex, so the future is
/// Send and the lock is never held across a Discord call.
pub struct DeliveryWorker {
    db: Arc<Mutex<Db>>,
    store: BlobStore,
    alert_channel: Option<String>,
    fallback_channel: Option<String>,
}

impl DeliveryWorker {
    pub fn new(db: Arc<Mutex<Db>>, store: BlobStore) -> Self {
        Self {
            db,
            store,
            alert_channel: None,
            fallback_channel: None,
        }
    }

    pub fn with_alert_channel(mut self, channel_id: impl Into<String>) -> Self {
        self.alert_channel = Some(channel_id.into());
        self
    }

    pub fn with_fallback_channel(mut self, channel_id: impl Into<String>) -> Self {
        self.fallback_channel = Some(channel_id.into());
        self
    }

    async fn fallback(
        &self,
        discord: &dyn DiscordPort,
        delivery: &PendingDelivery,
        error: &DiscordError,
    ) -> Result<bool, DeliveryError> {
        if !matches!(error, DiscordError::DmClosed) {
            return Ok(false);
        }
        let Some(channel) = &self.fallback_channel else {
            return Ok(false);
        };
        let Some(user_id) = user_of(delivery) else {
            return Ok(false);
        };
        let thread = discord
            .create_private_thread(
                &DiscordChannelId::new(channel.clone()),
                "Monsquaz Swap delivery",
            )
            .await
            .map_err(|error| DeliveryError::Codec(error.to_string()))?;
        discord
            .add_thread_member(&thread, &DiscordUserId::new(user_id.to_string()))
            .await
            .map_err(|error| DeliveryError::Codec(error.to_string()))?;
        let content = match delivery {
            PendingDelivery::Dm { content, .. } => content.clone(),
            PendingDelivery::Offer {
                track_code,
                due_at,
                claim_window_secs,
                ..
            } => {
                let hours = (*claim_window_secs as f64) / 3600.0;
                format!(
                    "Track {track_code} is available. Claim within {hours} hours (due {due_at})."
                )
            }
            PendingDelivery::File {
                filename,
                sha256,
                caption,
                ..
            } => {
                let bytes = self.store.read(sha256)?;
                discord
                    .send_file_to_channel(
                        &thread,
                        OutboundFile {
                            filename: filename.clone(),
                            bytes,
                            caption: caption.clone(),
                        },
                    )
                    .await
                    .map_err(|error| DeliveryError::Codec(error.to_string()))?;
                return Ok(true);
            }
            PendingDelivery::Channel { .. } => return Ok(false),
        };
        discord
            .send_channel_message(&thread, OutboundMessage { content })
            .await
            .map_err(|error| DeliveryError::Codec(error.to_string()))?;
        Ok(true)
    }

    pub async fn drain(
        &self,
        discord: &dyn DiscordPort,
        now: i64,
        limit: usize,
    ) -> Result<DeliveryReport, DeliveryError> {
        let mut report = DeliveryReport::default();
        for _ in 0..limit {
            let row = {
                let db = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                DeliveryRepo::pending(db.conn(), 1)?.pop()
            };
            let Some(row) = row else {
                break;
            };
            if row.attempts >= MAX_DELIVERY_ATTEMPTS {
                let db = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                DeliveryRepo::mark_failed(db.conn(), &row.id, now, "retry limit reached")?;
                report.failed += 1;
                continue;
            }
            let delivery = match PendingDelivery::decode(&row.payload) {
                Ok(delivery) => delivery,
                Err(error) => {
                    let db = self
                        .db
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    DeliveryRepo::mark_failed(db.conn(), &row.id, now, &error.to_string())?;
                    report.failed += 1;
                    continue;
                }
            };
            match deliver(&self.store, discord, &delivery).await {
                Ok(()) => {
                    let db = self
                        .db
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    DeliveryRepo::mark_sent(db.conn(), &row.id, now)?;
                    report.sent += 1;
                }
                Err(error) if permanent(&error) => {
                    let sent_fallback = self.fallback(discord, &delivery, &error).await?;
                    let db = self
                        .db
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if sent_fallback {
                        DeliveryRepo::mark_sent(db.conn(), &row.id, now)?;
                        report.sent += 1;
                    } else {
                        DeliveryRepo::mark_failed(db.conn(), &row.id, now, &error.to_string())?;
                        if let Some(channel) = &self.alert_channel
                            && !matches!(delivery, PendingDelivery::Channel { .. })
                        {
                            let alert = PendingDelivery::Channel {
                                channel_id: channel.clone(),
                                content: format!(
                                    "Delivery failed for {}. {} Open a private thread with the participant or retry the send.",
                                    describe(&delivery),
                                    error
                                ),
                            };
                            enqueue_pending(db.conn(), &row.event_id, &alert, now)?;
                        }
                        report.failed += 1;
                    }
                }
                Err(error) => {
                    let db = self
                        .db
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    DeliveryRepo::mark_retry(db.conn(), &row.id, now, &error.to_string())?;
                    report.retried += 1;
                }
            }
        }
        Ok(report)
    }
}

/// Deliver pending rows once. A permanent failure is marked failed; a
/// transient failure stays pending for the next pass.
pub async fn dispatch_pending(
    db: &Db,
    store: &BlobStore,
    discord: &dyn DiscordPort,
    now: i64,
    limit: usize,
) -> Result<DeliveryReport, DeliveryError> {
    let rows = DeliveryRepo::pending(db.conn(), limit)?;
    let mut report = DeliveryReport::default();
    for row in rows {
        if row.attempts >= MAX_DELIVERY_ATTEMPTS {
            DeliveryRepo::mark_failed(db.conn(), &row.id, now, "retry limit reached")?;
            report.failed += 1;
            continue;
        }
        let delivery = match PendingDelivery::decode(&row.payload) {
            Ok(delivery) => delivery,
            Err(error) => {
                DeliveryRepo::mark_failed(db.conn(), &row.id, now, &error.to_string())?;
                report.failed += 1;
                continue;
            }
        };
        match deliver(store, discord, &delivery).await {
            Ok(()) => {
                DeliveryRepo::mark_sent(db.conn(), &row.id, now)?;
                report.sent += 1;
            }
            Err(error) if permanent(&error) => {
                DeliveryRepo::mark_failed(db.conn(), &row.id, now, &error.to_string())?;
                report.failed += 1;
            }
            Err(error) => {
                DeliveryRepo::mark_retry(db.conn(), &row.id, now, &error.to_string())?;
                report.retried += 1;
            }
        }
    }
    Ok(report)
}

async fn deliver(
    store: &BlobStore,
    discord: &dyn DiscordPort,
    delivery: &PendingDelivery,
) -> Result<(), DiscordError> {
    match delivery {
        PendingDelivery::Dm { user_id, content } => discord
            .send_dm(
                &DiscordUserId::new(user_id.clone()),
                OutboundMessage {
                    content: content.clone(),
                },
            )
            .await
            .map(|_| ()),
        PendingDelivery::Offer {
            user_id,
            track_code,
            due_at,
            claim_window_secs,
            ..
        } => {
            let hours = (*claim_window_secs as f64) / 3600.0;
            let content = format!(
                "Track {track_code} is available. Claim within {hours} hours (due {due_at})."
            );
            discord
                .send_dm(
                    &DiscordUserId::new(user_id.clone()),
                    OutboundMessage { content },
                )
                .await
                .map(|_| ())
        }
        PendingDelivery::File {
            user_id,
            filename,
            sha256,
            caption,
            ..
        } => {
            let bytes = store
                .read(sha256)
                .map_err(|error| DiscordError::Other(error.to_string()))?;
            discord
                .send_file(
                    &DiscordUserId::new(user_id.clone()),
                    OutboundFile {
                        filename: filename.clone(),
                        bytes,
                        caption: caption.clone(),
                    },
                )
                .await
                .map(|_| ())
        }
        PendingDelivery::Channel {
            channel_id,
            content,
        } => discord
            .send_channel_message(
                &DiscordChannelId::new(channel_id.clone()),
                OutboundMessage {
                    content: content.clone(),
                },
            )
            .await
            .map(|_| ()),
    }
}

fn user_of(delivery: &PendingDelivery) -> Option<&str> {
    match delivery {
        PendingDelivery::Dm { user_id, .. }
        | PendingDelivery::Offer { user_id, .. }
        | PendingDelivery::File { user_id, .. } => Some(user_id),
        PendingDelivery::Channel { .. } => None,
    }
}

fn describe(delivery: &PendingDelivery) -> String {
    match delivery {
        PendingDelivery::Dm { user_id, .. } => format!("a message to participant {user_id}"),
        PendingDelivery::Offer {
            user_id,
            track_code,
            ..
        } => format!("offer {track_code} to participant {user_id}"),
        PendingDelivery::File {
            user_id,
            track_code,
            ..
        } => format!("file {track_code} to participant {user_id}"),
        PendingDelivery::Channel { channel_id, .. } => format!("channel {channel_id}"),
    }
}

fn permanent(error: &DiscordError) -> bool {
    matches!(
        error,
        DiscordError::DmClosed | DiscordError::NoMutualGuild | DiscordError::UnknownUser
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use swap_core::EventState;
    use swap_db::{EventRecord, EventRepo};
    use swap_discord::FakeDiscord;
    use tempfile::tempdir;

    fn database() -> Db {
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(
            db.conn(),
            &EventRecord {
                id: "evt-1".to_string(),
                name: "Test Swap".to_string(),
                state: EventState::Draft,
                seed: 42,
                settings_json: "{}".to_string(),
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        db
    }

    fn enqueue(db: &Db, delivery: &PendingDelivery, now: i64) {
        enqueue_pending(db.conn(), "evt-1", delivery, now).unwrap();
    }

    #[tokio::test]
    async fn a_pending_dm_is_sent_once() {
        let db = database();
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let discord = FakeDiscord::new();
        let delivery = PendingDelivery::Dm {
            user_id: "u1".to_string(),
            content: "hello".to_string(),
        };
        enqueue(&db, &delivery, 10);
        enqueue(&db, &delivery, 11);

        let report = dispatch_pending(&db, &store, &discord, 20, 10)
            .await
            .unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(discord.sent_dms().len(), 1);
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);
    }

    #[tokio::test]
    async fn the_worker_is_send_and_runs_on_a_spawned_task() {
        let db = Arc::new(Mutex::new(database()));
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        {
            let db = db.lock().unwrap();
            enqueue_pending(
                db.conn(),
                "evt-1",
                &PendingDelivery::Dm {
                    user_id: "u1".to_string(),
                    content: "hello".to_string(),
                },
                10,
            )
            .unwrap();
        }
        let discord = Arc::new(FakeDiscord::new());
        let worker = DeliveryWorker::new(db.clone(), store.clone());
        let task_discord = discord.clone();
        let handle =
            tokio::spawn(async move { worker.drain(task_discord.as_ref(), 20, 10).await.unwrap() });
        let report = handle.await.unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(discord.sent_dms().len(), 1);
    }

    #[tokio::test]
    async fn a_file_is_loaded_from_the_blob_store() {
        let db = database();
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let blob = store.store(b"module bytes").unwrap();
        let discord = FakeDiscord::new();
        enqueue(
            &db,
            &PendingDelivery::File {
                user_id: "u1".to_string(),
                track_code: "SAAAA".to_string(),
                filename: "SAAAA.it".to_string(),
                sha256: blob.sha256.clone(),
                caption: "Track SAAAA".to_string(),
            },
            10,
        );

        let report = dispatch_pending(&db, &store, &discord, 20, 10)
            .await
            .unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(discord.sent_files().len(), 1);
        assert_eq!(discord.sent_files()[0].file.bytes, b"module bytes");
    }

    #[tokio::test]
    async fn a_closed_dm_fails_but_a_transport_error_retries() {
        let db = database();
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let discord = FakeDiscord::new();
        enqueue(
            &db,
            &PendingDelivery::Dm {
                user_id: "u1".to_string(),
                content: "hello".to_string(),
            },
            10,
        );
        discord.fail_next_dm(DiscordError::DmClosed);
        let report = dispatch_pending(&db, &store, &discord, 20, 10)
            .await
            .unwrap();
        assert_eq!(report.failed, 1);
        let row = DeliveryRepo::get(db.conn(), &delivery_key("u1", "hello"))
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "failed");

        enqueue(
            &db,
            &PendingDelivery::Dm {
                user_id: "u2".to_string(),
                content: "hello".to_string(),
            },
            30,
        );
        discord.fail_next_dm(DiscordError::Transport("offline".to_string()));
        let report = dispatch_pending(&db, &store, &discord, 40, 10)
            .await
            .unwrap();
        assert_eq!(report.retried, 1);
        let row = DeliveryRepo::get(db.conn(), &delivery_key("u2", "hello"))
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 1);
    }

    fn delivery_key(user_id: &str, content: &str) -> String {
        format!(
            "evt-1:{}",
            PendingDelivery::Dm {
                user_id: user_id.to_string(),
                content: content.to_string(),
            }
            .dedupe_key()
        )
    }

    #[test]
    fn an_effect_becomes_a_pending_delivery() {
        let effect = Effect::Offer {
            user_id: "u1".to_string(),
            event_id: "evt-1".to_string(),
            assignment_id: "a1".to_string(),
            track_code: "SAAAA".to_string(),
            due_at: 2_000,
            claim_window_secs: 86_400,
        };
        let pending = PendingDelivery::from_effect(&effect);
        assert_eq!(pending.kind(), "offer");
        assert_eq!(pending.dedupe_key(), "offer:evt-1:a1");
        assert_eq!(sha256_hex(b"x").len(), 64);
    }
}

#[cfg(test)]
mod worker_alerts {
    use super::*;
    use swap_core::EventState;
    use swap_db::{EventRecord, EventRepo};
    use swap_discord::FakeDiscord;
    use tempfile::tempdir;

    #[tokio::test]
    async fn a_closed_dm_uses_a_private_thread_when_configured() {
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(
            db.conn(),
            &EventRecord {
                id: "evt-1".to_string(),
                name: "Test Swap".to_string(),
                state: EventState::Active,
                seed: 42,
                settings_json: "{}".to_string(),
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        let db = Arc::new(Mutex::new(db));
        {
            let db = db.lock().unwrap();
            enqueue_pending(
                db.conn(),
                "evt-1",
                &PendingDelivery::Dm {
                    user_id: "u1".to_string(),
                    content: "hello".to_string(),
                },
                10,
            )
            .unwrap();
        }
        let discord = FakeDiscord::new();
        discord.fail_next_dm(DiscordError::DmClosed);
        let worker = DeliveryWorker::new(db.clone(), store).with_fallback_channel("announcements");
        let report = worker.drain(&discord, 20, 10).await.unwrap();
        assert_eq!(report.sent, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(discord.created_threads().len(), 1);
        assert_eq!(discord.thread_members().len(), 1);
        assert_eq!(discord.channel_messages().len(), 1);
    }

    #[tokio::test]
    async fn a_closed_dm_queues_a_host_alert() {
        let directory = tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(
            db.conn(),
            &EventRecord {
                id: "evt-1".to_string(),
                name: "Test Swap".to_string(),
                state: EventState::Active,
                seed: 42,
                settings_json: "{}".to_string(),
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        let db = Arc::new(Mutex::new(db));
        {
            let db = db.lock().unwrap();
            enqueue_pending(
                db.conn(),
                "evt-1",
                &PendingDelivery::Dm {
                    user_id: "u1".to_string(),
                    content: "hello".to_string(),
                },
                10,
            )
            .unwrap();
        }
        let discord = FakeDiscord::new();
        discord.fail_next_dm(DiscordError::DmClosed);
        let worker = DeliveryWorker::new(db.clone(), store).with_alert_channel("alerts");
        let report = worker.drain(&discord, 20, 10).await.unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.sent, 1);
        assert_eq!(discord.channel_messages().len(), 1);
        assert!(
            discord.channel_messages()[0]
                .message
                .content
                .contains("Delivery failed")
        );
        assert_eq!(
            DeliveryRepo::count_pending(db.lock().unwrap().conn()).unwrap(),
            0
        );
    }
}
