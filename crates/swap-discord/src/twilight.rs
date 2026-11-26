//! Live Twilight adapter for the gateway contract.
//!
//! This module owns Discord payloads. Nothing outside it sees a Twilight type.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::client::Client;
use twilight_http::error::ErrorType;
use twilight_model::application::command::{
    Command, CommandOption, CommandOptionType, CommandType,
};
use twilight_model::application::interaction::InteractionData;
use twilight_model::application::interaction::application_command::{
    CommandDataOption, CommandOptionValue,
};
use twilight_model::application::interaction::modal::{
    ModalInteractionComponent, ModalInteractionData,
};
use twilight_model::id::Id;
use twilight_model::id::marker::GuildMarker;

use crate::gateway::{GatewayConnector, GatewayError, GatewaySession};
use crate::http::{HttpDiscordPort, is_text_channel};
use crate::inbound::{DmAttachment, InboundEvent, InteractionRef};
use crate::port::{DiscordError, DiscordPort, OutboundMessage};
use crate::setup::{
    BotIdentity, ChannelSummary, GuildSummary, SetupDirectory, SetupError, TokenValidator,
    validate_bot_token_shape,
};
use crate::{
    CustomId, DiscordChannelId, DiscordGuildId, DiscordUserId, MessageId, SWAP_COMMAND,
    parse_swap_subcommand,
};

const EVENT_TYPES: EventTypeFlags = EventTypeFlags::INTERACTION_CREATE
    .union(EventTypeFlags::MESSAGE_CREATE)
    .union(EventTypeFlags::READY);

/// Configuration for the live connector. The token is never printed.
#[derive(Clone)]
pub struct TwilightConfig {
    token: String,
    guild_id: DiscordGuildId,
}

impl TwilightConfig {
    pub fn new(token: impl Into<String>, guild_id: DiscordGuildId) -> Self {
        Self {
            token: token.into().trim().to_string(),
            guild_id,
        }
    }

    pub fn guild_id(&self) -> &DiscordGuildId {
        &self.guild_id
    }

    #[cfg(test)]
    fn token(&self) -> &str {
        &self.token
    }
}

/// Live directory operations for the setup wizard.
pub struct TwilightSetup {
    proxy: Option<String>,
}

impl TwilightSetup {
    pub fn new() -> Self {
        Self { proxy: None }
    }

    /// Use a local Discord API mock through `host:port`. This is a test hook
    /// and is not part of the production API.
    #[cfg(feature = "test-proxy")]
    pub fn with_proxy(host: impl Into<String>) -> Self {
        Self {
            proxy: Some(host.into()),
        }
    }

    fn client(&self, token: &str) -> Client {
        match &self.proxy {
            Some(proxy) => Client::builder()
                .token(token.trim().to_string())
                .proxy(proxy.clone(), true)
                .ratelimiter(None)
                .build(),
            None => Client::new(token.trim().to_string()),
        }
    }
}

impl Default for TwilightSetup {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SetupDirectory for TwilightSetup {
    async fn validate_token(&self, token: &str) -> Result<BotIdentity, SetupError> {
        validate_bot_token_shape(token)?;
        let http = self.client(token);
        let user = http
            .current_user()
            .await
            .map_err(map_setup_error)?
            .model()
            .await
            .map_err(|error| SetupError::Discord(error.to_string()))?;
        Ok(BotIdentity {
            id: user.id.get().to_string(),
            username: user.name,
        })
    }

    async fn guilds(&self, token: &str) -> Result<Vec<GuildSummary>, SetupError> {
        validate_bot_token_shape(token)?;
        let http = self.client(token);
        let guilds = http
            .current_user_guilds()
            .await
            .map_err(map_setup_error)?
            .models()
            .await
            .map_err(|error| SetupError::Discord(error.to_string()))?;
        Ok(guilds
            .into_iter()
            .map(|guild| GuildSummary {
                id: guild.id.get().to_string(),
                name: guild.name,
            })
            .collect())
    }

