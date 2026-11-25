use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::watch;

use crate::InboundEvent;

/// Subcommands that every guild-scoped `swap` command contains.
pub const SWAP_SUBCOMMANDS: &[(&str, &str)] = &[
    ("join", "Join the event with one seed track."),
    ("status", "Show your current assignment and deadline."),
    ("claim", "Claim the track offered to you."),
    ("submit", "Submit a new version of the track you hold."),
    ("skip", "Skip the current offer without claiming it."),
    ("pause", "Pause your own participation."),
    ("resume", "Resume your own participation."),
    ("extend", "Ask the host for more time."),
    ("leave", "Leave the event."),
    ("help", "Show the participant help."),
    ("schedule", "Show the public schedule summary."),
    ("fillin", "Ask to join as a fill-in."),
    ("title", "Set the anonymous track title."),
    ("host-lock", "Lock the roster or schedule."),
    ("host-reveal", "Reveal the event."),
    ("host-status", "Show host diagnostics."),
];

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GatewayError {
    #[error("gateway configuration is invalid: {0}")]
    Configuration(String),
    #[error("Discord rejected the bot token")]
    Authentication,
    #[error("gateway transport failed: {0}")]
    Transport(String),
    #[error("gateway payload is invalid: {0}")]
    Protocol(String),
}

impl GatewayError {
    /// A fatal error must stop the supervisor. A transient error can retry.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Configuration(_) | Self::Authentication)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayState {
    Starting,
    Online,
    Reconnecting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayHealth {
    pub state: GatewayState,
    pub attempt: u32,
    pub last_error: Option<String>,
    pub connected_at: Option<i64>,
    pub last_event_at: Option<i64>,
    pub handler_errors: u32,
}

impl Default for GatewayHealth {
    fn default() -> Self {
        Self {
            state: GatewayState::Starting,
            attempt: 0,
            last_error: None,
            connected_at: None,
            last_event_at: None,
            handler_errors: 0,
        }
    }
}

impl GatewayHealth {
    pub fn is_online(&self) -> bool {
        self.state == GatewayState::Online
    }
}

pub trait HealthSink: Send + Sync {
    fn publish(&self, health: GatewayHealth);
}

#[derive(Debug, Default)]
pub struct SharedHealth {
    current: Mutex<GatewayHealth>,
}

impl SharedHealth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> GatewayHealth {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GatewayHealth> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl HealthSink for SharedHealth {
    fn publish(&self, health: GatewayHealth) {
        *self.lock() = health;
    }
}

pub trait Clock: Send + Sync {
    fn now_unix(&self) -> i64;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }
}

#[async_trait]
pub trait Timer: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

#[derive(Debug, Default)]
pub struct TokioTimer;

#[async_trait]
impl Timer for TokioTimer {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    pub initial: Duration,
    pub maximum: Duration,
    pub multiplier: u32,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            maximum: Duration::from_secs(60),
            multiplier: 2,
        }
    }
}

impl BackoffPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        let mut delay = self.initial;
        for _ in 0..attempt {
            delay = delay.saturating_mul(self.multiplier);
            if delay >= self.maximum {
                return self.maximum;
            }
        }
        delay.min(self.maximum)
    }
}

#[async_trait]
pub trait GatewaySession: Send {
    /// Wait until Discord confirms the session is ready.
    async fn wait_ready(&mut self) -> Result<(), GatewayError> {
        Ok(())
    }

    /// Return one normalized event. `None` means the session closed cleanly.
    async fn next_event(&mut self) -> Result<Option<InboundEvent>, GatewayError>;

    async fn close(&mut self) -> Result<(), GatewayError>;
}

#[async_trait]
pub trait GatewayConnector: Send + Sync {
    async fn connect(&self) -> Result<Box<dyn GatewaySession>, GatewayError>;

    /// Register the guild-scoped command set. The call must be idempotent.
    async fn register_commands(&self) -> Result<usize, GatewayError>;
}

#[async_trait]
pub trait InboundHandler: Send + Sync {
    async fn handle(&self, event: InboundEvent) -> Result<(), String>;
}

