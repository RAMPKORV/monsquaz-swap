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

/// How the bot finds a fill-in for a stranded classic slot (D-040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FillInFlow {
    /// The host picks one eligible person. A decline or an expiry opens a
    /// host decision.
    #[default]
    HostAppoints,
    /// The bot offers the slot to up to 3 ranked eligible people across both
    /// tiers, one at a time. The first claim wins (D-064).
    AutoChain,
    /// The host orders a list without a size limit. The bot offers the slot
    /// down the list, one person at a time. Tier 1 still comes first; the host
    /// order applies inside each tier (D-050, D-070).
    HostList,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventSettings {
    pub mode: Mode,
    pub rounds: Option<u32>,
    pub seed_window_secs: i64,
    pub final_round_window_secs: i64,
    pub seed_min: u32,
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
    pub handoff_policy: Option<String>,
    pub async_expected_songs: Option<u32>,
    pub async_claims_per_participant: Option<u32>,
    pub async_release_time: Option<String>,
    pub async_wave_offset_secs: i64,
    pub async_late_seeds: Option<bool>,
    pub quiet_hours_start: u8,
    pub quiet_hours_end: u8,
    /// The default edit time a fill-in has after the claim, in seconds.
    pub fill_in_window_secs: i64,
    /// The claim window of a chain or host-list fill-in offer, in seconds
    /// (D-069).
    pub fill_in_claim_window_secs: i64,
    pub fill_in_flow: FillInFlow,
}

impl Default for EventSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Classic,
            rounds: None,
            seed_window_secs: 7 * 24 * 60 * 60,
            final_round_window_secs: 48 * 60 * 60,
            seed_min: 2,
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
            handoff_policy: None,
            async_expected_songs: None,
            async_claims_per_participant: None,
            async_release_time: None,
            async_wave_offset_secs: 2 * 60 * 60,
            async_late_seeds: None,
            quiet_hours_start: 22,
            quiet_hours_end: 8,
            fill_in_window_secs: 24 * 60 * 60,
            fill_in_claim_window_secs: 6 * 60 * 60,
            fill_in_flow: FillInFlow::HostAppoints,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("the claim window must be positive")]
    ClaimWindow,
    #[error("the round window must be positive")]
    RoundWindow,
    #[error("the seed window must be positive")]
    SeedWindow,
    #[error("the final round window must be positive")]
    FinalRoundWindow,
    #[error("the seed minimum must be at least 2")]
    SeedMinimum,
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
    #[error("the handoff policy must be Strict or Flexible")]
    HandoffPolicy,
    #[error("the expected song count must be between 1 and 1000")]
    AsyncSongs,
    #[error("the claims per participant must be between 1 and 40")]
    AsyncClaims,
    #[error("the release time must use HH:MM")]
    AsyncReleaseTime,
    #[error("the second wave offset must be positive")]
    AsyncWaveOffset,
    #[error("the quiet hours must be between 0 and 23")]
    QuietHours,
    #[error("the fill-in window must be between 1 hour and 14 days")]
    FillInWindow,
    #[error("the fill-in claim window must be between 1 and 48 hours")]
    FillInClaimWindow,
}

impl EventSettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.claim_window_secs <= 0 {
            return Err(SettingsError::ClaimWindow);
        }
        if self.round_window_secs <= 0 {
            return Err(SettingsError::RoundWindow);
        }
        if self.seed_window_secs <= 0 {
            return Err(SettingsError::SeedWindow);
        }
        if self.final_round_window_secs <= 0 {
            return Err(SettingsError::FinalRoundWindow);
        }
        if self.seed_min < 2 {
            return Err(SettingsError::SeedMinimum);
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
        if let Some(policy) = &self.handoff_policy
            && !matches!(policy.as_str(), "Strict" | "Flexible")
        {
            return Err(SettingsError::HandoffPolicy);
        }
        if let Some(songs) = self.async_expected_songs
            && !(1..=1000).contains(&songs)
        {
            return Err(SettingsError::AsyncSongs);
        }
        if let Some(claims) = self.async_claims_per_participant
            && !(1..=40).contains(&claims)
        {
            return Err(SettingsError::AsyncClaims);
        }
        if let Some(time) = &self.async_release_time {
            let valid = time.len() == 5
                && time.as_bytes()[2] == b':'
                && time
                    .get(..2)
                    .and_then(|hour| hour.parse::<u8>().ok())
                    .is_some_and(|hour| hour < 24)
                && time
                    .get(3..)
                    .and_then(|minute| minute.parse::<u8>().ok())
                    .is_some_and(|minute| minute < 60);
            if !valid {
                return Err(SettingsError::AsyncReleaseTime);
            }
        }
        if self.async_wave_offset_secs <= 0 {
            return Err(SettingsError::AsyncWaveOffset);
        }
        if self.quiet_hours_start > 23 || self.quiet_hours_end > 23 {
            return Err(SettingsError::QuietHours);
        }
        if !(60 * 60..=14 * 24 * 60 * 60).contains(&self.fill_in_window_secs) {
            return Err(SettingsError::FillInWindow);
        }
        if !(60 * 60..=48 * 60 * 60).contains(&self.fill_in_claim_window_secs) {
            return Err(SettingsError::FillInClaimWindow);
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
    fn the_fill_in_window_defaults_to_24_hours_and_is_bounded() {
        assert_eq!(EventSettings::default().fill_in_window_secs, 86_400);
        for (secs, valid) in [
            (0, false),
            (3_599, false),
            (3_600, true),
            (1_209_600, true),
            (1_209_601, false),
        ] {
            let settings = EventSettings {
                fill_in_window_secs: secs,
                ..EventSettings::default()
            };
            assert_eq!(settings.validate().is_ok(), valid, "{secs}");
        }
    }

    #[test]
    fn the_fill_in_claim_window_defaults_to_6_hours_and_is_bounded() {
        assert_eq!(EventSettings::default().fill_in_claim_window_secs, 21_600);
        for (secs, valid) in [
            (0, false),
            (3_599, false),
            (3_600, true),
            (172_800, true),
            (172_801, false),
        ] {
            let settings = EventSettings {
                fill_in_claim_window_secs: secs,
                ..EventSettings::default()
            };
            assert_eq!(settings.validate().is_ok(), valid, "{secs}");
        }
    }

    #[test]
    fn the_fill_in_flow_uses_snake_case_and_defaults_to_host_appoints() {
        assert_eq!(
            EventSettings::default().fill_in_flow,
            FillInFlow::HostAppoints
        );
        let settings: EventSettings =
            serde_json::from_str(r#"{"fill_in_flow": "auto_chain"}"#).unwrap();
        assert_eq!(settings.fill_in_flow, FillInFlow::AutoChain);
        assert!(serde_json::from_str::<EventSettings>(r#"{"fill_in_flow": "random"}"#).is_err());
    }

    #[test]
    fn an_old_settings_json_uses_defaults() {
        let settings: EventSettings = serde_json::from_str(r#"{"rounds": 2}"#).unwrap();
        assert_eq!(settings.rounds, Some(2));
        assert_eq!(settings.claim_window_secs, 86_400);
        assert_eq!(settings.fill_in_window_secs, 86_400);
        assert_eq!(settings.fill_in_claim_window_secs, 21_600);
        assert_eq!(settings.fill_in_flow, FillInFlow::HostAppoints);
    }
}
