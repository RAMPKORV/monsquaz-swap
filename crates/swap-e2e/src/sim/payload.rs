//! Discord JSON payload builders.
//!
//! Twilight 0.18 rejects payloads that omit required fields. These builders
//! keep one shape in one place.

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{Value, json};

use super::state::{SimAttachment, SimChannel, SimMessage, SimState, SimUser};

pub fn user_json(user: &SimUser) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "discriminator": "0001",
        "global_name": user.username,
        "avatar": null,
        "bot": user.bot,
        "system": false,
        "mfa_enabled": false,
        "banner": null,
        "accent_color": null,
        "locale": "en-US",
        "verified": true,
        "email": null,
        "flags": 0,
        "premium_type": 0,
        "public_flags": 0
    })
}

pub fn dm_channel_json(channel: &SimChannel, state: &SimState) -> Value {
    let recipients = channel
        .recipients
        .iter()
        .filter_map(|id| state.users.get(id))
        .map(user_json)
        .collect::<Vec<_>>();
    json!({
        "id": channel.id,
        "type": channel.kind,
        "recipients": recipients,
        "last_message_id": null,
        "flags": 0
    })
}

pub fn guild_channel_json(channel: &SimChannel) -> Value {
    json!({
        "id": channel.id,
        "type": channel.kind,
        "name": channel.name,
        "guild_id": channel.guild_id,
        "parent_id": channel.parent_id,
        "position": 0,
        "permission_overwrites": [],
        "nsfw": false,
        "topic": null,
        "last_message_id": null
    })
}

pub fn message_json(message: &SimMessage, state: &SimState, base_url: &str) -> Value {
    let author = state
        .users
        .get(&message.author_id)
        .map(user_json)
        .unwrap_or_else(|| {
            json!({
                "id": message.author_id,
                "username": "unknown",
                "discriminator": "0001"
            })
        });
    let guild_id = state
        .channels
        .get(&message.channel_id)
        .and_then(|channel| channel.guild_id.clone());
    json!({
        "id": message.id,
        "channel_id": message.channel_id,
        "guild_id": guild_id,
        "author": author,
        "content": message.content,
        "timestamp": timestamp(message.timestamp),
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mentions": [],
        "mention_roles": [],
        "attachments": message.attachments.iter().map(|attachment| attachment_json(attachment, base_url)).collect::<Vec<_>>(),
        "embeds": [],
        "pinned": false,
        "type": 0,
        "flags": 0,
        "components": message.components
    })
}

pub fn attachment_json(attachment: &SimAttachment, base_url: &str) -> Value {
    let url = format!(
        "{base_url}/cdn/{}/{}",
        attachment.id,
        encode_path(&attachment.filename)
    );
    json!({
        "id": attachment.id,
        "filename": attachment.filename,
        "size": 0,
        "url": url,
        "proxy_url": url,
        "content_type": attachment.content_type,
    })
}

pub fn application_json(application_id: &str, bot: &Value) -> Value {
    json!({
        "id": application_id,
        "name": "Monsquaz Swap Test",
        "description": "Local simulator application",
        "bot_public": false,
        "bot_require_code_grant": false,
        "verify_key": "simulator",
        "flags": 0,
        "owner": bot,
        "rpc_origins": []
    })
}

pub fn guild_json(guild_id: &str, name: &str) -> Value {
    json!({
        "id": guild_id,
        "name": name,
        "icon": null,
        "owner": false,
        "permissions": "2199023255551",
        "features": []
    })
}

pub fn ready_json(
    state: &SimState,
    config: &super::state::SimConfig,
    resume_gateway_url: &str,
) -> Value {
    let bot = state
        .users
        .get(&config.bot_user_id)
        .map(user_json)
        .unwrap_or(Value::Null);
    json!({
        "v": 10,
        "application": {
            "id": config.application_id,
            "flags": 0
        },
        "guilds": [{
            "id": config.guild_id,
            "unavailable": true
        }],
        "resume_gateway_url": resume_gateway_url,
        "session_id": "simulator-session",
        "shard": [0, 1],
        "user": bot
    })
}