    async fn channels(
        &self,
        token: &str,
        guild_id: &str,
    ) -> Result<Vec<ChannelSummary>, SetupError> {
        validate_bot_token_shape(token)?;
        let guild_id = guild_id
            .parse::<u64>()
            .map(Id::new)
            .map_err(|_| SetupError::InvalidSelection)?;
        let http = self.client(token);
        let channels = http
            .guild_channels(guild_id)
            .await
            .map_err(map_setup_error)?
            .models()
            .await
            .map_err(|error| SetupError::Discord(error.to_string()))?;
        Ok(channels
            .into_iter()
            .filter(|channel| is_text_channel(channel.kind))
            .filter_map(|channel| {
                let name = channel.name?;
                Some(ChannelSummary {
                    id: channel.id.get().to_string(),
                    name,
                })
            })
            .collect())
    }

    async fn send_test_dm(&self, token: &str, user_id: &str) -> Result<String, SetupError> {
        validate_bot_token_shape(token)?;
        let port = {
            #[cfg(feature = "test-proxy")]
            {
                match &self.proxy {
                    Some(proxy) => HttpDiscordPort::with_proxy(token, proxy.clone()),
                    None => HttpDiscordPort::new(token),
                }
            }
            #[cfg(not(feature = "test-proxy"))]
            {
                HttpDiscordPort::new(token)
            }
        }
        .map_err(|error| SetupError::Discord(error.to_string()))?;
        let message = OutboundMessage {
            content: "Monsquaz Swap test message. Your bot can reach you.".to_string(),
        };
        let message_id = port
            .send_dm(&DiscordUserId::new(user_id), message)
            .await
            .map_err(|error| match error {
                DiscordError::DmClosed => SetupError::DmClosed,
                other => SetupError::Discord(other.to_string()),
            })?;
        Ok(message_id.0)
    }
}

fn map_setup_error(error: twilight_http::Error) -> SetupError {
    match error.kind() {
        ErrorType::Unauthorized => SetupError::TokenRejected,
        _ => SetupError::Discord("Discord rejected the setup request".to_string()),
    }
}

impl fmt::Debug for TwilightConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TwilightConfig")
            .field("token", &"[redacted]")
            .field("guild_id", &self.guild_id)
            .finish()
    }
}

pub struct TwilightConnector {
    config: TwilightConfig,
    http: Client,
}

impl fmt::Debug for TwilightConnector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TwilightConnector")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl TwilightConnector {
    pub fn new(config: TwilightConfig) -> Result<Self, GatewayError> {
        validate_bot_token_shape(&config.token)
            .map_err(|error| GatewayError::Configuration(error.to_string()))?;
        let http = Client::new(config.token.clone());
        Ok(Self { config, http })
    }

    pub fn http(&self) -> &Client {
        &self.http
    }
}

#[async_trait]
impl GatewayConnector for TwilightConnector {
    async fn connect(&self) -> Result<Box<dyn GatewaySession>, GatewayError> {
        validate_bot_token_shape(&self.config.token)
            .map_err(|error| GatewayError::Configuration(error.to_string()))?;
        let shard = Shard::new(
            ShardId::ONE,
            self.config.token.clone(),
            Intents::GUILDS | Intents::DIRECT_MESSAGES,
        );
        Ok(Box::new(TwilightSession {
            shard,
            pending: VecDeque::new(),
            ready: false,
            closed: false,
        }))
    }

    async fn register_commands(&self) -> Result<usize, GatewayError> {
        let guild_id = parse_guild_id(&self.config.guild_id)?;
        let command = swap_command(guild_id);
        let application_id = self
            .http
            .current_user_application()
            .await
            .map_err(map_http_error)?
            .model()
            .await
            .map_err(|error| GatewayError::Transport(error.to_string()))?
            .id;
        self.http
            .interaction(application_id)
            .set_guild_commands(guild_id, std::slice::from_ref(&command))
            .await
            .map_err(map_http_error)?;
        Ok(1)
    }
}

struct TwilightSession {
    shard: Shard,
    pending: VecDeque<InboundEvent>,
    ready: bool,
    closed: bool,
}

