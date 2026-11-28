//! Bot setup service: token validation, keychain storage, channel mapping, and
//! the live test direct message.
//!
//! The service keeps the token out of `SettingsDto`. The token lives in the OS
//! keychain, or in memory for an explicitly allowed session-only setup.

use std::sync::Mutex;

#[cfg(any(not(feature = "live"), test))]
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(feature = "live")]
use tauri::Manager;
use tauri::{AppHandle, State};

use swap_discord::{
    BotIdentity, DiscordPort, DiscordUserId, RECOMMENDED_PERMISSIONS, SetupDirectory, invite_url,
};

use crate::host::{self, SettingsDto};

/// A keychain failure must never fall back to plaintext storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenStoreError {
    /// No credential store exists on this computer. A delete is vacuous.
    NoStore,
    /// A store exists but is locked or inaccessible. A token may remain.
    Unavailable,
    Failed(String),
}

impl TokenStoreError {
    fn message(&self) -> String {
        match self {
            Self::NoStore => {
                "This computer does not provide a keychain. Select Use this session only to continue.".to_string()
            }
            Self::Unavailable => {
                "The OS keychain is locked. Unlock it and try again.".to_string()
            }
            Self::Failed(_) => {
                "The OS keychain refused the token. Check the keychain and try again.".to_string()
            }
        }
    }
}

/// The app reads and writes the bot token through this trait. Tests use an
/// in-memory store; the app uses the OS keychain.
pub trait TokenStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, TokenStoreError>;
    fn set(&self, token: &str) -> Result<(), TokenStoreError>;
    fn delete(&self) -> Result<(), TokenStoreError>;
}

const KEYCHAIN_SERVICE: &str = "dev.monsquaz.swap";
const KEYCHAIN_ACCOUNT: &str = "discord-bot-token";

#[derive(Debug, Default)]
pub struct KeychainStore;

impl TokenStore for KeychainStore {
    fn get(&self) -> Result<Option<String>, TokenStoreError> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(map_keyring_error)?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn set(&self, token: &str) -> Result<(), TokenStoreError> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(map_keyring_error)?;
        entry.set_password(token).map_err(map_keyring_error)
    }

    fn delete(&self) -> Result<(), TokenStoreError> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(map_keyring_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

fn map_keyring_error(error: keyring::Error) -> TokenStoreError {
    match error {
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            TokenStoreError::NoStore
        }
        keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => {
            TokenStoreError::Unavailable
        }
        keyring::Error::NoEntry => TokenStoreError::NoStore,
        other => TokenStoreError::Failed(other.to_string()),
    }
}

/// Session-only token storage. The token never touches the disk.
#[derive(Default)]
pub struct SessionTokenStore {
    token: Mutex<Option<String>>,
}

