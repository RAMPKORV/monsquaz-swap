use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use swap_files::{DEFAULT_EXTENSIONS, DEFAULT_MAX_SIZE, FilePolicy};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Classic,
    Async,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventSettings {
    pub mode: Mode,
    pub rounds: Option<u32>,
    pub claim_window_secs: i64,
    pub round_window_secs: i64,
    pub max_file_bytes: u64,
    pub allowed_extensions: Vec<String>,
    pub pack_type: Option<String>,
    pub pack_required: Option<bool>,
    pub group_mode: Option<String>,
    pub group_count: Option<u32>,
    pub group_size: Option<u32>,
    pub rotation: Option<String>,
    pub scheduler: Option<String>,
    pub swap_cadence: Option<String>,
    pub schedule_visibility: Option<String>,
}

impl Default for EventSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Classic,
            rounds: None,
            claim_window_secs: 24 * 60 * 60,
            round_window_secs: 24 * 60 * 60,
            max_file_bytes: DEFAULT_MAX_SIZE,
            allowed_extensions: DEFAULT_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            pack_type: None,
            pack_required: None,
            group_mode: None,
            group_count: None,
            group_size: None,
            rotation: None,
            scheduler: None,
            swap_cadence: None,
            schedule_visibility: None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("the claim window must be positive")]
    ClaimWindow,
    #[error("the round window must be positive")]
    RoundWindow,
    #[error("the maximum file size must be between 1 byte and 20 MiB")]
    FileSize,
    #[error("the allowlist must contain at least one extension")]
    EmptyAllowlist,
    #[error("invalid extension: {0}")]
    InvalidExtension(String),
    #[error("the group count must be between 1 and 4")]
    GroupCount,
    #[error("the group size must be between 2 and 20")]
    GroupSize,
    #[error("the scheduler must be balanced, random_matching, or cyclic")]
    Scheduler,
}

impl EventSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.claim_window_secs <= 0 {
            return Err(SettingsError::ClaimWindow);
        }
        if self.round_window_secs <= 0 {
            return Err(SettingsError::RoundWindow);
        }
        if self.max_file_bytes == 0 || self.max_file_bytes > DEFAULT_MAX_SIZE {
            return Err(SettingsError::FileSize);
        }
        if self.allowed_extensions.is_empty() {
            return Err(SettingsError::EmptyAllowlist);
        }
        if let Some(count) = self.group_count
            && !(1..=4).contains(&count)
        {
            return Err(SettingsError::GroupCount);
        }
        if let Some(size) = self.group_size
            && !(2..=20).contains(&size)
        {
            return Err(SettingsError::GroupSize);
        }
        if let Some(scheduler) = &self.scheduler
            && swap_core::ScheduleStrategy::parse(scheduler).is_none()
        {
            return Err(SettingsError::Scheduler);
        }
        let mut seen = BTreeSet::new();
        for extension in &self.allowed_extensions {
            let normalized = normalize_extension(extension);
            if !is_safe_extension(&normalized) {
                return Err(SettingsError::InvalidExtension(extension.clone()));
            }
            if !seen.insert(normalized) {
                return Err(SettingsError::InvalidExtension(extension.clone()));
            }
        }
        Ok(())
    }

    pub fn file_policy(&self) -> FilePolicy {
        FilePolicy::from_extensions(self.allowed_extensions.iter().map(String::as_str))
            .with_max_size(self.max_file_bytes)
    }

    pub fn normalized_extensions(&self) -> Vec<String> {
        self.allowed_extensions
            .iter()
            .map(|extension| normalize_extension(extension))
            .collect()
    }
}

fn normalize_extension(extension: &str) -> String {
    extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn is_safe_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= 16
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_are_classic_and_valid() {
        let settings = EventSettings::default();
        settings.validate().unwrap();
        assert_eq!(settings.mode, Mode::Classic);
        assert_eq!(settings.claim_window_secs, 86_400);
        assert_eq!(settings.round_window_secs, 86_400);
    }

    #[test]
    fn a_empty_allowlist_is_refused() {
        let settings = EventSettings {
            allowed_extensions: vec![],
            ..EventSettings::default()
        };
        assert_eq!(settings.validate(), Err(SettingsError::EmptyAllowlist));
    }

    #[test]
    fn a_path_in_an_extension_is_refused() {
        let settings = EventSettings {
            allowed_extensions: vec!["../it".to_string()],
            ..EventSettings::default()
        };
        assert!(matches!(
            settings.validate(),
            Err(SettingsError::InvalidExtension(_))
        ));
    }

    #[test]
    fn an_unknown_scheduler_is_refused() {
        let settings = EventSettings {
            scheduler: Some("predictable".to_string()),
            ..EventSettings::default()
        };
        assert_eq!(settings.validate(), Err(SettingsError::Scheduler));
    }

    #[test]
    fn the_settings_survive_json() {
        let settings = EventSettings {
            rounds: Some(3),
            allowed_extensions: vec!["IT".to_string(), ".xm".to_string()],
            ..EventSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: EventSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
        assert_eq!(restored.normalized_extensions(), vec!["it", "xm"]);
    }

    #[test]
    fn an_old_settings_json_uses_defaults() {
        let settings: EventSettings = serde_json::from_str(r#"{"rounds": 2}"#).unwrap();
        assert_eq!(settings.rounds, Some(2));
        assert_eq!(settings.claim_window_secs, 86_400);
    }
}
