use std::sync::Arc;
use std::time::Duration;

use swap_discord::{
    DiscordGuildId, GatewayError, GatewayHealth, InboundHandler, SharedHealth, Supervisor,
    TwilightConfig, connector,
};
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::watch;

/// Owns the live gateway task on the Tauri runtime.
///
/// The setup service starts this host after it reads the token from the OS
/// keychain. The token never enters the UI after setup.
pub struct GatewayHost {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<(), GatewayError>>,
    health: Arc<SharedHealth>,
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
        let task = spawn(async move { supervisor.run(stop_rx).await });
        Ok(Self { stop, task, health })
    }

    pub fn health(&self) -> GatewayHealth {
        self.health.snapshot()
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
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => {
                task.abort();
                Err("the Discord gateway did not stop within 5 seconds".to_string())
            }
        }
    }
}
