use async_trait::async_trait;
use thiserror::Error;

/// VIEW_CHANNEL | SEND_MESSAGES | EMBED_LINKS | ATTACH_FILES | READ_MESSAGE_HISTORY
pub const RECOMMENDED_PERMISSIONS: u64 = 117_760;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotIdentity {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SetupError {
    #[error("the bot token is empty")]
    EmptyToken,
    #[error("the bot token does not have the expected three-part shape")]
    InvalidShape,
    #[error("the bot token is not valid")]
    TokenRejected,
    #[error("the guild or channel is not valid")]
    InvalidSelection,
    #[error("the user id is not a Discord snowflake")]
    InvalidUserId,
    #[error("the bot cannot send a direct message to this user")]
    DmClosed,
    #[error("token validation failed: {0}")]
    Validation(String),
    #[error("Discord setup request failed: {0}")]
    Discord(String),
}

/// The live implementation calls the Discord REST API. The setup wizard uses
/// this trait so tests never touch the network.
#[async_trait]
pub trait TokenValidator: Send + Sync {
    async fn validate(&self, token: &str) -> Result<BotIdentity, SetupError>;
}

/// Directory and delivery operations for the setup wizard.
///
/// Each call takes the token because the host has not saved it yet. The
/// implementation must never print or persist the token.
#[async_trait]
pub trait SetupDirectory: Send + Sync {
    async fn validate_token(&self, token: &str) -> Result<BotIdentity, SetupError>;

    async fn guilds(&self, token: &str) -> Result<Vec<GuildSummary>, SetupError>;

    async fn channels(
        &self,
        token: &str,
        guild_id: &str,
    ) -> Result<Vec<ChannelSummary>, SetupError>;

    /// Send a test direct message and return the Discord message id.
    async fn send_test_dm(&self, token: &str, user_id: &str) -> Result<String, SetupError>;
}

pub fn validate_bot_token_shape(token: &str) -> Result<(), SetupError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(SetupError::EmptyToken);
    }
    if token.len() < 20 {
        return Err(SetupError::InvalidShape);
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(SetupError::InvalidShape);
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(SetupError::InvalidShape);
    }
    Ok(())
}

pub fn invite_url(application_id: &str, permissions: u64) -> String {
    format!(
        "https://discord.com/oauth2/authorize?client_id={application_id}&scope=bot%20applications.commands&permissions={permissions}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_token() -> String {
        format!("{}.{}.{}", "a".repeat(24), "b".repeat(6), "c".repeat(27))
    }

    #[test]
    fn a_three_part_token_is_accepted() {
        assert_eq!(validate_bot_token_shape(&valid_token()), Ok(()));
    }

    #[test]
    fn an_empty_or_short_token_is_refused() {
        assert_eq!(validate_bot_token_shape(""), Err(SetupError::EmptyToken));
        assert_eq!(
            validate_bot_token_shape("abc.def.ghi"),
            Err(SetupError::InvalidShape)
        );
    }

    #[test]
    fn a_token_with_an_unsafe_character_is_refused() {
        let token = format!("{}.{}.{}", "a".repeat(24), "b".repeat(6), "c".repeat(27));
        assert_eq!(
            validate_bot_token_shape(&format!("{token}!")),
            Err(SetupError::InvalidShape)
        );
    }

    #[test]
    fn the_invite_url_has_the_required_scopes() {
        let url = invite_url("123456789", RECOMMENDED_PERMISSIONS);
        assert!(url.contains("client_id=123456789"));
        assert!(url.contains("scope=bot%20applications.commands"));
        assert!(url.contains("permissions=117760"));
    }
}
