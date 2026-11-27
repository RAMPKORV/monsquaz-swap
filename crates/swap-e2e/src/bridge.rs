//! The browser E2E bridge.
//!
//! The bridge serves the built React UI and a JSON form of the host commands.
//! It runs the same engine, database, files, clock, and Discord simulator as
//! the headless journeys. The browser injects a small `invoke` shim, so the UI
//! code runs unchanged.

#![cfg(feature = "bridge")]

use std::fs;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use monsquaz_swap_lib::e2e_api::{E2eDefaults, E2eHost, mission_control_json};
use serde::Deserialize;
use serde_json::{Value, json};
use swap_discord::{SetupDirectory, TwilightSetup, invite_url};
use tower_http::services::{ServeDir, ServeFile};

use crate::app::HeadlessApp;
use crate::fixtures::{mod_track, pack_zip};

struct Bridge {
    app: HeadlessApp,
    host: Mutex<E2eHost>,
}

#[derive(Deserialize)]
struct InvokeRequest {
    command: String,
    #[serde(default)]
    args: Value,
}

pub async fn run() -> Result<(), String> {
    let ui_dir = std::env::var("MSW_E2E_UI_DIR").unwrap_or_else(|_| "ui/dist".to_string());
    let port = std::env::var("MSW_E2E_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(4180);
    if !PathBuf::from(&ui_dir).join("index.html").is_file() {
        return Err(format!(
            "{ui_dir}/index.html is missing. Run pnpm --dir ui build first."
        ));
    }
    let app = HeadlessApp::start().await?;
    // The bridge is a single-run test process. The host commands read these
    // two variables on every call. No other thread reads them.
    unsafe {
        std::env::set_var("MSW_E2E", "1");
        std::env::set_var("MSW_E2E_CLOCK", app.workspace.clock());
    }
    let defaults = E2eDefaults {
        guild_id: app.sim.guild_id().to_string(),
        guild_name: "Monsquaz Swap Test Guild".to_string(),
        announcement_id: app.sim.announcements_channel().to_string(),
        announcement_name: "announcements".to_string(),
        reveal_id: app.sim.reveal_channel().to_string(),
        reveal_name: "reveal".to_string(),
        bot_identity: "Monsquaz Test Bot".to_string(),
    };
    let host = E2eHost::new(app.workspace.root().to_path_buf(), defaults);
    let state = Arc::new(Bridge {
        app,
        host: Mutex::new(host),
    });

    let api = Router::new()
        .route("/invoke", post(invoke))
        .route("/control/participant", post(control_participant))
        .route("/control/pack", post(control_pack))
        .route("/control/clock", post(control_clock))
        .route("/control/messages", get(control_messages))
        .route("/control/assignment", get(control_assignment))
        .route("/control/state", get(control_state))
        .route("/control/ready", get(control_ready))
        .with_state(state.clone());
    let router = Router::new().nest("/api", api).fallback_service(
        ServeDir::new(&ui_dir)
            .not_found_service(ServeFile::new(PathBuf::from(&ui_dir).join("index.html"))),
    );
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    println!("BRIDGE_URL=http://{address}");
    let _ = std::io::stdout().flush();
    axum::serve(listener, router)
        .await
        .map_err(|error| error.to_string())
}

async fn invoke(State(bridge): State<Arc<Bridge>>, Json(request): Json<InvokeRequest>) -> Response {
    if let Some(result) = bot_command(&bridge.app, &request.command, &request.args).await {
        return reply(result);
    }
    let db = bridge
        .app
        .db
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut host = bridge
        .host
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if request.command == "host_mission_control" {
        return reply(mission_control_json(
            &db,
            host.settings(),
            bridge.app.gateway_health(),
        ));
    }
    reply(host.dispatch(&db, &request.command, &request.args))
}

async fn bot_command(
    app: &HeadlessApp,
    command: &str,
    args: &Value,
) -> Option<Result<Value, String>> {
    let setup = TwilightSetup::with_proxy(app.sim.http_host());
    match command {
        "host_bot_validate" => {
            let token = args.get("token").and_then(Value::as_str)?.to_string();
            Some(match setup.validate_token(&token).await {
                Ok(identity) => Ok(json!({
                    "botId": identity.id,
                    "identity": identity.username,
                    "inviteUrl": invite_url(app.sim.application_id(), swap_discord::RECOMMENDED_PERMISSIONS),
                })),
                Err(error) => Err(error.to_string()),
            })
        }
        "host_bot_guilds" => {
            let token = args.get("token").and_then(Value::as_str)?.to_string();
            Some(match setup.guilds(&token).await {
                Ok(guilds) => Ok(Value::Array(
                    guilds
                        .into_iter()
                        .map(|guild| json!({ "id": guild.id, "name": guild.name }))
                        .collect(),
                )),
                Err(error) => Err(error.to_string()),
            })
        }
        "host_bot_channels" => {
            let token = args.get("token").and_then(Value::as_str)?.to_string();
            let guild_id = args.get("guildId").and_then(Value::as_str)?.to_string();
            Some(match setup.channels(&token, &guild_id).await {
                Ok(channels) => Ok(Value::Array(
                    channels
                        .into_iter()
                        .map(|channel| json!({ "id": channel.id, "name": channel.name }))
                        .collect(),
                )),
                Err(error) => Err(error.to_string()),
            })
        }
        "host_bot_test_dm" => {
            let user_id = args.get("userId").and_then(Value::as_str)?.to_string();
            let token = bridge_token(app);
            Some(
                setup
                    .send_test_dm(&token, &user_id)
                    .await
                    .map(|id| {
                        Value::String(format!("The test message was sent. Message id: {id}."))
                    })
                    .map_err(|error| error.to_string()),
            )
        }
        _ => None,
    }
}

fn bridge_token(app: &HeadlessApp) -> String {
    app.sim.token().to_string()
}

fn reply(result: Result<Value, String>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(json!({ "value": value }))).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response(),
    }
}