impl SessionTokenStore {
    pub fn clear(&self) {
        self.lock().take();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl TokenStore for SessionTokenStore {
    fn get(&self) -> Result<Option<String>, TokenStoreError> {
        Ok(self.lock().clone())
    }

    fn set(&self, token: &str) -> Result<(), TokenStoreError> {
        *self.lock() = Some(token.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), TokenStoreError> {
        self.lock().take();
        Ok(())
    }
}

/// Runtime state owned by Tauri.
#[derive(Default)]
pub struct BotRuntime {
    session: SessionTokenStore,
}

impl BotRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// The session token takes precedence over the keychain token.
    fn read_token(&self, keychain: &dyn TokenStore) -> Result<Option<String>, String> {
        if let Some(token) = self.session.get().map_err(|error| error.message())? {
            return Ok(Some(token));
        }
        keychain.get().map_err(|error| error.message())
    }

    fn clear_session(&self) {
        self.session.clear();
    }

    /// Read the session token first, then the keychain token.
    #[cfg(feature = "live")]
    pub fn current_token(&self) -> Result<Option<String>, String> {
        self.read_token(&KeychainStore)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotPreviewDto {
    pub bot_id: String,
    pub identity: String,
    pub invite_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuildOptionDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelOptionDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BotStatusDto {
    pub configured: bool,
    pub token_storage: String,
    pub keychain_error: bool,
    pub gateway: Option<GatewayStatusDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatusDto {
    pub state: String,
    pub attempt: u32,
    pub last_error: Option<String>,
    pub connected_at: Option<i64>,
    pub handler_errors: u32,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotSetupInput {
    pub token: String,
    pub guild_id: String,
    pub announcement_channel_id: String,
    pub reveal_channel_id: String,
    pub allow_session_only: bool,
}

impl BotSetupInput {
    fn token(&self) -> &str {
        self.token.trim()
    }
}

#[cfg(feature = "live")]
fn directory() -> std::sync::Arc<dyn SetupDirectory> {
    std::sync::Arc::new(swap_discord::TwilightSetup::new())
}

#[cfg(not(feature = "live"))]
fn directory() -> std::sync::Arc<dyn SetupDirectory> {
    std::sync::Arc::new(DisabledDirectory)
}

#[cfg(not(feature = "live"))]
use swap_discord::{ChannelSummary, GuildSummary, SetupError};

#[cfg(not(feature = "live"))]
struct DisabledDirectory;

#[cfg(not(feature = "live"))]
#[async_trait]
impl SetupDirectory for DisabledDirectory {
    async fn validate_token(&self, _token: &str) -> Result<BotIdentity, SetupError> {
        Err(SetupError::Discord(
            "this build does not include Discord support".to_string(),
        ))
    }

    async fn guilds(&self, _token: &str) -> Result<Vec<GuildSummary>, SetupError> {
        Err(SetupError::Discord(
            "this build does not include Discord support".to_string(),
        ))
    }

    async fn channels(
        &self,
        _token: &str,
        _guild_id: &str,
    ) -> Result<Vec<ChannelSummary>, SetupError> {
        Err(SetupError::Discord(
            "this build does not include Discord support".to_string(),
        ))
    }

    async fn send_test_dm(&self, _token: &str, _user_id: &str) -> Result<String, SetupError> {
        Err(SetupError::Discord(
            "this build does not include Discord support".to_string(),
        ))
    }
}

#[cfg(feature = "live")]
fn live_port(token: &str) -> Result<std::sync::Arc<dyn DiscordPort>, String> {
    let port = swap_discord::HttpDiscordPort::new(token).map_err(|error| error.to_string())?;
    Ok(std::sync::Arc::new(port))
}

#[cfg(not(feature = "live"))]
fn live_port(_token: &str) -> Result<std::sync::Arc<dyn DiscordPort>, String> {
    Err("this build does not include Discord support".to_string())
}

fn identity_label(identity: &BotIdentity) -> String {
    format!("{} ({})", identity.username, identity.id)
}

async fn preview(directory: &dyn SetupDirectory, token: &str) -> Result<BotPreviewDto, String> {
    let token = token.trim();
    let identity = directory
        .validate_token(token)
        .await
        .map_err(|error| error.to_string())?;
    Ok(BotPreviewDto {
        invite_url: invite_url(&identity.id, RECOMMENDED_PERMISSIONS),
        bot_id: identity.id.clone(),
        identity: identity_label(&identity),
    })
}

async fn guild_options(
    directory: &dyn SetupDirectory,
    token: &str,
) -> Result<Vec<GuildOptionDto>, String> {
    let mut guilds = directory
        .guilds(token.trim())
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|guild| GuildOptionDto {
            id: guild.id,
            name: guild.name,
        })
        .collect::<Vec<_>>();
    guilds.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(guilds)
}

async fn channel_options(
    directory: &dyn SetupDirectory,
    token: &str,
    guild_id: &str,
) -> Result<Vec<ChannelOptionDto>, String> {
    let mut channels = directory
        .channels(token.trim(), guild_id.trim())
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|channel| ChannelOptionDto {
            id: channel.id,
            name: channel.name,
        })
        .collect::<Vec<_>>();
    channels.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(channels)
}

/// Validate the selection, store the token, and fill the non-secret settings.
async fn save_setup(
    directory: &dyn SetupDirectory,
    keychain: &dyn TokenStore,
    session: &dyn TokenStore,
    settings: &mut SettingsDto,
    input: &BotSetupInput,
) -> Result<(), String> {
    let token = input.token();
    let identity = directory
        .validate_token(token)
        .await
        .map_err(|error| error.to_string())?;

    let guilds = directory
        .guilds(token)
        .await
        .map_err(|error| error.to_string())?;
    let guild = guilds
        .iter()
        .find(|guild| guild.id == input.guild_id.trim())
        .ok_or_else(|| "Choose a server that the bot can join.".to_string())?;

    let channels = directory
        .channels(token, &guild.id)
        .await
        .map_err(|error| error.to_string())?;
    let announcement = channels
        .iter()
        .find(|channel| channel.id == input.announcement_channel_id.trim())
        .ok_or_else(|| "Choose an announcement channel in this server.".to_string())?;
    let reveal = channels
        .iter()
        .find(|channel| channel.id == input.reveal_channel_id.trim())
        .ok_or_else(|| "Choose a reveal channel in this server.".to_string())?;
    if announcement.id == reveal.id {
        return Err("Choose two different channels for announcements and the reveal.".to_string());
    }

    if input.allow_session_only {
        session.set(token).map_err(|error| error.message())?;
        let _ = keychain.delete();
        settings.token_storage = "session".to_string();
    } else {
        keychain.set(token).map_err(|error| error.message())?;
        session.delete().map_err(|error| error.message())?;
        settings.token_storage = "keychain".to_string();
    }

    settings.bot_configured = true;
    settings.bot_identity = format!("{} ({})", identity.username, identity.id);
    settings.guild_id = guild.id.clone();
    settings.guild_name = guild.name.clone();
    settings.announcement_channel_id = announcement.id.clone();
    settings.announcement_channel_name = announcement.name.clone();
    settings.reveal_channel_id = reveal.id.clone();
    settings.reveal_channel_name = reveal.name.clone();
    Ok(())
}

async fn test_dm(port: &dyn DiscordPort, user_id: &str) -> Result<String, String> {
    let user_id = user_id.trim();
    if user_id.is_empty() || !user_id.chars().all(|character| character.is_ascii_digit()) {
        return Err(
            "Enter the Discord user id of the person who receives the test message.".to_string(),
        );
    }
    let message = swap_discord::OutboundMessage {
        content: "Monsquaz Swap test message. Your bot can reach you.".to_string(),
    };
    let message_id = port
        .send_dm(&DiscordUserId::new(user_id), message)
        .await
        .map_err(|error| error.to_string())?;
    Ok(message_id.0)
}

fn status_from(
    keychain: Result<Option<String>, TokenStoreError>,
    session: Option<String>,
) -> BotStatusDto {
    if session.is_some() {
        return BotStatusDto {
            configured: true,
            token_storage: "session".to_string(),
            keychain_error: false,
            gateway: None,
        };
    }
    match keychain {
        Ok(Some(_)) => BotStatusDto {
            configured: true,
            token_storage: "keychain".to_string(),
            keychain_error: false,
            gateway: None,
        },
        Ok(None) => BotStatusDto {
            configured: false,
            token_storage: "none".to_string(),
            keychain_error: false,
            gateway: None,
        },
        Err(TokenStoreError::NoStore) => BotStatusDto {
            configured: false,
            token_storage: "none".to_string(),
            keychain_error: false,
            gateway: None,
        },
        Err(_) => BotStatusDto {
            configured: false,
            token_storage: "none".to_string(),
            keychain_error: true,
            gateway: None,
        },
    }
}

fn apply_clear(
    keychain: Result<(), TokenStoreError>,
    session: &dyn TokenStore,
    settings: &mut SettingsDto,
) -> Result<(), String> {
    match keychain {
        Ok(()) | Err(TokenStoreError::NoStore) => {}
        Err(error) => {
            // The in-memory token is gone, but a keychain token may remain.
            // Keep the mapping and the configured state so the host can retry
            // after unlocking the keychain.
            session.delete().map_err(|store| store.message())?;
            return Err(error.message());
        }
    }
    session.delete().map_err(|error| error.message())?;
    clear_bot_fields(settings);
    Ok(())
}

#[cfg(test)]
fn clear_setup(
    keychain: &dyn TokenStore,
    session: &dyn TokenStore,
    settings: &mut SettingsDto,
) -> Result<(), String> {
    apply_clear(keychain.delete(), session, settings)
}

fn clear_bot_fields(settings: &mut SettingsDto) {
    settings.bot_configured = false;
    settings.bot_identity.clear();
    settings.token_storage = "none".to_string();
    settings.guild_id.clear();
    settings.guild_name.clear();
    settings.announcement_channel_id.clear();
    settings.announcement_channel_name.clear();
    settings.reveal_channel_id.clear();
    settings.reveal_channel_name.clear();
}

#[tauri::command]
pub async fn host_bot_validate(token: String) -> Result<BotPreviewDto, String> {
    preview(directory().as_ref(), &token).await
}

#[tauri::command]
pub async fn host_bot_guilds(token: String) -> Result<Vec<GuildOptionDto>, String> {
    guild_options(directory().as_ref(), &token).await
}

#[tauri::command]
pub async fn host_bot_channels(
    token: String,
    guild_id: String,
) -> Result<Vec<ChannelOptionDto>, String> {
    channel_options(directory().as_ref(), &token, &guild_id).await
}

#[tauri::command]
pub async fn host_bot_save(
    app: AppHandle,
    runtime: State<'_, BotRuntime>,
    input: BotSetupInput,
) -> Result<SettingsDto, String> {
    let mut settings = host::read_settings(&app)?;
    save_setup(
        directory().as_ref(),
        &KeychainStore,
        &runtime.session,
        &mut settings,
        &input,
    )
    .await?;
    if let Err(error) = host::write_settings(&app, &settings) {
        let _ = KeychainStore.delete();
        runtime.clear_session();
        return Err(error);
    }
    #[cfg(feature = "live")]
    {
        let live = app.state::<crate::live_runtime::LiveRuntime>();
        live.stop().await?;
        if let Some(token) = runtime.current_token()?
            && !settings.guild_id.is_empty()
        {
            live.start(
                &app,
                token,
                settings.guild_id.clone(),
                Some(settings.announcement_channel_id.clone()),
                settings.dm_fallback,
            )?;
        }
    }
    Ok(settings)
}

#[tauri::command]
pub async fn host_bot_test_dm(
    runtime: State<'_, BotRuntime>,
    user_id: String,
) -> Result<String, String> {
    let token = runtime
        .read_token(&KeychainStore)?
        .ok_or_else(|| "Save the bot token before you send a test message.".to_string())?;
    let port = live_port(&token)?;
    test_dm(port.as_ref(), &user_id).await
}

pub async fn status(runtime: &BotRuntime, persisted_storage: &str) -> Result<BotStatusDto, String> {
    let session = runtime.session.get().map_err(|error| error.message())?;
    if session.is_some() {
        return Ok(status_from(Ok(None), session));
    }
    let keychain = tauri::async_runtime::spawn_blocking(|| KeychainStore.get())
        .await
        .map_err(|error| error.to_string())?;
    if persisted_storage == "session" && matches!(keychain, Ok(Some(_))) {
        // A previous session-only save could not delete the old keychain
        // entry. Clean it instead of letting it resurface after restart.
        let _ = tauri::async_runtime::spawn_blocking(|| KeychainStore.delete()).await;
        return Ok(status_from(Ok(None), None));
    }
    Ok(status_from(keychain, session))
}

#[tauri::command]
pub async fn host_bot_status(
    app: AppHandle,
    runtime: State<'_, BotRuntime>,
) -> Result<BotStatusDto, String> {
    let settings = host::read_settings(&app).unwrap_or_default();
    let mut status = status(&runtime, &settings.token_storage).await?;
    #[cfg(feature = "live")]
    {
        status.gateway = app
            .state::<crate::live_runtime::LiveRuntime>()
            .health()
            .map(|health| GatewayStatusDto {
                state: health.state.as_str().to_string(),
                attempt: health.attempt,
                last_error: health.last_error,
                connected_at: health.connected_at,
                handler_errors: health.handler_errors,
            });
    }
    Ok(status)
}

#[tauri::command]
pub async fn host_bot_clear(
    app: AppHandle,
    runtime: State<'_, BotRuntime>,
) -> Result<SettingsDto, String> {
    let mut settings = host::read_settings(&app).unwrap_or_default();
    let keychain = tauri::async_runtime::spawn_blocking(|| KeychainStore.delete())
        .await
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "live")]
    {
        app.state::<crate::live_runtime::LiveRuntime>()
            .stop()
            .await?;
    }
    apply_clear(keychain, &runtime.session, &mut settings)?;
    host::write_settings(&app, &settings)?;
    Ok(settings)
}

/// A session token cannot survive a restart. Clear the stale session label
/// before the first window loads.
pub fn normalize_stale_session(app: &AppHandle) {
    let Ok(mut settings) = host::read_settings(app) else {
        return;
    };
    if settings.token_storage == "session" {
        clear_bot_fields(&mut settings);
        let _ = host::write_settings(app, &settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use swap_discord::{ChannelSummary, FakeDiscord, GuildSummary, SetupError};

    struct FakeDirectory {
        identity: BotIdentity,
        guilds: Vec<GuildSummary>,
        channels: Vec<ChannelSummary>,
        fail_validation: bool,
    }

    impl FakeDirectory {
        fn valid() -> Self {
            Self {
                identity: BotIdentity {
                    id: "900".to_string(),
                    username: "Monsquaz Bot".to_string(),
                },
                guilds: vec![GuildSummary {
                    id: "10".to_string(),
                    name: "Night Shift".to_string(),
                }],
                channels: vec![
                    ChannelSummary {
                        id: "11".to_string(),
                        name: "swap-announcements".to_string(),
                    },
                    ChannelSummary {
                        id: "12".to_string(),
                        name: "swap-reveal".to_string(),
                    },
                    ChannelSummary {
                        id: "13".to_string(),
                        name: "swap-chat".to_string(),
                    },
                ],
                fail_validation: false,
            }
        }
    }

    #[async_trait]
    impl SetupDirectory for FakeDirectory {
        async fn validate_token(&self, _token: &str) -> Result<BotIdentity, SetupError> {
            if self.fail_validation {
                return Err(SetupError::TokenRejected);
            }
            Ok(self.identity.clone())
        }

        async fn guilds(&self, _token: &str) -> Result<Vec<GuildSummary>, SetupError> {
            Ok(self.guilds.clone())
        }

        async fn channels(
            &self,
            _token: &str,
            _guild_id: &str,
        ) -> Result<Vec<ChannelSummary>, SetupError> {
            Ok(self.channels.clone())
        }

        async fn send_test_dm(&self, _token: &str, _user_id: &str) -> Result<String, SetupError> {
            Ok("m1".to_string())
        }
    }

    struct MemoryStore {
        token: Mutex<Option<String>>,
        fail_set: bool,
        delete_error: Option<TokenStoreError>,
    }

    impl MemoryStore {
        fn failing() -> Self {
            Self {
                token: Mutex::new(None),
                fail_set: true,
                delete_error: Some(TokenStoreError::Unavailable),
            }
        }

        fn no_store() -> Self {
            Self {
                token: Mutex::new(None),
                fail_set: true,
                delete_error: Some(TokenStoreError::NoStore),
            }
        }

        fn token(&self) -> Option<String> {
            self.token.lock().unwrap().clone()
        }
    }

    impl Default for MemoryStore {
        fn default() -> Self {
            Self {
                token: Mutex::new(None),
                fail_set: false,
                delete_error: None,
            }
        }
    }

    impl TokenStore for MemoryStore {
        fn get(&self) -> Result<Option<String>, TokenStoreError> {
            if self.fail_set {
                return Err(TokenStoreError::Unavailable);
            }
            Ok(self.token())
        }

        fn set(&self, token: &str) -> Result<(), TokenStoreError> {
            if self.fail_set {
                return Err(TokenStoreError::NoStore);
            }
            *self.token.lock().unwrap() = Some(token.to_string());
            Ok(())
        }

        fn delete(&self) -> Result<(), TokenStoreError> {
            if let Some(error) = &self.delete_error {
                return Err(error.clone());
            }
            *self.token.lock().unwrap() = None;
            Ok(())
        }
    }

    fn token() -> String {
        format!("{}.{}.{}", "a".repeat(24), "b".repeat(6), "c".repeat(27))
    }

    fn input() -> BotSetupInput {
        BotSetupInput {
            token: token(),
            guild_id: "10".to_string(),
            announcement_channel_id: "11".to_string(),
            reveal_channel_id: "12".to_string(),
            allow_session_only: false,
        }
    }

    fn settings() -> SettingsDto {
        SettingsDto::default()
    }

    #[tokio::test]
    async fn validation_returns_the_identity_and_invite_url() {
        let directory = FakeDirectory::valid();
        let preview = preview(&directory, &token()).await.unwrap();
        assert_eq!(preview.identity, "Monsquaz Bot (900)");
        assert!(preview.invite_url.contains("client_id=900"));
        assert!(
            preview
                .invite_url
                .contains("scope=bot%20applications.commands")
        );
    }

    #[tokio::test]
    async fn a_rejected_token_is_reported() {
        let mut directory = FakeDirectory::valid();
        directory.fail_validation = true;
        assert_eq!(
            preview(&directory, &token()).await.unwrap_err(),
            "the bot token is not valid".to_string()
        );
    }

    // scenario: The bot token is never shown again
    // scenario: The token is stored in the OS keychain
    #[tokio::test]
    async fn saving_stores_the_token_outside_the_settings() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::default();
        let session = MemoryStore::default();
        let mut settings = settings();

        save_setup(&directory, &keychain, &session, &mut settings, &input())
            .await
            .unwrap();

        assert_eq!(keychain.token(), Some(token()));
        assert_eq!(settings.token_storage, "keychain");
        assert_eq!(settings.guild_name, "Night Shift");
        assert_eq!(settings.announcement_channel_name, "swap-announcements");
        assert_eq!(settings.reveal_channel_name, "swap-reveal");
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains(&token()));
        assert!(!json.contains("aaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    // scenario: Linux without a secret service
    #[tokio::test]
    async fn a_keychain_failure_never_stores_plaintext() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::failing();
        let session = MemoryStore::default();
        let mut settings = settings();

        let result = save_setup(&directory, &keychain, &session, &mut settings, &input()).await;
        assert!(result.is_err());
        assert!(!settings.bot_configured);
        assert_eq!(settings.token_storage, "none");
        assert!(keychain.token().is_none());
        assert!(session.token().is_none());
    }

    // scenario: Linux without a secret service
    #[tokio::test]
    async fn an_explicit_session_fallback_keeps_the_token_in_memory() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::failing();
        let session = MemoryStore::default();
        let mut settings = settings();
        let input = BotSetupInput {
            allow_session_only: true,
            ..input()
        };

        save_setup(&directory, &keychain, &session, &mut settings, &input)
            .await
            .unwrap();
        assert_eq!(session.token(), Some(token()));
        assert_eq!(settings.token_storage, "session");
        assert_eq!(settings.bot_identity, "Monsquaz Bot (900)");
    }

    #[tokio::test]
    async fn an_explicit_session_choice_wins_over_a_working_keychain() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::default();
        keychain.set("old-keychain-token").unwrap();
        let session = MemoryStore::default();
        let mut settings = settings();
        let input = BotSetupInput {
            allow_session_only: true,
            ..input()
        };

        save_setup(&directory, &keychain, &session, &mut settings, &input)
            .await
            .unwrap();

        assert_eq!(session.token(), Some(token()));
        assert!(keychain.token().is_none());
        assert_eq!(settings.token_storage, "session");
    }

    #[tokio::test]
    async fn an_unknown_guild_is_refused_before_the_token_is_stored() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::default();
        let session = MemoryStore::default();
        let mut settings = settings();
        let input = BotSetupInput {
            guild_id: "99".to_string(),
            ..input()
        };

        let result = save_setup(&directory, &keychain, &session, &mut settings, &input).await;
        assert_eq!(
            result,
            Err("Choose a server that the bot can join.".to_string())
        );
        assert!(keychain.token().is_none());
    }

    // scenario: The host maps the announcement and reveal channels
    #[tokio::test]
    async fn the_selected_channels_must_belong_to_the_guild() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::default();
        let session = MemoryStore::default();
        let mut settings = settings();
        let input = BotSetupInput {
            reveal_channel_id: "99".to_string(),
            ..input()
        };

        let result = save_setup(&directory, &keychain, &session, &mut settings, &input).await;
        assert_eq!(
            result,
            Err("Choose a reveal channel in this server.".to_string())
        );
        assert!(keychain.token().is_none());
    }

    // scenario: The host maps the announcement and reveal channels
    #[tokio::test]
    async fn announcements_and_the_reveal_must_differ() {
        let directory = FakeDirectory::valid();
        let keychain = MemoryStore::default();
        let session = MemoryStore::default();
        let mut settings = settings();
        let input = BotSetupInput {
            reveal_channel_id: "11".to_string(),
            ..input()
        };

        let result = save_setup(&directory, &keychain, &session, &mut settings, &input).await;
        assert!(result.unwrap_err().contains("two different channels"));
    }

    #[tokio::test]
    async fn a_test_dm_goes_through_the_port() {
        let discord = FakeDiscord::new();
        let message_id = test_dm(&discord, "42").await.unwrap();
        assert_eq!(message_id, "m1");
        assert_eq!(discord.sent_dms().len(), 1);
        assert_eq!(discord.sent_dms()[0].user_id, DiscordUserId::new("42"));
    }

    #[tokio::test]
    async fn a_test_dm_rejects_a_non_numeric_user_id() {
        let discord = FakeDiscord::new();
        assert!(test_dm(&discord, "not-a-user").await.is_err());
        assert!(discord.sent_dms().is_empty());
    }

    #[test]
    fn clearing_a_session_survives_a_missing_keychain() {
        let keychain = MemoryStore::no_store();
        let session = MemoryStore::default();
        session.set("session-token").unwrap();
        let mut settings = settings();
        settings.bot_configured = true;
        settings.token_storage = "session".to_string();

        clear_setup(&keychain, &session, &mut settings).unwrap();

        assert!(session.token().is_none());
        assert!(!settings.bot_configured);
        assert_eq!(settings.token_storage, "none");
    }

    #[test]
    fn clearing_a_session_with_a_locked_keychain_keeps_the_mapping_for_retry() {
        let keychain = MemoryStore::failing();
        let session = MemoryStore::default();
        session.set("session-token").unwrap();
        let mut settings = settings();
        settings.bot_configured = true;
        settings.token_storage = "session".to_string();
        settings.guild_id = "10".to_string();

        let result = clear_setup(&keychain, &session, &mut settings);

        assert!(result.is_err());
        assert!(session.token().is_none());
        assert!(settings.bot_configured);
        assert_eq!(settings.token_storage, "session");
        assert_eq!(settings.guild_id, "10");
    }

    #[test]
    fn clearing_a_keychain_connection_keeps_the_state_when_the_store_is_locked() {
        let keychain = MemoryStore::failing();
        let session = MemoryStore::default();
        let mut settings = settings();
        settings.bot_configured = true;
        settings.token_storage = "keychain".to_string();
        settings.guild_id = "10".to_string();

        let result = clear_setup(&keychain, &session, &mut settings);

        assert!(result.is_err());
        assert!(settings.bot_configured);
        assert_eq!(settings.token_storage, "keychain");
        assert_eq!(settings.guild_id, "10");
    }

    #[test]
    fn clearing_removes_every_bot_field_and_both_tokens() {
        let keychain = MemoryStore::default();
        keychain.set(&token()).unwrap();
        let session = MemoryStore::default();
        session.set("session-token").unwrap();
        let mut settings = settings();
        settings.bot_configured = true;
        settings.bot_identity = "Monsquaz Bot (900)".to_string();
        settings.token_storage = "keychain".to_string();
        settings.guild_id = "10".to_string();
        settings.guild_name = "Night Shift".to_string();
        settings.announcement_channel_id = "11".to_string();
        settings.announcement_channel_name = "swap-announcements".to_string();
        settings.reveal_channel_id = "12".to_string();
        settings.reveal_channel_name = "swap-reveal".to_string();

        clear_setup(&keychain, &session, &mut settings).unwrap();

        assert!(keychain.token().is_none());
        assert!(session.token().is_none());
        assert!(!settings.bot_configured);
        assert!(settings.bot_identity.is_empty());
        assert_eq!(settings.token_storage, "none");
        assert!(settings.guild_id.is_empty());
        assert!(settings.announcement_channel_id.is_empty());
        assert!(settings.reveal_channel_id.is_empty());
    }

    #[test]
    fn the_status_reflects_the_active_store() {
        assert_eq!(status_from(Ok(None), None).token_storage, "none");
        assert!(!status_from(Ok(None), None).configured);
        assert_eq!(
            status_from(Ok(None), Some("session".to_string())).token_storage,
            "session"
        );
        assert_eq!(
            status_from(Ok(Some("keychain".to_string())), None).token_storage,
            "keychain"
        );
        assert_eq!(
            status_from(
                Ok(Some("keychain".to_string())),
                Some("session".to_string())
            )
            .token_storage,
            "session"
        );
        let unavailable = status_from(Err(TokenStoreError::Unavailable), None);
        assert!(unavailable.keychain_error);
        assert!(!unavailable.configured);
        let no_store = status_from(Err(TokenStoreError::NoStore), None);
        assert!(!no_store.keychain_error);
        assert!(!no_store.configured);
    }

    #[test]
    fn the_session_token_takes_precedence() {
        let runtime = BotRuntime::new();
        let keychain = MemoryStore::default();
        keychain.set("keychain-token").unwrap();
        runtime.session.set("session-token").unwrap();
        assert_eq!(
            runtime.read_token(&keychain).unwrap(),
            Some("session-token".to_string())
        );
        runtime.clear_session();
        assert_eq!(
            runtime.read_token(&keychain).unwrap(),
            Some("keychain-token".to_string())
        );
    }

    #[test]
    fn options_are_sorted_for_the_host() {
        let directory = FakeDirectory::valid();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let channels = runtime
            .block_on(channel_options(&directory, &token(), "10"))
            .unwrap();
        let names = channels
            .iter()
            .map(|channel| channel.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["swap-announcements", "swap-chat", "swap-reveal"]
        );
    }
}
