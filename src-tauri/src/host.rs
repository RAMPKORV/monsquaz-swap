use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};

use swap_core::{EventState, Membership, Role, RoundStatus};
use swap_db::{
    AssignmentRepo, AuditRepo, AvailabilityRepo, Db, DeadlineRepo, EventRecord, EventRepo,
    LateSeedRepo, MetaRepo, PackRepo, ParticipantRepo, PlanRepo, RoundRepo, TrackRepo,
};
use swap_engine::{
    CreditEntry, EngineError, EventSettings, Mode, TrackCredits, accept_late_seed,
    add_availability, add_track_comment, create_event, dispatch_async_wave, edit_plan_assignment,
    extend_seed_deadline, generate_schedule, lock_roster, move_round_deadline, open_round,
    open_seed_round, pending_late_seeds, plan_edit_candidates, redact_track_comment,
    reject_late_seed, remove_availability, reopen_seed_round, repair_group_schedule,
    reveal_package, reveal_with_channel, set_track_title, start_async_event,
    start_event_with_channel, upload_pack, verify_commitment,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionControlDto {
    first_run: bool,
    event: Option<EventSummaryDto>,
    bot: BotHealthDto,
    next_action: Option<NextActionDto>,
    decisions: Vec<DecisionDto>,
    activity: Vec<ActivityDto>,
    timeline: Vec<TimelineStepDto>,
    facts: Vec<EventFactDto>,
    join: JoinInfoDto,
    late_seeds: Vec<LateSeedDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinInfoDto {
    formats: Vec<String>,
    max_file_mib: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummaryDto {
    id: String,
    name: String,
    mode: String,
    stage: String,
    participants: usize,
    seeded: usize,
    fill_ins: usize,
    tracks: usize,
    round: u32,
    rounds: u32,
    exchange_rounds: u32,
    seed_round_open: bool,
    seed_deadline_at: Option<i64>,
    late_seed_count: usize,
    pack_id: Option<String>,
    open_schedule: bool,
    handoff_policy: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterLockDto {
    active: usize,
    fill_ins: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundOpenDto {
    round: u32,
    offers: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackDto {
    id: String,
    filename: String,
    sha256: String,
    size: u64,
    locked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LateSeedDto {
    id: String,
    display_name: String,
    track_code: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyDto {
    recipe: String,
    commitment: String,
    verified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealDto {
    published: bool,
    blocked: Vec<String>,
    tracks: Vec<RevealTrackDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealTrackDto {
    track_code: String,
    seed_owner_name: String,
    seed_owner: String,
    entries: Vec<RevealEntryDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealEntryDto {
    round: u32,
    display_name: String,
    status: String,
    fill_in: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealPackageDto {
    event_name: String,
    event_state: String,
    published: bool,
    album: Vec<RevealShowTrackDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealShowTrackDto {
    position: u32,
    track_code: String,
    title: Option<String>,
    seed_owner_id: String,
    seed_owner_name: String,
    credits: Vec<RevealShowCreditDto>,
    comments: Vec<RevealShowCommentDto>,
    versions: Vec<RevealShowVersionDto>,
    stats: RevealShowStatsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealShowCreditDto {
    round: u32,
    participant_id: String,
    display_name: String,
    status: String,
    fill_in: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealShowCommentDto {
    id: String,
    participant_id: String,
    author_name: String,
    kind: String,
    body: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealShowVersionDto {
    version: u32,
    holder_id: String,
    holder_name: String,
    role: String,
    round: Option<u32>,
    file_id: String,
    extension: String,
    created_at: i64,
    playable: bool,
    final_version: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealShowStatsDto {
    edits: u32,
    fill_ins: u32,
    skips: u32,
    contributors: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BotHealthDto {
    status: String,
    label: String,
    detail: String,
    last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NextActionDto {
    eyebrow: String,
    title: String,
    detail: String,
    action_label: String,
    secondary_label: Option<String>,
    tone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionDto {
    id: String,
    kind: String,
    title: String,
    detail: String,
    due: String,
    action_label: String,
    assignment_id: Option<String>,
    decision_id: Option<String>,
    requested_hours: Option<i64>,
    deadline: Option<String>,
    new_deadline: Option<String>,
    extensions_used: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityDto {
    id: String,
    at: String,
    actor: String,
    action: String,
    subject: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineStepDto {
    id: String,
    label: String,
    caption: String,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventFactDto {
    label: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryDto {
    id: String,
    at: String,
    actor: String,
    action: String,
    subject: String,
    reason: String,
    code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationsDto {
    deadlines: bool,
    bot_offline: bool,
    releases: bool,
}

impl Default for NotificationsDto {
    fn default() -> Self {
        Self {
            deadlines: true,
            bot_offline: true,
            releases: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsDto {
    pub(crate) bot_configured: bool,
    pub(crate) bot_identity: String,
    pub(crate) token_storage: String,
    pub(crate) guild_id: String,
    pub(crate) guild_name: String,
    pub(crate) announcement_channel_id: String,
    pub(crate) announcement_channel_name: String,
    pub(crate) reveal_channel_id: String,
    pub(crate) reveal_channel_name: String,
    pub(crate) dm_fallback: bool,
    timezone: String,
    retention_days: u32,
    theme: String,
    notifications: NotificationsDto,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            bot_configured: false,
            bot_identity: String::new(),
            token_storage: "none".to_string(),
            guild_id: String::new(),
            guild_name: String::new(),
            announcement_channel_id: String::new(),
            announcement_channel_name: String::new(),
            reveal_channel_id: String::new(),
            reveal_channel_name: String::new(),
            dm_fallback: true,
            timezone: "Europe/Stockholm".to_string(),
            retention_days: 90,
            theme: "system".to_string(),
            notifications: NotificationsDto {
                deadlines: true,
                bot_offline: true,
                releases: true,
            },
        }
    }
}

static TRAY_READY: AtomicBool = AtomicBool::new(false);

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Monsquaz Swap", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".to_string()))?;
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Monsquaz Swap")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    TRAY_READY.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn tray_ready() -> bool {
    TRAY_READY.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDraftDto {
    name: String,
    mode: String,
    rounds: Option<u32>,
    claim_window_hours: i64,
    round_window_hours: i64,
    quiet_hours_start: u8,
    quiet_hours_end: u8,
    allowlist: Vec<String>,
    pack_type: String,
    pack_required: bool,
    group_mode: String,
    group_count: u32,
    group_size: u32,
    rotation: String,
    swap_cadence: String,
    schedule_visibility: String,
    handoff_policy: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupDataDto {
    first_run: bool,
    event: Option<EventSummaryDto>,
    draft: EventDraftDto,
    warnings: Vec<SetupWarningDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupWarningDto {
    id: String,
    tone: String,
    title: String,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleDataDto {
    event: Option<EventSummaryDto>,
    rows: Vec<PersonRowDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonRowDto {
    id: String,
    display_name: String,
    role: String,
    status: String,
    seed_track: Option<String>,
    timezone: String,
    max_handoffs: u32,
    joined: String,
    delivery_risk: bool,
    away: Vec<AwayDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AwayDto {
    id: String,
    starts_at: i64,
    ends_at: i64,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDataDto {
    status: String,
    event: Option<EventSummaryDto>,
    version: u32,
    commitment: String,
    rounds: u32,
    exchange_rounds: u32,
    total_rounds: u32,
    gaps: u32,
    handoff_policy: String,
    open_schedule: bool,
    tracks: usize,
    participants: usize,
    committed: bool,
    matrix: Vec<MatrixTrackDto>,
    seed_owners: Vec<SeedOwnerDto>,
    graph: GraphDto,
    groups: Vec<PlannerGroupDto>,
    phases: Vec<PlannerPhaseDto>,
    warnings: Vec<PlannerWarningDto>,
    fun_score: Option<u32>,
    fun_guardrails: Vec<GuardrailDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeedOwnerDto {
    track_code: String,
    owner_id: String,
    owner_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatrixTrackDto {
    track_code: String,
    owner_id: String,
    owner_group: String,
    cells: Vec<PlanCellDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEditCandidateDto {
    participant_id: String,
    participant_name: String,
    current_track: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanCellDto {
    round: u32,
    track_code: String,
    participant_id: String,
    participant_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphDto {
    round: u32,
    edges: Vec<GraphEdgeDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphEdgeDto {
    track_code: String,
    participant_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlannerGroupDto {
    name: String,
    size: usize,
    tracks: usize,
    track_codes: Vec<String>,
    participants: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlannerPhaseDto {
    index: u32,
    start_round: u32,
    rounds: u32,
    global_pool: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlannerWarningDto {
    id: String,
    severity: String,
    title: String,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GuardrailDto {
    label: String,
    value: String,
}

fn default_draft() -> EventDraftDto {
    EventDraftDto {
        name: String::new(),
        mode: "Classic".to_string(),
        rounds: None,
        claim_window_hours: 24,
        round_window_hours: 24,
        quiet_hours_start: 22,
        quiet_hours_end: 8,
        allowlist: swap_files::DEFAULT_EXTENSIONS
            .iter()
            .map(|extension| (*extension).to_string())
            .collect(),
        pack_type: "Module template + samples".to_string(),
        pack_required: true,
        group_mode: "Lookup".to_string(),
        group_count: 1,
        group_size: 5,
        rotation: "Participants".to_string(),
        swap_cadence: "K rounds".to_string(),
        schedule_visibility: "Hidden".to_string(),
        handoff_policy: "Strict".to_string(),
    }
}

fn draft_from(event: &EventRecord, settings: &EventSettings) -> EventDraftDto {
    EventDraftDto {
        name: event.name.clone(),
        mode: match settings.mode {
            Mode::Classic => "Classic".to_string(),
            Mode::Async => "Async".to_string(),
        },
        rounds: settings.rounds,
        claim_window_hours: settings.claim_window_secs / 3600,
        round_window_hours: settings.round_window_secs / 3600,
        quiet_hours_start: settings.quiet_hours_start,
        quiet_hours_end: settings.quiet_hours_end,
        allowlist: settings.allowed_extensions.clone(),
        pack_type: settings
            .pack_type
            .clone()
            .unwrap_or_else(|| "Module template + samples".to_string()),
        pack_required: settings.pack_required.unwrap_or(true),
        group_mode: settings
            .group_mode
            .clone()
            .unwrap_or_else(|| "Lookup".to_string()),
        group_count: settings.group_count.unwrap_or(1),
        group_size: settings.group_size.unwrap_or(5),
        rotation: settings
            .rotation
            .clone()
            .unwrap_or_else(|| "Participants".to_string()),
        swap_cadence: settings
            .swap_cadence
            .clone()
            .unwrap_or_else(|| "K rounds".to_string()),
        schedule_visibility: settings
            .schedule_visibility
            .clone()
            .unwrap_or_else(|| "Hidden".to_string()),
        handoff_policy: settings
            .handoff_policy
            .clone()
            .unwrap_or_else(|| "Strict".to_string()),
    }
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("monsquaz.db"))
}

fn open_db(app: &AppHandle) -> Result<Db, String> {
    Db::open(database_path(app)?).map_err(|error| error.to_string())
}

fn open_store(app: &AppHandle) -> Result<swap_files::BlobStore, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(swap_files::BlobStore::new(directory.join("files")))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn current_event(db: &Db) -> Result<Option<EventRecord>, String> {
    let Some(id) =
        MetaRepo::get(db.conn(), "host.current_event").map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    EventRepo::get(db.conn(), &id).map_err(|error| error.to_string())
}

fn summary(db: &Db, event: &EventRecord) -> Result<EventSummaryDto, String> {
    let participants =
        ParticipantRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let tracks =
        TrackRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let plan = PlanRepo::latest(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let assignments =
        AssignmentRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let current_exchange = assignments
        .iter()
        .map(|assignment| assignment.round + 1)
        .max()
        .unwrap_or(0);
    let seed_round = RoundRepo::get(db.conn(), &event.id, 1).map_err(|error| error.to_string())?;
    let seed_round_open = seed_round
        .as_ref()
        .is_some_and(|round| round.status == RoundStatus::Open);
    let seed_deadline_at = seed_round.as_ref().and_then(|round| round.deadline_at);
    let late_seed_count = LateSeedRepo::list_pending(db.conn(), &event.id)
        .map_err(|error| error.to_string())?
        .len();
    let pack_id = PackRepo::get_for_event(db.conn(), &event.id)
        .map_err(|error| error.to_string())?
        .map(|pack| pack.id);
    let settings: EventSettings = serde_json::from_str(&event.settings_json).unwrap_or_default();
    let active = participants
        .iter()
        .filter(|participant| {
            matches!(
                participant.membership,
                Membership::SignedUp | Membership::Seeded | Membership::Active
            )
        })
        .count();
    let fill_ins = participants
        .iter()
        .filter(|participant| {
            participant.role == Role::FillIn
                && matches!(
                    participant.membership,
                    Membership::Seeded | Membership::Active
                )
        })
        .count();
    let exchange_rounds = plan
        .as_ref()
        .map(|plan| plan.rounds)
        .or(settings.rounds)
        .unwrap_or_else(|| active.saturating_sub(1) as u32);
    let total_rounds = if exchange_rounds > 0 {
        exchange_rounds + 1
    } else if seed_round_open {
        1
    } else {
        0
    };
    let round = if seed_round_open {
        1
    } else if current_exchange > 0 {
        current_exchange + 1
    } else {
        0
    };
    Ok(EventSummaryDto {
        id: event.id.clone(),
        name: event.name.clone(),
        mode: match settings.mode {
            Mode::Classic => "Classic".to_string(),
            Mode::Async => "Async".to_string(),
        },
        stage: match event.state {
            EventState::Draft => "Setup",
            EventState::Signup => "Join phase",
            EventState::ScheduleReview => "Schedule review",
            EventState::Active => "Active",
            EventState::Paused => "Paused",
            EventState::Reveal | EventState::Archived => "Reveal",
        }
        .to_string(),
        participants: active,
        seeded: tracks.len(),
        fill_ins,
        tracks: tracks.len(),
        round,
        rounds: total_rounds,
        exchange_rounds,
        seed_round_open,
        seed_deadline_at,
        late_seed_count,
        pack_id,
        open_schedule: settings.schedule_visibility.as_deref() == Some("Open"),
        handoff_policy: settings
            .handoff_policy
            .clone()
            .unwrap_or_else(|| "Strict".to_string()),
    })
}

#[tauri::command]
pub fn host_event_setup(app: AppHandle) -> Result<SetupDataDto, String> {
    let db = open_db(&app)?;
    setup_data(&db)
}

fn setup_data(db: &Db) -> Result<SetupDataDto, String> {
    let Some(event) = current_event(db)? else {
        return Ok(SetupDataDto {
            first_run: true,
            event: None,
            draft: default_draft(),
            warnings: Vec::new(),
        });
    };
    let settings: EventSettings = serde_json::from_str(&event.settings_json).unwrap_or_default();
    Ok(SetupDataDto {
        first_run: false,
        event: Some(summary(db, &event)?),
        draft: draft_from(&event, &settings),
        warnings: Vec::new(),
    })
}

#[tauri::command]
pub fn host_set_participant_paused(
    app: AppHandle,
    user_id: String,
    paused: bool,
) -> Result<PeopleDataDto, String> {
    let db = open_db(&app)?;
    set_participant_paused(&db, &user_id, paused)
}

fn set_participant_paused(db: &Db, user_id: &str, paused: bool) -> Result<PeopleDataDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let mut participant = ParticipantRepo::get(db.conn(), &event.id, user_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the participant is not in this event".to_string())?;
    if matches!(
        participant.membership,
        Membership::Left | Membership::Removed
    ) {
        return Err("the participant has left the event".to_string());
    }
    participant.readiness = if paused {
        swap_core::Readiness::Paused
    } else {
        swap_core::Readiness::Ready
    };
    participant.updated_at = now_secs();
    ParticipantRepo::upsert(db.conn(), &participant).map_err(|error| error.to_string())?;
    AuditRepo::insert(
        db.conn(),
        &event.id,
        now_secs(),
        "Host",
        if paused {
            "Participant paused"
        } else {
            "Participant resumed"
        },
        Some(&participant.display_name),
        "{}",
    )
    .map_err(|error| error.to_string())?;
    people_data(db)
}

#[tauri::command]
pub fn host_add_availability(
    app: AppHandle,
    participant_id: String,
    starts_at: i64,
    ends_at: i64,
    note: Option<String>,
) -> Result<PeopleDataDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    add_availability(
        &db,
        &event.id,
        &participant_id,
        starts_at,
        ends_at,
        note.as_deref(),
        now_secs(),
    )
    .map_err(|error| error.to_string())?;
    people_data(&db)
}

#[tauri::command]
pub fn host_remove_availability(app: AppHandle, id: String) -> Result<PeopleDataDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    remove_availability(&db, &event.id, &id, now_secs()).map_err(|error| error.to_string())?;
    people_data(&db)
}

#[tauri::command]
pub fn host_people(app: AppHandle) -> Result<PeopleDataDto, String> {
    let db = open_db(&app)?;
    people_data(&db)
}

fn people_data(db: &Db) -> Result<PeopleDataDto, String> {
    let Some(event) = current_event(db)? else {
        return Ok(PeopleDataDto {
            event: None,
            rows: Vec::new(),
        });
    };
    let tracks =
        TrackRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let participants =
        ParticipantRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let availability = AvailabilityRepo::list_for_event(db.conn(), &event.id)
        .map_err(|error| error.to_string())?;
    let rows = participants
        .into_iter()
        .map(|participant| PersonRowDto {
            away: availability
                .iter()
                .filter(|range| range.participant_id == participant.user_id)
                .map(|range| AwayDto {
                    id: range.id.clone(),
                    starts_at: range.starts_at,
                    ends_at: range.ends_at,
                    note: range.note.clone(),
                })
                .collect(),
            id: participant.user_id.clone(),
            display_name: participant.display_name,
            role: match participant.role {
                Role::Participant => "Seed",
                Role::FillIn => "Fill-in",
            }
            .to_string(),
            status: if participant.membership == Membership::Left {
                "Left"
            } else if participant.readiness == swap_core::Readiness::Paused {
                "Paused"
            } else {
                "Ready"
            }
            .to_string(),
            seed_track: tracks
                .iter()
                .find(|track| track.owner_id == participant.user_id)
                .map(|track| track.code.clone()),
            timezone: participant.timezone,
            max_handoffs: participant.max_handoffs,
            joined: String::new(),
            delivery_risk: false,
        })
        .collect();
    Ok(PeopleDataDto {
        event: Some(summary(db, &event)?),
        rows,
    })
}

#[tauri::command]
pub fn host_plan(app: AppHandle) -> Result<PlanDataDto, String> {
    let db = open_db(&app)?;
    plan_data(&db)
}

fn plan_data(db: &Db) -> Result<PlanDataDto, String> {
    let Some(event) = current_event(db)? else {
        return Ok(PlanDataDto {
            status: "No event".to_string(),
            event: None,
            version: 0,
            commitment: String::new(),
            rounds: 0,
            exchange_rounds: 0,
            total_rounds: 0,
            gaps: 0,
            handoff_policy: "Strict".to_string(),
            open_schedule: false,
            tracks: 0,
            participants: 0,
            committed: false,
            matrix: Vec::new(),
            seed_owners: Vec::new(),
            graph: GraphDto {
                round: 0,
                edges: Vec::new(),
            },
            groups: Vec::new(),
            phases: Vec::new(),
            warnings: Vec::new(),
            fun_score: None,
            fun_guardrails: Vec::new(),
        });
    };
    let summary = summary(db, &event)?;
    let Some(plan) = PlanRepo::latest(db.conn(), &event.id).map_err(|error| error.to_string())?
    else {
        return Ok(PlanDataDto {
            status: "No plan".to_string(),
            event: Some(summary),
            version: 0,
            commitment: String::new(),
            rounds: event_rounds(db, &event)?,
            exchange_rounds: event_rounds(db, &event)?,
            total_rounds: event_rounds(db, &event)? + 1,
            gaps: 0,
            handoff_policy: "Strict".to_string(),
            open_schedule: false,
            tracks: TrackRepo::list_for_event(db.conn(), &event.id)
                .map_err(|error| error.to_string())?
                .len(),
            participants: ParticipantRepo::list_for_event(db.conn(), &event.id)
                .map_err(|error| error.to_string())?
                .len(),
            committed: false,
            matrix: Vec::new(),
            seed_owners: Vec::new(),
            graph: GraphDto {
                round: 0,
                edges: Vec::new(),
            },
            groups: Vec::new(),
            phases: Vec::new(),
            warnings: Vec::new(),
            fun_score: None,
            fun_guardrails: Vec::new(),
        });
    };
    let assignments = PlanRepo::list_assignments(db.conn(), &event.id, plan.version)
        .map_err(|error| error.to_string())?;
    let participants =
        ParticipantRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let name_for = |user_id: &str| {
        participants
            .iter()
            .find(|participant| participant.user_id == user_id)
            .map(|participant| participant.display_name.clone())
            .unwrap_or_else(|| user_id.to_string())
    };
    let tracks =
        TrackRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let settings: EventSettings = if plan.settings_json.trim() == "{}" {
        serde_json::from_str(&event.settings_json).unwrap_or_default()
    } else {
        serde_json::from_str(&plan.settings_json).unwrap_or_default()
    };
    let (groups, group_of) = planner_groups(&settings, &participants, &tracks);
    let mut warnings = planner_group_warnings(&settings, &participants, plan.rounds);
    let phases = planner_phases(&settings, &assignments, &tracks, &group_of, plan.rounds);
    for phase in phases.iter().filter(|phase| phase.global_pool) {
        warnings.push(PlannerWarningDto {
            id: format!("group-fallback-{}", phase.index),
            severity: "warn".to_string(),
            title: "The planner used the global pool in a phase".to_string(),
            detail: format!(
                "Phase {} could not fill inside its groups. It uses all tracks and preserves the seen ledger. Later phases try groups again.",
                phase.index + 1
            ),
        });
    }
    let group_for = |owner_id: &str| {
        group_of
            .get(owner_id)
            .cloned()
            .unwrap_or_else(|| "—".to_string())
    };
    let matrix = tracks
        .iter()
        .map(|track| MatrixTrackDto {
            track_code: track.code.clone(),
            owner_id: track.owner_id.clone(),
            owner_group: group_for(&track.owner_id),
            cells: (0..plan.rounds)
                .map(|round| {
                    let participant = assignments
                        .iter()
                        .find(|assignment| {
                            assignment.round == round && assignment.track_code == track.code
                        })
                        .map(|assignment| assignment.participant_id.as_str())
                        .unwrap_or("");
                    PlanCellDto {
                        round,
                        track_code: track.code.clone(),
                        participant_id: participant.to_string(),
                        participant_name: name_for(participant),
                    }
                })
                .collect(),
        })
        .collect();
    let edges = assignments
        .iter()
        .filter(|assignment| assignment.round == 0)
        .map(|assignment| GraphEdgeDto {
            track_code: assignment.track_code.clone(),
            participant_name: name_for(&assignment.participant_id),
        })
        .collect();
    let committed = matches!(
        event.state,
        EventState::Active | EventState::Paused | EventState::Reveal | EventState::Archived
    );
    let settings: EventSettings = serde_json::from_str(&event.settings_json).unwrap_or_default();
    let seed_owners = tracks
        .iter()
        .map(|track| SeedOwnerDto {
            track_code: track.code.clone(),
            owner_id: track.owner_id.clone(),
            owner_name: participants
                .iter()
                .find(|participant| participant.user_id == track.owner_id)
                .map(|participant| participant.display_name.clone())
                .unwrap_or_else(|| track.owner_id.clone()),
        })
        .collect();
    Ok(PlanDataDto {
        status: if committed { "Committed" } else { "Draft" }.to_string(),
        event: Some(summary),
        version: plan.version,
        commitment: plan.commitment.clone(),
        rounds: plan.rounds,
        exchange_rounds: plan.rounds,
        total_rounds: plan.rounds + 1,
        gaps: (plan.rounds as usize * participants.len()).saturating_sub(assignments.len()) as u32,
        handoff_policy: settings
            .handoff_policy
            .clone()
            .unwrap_or_else(|| "Strict".to_string()),
        open_schedule: settings.schedule_visibility.as_deref() == Some("Open"),
        tracks: tracks.len(),
        participants: participants.len(),
        committed,
        matrix,
        seed_owners,
        graph: GraphDto { round: 0, edges },
        groups,
        phases,
        warnings,
        fun_score: None,
        fun_guardrails: Vec::new(),
    })
}

fn planner_groups(
    settings: &EventSettings,
    participants: &[swap_db::ParticipantRecord],
    tracks: &[swap_db::TrackRecord],
) -> (
    Vec<PlannerGroupDto>,
    std::collections::BTreeMap<String, String>,
) {
    let count = settings.group_count.unwrap_or(1) as usize;
    if settings.mode != Mode::Classic || count <= 1 {
        return (Vec::new(), std::collections::BTreeMap::new());
    }
    let mut active: Vec<&swap_db::ParticipantRecord> = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active
                && participant.role == Role::Participant
                && participant.readiness == swap_core::Readiness::Ready
        })
        .collect();
    active.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    if active.is_empty() {
        return (Vec::new(), std::collections::BTreeMap::new());
    }
    let count = count.min(active.len());
    let sizes =
        swap_core::group_sizes(active.len(), count as u32, settings.group_size.unwrap_or(0));
    if sizes.len() <= 1 {
        return (Vec::new(), std::collections::BTreeMap::new());
    }
    let mut groups = Vec::new();
    let mut group_of = std::collections::BTreeMap::new();
    let mut start = 0;
    for (index, group_size) in sizes.iter().enumerate() {
        let end = start + *group_size as usize;
        let members = &active[start..end];
        start = end;
        let name = group_name(index);
        let mut track_codes: Vec<String> = tracks
            .iter()
            .filter(|track| {
                members
                    .iter()
                    .any(|participant| participant.user_id == track.owner_id)
            })
            .map(|track| track.code.clone())
            .collect();
        track_codes.sort();
        let owned = track_codes.len();
        for participant in members {
            group_of.insert(participant.user_id.clone(), name.clone());
        }
        groups.push(PlannerGroupDto {
            name,
            size: members.len(),
            tracks: owned,
            track_codes,
            participants: members
                .iter()
                .map(|participant| participant.display_name.clone())
                .collect(),
        });
    }
    (groups, group_of)
}

fn group_name(index: usize) -> String {
    char::from(b'A' + (index % 26) as u8).to_string()
}

fn planner_group_warnings(
    settings: &EventSettings,
    participants: &[swap_db::ParticipantRecord],
    rounds: u32,
) -> Vec<PlannerWarningDto> {
    if settings.mode != Mode::Classic
        || matches!(settings.group_mode.as_deref(), Some("Single group"))
    {
        return Vec::new();
    }
    let active = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active
                && participant.role == Role::Participant
                && participant.readiness == swap_core::Readiness::Ready
        })
        .count();
    if active == 0 {
        return Vec::new();
    }
    let count = settings
        .group_count
        .unwrap_or(1)
        .clamp(1, swap_core::MAX_GROUP_COUNT);
    let size = settings.group_size.unwrap_or(0);
    let sizes = swap_core::group_sizes(active, count, size);
    let cadence = settings.swap_cadence.as_deref().unwrap_or("K rounds");
    swap_core::group_warnings(&sizes, rounds, cadence)
        .into_iter()
        .map(|warning| match warning {
            swap_core::GroupWarning::Small { size } => PlannerWarningDto {
                id: "group-size-small".to_string(),
                severity: "info".to_string(),
                title: "Group size is below the soft minimum".to_string(),
                detail: format!(
                    "The smallest group has {size} people. The soft minimum is 5. The host can continue."
                ),
            },
            swap_core::GroupWarning::Large { size } => PlannerWarningDto {
                id: "group-size-large".to_string(),
                severity: "info".to_string(),
                title: "Group size is above the soft maximum".to_string(),
                detail: format!(
                    "The largest group has {size} people. The soft maximum is 12. The host can continue."
                ),
            },
            swap_core::GroupWarning::Unequal { smallest, largest } => PlannerWarningDto {
                id: "group-size-unequal".to_string(),
                severity: "info".to_string(),
                title: "Groups are unequal".to_string(),
                detail: format!(
                    "The groups have {smallest} and {largest} people. Rebalance them if the event allows it."
                ),
            },
            swap_core::GroupWarning::Cadence { phases } => PlannerWarningDto {
                id: "group-cadence".to_string(),
                severity: "info".to_string(),
                title: "The swap cadence is outside the target".to_string(),
                detail: format!(
                    "The default cadence gives {phases} phases. The target is 3 to 5 phases. The host can continue."
                ),
            },
            swap_core::GroupWarning::Exhausted { size, rounds } => PlannerWarningDto {
                id: "group-exhausted".to_string(),
                severity: "info".to_string(),
                title: "The group cannot exhaust before the event ends".to_string(),
                detail: format!(
                    "A group of {size} cannot exhaust in {rounds} rounds. Use a K-round swap."
                ),
            },
        })
        .collect()
}

fn planner_phases(
    settings: &EventSettings,
    assignments: &[swap_db::PlanAssignmentRecord],
    tracks: &[swap_db::TrackRecord],
    group_of: &std::collections::BTreeMap<String, String>,
    rounds: u32,
) -> Vec<PlannerPhaseDto> {
    if settings.mode != Mode::Classic || rounds == 0 {
        return Vec::new();
    }
    let mut sizes: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for group in group_of.values() {
        *sizes.entry(group.as_str()).or_default() += 1;
    }
    let group_count = sizes.len();
    if group_count <= 1 {
        return Vec::new();
    }
    let largest = sizes.values().copied().max().unwrap_or(1);
    let cadence = if settings.swap_cadence.as_deref() == Some("Exhausted") {
        largest.saturating_sub(1).max(1)
    } else {
        swap_core::default_cadence(rounds)
    };
    if cadence == 0 {
        return Vec::new();
    }
    let group_index = |name: &str| -> Option<usize> {
        name.as_bytes().first().map(|byte| (byte - b'A') as usize)
    };
    let owners: std::collections::BTreeMap<&str, &str> = tracks
        .iter()
        .map(|track| (track.code.as_str(), track.owner_id.as_str()))
        .collect();
    let phase_count = rounds.div_ceil(cadence);
    (0..phase_count)
        .map(|phase| {
            let start_round = phase * cadence;
            let phase_rounds = cadence.min(rounds - start_round);
            let mut global_pool = false;
            for assignment in assignments.iter().filter(|assignment| {
                assignment.round >= start_round && assignment.round < start_round + phase_rounds
            }) {
                let Some(person_group) = group_of
                    .get(&assignment.participant_id)
                    .and_then(|name| group_index(name))
                else {
                    continue;
                };
                let Some(owner) = owners.get(assignment.track_code.as_str()) else {
                    continue;
                };
                let Some(track_group) = group_of.get(*owner).and_then(|name| group_index(name))
                else {
                    continue;
                };
                let expected =
                    (track_group + group_count - (phase as usize % group_count)) % group_count;
                if person_group != expected {
                    global_pool = true;
                    break;
                }
            }
            PlannerPhaseDto {
                index: phase,
                start_round,
                rounds: phase_rounds,
                global_pool,
            }
        })
        .collect()
}

fn event_rounds(db: &Db, event: &EventRecord) -> Result<u32, String> {
    let settings: EventSettings = serde_json::from_str(&event.settings_json).unwrap_or_default();
    if let Some(rounds) = settings.rounds {
        return Ok(rounds);
    }
    let active = ParticipantRepo::list_for_event(db.conn(), &event.id)
        .map_err(|error| error.to_string())?
        .iter()
        .filter(|participant| {
            matches!(
                participant.membership,
                Membership::SignedUp | Membership::Seeded | Membership::Active
            )
        })
        .count();
    Ok(active.saturating_sub(1) as u32)
}

#[tauri::command]
pub fn host_create_event(app: AppHandle, draft: EventDraftDto) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    create_host_event(&db, draft)
}

fn create_host_event(db: &Db, draft: EventDraftDto) -> Result<EventSummaryDto, String> {
    if draft.name.trim().is_empty() {
        return Err("the event name is empty".to_string());
    }
    let settings = EventSettings {
        mode: if draft.mode == "Async" {
            Mode::Async
        } else {
            Mode::Classic
        },
        rounds: draft.rounds,
        seed_window_secs: 7 * 24 * 60 * 60,
        final_round_window_secs: 48 * 60 * 60,
        seed_min: 2,
        claim_window_secs: (draft.claim_window_hours.max(1)) * 3600,
        round_window_secs: (draft.round_window_hours.max(1)) * 3600,
        max_file_bytes: swap_files::DEFAULT_MAX_SIZE,
        allowed_extensions: draft.allowlist,
        pack_type: Some(draft.pack_type),
        pack_required: Some(draft.pack_required),
        group_mode: Some(draft.group_mode),
        group_count: Some(draft.group_count),
        group_size: Some(draft.group_size),
        rotation: Some(draft.rotation),
        scheduler: None,
        swap_cadence: Some(draft.swap_cadence),
        schedule_visibility: Some(draft.schedule_visibility),
        handoff_policy: Some(draft.handoff_policy),
        quiet_hours_start: draft.quiet_hours_start,
        quiet_hours_end: draft.quiet_hours_end,
    };
    settings.validate().map_err(|error| error.to_string())?;
    let now = now_secs();
    let id = format!("evt-{}", now_nanos());
    create_event(db, &id, &draft.name, now as u64, &settings, now)
        .map_err(|error| error.to_string())?;
    AuditRepo::insert(
        db.conn(),
        &id,
        now,
        "Host",
        "Event created",
        Some(&draft.name),
        &serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string()),
    )
    .map_err(|error| error.to_string())?;
    MetaRepo::set(db.conn(), "host.current_event", &id).map_err(|error| error.to_string())?;
    let event = EventRepo::get(db.conn(), &id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the event was not stored".to_string())?;
    summary(db, &event)
}

#[tauri::command]
pub fn host_generate_plan(app: AppHandle) -> Result<PlanDataDto, String> {
    let db = open_db(&app)?;
    generate_host_plan(&db)
}

#[tauri::command]
pub fn host_repair_groups(app: AppHandle) -> Result<PlanDataDto, String> {
    let db = open_db(&app)?;
    repair_host_groups(&db)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncWaveDto {
    offers: usize,
}

#[tauri::command]
pub fn host_async_dispatch(app: AppHandle) -> Result<AsyncWaveDto, String> {
    let db = open_db(&app)?;
    let result = dispatch_host_async_wave(&db);
    if result.as_ref().is_ok_and(|wave| wave.offers > 0) {
        #[cfg(feature = "live")]
        app.state::<crate::live_runtime::LiveRuntime>().wake();
    }
    result
}

fn dispatch_host_async_wave(db: &Db) -> Result<AsyncWaveDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    let outcome = dispatch_async_wave(db, &event.id, now).map_err(|error| error.to_string())?;
    Ok(AsyncWaveDto {
        offers: outcome.offers,
    })
}

fn repair_host_groups(db: &Db) -> Result<PlanDataDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    let outcome = repair_group_schedule(db, &event.id, now).map_err(|error| error.to_string())?;
    AuditRepo::insert(
        db.conn(),
        &event.id,
        now,
        "Host",
        "Groups repaired",
        Some(&format!("Plan v{}", outcome.version)),
        &serde_json::json!({
            "from": outcome.previous_sizes,
            "to": outcome.sizes,
            "commitment": outcome.commitment
        })
        .to_string(),
    )
    .map_err(|error| error.to_string())?;
    plan_data(db)
}

fn generate_host_plan(db: &Db) -> Result<PlanDataDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    if event.state == EventState::Signup {
        return Err("Lock the roster before you generate the schedule.".to_string());
    }
    let outcome = generate_schedule(db, &event.id, now).map_err(|error| error.to_string())?;
    AuditRepo::insert(
        db.conn(),
        &event.id,
        now,
        "Host",
        "Schedule generated",
        Some(&format!("Plan v{}", outcome.version)),
        &serde_json::json!({ "rounds": outcome.rounds, "commitment": outcome.commitment })
            .to_string(),
    )
    .map_err(|error| error.to_string())?;
    plan_data(db)
}

#[tauri::command]
pub fn host_upload_pack(app: AppHandle, path: String) -> Result<PackDto, String> {
    let db = open_db(&app)?;
    let store = open_store(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    let source = Path::new(&path);
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the pack file has no name".to_string())?;
    let bytes = fs::read(source).map_err(|error| error.to_string())?;
    let outcome = upload_pack(&db, &store, &event.id, filename, &bytes, now_secs())
        .map_err(|error| error.to_string())?;
    Ok(PackDto {
        id: outcome.id,
        filename: filename.to_string(),
        sha256: outcome.sha256,
        size: outcome.size,
        locked: false,
    })
}

#[tauri::command]
pub fn host_open_seed_round(
    app: AppHandle,
    deadline_at: i64,
    channel_id: Option<String>,
) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    open_seed_round(
        &db,
        &event.id,
        deadline_at,
        channel_id.as_deref(),
        now_secs(),
    )
    .map_err(|error| error.to_string())?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    summary(&db, &event)
}

#[tauri::command]
pub fn host_extend_seed_deadline(
    app: AppHandle,
    deadline_at: i64,
) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    extend_seed_deadline(&db, &event.id, deadline_at, now_secs())
        .map_err(|error| error.to_string())?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    summary(&db, &event)
}

#[tauri::command]
pub fn host_reopen_seed_round(app: AppHandle, deadline_at: i64) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    reopen_seed_round(&db, &event.id, deadline_at, now_secs())
        .map_err(|error| error.to_string())?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    summary(&db, &event)
}

#[tauri::command]
pub fn host_pending_late_seeds(app: AppHandle) -> Result<Vec<LateSeedDto>, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    Ok(pending_late_seeds(&db, &event.id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|seed| LateSeedDto {
            id: seed.id,
            display_name: seed.display_name,
            track_code: seed.track_code,
            created_at: seed.created_at,
        })
        .collect())
}

#[tauri::command]
pub fn host_decide_late_seed(
    app: AppHandle,
    id: String,
    accept: bool,
    reason: Option<String>,
) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    if accept {
        accept_late_seed(&db, &event.id, &id, now_secs()).map_err(|error| error.to_string())?;
    } else {
        reject_late_seed(
            &db,
            &event.id,
            &id,
            reason.as_deref().unwrap_or("not a fit for this event"),
            now_secs(),
        )
        .map_err(|error| error.to_string())?;
    }
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    summary(&db, &event)
}

#[tauri::command]
pub fn host_move_round_deadline(
    app: AppHandle,
    round_no: u32,
    deadline_at: i64,
    reason: String,
) -> Result<EventSummaryDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    move_round_deadline(&db, &event.id, round_no, deadline_at, &reason, now_secs())
        .map_err(|error| error.to_string())?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    summary(&db, &event)
}

#[tauri::command]
pub fn host_verify_commitment(app: AppHandle) -> Result<VerifyDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    let outcome = verify_commitment(&db, &event.id).map_err(|error| error.to_string())?;
    Ok(VerifyDto {
        recipe: outcome.recipe,
        commitment: outcome.commitment,
        verified: outcome.verified,
    })
}

#[tauri::command]
pub fn host_lock_roster(app: AppHandle) -> Result<RosterLockDto, String> {
    let db = open_db(&app)?;
    lock_host_roster(&db)
}

fn lock_host_roster(db: &Db) -> Result<RosterLockDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    let outcome = lock_roster(db, &event.id, now).map_err(|error| error.to_string())?;
    Ok(RosterLockDto {
        active: outcome.active,
        fill_ins: outcome.fill_ins,
    })
}

#[tauri::command]
pub fn host_open_round(app: AppHandle, round: u32) -> Result<RoundOpenDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    let outcome = open_round(&db, &event.id, round, now).map_err(|error| error.to_string())?;
    #[cfg(feature = "live")]
    app.state::<crate::live_runtime::LiveRuntime>().wake();
    Ok(RoundOpenDto {
        round,
        offers: outcome.offers,
    })
}

#[tauri::command]
pub fn host_reveal(app: AppHandle) -> Result<RevealDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    let settings = host_settings(app.clone()).unwrap_or_default();
    let channel = settings.reveal_channel_id.trim();
    if channel.is_empty() {
        return Err("Map the reveal channel before you reveal the event.".to_string());
    }
    let now = now_secs();
    match reveal_with_channel(&db, &event.id, Some(channel), now) {
        Ok(outcome) => {
            AuditRepo::insert(
                db.conn(),
                &event.id,
                now,
                "Host",
                "Event revealed",
                None,
                &serde_json::json!({ "tracks": outcome.tracks.len() }).to_string(),
            )
            .map_err(|error| error.to_string())?;
            #[cfg(feature = "live")]
            app.state::<crate::live_runtime::LiveRuntime>().wake();
            Ok(RevealDto {
                published: true,
                blocked: Vec::new(),
                tracks: outcome.tracks.into_iter().map(reveal_track_dto).collect(),
            })
        }
        Err(EngineError::PendingAssignments { ids, .. }) => Ok(RevealDto {
            published: false,
            blocked: ids
                .into_iter()
                .map(|id| {
                    AssignmentRepo::get(db.conn(), &id)
                        .ok()
                        .flatten()
                        .map(|assignment| {
                            format!(
                                "Track {} · round {}",
                                assignment.track_code,
                                assignment.round + 1
                            )
                        })
                        .unwrap_or(id)
                })
                .collect(),
            tracks: Vec::new(),
        }),
        Err(error) => Err(error.to_string()),
    }
}

fn reveal_track_dto(track: TrackCredits) -> RevealTrackDto {
    RevealTrackDto {
        track_code: track.track_code,
        seed_owner_name: track.seed_owner_name,
        seed_owner: track.seed_owner,
        entries: track
            .entries
            .into_iter()
            .map(|entry: CreditEntry| RevealEntryDto {
                round: entry.round,
                display_name: entry.display_name,
                status: format!("{:?}", entry.status),
                fill_in: entry.fill_in,
            })
            .collect(),
    }
}

fn reveal_package_dto(db: &Db) -> Result<RevealPackageDto, String> {
    let Some(event) = current_event(db)? else {
        return Ok(RevealPackageDto {
            event_name: String::new(),
            event_state: "Draft".to_string(),
            published: false,
            album: Vec::new(),
        });
    };
    let package = reveal_package(db, &event.id).map_err(|error| error.to_string())?;
    Ok(RevealPackageDto {
        event_name: package.event_name,
        event_state: format!("{:?}", package.event_state),
        published: package.published,
        album: package
            .album
            .into_iter()
            .map(|track| RevealShowTrackDto {
                position: track.position,
                track_code: track.track_code,
                title: track.title,
                seed_owner_id: track.seed_owner_id,
                seed_owner_name: track.seed_owner_name,
                credits: track
                    .credits
                    .into_iter()
                    .map(|entry| RevealShowCreditDto {
                        round: entry.round,
                        participant_id: entry.participant_id,
                        display_name: entry.display_name,
                        status: format!("{:?}", entry.status),
                        fill_in: entry.fill_in,
                    })
                    .collect(),
                comments: track
                    .comments
                    .into_iter()
                    .map(|comment| RevealShowCommentDto {
                        id: comment.id,
                        participant_id: comment.participant_id,
                        author_name: comment.author_name,
                        kind: comment.kind,
                        body: comment.body,
                        created_at: comment.created_at,
                    })
                    .collect(),
                versions: track
                    .versions
                    .into_iter()
                    .map(|version| RevealShowVersionDto {
                        version: version.version,
                        holder_id: version.holder_id,
                        holder_name: version.holder_name,
                        role: match version.role {
                            swap_engine::RevealVersionRole::Seed => "seed".to_string(),
                            swap_engine::RevealVersionRole::Edit => "edit".to_string(),
                        },
                        round: version.round,
                        file_id: crate::preview::register(&version.file_id),
                        extension: version.extension,
                        created_at: version.created_at,
                        playable: version.playable,
                        final_version: version.final_version,
                    })
                    .collect(),
                stats: RevealShowStatsDto {
                    edits: track.stats.edits,
                    fill_ins: track.stats.fill_ins,
                    skips: track.stats.skips,
                    contributors: track.stats.contributors,
                },
            })
            .collect(),
    })
}

#[tauri::command]
pub fn host_reveal_package(app: AppHandle) -> Result<RevealPackageDto, String> {
    let db = open_db(&app)?;
    reveal_package_dto(&db)
}

#[tauri::command]
pub fn host_set_track_title(
    app: AppHandle,
    track_code: String,
    title: String,
) -> Result<RevealPackageDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    set_track_title(&db, &event.id, &track_code, &title, "host", now_secs())
        .map_err(|error| error.to_string())?;
    reveal_package_dto(&db)
}

#[tauri::command]
pub fn host_add_track_comment(
    app: AppHandle,
    track_code: String,
    participant_id: String,
    kind: String,
    body: String,
) -> Result<RevealPackageDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    add_track_comment(
        &db,
        &event.id,
        &track_code,
        &participant_id,
        &kind,
        &body,
        now_secs(),
    )
    .map_err(|error| error.to_string())?;
    reveal_package_dto(&db)
}

#[tauri::command]
pub fn host_redact_track_comment(
    app: AppHandle,
    comment_id: String,
) -> Result<RevealPackageDto, String> {
    let db = open_db(&app)?;
    redact_track_comment(&db, &comment_id, now_secs()).map_err(|error| error.to_string())?;
    reveal_package_dto(&db)
}

#[tauri::command]
pub fn host_decide_extension(
    app: AppHandle,
    decision_id: String,
    approve: bool,
    reason: String,
) -> Result<String, String> {
    let db = open_db(&app)?;
    let outcome = swap_engine::decide_extension(&db, &decision_id, approve, &reason, now_secs())
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "live")]
    app.state::<crate::live_runtime::LiveRuntime>().wake();
    Ok(match outcome.status {
        swap_engine::ExtensionStatus::Approved => "approved",
        swap_engine::ExtensionStatus::Refused => "refused",
        swap_engine::ExtensionStatus::PendingHost => "pending",
    }
    .to_string())
}

#[tauri::command]
pub fn host_decide_deadline(
    app: AppHandle,
    assignment_id: String,
    action: String,
    hours: i64,
    reason: String,
) -> Result<(), String> {
    let db = open_db(&app)?;
    swap_engine::decide_deadline(&db, &assignment_id, &action, hours, &reason, now_secs())
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "live")]
    app.state::<crate::live_runtime::LiveRuntime>().wake();
    Ok(())
}

#[tauri::command]
pub fn host_plan_edit_candidates(
    app: AppHandle,
    round: u32,
    track_code: String,
) -> Result<Vec<PlanEditCandidateDto>, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    plan_edit_candidates(&db, &event.id, round, &track_code)
        .map(|candidates| {
            candidates
                .into_iter()
                .map(|candidate| PlanEditCandidateDto {
                    participant_id: candidate.participant_id,
                    participant_name: candidate.participant_name,
                    current_track: candidate.current_track,
                })
                .collect()
        })
        .map_err(|error| friendly_plan_error(&db, &event.id, error))
}

#[tauri::command]
pub fn host_edit_plan_assignment(
    app: AppHandle,
    round: u32,
    track_code: String,
    participant_id: String,
    expected_version: u32,
) -> Result<PlanDataDto, String> {
    let db = open_db(&app)?;
    let event = current_event(&db)?.ok_or_else(|| "there is no event".to_string())?;
    edit_plan_assignment(
        &db,
        &event.id,
        round,
        &track_code,
        &participant_id,
        expected_version,
        now_secs(),
    )
    .map_err(|error| friendly_plan_error(&db, &event.id, error))?;
    plan_data(&db)
}

fn friendly_plan_error(db: &Db, event_id: &str, error: EngineError) -> String {
    let name = |user_id: &str| {
        ParticipantRepo::get(db.conn(), event_id, user_id)
            .ok()
            .flatten()
            .map(|participant| participant.display_name)
            .unwrap_or_else(|| user_id.to_string())
    };
    match error {
        EngineError::PlanOwnTrack { participant, track } => {
            format!(
                "{} seeded track {track}. Choose another person.",
                name(&participant)
            )
        }
        EngineError::PlanRepeat { participant, track } => format!(
            "{} already worked on track {track} in another round. Choose another person.",
            name(&participant)
        ),
        EngineError::PlanParticipantNotInRound => {
            "That person is not active and ready in this round.".to_string()
        }
        EngineError::PlanChanged => {
            "The plan changed. Close this dialog and try again.".to_string()
        }
        EngineError::PlanInvalid(_) => {
            "The plan is incomplete. Replace the draft before you edit it.".to_string()
        }
        EngineError::PlanNoChange => "That person already holds this track.".to_string(),
        EngineError::PlanCellNotFound { .. } => {
            "That assignment is no longer in the plan. Reload the planner.".to_string()
        }
        other => other.to_string(),
    }
}

#[tauri::command]
pub fn host_start_event(app: AppHandle) -> Result<PlanDataDto, String> {
    let db = open_db(&app)?;
    let settings = host_settings(app.clone()).unwrap_or_default();
    let channel = settings.announcement_channel_id.trim().to_string();
    let result = start_host_event_with_channel(
        &db,
        if channel.is_empty() {
            None
        } else {
            Some(channel.as_str())
        },
    );
    if result.is_ok() {
        #[cfg(feature = "live")]
        app.state::<crate::live_runtime::LiveRuntime>().wake();
    }
    result
}

fn start_host_event_with_channel(db: &Db, channel_id: Option<&str>) -> Result<PlanDataDto, String> {
    let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
    let now = now_secs();
    let settings: swap_engine::EventSettings =
        serde_json::from_str(&event.settings_json).unwrap_or_default();
    if settings.mode == swap_engine::Mode::Async {
        start_async_event(db, &event.id, now).map_err(|error| error.to_string())?;
        AuditRepo::insert(
            db.conn(),
            &event.id,
            now,
            "Host",
            "Event started",
            None,
            &serde_json::json!({ "mode": "async" }).to_string(),
        )
        .map_err(|error| error.to_string())?;
        return plan_data(db);
    }
    let outcome = start_event_with_channel(db, &event.id, channel_id, now)
        .map_err(|error| error.to_string())?;
    AuditRepo::insert(
        db.conn(),
        &event.id,
        now,
        "Host",
        "Event started",
        Some(&format!("Plan v{}", outcome.plan_version)),
        &serde_json::json!({
            "commitment": outcome.commitment,
            "announced": !outcome.effects.is_empty()
        })
        .to_string(),
    )
    .map_err(|error| error.to_string())?;
    plan_data(db)
}

#[tauri::command]
pub async fn host_mission_control(
    app: AppHandle,
    runtime: State<'_, crate::setup_host::BotRuntime>,
) -> Result<MissionControlDto, String> {
    let db = open_db(&app)?;
    let mut settings = host_settings(app.clone()).unwrap_or_default();
    if let Ok(status) = crate::setup_host::status(&runtime, &settings.token_storage).await
        && !status.keychain_error
    {
        settings.bot_configured = status.configured;
        settings.token_storage = status.token_storage;
    }
    let gateway = {
        #[cfg(feature = "live")]
        {
            app.state::<crate::live_runtime::LiveRuntime>().health()
        }
        #[cfg(not(feature = "live"))]
        {
            None
        }
    };
    mission_control_with_health(&db, &settings, gateway)
}

#[cfg(test)]
fn mission_control(db: &Db, settings: &SettingsDto) -> Result<MissionControlDto, String> {
    mission_control_with_health(db, settings, None)
}

fn mission_control_with_health(
    db: &Db,
    settings: &SettingsDto,
    gateway: Option<swap_discord::GatewayHealth>,
) -> Result<MissionControlDto, String> {
    let Some(event) = current_event(db)? else {
        return Ok(MissionControlDto {
            first_run: true,
            event: None,
            bot: bot_health(settings, false, gateway.as_ref()),
            next_action: None,
            decisions: Vec::new(),
            activity: Vec::new(),
            timeline: Vec::new(),
            facts: Vec::new(),
            join: JoinInfoDto {
                formats: swap_files::DEFAULT_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_string())
                    .collect(),
                max_file_mib: swap_files::DEFAULT_MAX_SIZE / (1024 * 1024),
            },
            late_seeds: Vec::new(),
        });
    };
    let event_summary = summary(db, &event)?;
    let plan = PlanRepo::latest(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let settings_json: EventSettings =
        serde_json::from_str(&event.settings_json).unwrap_or_default();
    let audit =
        AuditRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let activity = audit
        .iter()
        .take(6)
        .map(|entry| ActivityDto {
            id: format!("activity-{}", entry.id),
            at: format_local_time(entry.at, &settings.timezone),
            actor: entry.actor.clone(),
            action: entry.action.clone(),
            subject: entry.subject.clone().unwrap_or_default(),
        })
        .collect();
    let seeded = event_summary.tracks > 0;
    let roster_locked = ParticipantRepo::list_for_event(db.conn(), &event.id)
        .map_err(|error| error.to_string())?
        .iter()
        .any(|participant| participant.membership == Membership::Active);
    let planned = plan.is_some();
    let mut timeline = vec![
        TimelineStepDto {
            id: "seeds".to_string(),
            label: "Seeds".to_string(),
            caption: format!(
                "{} {}",
                event_summary.tracks,
                if event_summary.tracks == 1 {
                    "track"
                } else {
                    "tracks"
                }
            ),
            state: step_state(seeded, true),
        },
        TimelineStepDto {
            id: "roster".to_string(),
            label: "Roster".to_string(),
            caption: if roster_locked {
                format!("{} active", event_summary.participants)
            } else {
                "Not locked".to_string()
            },
            state: step_state(roster_locked, seeded),
        },
        TimelineStepDto {
            id: "schedule".to_string(),
            label: "Schedule".to_string(),
            caption: plan
                .as_ref()
                .map(|plan| format!("Version {}", plan.version))
                .unwrap_or_else(|| "Not committed".to_string()),
            state: step_state(planned, roster_locked),
        },
        TimelineStepDto {
            id: "rounds".to_string(),
            label: "Rounds".to_string(),
            caption: format!("{} planned", event_summary.rounds),
            state: if matches!(event.state, EventState::Active | EventState::Paused) {
                "current"
            } else if matches!(event.state, EventState::Reveal | EventState::Archived) {
                "done"
            } else {
                "next"
            }
            .to_string(),
        },
        TimelineStepDto {
            id: "reveal".to_string(),
            label: "Reveal".to_string(),
            caption: "After the final round".to_string(),
            state: if matches!(event.state, EventState::Reveal | EventState::Archived) {
                "current"
            } else {
                "next"
            }
            .to_string(),
        },
    ];
    if !timeline.iter().any(|step| step.state == "current")
        && let Some(first_next) = timeline.iter_mut().find(|step| step.state == "next")
    {
        first_next.state = "current".to_string();
    }
    let active_assignments =
        AssignmentRepo::list_active(db.conn(), &event.id).map_err(|error| error.to_string())?;
    let next_action = if matches!(event.state, EventState::Reveal | EventState::Archived) {
        None
    } else if matches!(event.state, EventState::Draft) {
        Some(NextActionDto {
            eyebrow: "Next action".to_string(),
            title: "Open Round 1".to_string(),
            detail: "Upload the host pack and set the seed deadline. Round 1 is the seed round."
                .to_string(),
            action_label: "Set up Round 1".to_string(),
            secondary_label: None,
            tone: "attention".to_string(),
        })
    } else if matches!(event.state, EventState::Active) {
        if !active_assignments.is_empty() {
            None
        } else if event_summary.round < event_summary.rounds {
            let next_round = if event_summary.round == 0 {
                2
            } else {
                event_summary.round + 1
            };
            Some(NextActionDto {
                eyebrow: "Next action".to_string(),
                title: format!("Open round {next_round}"),
                detail: "Every track in the current round is resolved. Open the next round when you are ready.".to_string(),
                action_label: format!("Open Round {next_round}"),
                secondary_label: None,
                tone: "attention".to_string(),
            })
        } else if event_summary.rounds > 0 && settings.reveal_channel_id.trim().is_empty() {
            Some(NextActionDto {
                eyebrow: "Next action".to_string(),
                title: "Map the reveal channel".to_string(),
                detail: "Every planned round is complete. Map the reveal channel before you reveal the event.".to_string(),
                action_label: "Open settings".to_string(),
                secondary_label: None,
                tone: "attention".to_string(),
            })
        } else if event_summary.rounds > 0 {
            Some(NextActionDto {
                eyebrow: "Next action".to_string(),
                title: "Reveal the event".to_string(),
                detail: "Every planned round is complete. Reveal publishes the credits to the reveal channel.".to_string(),
                action_label: "Reveal event".to_string(),
                secondary_label: None,
                tone: "attention".to_string(),
            })
        } else {
            None
        }
    } else if matches!(event.state, EventState::Paused) {
        None
    } else if !roster_locked && event_summary.tracks > 0 {
        let missing = event_summary
            .participants
            .saturating_sub(event_summary.tracks);
        Some(NextActionDto {
            eyebrow: "Next action".to_string(),
            title: "Lock the roster".to_string(),
            detail: if missing == 0 {
                "All seeds are in. Lock the roster to close seed intake and build the schedule."
                    .to_string()
            } else {
                format!(
                    "{} seeds are in and {} people still need a seed. Locking closes seed intake; those people become fill-ins.",
                    event_summary.tracks, missing
                )
            },
            action_label: "Lock roster".to_string(),
            secondary_label: None,
            tone: "attention".to_string(),
        })
    } else if !roster_locked {
        Some(NextActionDto {
            eyebrow: "Next action".to_string(),
            title: "Invite the first people".to_string(),
            detail: "The event exists. Share the join instructions so people can send one seed track to the bot.".to_string(),
            action_label: "Set up Round 1".to_string(),
            secondary_label: None,
            tone: "info".to_string(),
        })
    } else if !planned {
        Some(NextActionDto {
            eyebrow: "Next action".to_string(),
            title: "Build the schedule".to_string(),
            detail: "The roster is in. Generate the draft schedule and check the matrix before anyone edits.".to_string(),
            action_label: "Open schedule planner".to_string(),
            secondary_label: None,
            tone: "info".to_string(),
        })
    } else {
        Some(NextActionDto {
            eyebrow: "Next action".to_string(),
            title: "Lock the schedule".to_string(),
            detail: "Review the assignment matrix, then lock the plan. Locking publishes a commitment without any file hash.".to_string(),
            action_label: "Open schedule planner".to_string(),
            secondary_label: None,
            tone: "attention".to_string(),
        })
    };
    let mut decisions = Vec::new();
    if !roster_locked && seeded && event_summary.tracks < event_summary.participants {
        decisions.push(DecisionDto {
            id: "missing-seeds".to_string(),
            kind: "Seeds".to_string(),
            title: format!(
                "{} people still need a seed",
                event_summary.participants - event_summary.tracks
            ),
            detail: "A participant without a seed becomes a fill-in when signups close. The host can wait or invite another seed.".to_string(),
            due: "Before roster lock".to_string(),
            action_label: "Review".to_string(),
            assignment_id: None,
            decision_id: None,
            requested_hours: None,
            deadline: None,
            new_deadline: None,
            extensions_used: None,
        });
    }
    if let Ok(open) = swap_db::HostDecisionRepo::list_open(db.conn(), &event.id) {
        let assignments = AssignmentRepo::list_for_event(db.conn(), &event.id).unwrap_or_default();
        let participants =
            ParticipantRepo::list_for_event(db.conn(), &event.id).unwrap_or_default();
        for decision in open {
            let assignment = decision
                .assignment_id
                .as_deref()
                .and_then(|id| assignments.iter().find(|assignment| assignment.id == id));
            let track_code = assignment
                .map(|assignment| assignment.track_code.clone())
                .unwrap_or_else(|| "the track".to_string());
            let participant = assignment.and_then(|assignment| {
                participants
                    .iter()
                    .find(|participant| participant.user_id == assignment.participant_id)
            });
            let (kind, title, detail) = if decision.kind == "extension" {
                (
                    "Extension",
                    format!(
                        "{} asked for {} more hours",
                        participant
                            .map(|participant| participant.display_name.as_str())
                            .unwrap_or("A participant"),
                        decision.requested_hours.unwrap_or(6)
                    ),
                    format!(
                        "Track {track_code} keeps its holder until the host decides. The deadline does not move yet."
                    ),
                )
            } else {
                (
                    "Deadline",
                    format!("Track {track_code} passed its deadline"),
                    "The track stays with its holder. Extend, mark missed, or keep waiting."
                        .to_string(),
                )
            };
            let due_at = assignment.and_then(|assignment| assignment.due_at);
            let hours = decision.requested_hours.unwrap_or(6);
            let deadline = due_at.map(|at| format_local_time(at, &settings.timezone));
            let new_deadline = due_at.map(|at| {
                format_local_time(at.max(now_secs()) + hours * 3_600, &settings.timezone)
            });
            let extensions_used = assignment
                .and_then(|assignment| DeadlineRepo::get(db.conn(), &assignment.id).ok().flatten())
                .map(|deadline| deadline.extension_count);
            decisions.push(DecisionDto {
                id: format!("host-{}", decision.id),
                kind: kind.to_string(),
                title,
                detail,
                due: "Awaiting host".to_string(),
                action_label: "Review".to_string(),
                assignment_id: decision.assignment_id.clone(),
                decision_id: Some(decision.id),
                requested_hours: decision.requested_hours,
                deadline,
                new_deadline,
                extensions_used,
            });
        }
    }
    let facts = vec![
        EventFactDto {
            label: "Mode".to_string(),
            value: match settings_json.mode {
                Mode::Classic => format!("Classic, {} rounds", event_summary.rounds),
                Mode::Async => "Async queue".to_string(),
            },
        },
        EventFactDto {
            label: "People".to_string(),
            value: if roster_locked {
                format!(
                    "{} active, including {} fill-ins",
                    event_summary.participants, event_summary.fill_ins
                )
            } else {
                format!("{} signed up", event_summary.participants)
            },
        },
        EventFactDto {
            label: "Tracks".to_string(),
            value: format!("{} anonymous tracks", event_summary.tracks),
        },
        EventFactDto {
            label: "Seed round".to_string(),
            value: format!("{} days (Round 1)", settings_json.seed_window_secs / 86_400),
        },
        EventFactDto {
            label: "Exchange round".to_string(),
            value: format!(
                "{} hours (final {} hours)",
                settings_json.round_window_secs / 3600,
                settings_json.final_round_window_secs / 3600
            ),
        },
        EventFactDto {
            label: "Timezone".to_string(),
            value: settings.timezone.clone(),
        },
    ];
    let late_seeds = pending_late_seeds(db, &event.id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|seed| LateSeedDto {
            id: seed.id,
            display_name: seed.display_name,
            track_code: seed.track_code,
            created_at: seed.created_at,
        })
        .collect();
    Ok(MissionControlDto {
        first_run: false,
        event: Some(event_summary),
        bot: bot_health(settings, true, gateway.as_ref()),
        next_action,
        decisions,
        activity,
        timeline,
        facts,
        join: JoinInfoDto {
            formats: settings_json.allowed_extensions.clone(),
            max_file_mib: settings_json.max_file_bytes / (1024 * 1024),
        },
        late_seeds,
    })
}

fn step_state(done: bool, prerequisite: bool) -> String {
    if done {
        "done".to_string()
    } else if prerequisite {
        "current".to_string()
    } else {
        "next".to_string()
    }
}

fn bot_health(
    settings: &SettingsDto,
    has_event: bool,
    gateway: Option<&swap_discord::GatewayHealth>,
) -> BotHealthDto {
    if !settings.bot_configured {
        BotHealthDto {
            status: "Not configured".to_string(),
            label: "No bot connected".to_string(),
            detail: "Connect a bot token to send offers and collect files.".to_string(),
            last_seen: None,
        }
    } else if let Some(health) = gateway {
        let (status, detail) = match health.state {
            swap_discord::GatewayState::Online => (
                "Online",
                "Connected to Discord. Offers and receipts can leave the outbox.",
            ),
            swap_discord::GatewayState::Starting => (
                "Connecting",
                health
                    .last_error
                    .as_deref()
                    .unwrap_or("Starting the gateway."),
            ),
            swap_discord::GatewayState::Reconnecting => (
                "Reconnecting",
                health
                    .last_error
                    .as_deref()
                    .unwrap_or("Trying to reach Discord."),
            ),
            swap_discord::GatewayState::Failed => (
                "Offline",
                health
                    .last_error
                    .as_deref()
                    .unwrap_or("The gateway failed."),
            ),
            swap_discord::GatewayState::Stopped => ("Offline", "The gateway is stopped."),
        };
        BotHealthDto {
            status: status.to_string(),
            label: if settings.bot_identity.is_empty() {
                "Monsquaz Bot".to_string()
            } else {
                settings.bot_identity.clone()
            },
            detail: detail.to_string(),
            last_seen: health
                .connected_at
                .map(|at| format_local_time(at, &settings.timezone)),
        }
    } else if has_event {
        BotHealthDto {
            status: "Offline".to_string(),
            label: if settings.bot_identity.is_empty() {
                "Monsquaz Bot".to_string()
            } else {
                settings.bot_identity.clone()
            },
            detail: "The gateway is not running. Start the bot to send offers.".to_string(),
            last_seen: None,
        }
    } else {
        BotHealthDto {
            status: "Offline".to_string(),
            label: "Monsquaz Bot".to_string(),
            detail: "The gateway is not running. Start the bot when the event opens.".to_string(),
            last_seen: None,
        }
    }
}

#[tauri::command]
pub fn host_audit_log(app: AppHandle) -> Result<Vec<AuditEntryDto>, String> {
    let db = open_db(&app)?;
    let settings = host_settings(app).unwrap_or_default();
    audit_log(&db, &settings)
}

fn audit_log(db: &Db, settings: &SettingsDto) -> Result<Vec<AuditEntryDto>, String> {
    let Some(event) = current_event(db)? else {
        return Ok(Vec::new());
    };
    let entries =
        AuditRepo::list_for_event(db.conn(), &event.id).map_err(|error| error.to_string())?;
    Ok(entries
        .into_iter()
        .map(|entry| AuditEntryDto {
            id: format!("audit-{}", entry.id),
            at: format_local_time(entry.at, &settings.timezone),
            actor: entry.actor,
            action: entry.action,
            subject: entry.subject.unwrap_or_default(),
            reason: String::new(),
            code: format!("A-{:04}", entry.id),
        })
        .collect())
}

pub(crate) fn read_settings(app: &AppHandle) -> Result<SettingsDto, String> {
    let path = settings_path(app)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SettingsDto::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn write_settings(app: &AppHandle, settings: &SettingsDto) -> Result<(), String> {
    validate_settings(settings)?;
    let path = settings_path(app)?;
    let contents = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn host_settings(app: AppHandle) -> Result<SettingsDto, String> {
    read_settings(&app)
}

pub(crate) fn save_settings_values(
    app: AppHandle,
    mut settings: SettingsDto,
) -> Result<SettingsDto, String> {
    let current = read_settings(&app)?;
    settings.copy_bot_fields_from(&current);
    write_settings(&app, &settings)?;
    Ok(settings)
}

impl SettingsDto {
    pub(crate) fn copy_bot_fields_from(&mut self, other: &Self) {
        self.bot_configured = other.bot_configured;
        self.bot_identity = other.bot_identity.clone();
        self.token_storage = other.token_storage.clone();
        self.guild_id = other.guild_id.clone();
        self.guild_name = other.guild_name.clone();
        self.announcement_channel_id = other.announcement_channel_id.clone();
        self.announcement_channel_name = other.announcement_channel_name.clone();
        self.reveal_channel_id = other.reveal_channel_id.clone();
        self.reveal_channel_name = other.reveal_channel_name.clone();
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("host-settings.json"))
}

fn validate_settings(settings: &SettingsDto) -> Result<(), String> {
    if !(1..=3650).contains(&settings.retention_days) {
        return Err("retention_days must be between 1 and 3650".to_string());
    }
    if !matches!(settings.theme.as_str(), "dark" | "light" | "system") {
        return Err("theme must be dark, light, or system".to_string());
    }
    if settings.timezone.trim().is_empty() || settings.timezone.chars().count() > 64 {
        return Err("timezone must be between 1 and 64 characters".to_string());
    }
    if !matches!(
        settings.token_storage.as_str(),
        "keychain" | "session" | "none"
    ) {
        return Err("token_storage must be keychain, session, or none".to_string());
    }
    for (label, value) in [
        ("guild id", &settings.guild_id),
        ("guild name", &settings.guild_name),
        ("announcement channel id", &settings.announcement_channel_id),
        (
            "announcement channel name",
            &settings.announcement_channel_name,
        ),
        ("reveal channel id", &settings.reveal_channel_id),
        ("reveal channel name", &settings.reveal_channel_name),
        ("bot identity", &settings.bot_identity),
    ] {
        if value.chars().count() > 200 {
            return Err(format!("{label} must be at most 200 characters"));
        }
    }
    Ok(())
}

fn format_local_time(seconds: i64, timezone: &str) -> String {
    let zone: Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let Some(instant) = DateTime::<Utc>::from_timestamp(seconds, 0) else {
        return String::new();
    };
    instant
        .with_timezone(&zone)
        .format("%-d %b, %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use swap_db::PackRecord;

    #[test]
    fn the_default_settings_are_valid() {
        validate_settings(&SettingsDto::default()).unwrap();
    }

    #[test]
    fn bad_settings_are_refused() {
        let invalid_retention = SettingsDto {
            retention_days: 0,
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_retention).is_err());

        let invalid_theme = SettingsDto {
            theme: "neon".to_string(),
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_theme).is_err());

        let invalid_timezone = SettingsDto {
            timezone: " ".to_string(),
            ..SettingsDto::default()
        };
        assert!(validate_settings(&invalid_timezone).is_err());
    }

    #[test]
    fn ui_settings_cannot_change_bot_owned_fields() {
        let stored = SettingsDto {
            bot_configured: true,
            bot_identity: "Monsquaz Bot (900)".to_string(),
            token_storage: "keychain".to_string(),
            guild_id: "10".to_string(),
            guild_name: "Night Shift".to_string(),
            announcement_channel_id: "11".to_string(),
            reveal_channel_id: "12".to_string(),
            ..SettingsDto::default()
        };
        let mut proposed = SettingsDto {
            bot_configured: false,
            bot_identity: "forged".to_string(),
            token_storage: "none".to_string(),
            guild_id: "99".to_string(),
            ..SettingsDto::default()
        };

        proposed.copy_bot_fields_from(&stored);

        assert!(proposed.bot_configured);
        assert_eq!(proposed.bot_identity, "Monsquaz Bot (900)");
        assert_eq!(proposed.token_storage, "keychain");
        assert_eq!(proposed.guild_id, "10");
        assert_eq!(proposed.announcement_channel_id, "11");
        assert_eq!(proposed.reveal_channel_id, "12");
    }

    #[test]
    fn the_bot_health_uses_the_gateway_state() {
        let settings = SettingsDto {
            bot_configured: true,
            ..SettingsDto::default()
        };
        let online = swap_discord::GatewayHealth {
            state: swap_discord::GatewayState::Online,
            attempt: 0,
            last_error: None,
            connected_at: Some(1_700_000_000),
            last_event_at: None,
            handler_errors: 0,
        };
        assert_eq!(bot_health(&settings, true, Some(&online)).status, "Online");
        let reconnecting = swap_discord::GatewayHealth {
            state: swap_discord::GatewayState::Reconnecting,
            attempt: 3,
            last_error: Some("Discord timed out".to_string()),
            connected_at: None,
            last_event_at: None,
            handler_errors: 0,
        };
        let dto = bot_health(&settings, true, Some(&reconnecting));
        assert_eq!(dto.status, "Reconnecting");
        assert_eq!(dto.detail, "Discord timed out");
        assert_eq!(bot_health(&settings, true, None).status, "Offline");
    }

    #[test]
    fn the_settings_payload_has_no_secret_field() {
        let json = serde_json::to_string(&SettingsDto::default()).unwrap();
        assert!(json.contains("\"botConfigured\""));
        assert!(json.contains("\"retentionDays\""));
        assert!(json.contains("\"tokenStorage\""));
        assert!(!json.contains("\"token\""));
        assert!(!json.contains("secret"));
    }

    fn test_db(name: &str) -> (Db, PathBuf) {
        let directory = std::env::temp_dir().join(format!("monsquaz-{name}-{}", now_nanos()));
        fs::create_dir_all(&directory).unwrap();
        let db = Db::open(directory.join("test.db")).unwrap();
        (db, directory)
    }

    fn ensure_seed_round(db: &Db, event_id: &str) {
        let Ok(event) = EventRepo::get(db.conn(), event_id) else {
            return;
        };
        let Some(event) = event else {
            return;
        };
        if event.state != EventState::Draft {
            return;
        }
        if PackRepo::get_for_event(db.conn(), event_id)
            .unwrap()
            .is_none()
        {
            PackRepo::insert(
                db.conn(),
                &PackRecord {
                    id: "pack-test".to_string(),
                    event_id: event_id.to_string(),
                    filename: "pack.zip".to_string(),
                    sha256: swap_files::sha256_hex(b"test-pack"),
                    size: 9,
                    storage_path: "blobs/test-pack".to_string(),
                    created_at: 100,
                    locked_at: None,
                },
            )
            .unwrap();
        }
        let now = now_secs();
        open_seed_round(db, event_id, now + 7 * 24 * 60 * 60, None, now).unwrap();
    }

    fn seed_person(db: &Db, event_id: &str, id: &str, name: &str) {
        ensure_seed_round(db, event_id);
        ParticipantRepo::upsert(
            db.conn(),
            &swap_db::ParticipantRecord {
                event_id: event_id.to_string(),
                user_id: id.to_string(),
                display_name: name.to_string(),
                role: Role::Participant,
                membership: Membership::Seeded,
                readiness: swap_core::Readiness::Ready,
                timezone: "Europe/Stockholm".to_string(),
                max_handoffs: 20,
                created_at: 100,
                updated_at: 100,
            },
        )
        .unwrap();
    }

    fn seed_track(db: &Db, event_id: &str, code: &str, owner: &str) {
        ensure_seed_round(db, event_id);
        TrackRepo::insert(
            db.conn(),
            &swap_db::TrackRecord {
                event_id: event_id.to_string(),
                code: code.to_string(),
                owner_id: owner.to_string(),
                state: swap_core::TrackState::Seeded,
                created_at: 100,
            },
        )
        .unwrap();
    }

    #[test]
    fn the_setup_people_and_plan_commands_read_the_same_event() {
        let (db, directory) = test_db("event-round-trip");
        let draft = EventDraftDto {
            name: "Test Rotation".to_string(),
            rounds: Some(2),
            group_count: 2,
            group_size: 2,
            ..default_draft()
        };
        let created = create_host_event(&db, draft).unwrap();
        assert_eq!(created.name, "Test Rotation");
        assert_eq!(created.rounds, 3);

        let setup = setup_data(&db).unwrap();
        assert!(!setup.first_run);
        assert_eq!(setup.event.as_ref().unwrap().name, "Test Rotation");
        assert_eq!(setup.draft.name, "Test Rotation");

        let people = people_data(&db).unwrap();
        assert_eq!(people.event.as_ref().unwrap().name, "Test Rotation");
        assert!(people.rows.is_empty());

        let plan = plan_data(&db).unwrap();
        assert_eq!(plan.status, "No plan");
        assert_eq!(plan.rounds, 2);
        assert_eq!(plan.tracks, 0);

        seed_person(&db, &created.id, "u-alice", "Alice");
        seed_person(&db, &created.id, "u-bob", "Bob");
        seed_track(&db, &created.id, "A1", "u-alice");
        seed_track(&db, &created.id, "B1", "u-bob");

        let people = people_data(&db).unwrap();
        assert_eq!(people.rows.len(), 2);
        assert_eq!(people.rows[0].seed_track.as_deref(), Some("A1"));
        assert_eq!(people.rows[1].seed_track.as_deref(), Some("B1"));

        let paused = set_participant_paused(&db, "u-alice", true).unwrap();
        assert_eq!(paused.rows[0].status, "Paused");
        let resumed = set_participant_paused(&db, "u-alice", false).unwrap();
        assert_eq!(resumed.rows[0].status, "Ready");
        let entries = audit_log(&db, &SettingsDto::default()).unwrap();
        assert_eq!(entries[0].action, "Participant resumed");
        assert_eq!(entries[1].action, "Participant paused");

        PlanRepo::insert(
            db.conn(),
            &swap_db::PlanRecord {
                event_id: created.id.clone(),
                version: 1,
                seed: 7,
                rounds: 2,
                settings_json: "{}".to_string(),
                commitment: "commit-1".to_string(),
                committed_at: 100,
                reason: None,
            },
        )
        .unwrap();
        for (round, track, person) in [(0, "A1", "u-bob"), (0, "B1", "u-alice")] {
            PlanRepo::insert_assignment(
                db.conn(),
                &swap_db::PlanAssignmentRecord {
                    event_id: created.id.clone(),
                    plan_version: 1,
                    round,
                    track_code: track.to_string(),
                    participant_id: person.to_string(),
                },
            )
            .unwrap();
        }

        let plan = plan_data(&db).unwrap();
        assert_eq!(plan.status, "Draft");
        assert_eq!(plan.matrix.len(), 2);
        assert_eq!(plan.graph.edges.len(), 2);
        assert!(
            plan.graph
                .edges
                .iter()
                .any(|edge| edge.track_code == "A1" && edge.participant_name == "Bob")
        );
        assert!(
            plan.matrix
                .iter()
                .find(|track| track.track_code == "A1")
                .unwrap()
                .cells
                .iter()
                .any(|cell| cell.participant_name == "Bob")
        );
        assert!(plan.groups.is_empty());

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_planner_applies_lookup_groups() {
        let (db, directory) = test_db("lookup-groups");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Two Groups".to_string(),
                rounds: Some(9),
                ..default_draft()
            },
        )
        .unwrap();
        for index in 0..10 {
            let id = format!("u-{index:02}");
            let name = format!("Person {index:02}");
            let code = format!("T{index:02}");
            seed_person(&db, &created.id, &id, &name);
            seed_track(&db, &created.id, &code, &id);
        }
        lock_host_roster(&db).unwrap();
        let plan = generate_host_plan(&db).unwrap();
        assert_eq!(plan.groups.len(), 2);
        assert!(plan.groups.iter().all(|group| group.size == 5));
        assert!(plan.groups.iter().all(|group| group.tracks == 5));
        assert_eq!(plan.phases.len(), 5);
        assert!(plan.phases.last().unwrap().global_pool);
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.id == "group-fallback-4" && warning.severity == "warn")
        );

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_small_group_produces_a_soft_warning() {
        let (db, directory) = test_db("small-group");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Small Group".to_string(),
                rounds: Some(2),
                group_mode: "Manual".to_string(),
                group_count: 1,
                group_size: 3,
                ..default_draft()
            },
        )
        .unwrap();
        for index in 0..3 {
            let id = format!("u-{index:02}");
            let name = format!("Person {index:02}");
            let code = format!("T{index:02}");
            seed_person(&db, &created.id, &id, &name);
            seed_track(&db, &created.id, &code, &id);
        }
        lock_host_roster(&db).unwrap();
        let plan = generate_host_plan(&db).unwrap();
        let warning = plan
            .warnings
            .iter()
            .find(|warning| warning.id == "group-size-small")
            .expect("the small group warning is present");
        assert_eq!(warning.severity, "info");
        assert!(warning.detail.contains("3"));

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn group_repair_merges_the_underfilled_group_and_records_it() {
        let (db, directory) = test_db("group-repair");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Repair Groups".to_string(),
                rounds: Some(4),
                group_mode: "Manual".to_string(),
                group_count: 2,
                group_size: 4,
                ..default_draft()
            },
        )
        .unwrap();
        for index in 0..10 {
            let id = format!("u-{index:02}");
            let name = format!("Person {index:02}");
            let code = format!("T{index:02}");
            seed_person(&db, &created.id, &id, &name);
            seed_track(&db, &created.id, &code, &id);
        }
        lock_host_roster(&db).unwrap();
        let before = generate_host_plan(&db).unwrap();
        assert_eq!(before.groups.len(), 3);

        let repaired = repair_host_groups(&db).unwrap();
        assert_eq!(repaired.version, 2);
        assert_eq!(repaired.groups.len(), 2);
        assert!(repaired.groups.iter().all(|group| group.size == 5));

        let entries = AuditRepo::list_for_event(db.conn(), &created.id).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.action == "Groups repaired")
            .expect("the repair is in the audit log");
        let detail: serde_json::Value = serde_json::from_str(&entry.detail_json).unwrap();
        assert_eq!(detail["from"], serde_json::json!([4, 4, 2]));
        assert_eq!(detail["to"], serde_json::json!([5, 5]));

        assert_eq!(
            repair_host_groups(&db).unwrap_err(),
            "the groups do not need repair"
        );

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn async_start_and_dispatch_write_offers() {
        let (db, directory) = test_db("async-dispatch");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Async Wave".to_string(),
                mode: "Async".to_string(),
                rounds: Some(3),
                ..default_draft()
            },
        )
        .unwrap();
        for index in 0..4 {
            let id = format!("u-{index:02}");
            let name = format!("Person {index:02}");
            let code = format!("T{index:02}");
            seed_person(&db, &created.id, &id, &name);
            seed_track(&db, &created.id, &code, &id);
        }
        lock_host_roster(&db).unwrap();
        start_host_event_with_channel(&db, None).unwrap();
        let wave = dispatch_host_async_wave(&db).unwrap();
        assert_eq!(wave.offers, 4);
        let entries = AuditRepo::list_for_event(db.conn(), &created.id).unwrap();
        assert!(
            entries
                .iter()
                .any(|entry| entry.action == "Async wave dispatched")
        );
        assert!(entries.iter().any(|entry| entry.action == "Event started"
            && entry.detail_json.contains("async")));

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_bad_draft_does_not_create_an_event() {
        let (db, directory) = test_db("bad-draft");
        let draft = EventDraftDto {
            name: "   ".to_string(),
            ..default_draft()
        };
        assert!(create_host_event(&db, draft).is_err());
        assert!(setup_data(&db).unwrap().first_run);
        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_first_run_payload_matches_the_ui_contract() {
        let (db, directory) = test_db("first-run");
        let mission = mission_control(&db, &SettingsDto::default()).unwrap();
        let json = serde_json::to_value(mission).unwrap();
        assert_eq!(json["firstRun"], true);
        assert_eq!(json["event"], serde_json::Value::Null);
        assert_eq!(json["bot"]["status"], "Not configured");
        assert!(json["decisions"].as_array().unwrap().is_empty());
        assert!(json["timeline"].as_array().unwrap().is_empty());
        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mission_control_and_audit_log_follow_the_created_event() {
        let (db, directory) = test_db("mission");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Live Mission".to_string(),
                rounds: Some(2),
                ..default_draft()
            },
        )
        .unwrap();

        let mission = mission_control(&db, &SettingsDto::default()).unwrap();
        assert!(!mission.first_run);
        let summary = mission.event.as_ref().unwrap();
        assert_eq!(summary.id, created.id);
        assert_eq!(summary.name, "Live Mission");
        assert_eq!(mission.timeline.len(), 5);
        assert_eq!(mission.timeline[0].state, "current");
        assert_eq!(mission.facts.len(), 6);
        assert_eq!(
            mission.next_action.as_ref().unwrap().action_label,
            "Set up Round 1"
        );

        let entries = audit_log(&db, &SettingsDto::default()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor, "Host");
        assert_eq!(entries[0].action, "Event created");
        assert_eq!(entries[0].subject, "Live Mission");

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_lock_action_counts_signups_without_seeds() {
        let (db, directory) = test_db("lock-counts");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Locked Soon".to_string(),
                rounds: Some(2),
                ..default_draft()
            },
        )
        .unwrap();
        for index in 0..2 {
            let id = format!("u-seed-{index}");
            seed_person(&db, &created.id, &id, &format!("Seed {index}"));
            seed_track(&db, &created.id, &format!("T{index}"), &id);
        }
        for index in 0..2 {
            ParticipantRepo::upsert(
                db.conn(),
                &swap_db::ParticipantRecord {
                    event_id: created.id.clone(),
                    user_id: format!("u-signup-{index}"),
                    display_name: format!("Signup {index}"),
                    role: Role::Participant,
                    membership: Membership::SignedUp,
                    readiness: swap_core::Readiness::Ready,
                    timezone: "UTC".to_string(),
                    max_handoffs: 20,
                    created_at: 100,
                    updated_at: 100,
                },
            )
            .unwrap();
        }
        let mission = mission_control(&db, &SettingsDto::default()).unwrap();
        let event = mission.event.unwrap();
        assert_eq!(event.participants, 4);
        assert_eq!(event.seeded, 2);
        let action = mission.next_action.unwrap();
        assert_eq!(action.action_label, "Lock roster");
        assert!(action.detail.contains("2 people still need a seed"));
        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_planner_generates_a_real_schedule_from_seeded_tracks() {
        let (db, directory) = test_db("generate-plan");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Three Seeds".to_string(),
                rounds: Some(2),
                ..default_draft()
            },
        )
        .unwrap();
        for (id, name, code) in [
            ("u-alice", "Alice", "A1"),
            ("u-bob", "Bob", "B1"),
            ("u-carol", "Carol", "C1"),
        ] {
            seed_person(&db, &created.id, id, name);
            seed_track(&db, &created.id, code, id);
        }

        lock_host_roster(&db).unwrap();
        let plan = generate_host_plan(&db).unwrap();
        assert_eq!(plan.status, "Draft");
        assert_eq!(plan.matrix.len(), 3);
        assert_eq!(plan.rounds, 2);
        assert!(plan.matrix.iter().all(|track| track.cells.len() == 2));
        assert!(
            plan.matrix
                .iter()
                .flat_map(|track| track.cells.iter())
                .all(|cell| !cell.participant_id.is_empty())
        );
        for round in 0..plan.rounds {
            let mut people: Vec<&str> = plan
                .matrix
                .iter()
                .flat_map(|track| track.cells.iter())
                .filter(|cell| cell.round == round)
                .map(|cell| cell.participant_id.as_str())
                .collect();
            people.sort_unstable();
            people.dedup();
            assert_eq!(people.len(), 3, "round {round} must use each person once");
        }
        for track in &plan.matrix {
            for cell in &track.cells {
                let owner = match track.track_code.as_str() {
                    "A1" => "u-alice",
                    "B1" => "u-bob",
                    _ => "u-carol",
                };
                assert_ne!(cell.participant_id, owner);
            }
        }

        let entries = audit_log(&db, &SettingsDto::default()).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].action, "Schedule generated");
        assert_eq!(entries[0].subject, "Plan v1");
        assert_eq!(entries[1].action, "Seed round closed and roster locked");
        assert_eq!(entries[2].action, "Seed round opened");

        let again = generate_host_plan(&db).unwrap();
        assert_eq!(again.status, "Draft");
        let entries = audit_log(&db, &SettingsDto::default()).unwrap();
        assert_eq!(entries[0].subject, "Plan v2");

        let started = start_host_event_with_channel(&db, None).unwrap();
        assert!(started.committed);
        assert_eq!(started.status, "Committed");
        let entries = audit_log(&db, &SettingsDto::default()).unwrap();
        assert_eq!(entries[0].action, "Event started");

        let mission = mission_control(&db, &SettingsDto::default()).unwrap();
        assert_eq!(
            mission.next_action.as_ref().unwrap().action_label,
            "Open Round 2"
        );
        assert_eq!(mission.event.as_ref().unwrap().stage, "Active");
        assert_eq!(mission.event.as_ref().unwrap().round, 0);

        let event = current_event(&db).unwrap().unwrap();
        open_round(&db, &event.id, 0, now_secs()).unwrap();
        let mission = mission_control(&db, &SettingsDto::default()).unwrap();
        assert!(mission.next_action.is_none());
        assert_eq!(mission.event.as_ref().unwrap().round, 2);

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn starting_an_event_publishes_the_commitment() {
        use swap_db::DeliveryRepo;

        let (db, directory) = test_db("start-commitment");
        let created = create_host_event(
            &db,
            EventDraftDto {
                name: "Locked Swap".to_string(),
                rounds: Some(2),
                ..default_draft()
            },
        )
        .unwrap();
        for (id, name, code) in [
            ("u-alice", "Alice", "A1"),
            ("u-bob", "Bob", "B1"),
            ("u-carol", "Carol", "C1"),
        ] {
            seed_person(&db, &created.id, id, name);
            seed_track(&db, &created.id, code, id);
        }
        lock_host_roster(&db).unwrap();
        let plan = generate_host_plan(&db).unwrap();
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);

        let started = start_host_event_with_channel(&db, Some("announce")).unwrap();
        assert!(started.committed);
        assert_eq!(started.commitment, plan.commitment);
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 1);

        let pending = DeliveryRepo::pending(db.conn(), 10).unwrap();
        let channel = pending
            .iter()
            .find(|delivery| delivery.kind == "channel")
            .unwrap();
        assert!(
            channel
                .payload
                .windows("announce".len())
                .any(|window| window == b"announce")
        );
        let short = &plan.commitment[..16];
        assert!(
            channel
                .payload
                .windows(short.len())
                .any(|window| window == short.as_bytes())
        );

        let entries = AuditRepo::list_for_event(db.conn(), &created.id).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.action == "Event started")
            .unwrap();
        assert_eq!(entry.subject.as_deref(), Some("Plan v1"));
        let detail: serde_json::Value = serde_json::from_str(&entry.detail_json).unwrap();
        assert_eq!(detail["commitment"], plan.commitment);
        assert_eq!(detail["announced"], true);

        drop(db);
        fs::remove_dir_all(directory).unwrap();
    }
}
