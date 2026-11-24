use std::collections::BTreeMap;

use crate::command::SlashCommand;
use crate::{CustomId, DiscordChannelId, DiscordGuildId, DiscordUserId, MessageId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmAttachment {
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub url: String,
}

/// Normalized inbound event. The Twilight adapter produces this; the app
/// router consumes it. No Discord payload types cross this boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundEvent {
    SlashCommand {
        user_id: DiscordUserId,
        guild_id: Option<DiscordGuildId>,
        command: SlashCommand,
        options: BTreeMap<String, String>,
    },
    Button {
        user_id: DiscordUserId,
        custom_id: CustomId,
    },
    ModalSubmit {
        user_id: DiscordUserId,
        custom_id: CustomId,
        fields: BTreeMap<String, String>,
    },
    DmAttachment {
        user_id: DiscordUserId,
        channel_id: DiscordChannelId,
        message_id: MessageId,
        attachment: DmAttachment,
    },
    MemberJoin {
        user_id: DiscordUserId,
        guild_id: DiscordGuildId,
    },
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
            Self::SlashCommand { guild_id, .. } => guild_id.as_ref(),
            Self::MemberJoin { guild_id, .. } => Some(guild_id),
            Self::Button { .. } | Self::ModalSubmit { .. } | Self::DmAttachment { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;

    #[test]
    fn the_actor_is_available_for_every_event() {
        let user = DiscordUserId::new("u1");
        let event = InboundEvent::Button {
            user_id: user.clone(),
            custom_id: CustomId::new("evt-1", Action::Claim, None).unwrap(),
        };
        assert_eq!(event.user_id(), &user);
        assert_eq!(event.guild_id(), None);
    }
}