/// One participant action that becomes an `INTERACTION_CREATE` event.
#[derive(Debug, Clone)]
pub enum SimInteraction {
    Command {
        name: String,
        options: Vec<SimOption>,
    },
    Button {
        custom_id: String,
    },
    Modal {
        custom_id: String,
        fields: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone)]
pub struct SimOption {
    pub name: String,
    pub value: Value,
}

/// The envelope fields that every interaction carries.
pub struct InteractionContext<'a> {
    pub id: &'a str,
    pub kind: u64,
    pub user_id: &'a str,
    pub guild_id: Option<&'a str>,
    pub channel_id: &'a str,
    pub application_id: &'a str,
}

pub fn interaction_json(
    context: &InteractionContext<'_>,
    state: &SimState,
    interaction: &SimInteraction,
) -> Value {
    let InteractionContext {
        id,
        kind,
        user_id,
        guild_id,
        channel_id,
        application_id,
    } = *context;
    let user = state
        .users
        .get(user_id)
        .map(user_json)
        .unwrap_or(Value::Null);
    let data = match interaction {
        SimInteraction::Command { name, options } => json!({
            "id": "900000000000000099",
            "name": super::state::SWAP_COMMAND_NAME,
            "type": 1,
            "options": [{
                "name": name,
                "type": 1,
                "options": options.iter().map(option_json).collect::<Vec<_>>()
            }]
        }),
        SimInteraction::Button { custom_id } => json!({
            "custom_id": custom_id,
            "component_type": 2,
            "values": []
        }),
        SimInteraction::Modal { custom_id, fields } => json!({
            "custom_id": custom_id,
            "components": fields.iter().enumerate().map(|(index, (name, value))| json!({
                "type": 1,
                "id": index as i32,
                "components": [{
                    "type": 4,
                    "id": 100 + index as i32,
                    "custom_id": name,
                    "value": value
                }]
            })).collect::<Vec<_>>()
        }),
    };
    json!({
        "id": id,
        "application_id": application_id,
        "type": kind,
        "token": format!("sim-token-{id}"),
        "attachment_size_limit": 25 * 1024 * 1024,
        "authorizing_integration_owners": match guild_id {
            Some(guild) => json!({ "0": guild }),
            None => json!({ "1": user_id }),
        },
        "entitlements": [],
        "data": data,
        "channel_id": channel_id,
        "guild_id": guild_id,
        "guild_locale": "en-US",
        "locale": "en-US",
        "member": guild_id.map(|_| json!({
            "deaf": false,
            "mute": false,
            "flags": 0,
            "joined_at": "2026-01-01T00:00:00.000000+00:00",
            "roles": [],
            "user": user.clone()
        })),
        "user": if guild_id.is_none() { user } else { Value::Null },
    })
}

pub fn message_create_event(message: &SimMessage, state: &SimState, base_url: &str) -> Value {
    let channel = state.channels.get(&message.channel_id);
    let mut payload = message_json(message, state, base_url);
    if let Some(channel) = channel
        && let Some(object) = payload.as_object_mut()
    {
        object.insert("channel_type".to_string(), json!(channel.kind));
    }
    payload
}

fn option_json(option: &SimOption) -> Value {
    let kind = match &option.value {
        Value::Bool(_) => 5,
        Value::Number(number) if number.is_i64() || number.is_u64() => 4,
        _ => 3,
    };
    json!({
        "name": option.name,
        "type": kind,
        "value": option.value
    })
}

