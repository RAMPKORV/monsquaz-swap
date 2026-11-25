//! Live gateway smoke test.
//!
//! Run with `DISCORD_TOKEN` and `DISCORD_GUILD` set. The example prints event
//! counts only. It never prints the token or a message body.

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use swap_discord::{
    DiscordGuildId, GatewayState, InboundEvent, InboundHandler, SharedHealth, Supervisor,
    TwilightConfig, connector,
};
use tokio::sync::watch;

struct CountHandler;

#[async_trait]
impl InboundHandler for CountHandler {
    async fn handle(&self, event: InboundEvent) -> Result<(), String> {
        println!("received {:?}", std::mem::discriminant(&event));
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("DISCORD_TOKEN")?;
    let guild = std::env::var("DISCORD_GUILD")?;
    let config = TwilightConfig::new(token, DiscordGuildId::new(guild));
    let connector = connector(config)?;
    let health = Arc::new(SharedHealth::new());
    let supervisor = Supervisor::new(connector, Arc::new(CountHandler)).with_health(health.clone());

    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = stop_tx.send(true);
        }
    });

    supervisor.run(stop_rx).await?;
    let snapshot = health.snapshot();
    println!("gateway stopped: {snapshot:?}");
    if snapshot.state != GatewayState::Stopped {
        return Err(format!("gateway did not stop cleanly: {:?}", snapshot.state).into());
    }
    Ok(())
}
