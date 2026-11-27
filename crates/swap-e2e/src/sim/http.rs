//! Discord REST surface and the test control API.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::multipart::Multipart;
use axum::extract::{FromRequest, Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use super::payload::{self, SimInteraction, SimOption};
use super::state::{GUILD_ID, PARTICIPANTS, SimAttachment, SimChannel, SimMessage, SimShared};

const MAX_BODY: usize = 25 * 1024 * 1024;
const MESSAGE_EPOCH: i64 = 1_760_000_000;

pub(crate) fn router(shared: Arc<SimShared>) -> Router {
    Router::new()
        .route("/api/v10/users/@me", get(get_me))
        .route("/api/v10/users/@me/guilds", get(get_my_guilds))
        .route("/api/v10/users/@me/channels", post(create_dm))
        .route("/api/v10/guilds/{guild_id}", get(get_guild))
        .route(
            "/api/v10/guilds/{guild_id}/channels",
            get(get_guild_channels),
        )
        .route("/api/v10/applications/@me", get(get_application))
        .route(
            "/api/v10/applications/{application_id}/guilds/{guild_id}/commands",
            put(set_guild_commands),
        )
        .route(
            "/api/v10/channels/{channel_id}/messages",
            post(create_message).get(list_messages),
        )
        .route(
            "/api/v10/channels/{channel_id}/messages/{message_id}",
            patch(edit_message),
        )
        .route(
            "/api/v10/channels/{channel_id}/threads",
            post(create_thread),
        )
        .route(
            "/api/v10/channels/{channel_id}/thread-members/{user_id}",
            put(add_thread_member),
        )
        .route("/cdn/{attachment_id}/{filename}", get(download_attachment))
        .route("/_control/messages", post(control_message))
        .route("/_control/messages", get(control_messages))
        .route("/_control/interactions", post(control_interaction))
        .route("/_control/reset", post(control_reset))
        .route("/_control/state", get(control_state))
        .route("/_control/ready", get(control_ready))
        .with_state(shared)
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: u64,
    message: String,
}

impl ApiError {
    fn unknown_channel() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: 10003,
            message: "Unknown Channel".to_string(),
        }
    }

    fn unknown_user() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: 10013,
            message: "Unknown User".to_string(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: 50035,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "message": self.message, "code": self.code })),
        )
            .into_response()
    }
}

async fn get_me(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    let state = shared.state();
    let user = state
        .users
        .get(&shared.config.bot_user_id)
        .map(payload::user_json)
        .unwrap_or(Value::Null);
    Json(user)
}

async fn get_my_guilds(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    Json(json!([payload::guild_json(
        &shared.config.guild_id,
        &shared.config.guild_name
    )]))
}

async fn get_guild(
    State(shared): State<Arc<SimShared>>,
    Path(guild_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if guild_id != shared.config.guild_id {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: 10004,
            message: "Unknown Guild".to_string(),
        });
    }
    Ok(Json(payload::guild_json(
        &shared.config.guild_id,
        &shared.config.guild_name,
    )))
}

async fn get_guild_channels(
    State(shared): State<Arc<SimShared>>,
    Path(guild_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if guild_id != shared.config.guild_id {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: 10004,
            message: "Unknown Guild".to_string(),
        });
    }
    let state = shared.state();
    let channels = state
        .channels
        .values()
        .filter(|channel| channel.guild_id.as_deref() == Some(guild_id.as_str()))
        .map(payload::guild_channel_json)
        .collect::<Vec<_>>();
    Ok(Json(Value::Array(channels)))
}

async fn get_application(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    let state = shared.state();
    let bot = state
        .users
        .get(&shared.config.bot_user_id)
        .map(payload::user_json)
        .unwrap_or(Value::Null);
    Json(payload::application_json(
        &shared.config.application_id,
        &bot,
    ))
}

#[derive(Deserialize)]
struct DmRequest {
    recipient_id: String,
}

async fn create_dm(
    State(shared): State<Arc<SimShared>>,
    Json(request): Json<DmRequest>,
) -> Result<Json<Value>, ApiError> {
    let channel = ensure_dm_channel(&shared, &request.recipient_id)?;
    let state = shared.state();
    Ok(Json(payload::dm_channel_json(&channel, &state)))
}

