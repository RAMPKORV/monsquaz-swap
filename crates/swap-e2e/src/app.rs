//! A headless app: the real engine, database, files, clock, and Discord
//! transport. Tests drive the host and the participants through this type.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use swap_core::{AssignmentStatus, EventState};
use swap_db::{AssignmentRepo, Db, EventRepo, ParticipantRepo};
use swap_discord::{
    DiscordGuildId, GatewayError, GatewayHealth, HttpAttachmentFetcher, HttpDiscordPort,
    SharedHealth, Supervisor, TwilightConfig, connector_with_proxy,
};
use swap_engine::{Clock, EventSettings, InboundService, Mode, OpenMptPreparer};

use crate::clock::FileClock;
use crate::sim::{DiscordSim, InjectMessageRequest};
use crate::workspace::TempWorkspace;

pub const BASE_TIME: i64 = 1_760_000_000;

/// A fixed event secret makes track codes and the plan reproducible.
pub const E2E_SECRET: &str = "000102030405060708090a0b0c0d0e0f";

/// The live stack without the Tauri window.
pub struct HeadlessApp {
    pub sim: DiscordSim,
    pub workspace: TempWorkspace,
    pub clock: FileClock,
    pub db: Arc<Mutex<Db>>,
    pub service: Arc<InboundService>,
    health: Arc<SharedHealth>,
    stop: tokio::sync::watch::Sender<bool>,
    gateway: tokio::task::JoinHandle<Result<(), GatewayError>>,
    dispatch: tokio::task::JoinHandle<()>,
}

impl HeadlessApp {
    pub async fn start() -> Result<Self, String> {
        let config = crate::sim::SimConfig {
            epoch: BASE_TIME,
            ..crate::sim::SimConfig::default()
        };
        let sim = DiscordSim::with_config(config)
            .await
            .map_err(|error| error.to_string())?;
        let workspace = TempWorkspace::new("app").map_err(|error| error.to_string())?;
        let clock = FileClock::new(workspace.clock());
        clock.set(BASE_TIME).map_err(|error| error.to_string())?;
        let db = Db::open(workspace.db()).map_err(|error| error.to_string())?;
        let store = swap_files::BlobStore::new(workspace.files());
        let preparer = OpenMptPreparer::discover()
            .map_err(|error| format!("openmpt123 is required: {error}"))?;
        let discord = HttpDiscordPort::with_proxy(sim.token(), sim.http_host())
            .map_err(|error| error.to_string())?;
        let fetcher =
            HttpAttachmentFetcher::with_base(sim.cdn_base()).map_err(|error| error.to_string())?;
        let db = Arc::new(Mutex::new(db));
        let service = Arc::new(
            InboundService::new(
                db.clone(),
                store,
                Arc::new(preparer),
                Arc::new(discord),
                Arc::new(fetcher),
                Arc::new(clock.clone()),
            )
            .with_alert_channel(Some(sim.announcements_channel().to_string()))
            .with_fallback_channel(Some(sim.announcements_channel().to_string())),
        );
        service.mark_backfill_pending();

        let connector = connector_with_proxy(
            TwilightConfig::new(sim.token(), DiscordGuildId::new(sim.guild_id())),
            sim.http_host(),
        )
        .map_err(|error| error.to_string())?;
        let health = Arc::new(SharedHealth::new());
        let supervisor = Supervisor::new(connector, service.clone()).with_health(health.clone());
        let (stop, _) = tokio::sync::watch::channel(false);
        let gateway_stop = stop.subscribe();
        let dispatch_stop = stop.subscribe();
        let gateway = tokio::spawn(async move {
            let stop_rx = gateway_stop;
            supervisor.run(stop_rx).await
        });

        if !sim.wait_ready(Duration::from_secs(10)).await {
            return Err("the gateway did not become ready".to_string());
        }
        if !sim.wait_commands(Duration::from_secs(10)).await {
            return Err("the bot did not register its commands".to_string());
        }

        let task_service = service.clone();
        let wake = service.wake_notifier();
        let dispatch = tokio::spawn(async move {
            let mut stop_rx = dispatch_stop;
            loop {
                tokio::select! {
                    changed = stop_rx.changed() => {
                        if changed.is_err() || *stop_rx.borrow() {
                            break;
                        }
                    }
                    _ = wake.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(25)) => {}
                }
                let _ = task_service.tick_deadlines();
                let _ = task_service.drain_pending().await;
            }
        });