pub struct Supervisor {
    connector: Arc<dyn GatewayConnector>,
    handler: Arc<dyn InboundHandler>,
    health: Arc<dyn HealthSink>,
    clock: Arc<dyn Clock>,
    timer: Arc<dyn Timer>,
    policy: BackoffPolicy,
    stable_after_secs: i64,
}

impl Supervisor {
    pub fn new(connector: Arc<dyn GatewayConnector>, handler: Arc<dyn InboundHandler>) -> Self {
        Self {
            connector,
            handler,
            health: Arc::new(SharedHealth::new()),
            clock: Arc::new(SystemClock),
            timer: Arc::new(TokioTimer),
            policy: BackoffPolicy::default(),
            stable_after_secs: 60,
        }
    }

    pub fn with_health(mut self, health: Arc<dyn HealthSink>) -> Self {
        self.health = health;
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_timer(mut self, timer: Arc<dyn Timer>) -> Self {
        self.timer = timer;
        self
    }

    pub fn with_policy(mut self, policy: BackoffPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_stable_after(mut self, seconds: i64) -> Self {
        self.stable_after_secs = seconds;
        self
    }

    /// Run until the stop channel becomes true.
    ///
    /// Transient failures reconnect with exponential backoff. A fatal error
    /// stops the loop and leaves the last error in the health snapshot.
    pub async fn run(&self, mut stop: watch::Receiver<bool>) -> Result<(), GatewayError> {
        let mut attempt = 0u32;
        let mut handler_errors = 0u32;
        let mut last_connected_at: Option<i64> = None;
        let mut last_event_at: Option<i64> = None;
        self.publish(GatewayHealth {
            state: GatewayState::Starting,
            ..GatewayHealth::default()
        });

        loop {
            if *stop.borrow() {
                break;
            }

            let mut session = tokio::select! {
                biased;
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        break;
                    }
                    continue;
                }
                result = self.connector.connect() => match result {
                    Ok(session) => session,
                    Err(error) => {
                        if error.is_fatal() {
                            self.publish(GatewayHealth {
                                state: GatewayState::Failed,
                                attempt,
                                last_error: Some(error.to_string()),
                                connected_at: last_connected_at,
                                last_event_at,
                                handler_errors,
                            });
                            return Err(error);
                        }
                        self.publish(GatewayHealth {
                            state: GatewayState::Reconnecting,
                            attempt,
                            last_error: Some(error.to_string()),
                            connected_at: last_connected_at,
                            last_event_at,
                            handler_errors,
                        });
                        if !self.sleep_or_stop(&mut stop, self.policy.delay(attempt)).await {
                            break;
                        }
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                }
            };

            let ready = tokio::select! {
                biased;
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        let _ = session.close().await;
                        self.publish(GatewayHealth {
                            state: GatewayState::Stopped,
                            attempt,
                            last_error: None,
                            connected_at: last_connected_at,
                            last_event_at,
                            handler_errors,
                        });
                        return Ok(());
                    }
                    continue;
                }
                result = session.wait_ready() => result,
            };
            if let Err(error) = ready {
                let _ = session.close().await;
                if error.is_fatal() {
                    self.publish(GatewayHealth {
                        state: GatewayState::Failed,
                        attempt,
                        last_error: Some(error.to_string()),
                        connected_at: last_connected_at,
                        last_event_at,
                        handler_errors,
                    });
                    return Err(error);
                }
                self.publish(GatewayHealth {
                    state: GatewayState::Reconnecting,
                    attempt,
                    last_error: Some(error.to_string()),
                    connected_at: last_connected_at,
                    last_event_at,
                    handler_errors,
                });
                if !self
                    .sleep_or_stop(&mut stop, self.policy.delay(attempt))
                    .await
                {
                    break;
                }
                attempt = attempt.saturating_add(1);
                continue;
            }

            let registration = tokio::select! {
                biased;
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() {
                        let _ = session.close().await;
                        self.publish(GatewayHealth {
                            state: GatewayState::Stopped,
                            attempt,
                            last_error: None,
                            connected_at: last_connected_at,
                            last_event_at,
                            handler_errors,
                        });
                        return Ok(());
                    }
                    continue;
                }
                result = self.connector.register_commands() => result,
            };
            if let Err(error) = registration {
                let _ = session.close().await;
                if error.is_fatal() {
                    self.publish(GatewayHealth {
                        state: GatewayState::Failed,
                        attempt,
                        last_error: Some(error.to_string()),
                        connected_at: last_connected_at,
                        last_event_at,
                        handler_errors,
                    });
                    return Err(error);
                }
                self.publish(GatewayHealth {
                    state: GatewayState::Reconnecting,
                    attempt,
                    last_error: Some(error.to_string()),
                    connected_at: last_connected_at,
                    last_event_at,
                    handler_errors,
                });
                if !self
                    .sleep_or_stop(&mut stop, self.policy.delay(attempt))
                    .await
                {
                    break;
                }
                attempt = attempt.saturating_add(1);
                continue;
            }

