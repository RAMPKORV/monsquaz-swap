use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use thiserror::Error;

use crate::inbound::{DmAttachment, InteractionRef};
use crate::{DiscordChannelId, DiscordUserId, MessageId};

/// The look of a message button. Discord maps these to button styles 1, 2,
/// and 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Danger,
}

/// One message button. The custom id is a `CustomId` string. The app checks
/// the event state and the actor again when a person presses the button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub custom_id: String,
    pub label: String,
    pub style: ButtonStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutboundMessage {
    pub content: String,
    /// Buttons in one action row. Discord allows at most five.
    pub buttons: Vec<Button>,
}

impl OutboundMessage {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            buttons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutboundFile {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub caption: String,
    /// Buttons in one action row under the file.
    pub buttons: Vec<Button>,
}

/// An ephemeral answer to an interaction. Only the person who pressed the
/// button or sent the command sees it. It is not a direct message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractionReply {
    pub content: String,
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundDm {
    pub message_id: MessageId,
    pub content: String,
    pub attachments: Vec<DmAttachment>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    #[error("the attachment URL is not a Discord CDN URL")]
    InvalidUrl,
    #[error("the attachment is larger than the configured limit")]
    TooLarge,
    #[error("the attachment download failed: {0}")]
    Transport(String),
}

/// Downloads an attachment from a Discord CDN URL. The app calls this on
/// receipt; it must never forward the URL or store it as the file location.
#[async_trait]
pub trait AttachmentFetcher: Send + Sync {
    async fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, AttachmentError>;
}

#[derive(Debug, Default)]
struct FakeAttachmentState {
    files: HashMap<String, Vec<u8>>,
    fail_next: Option<AttachmentError>,
}

#[derive(Debug, Default)]
pub struct FakeAttachmentFetcher {
    state: Mutex<FakeAttachmentState>,
}

impl FakeAttachmentFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, url: &str, bytes: &[u8]) {
        self.lock().files.insert(url.to_string(), bytes.to_vec());
    }

    pub fn fail_next(&self, error: AttachmentError) {
        self.lock().fail_next = Some(error);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeAttachmentState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl AttachmentFetcher for FakeAttachmentFetcher {
    async fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, AttachmentError> {
        let mut state = self.lock();
        if let Some(error) = state.fail_next.take() {
            return Err(error);
        }
        let bytes = state
            .files
            .get(url)
            .cloned()
            .ok_or_else(|| AttachmentError::Transport("not found".to_string()))?;
        if bytes.len() as u64 > max_bytes {
            return Err(AttachmentError::TooLarge);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DiscordError {
    #[error("direct messages are closed or the bot is blocked")]
    DmClosed,
    #[error("the user shares no guild with the bot")]
    NoMutualGuild,
    #[error("the Discord user does not exist")]
    UnknownUser,
    #[error("Discord says the bot lacks access")]
    MissingAccess,
    #[error("Discord rate-limited the request")]
    RateLimited,
    #[error("transport error: {0}")]
    Transport(String),
    #[error("Discord error: {0}")]
    Other(String),
}

/// The app calls this port. The live adapter maps it to Twilight HTTP calls;
/// tests and simulations use `FakeDiscord`.
#[async_trait]
pub trait DiscordPort: Send + Sync {
    async fn send_dm(
        &self,
        user_id: &DiscordUserId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError>;

    async fn send_file(
        &self,
        user_id: &DiscordUserId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError>;

    async fn edit_dm(
        &self,
        user_id: &DiscordUserId,
        message_id: &MessageId,
        message: OutboundMessage,
    ) -> Result<(), DiscordError>;

    async fn send_channel_message(
        &self,
        channel_id: &DiscordChannelId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError>;

    async fn create_private_thread(
        &self,
        channel_id: &DiscordChannelId,
        name: &str,
    ) -> Result<DiscordChannelId, DiscordError>;

    async fn add_thread_member(
        &self,
        thread_id: &DiscordChannelId,
        user_id: &DiscordUserId,
    ) -> Result<(), DiscordError>;

    async fn send_file_to_channel(
        &self,
        channel_id: &DiscordChannelId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError>;

    async fn fetch_dm_after(
        &self,
        user_id: &DiscordUserId,
        after: Option<&MessageId>,
    ) -> Result<Vec<InboundDm>, DiscordError>;

    /// Answer an interaction with an ephemeral message (callback type 4,
    /// flag 64). Discord accepts the answer only within 3 seconds of the
    /// interaction, and the token expires after 15 minutes.
    async fn reply_ephemeral(
        &self,
        interaction: &InteractionRef,
        reply: InteractionReply,
    ) -> Result<(), DiscordError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralReply {
    pub interaction_id: String,
    pub reply: InteractionReply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentDm {
    pub user_id: DiscordUserId,
    pub message: OutboundMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentFile {
    pub user_id: DiscordUserId,
    pub file: OutboundFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentChannelMessage {
    pub channel_id: DiscordChannelId,
    pub message: OutboundMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedThread {
    pub channel_id: DiscordChannelId,
    pub parent_id: DiscordChannelId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMember {
    pub thread_id: DiscordChannelId,
    pub user_id: DiscordUserId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentChannelFile {
    pub channel_id: DiscordChannelId,
    pub file: OutboundFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditedMessage {
    pub user_id: DiscordUserId,
    pub message_id: MessageId,
    pub message: OutboundMessage,
}

#[derive(Debug, Default)]
struct FakeState {
    next_id: u64,
    dms: Vec<SentDm>,
    files: Vec<SentFile>,
    channel_messages: Vec<SentChannelMessage>,
    threads: Vec<CreatedThread>,
    thread_members: Vec<ThreadMember>,
    channel_files: Vec<SentChannelFile>,
    edits: Vec<EditedMessage>,
    replies: Vec<EphemeralReply>,
    inbound: Vec<(DiscordUserId, InboundDm)>,
    fail_next_dm: Option<DiscordError>,
    fail_next_thread: Option<DiscordError>,
    fail_next_fetch: Option<DiscordError>,
}

#[derive(Debug, Default)]
pub struct FakeDiscord {
    state: Mutex<FakeState>,
}

impl FakeDiscord {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_inbound(&self, user_id: DiscordUserId, message: InboundDm) {
        let mut state = self.lock();
        state.inbound.push((user_id, message));
    }

    pub fn fail_next_dm(&self, error: DiscordError) {
        self.lock().fail_next_dm = Some(error);
    }

    pub fn fail_next_thread(&self, error: DiscordError) {
        self.lock().fail_next_thread = Some(error);
    }

    pub fn fail_next_fetch(&self, error: DiscordError) {
        self.lock().fail_next_fetch = Some(error);
    }

    pub fn sent_dms(&self) -> Vec<SentDm> {
        self.lock().dms.clone()
    }

    pub fn sent_files(&self) -> Vec<SentFile> {
        self.lock().files.clone()
    }

    pub fn channel_messages(&self) -> Vec<SentChannelMessage> {
        self.lock().channel_messages.clone()
    }

    pub fn created_threads(&self) -> Vec<CreatedThread> {
        self.lock().threads.clone()
    }

    pub fn thread_members(&self) -> Vec<ThreadMember> {
        self.lock().thread_members.clone()
    }

    pub fn channel_files(&self) -> Vec<SentChannelFile> {
        self.lock().channel_files.clone()
    }

    pub fn edits(&self) -> Vec<EditedMessage> {
        self.lock().edits.clone()
    }

    pub fn ephemeral_replies(&self) -> Vec<EphemeralReply> {
        self.lock().replies.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn next_message_id(state: &mut FakeState) -> MessageId {
        state.next_id += 1;
        MessageId::new(format!("m{}", state.next_id))
    }
}

#[async_trait]
impl DiscordPort for FakeDiscord {
    async fn send_dm(
        &self,
        user_id: &DiscordUserId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError> {
        let mut state = self.lock();
        if let Some(error) = state.fail_next_dm.take() {
            return Err(error);
        }
        let message_id = Self::next_message_id(&mut state);
        state.dms.push(SentDm {
            user_id: user_id.clone(),
            message,
        });
        Ok(message_id)
    }

    async fn send_file(
        &self,
        user_id: &DiscordUserId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError> {
        let mut state = self.lock();
        let message_id = Self::next_message_id(&mut state);
        state.files.push(SentFile {
            user_id: user_id.clone(),
            file,
        });
        Ok(message_id)
    }

    async fn edit_dm(
        &self,
        user_id: &DiscordUserId,
        message_id: &MessageId,
        message: OutboundMessage,
    ) -> Result<(), DiscordError> {
        self.lock().edits.push(EditedMessage {
            user_id: user_id.clone(),
            message_id: message_id.clone(),
            message,
        });
        Ok(())
    }

    async fn send_channel_message(
        &self,
        channel_id: &DiscordChannelId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError> {
        let mut state = self.lock();
        let message_id = Self::next_message_id(&mut state);
        state.channel_messages.push(SentChannelMessage {
            channel_id: channel_id.clone(),
            message,
        });
        Ok(message_id)
    }

    async fn create_private_thread(
        &self,
        channel_id: &DiscordChannelId,
        name: &str,
    ) -> Result<DiscordChannelId, DiscordError> {
        let mut state = self.lock();
        if let Some(error) = state.fail_next_thread.take() {
            return Err(error);
        }
        let thread_id = DiscordChannelId::new(format!("thread-{}", state.next_id + 1));
        state.next_id += 1;
        state.threads.push(CreatedThread {
            channel_id: thread_id.clone(),
            parent_id: channel_id.clone(),
            name: name.to_string(),
        });
        Ok(thread_id)
    }

    async fn add_thread_member(
        &self,
        thread_id: &DiscordChannelId,
        user_id: &DiscordUserId,
    ) -> Result<(), DiscordError> {
        self.lock().thread_members.push(ThreadMember {
            thread_id: thread_id.clone(),
            user_id: user_id.clone(),
        });
        Ok(())
    }

    async fn send_file_to_channel(
        &self,
        channel_id: &DiscordChannelId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError> {
        let mut state = self.lock();
        let message_id = Self::next_message_id(&mut state);
        state.channel_files.push(SentChannelFile {
            channel_id: channel_id.clone(),
            file,
        });
        Ok(message_id)
    }

    async fn fetch_dm_after(
        &self,
        user_id: &DiscordUserId,
        after: Option<&MessageId>,
    ) -> Result<Vec<InboundDm>, DiscordError> {
        let mut state = self.lock();
        if let Some(error) = state.fail_next_fetch.take() {
            return Err(error);
        }
        let mut messages: Vec<InboundDm> = state
            .inbound
            .iter()
            .filter(|(candidate, _)| candidate == user_id)
            .map(|(_, message)| message.clone())
            .collect();
        if let Some(after) = after
            && let Some(position) = messages
                .iter()
                .position(|message| &message.message_id == after)
        {
            messages = messages.split_off(position + 1);
        }
        Ok(messages)
    }

    async fn reply_ephemeral(
        &self,
        interaction: &InteractionRef,
        reply: InteractionReply,
    ) -> Result<(), DiscordError> {
        self.lock().replies.push(EphemeralReply {
            interaction_id: interaction.id.clone(),
            reply,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_dm_and_a_file_are_recorded() {
        let discord = FakeDiscord::new();
        let user = DiscordUserId::new("u1");
        let id = discord
            .send_dm(&user, OutboundMessage::text("Track S7F3A is yours"))
            .await
            .unwrap();
        assert_eq!(id, MessageId::new("m1"));
        discord
            .send_file(
                &user,
                OutboundFile {
                    filename: "S7F3A.it".to_string(),
                    bytes: vec![1, 2, 3],
                    caption: "claim".to_string(),
                    buttons: Vec::new(),
                },
            )
            .await
            .unwrap();
        assert_eq!(discord.sent_dms().len(), 1);
        assert_eq!(discord.sent_files().len(), 1);
    }

    #[tokio::test]
    async fn a_failure_can_be_injected() {
        let discord = FakeDiscord::new();
        discord.fail_next_dm(DiscordError::DmClosed);
        let result = discord
            .send_dm(&DiscordUserId::new("u1"), OutboundMessage::text("hello"))
            .await;
        assert_eq!(result, Err(DiscordError::DmClosed));
        assert!(discord.sent_dms().is_empty());
    }

    #[tokio::test]
    async fn inbound_messages_can_be_resumed_after_a_cursor() {
        let discord = FakeDiscord::new();
        let user = DiscordUserId::new("u1");
        for index in 1..=3 {
            discord.push_inbound(
                user.clone(),
                InboundDm {
                    message_id: MessageId::new(format!("m{index}")),
                    content: format!("message {index}"),
                    attachments: vec![],
                },
            );
        }
        let messages = discord
            .fetch_dm_after(&user, Some(&MessageId::new("m1")))
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message_id, MessageId::new("m2"));
    }
}
