mod host;

use serde::Serialize;
use swap_core::CORE_VERSION;
use tauri::WindowEvent;

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
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if let Err(error) = host::setup_tray(app) {
                eprintln!("tray unavailable: {error}");
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
            host::host_create_event
        ])
        .run(tauri::generate_context!())
        .expect("error while running Monsquaz Swap");
}
