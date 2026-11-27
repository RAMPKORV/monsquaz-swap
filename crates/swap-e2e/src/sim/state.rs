//! In-memory state for the local Discord simulator.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use serde_json::Value;
use tokio::sync::{Notify, broadcast};

pub const DEFAULT_TOKEN: &str =
    "MTAwMDAwMDAwMDAwMDAwMDAw.GGGGGG.abcdefghijklmnopqrstuvwxyz0123456789";
pub const BOT_USER_ID: &str = "900000000000000001";
pub const APPLICATION_ID: &str = "900000000000000002";
pub const GUILD_ID: &str = "900000000000000010";
pub const ANNOUNCEMENTS_CHANNEL_ID: &str = "900000000000000011";
pub const REVEAL_CHANNEL_ID: &str = "900000000000000012";
pub const GENERAL_CHANNEL_ID: &str = "900000000000000013";
pub const SWAP_COMMAND_NAME: &str = "swap";

const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;

fn snowflake(epoch_seconds: i64) -> u64 {
    let milliseconds = epoch_seconds.saturating_mul(1000) - DISCORD_EPOCH_MS;
    (milliseconds.max(1) as u64) << 22
}

/// The seven fake participants. The order is stable across runs.
pub const PARTICIPANTS: [(&str, &str); 7] = [
    ("mira", "910000000000000001"),
    ("jonas", "910000000000000002"),
    ("selma", "910000000000000003"),
    ("theo", "910000000000000004"),
    ("ines", "910000000000000005"),
    ("ada", "910000000000000006"),
    ("rune", "910000000000000007"),
];

#[derive(Debug, Clone)]
pub struct SimConfig {
    pub token: String,
    pub bot_user_id: String,
    pub application_id: String,
    pub guild_id: String,
    pub guild_name: String,
    pub channels: Vec<SimChannelConfig>,
    /// Epoch seconds that seed the first snowflake. Tests set this to the
    /// frozen clock so message IDs are not older than the event.
    pub epoch: i64,
}

