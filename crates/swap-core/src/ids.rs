use serde::{Deserialize, Serialize};

/// Stable Discord user identity. The string is a snowflake. Keep it as a
/// string in DTOs so JavaScript never loses precision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub String);

/// Opaque track identity. Never derive it from an owner or a submission order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackCode(pub String);

impl ParticipantId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl TrackCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
