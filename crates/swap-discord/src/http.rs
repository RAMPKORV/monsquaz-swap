//! Live Discord REST adapter for `DiscordPort`.
//!
//! The adapter holds the token in the HTTP client only. Every method resolves
//! the direct-message channel first, because Discord identifies a DM by the
//! channel, not by the recipient.

use async_trait::async_trait;
use twilight_http::api_error::ApiError;
use twilight_http::client::Client;
use twilight_http::error::ErrorType;
use twilight_model::channel::ChannelType;
use twilight_model::channel::message::Message;
use twilight_model::channel::message::MessageType;
use twilight_model::http::attachment::Attachment as DiscordAttachment;
use twilight_model::id::Id;
use twilight_model::id::marker::{ChannelMarker, MessageMarker, UserMarker};

use crate::inbound::DmAttachment;
use crate::port::{
    AttachmentError, AttachmentFetcher, DiscordError, DiscordPort, InboundDm, OutboundFile,
    OutboundMessage,
};
use crate::setup::validate_bot_token_shape;
use crate::{DiscordChannelId, DiscordUserId, MessageId};

/// Discord REST adapter for participant delivery and DM recovery.
pub struct HttpDiscordPort {
    http: Client,
}

impl HttpDiscordPort {
    pub fn new(token: &str) -> Result<Self, DiscordError> {
        validate_bot_token_shape(token).map_err(|error| DiscordError::Other(error.to_string()))?;
        Ok(Self {
            http: Client::new(token.trim().to_string()),
        })
    }

    /// Build a port that sends requests through a local Discord API mock.
    /// The host is `host:port`; the connection uses HTTP. This is a test hook
    /// and is not part of the production API.
    #[cfg(feature = "test-proxy")]
    pub fn with_proxy(token: &str, host: impl Into<String>) -> Result<Self, DiscordError> {
        validate_bot_token_shape(token).map_err(|error| DiscordError::Other(error.to_string()))?;
        let http = Client::builder()
            .token(token.trim().to_string())
            .proxy(host.into(), true)
            .ratelimiter(None)
            .build();
        Ok(Self { http })
    }

    async fn open_dm(&self, user_id: &DiscordUserId) -> Result<Id<ChannelMarker>, DiscordError> {
        let user = parse_id::<UserMarker>(&user_id.0)?;
        let channel = self
            .http
            .create_private_channel(user)
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(map_body_error)?;
        Ok(channel.id)
    }

    async fn send_message(
        &self,
        channel: Id<ChannelMarker>,
        message: &OutboundMessage,
    ) -> Result<MessageId, DiscordError> {
        let sent = self
            .http
            .create_message(channel)
            .content(&message.content)
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(map_body_error)?;
        Ok(MessageId::new(sent.id.get().to_string()))
    }

    fn map_message(message: Message) -> InboundDm {
        InboundDm {
            message_id: MessageId::new(message.id.get().to_string()),
            content: message.content,
            attachments: message
                .attachments
                .into_iter()
                .map(|attachment| DmAttachment {
                    id: attachment.id.get().to_string(),
                    filename: attachment.filename,
                    size: attachment.size,
                    url: attachment.url,
                })
                .collect(),
        }
    }
}

