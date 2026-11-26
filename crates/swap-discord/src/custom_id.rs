use std::fmt;

use thiserror::Error;

/// Participant-facing actions carried by persistent Discord buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Join,
    Claim,
    Submit,
    Skip,
    Pause,
    Resume,
    Extend,
    Leave,
    Title,
    FillIn,
    AcceptFillIn,
    DeclineFillIn,
    Status,
    Schedule,
    Help,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Claim => "claim",
            Self::Submit => "submit",
            Self::Skip => "skip",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Extend => "extend",
            Self::Leave => "leave",
            Self::Title => "title",
            Self::FillIn => "fillin",
            Self::AcceptFillIn => "accept_fill",
            Self::DeclineFillIn => "decline_fill",
            Self::Status => "status",
            Self::Schedule => "schedule",
            Self::Help => "help",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "join" => Self::Join,
            "claim" => Self::Claim,
            "submit" => Self::Submit,
            "skip" => Self::Skip,
            "pause" => Self::Pause,
            "resume" => Self::Resume,
            "extend" => Self::Extend,
            "leave" => Self::Leave,
            "title" => Self::Title,
            "fillin" => Self::FillIn,
            "accept_fill" => Self::AcceptFillIn,
            "decline_fill" => Self::DeclineFillIn,
            "status" => Self::Status,
            "schedule" => Self::Schedule,
            "help" => Self::Help,
            _ => return None,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CustomIdError {
    #[error("custom id must have the form swap:<event>:<action>:<token>")]
    Malformed,
    #[error("event id is empty or contains unsafe characters")]
    InvalidEventId,
    #[error("unknown action: {0}")]
    UnknownAction(String),
    #[error("token contains unsafe characters")]
    InvalidToken,
    #[error("custom id must be at most 100 characters")]
    TooLong,
}

/// Stable button custom id. Never trust a custom id without re-checking the
/// event state and the actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomId {
    event_id: String,
    action: Action,
    token: Option<String>,
}

impl CustomId {
    pub fn new(
        event_id: impl Into<String>,
        action: Action,
        token: Option<String>,
    ) -> Result<Self, CustomIdError> {
        let event_id = event_id.into();
        if !is_safe_segment(&event_id, 64) {
            return Err(CustomIdError::InvalidEventId);
        }
        if token
            .as_deref()
            .is_some_and(|token| !is_safe_segment(token, 64))
        {
            return Err(CustomIdError::InvalidToken);
        }
        let token_length = token.as_deref().map(str::len).unwrap_or(0);
        if 7 + event_id.len() + action.as_str().len() + token_length > 100 {
            return Err(CustomIdError::TooLong);
        }
        Ok(Self {
            event_id,
            action,
            token,
        })
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn parse(value: &str) -> Result<Self, CustomIdError> {
        if value.len() > 100 {
            return Err(CustomIdError::TooLong);
        }
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 4 || parts[0] != "swap" {
            return Err(CustomIdError::Malformed);
        }
        let event_id = parts[1];
        if !is_safe_segment(event_id, 64) {
            return Err(CustomIdError::InvalidEventId);
        }
        let action = Action::parse(parts[2])
            .ok_or_else(|| CustomIdError::UnknownAction(parts[2].to_string()))?;
        let token = if parts[3].is_empty() {
            None
        } else if is_safe_segment(parts[3], 64) {
            Some(parts[3].to_string())
        } else {
            return Err(CustomIdError::InvalidToken);
        };
        Ok(Self {
            event_id: event_id.to_string(),
            action,
            token,
        })
    }
}

impl fmt::Display for CustomId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "swap:{}:{}:{}",
            self.event_id,
            self.action.as_str(),
            self.token.as_deref().unwrap_or("")
        )
    }
}

fn is_safe_segment(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_id_round_trips() {
        let id = CustomId::new("evt-1", Action::Claim, Some("assign-7".to_string())).unwrap();
        assert_eq!(id.to_string(), "swap:evt-1:claim:assign-7");
        assert_eq!(CustomId::parse(&id.to_string()).unwrap(), id);
    }

    #[test]
    fn an_empty_token_is_allowed() {
        let id = CustomId::new("evt-1", Action::Help, None).unwrap();
        assert_eq!(id.to_string(), "swap:evt-1:help:");
        assert_eq!(CustomId::parse(&id.to_string()).unwrap().token(), None);
    }

    #[test]
    fn malformed_ids_are_refused() {
        assert_eq!(CustomId::parse("claim"), Err(CustomIdError::Malformed));
        assert_eq!(
            CustomId::parse("swap:evt-1:claim"),
            Err(CustomIdError::Malformed)
        );
        assert_eq!(
            CustomId::parse("other:evt-1:claim:"),
            Err(CustomIdError::Malformed)
        );
    }

    #[test]
    fn an_unknown_action_is_refused() {
        assert_eq!(
            CustomId::parse("swap:evt-1:dance:"),
            Err(CustomIdError::UnknownAction("dance".to_string()))
        );
    }

    #[test]
    fn unsafe_segments_are_refused() {
        assert_eq!(
            CustomId::parse("swap:../evt:claim:"),
            Err(CustomIdError::InvalidEventId)
        );
        assert_eq!(
            CustomId::new("evt-1", Action::Claim, Some("a:b".to_string())),
            Err(CustomIdError::InvalidToken)
        );
    }

    #[test]
    fn ids_over_the_discord_limit_are_refused() {
        let event = "e".repeat(64);
        let token = "t".repeat(64);
        assert_eq!(
            CustomId::new(event.clone(), Action::Claim, Some(token)),
            Err(CustomIdError::TooLong)
        );
        assert_eq!(
            CustomId::parse(&format!("swap:{event}:claim:{}", "t".repeat(64))),
            Err(CustomIdError::TooLong)
        );
    }

    proptest::proptest! {
        #[test]
        fn parsing_arbitrary_text_never_panics(value in proptest::prelude::any::<String>()) {
            let _ = CustomId::parse(&value);
        }

        #[test]
        fn every_action_round_trips(
            event in "[A-Za-z0-9_-]{1,32}",
            token in proptest::option::of("[A-Za-z0-9_-]{1,32}"),
            action_index in 0usize..15,
        ) {
            let actions = [
                Action::Join,
                Action::Claim,
                Action::Submit,
                Action::Skip,
                Action::Pause,
                Action::Resume,
                Action::Extend,
                Action::Leave,
                Action::Title,
                Action::FillIn,
                Action::AcceptFillIn,
                Action::DeclineFillIn,
                Action::Status,
                Action::Schedule,
                Action::Help,
            ];
            let id = CustomId::new(event, actions[action_index], token).unwrap();
            let parsed = CustomId::parse(&id.to_string()).unwrap();
            proptest::prop_assert_eq!(parsed, id);
        }
    }
}