async fn create_message(
    State(shared): State<Arc<SimShared>>,
    Path(channel_id): Path<String>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let mut content = String::new();
    let mut files = Vec::new();
    if content_type.starts_with("multipart/form-data") {
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| ApiError::bad_request(error.to_string()))?
        {
            let name = field.name().unwrap_or_default().to_string();
            if name == "payload_json" {
                let text = field
                    .text()
                    .await
                    .map_err(|error| ApiError::bad_request(error.to_string()))?;
                let payload: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                content = payload
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
            } else if name.starts_with("files[") {
                let filename = field.file_name().unwrap_or("file.bin").to_string();
                let content_type = field.content_type().map(str::to_string);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| ApiError::bad_request(error.to_string()))?;
                files.push((content_type, filename, bytes.to_vec()));
            }
        }
    } else {
        let bytes = axum::body::to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        let payload: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        content = payload
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }

    let message = store_message(
        &shared,
        &channel_id,
        &shared.config.bot_user_id,
        content,
        files,
    )?;
    let state = shared.state();
    Ok(Json(payload::message_json(
        &message,
        &state,
        &shared.base_url,
    )))
}

#[derive(Deserialize)]
struct MessageQuery {
    after: Option<String>,
    limit: Option<usize>,
}

async fn list_messages(
    State(shared): State<Arc<SimShared>>,
    Path(channel_id): Path<String>,
    Query(query): Query<MessageQuery>,
) -> Result<Json<Value>, ApiError> {
    let state = shared.state();
    if !state.channels.contains_key(&channel_id) {
        return Err(ApiError::unknown_channel());
    }
    let after = query
        .after
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let limit = query.limit.unwrap_or(50).min(100);
    let messages = state
        .messages
        .get(&channel_id)
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message
                        .id
                        .parse::<u64>()
                        .map(|id| id > after)
                        .unwrap_or(true)
                })
                .take(limit)
                .map(|message| payload::message_json(message, &state, &shared.base_url))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Json(Value::Array(messages)))
}

#[derive(Deserialize)]
struct EditMessage {
    content: Option<String>,
}

async fn edit_message(
    State(shared): State<Arc<SimShared>>,
    Path((channel_id, message_id)): Path<(String, String)>,
    Json(request): Json<EditMessage>,
) -> Result<Json<Value>, ApiError> {
    let mut state = shared.state_mut();
    let message = state
        .messages
        .get_mut(&channel_id)
        .and_then(|messages| messages.iter_mut().find(|message| message.id == message_id))
        .ok_or_else(ApiError::unknown_channel)?;
    if let Some(content) = request.content {
        message.content = content;
    }
    let message = message.clone();
    Ok(Json(payload::message_json(
        &message,
        &state,
        &shared.base_url,
    )))
}

#[derive(Deserialize)]
struct ThreadRequest {
    name: String,
    #[serde(rename = "type")]
    kind: Option<u64>,
}

async fn create_thread(
    State(shared): State<Arc<SimShared>>,
    Path(channel_id): Path<String>,
    Json(request): Json<ThreadRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut state = shared.state_mut();
    let parent = state
        .channels
        .get(&channel_id)
        .cloned()
        .ok_or_else(ApiError::unknown_channel)?;
    let id = shared.new_id();
    let thread = SimChannel {
        id: id.clone(),
        name: request.name,
        kind: request.kind.unwrap_or(12),
        guild_id: parent.guild_id,
        parent_id: Some(parent.id),
        recipients: Vec::new(),
        thread_members: vec![shared.config.bot_user_id.clone()],
    };
    state.channels.insert(id, thread.clone());
    Ok(Json(payload::guild_channel_json(&thread)))
}

async fn add_thread_member(
    State(shared): State<Arc<SimShared>>,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let mut state = shared.state_mut();
    let channel = state
        .channels
        .get_mut(&channel_id)
        .ok_or_else(ApiError::unknown_channel)?;
    if !channel.thread_members.contains(&user_id) {
        channel.thread_members.push(user_id);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn set_guild_commands(
    State(shared): State<Arc<SimShared>>,
    Path((_application_id, guild_id)): Path<(String, String)>,
    Json(commands): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if guild_id != shared.config.guild_id {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            code: 10004,
            message: "Unknown Guild".to_string(),
        });
    }
    let commands = commands.as_array().cloned().unwrap_or_default();
    shared.state_mut().commands = commands.clone();
    shared.mark_commands_registered();
    Ok(Json(Value::Array(commands)))
}