#[async_trait]
impl DiscordPort for HttpDiscordPort {
    async fn send_dm(
        &self,
        user_id: &DiscordUserId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError> {
        let channel = self.open_dm(user_id).await?;
        self.send_message(channel, &message).await
    }

    async fn send_file(
        &self,
        user_id: &DiscordUserId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError> {
        let channel = self.open_dm(user_id).await?;
        let filename = safe_filename(&file.filename);
        let attachment = DiscordAttachment {
            file: file.bytes,
            filename: filename.to_string(),
            id: 0,
            description: None,
            duration_secs: None,
            is_spoiler: None,
            title: None,
            waveform: None,
        };
        let mut message = self.http.create_message(channel);
        if !file.caption.trim().is_empty() {
            message = message.content(&file.caption);
        }
        let sent = message
            .attachments(std::slice::from_ref(&attachment))
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(map_body_error)?;
        Ok(MessageId::new(sent.id.get().to_string()))
    }

    async fn edit_dm(
        &self,
        user_id: &DiscordUserId,
        message_id: &MessageId,
        message: OutboundMessage,
    ) -> Result<(), DiscordError> {
        let channel = self.open_dm(user_id).await?;
        let message_id = parse_id::<MessageMarker>(&message_id.0)?;
        self.http
            .update_message(channel, message_id)
            .content(Some(&message.content))
            .await
            .map_err(map_http_error)?;
        Ok(())
    }

    async fn send_channel_message(
        &self,
        channel_id: &DiscordChannelId,
        message: OutboundMessage,
    ) -> Result<MessageId, DiscordError> {
        let channel = parse_id::<ChannelMarker>(&channel_id.0)?;
        self.send_message(channel, &message).await
    }

    async fn create_private_thread(
        &self,
        channel_id: &DiscordChannelId,
        name: &str,
    ) -> Result<DiscordChannelId, DiscordError> {
        let channel = parse_id::<ChannelMarker>(&channel_id.0)?;
        let thread = self
            .http
            .create_thread(channel, name, ChannelType::PrivateThread)
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(map_body_error)?;
        Ok(DiscordChannelId::new(thread.id.get().to_string()))
    }

    async fn add_thread_member(
        &self,
        thread_id: &DiscordChannelId,
        user_id: &DiscordUserId,
    ) -> Result<(), DiscordError> {
        let thread = parse_id::<ChannelMarker>(&thread_id.0)?;
        let user = parse_id::<UserMarker>(&user_id.0)?;
        self.http
            .add_thread_member(thread, user)
            .await
            .map_err(map_http_error)?;
        Ok(())
    }

    async fn send_file_to_channel(
        &self,
        channel_id: &DiscordChannelId,
        file: OutboundFile,
    ) -> Result<MessageId, DiscordError> {
        let channel = parse_id::<ChannelMarker>(&channel_id.0)?;
        let filename = safe_filename(&file.filename);
        let attachment = DiscordAttachment {
            file: file.bytes,
            filename: filename.to_string(),
            id: 0,
            description: None,
            duration_secs: None,
            is_spoiler: None,
            title: None,
            waveform: None,
        };
        let mut message = self.http.create_message(channel);
        if !file.caption.trim().is_empty() {
            message = message.content(&file.caption);
        }
        let sent = message
            .attachments(std::slice::from_ref(&attachment))
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(map_body_error)?;
        Ok(MessageId::new(sent.id.get().to_string()))
    }

    async fn fetch_dm_after(
        &self,
        user_id: &DiscordUserId,
        after: Option<&MessageId>,
    ) -> Result<Vec<InboundDm>, DiscordError> {
        let channel = self.open_dm(user_id).await?;
        let mut cursor = match after {
            Some(message_id) => Some(parse_id::<MessageMarker>(&message_id.0)?),
            None => None,
        };
        let mut messages = Vec::new();
        // Recover at most 1000 messages after the cursor. The caller records a
        // cursor on every successful delivery, so a normal outage is smaller.
        for _ in 0..10 {
            let response = match cursor {
                Some(message_id) => {
                    self.http
                        .channel_messages(channel)
                        .after(message_id)
                        .limit(100)
                        .await
                }
                None => self.http.channel_messages(channel).limit(100).await,
            }
            .map_err(map_http_error)?;
            let page = response.models().await.map_err(map_body_error)?;
            let full_page = page.len() == 100;
            let next = page.iter().map(|message| message.id).max();
            messages.extend(page);
            if !full_page {
                break;
            }
            match next {
                Some(message_id) if Some(message_id) != cursor => cursor = Some(message_id),
                _ => break,
            }
        }
        messages.sort_by_key(|message| message.id.get());
        Ok(messages
            .into_iter()
            .filter(|message| {
                !message.author.bot
                    && matches!(message.kind, MessageType::Regular | MessageType::Reply)
            })
            .map(Self::map_message)
            .collect())
    }
}

fn parse_id<M>(value: &str) -> Result<Id<M>, DiscordError> {
    value
        .parse::<u64>()
        .map(Id::new)
        .map_err(|_| DiscordError::Transport("Discord id is not a snowflake".to_string()))
}

fn safe_filename(value: &str) -> &str {
    let name = value.rsplit(['/', '\\']).next().unwrap_or(value).trim();
    if name.is_empty() || name == "." || name == ".." {
        "track.bin"
    } else {
        name
    }
}

fn map_body_error(error: twilight_http::response::DeserializeBodyError) -> DiscordError {
    DiscordError::Transport(format!("Discord response is unreadable: {error}"))
}

fn map_http_error(error: twilight_http::Error) -> DiscordError {
    if let ErrorType::Response {
        status, error: api, ..
    } = error.kind()
    {
        if status.get() == 429 {
            return DiscordError::RateLimited;
        }
        return map_api_error(api);
    }
    match error.kind() {
        ErrorType::Unauthorized => {
            DiscordError::Other("Discord rejected the bot token".to_string())
        }
        _ => DiscordError::Transport("Discord request failed".to_string()),
    }
}

fn map_api_error(error: &ApiError) -> DiscordError {
    match error {
        ApiError::General(api) => map_api_code(api.code),
        ApiError::Ratelimited(_) => DiscordError::RateLimited,
        ApiError::Message(_) => {
            DiscordError::Other("Discord rejected the message fields".to_string())
        }
        _ => DiscordError::Other("Discord rejected the request".to_string()),
    }
}

fn map_api_code(code: u64) -> DiscordError {
    match code {
        10013 => DiscordError::UnknownUser,
        50007 => DiscordError::DmClosed,
        50001 | 50002 | 50013 => DiscordError::MissingAccess,
        code => DiscordError::Other(format!("Discord API error {code}")),
    }
}

/// Text-like channels the host can map for announcements and the reveal.
pub fn is_text_channel(kind: ChannelType) -> bool {
    matches!(
        kind,
        ChannelType::GuildText | ChannelType::GuildAnnouncement
    )
}

/// Downloads Discord CDN attachments for immediate intake.
pub struct HttpAttachmentFetcher {
    client: reqwest::Client,
}

impl HttpAttachmentFetcher {
    pub fn new() -> Result<Self, AttachmentError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| AttachmentError::Transport(error.to_string()))?;
        Ok(Self { client })
    }
}

