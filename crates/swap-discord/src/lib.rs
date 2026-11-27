//! Discord adapter contract for Monsquaz Swap.
//!
//! The live Twilight gateway maps Discord payloads into these DTOs and calls
//! the application services through `DiscordPort`. Keeping the contract here
//! lets the scheduler, UI, and simulation use the same fake transport.

pub mod command;
pub mod custom_id;
pub mod gateway;
#[cfg(feature = "live")]
pub mod http;
pub mod ids;
pub mod inbound;
pub mod port;
pub mod setup;
#[cfg(feature = "live")]
pub mod twilight;

pub use command::{SWAP_COMMAND, SlashCommand, parse_swap_subcommand};
pub use custom_id::{Action, CustomId, CustomIdError};
pub use gateway::{
    BackoffPolicy, Clock, GatewayConnector, GatewayError, GatewayHealth, GatewaySession,
    GatewayState, HealthSink, InboundHandler, SWAP_SUBCOMMANDS, SharedHealth, Supervisor,
    SystemClock, Timer, TokioTimer,
};
#[cfg(feature = "live")]
pub use http::{HttpAttachmentFetcher, HttpDiscordPort};
pub use ids::{DiscordChannelId, DiscordGuildId, DiscordUserId, MessageId};
pub use inbound::{DmAttachment, InboundEvent, InteractionRef};
pub use port::{
    AttachmentError, AttachmentFetcher, DiscordError, DiscordPort, FakeAttachmentFetcher,
    FakeDiscord, InboundDm, OutboundFile, OutboundMessage, SentDm, SentFile,
};
pub use setup::{
    BotIdentity, ChannelSummary, GuildSummary, RECOMMENDED_PERMISSIONS, SetupDirectory, SetupError,
    TokenValidator, invite_url, validate_bot_token_shape,
};
#[cfg(all(feature = "live", feature = "test-proxy"))]
pub use twilight::connector_with_proxy;
#[cfg(feature = "live")]
pub use twilight::{
    TwilightConfig, TwilightConnector, TwilightSetup, TwilightTokenValidator, connector,
    subcommand_count,
};
