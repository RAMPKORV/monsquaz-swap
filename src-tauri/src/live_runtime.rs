//! Live Discord runtime: one gateway, one inbound service, one delivery worker.
//!
//! The runtime opens the same SQLite database path as the host commands. The
//! outbox and event state survive an app restart.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use swap_db::Db;
use swap_discord::{HttpAttachmentFetcher, HttpDiscordPort};
use swap_engine::{InboundService, OpenMptPreparer, SystemClock};
use swap_files::BlobStore;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use crate::gateway_host::GatewayHost;

struct Running {
    gateway: GatewayHost,
    stop: watch::Sender<bool>,
    dispatcher: tauri::async_runtime::JoinHandle<()>,
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
        let mut slot = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_some() {
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
        service.mark_backfill_pending();
        let gateway = GatewayHost::start(token, guild_id, service.clone())?;
        let (stop, stop_rx) = watch::channel(false);
        let task_service = service.clone();
        let wake = service.wake_notifier();
        let dispatcher = tauri::async_runtime::spawn(async move {
            let _ = task_service.drain_pending().await;
            let _ = task_service.backfill().await;
            let drain_service = task_service;
            run_dispatch_loop(Duration::from_secs(30), wake, stop_rx, move || {
                let service = drain_service.clone();
                async move {
                    let _ = service.retry_backfill().await;
                    let _ = service.drain_pending().await;
                }
            })
            .await;
        });
        *slot = Some(Running {
            gateway,
            stop,
            dispatcher,
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
            let _ = running.stop.send(true);
            running.dispatcher.abort();
            running.gateway.shutdown().await?;
        }
        Ok(())
    }
}

async fn run_dispatch_loop<F, Fut>(
    interval: Duration,
    wake: Arc<tokio::sync::Notify>,
    mut stop: watch::Receiver<bool>,
    mut drain: F,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    loop {
        tokio::select! {
            result = stop.changed() => {
                if result.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = wake.notified() => {
                drain().await;
            }
            _ = tokio::time::sleep(interval) => {
                drain().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::sync::{Notify, watch};

    use super::run_dispatch_loop;

    #[tokio::test]
    async fn the_dispatch_loop_drains_on_wake_and_stops() {
        let wake = Arc::new(Notify::new());
        let (stop, stop_rx) = watch::channel(false);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        let handle = tokio::spawn(run_dispatch_loop(
            Duration::from_secs(30),
            wake.clone(),
            stop_rx,
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            },
        ));
        wake.notify_one();
        tokio::time::timeout(Duration::from_secs(2), async {
            while ticks.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        stop.send(true).unwrap();
        handle.await.unwrap();
        assert_eq!(ticks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_dispatch_loop_drains_on_the_interval() {
        let wake = Arc::new(Notify::new());
        let (stop, stop_rx) = watch::channel(false);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        let handle = tokio::spawn(run_dispatch_loop(
            Duration::from_millis(10),
            wake,
            stop_rx,
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            },
        ));
        tokio::time::timeout(Duration::from_secs(2), async {
            while ticks.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        stop.send(true).unwrap();
        handle.await.unwrap();
        assert!(ticks.load(Ordering::SeqCst) >= 1);
    }
}