fn timestamp(epoch_seconds: i64) -> String {
    Utc.timestamp_opt(epoch_seconds, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn encode_path(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{SimConfig, SimMessage, SimState};

    fn state() -> (SimConfig, SimState) {
        let config = SimConfig::default();
        let state = SimState::new(&config);
        (config, state)
    }

    fn interaction(
        id: &str,
        kind: u64,
        guild_id: Option<&str>,
        channel_id: &str,
        config: &SimConfig,
        state: &SimState,
        value: &SimInteraction,
    ) -> Value {
        interaction_json(
            &InteractionContext {
                id,
                kind,
                user_id: super::super::state::PARTICIPANTS[0].1,
                guild_id,
                channel_id,
                application_id: &config.application_id,
            },
            state,
            value,
        )
    }

    #[test]
    fn ready_deserializes_as_twilight_ready() {
        let (config, state) = state();
        let value = ready_json(&state, &config, "ws://127.0.0.1:9/");
        let raw = serde_json::to_string(&value).unwrap();
        let parsed: twilight_model::gateway::payload::incoming::Ready =
            serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.session_id, "simulator-session");
        assert_eq!(parsed.user.id.get().to_string(), config.bot_user_id);
    }

    #[test]
    fn a_dm_message_deserializes_as_twilight_message_create() {
        let (_, state) = state();
        let message = SimMessage {
            id: "930000000000000001".to_string(),
            channel_id: "920000000000000001".to_string(),
            author_id: super::super::state::PARTICIPANTS[0].1.to_string(),
            content: "hello".to_string(),
            attachments: vec![SimAttachment {
                id: "930000000000000002".to_string(),
                filename: "seed.mod".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            }],
            timestamp: 1_760_000_000,
            components: Vec::new(),
        };
        let event = message_create_event(&message, &state, "http://127.0.0.1:9");
        let raw = serde_json::to_string(&event).unwrap();
        let parsed: twilight_model::gateway::payload::incoming::MessageCreate =
            serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.content, "hello");
        assert_eq!(parsed.attachments[0].filename, "seed.mod");
        assert!(parsed.guild_id.is_none());
    }

    #[test]
    fn the_application_deserializes_as_twilight_application() {
        let (config, state) = state();
        let bot = user_json(state.users.get(&config.bot_user_id).unwrap());
        let value = application_json(&config.application_id, &bot);
        let raw = serde_json::to_string(&value).unwrap();
        let parsed: twilight_model::oauth::Application = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.id.get().to_string(), config.application_id);
        assert_eq!(parsed.name, "Monsquaz Swap Test");
    }

    #[test]
    fn a_command_interaction_deserializes_as_twilight_interaction() {
        let (config, state) = state();
        let value = interaction(
            "930000000000000005",
            2,
            Some(&config.guild_id),
            super::super::state::GENERAL_CHANNEL_ID,
            &config,
            &state,
            &SimInteraction::Command {
                name: "join".to_string(),
                options: vec![SimOption {
                    name: "timezone".to_string(),
                    value: json!("Europe/Stockholm"),
                }],
            },
        );
        let raw = serde_json::to_string(&value).unwrap();
        let parsed: twilight_model::application::interaction::Interaction =
            serde_json::from_str(&raw).unwrap();
        assert!(matches!(
            parsed.data,
            Some(twilight_model::application::interaction::InteractionData::ApplicationCommand(_))
        ));
        assert!(parsed.author_id().is_some());
    }

    #[test]
    fn a_modal_interaction_deserializes_as_twilight_interaction() {
        let (config, state) = state();
        let value = interaction(
            "930000000000000004",
            5,
            None,
            "920000000000000001",
            &config,
            &state,
            &SimInteraction::Modal {
                custom_id: "swap:evt:title:tok".to_string(),
                fields: vec![("title".to_string(), "A new title".to_string())],
            },
        );
        let raw = serde_json::to_string(&value).unwrap();
        let parsed: twilight_model::application::interaction::Interaction =
            serde_json::from_str(&raw).unwrap();
        assert!(matches!(
            parsed.data,
            Some(twilight_model::application::interaction::InteractionData::ModalSubmit(_))
        ));
    }

    #[test]
    fn a_button_interaction_deserializes_as_twilight_interaction() {
        let (config, state) = state();
        let value = interaction(
            "930000000000000003",
            3,
            None,
            "920000000000000001",
            &config,
            &state,
            &SimInteraction::Button {
                custom_id: "swap:evt:claim:tok".to_string(),
            },
        );
        let raw = serde_json::to_string(&value).unwrap();
        let parsed: twilight_model::application::interaction::Interaction =
            serde_json::from_str(&raw).unwrap();
        assert!(matches!(
            parsed.data,
            Some(twilight_model::application::interaction::InteractionData::MessageComponent(_))
        ));
    }
}
