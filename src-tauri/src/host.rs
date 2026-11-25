use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionControlDto {
    first_run: bool,
    event: Option<EventSummaryDto>,
    bot: BotHealthDto,
    next_action: Option<NextActionDto>,
    decisions: Vec<DecisionDto>,
    activity: Vec<ActivityDto>,
    timeline: Vec<TimelineStepDto>,
    facts: Vec<EventFactDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventSummaryDto {
    id: String,
    name: String,
    mode: String,
    stage: String,
    participants: usize,
    seeded: usize,
    fill_ins: usize,
    tracks: usize,
    round: u32,
    rounds: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BotHealthDto {
    status: String,
    label: String,
    detail: String,
    last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NextActionDto {
    eyebrow: String,
    title: String,
    detail: String,
    action_label: String,
    secondary_label: Option<String>,
    tone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionDto {
    id: String,
    kind: String,
    title: String,
    detail: String,
    due: String,
    action_label: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityDto {
    id: String,
    at: String,
    actor: String,
    action: String,
    subject: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineStepDto {
    id: String,
    label: String,
    caption: String,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventFactDto {
    label: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryDto {
    id: String,
    at: String,
    actor: String,
    action: String,
    subject: String,
    reason: String,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsDto {
    deadlines: bool,
    bot_offline: bool,
    releases: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    bot_configured: bool,
    bot_identity: String,
    guild_name: String,
    announcement_channel: String,
    reveal_channel: String,
    dm_fallback: bool,
    timezone: String,
    retention_days: u32,
    theme: String,
    notifications: NotificationsDto,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            bot_configured: false,
            bot_identity: String::new(),
            guild_name: String::new(),
            announcement_channel: String::new(),
            reveal_channel: String::new(),
            dm_fallback: true,
            timezone: "Europe/Stockholm".to_string(),
            retention_days: 90,
            theme: "dark".to_string(),
            notifications: NotificationsDto {
                deadlines: true,
                bot_offline: true,
                releases: true,
            },
        }
    }
}

static TRAY_READY: AtomicBool = AtomicBool::new(false);

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Monsquaz Swap", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".to_string()))?;
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Monsquaz Swap")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    TRAY_READY.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn tray_ready() -> bool {
    TRAY_READY.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn host_mission_control() -> MissionControlDto {
    MissionControlDto {
        first_run: true,
        event: None,
        bot: BotHealthDto {
            status: "Not configured".to_string(),
            label: "No bot connected".to_string(),
            detail: "Connect a bot token to send offers and collect files.".to_string(),
            last_seen: None,
        },
        next_action: None,
        decisions: Vec::new(),
        activity: Vec::new(),
        timeline: Vec::new(),
        facts: Vec::new(),
    }
}

#[tauri::command]
pub fn host_audit_log() -> Vec<AuditEntryDto> {
    Vec::new()
}

#[tauri::command]
pub fn host_settings(app: AppHandle) -> Result<SettingsDto, String> {
    let path = settings_path(&app)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SettingsDto::default()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn save_host_settings(app: AppHandle, settings: SettingsDto) -> Result<SettingsDto, String> {
    validate_settings(&settings)?;
    let path = settings_path(&app)?;
    let contents = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())?;
    Ok(settings)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("host-settings.json"))
}

fn validate_settings(settings: &SettingsDto) -> Result<(), String> {
    if !(1..=3650).contains(&settings.retention_days) {
        return Err("retention_days must be between 1 and 3650".to_string());
    }
    if !matches!(settings.theme.as_str(), "dark" | "light" | "system") {
        return Err("theme must be dark, light, or system".to_string());
    }
    if settings.timezone.trim().is_empty() || settings.timezone.chars().count() > 64 {
        return Err("timezone must be between 1 and 64 characters".to_string());
    }
    for (label, value) in [
        ("guild name", &settings.guild_name),
        ("announcement channel", &settings.announcement_channel),
        ("reveal channel", &settings.reveal_channel),
        ("bot identity", &settings.bot_identity),
    ] {
        if value.chars().count() > 200 {
            return Err(format!("{label} must be at most 200 characters"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_are_valid() {
        validate_settings(&SettingsDto::default()).unwrap();
    }

    #[test]
    fn bad_settings_are_refused() {
        let invalid_retention = SettingsDto {
            retention_days: 0,
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_retention).is_err());

        let invalid_theme = SettingsDto {
            theme: "neon".to_string(),
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_theme).is_err());

        let invalid_timezone = SettingsDto {
            timezone: " ".to_string(),
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_timezone).is_err());
    }

    #[test]
    fn the_settings_payload_has_no_secret_field() {
        let json = serde_json::to_string(&SettingsDto::default()).unwrap();
        assert!(json.contains("\"botConfigured\""));
        assert!(json.contains("\"retentionDays\""));
        assert!(!json.contains("token"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn the_first_run_payload_matches_the_ui_contract() {
        let json = serde_json::to_value(host_mission_control()).unwrap();
        assert_eq!(json["firstRun"], true);
        assert_eq!(json["event"], serde_json::Value::Null);
        assert_eq!(json["bot"]["status"], "Not configured");
        assert!(json["decisions"].as_array().unwrap().is_empty());
        assert!(json["timeline"].as_array().unwrap().is_empty());
    }
}
