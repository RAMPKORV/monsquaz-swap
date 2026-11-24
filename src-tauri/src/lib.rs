use serde::Serialize;
use swap_core::CORE_VERSION;

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
        .invoke_handler(tauri::generate_handler![health])
        .run(tauri::generate_context!())
        .expect("error while running Monsquaz Swap");
}