            let connected_at = self.clock.now_unix();
            last_connected_at = Some(connected_at);
            self.publish(GatewayHealth {
                state: GatewayState::Online,
                attempt,
                last_error: None,
                connected_at: Some(connected_at),
                last_event_at: None,
                handler_errors,
            });

            let disconnect = loop {
                tokio::select! {
                    biased;
                    changed = stop.changed() => {
                        if changed.is_err() || *stop.borrow() {
                            let close_error = session.close().await.err();
                            self.publish(GatewayHealth {
                                state: GatewayState::Stopped,
                                attempt,
                                last_error: close_error.map(|error| error.to_string()),
                                connected_at: Some(connected_at),
                                last_event_at,
                                handler_errors,
                            });
                            return Ok(());
                        }
                    }
                    result = session.next_event() => match result {
                        Ok(Some(event)) => {
                            let handled = tokio::time::timeout(
                                Duration::from_secs(5),
                                self.handler.handle(event),
                            )
                            .await;
                            if !matches!(handled, Ok(Ok(()))) {
                                handler_errors = handler_errors.saturating_add(1);
                            }
                            last_event_at = Some(self.clock.now_unix());
                            self.publish(GatewayHealth {
                                state: GatewayState::Online,
                                attempt,
                                last_error: None,
                                connected_at: Some(connected_at),
                                last_event_at,
                                handler_errors,
                            });
                        }
                        Ok(None) => break None,
                        Err(error) => {
                            if error.is_fatal() {
                                let _ = session.close().await;
                                self.publish(GatewayHealth {
                                    state: GatewayState::Failed,
                                    attempt,
                                    last_error: Some(error.to_string()),
                                    connected_at: Some(connected_at),
                                    last_event_at: Some(self.clock.now_unix()),
                                    handler_errors,
                                });
                                return Err(error);
                            }
                            break Some(error);
                        }
                    }
                }
            };