impl Default for HttpAttachmentFetcher {
    fn default() -> Self {
        Self::new().expect("the attachment HTTP client must build")
    }
}

#[async_trait]
impl AttachmentFetcher for HttpAttachmentFetcher {
    async fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, AttachmentError> {
        if !url.starts_with("https://cdn.discordapp.com/")
            && !url.starts_with("https://media.discordapp.net/")
        {
            return Err(AttachmentError::InvalidUrl);
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| AttachmentError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(AttachmentError::Transport(format!(
                "Discord CDN returned {}",
                response.status()
            )));
        }
        if let Some(length) = response.content_length()
            && length > max_bytes
        {
            return Err(AttachmentError::TooLarge);
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AttachmentError::Transport(error.to_string()))?
        {
            if bytes.len() as u64 + chunk.len() as u64 > max_bytes {
                return Err(AttachmentError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_attachment_fetcher_refuses_a_foreign_url() {
        let fetcher = HttpAttachmentFetcher::new().unwrap();
        assert_eq!(
            fetcher.fetch("http://localhost/file", 10).await,
            Err(AttachmentError::InvalidUrl)
        );
    }

    #[test]
    fn a_filename_cannot_escape_the_store() {
        assert_eq!(safe_filename("../../secret.it"), "secret.it");
        assert_eq!(safe_filename("C:\\Users\\host\\track.it"), "track.it");
        assert_eq!(safe_filename(""), "track.bin");
        assert_eq!(safe_filename(".."), "track.bin");
    }

    #[test]
    fn discord_error_codes_are_classified() {
        assert_eq!(map_api_code(10013), DiscordError::UnknownUser);
        assert_eq!(map_api_code(50007), DiscordError::DmClosed);
        assert_eq!(map_api_code(50001), DiscordError::MissingAccess);
        assert_eq!(map_api_code(50002), DiscordError::MissingAccess);
        assert_eq!(map_api_code(50013), DiscordError::MissingAccess);
    }

    #[test]
    fn only_text_like_channels_are_accepted() {
        assert!(is_text_channel(ChannelType::GuildText));
        assert!(is_text_channel(ChannelType::GuildAnnouncement));
        assert!(!is_text_channel(ChannelType::GuildVoice));
        assert!(!is_text_channel(ChannelType::GuildCategory));
    }

    #[test]
    fn an_invalid_token_shape_is_refused_before_any_request() {
        assert!(HttpDiscordPort::new("short").is_err());
    }
}
