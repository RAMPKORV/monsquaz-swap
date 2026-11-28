use swap_discord::{
    DiscordChannelId, DiscordError, DiscordPort, DiscordUserId, MessageId, OutboundFile,
    OutboundMessage,
};

/// A side effect that the transport must deliver. The engine commits the state
/// change first, then the caller dispatches these effects. A crash between the
/// commit and the send must not duplicate the send; the dispatcher is
/// idempotent at the transport boundary in a later slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Dm {
        user_id: String,
        content: String,
    },
    Offer {
        user_id: String,
        event_id: String,
        assignment_id: String,
        track_code: String,
        due_at: i64,
        claim_window_secs: i64,
    },
    TrackFile {
        user_id: String,
        track_code: String,
        filename: String,
        bytes: Vec<u8>,
        caption: String,
    },
    Channel {
        channel_id: String,
        content: String,
    },
    HostAlert {
        key: String,
        content: String,
    },
    /// A reminder or a deadline prompt to a participant. The DM carries a
    /// Leave button (D-051).
    Prompt {
        user_id: String,
        event_id: String,
        content: String,
    },
}

/// Deliver every effect. Failures are returned per effect so the caller can
/// retry, alert the host, or use the private-thread fallback.
pub async fn dispatch(
    discord: &dyn DiscordPort,
    effects: &[Effect],
) -> Vec<Result<MessageId, DiscordError>> {
    let mut results = Vec::with_capacity(effects.len());
    for effect in effects {
        let result = match effect {
            Effect::Dm { user_id, content } => {
                discord
                    .send_dm(
                        &DiscordUserId::new(user_id.clone()),
                        OutboundMessage {
                            buttons: Vec::new(),
                            content: content.clone(),
                        },
                    )
                    .await
            }
            Effect::Prompt {
                user_id,
                event_id,
                content,
            } => {
                discord
                    .send_dm(
                        &DiscordUserId::new(user_id.clone()),
                        OutboundMessage {
                            content: content.clone(),
                            buttons: crate::delivery::leave_button(event_id),
                        },
                    )
                    .await
            }
            Effect::Offer {
                user_id,
                event_id,
                track_code,
                due_at,
                claim_window_secs,
                ..
            } => {
                let content =
                    crate::delivery::offer_content(track_code, *claim_window_secs, *due_at);
                discord
                    .send_dm(
                        &DiscordUserId::new(user_id.clone()),
                        OutboundMessage {
                            content,
                            buttons: crate::delivery::leave_button(event_id),
                        },
                    )
                    .await
            }
            Effect::TrackFile {
                user_id,
                filename,
                bytes,
                caption,
                ..
            } => {
                discord
                    .send_file(
                        &DiscordUserId::new(user_id.clone()),
                        OutboundFile {
                            buttons: Vec::new(),
                            filename: filename.clone(),
                            bytes: bytes.clone(),
                            caption: caption.clone(),
                        },
                    )
                    .await
            }
            Effect::Channel {
                channel_id,
                content,
            } => {
                discord
                    .send_channel_message(
                        &DiscordChannelId::new(channel_id.clone()),
                        OutboundMessage {
                            buttons: Vec::new(),
                            content: content.clone(),
                        },
                    )
                    .await
            }
            Effect::HostAlert { .. } => Err(DiscordError::Other(
                "host alerts must use the delivery worker".to_string(),
            )),
        };
        results.push(result);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use swap_discord::FakeDiscord;

    #[tokio::test]
    async fn every_effect_reaches_the_port() {
        let discord = FakeDiscord::new();
        let results = dispatch(
            &discord,
            &[
                Effect::Dm {
                    user_id: "u1".to_string(),
                    content: "hello".to_string(),
                },
                Effect::Offer {
                    user_id: "u1".to_string(),
                    event_id: "evt".to_string(),
                    assignment_id: "a1".to_string(),
                    track_code: "SAAAA".to_string(),
                    due_at: 2_000,
                    claim_window_secs: 86_400,
                },
                Effect::TrackFile {
                    user_id: "u1".to_string(),
                    track_code: "SAAAA".to_string(),
                    filename: "SAAAA.it".to_string(),
                    bytes: vec![1, 2, 3],
                    caption: "Track SAAAA".to_string(),
                },
                Effect::Channel {
                    channel_id: "c1".to_string(),
                    content: "Round 1 is open".to_string(),
                },
            ],
        )
        .await;
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(Result::is_ok));
        assert_eq!(discord.sent_dms().len(), 2);
        assert_eq!(discord.sent_files().len(), 1);
        assert_eq!(discord.channel_messages().len(), 1);
        assert_eq!(discord.sent_files()[0].file.filename, "SAAAA.it");
    }
}