        Ok(Self {
            sim,
            workspace,
            clock,
            db,
            service,
            health,
            stop,
            gateway,
            dispatch,
        })
    }

    pub fn now(&self) -> i64 {
        self.clock.now()
    }

    pub fn gateway_health(&self) -> Option<GatewayHealth> {
        Some(self.health.snapshot())
    }

    pub fn advance(&self, seconds: i64) -> i64 {
        let now = self
            .clock
            .advance(seconds)
            .expect("the test clock must advance");
        self.sim.set_epoch(now);
        now
    }

    pub fn with_db<T>(&self, read: impl FnOnce(&Db) -> T) -> T {
        let db = self
            .db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        read(&db)
    }

    pub async fn drain(&self) -> Result<usize, String> {
        let report = self.service.drain_pending().await?;
        Ok(report.sent)
    }

    pub fn setting(&self) -> EventSettings {
        EventSettings {
            mode: Mode::Classic,
            rounds: Some(3),
            pack_type: Some("Zip".to_string()),
            pack_required: Some(true),
            group_mode: Some("Groups".to_string()),
            group_count: Some(1),
            group_size: Some(6),
            rotation: Some("Fixed".to_string()),
            scheduler: Some("balanced".to_string()),
            swap_cadence: Some("PerRound".to_string()),
            schedule_visibility: Some("Own".to_string()),
            handoff_policy: Some("Strict".to_string()),
            ..EventSettings::default()
        }
    }

    pub fn async_setting(&self) -> EventSettings {
        EventSettings {
            mode: Mode::Async,
            pack_type: Some("Zip".to_string()),
            pack_required: Some(true),
            async_expected_songs: Some(4),
            async_claims_per_participant: Some(2),
            async_release_time: Some("00:00".to_string()),
            async_wave_offset_secs: 2 * 60 * 60,
            async_late_seeds: Some(false),
            handoff_policy: Some("Flexible".to_string()),
            ..EventSettings::default()
        }
    }

    pub fn create_event(&self, name: &str) -> Result<String, String> {
        self.create_event_with(name, self.setting())
    }

    pub fn create_event_with(&self, name: &str, settings: EventSettings) -> Result<String, String> {
        settings.validate().map_err(|error| error.to_string())?;
        let id = format!("evt-e2e-{}", self.now());
        swap_engine::create_event_with_secret(
            &self.db(),
            &id,
            name,
            42,
            &settings,
            Some(E2E_SECRET),
            self.now(),
        )
        .map_err(|error| error.to_string())?;
        swap_db::MetaRepo::set(self.db().conn(), "host.current_event", &id)
            .map_err(|error| error.to_string())?;
        Ok(id)
    }

    pub fn upload_pack(&self, event_id: &str, filename: &str, bytes: &[u8]) -> Result<(), String> {
        let store = swap_files::BlobStore::new(self.workspace.files());
        swap_engine::upload_pack(&self.db(), &store, event_id, filename, bytes, self.now())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn open_seed_round(&self, event_id: &str, deadline_at: i64) -> Result<(), String> {
        swap_engine::open_seed_round(
            &self.db(),
            event_id,
            deadline_at,
            Some(self.sim.announcements_channel()),
            self.now(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    pub fn lock_roster(&self, event_id: &str) -> Result<swap_engine::LockOutcome, String> {
        swap_engine::lock_roster(&self.db(), event_id, self.now())
            .map_err(|error| error.to_string())
    }

    pub fn generate_schedule(
        &self,
        event_id: &str,
    ) -> Result<swap_engine::ScheduleOutcome, String> {
        swap_engine::generate_schedule(&self.db(), event_id, self.now())
            .map_err(|error| error.to_string())
    }

    pub fn verify_commitment(&self, event_id: &str) -> Result<bool, String> {
        swap_engine::verify_commitment(&self.db(), event_id)
            .map(|outcome| outcome.verified)
            .map_err(|error| error.to_string())
    }

    pub fn start_event(&self, event_id: &str) -> Result<(), String> {
        swap_engine::start_event_with_channel(
            &self.db(),
            event_id,
            Some(self.sim.announcements_channel()),
            self.now(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    pub fn start_async_event(&self, event_id: &str) -> Result<(), String> {
        swap_engine::start_async_event(&self.db(), event_id, self.now())
            .map_err(|error| error.to_string())
    }

    pub fn dispatch_async_release(&self, event_id: &str) -> Result<usize, String> {
        swap_engine::dispatch_async_release(&self.db(), event_id, self.now())
            .map(|outcome| outcome.offers)
            .map_err(|error| error.to_string())
    }

    pub fn dispatch_async_wave(&self, event_id: &str) -> Result<usize, String> {
        swap_engine::dispatch_async_wave(&self.db(), event_id, self.now())
            .map(|outcome| outcome.offers)
            .map_err(|error| error.to_string())
    }

    pub fn async_coverage(&self, event_id: &str) -> Result<swap_engine::AsyncCoverage, String> {
        swap_engine::async_coverage(&self.db(), event_id).map_err(|error| error.to_string())
    }

    pub fn tick(&self) -> Result<(), String> {
        self.service
            .tick_deadlines()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn open_round(&self, event_id: &str, round: u32) -> Result<(), String> {
        swap_engine::open_round(&self.db(), event_id, round, self.now())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn move_round_deadline(
        &self,
        event_id: &str,
        round_no: u32,
        deadline_at: i64,
        reason: &str,
    ) -> Result<(), String> {
        swap_engine::move_round_deadline(
            &self.db(),
            event_id,
            round_no,
            deadline_at,
            reason,
            self.now(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    pub fn assign_fill_in(
        &self,
        event_id: &str,
        track_code: &str,
        round_no: u32,
        user_id: &str,
    ) -> Result<(), String> {
        swap_engine::assign_fill_in(
            &self.db(),
            event_id,
            track_code,
            round_no,
            user_id,
            self.now(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    pub fn reveal(&self, event_id: &str) -> Result<swap_engine::RevealOutcome, String> {
        swap_engine::reveal_with_channel(
            &self.db(),
            event_id,
            Some(self.sim.reveal_channel()),
            self.now(),
        )
        .map_err(|error| error.to_string())
    }

    pub fn reveal_zip(&self, event_id: &str) -> Result<Vec<u8>, String> {
        let store = swap_files::BlobStore::new(self.workspace.files());
        swap_engine::reveal_zip(&self.db(), &store, event_id).map_err(|error| error.to_string())
    }

    pub fn join(&self, user_id: &str) -> Result<(), String> {
        self.sim
            .command(user_id, "join", BTreeMap::new())
            .map(|_| ())
    }

    pub fn claim(&self, user_id: &str) -> Result<(), String> {
        self.sim
            .command(user_id, "claim", BTreeMap::new())
            .map(|_| ())
    }

    pub fn skip(&self, user_id: &str) -> Result<(), String> {
        self.sim
            .command(user_id, "skip", BTreeMap::new())
            .map(|_| ())
    }

    pub fn send_file(
        &self,
        user_id: &str,
        filename: &str,
        bytes: &[u8],
        content: &str,
    ) -> Result<(), String> {
        self.sim
            .send_message(InjectMessageRequest {
                user_id: user_id.to_string(),
                content: content.to_string(),
                filename: Some(filename.to_string()),
                bytes_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                content_type: Some("application/octet-stream".to_string()),
            })
            .map(|_| ())
    }

    pub async fn wait_for_dm(
        &self,
        user_id: &str,
        needle: &str,
        timeout: Duration,
    ) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(message) = self
                .sim
                .bot_messages_for(user_id)
                .into_iter()
                .find(|message| message.contains(needle))
            {
                return Ok(message);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out waiting for a DM to {user_id} with {needle:?}; got {:?}",
                    self.sim.bot_messages_for(user_id)
                ));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn wait_until(
        &self,
        label: &str,
        timeout: Duration,
        mut check: impl FnMut() -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if check() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("timed out waiting for {label}"));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub async fn wait_for_assignment(
        &self,
        event_id: &str,
        user_id: &str,
        status: AssignmentStatus,
        timeout: Duration,
    ) -> Result<String, String> {
        let mut track = None;
        self.wait_until("an assignment status", timeout, || {
            let assignment = self.with_db(|db| {
                AssignmentRepo::latest_for_participant(db.conn(), event_id, user_id)
                    .ok()
                    .flatten()
            });
            match assignment {
                Some(assignment) if assignment.status == status => {
                    track = Some(assignment.track_code.clone());
                    true
                }
                _ => false,
            }
        })
        .await?;
        Ok(track.unwrap_or_default())
    }

    pub async fn wait_for_event_state(
        &self,
        event_id: &str,
        state: EventState,
        timeout: Duration,
    ) -> Result<(), String> {
        self.wait_until("the event state", timeout, || {
            self.with_db(|db| {
                EventRepo::get(db.conn(), event_id)
                    .ok()
                    .flatten()
                    .is_some_and(|event| event.state == state)
            })
        })
        .await
    }

    pub async fn wait_for_participant(
        &self,
        event_id: &str,
        user_id: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        self.wait_until("a participant", timeout, || {
            self.with_db(|db| {
                ParticipantRepo::get(db.conn(), event_id, user_id)
                    .ok()
                    .flatten()
                    .is_some()
            })
        })
        .await
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Db> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub async fn shutdown(self) {
        let _ = self.stop.send(true);
        self.service.stop_queues().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.dispatch).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.gateway).await;
    }
}
