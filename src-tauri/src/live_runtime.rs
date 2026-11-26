//! Live Discord runtime: one gateway, one inbound service, one delivery worker.
//!
//! The runtime opens the same SQLite database path as the host commands. The
//! outbox and event state survive an app restart.

use std::sync::{Arc, Mutex};

use swap_db::Db;
use swap_discord::{HttpAttachmentFetcher, HttpDiscordPort};
use swap_engine::{InboundService, OpenMptPreparer, SystemClock};
use swap_files::BlobStore;
use tauri::{AppHandle, Manager};

use crate::gateway_host::GatewayHost;

struct Running {
    gateway: GatewayHost,
}

#[derive(Default)]
pub struct LiveRuntime {
    inner: Mutex<Option<Running>>,
}

impl LiveRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub fn start(
        &self,
        app: &AppHandle,
        token: String,
        guild_id: String,
        alert_channel_id: Option<String>,
        dm_fallback: bool,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("the Discord gateway is already running".to_string());
        }
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let db = Arc::new(Mutex::new(
            Db::open(directory.join("monsquaz.db")).map_err(|error| error.to_string())?,
        ));
        let store = BlobStore::new(directory.join("files"));
        let preparer = Arc::new(
            OpenMptPreparer::discover()
                .map_err(|error| format!("openmpt123 is not available: {error}"))?,
        );
        let discord = Arc::new(HttpDiscordPort::new(&token).map_err(|error| error.to_string())?);
        let fetcher = Arc::new(HttpAttachmentFetcher::new().map_err(|error| error.to_string())?);
        let fallback_channel = if dm_fallback {
            alert_channel_id.clone()
        } else {
            None
        };
        let service = Arc::new(
            InboundService::new(db, store, preparer, discord, fetcher, Arc::new(SystemClock))
                .with_alert_channel(alert_channel_id)
                .with_fallback_channel(fallback_channel),
        );
        let gateway = GatewayHost::start(token, guild_id, service.clone())?;
        *self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Running { gateway });
        tauri::async_runtime::spawn(async move {
            let _ = service.backfill().await;
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let running = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(running) = running {
            running.gateway.shutdown().await?;
        }
        Ok(())
    }
}