async fn download_attachment(
    State(shared): State<Arc<SimShared>>,
    Path((attachment_id, _filename)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let state = shared.state();
    let bytes = state
        .attachment_bytes
        .get(&attachment_id)
        .cloned()
        .ok_or_else(ApiError::unknown_channel)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Bytes::from((*bytes).clone()),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct InjectMessageRequest {
    pub user_id: String,
    #[serde(default)]
    pub content: String,
    pub filename: Option<String>,
    pub bytes_base64: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InjectInteractionRequest {
    pub kind: String,
    pub user_id: String,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
    pub custom_id: Option<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

async fn control_message(
    State(shared): State<Arc<SimShared>>,
    Json(request): Json<InjectMessageRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(inject_message(&shared, request)?))
}

async fn control_messages(
    State(shared): State<Arc<SimShared>>,
    Query(query): Query<MessageQueryByUser>,
) -> Json<Value> {
    Json(Value::Array(messages_for(
        &shared,
        query.user_id.as_deref(),
    )))
}

#[derive(Deserialize)]
struct MessageQueryByUser {
    user_id: Option<String>,
}

async fn control_interaction(
    State(shared): State<Arc<SimShared>>,
    Json(request): Json<InjectInteractionRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(inject_interaction(&shared, request)?))
}

async fn control_reset(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    reset(&shared);
    Json(json!({ "ok": true }))
}

async fn control_state(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    let state = shared.state();
    Json(json!({
        "channels": state.channels.len(),
        "messages": state.messages.values().map(Vec::len).sum::<usize>(),
        "commands": state.commands.len(),
        "ready": shared.ready.load(std::sync::atomic::Ordering::SeqCst),
        "commands_registered": shared
            .commands_registered
            .load(std::sync::atomic::Ordering::SeqCst),
    }))
}

async fn control_ready(State(shared): State<Arc<SimShared>>) -> Json<Value> {
    Json(json!({
        "ready": shared.ready.load(std::sync::atomic::Ordering::SeqCst),
        "commands_registered": shared
            .commands_registered
            .load(std::sync::atomic::Ordering::SeqCst),
    }))
}

pub(crate) fn ensure_dm_channel(
    shared: &Arc<SimShared>,
    user_id: &str,
) -> Result<SimChannel, ApiError> {
    {
        let state = shared.state();
        if !state.users.contains_key(user_id) {
            return Err(ApiError::unknown_user());
        }
        if let Some(channel) = state.dm_channel_for(user_id) {
            return Ok(channel.clone());
        }
    }
    let mut state = shared.state_mut();
    let id = shared.new_id();
    let channel = SimChannel {
        id: id.clone(),
        name: String::new(),
        kind: 1,
        guild_id: None,
        parent_id: None,
        recipients: vec![user_id.to_string()],
        thread_members: Vec::new(),
    };
    state.channels.insert(id.clone(), channel.clone());
    state.dm_by_user.insert(user_id.to_string(), id);
    Ok(channel)
}

fn store_message(
    shared: &Arc<SimShared>,
    channel_id: &str,
    author_id: &str,
    content: String,
    files: Vec<(Option<String>, String, Vec<u8>)>,
) -> Result<SimMessage, ApiError> {
    let mut state = shared.state_mut();
    if !state.channels.contains_key(channel_id) {
        return Err(ApiError::unknown_channel());
    }
    let id = shared.new_id();
    let mut attachments = Vec::new();
    for (content_type, filename, bytes) in files {
        let attachment_id = shared.new_id();
        state
            .attachment_bytes
            .insert(attachment_id.clone(), Arc::new(bytes));
        attachments.push(SimAttachment {
            id: attachment_id,
            filename,
            content_type,
        });
    }
    let message = SimMessage {
        id: id.clone(),
        channel_id: channel_id.to_string(),
        author_id: author_id.to_string(),
        content,
        attachments,
        timestamp: MESSAGE_EPOCH,
    };
    state
        .messages
        .entry(channel_id.to_string())
        .or_default()
        .push(message.clone());
    Ok(message)
}

/// Inject a direct message from a participant and dispatch `MESSAGE_CREATE`.
pub fn inject_message(
    shared: &Arc<SimShared>,
    request: InjectMessageRequest,
) -> Result<Value, ApiError> {
    let channel = ensure_dm_channel(shared, &request.user_id)?;
    let files = match (&request.filename, &request.bytes_base64) {
        (Some(filename), Some(bytes)) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(bytes)
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            vec![(request.content_type.clone(), filename.clone(), bytes)]
        }
        _ => Vec::new(),
    };
    let message = store_message(
        shared,
        &channel.id,
        &request.user_id,
        request.content,
        files,
    )?;
    let payload = {
        let state = shared.state();
        payload::message_create_event(&message, &state, &shared.base_url)
    };
    shared.dispatch("MESSAGE_CREATE", payload);
    Ok(json!({ "message_id": message.id, "channel_id": channel.id }))
}

/// Inject an interaction from a participant and dispatch `INTERACTION_CREATE`.
pub fn inject_interaction(
    shared: &Arc<SimShared>,
    request: InjectInteractionRequest,
) -> Result<Value, ApiError> {
    let channel_id = match request.channel_id.clone() {
        Some(channel) => channel,
        None => {
            let channel = ensure_dm_channel(shared, &request.user_id)?;
            channel.id
        }
    };
    let interaction = match request.kind.as_str() {
        "command" => SimInteraction::Command {
            name: request
                .name
                .clone()
                .ok_or_else(|| ApiError::bad_request("the command name is missing"))?,
            options: request
                .options
                .iter()
                .map(|(name, value)| SimOption {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
        },
        "button" => SimInteraction::Button {
            custom_id: request
                .custom_id
                .clone()
                .ok_or_else(|| ApiError::bad_request("the custom id is missing"))?,
        },
        "modal" => SimInteraction::Modal {
            custom_id: request
                .custom_id
                .clone()
                .ok_or_else(|| ApiError::bad_request("the custom id is missing"))?,
            fields: request
                .fields
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
        },
        other => return Err(ApiError::bad_request(format!("unknown kind: {other}"))),
    };
    let kind = match request.kind.as_str() {
        "command" => 2,
        "button" => 3,
        _ => 5,
    };
    let id = shared.new_id();
    let guild_id = request.guild_id.as_deref();
    let application_id = shared.config.application_id.clone();
    let payload = {
        let state = shared.state();
        payload::interaction_json(
            &payload::InteractionContext {
                id: &id,
                kind,
                user_id: &request.user_id,
                guild_id,
                channel_id: &channel_id,
                application_id: &application_id,
            },
            &state,
            &interaction,
        )
    };
    shared.dispatch("INTERACTION_CREATE", payload);
    Ok(json!({ "interaction_id": id, "channel_id": channel_id }))
}

/// Return the messages in the DM channel for a user, oldest first.
pub fn messages_for(shared: &Arc<SimShared>, user_id: Option<&str>) -> Vec<Value> {
    let state = shared.state();
    let channel_ids = match user_id {
        Some(user) => state
            .dm_channel_for(user)
            .map(|channel| vec![channel.id.clone()]),
        None => Some(state.messages.keys().cloned().collect()),
    };
    let Some(channel_ids) = channel_ids else {
        return Vec::new();
    };
    channel_ids
        .iter()
        .filter_map(|channel_id| state.messages.get(channel_id))
        .flatten()
        .map(|message| payload::message_json(message, &state, &shared.base_url))
        .collect()
}

pub fn reset(shared: &Arc<SimShared>) {
    let mut state = shared.state_mut();
    state.messages.clear();
    state.attachment_bytes.clear();
}

pub fn command_count(shared: &Arc<SimShared>) -> usize {
    shared.state().commands.len()
}

pub fn channel_for(shared: &Arc<SimShared>, name: &str) -> Option<String> {
    shared
        .state()
        .channels
        .values()
        .find(|channel| channel.name == name && channel.guild_id.is_some())
        .map(|channel| channel.id.clone())
}

pub fn participant_id(name: &str) -> Option<&'static str> {
    PARTICIPANTS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, id)| *id)
}

pub fn default_guild_id() -> &'static str {
    GUILD_ID
}
