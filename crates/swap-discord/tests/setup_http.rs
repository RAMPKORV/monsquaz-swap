#![cfg(all(feature = "live", feature = "test-proxy"))]
//! Local Discord API mock tests for the live setup and delivery adapters.
//!
//! Twilight can route requests to a local HTTP host through its proxy setting.
//! These tests exercise the real serializers and deserializers without a token
//! or a live Discord connection.

use swap_discord::{
    Button, ButtonStyle, DiscordChannelId, DiscordPort, DiscordUserId, HttpDiscordPort,
    InteractionRef, InteractionReply, OutboundFile, OutboundMessage, SetupDirectory, TwilightSetup,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CURRENT_USER: &str = r#"{
  "accent_color": null,
  "avatar": null,
  "banner": null,
  "bot": true,
  "discriminator": 0,
  "email": null,
  "flags": null,
  "global_name": null,
  "id": "900",
  "locale": null,
  "mfa_enabled": false,
  "username": "Monsquaz Bot",
  "premium_type": null,
  "public_flags": null,
  "verified": null
}"#;

const GUILDS: &str = r#"[{
  "id": "10",
  "name": "Night Shift",
  "icon": null,
  "owner": true,
  "permissions": "8",
  "features": []
}]"#;

const CHANNELS: &str = r#"[
  {"id": "11", "type": 0, "name": "swap-announcements"},
  {"id": "12", "type": 0, "name": "swap-reveal"},
  {"id": "13", "type": 2, "name": "voice"},
  {"id": "14", "type": 5, "name": "updates"}
]"#;

const DM_CHANNEL: &str = r#"{"id": "100", "type": 1}"#;
const PRIVATE_THREAD: &str = r#"{"id": "101", "type": 12, "name": "Monsquaz Swap delivery"}"#;

const MESSAGE: &str = r#"{
  "attachments": [],
  "author": {
    "accent_color": null,
    "avatar": null,
    "avatar_decoration": null,
    "avatar_decoration_data": null,
    "banner": null,
    "bot": true,
    "discriminator": 0,
    "email": null,
    "flags": null,
    "global_name": null,
    "id": "900",
    "locale": null,
    "mfa_enabled": null,
    "username": "Monsquaz Bot",
    "premium_type": null,
    "primary_guild": null,
    "public_flags": null,
    "system": null,
    "verified": null
  },
  "channel_id": "100",
  "components": [],
  "content": "Monsquaz Swap test message.",
  "edited_timestamp": null,
  "embeds": [],
  "id": "555",
  "mention_everyone": false,
  "mention_roles": [],
  "mentions": [],
  "pinned": false,
  "reactions": [],
  "timestamp": "2026-11-25T00:00:00.000000+00:00",
  "tts": false,
  "type": 0
}"#;

fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn proxy_host(server: &MockServer) -> String {
    server.address().to_string()
}

fn assert_bot_auth(requests: &[wiremock::Request], token: &str) {
    let expected = format!("Bot {token}");
    for request in requests {
        assert_eq!(
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some(expected.as_str()),
            "request {} had the wrong authorization header",
            request.url.path()
        );
    }
}

async fn mount_json(server: &MockServer, method_name: &str, route: &str, body: &str) {
    Mock::given(method(method_name))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_setup_directory_reads_identity_guilds_and_text_channels() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(&server, "GET", "/api/v10/users/@me", CURRENT_USER).await;
    mount_json(&server, "GET", "/api/v10/users/@me/guilds", GUILDS).await;
    mount_json(&server, "GET", "/api/v10/guilds/10/channels", CHANNELS).await;

    let setup = TwilightSetup::with_proxy(proxy_host(&server));
    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";

    let identity = setup.validate_token(token).await.unwrap();
    assert_eq!(identity.id, "900");
    assert_eq!(identity.username, "Monsquaz Bot");

    let guilds = setup.guilds(token).await.unwrap();
    assert_eq!(guilds.len(), 1);
    assert_eq!(guilds[0].name, "Night Shift");

    let channels = setup.channels(token, "10").await.unwrap();
    assert_eq!(channels.len(), 3);
    assert!(channels.iter().any(|channel| channel.name == "swap-reveal"));
    assert!(!channels.iter().any(|channel| channel.name == "voice"));
}

