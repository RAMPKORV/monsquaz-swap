//! A local Discord API and gateway simulator.
//!
//! The simulator gives the real Twilight adapter a full loop: HTTP requests,
//! gateway events, attachments, threads, and interactions. It uses no real
//! Discord account and no network outside the machine.

mod gateway;
mod http;
mod payload;
mod state;

use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub use http::{InjectInteractionRequest, InjectMessageRequest, default_guild_id, participant_id};
pub use payload::SimInteraction;
pub use state::{
    ANNOUNCEMENTS_CHANNEL_ID, APPLICATION_ID, BOT_USER_ID, DEFAULT_TOKEN, GENERAL_CHANNEL_ID,
    GUILD_ID, GatewayFrame, PARTICIPANTS, REVEAL_CHANNEL_ID, SimConfig, SimState,
};

/// A running simulator. The server stops when this value drops.
pub struct DiscordSim {
    shared: Arc<state::SimShared>,
    base_url: String,
    http_host: String,
    ws_url: String,
    task: JoinHandle<()>,
}

impl DiscordSim {
    pub async fn start() -> io::Result<Self> {
        Self::with_config(SimConfig::default()).await
    }

    pub async fn with_config(config: SimConfig) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let base_url = format!("http://{address}");
        let shared = Arc::new(state::SimShared::new(config, base_url.clone()));
        let router = Router::new()
            .route("/", get(gateway::upgrade))
            .route("/_control/ping", get(|| async { "pong" }))
            .with_state(shared.clone())
            .merge(http::router(shared.clone()));
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                eprintln!("the Discord simulator stopped: {error}");
            }
        });
        Ok(Self {
            shared,
            base_url,
            http_host: address.to_string(),
            ws_url: format!("ws://{address}"),
            task,
        })
    }

    pub fn token(&self) -> &str {
        &self.shared.config.token
    }

    pub fn guild_id(&self) -> &str {
        &self.shared.config.guild_id
    }

    /// The `host:port` value for the Twilight HTTP proxy hook.
    pub fn http_host(&self) -> &str {
        &self.http_host
    }

    /// The `ws://host:port` value for the Twilight gateway proxy hook.
    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    /// The base URL that an attachment fetcher must accept.
    pub fn cdn_base(&self) -> &str {
        &self.base_url
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn bot_user_id(&self) -> &str {
        &self.shared.config.bot_user_id
    }

    pub fn application_id(&self) -> &str {
        &self.shared.config.application_id
    }

    pub fn participant(&self, name: &str) -> Option<&'static str> {
        participant_id(name)
    }

    pub fn channel(&self, name: &str) -> Option<String> {
        http::channel_for(&self.shared, name)
    }

    pub fn announcements_channel(&self) -> &'static str {
        ANNOUNCEMENTS_CHANNEL_ID
    }

    pub fn reveal_channel(&self) -> &'static str {
        REVEAL_CHANNEL_ID
    }

    pub fn general_channel(&self) -> &'static str {
        GENERAL_CHANNEL_ID
    }

    pub fn is_ready(&self) -> bool {
        self.shared.ready.load(Ordering::SeqCst)
    }

    pub fn commands_registered(&self) -> bool {
        self.shared.commands_registered.load(Ordering::SeqCst)
    }

    /// Wait until the bot identifies with the gateway.
    pub async fn wait_ready(&self, timeout: Duration) -> bool {
        wait_for(&self.shared.ready_notify, || self.is_ready(), timeout).await
    }

    /// Wait until the bot registers its guild commands.
    pub async fn wait_commands(&self, timeout: Duration) -> bool {
        wait_for(
            &self.shared.commands_notify,
            || self.commands_registered(),
            timeout,
        )
        .await
    }

    /// Send a direct message from a participant and dispatch `MESSAGE_CREATE`.
    pub fn send_message(&self, request: InjectMessageRequest) -> Result<Value, String> {
        http::inject_message(&self.shared, request).map_err(|error| format!("{error:?}"))
    }

    /// Press a button as a participant.
    pub fn click(&self, user_id: &str, custom_id: &str) -> Result<Value, String> {
        self.send_interaction(InjectInteractionRequest {
            kind: "button".to_string(),
            user_id: user_id.to_string(),
            guild_id: None,
            channel_id: None,
            name: None,
            options: Default::default(),
            custom_id: Some(custom_id.to_string()),
            fields: Default::default(),
        })
    }

    /// Send a slash command as a participant in the guild.
    pub fn command(
        &self,
        user_id: &str,
        name: &str,
        options: std::collections::BTreeMap<String, Value>,
    ) -> Result<Value, String> {
        self.send_interaction(InjectInteractionRequest {
            kind: "command".to_string(),
            user_id: user_id.to_string(),
            guild_id: Some(self.guild_id().to_string()),
            channel_id: Some(GENERAL_CHANNEL_ID.to_string()),
            name: Some(name.to_string()),
            options,
            custom_id: None,
            fields: Default::default(),
        })
    }

    /// Submit a modal as a participant.
    pub fn modal(
        &self,
        user_id: &str,
        custom_id: &str,
        fields: std::collections::BTreeMap<String, String>,
    ) -> Result<Value, String> {
        self.send_interaction(InjectInteractionRequest {
            kind: "modal".to_string(),
            user_id: user_id.to_string(),
            guild_id: None,
            channel_id: None,
            name: None,
            options: Default::default(),
            custom_id: Some(custom_id.to_string()),
            fields,
        })
    }

    pub fn send_interaction(&self, request: InjectInteractionRequest) -> Result<Value, String> {
        http::inject_interaction(&self.shared, request).map_err(|error| format!("{error:?}"))
    }

    /// All messages in the DM channel for a user, oldest first.
    pub fn messages_for(&self, user_id: &str) -> Vec<Value> {
        http::messages_for(&self.shared, Some(user_id))
    }

    /// The content of every message that the bot sent to a user.
    pub fn bot_messages_for(&self, user_id: &str) -> Vec<String> {
        self.messages_for(user_id)
            .into_iter()
            .filter(|message| {
                message
                    .get("author")
                    .and_then(|author| author.get("id"))
                    .and_then(Value::as_str)
                    == Some(self.bot_user_id())
            })
            .filter_map(|message| {
                message
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect()
    }

    /// The last message that the bot sent to a user.
    pub fn last_bot_message(&self, user_id: &str) -> Option<String> {
        self.bot_messages_for(user_id).pop()
    }

    /// The attachment bytes that the bot sent to a user, with file names.
    pub fn bot_attachments_for(&self, user_id: &str) -> Vec<(String, Vec<u8>)> {
        let state = self.shared.state();
        let Some(channel) = state.dm_channel_for(user_id) else {
            return Vec::new();
        };
        let Some(messages) = state.messages.get(&channel.id) else {
            return Vec::new();
        };
        let mut output = Vec::new();
        for message in messages {
            if message.author_id != self.bot_user_id() {
                continue;
            }
            for attachment in &message.attachments {
                if let Some(bytes) = state.attachment_bytes.get(&attachment.id) {
                    output.push((attachment.filename.clone(), (**bytes).clone()));
                }
            }
        }
        output
    }

    pub fn command_count(&self) -> usize {
        http::command_count(&self.shared)
    }

    pub fn reset(&self) {
        http::reset(&self.shared);
    }

    /// Move the snowflake epoch forward when the test clock advances.
    pub fn set_epoch(&self, epoch: i64) {
        self.shared.set_epoch(epoch);
    }

    pub fn state(&self) -> std::sync::RwLockReadGuard<'_, SimState> {
        self.shared.state()
    }
}

impl Drop for DiscordSim {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn wait_for(
    notify: &tokio::sync::Notify,
    ready: impl Fn() -> bool,
    timeout: Duration,
) -> bool {
    if ready() {
        return true;
    }
    let notified = notify.notified();
    if ready() {
        return true;
    }
    tokio::time::timeout(timeout, notified).await.is_ok() && ready()
}