            let close_error = session.close().await.err();
            if *stop.borrow() {
                self.publish(GatewayHealth {
                    state: GatewayState::Stopped,
                    attempt,
                    last_error: close_error.map(|error| error.to_string()),
                    connected_at: Some(connected_at),
                    last_event_at,
                    handler_errors,
                });
                return Ok(());
            }
            if self.clock.now_unix().saturating_sub(connected_at) >= self.stable_after_secs {
                attempt = 0;
            }
            let error = disconnect
                .map(|error| error.to_string())
                .or_else(|| close_error.map(|error| error.to_string()));
            self.publish(GatewayHealth {
                state: GatewayState::Reconnecting,
                attempt,
                last_error: error,
                connected_at: Some(connected_at),
                last_event_at,
                handler_errors,
            });
            if !self
                .sleep_or_stop(&mut stop, self.policy.delay(attempt))
                .await
            {
                break;
            }
            attempt = attempt.saturating_add(1);
        }

        self.publish(GatewayHealth {
            state: GatewayState::Stopped,
            attempt,
            last_error: None,
            connected_at: last_connected_at,
            last_event_at,
            handler_errors,
        });
        Ok(())
    }

    async fn sleep_or_stop(&self, stop: &mut watch::Receiver<bool>, duration: Duration) -> bool {
        tokio::select! {
            biased;
            changed = stop.changed() => changed.is_ok() && !*stop.borrow(),
            _ = self.timer.sleep(duration) => true,
        }
    }

    fn publish(&self, health: GatewayHealth) {
        self.health.publish(health);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;
    use crate::{DiscordUserId, InboundEvent};

    struct FakeClock(AtomicI64);

    impl Clock for FakeClock {
        fn now_unix(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct FakeTimer(Mutex<Vec<Duration>>);

    #[async_trait]
    impl Timer for FakeTimer {
        async fn sleep(&self, duration: Duration) {
            self.0.lock().unwrap().push(duration);
        }
    }

    struct RecordingHandler {
        seen: Mutex<Vec<InboundEvent>>,
        stop: watch::Sender<bool>,
        stop_after: usize,
    }

    #[async_trait]
    impl InboundHandler for RecordingHandler {
        async fn handle(&self, event: InboundEvent) -> Result<(), String> {
            self.seen.lock().unwrap().push(event);
            if self.seen.lock().unwrap().len() >= self.stop_after {
                self.stop.send(true).unwrap();
            }
            Ok(())
        }
    }

    struct FakeSession {
        events: VecDeque<Result<Option<InboundEvent>, GatewayError>>,
        block_after: bool,
    }

    #[async_trait]
    impl GatewaySession for FakeSession {
        async fn next_event(&mut self) -> Result<Option<InboundEvent>, GatewayError> {
            if let Some(event) = self.events.pop_front() {
                return event;
            }
            if self.block_after {
                std::future::pending::<()>().await;
            }
            Ok(None)
        }

        async fn close(&mut self) -> Result<(), GatewayError> {
            Ok(())
        }
    }

    struct FakeConnector {
        sessions: Mutex<VecDeque<Box<dyn GatewaySession>>>,
        connect_errors: Mutex<VecDeque<GatewayError>>,
        registration_errors: Mutex<VecDeque<GatewayError>>,
        connects: AtomicUsize,
        registrations: AtomicUsize,
    }

    #[async_trait]
    impl GatewayConnector for FakeConnector {
        async fn connect(&self) -> Result<Box<dyn GatewaySession>, GatewayError> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = self.connect_errors.lock().unwrap().pop_front() {
                return Err(error);
            }
            self.sessions
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(GatewayError::Transport("no session".to_string()))
        }

        async fn register_commands(&self) -> Result<usize, GatewayError> {
            self.registrations.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = self.registration_errors.lock().unwrap().pop_front() {
                return Err(error);
            }
            Ok(1)
        }
    }

    fn event(id: &str) -> InboundEvent {
        InboundEvent::Button {
            interaction: crate::InteractionRef::new("i1", "token"),
            user_id: DiscordUserId::new(id),
            guild_id: None,
            custom_id: crate::CustomId::new("evt-1", crate::Action::Claim, None).unwrap(),
        }
    }

    #[tokio::test]
    async fn existing_sessions_are_reused_and_backoff_is_exponential() {
        let (stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let timer = Arc::new(FakeTimer(Mutex::new(Vec::new())));
        let seen = Arc::new(RecordingHandler {
            seen: Mutex::new(Vec::new()),
            stop: stop_tx,
            stop_after: 3,
        });
        let sessions: VecDeque<Box<dyn GatewaySession>> = VecDeque::from(vec![
            Box::new(FakeSession {
                events: VecDeque::from(vec![Ok(Some(event("u1"))), Ok(None)]),
                block_after: false,
            }) as Box<dyn GatewaySession>,
            Box::new(FakeSession {
                events: VecDeque::from(vec![Ok(Some(event("u2"))), Ok(None)]),
                block_after: false,
            }) as Box<dyn GatewaySession>,
            Box::new(FakeSession {
                events: VecDeque::from(vec![Ok(Some(event("u3")))]),
                block_after: true,
            }) as Box<dyn GatewaySession>,
        ]);
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(sessions),
            connect_errors: Mutex::new(VecDeque::new()),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
            registration_errors: Mutex::new(VecDeque::new()),
        });
        let supervisor = Supervisor::new(connector.clone(), seen.clone())
            .with_health(health.clone())
            .with_clock(Arc::new(FakeClock(AtomicI64::new(0))))
            .with_timer(timer.clone())
            .with_policy(BackoffPolicy {
                initial: Duration::from_secs(1),
                maximum: Duration::from_secs(8),
                multiplier: 2,
            })
            .with_stable_after(10);

        supervisor.run(stop_rx).await.unwrap();

        assert_eq!(seen.seen.lock().unwrap().len(), 3);
        assert_eq!(connector.connects.load(Ordering::SeqCst), 3);
        assert_eq!(connector.registrations.load(Ordering::SeqCst), 3);
        let delays = timer.0.lock().unwrap().clone();
        assert_eq!(delays, vec![Duration::from_secs(1), Duration::from_secs(2)]);
        assert_eq!(health.snapshot().state, GatewayState::Stopped);
    }

    #[tokio::test]
    async fn a_fatal_connect_error_stops_the_supervisor() {
        let (_stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::new()),
            connect_errors: Mutex::new(VecDeque::from(vec![GatewayError::Authentication])),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
            registration_errors: Mutex::new(VecDeque::new()),
        });
        let supervisor = Supervisor::new(
            connector,
            Arc::new(RecordingHandler {
                seen: Mutex::new(Vec::new()),
                stop: watch::channel(false).0,
                stop_after: 3,
            }),
        )
        .with_health(health.clone());

        let result = supervisor.run(stop_rx).await;
        assert_eq!(result, Err(GatewayError::Authentication));
        assert_eq!(health.snapshot().state, GatewayState::Failed);
    }

    #[tokio::test]
    async fn a_stop_before_connect_does_not_open_a_session() {
        let (_stop_tx, stop_rx) = watch::channel(true);
        let health = Arc::new(SharedHealth::new());
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::new()),
            connect_errors: Mutex::new(VecDeque::new()),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
            registration_errors: Mutex::new(VecDeque::new()),
        });
        let supervisor = Supervisor::new(
            connector.clone(),
            Arc::new(RecordingHandler {
                seen: Mutex::new(Vec::new()),
                stop: watch::channel(false).0,
                stop_after: 3,
            }),
        )
        .with_health(health.clone());

        supervisor.run(stop_rx).await.unwrap();
        assert_eq!(connector.connects.load(Ordering::SeqCst), 0);
        assert_eq!(health.snapshot().state, GatewayState::Stopped);
    }

    #[tokio::test]
    async fn a_transient_connect_error_retries() {
        let (stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let timer = Arc::new(FakeTimer(Mutex::new(Vec::new())));
        let handler = Arc::new(RecordingHandler {
            seen: Mutex::new(Vec::new()),
            stop: stop_tx,
            stop_after: 1,
        });
        // The first event stops the supervisor; the second event is not reached.
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::from(vec![Box::new(FakeSession {
                events: VecDeque::from(vec![Ok(Some(event("u1"))), Ok(None)]),
                block_after: false,
            })
                as Box<dyn GatewaySession>])),
            connect_errors: Mutex::new(VecDeque::from(vec![GatewayError::Transport(
                "temporary".to_string(),
            )])),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
            registration_errors: Mutex::new(VecDeque::new()),
        });
        let supervisor = Supervisor::new(connector.clone(), handler.clone())
            .with_health(health.clone())
            .with_clock(Arc::new(FakeClock(AtomicI64::new(0))))
            .with_timer(timer.clone())
            .with_policy(BackoffPolicy {
                initial: Duration::from_secs(1),
                maximum: Duration::from_secs(8),
                multiplier: 2,
            });
        supervisor.run(stop_rx).await.unwrap();
        assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
        assert_eq!(connector.registrations.load(Ordering::SeqCst), 1);
        assert_eq!(
            timer.0.lock().unwrap().as_slice(),
            &[Duration::from_secs(1)]
        );
    }

    #[tokio::test]
    async fn a_handler_error_is_counted_and_does_not_close_the_session() {
        struct FailingHandler {
            count: AtomicUsize,
            stop: watch::Sender<bool>,
        }

        #[async_trait]
        impl InboundHandler for FailingHandler {
            async fn handle(&self, _event: InboundEvent) -> Result<(), String> {
                let count = self.count.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= 2 {
                    self.stop.send(true).unwrap();
                }
                Err("application rejected the event".to_string())
            }
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::from(vec![Box::new(FakeSession {
                events: VecDeque::from(vec![
                    Ok(Some(event("u1"))),
                    Ok(Some(event("u2"))),
                    Ok(None),
                ]),
                block_after: false,
            })
                as Box<dyn GatewaySession>])),
            connect_errors: Mutex::new(VecDeque::new()),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
            registration_errors: Mutex::new(VecDeque::new()),
        });
        let supervisor = Supervisor::new(
            connector,
            Arc::new(FailingHandler {
                count: AtomicUsize::new(0),
                stop: stop_tx,
            }),
        )
        .with_health(health.clone())
        .with_clock(Arc::new(FakeClock(AtomicI64::new(0))))
        .with_timer(Arc::new(FakeTimer(Mutex::new(Vec::new()))));

        supervisor.run(stop_rx).await.unwrap();
        assert_eq!(health.snapshot().handler_errors, 2);
        assert_eq!(health.snapshot().state, GatewayState::Stopped);
    }

    #[tokio::test]
    async fn a_transient_registration_error_reconnects() {
        let (stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let timer = Arc::new(FakeTimer(Mutex::new(Vec::new())));
        let handler = Arc::new(RecordingHandler {
            seen: Mutex::new(Vec::new()),
            stop: stop_tx,
            stop_after: 1,
        });
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::from(vec![
                Box::new(FakeSession {
                    events: VecDeque::new(),
                    block_after: false,
                }) as Box<dyn GatewaySession>,
                Box::new(FakeSession {
                    events: VecDeque::from(vec![Ok(Some(event("u1")))]),
                    block_after: true,
                }) as Box<dyn GatewaySession>,
            ])),
            connect_errors: Mutex::new(VecDeque::new()),
            registration_errors: Mutex::new(VecDeque::from(vec![GatewayError::Transport(
                "registration failed".to_string(),
            )])),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
        });
        let supervisor = Supervisor::new(connector.clone(), handler)
            .with_health(health.clone())
            .with_clock(Arc::new(FakeClock(AtomicI64::new(0))))
            .with_timer(timer.clone());

        supervisor.run(stop_rx).await.unwrap();
        assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
        assert_eq!(connector.registrations.load(Ordering::SeqCst), 2);
        assert_eq!(
            timer.0.lock().unwrap().as_slice(),
            &[Duration::from_secs(1)]
        );
    }

    #[tokio::test]
    async fn a_fatal_session_error_stops_the_supervisor() {
        let (_stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let connector = Arc::new(FakeConnector {
            sessions: Mutex::new(VecDeque::from(vec![Box::new(FakeSession {
                events: VecDeque::from(vec![Err(GatewayError::Authentication)]),
                block_after: false,
            })
                as Box<dyn GatewaySession>])),
            connect_errors: Mutex::new(VecDeque::new()),
            registration_errors: Mutex::new(VecDeque::new()),
            connects: AtomicUsize::new(0),
            registrations: AtomicUsize::new(0),
        });
        let supervisor = Supervisor::new(
            connector,
            Arc::new(RecordingHandler {
                seen: Mutex::new(Vec::new()),
                stop: watch::channel(false).0,
                stop_after: 1,
            }),
        )
        .with_health(health.clone());

        assert_eq!(
            supervisor.run(stop_rx).await,
            Err(GatewayError::Authentication)
        );
        assert_eq!(health.snapshot().state, GatewayState::Failed);
    }

    #[test]
    fn the_backoff_is_bounded() {
        let policy = BackoffPolicy {
            initial: Duration::from_secs(1),
            maximum: Duration::from_secs(10),
            multiplier: 2,
        };
        assert_eq!(policy.delay(0), Duration::from_secs(1));
        assert_eq!(policy.delay(1), Duration::from_secs(2));
        assert_eq!(policy.delay(2), Duration::from_secs(4));
        assert_eq!(policy.delay(3), Duration::from_secs(8));
        assert_eq!(policy.delay(4), Duration::from_secs(10));
        assert_eq!(policy.delay(100), Duration::from_secs(10));
    }

    #[tokio::test]
    async fn a_stop_during_ready_waits_closes_the_session() {
        struct ReadySession {
            started: Arc<tokio::sync::Notify>,
            closed: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl GatewaySession for ReadySession {
            async fn wait_ready(&mut self) -> Result<(), GatewayError> {
                self.started.notify_waiters();
                std::future::pending::<Result<(), GatewayError>>().await
            }

            async fn next_event(&mut self) -> Result<Option<InboundEvent>, GatewayError> {
                Ok(None)
            }

            async fn close(&mut self) -> Result<(), GatewayError> {
                self.closed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        struct ReadyConnector {
            session: Mutex<Option<ReadySession>>,
            registrations: AtomicUsize,
        }

        #[async_trait]
        impl GatewayConnector for ReadyConnector {
            async fn connect(&self) -> Result<Box<dyn GatewaySession>, GatewayError> {
                self.session
                    .lock()
                    .unwrap()
                    .take()
                    .map(|session| Box::new(session) as Box<dyn GatewaySession>)
                    .ok_or(GatewayError::Transport("no session".to_string()))
            }

            async fn register_commands(&self) -> Result<usize, GatewayError> {
                self.registrations.fetch_add(1, Ordering::SeqCst);
                Ok(1)
            }
        }

        let started = Arc::new(tokio::sync::Notify::new());
        let closed = Arc::new(AtomicUsize::new(0));
        let connector = Arc::new(ReadyConnector {
            session: Mutex::new(Some(ReadySession {
                started: started.clone(),
                closed: closed.clone(),
            })),
            registrations: AtomicUsize::new(0),
        });
        let health = Arc::new(SharedHealth::new());
        let supervisor = Supervisor::new(
            connector.clone(),
            Arc::new(RecordingHandler {
                seen: Mutex::new(Vec::new()),
                stop: watch::channel(false).0,
                stop_after: 1,
            }),
        )
        .with_health(health.clone());
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(async move { supervisor.run(stop_rx).await });
        started.notified().await;
        stop_tx.send(true).unwrap();
        task.await.unwrap().unwrap();

        assert_eq!(closed.load(Ordering::SeqCst), 1);
        assert_eq!(connector.registrations.load(Ordering::SeqCst), 0);
        assert_eq!(health.snapshot().state, GatewayState::Stopped);
    }

    #[tokio::test]
    async fn a_failed_reconnect_keeps_the_last_connection_time() {
        struct ExhaustingConnector {
            session: Mutex<Option<FakeSession>>,
            stop: watch::Sender<bool>,
        }

        #[async_trait]
        impl GatewayConnector for ExhaustingConnector {
            async fn connect(&self) -> Result<Box<dyn GatewaySession>, GatewayError> {
                if let Some(session) = self.session.lock().unwrap().take() {
                    return Ok(Box::new(session));
                }
                let _ = self.stop.send(true);
                Err(GatewayError::Transport("outage".to_string()))
            }

            async fn register_commands(&self) -> Result<usize, GatewayError> {
                Ok(1)
            }
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        let health = Arc::new(SharedHealth::new());
        let connector = Arc::new(ExhaustingConnector {
            session: Mutex::new(Some(FakeSession {
                events: VecDeque::from(vec![Ok(Some(event("u1"))), Ok(None)]),
                block_after: false,
            })),
            stop: stop_tx,
        });
        let supervisor = Supervisor::new(
            connector,
            Arc::new(RecordingHandler {
                seen: Mutex::new(Vec::new()),
                stop: watch::channel(false).0,
                stop_after: 2,
            }),
        )
        .with_health(health.clone())
        .with_clock(Arc::new(FakeClock(AtomicI64::new(100))))
        .with_timer(Arc::new(FakeTimer(Mutex::new(Vec::new()))));

        supervisor.run(stop_rx).await.unwrap();
        assert_eq!(health.snapshot().connected_at, Some(100));
        assert_eq!(health.snapshot().state, GatewayState::Stopped);
    }

    #[test]
    fn the_command_set_covers_every_contract_subcommand() {
        assert_eq!(SWAP_SUBCOMMANDS.len(), 16);
        assert!(
            SWAP_SUBCOMMANDS
                .iter()
                .any(|(name, _)| *name == "host-lock")
        );
        for (name, _) in SWAP_SUBCOMMANDS {
            assert!(
                crate::parse_swap_subcommand(name).is_some(),
                "unknown command: {name}"
            );
        }
    }
}
