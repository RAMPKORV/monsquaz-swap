use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use swap_discord::{
    DiscordGuildId, GatewayError, GatewayHealth, GatewayState, InboundHandler, SharedHealth,
    Supervisor, TwilightConfig, connector,
};
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::watch;

/// Owns the live gateway task on the Tauri runtime.
///
/// The setup service starts this host after it reads the token from the OS
/// keychain. The token never enters the UI after setup.
struct FinishGuard(Arc<AtomicBool>);

impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

pub struct GatewayHost {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<(), GatewayError>>,
    health: Arc<SharedHealth>,
    finished: Arc<AtomicBool>,
}

impl GatewayHost {
    pub fn start(
        token: String,
        guild_id: String,
        handler: Arc<dyn InboundHandler>,
    ) -> Result<Self, String> {
        let config = TwilightConfig::new(token, DiscordGuildId::new(guild_id));
        let connector = connector(config).map_err(|error| error.to_string())?;
        let health = Arc::new(SharedHealth::new());
        let supervisor = Supervisor::new(connector, handler).with_health(health.clone());
        let (stop, stop_rx) = watch::channel(false);
        let finished = Arc::new(AtomicBool::new(false));
        let task_finished = finished.clone();
        let task = spawn(async move {
            let _finished = FinishGuard(task_finished);
            supervisor.run(stop_rx).await
        });
        Ok(Self {
            stop,
            task,
            health,
            finished,
        })
    }

    pub fn health(&self) -> GatewayHealth {
        let mut health = self.health.snapshot();
        if self.finished.load(Ordering::SeqCst)
            && !matches!(health.state, GatewayState::Stopped | GatewayState::Failed)
        {
            health.state = GatewayState::Failed;
            health.last_error = Some("the gateway task stopped unexpectedly".to_string());
        }
        health
    }

    pub async fn shutdown(self) -> Result<(), String> {
        let _ = self.stop.send(true);
        let mut task = self.task;
        match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => {
                eprintln!("the Discord gateway stopped with an error: {error}");
                Ok(())
            }
            Ok(Err(error)) => {
                eprintln!("the Discord gateway task ended: {error}");
                Ok(())
            }
            Err(_) => {
                task.abort();
                Err("the Discord gateway did not stop within 5 seconds".to_string())
            }
        }
    }
}
