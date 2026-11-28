// The setup service starts this host after it reads the keychain token.
#[cfg(feature = "live")]
mod gateway_host;
mod host;
#[cfg(feature = "live")]
mod live_runtime;
mod preview;
mod setup_host;

#[cfg(feature = "e2e")]
pub use host::e2e_api;

use serde::Serialize;
use swap_core::CORE_VERSION;
use tauri::{Manager, WindowEvent};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    status: &'static str,
    app_version: &'static str,
    core_version: &'static str,
}

#[tauri::command]
async fn save_host_settings(
    app: tauri::AppHandle,
    settings: host::SettingsDto,
) -> Result<host::SettingsDto, String> {
    let before = host::read_settings(&app).unwrap_or_default();
    let _restart = before.dm_fallback != settings.dm_fallback;
    #[cfg(feature = "live")]
    let restart_token = if _restart {
        let live = app.state::<live_runtime::LiveRuntime>();
        if live.is_running() {
            let token = app
                .state::<setup_host::BotRuntime>()
                .current_token()
                .map_err(|error| format!("the bot token is not available: {error}"))?
                .ok_or_else(|| "the bot token is not available".to_string())?;
            Some(token)
        } else {
            None
        }
    } else {
        None
    };
    let saved = host::save_settings_values(app.clone(), settings)?;
    #[cfg(feature = "live")]
    if let Some(token) = restart_token {
        let live = app.state::<live_runtime::LiveRuntime>();
        live.stop()
            .await
            .map_err(|error| format!("settings saved, but the gateway did not stop: {error}"))?;
        if let Err(error) = live.start(
            &app,
            token.clone(),
            saved.guild_id.clone(),
            Some(saved.announcement_channel_id.clone()),
            saved.dm_fallback,
        ) {
            let rollback = live.start(
                &app,
                token,
                before.guild_id.clone(),
                Some(before.announcement_channel_id.clone()),
                before.dm_fallback,
            );
            return Err(match rollback {
                Ok(()) => format!(
                    "settings saved, but the gateway did not start with the new fallback: {error}. The old fallback is active."
                ),
                Err(rollback_error) => format!(
                    "settings saved, but the gateway is offline: {error}. The rollback failed: {rollback_error}"
                ),
            });
        }
    }
    Ok(saved)
}

#[tauri::command]
fn health() -> Health {
    Health {
        status: "ok",
        app_version: env!("CARGO_PKG_VERSION"),
        core_version: CORE_VERSION,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = preview::register_protocol(tauri::Builder::default());
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(setup_host::BotRuntime::new());
    #[cfg(feature = "live")]
    let builder = builder.manage(live_runtime::LiveRuntime::new());
    let app = builder
        .setup(|app| {
            setup_host::normalize_stale_session(app.handle());
            if let Err(error) = host::setup_tray(app) {
                eprintln!("tray unavailable: {error}");
            }
            #[cfg(feature = "live")]
            {
                let settings = host::read_settings(app.handle()).unwrap_or_default();
                let token = app
                    .state::<setup_host::BotRuntime>()
                    .current_token()
                    .ok()
                    .flatten();
                if let Some(token) = token
                    && !settings.guild_id.is_empty()
                    && let Err(error) = app.state::<live_runtime::LiveRuntime>().start(
                        app.handle(),
                        token,
                        settings.guild_id.clone(),
                        Some(settings.announcement_channel_id.clone()),
                        settings.dm_fallback,
                    )
                {
                    eprintln!("Discord gateway did not start: {error}");
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && host::tray_ready()
            {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            health,
            host::host_mission_control,
            host::host_audit_log,
            host::host_settings,
            save_host_settings,
            host::host_event_setup,
            host::host_people,
            host::host_add_availability,
            host::host_remove_availability,
            host::host_set_participant_paused,
            host::host_mark_participant_left,
            host::host_restore_participant,
            host::host_plan,
            host::host_generate_plan,
            host::host_repair_groups,
            host::host_async_dispatch,
            host::host_plan_edit_candidates,
            host::host_edit_plan_assignment,
            host::host_lock_roster,
            host::host_upload_pack,
            host::host_open_seed_round,
            host::host_extend_seed_deadline,
            host::host_reopen_seed_round,
            host::host_pending_late_seeds,
            host::host_decide_late_seed,
            host::host_move_round_deadline,
            host::host_verify_commitment,
            host::host_open_round,
            host::host_reveal,
            host::host_reveal_package,
            host::host_export_reveal_zip,
            host::host_fill_in_candidates,
            host::host_assign_fill_in,
            host::host_start_fill_in_list,
            host::host_set_track_title,
            host::host_add_track_comment,
            host::host_redact_track_comment,
            host::host_decide_extension,
            host::host_decide_deadline,
            host::host_decide_expiry,
            host::host_start_event,
            host::host_start_async_event,
            host::host_create_event,
            setup_host::host_bot_validate,
            setup_host::host_bot_guilds,
            setup_host::host_bot_channels,
            setup_host::host_bot_save,
            setup_host::host_bot_test_dm,
            setup_host::host_bot_status,
            setup_host::host_bot_clear
        ])
        .build(tauri::generate_context!())
        .expect("error while building Monsquaz Swap");
    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            #[cfg(feature = "live")]
            {
                let live = app_handle.state::<live_runtime::LiveRuntime>();
                if live.is_running()
                    && let Err(error) = tauri::async_runtime::block_on(live.stop())
                {
                    eprintln!("Discord gateway did not stop: {error}");
                }
            }
            #[cfg(not(feature = "live"))]
            let _ = app_handle;
        }
    });
}
