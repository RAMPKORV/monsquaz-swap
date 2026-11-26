// The setup service starts this host after it reads the keychain token.
#[cfg(feature = "live")]
mod gateway_host;
mod host;
#[cfg(feature = "live")]
mod live_runtime;
mod setup_host;

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
fn health() -> Health {
    Health {
        status: "ok",
        app_version: env!("CARGO_PKG_VERSION"),
        core_version: CORE_VERSION,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(setup_host::BotRuntime::new());
    #[cfg(feature = "live")]
    let builder = builder.manage(live_runtime::LiveRuntime::new());
    builder
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
            host::save_host_settings,
            host::host_event_setup,
            host::host_people,
            host::host_set_participant_paused,
            host::host_plan,
            host::host_generate_plan,
            host::host_start_event,
            host::host_create_event,
            setup_host::host_bot_validate,
            setup_host::host_bot_guilds,
            setup_host::host_bot_channels,
            setup_host::host_bot_save,
            setup_host::host_bot_test_dm,
            setup_host::host_bot_status,
            setup_host::host_bot_clear
        ])
        .run(tauri::generate_context!())
        .expect("error while running Monsquaz Swap");
}