#[derive(Debug, Clone)]
pub struct SimChannelConfig {
    pub id: String,
    pub name: String,
    pub kind: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            token: DEFAULT_TOKEN.to_string(),
            bot_user_id: BOT_USER_ID.to_string(),
            application_id: APPLICATION_ID.to_string(),
            guild_id: GUILD_ID.to_string(),
            guild_name: "Monsquaz Swap Test Guild".to_string(),
            channels: vec![
                SimChannelConfig {
                    id: ANNOUNCEMENTS_CHANNEL_ID.to_string(),
                    name: "announcements".to_string(),
                    kind: 0,
                },
                SimChannelConfig {
                    id: REVEAL_CHANNEL_ID.to_string(),
                    name: "reveal".to_string(),
                    kind: 0,
                },
                SimChannelConfig {
                    id: GENERAL_CHANNEL_ID.to_string(),
                    name: "general".to_string(),
                    kind: 0,
                },
            ],
            epoch: crate::clock::system_now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimUser {
    pub id: String,
    pub username: String,
    pub bot: bool,
}

#[derive(Debug, Clone)]
pub struct SimChannel {
    pub id: String,
    pub name: String,
    pub kind: u64,
    pub guild_id: Option<String>,
    pub parent_id: Option<String>,
    pub recipients: Vec<String>,
    pub thread_members: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SimAttachment {
    pub id: String,
    pub filename: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SimMessage {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub attachments: Vec<SimAttachment>,
    pub timestamp: i64,
}

#[derive(Debug, Default)]
pub struct SimState {
    pub users: BTreeMap<String, SimUser>,
    pub channels: BTreeMap<String, SimChannel>,
    pub messages: BTreeMap<String, Vec<SimMessage>>,
    pub attachment_bytes: BTreeMap<String, Arc<Vec<u8>>>,
    pub commands: Vec<Value>,
    pub dm_by_user: BTreeMap<String, String>,
}

impl SimState {
    pub fn new(config: &SimConfig) -> Self {
        let mut state = Self::default();
        state.users.insert(
            config.bot_user_id.clone(),
            SimUser {
                id: config.bot_user_id.clone(),
                username: "Monsquaz Test Bot".to_string(),
                bot: true,
            },
        );
        for (name, id) in PARTICIPANTS {
            state.users.insert(
                id.to_string(),
                SimUser {
                    id: id.to_string(),
                    username: name.to_string(),
                    bot: false,
                },
            );
        }
        for channel in &config.channels {
            state.channels.insert(
                channel.id.clone(),
                SimChannel {
                    id: channel.id.clone(),
                    name: channel.name.clone(),
                    kind: channel.kind,
                    guild_id: Some(config.guild_id.clone()),
                    parent_id: None,
                    recipients: Vec::new(),
                    thread_members: Vec::new(),
                },
            );
        }
        state
    }

    pub fn dm_channel_for(&self, user_id: &str) -> Option<&SimChannel> {
        self.dm_by_user
            .get(user_id)
            .and_then(|channel_id| self.channels.get(channel_id))
    }
}

/// A gateway frame that the WebSocket handler sends to the bot.
#[derive(Debug, Clone)]
pub struct GatewayFrame {
    pub text: String,
}

pub(crate) struct SimShared {
    pub config: SimConfig,
    pub base_url: String,
    pub state: RwLock<SimState>,
    pub events: broadcast::Sender<GatewayFrame>,
    pub sequence: AtomicU64,
    pub next_id: AtomicU64,
    pub epoch: AtomicI64,
    pub ready: AtomicBool,
    pub ready_notify: Notify,
    pub commands_registered: AtomicBool,
    pub commands_notify: Notify,
}

impl SimShared {
    pub fn new(config: SimConfig, base_url: String) -> Self {
        let (events, _) = broadcast::channel(256);
        let epoch = config.epoch;
        let epoch_ms = config.epoch.saturating_mul(1000) - DISCORD_EPOCH_MS;
        let first_id = (epoch_ms.max(1) as u64) << 22;
        Self {
            state: RwLock::new(SimState::new(&config)),
            config,
            base_url,
            events,
            sequence: AtomicU64::new(0),
            next_id: AtomicU64::new(first_id),
            epoch: AtomicI64::new(epoch),
            ready: AtomicBool::new(false),
            ready_notify: Notify::new(),
            commands_registered: AtomicBool::new(false),
            commands_notify: Notify::new(),
        }
    }

    pub fn state(&self) -> std::sync::RwLockReadGuard<'_, SimState> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn state_mut(&self) -> std::sync::RwLockWriteGuard<'_, SimState> {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn new_id(&self) -> String {
        let epoch = self.epoch.load(Ordering::Relaxed);
        let base = snowflake(epoch);
        let mut current = self.next_id.load(Ordering::Relaxed);
        loop {
            let next = current.saturating_add(1).max(base);
            match self.next_id.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next.to_string(),
                Err(actual) => current = actual,
            }
        }
    }

    /// Move the snowflake epoch forward with the test clock.
    pub fn set_epoch(&self, epoch: i64) {
        self.epoch.fetch_max(epoch, Ordering::Relaxed);
    }

    /// Send one dispatch event to every connected gateway.
    pub fn dispatch(&self, event: &str, data: Value) -> u64 {
        let sequence = self.next_sequence();
        let text = serde_json::json!({
            "op": 0,
            "t": event,
            "s": sequence,
            "d": data,
        })
        .to_string();
        let _ = self.events.send(GatewayFrame { text });
        sequence
    }

    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
        self.ready_notify.notify_waiters();
    }

    pub fn mark_commands_registered(&self) {
        self.commands_registered.store(true, Ordering::SeqCst);
        self.commands_notify.notify_waiters();
    }
}
