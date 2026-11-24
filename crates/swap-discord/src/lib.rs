//! Discord adapter contract for Monsquaz Swap.
//!
//! The live Twilight gateway maps Discord payloads into these DTOs and calls
//! the application services through `DiscordPort`. Keeping the contract here
//! lets the scheduler, UI, and simulation use the same fake transport.

pub mod command;
pub mod custom_id;
pub mod ids;
pub mod inbound;
pub mod port;
pub mod setup;

pub use command::{SWAP_COMMAND, SlashCommand, parse_swap_subcommand};
pub use custom_id::{Action, CustomId, CustomIdError};
pub use ids::{DiscordChannelId, DiscordGuildId, DiscordUserId, MessageId};
pub use inbound::{DmAttachment, InboundEvent};
pub use port::{
    DiscordError, DiscordPort, FakeDiscord, InboundDm, OutboundFile, OutboundMessage, SentDm,
    SentFile,
};
pub use setup::{
    BotIdentity, RECOMMENDED_PERMISSIONS, SetupError, TokenValidator, invite_url,
    validate_bot_token_shape,
};