#[derive(Deserialize)]
struct ParticipantRequest {
    action: String,
    user: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
}

async fn control_participant(
    State(bridge): State<Arc<Bridge>>,
    Json(request): Json<ParticipantRequest>,
) -> Response {
    let user = match bridge.app.sim.participant(&request.user) {
        Some(user) => user,
        None => {
            return reply(Err(format!(
                "the simulator has no participant {}",
                request.user
            )));
        }
    };
    let result = match request.action.as_str() {
        "join" => bridge.app.join(user),
        "seed" => bridge.app.send_file(
            user,
            &format!("{user}-seed.mod"),
            &mod_track(
                &request.title.unwrap_or_else(|| format!("Seed for {user}")),
                0,
                0,
            ),
            "my seed",
        ),
        "claim" => bridge.app.claim(user),
        "submit" => bridge.app.send_file(
            user,
            &request
                .file_name
                .unwrap_or_else(|| format!("{user}-edit.mod")),
            &mod_track("An edit", 4, 0),
            "my edit",
        ),
        "skip" => bridge.app.skip(user),
        "pause" => bridge
            .app
            .sim
            .command(user, "pause", Default::default())
            .map(|_| ()),
        "resume" => bridge
            .app
            .sim
            .command(user, "resume", Default::default())
            .map(|_| ()),
        other => return reply(Err(format!("unknown participant action: {other}"))),
    };
    match result {
        Ok(()) => {
            let _ = bridge.app.drain().await;
            reply(Ok(json!({ "ok": true })))
        }
        Err(error) => reply(Err(error)),
    }
}

#[derive(Deserialize)]
struct ClockRequest {
    seconds: i64,
}

async fn control_clock(
    State(bridge): State<Arc<Bridge>>,
    Json(request): Json<ClockRequest>,
) -> Response {
    let now = bridge.app.advance(request.seconds);
    reply(Ok(json!({ "clock": now })))
}

async fn control_pack(State(bridge): State<Arc<Bridge>>) -> Response {
    let first = mod_track("Pack one", 0, 0);
    let second = mod_track("Pack two", 4, 0);
    let bytes = pack_zip(&[("01-pack.mod", &first), ("02-pack.mod", &second)]);
    let path = bridge.app.workspace.root().join("pack.zip");
    match fs::write(&path, bytes) {
        Ok(()) => reply(Ok(json!({ "path": path }))),
        Err(error) => reply(Err(error.to_string())),
    }
}

#[derive(Deserialize)]
struct MessageQuery {
    user: String,
}

async fn control_messages(
    State(bridge): State<Arc<Bridge>>,
    Query(query): Query<MessageQuery>,
) -> Response {
    let Some(user) = bridge.app.sim.participant(&query.user) else {
        return reply(Err("unknown participant".to_string()));
    };
    reply(Ok(Value::Array(
        bridge
            .app
            .sim
            .bot_messages_for(user)
            .into_iter()
            .map(Value::String)
            .collect(),
    )))
}

#[derive(Deserialize)]
struct UserQuery {
    user: String,
}

async fn control_assignment(
    State(bridge): State<Arc<Bridge>>,
    Query(query): Query<UserQuery>,
) -> Response {
    let db = bridge
        .app
        .db
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(event_id) = swap_db::MetaRepo::get(db.conn(), "host.current_event")
        .ok()
        .flatten()
    else {
        return reply(Err("there is no event".to_string()));
    };
    let Some(user) = bridge.app.sim.participant(&query.user) else {
        return reply(Err("unknown participant".to_string()));
    };
    let assignment = swap_db::AssignmentRepo::latest_for_participant(db.conn(), &event_id, user)
        .map_err(|error| error.to_string());
    reply(assignment.map(|assignment| {
        json!({
            "trackCode": assignment.as_ref().map(|value| value.track_code.clone()),
            "round": assignment.as_ref().map(|value| value.round),
            "status": assignment.as_ref().map(|value| format!("{:?}", value.status)),
        })
    }))
}

async fn control_state(State(bridge): State<Arc<Bridge>>) -> Response {
    let db = bridge
        .app
        .db
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let event_id = swap_db::MetaRepo::get(db.conn(), "host.current_event")
        .ok()
        .flatten();
    let event = event_id
        .as_deref()
        .and_then(|id| swap_db::EventRepo::get(db.conn(), id).ok().flatten())
        .map(|event| {
            json!({
                "eventId": event.id,
                "name": event.name,
                "state": format!("{:?}", event.state),
            })
        });
    let participants = event_id
        .as_deref()
        .and_then(|id| swap_db::ParticipantRepo::list_for_event(db.conn(), id).ok())
        .map(|participants| participants.len())
        .unwrap_or(0);
    let tracks = event_id
        .as_deref()
        .and_then(|id| swap_db::TrackRepo::list_for_event(db.conn(), id).ok())
        .map(|tracks| tracks.len())
        .unwrap_or(0);
    reply(Ok(json!({
        "event": event,
        "participants": participants,
        "tracks": tracks,
        "clock": bridge.app.now(),
    })))
}

async fn control_ready(State(bridge): State<Arc<Bridge>>) -> Response {
    reply(Ok(json!({
        "ok": true,
        "token": bridge.app.sim.token(),
    })))
}