impl TwilightSession {
    /// Read events until Discord sends READY. Buffer any mapped events.
    async fn wait_until_ready(&mut self) -> Result<(), GatewayError> {
        while !self.ready {
            if self.closed {
                return Err(GatewayError::Transport(
                    "gateway closed before READY".to_string(),
                ));
            }
            self.pump().await?;
        }
        Ok(())
    }

    /// Read one raw gateway event and map it into the pending queue.
    async fn pump(&mut self) -> Result<(), GatewayError> {
        let Some(item) = self.shard.next_event(EVENT_TYPES).await else {
            self.closed = true;
            if self.shard.state() == twilight_gateway::ShardState::FatallyClosed {
                return Err(GatewayError::Configuration(
                    "Discord closed the gateway fatally. Check the token and the intents."
                        .to_string(),
                ));
            }
            return Ok(());
        };
        let event = match item {
            Ok(event) => event,
            Err(error) => return Err(map_receive_error(&error)),
        };
        match event {
            Event::InteractionCreate(interaction) => {
                self.pending.extend(map_interaction(&interaction));
            }
            Event::MessageCreate(message) => {
                if message.guild_id.is_none() && !message.author.bot {
                    let user_id = DiscordUserId::new(message.author.id.get().to_string());
                    let channel_id = DiscordChannelId::new(message.channel_id.get().to_string());
                    let message_id = MessageId::new(message.id.get().to_string());
                    for attachment in &message.attachments {
                        self.pending.push_back(InboundEvent::DmAttachment {
                            user_id: user_id.clone(),
                            channel_id: channel_id.clone(),
                            message_id: message_id.clone(),
                            author_name: message
                                .author
                                .global_name
                                .clone()
                                .or_else(|| Some(message.author.name.clone())),
                            attachment: DmAttachment {
                                id: attachment.id.get().to_string(),
                                filename: attachment.filename.clone(),
                                size: attachment.size,
                                url: attachment.url.clone(),
                            },
                        });
                    }
                }
            }
            Event::Ready(_) => self.ready = true,
            Event::GatewayClose(Some(frame)) => {
                if let Some(error) = fatal_close_error(frame.code) {
                    return Err(error);
                }
                self.closed = true;
                if self.shard.state() == twilight_gateway::ShardState::FatallyClosed {
                    return Err(GatewayError::Configuration(
                        "Discord closed the gateway fatally. Check the token and the intents."
                            .to_string(),
                    ));
                }
            }
            Event::GatewayClose(None) => {
                self.closed = true;
                if self.shard.state() == twilight_gateway::ShardState::FatallyClosed {
                    return Err(GatewayError::Configuration(
                        "Discord closed the gateway fatally. Check the token and the intents."
                            .to_string(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[async_trait]
impl GatewaySession for TwilightSession {
    async fn wait_ready(&mut self) -> Result<(), GatewayError> {
        self.wait_until_ready().await
    }

    async fn next_event(&mut self) -> Result<Option<InboundEvent>, GatewayError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }
            if self.closed {
                return Ok(None);
            }
            self.pump().await?;
        }
    }

    async fn close(&mut self) -> Result<(), GatewayError> {
        if self.closed {
            return Ok(());
        }
        self.shard.close(twilight_gateway::CloseFrame {
            code: 1000,
            reason: "host shutdown".into(),
        });
        let closed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match self.shard.next_event(EventTypeFlags::empty()).await {
                    None | Some(Ok(Event::GatewayClose(_))) => break,
                    Some(Ok(_)) => {
                        if self.shard.state() != twilight_gateway::ShardState::Active {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                }
            }
        })
        .await;
        self.closed = true;
        closed.map_err(|_| GatewayError::Transport("gateway close timed out".to_string()))?;
        Ok(())
    }
}

fn map_interaction(
    interaction: &twilight_model::application::interaction::Interaction,
) -> Vec<InboundEvent> {
    let Some(user_id) = interaction
        .author_id()
        .map(|id| DiscordUserId::new(id.get().to_string()))
    else {
        return Vec::new();
    };
    let interaction_ref =
        InteractionRef::new(interaction.id.get().to_string(), interaction.token.clone());
    let guild_id = interaction
        .guild_id
        .map(|id| DiscordGuildId::new(id.get().to_string()));

    match interaction.data.as_ref() {
        Some(InteractionData::ApplicationCommand(data)) => {
            if interaction.kind
                != twilight_model::application::interaction::InteractionType::ApplicationCommand
                || data.name != SWAP_COMMAND
            {
                return Vec::new();
            }
            let Some(first) = data.options.first() else {
                return Vec::new();
            };
            let Some(command) = parse_swap_subcommand(&first.name) else {
                return Vec::new();
            };
            let options = match &first.value {
                CommandOptionValue::SubCommand(options) => flatten_options(options),
                _ => BTreeMap::new(),
            };
            vec![InboundEvent::SlashCommand {
                interaction: interaction_ref,
                user_id,
                guild_id,
                command,
                options,
            }]
        }
        Some(InteractionData::MessageComponent(data)) => {
            let Ok(custom_id) = CustomId::parse(&data.custom_id) else {
                return Vec::new();
            };
            vec![InboundEvent::Button {
                interaction: interaction_ref,
                user_id,
                guild_id,
                custom_id,
            }]
        }
        Some(InteractionData::ModalSubmit(data)) => {
            let Ok(custom_id) = CustomId::parse(&data.custom_id) else {
                return Vec::new();
            };
            vec![InboundEvent::ModalSubmit {
                interaction: interaction_ref,
                user_id,
                guild_id,
                custom_id,
                fields: modal_fields(data),
            }]
        }
        _ => Vec::new(),
    }
}

fn flatten_options(options: &[CommandDataOption]) -> BTreeMap<String, String> {
    options
        .iter()
        .filter_map(|option| option_value(&option.value).map(|value| (option.name.clone(), value)))
        .collect()
}

fn option_value(value: &CommandOptionValue) -> Option<String> {
    Some(match value {
        CommandOptionValue::Attachment(id) => id.get().to_string(),
        CommandOptionValue::Channel(id) => id.get().to_string(),
        CommandOptionValue::Mentionable(id) => id.get().to_string(),
        CommandOptionValue::Role(id) => id.get().to_string(),
        CommandOptionValue::User(id) => id.get().to_string(),
        CommandOptionValue::Boolean(value) => value.to_string(),
        CommandOptionValue::Focused(value, _) | CommandOptionValue::String(value) => value.clone(),
        CommandOptionValue::Integer(value) => value.to_string(),
        CommandOptionValue::Number(value) => value.to_string(),
        CommandOptionValue::SubCommand(options) | CommandOptionValue::SubCommandGroup(options) => {
            serde_json::to_string(&flatten_options(options)).ok()?
        }
    })
}

fn modal_fields(data: &ModalInteractionData) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for component in &data.components {
        collect_modal_component(component, &mut fields);
    }
    fields
}

fn collect_modal_component(
    component: &ModalInteractionComponent,
    fields: &mut BTreeMap<String, String>,
) {
    match component {
        ModalInteractionComponent::TextInput(input) => {
            fields.insert(input.custom_id.clone(), input.value.clone());
        }
        ModalInteractionComponent::ActionRow(row) => {
            for child in &row.components {
                collect_modal_component(child, fields);
            }
        }
        ModalInteractionComponent::Label(label) => {
            collect_modal_component(&label.component, fields);
        }
        _ => {}
    }
}

fn parse_guild_id(guild_id: &DiscordGuildId) -> Result<Id<GuildMarker>, GatewayError> {
    guild_id
        .0
        .parse::<u64>()
        .map(Id::new)
        .map_err(|_| GatewayError::Configuration("guild id is not a snowflake".to_string()))
}

#[allow(deprecated)]
fn swap_command(guild_id: Id<GuildMarker>) -> Command {
    let options = crate::SWAP_SUBCOMMANDS
        .iter()
        .map(|(name, description)| CommandOption {
            autocomplete: None,
            channel_types: None,
            choices: None,
            description: (*description).to_string(),
            description_localizations: None,
            file_types: None,
            kind: CommandOptionType::SubCommand,
            max_length: None,
            max_value: None,
            min_length: None,
            min_value: None,
            name: (*name).to_string(),
            name_localizations: None,
            options: subcommand_options(name),
            required: None,
        })
        .collect();
    Command {
        application_id: None,
        contexts: None,
        default_member_permissions: None,
        description: "Run a Monsquaz Swap participant or host action.".to_string(),
        description_localizations: None,
        dm_permission: Some(false),
        guild_id: Some(guild_id),
        id: None,
        integration_types: None,
        kind: CommandType::ChatInput,
        name: SWAP_COMMAND.to_string(),
        name_localizations: None,
        nsfw: None,
        options,
        version: Id::new(1),
    }
}

fn subcommand_options(name: &str) -> Option<Vec<CommandOption>> {
    let option = |name: &str, description: &str, kind, required: bool| CommandOption {
        autocomplete: None,
        channel_types: None,
        choices: None,
        description: description.to_string(),
        description_localizations: None,
        file_types: None,
        kind,
        max_length: None,
        max_value: None,
        min_length: None,
        min_value: None,
        name: name.to_string(),
        name_localizations: None,
        options: None,
        required: Some(required),
    };
    let options = match name {
        "join" => vec![
            option(
                "timezone",
                "Your IANA timezone, such as Europe/Stockholm.",
                CommandOptionType::String,
                false,
            ),
            option(
                "max_handoffs",
                "The maximum number of tracks you will edit.",
                CommandOptionType::Integer,
                false,
            ),
        ],
        "title" => vec![option(
            "title",
            "The anonymous title for the track you hold.",
            CommandOptionType::String,
            true,
        )],
        "extend" => vec![option(
            "hours",
            "How many extra hours you need, 1 to 12.",
            CommandOptionType::Integer,
            false,
        )],
        "host-lock" => vec![option(
            "scope",
            "Use roster or schedule.",
            CommandOptionType::String,
            true,
        )],
        "host-reveal" => vec![option(
            "confirm",
            "Set true to reveal the event.",
            CommandOptionType::Boolean,
            false,
        )],
        "claim" | "skip" => vec![option(
            "token",
            "Optional offer token.",
            CommandOptionType::String,
            false,
        )],
        _ => Vec::new(),
    };
    (!options.is_empty()).then_some(options)
}

fn fatal_close_error(code: u16) -> Option<GatewayError> {
    match code {
        4004 => Some(GatewayError::Authentication),
        4010..=4014 => Some(GatewayError::Configuration(format!(
            "Discord refused the gateway intents (close code {code})"
        ))),
        _ => None,
    }
}

fn map_receive_error(error: &twilight_gateway::error::ReceiveMessageError) -> GatewayError {
    use twilight_gateway::error::ReceiveMessageErrorType;
    match error.kind() {
        ReceiveMessageErrorType::Deserializing { .. } => {
            GatewayError::Protocol("Discord sent an unreadable gateway event.".to_string())
        }
        ReceiveMessageErrorType::Compression => {
            GatewayError::Transport("Gateway compression failed.".to_string())
        }
        ReceiveMessageErrorType::Reconnect => {
            GatewayError::Transport("Gateway reconnect.".to_string())
        }
        _ => GatewayError::Protocol("Discord sent an unsupported gateway event.".to_string()),
    }
}

fn map_http_error(error: twilight_http::Error) -> GatewayError {
    match error.kind() {
        ErrorType::Unauthorized => GatewayError::Authentication,
        ErrorType::Response { status, .. }
            if *status == twilight_http::response::StatusCode::FORBIDDEN
                || *status == twilight_http::response::StatusCode::NOT_FOUND =>
        {
            GatewayError::Configuration(
                "Discord refused the command registration. Check the bot scope, guild, and permissions."
                    .to_string(),
            )
        }
        _ => GatewayError::Transport(error.to_string()),
    }
}

/// Token validator for the setup wizard. It calls `GET /users/@me`.
pub struct TwilightTokenValidator;

impl TwilightTokenValidator {
    pub fn new(token: &str) -> Result<Self, SetupError> {
        validate_bot_token_shape(token)?;
        Ok(Self)
    }
}

#[async_trait]
impl TokenValidator for TwilightTokenValidator {
    async fn validate(&self, token: &str) -> Result<BotIdentity, SetupError> {
        validate_bot_token_shape(token)?;
        let token = token.trim();
        let http = Client::new(token.to_string());
        let response = http
            .current_user()
            .await
            .map_err(|error| SetupError::Validation(error.to_string()))?;
        let user = response
            .model()
            .await
            .map_err(|error| SetupError::Validation(error.to_string()))?;
        Ok(BotIdentity {
            id: user.id.get().to_string(),
            username: user.name,
        })
    }
}

/// Convert a guild command count to the command set used by the supervisor.
pub fn subcommand_count() -> usize {
    crate::SWAP_SUBCOMMANDS.len()
}

/// Expose the live connector as a trait object for the supervisor.
pub fn connector(config: TwilightConfig) -> Result<Arc<dyn GatewayConnector>, GatewayError> {
    Ok(Arc::new(TwilightConnector::new(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SlashCommand;

    #[test]
    fn the_token_is_redacted_in_debug_output() {
        let config = TwilightConfig::new(
            "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc",
            DiscordGuildId::new("123"),
        );
        let debug = format!("{config:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("aaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    #[test]
    fn fatal_close_codes_are_classified() {
        assert_eq!(fatal_close_error(4004), Some(GatewayError::Authentication));
        for code in [4010, 4011, 4012, 4013, 4014] {
            assert!(matches!(
                fatal_close_error(code),
                Some(GatewayError::Configuration(_))
            ));
        }
        assert!(fatal_close_error(4000).is_none());
    }

    #[test]
    fn a_padded_token_is_normalized() {
        let config = TwilightConfig::new(
            "  aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc\n",
            DiscordGuildId::new("123"),
        );
        assert_eq!(
            config.token(),
            "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc"
        );
    }

    #[test]
    fn a_bad_token_is_refused_before_connect() {
        let config = TwilightConfig::new("short", DiscordGuildId::new("123"));
        assert!(matches!(
            TwilightConnector::new(config),
            Err(GatewayError::Configuration(_))
        ));
    }

    #[test]
    fn the_command_has_every_subcommand() {
        let command = swap_command(Id::new(42));
        assert_eq!(command.name, SWAP_COMMAND);
        assert_eq!(command.options.len(), subcommand_count());
        assert_eq!(command.guild_id, Some(Id::new(42)));
        assert!(
            command
                .options
                .iter()
                .all(|option| option.kind == CommandOptionType::SubCommand)
        );
        let title = command
            .options
            .iter()
            .find(|option| option.name == "title")
            .unwrap();
        let title_option = &title.options.as_ref().unwrap()[0];
        assert_eq!(title_option.kind, CommandOptionType::String);
        assert_eq!(title_option.required, Some(true));
    }

    #[test]
    fn option_values_are_flattened() {
        let options = vec![CommandDataOption {
            name: "reason".to_string(),
            value: CommandOptionValue::String("late".to_string()),
        }];
        let fields = flatten_options(&options);
        assert_eq!(fields.get("reason").map(String::as_str), Some("late"));
    }

    #[test]
    fn a_slash_command_maps_to_the_contract() {
        let options = [CommandDataOption {
            name: "claim".to_string(),
            value: CommandOptionValue::SubCommand(vec![CommandDataOption {
                name: "token".to_string(),
                value: CommandOptionValue::String("a1".to_string()),
            }]),
        }];
        assert_eq!(parse_swap_subcommand("claim"), Some(SlashCommand::Claim));
        let nested = match &options[0].value {
            CommandOptionValue::SubCommand(options) => options,
            _ => panic!("expected a subcommand"),
        };
        let fields = flatten_options(nested);
        assert_eq!(fields.get("token").map(String::as_str), Some("a1"));
    }
}
