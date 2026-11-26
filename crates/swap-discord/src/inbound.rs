use std::collections::BTreeMap;
use std::fmt;

use crate::command::SlashCommand;
use crate::{CustomId, DiscordChannelId, DiscordGuildId, DiscordUserId, MessageId};

#[derive(Clone, PartialEq, Eq)]
pub struct DmAttachment {
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub url: String,
}

/// The short-lived interaction context needed to acknowledge an interaction.
/// The token is redacted in Debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct InteractionRef {
    pub id: String,
    token: String,
}

impl InteractionRef {
    pub fn new(id: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            token: token.into(),
        }
    }

    /// The raw token is available to the HTTP adapter only.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for InteractionRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InteractionRef")
            .field("id", &self.id)
            .field("token", &"[redacted]")
            .finish()
    }
}

/// Normalized inbound event. The Twilight adapter produces this; the app
/// router consumes it. No Discord payload types cross this boundary.
#[derive(Clone, PartialEq, Eq)]
pub enum InboundEvent {
    SlashCommand {
        interaction: InteractionRef,
        user_id: DiscordUserId,
        guild_id: Option<DiscordGuildId>,
        command: SlashCommand,
        options: BTreeMap<String, String>,
    },
    Button {
        interaction: InteractionRef,
        user_id: DiscordUserId,
        guild_id: Option<DiscordGuildId>,
        custom_id: CustomId,
    },
    ModalSubmit {
        interaction: InteractionRef,
        user_id: DiscordUserId,
        guild_id: Option<DiscordGuildId>,
        custom_id: CustomId,
        fields: BTreeMap<String, String>,
    },
    DmAttachment {
        user_id: DiscordUserId,
        channel_id: DiscordChannelId,
        message_id: MessageId,
        author_name: Option<String>,
        attachment: DmAttachment,
    },
    MemberJoin {
        user_id: DiscordUserId,
        guild_id: DiscordGuildId,
    },
}

impl fmt::Debug for DmAttachment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DmAttachment")
            .field("id", &self.id)
            .field("size", &self.size)
            .field("filename", &"[redacted]")
            .field("url", &"[redacted]")
            .finish()
    }
}

impl fmt::Debug for InboundEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlashCommand {
                interaction,
                user_id,
                guild_id,
                command,
                options,
            } => formatter
                .debug_struct("SlashCommand")
                .field("interaction", interaction)
                .field("user_id", user_id)
                .field("guild_id", guild_id)
                .field("command", command)
                .field("option_count", &options.len())
                .finish(),
            Self::Button {
                interaction,
                user_id,
                guild_id,
                custom_id,
            } => formatter
                .debug_struct("Button")
                .field("interaction", interaction)
                .field("user_id", user_id)
                .field("guild_id", guild_id)
                .field("event_id", &custom_id.event_id())
                .field("action", &custom_id.action())
                .finish(),
            Self::ModalSubmit {
                interaction,
                user_id,
                guild_id,
                custom_id,
                fields,
            } => formatter
                .debug_struct("ModalSubmit")
                .field("interaction", interaction)
                .field("user_id", user_id)
                .field("guild_id", guild_id)
                .field("event_id", &custom_id.event_id())
                .field("action", &custom_id.action())
                .field("field_count", &fields.len())
                .finish(),
            Self::DmAttachment {
                user_id,
                channel_id,
                message_id,
                author_name,
                attachment,
            } => formatter
                .debug_struct("DmAttachment")
                .field("user_id", user_id)
                .field("channel_id", channel_id)
                .field("message_id", message_id)
                .field("author_name", author_name)
                .field("attachment", attachment)
                .finish(),
            Self::MemberJoin { user_id, guild_id } => formatter
                .debug_struct("MemberJoin")
                .field("user_id", user_id)
                .field("guild_id", guild_id)
                .finish(),
        }
    }
}

impl InboundEvent {
    pub fn user_id(&self) -> &DiscordUserId {
        match self {
            Self::SlashCommand { user_id, .. }
            | Self::Button { user_id, .. }
            | Self::ModalSubmit { user_id, .. }
            | Self::DmAttachment { user_id, .. }
            | Self::MemberJoin { user_id, .. } => user_id,
        }
    }

    pub fn guild_id(&self) -> Option<&DiscordGuildId> {
        match self {
            Self::SlashCommand { guild_id, .. }
            | Self::Button { guild_id, .. }
            | Self::ModalSubmit { guild_id, .. } => guild_id.as_ref(),
            Self::MemberJoin { guild_id, .. } => Some(guild_id),
            Self::DmAttachment { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;

    #[test]
    fn interaction_debug_output_redacts_the_token() {
        let interaction = InteractionRef::new("i1", "secret-token");
        let debug = format!("{interaction:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn event_debug_output_redacts_user_content() {
        let mut options = BTreeMap::new();
        options.insert("title".to_string(), "secret title".to_string());
        let event = InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i1", "super-secret"),
            user_id: DiscordUserId::new("u1"),
            guild_id: None,
            command: SlashCommand::Title,
            options,
        };
        let debug = format!("{event:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret title"));
        assert!(!debug.contains("super-secret"));

        let attachment = DmAttachment {
            id: "a1".to_string(),
            filename: "secret.it".to_string(),
            size: 12,
            url: "https://cdn.example/secret".to_string(),
        };
        let debug = format!("{attachment:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret.it"));
        assert!(!debug.contains("cdn.example"));
    }

    #[test]
    fn the_actor_is_available_for_every_event() {
        let user = DiscordUserId::new("u1");
        let event = InboundEvent::Button {
            interaction: InteractionRef::new("i1", "token"),
            user_id: user.clone(),
            guild_id: Some(DiscordGuildId::new("g1")),
            custom_id: CustomId::new("evt-1", Action::Claim, None).unwrap(),
        };
        assert_eq!(event.user_id(), &user);
        assert_eq!(event.guild_id(), Some(&DiscordGuildId::new("g1")));
    }
}
