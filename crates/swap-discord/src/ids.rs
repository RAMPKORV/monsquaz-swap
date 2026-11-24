use serde::{Deserialize, Serialize};

/// Discord snowflake as a string. JavaScript loses precision above 2^53.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscordUserId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscordGuildId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscordChannelId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub String);

impl DiscordUserId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl DiscordGuildId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl DiscordChannelId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl MessageId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
