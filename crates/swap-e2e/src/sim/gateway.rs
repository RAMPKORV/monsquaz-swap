//! Gateway WebSocket for the local Discord simulator.
//!
//! The server sends every payload as a text frame. Twilight only decompresses
//! binary frames, so a text frame is valid even when the URL asks for zstd.
//! Compression is a transport optimization; the JSON shape is what these tests
//! prove.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

use super::payload;
use super::state::SimShared;

pub(crate) async fn upgrade(
    ws: WebSocketUpgrade,
    State(shared): State<Arc<SimShared>>,
) -> Response {
    ws.on_upgrade(move |socket| handle(socket, shared))
}

async fn handle(socket: WebSocket, shared: Arc<SimShared>) {
    let (mut sender, mut receiver) = socket.split();
    let hello = json!({ "op": 10, "d": { "heartbeat_interval": 45_000 } }).to_string();
    if sender.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    let mut events = shared.events.subscribe();
    loop {
        tokio::select! {
            incoming = receiver.next() => {
                let Some(message) = incoming else {
                    break;
                };
                match message {
                    Ok(Message::Text(text)) => {
                        let value: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                        match value.get("op").and_then(Value::as_u64) {
                            Some(2) => {
                                shared.mark_ready();
                                let ready = {
                                    let state = shared.state();
                                    let resume = format!(
                                        "{}/",
                                        shared.base_url.replacen("http", "ws", 1)
                                    );
                                    payload::ready_json(&state, &shared.config, &resume)
                                };
                                let frame = json!({
                                    "op": 0,
                                    "t": "READY",
                                    "s": shared.next_sequence(),
                                    "d": ready,
                                }).to_string();
                                if sender.send(Message::Text(frame.into())).await.is_err() {
                                    break;
                                }
                            }
                            Some(1) => {
                                let ack = json!({ "op": 11, "d": Value::Null }).to_string();
                                if sender.send(Message::Text(ack.into())).await.is_err() {
                                    break;
                                }
                            }
                            Some(6) => {
                                let frame = json!({
                                    "op": 0,
                                    "t": "RESUMED",
                                    "s": shared.next_sequence(),
                                    "d": {},
                                }).to_string();
                                if sender.send(Message::Text(frame.into())).await.is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(frame) => {
                        if sender.send(Message::Text(frame.text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    shared.ready.store(false, Ordering::SeqCst);
}