#[tokio::test]
async fn the_http_port_sends_a_dm_through_the_mock() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(&server, "POST", "/api/v10/users/@me/channels", DM_CHANNEL).await;
    mount_json(&server, "POST", "/api/v10/channels/100/messages", MESSAGE).await;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let port = HttpDiscordPort::with_proxy(token, proxy_host(&server)).unwrap();
    let id = port
        .send_dm(
            &DiscordUserId::new("42"),
            OutboundMessage {
                buttons: Vec::new(),
                content: "Monsquaz Swap test message.".to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(id.0, "555");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_bot_auth(&requests, token);
    assert_eq!(requests[0].url.path(), "/api/v10/users/@me/channels");
    assert!(String::from_utf8_lossy(&requests[0].body).contains("\"42\""));
    assert_eq!(requests[1].url.path(), "/api/v10/channels/100/messages");
    assert!(String::from_utf8_lossy(&requests[1].body).contains("Monsquaz Swap test message."));
}

#[tokio::test]
async fn the_http_port_sends_a_file_through_the_mock() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(&server, "POST", "/api/v10/users/@me/channels", DM_CHANNEL).await;
    mount_json(&server, "POST", "/api/v10/channels/100/messages", MESSAGE).await;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let port = HttpDiscordPort::with_proxy(token, proxy_host(&server)).unwrap();
    let id = port
        .send_file(
            &DiscordUserId::new("42"),
            OutboundFile {
                buttons: Vec::new(),
                filename: "S7F3A.it".to_string(),
                bytes: vec![1, 2, 3],
                caption: "Track S7F3A. Your turn.".to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(id.0, "555");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_bot_auth(&requests, token);
    assert_eq!(requests[0].url.path(), "/api/v10/users/@me/channels");
    assert_eq!(requests[1].url.path(), "/api/v10/channels/100/messages");
    let body = String::from_utf8_lossy(&requests[1].body);
    assert!(body.contains("S7F3A.it"));
    assert!(body.contains("Track S7F3A. Your turn."));
}

#[tokio::test]
async fn the_setup_test_dm_uses_the_same_mock() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(&server, "POST", "/api/v10/users/@me/channels", DM_CHANNEL).await;
    mount_json(&server, "POST", "/api/v10/channels/100/messages", MESSAGE).await;

    let setup = TwilightSetup::with_proxy(proxy_host(&server));
    let token = "aaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let id = setup.send_test_dm(token, "42").await.unwrap();
    assert_eq!(id, "555");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_bot_auth(&requests, token);
    assert_eq!(requests[0].url.path(), "/api/v10/users/@me/channels");
    assert_eq!(requests[1].url.path(), "/api/v10/channels/100/messages");
    assert!(
        String::from_utf8_lossy(&requests[1].body)
            .contains("Monsquaz Swap test message. Your bot can reach you.")
    );
}

#[tokio::test]
async fn the_http_port_creates_a_private_thread_and_sends_a_file() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(
        &server,
        "POST",
        "/api/v10/channels/10/threads",
        PRIVATE_THREAD,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/channels/101/thread-members/42"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_json(&server, "POST", "/api/v10/channels/101/messages", MESSAGE).await;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let port = HttpDiscordPort::with_proxy(token, proxy_host(&server)).unwrap();
    let thread = port
        .create_private_thread(&DiscordChannelId::new("10"), "Monsquaz Swap delivery")
        .await
        .unwrap();
    assert_eq!(thread.0, "101");
    port.add_thread_member(&thread, &DiscordUserId::new("42"))
        .await
        .unwrap();
    let id = port
        .send_file_to_channel(
            &thread,
            OutboundFile {
                buttons: Vec::new(),
                filename: "S7F3A.it".to_string(),
                bytes: vec![1, 2, 3],
                caption: "Track S7F3A. Your turn.".to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(id.0, "555");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert_bot_auth(&requests, token);
    assert_eq!(requests[0].url.path(), "/api/v10/channels/10/threads");
    assert_eq!(
        requests[1].url.path(),
        "/api/v10/channels/101/thread-members/42"
    );
    assert_eq!(requests[2].url.path(), "/api/v10/channels/101/messages");
}

#[tokio::test]
async fn a_dm_and_a_file_carry_buttons_as_one_action_row() {
    install_crypto();
    let server = MockServer::start().await;
    mount_json(&server, "POST", "/api/v10/users/@me/channels", DM_CHANNEL).await;
    mount_json(&server, "POST", "/api/v10/channels/100/messages", MESSAGE).await;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let port = HttpDiscordPort::with_proxy(token, proxy_host(&server)).unwrap();
    let leave = Button {
        custom_id: "swap:evt-1:leave:".to_string(),
        label: "Leave event".to_string(),
        style: ButtonStyle::Secondary,
    };
    port.send_dm(
        &DiscordUserId::new("42"),
        OutboundMessage {
            content: "Track S7F3A is available.".to_string(),
            buttons: vec![leave.clone()],
        },
    )
    .await
    .unwrap();
    port.send_file(
        &DiscordUserId::new("42"),
        OutboundFile {
            filename: "S7F3A.it".to_string(),
            bytes: vec![1, 2, 3],
            caption: "Track S7F3A. Your turn.".to_string(),
            buttons: vec![leave],
        },
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    let message: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    let row = &message["components"][0];
    assert_eq!(row["type"], 1);
    assert_eq!(row["components"][0]["type"], 2);
    assert_eq!(row["components"][0]["style"], 2);
    assert_eq!(row["components"][0]["custom_id"], "swap:evt-1:leave:");
    assert_eq!(row["components"][0]["label"], "Leave event");
    let file_body = String::from_utf8_lossy(&requests[3].body);
    assert!(file_body.contains("swap:evt-1:leave:"));
    assert!(file_body.contains("S7F3A.it"));
}

#[tokio::test]
async fn an_ephemeral_reply_uses_the_interaction_callback() {
    install_crypto();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/interactions/777/secret-token/callback"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let token = "aaaaaaaaaaaaaaaaaaaaaaaa.bbbbbb.ccccccccccccccccccccccccc";
    let port = HttpDiscordPort::with_proxy(token, proxy_host(&server)).unwrap();
    port.reply_ephemeral(
        &InteractionRef::new("777", "secret-token"),
        InteractionReply {
            content: "Leave the event? Confirm below.".to_string(),
            buttons: vec![Button {
                custom_id: "swap:evt-1:leave_ok:1700000000".to_string(),
                label: "Confirm".to_string(),
                style: ButtonStyle::Danger,
            }],
        },
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["type"], 4);
    assert_eq!(body["data"]["flags"], 64);
    assert_eq!(body["data"]["content"], "Leave the event? Confirm below.");
    assert_eq!(body["data"]["components"][0]["components"][0]["style"], 4);
    assert_eq!(
        body["data"]["components"][0]["components"][0]["custom_id"],
        "swap:evt-1:leave_ok:1700000000"
    );
}
