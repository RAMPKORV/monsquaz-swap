//! A JSON command surface for the browser E2E bridge.
//!
//! This module is test-only and compile-gated. It reuses the host helpers so
//! the browser journey runs the same code as the Tauri commands.

#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use swap_db::{AssignmentRepo, AuditRepo, Db};
use swap_engine::{
    EngineError, accept_late_seed, add_availability, add_track_comment, assign_fill_in,
    edit_plan_assignment, extend_seed_deadline, fill_in_candidates, move_round_deadline,
    open_round, open_seed_round, pending_late_seeds, plan_edit_candidates, redact_track_comment,
    reject_late_seed, remove_availability, reopen_seed_round, reveal_with_channel, reveal_zip,
    set_track_title, upload_pack, verify_commitment,
};

use super::*;

/// The fixed values that the local Discord simulator provides.
#[derive(Debug, Clone)]
pub struct E2eDefaults {
    pub guild_id: String,
    pub guild_name: String,
    pub announcement_id: String,
    pub announcement_name: String,
    pub reveal_id: String,
    pub reveal_name: String,
    pub bot_identity: String,
}

/// Host settings and the JSON dispatcher for one bridge process.
pub struct E2eHost {
    data_dir: PathBuf,
    settings: SettingsDto,
    defaults: E2eDefaults,
}

/// The server and channel names that the browser setup sends.
#[derive(Debug, Clone)]
pub struct BotMapping<'a> {
    pub guild_id: &'a str,
    pub guild_name: &'a str,
    pub announcement_id: &'a str,
    pub announcement_name: &'a str,
    pub reveal_id: &'a str,
    pub reveal_name: &'a str,
}

impl E2eHost {
    pub fn new(data_dir: PathBuf, defaults: E2eDefaults) -> Self {
        let settings = fs::read_to_string(data_dir.join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| SettingsDto {
                token_storage: "session".to_string(),
                ..SettingsDto::default()
            });
        Self {
            data_dir,
            settings,
            defaults,
        }
    }

    pub fn defaults(&self) -> &E2eDefaults {
        &self.defaults
    }

    pub fn settings(&self) -> &SettingsDto {
        &self.settings
    }

    pub fn settings_value(&self) -> Result<Value, String> {
        serde_json::to_value(&self.settings).map_err(|error| error.to_string())
    }

    pub fn replace_settings(&mut self, settings: SettingsDto) -> Result<(), String> {
        self.settings = settings;
        self.persist()
    }

    pub fn configure_bot(
        &mut self,
        mapping: BotMapping<'_>,
        session_only: bool,
    ) -> Result<(), String> {
        self.settings.bot_configured = true;
        self.settings.bot_identity = self.defaults.bot_identity.clone();
        self.settings.token_storage = if session_only {
            "session".to_string()
        } else {
            "keychain".to_string()
        };
        self.settings.guild_id = mapping.guild_id.to_string();
        self.settings.guild_name = mapping.guild_name.to_string();
        self.settings.announcement_channel_id = mapping.announcement_id.to_string();
        self.settings.announcement_channel_name = mapping.announcement_name.to_string();
        self.settings.reveal_channel_id = mapping.reveal_id.to_string();
        self.settings.reveal_channel_name = mapping.reveal_name.to_string();
        self.persist()
    }

    pub fn clear_bot(&mut self) -> Result<(), String> {
        self.settings.bot_configured = false;
        self.settings.bot_identity.clear();
        self.settings.token_storage = "none".to_string();
        self.settings.guild_id.clear();
        self.settings.guild_name.clear();
        self.settings.announcement_channel_id.clear();
        self.settings.announcement_channel_name.clear();
        self.settings.reveal_channel_id.clear();
        self.settings.reveal_channel_name.clear();
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
        let text =
            serde_json::to_string_pretty(&self.settings).map_err(|error| error.to_string())?;
        fs::write(self.data_dir.join("settings.json"), text).map_err(|error| error.to_string())
    }

    /// The browser calls one command. Unknown commands fail with a clear text.
    pub fn dispatch(&mut self, db: &Db, command: &str, args: &Value) -> Result<Value, String> {
        match command {
            "host_settings" => self.settings_value(),
            "save_host_settings" => {
                let settings: SettingsDto = serde_json::from_value(
                    args.get("settings")
                        .cloned()
                        .ok_or_else(|| "the settings are missing".to_string())?,
                )
                .map_err(|error| error.to_string())?;
                self.settings = settings;
                self.persist()?;
                self.settings_value()
            }
            "host_bot_status" => Ok(json!({
                "configured": self.settings.bot_configured,
                "tokenStorage": self.settings.token_storage,
                "keychainError": false,
            })),
            "host_bot_save" => {
                let input = args
                    .get("input")
                    .cloned()
                    .ok_or_else(|| "the bot setup input is missing".to_string())?;
                let guild_id = string(&input, "guildId")?;
                let guild_name = self.defaults.guild_name.clone();
                let announcement = string(&input, "announcementChannelId")?;
                let reveal = string(&input, "revealChannelId")?;
                let session_only = bool_arg(&input, "allowSessionOnly").unwrap_or(true);
                self.configure_bot(
                    BotMapping {
                        guild_id: &guild_id,
                        guild_name: &guild_name,
                        announcement_id: &announcement,
                        announcement_name: "announcements",
                        reveal_id: &reveal,
                        reveal_name: "reveal",
                    },
                    session_only,
                )?;
                self.settings_value()
            }
            "host_bot_clear" => {
                self.clear_bot()?;
                self.settings_value()
            }
            "host_event_setup" => to_value(setup_data(db)?),
            "host_mission_control" => {
                to_value(mission_control_with_health(db, &self.settings, None)?)
            }
            "host_create_event" => {
                let draft: EventDraftDto = serde_json::from_value(
                    args.get("draft")
                        .cloned()
                        .ok_or_else(|| "the event draft is missing".to_string())?,
                )
                .map_err(|error| error.to_string())?;
                to_value(create_host_event(db, draft)?)
            }
            "host_people" => to_value(people_data(db)?),
            "host_set_participant_paused" => {
                let user_id = string(args, "userId")?;
                let paused = bool_arg(args, "paused")?;
                to_value(set_participant_paused(db, &user_id, paused)?)
            }
            "host_mark_participant_left" => {
                let user_id = string(args, "userId")?;
                to_value(mark_participant_left(db, &user_id)?)
            }
            "host_restore_participant" => {
                let user_id = string(args, "userId")?;
                to_value(restore_participant(db, &user_id)?)
            }
            "host_add_availability" => {
                let participant_id = string(args, "participantId")?;
                let starts_at = i64_arg(args, "startsAt")?;
                let ends_at = i64_arg(args, "endsAt")?;
                let note = optional_string(args, "note");
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                add_availability(
                    db,
                    &event.id,
                    &participant_id,
                    starts_at,
                    ends_at,
                    note.as_deref(),
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                to_value(people_data(db)?)
            }
            "host_remove_availability" => {
                let id = string(args, "id")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                remove_availability(db, &event.id, &id, now_secs())
                    .map_err(|error| error.to_string())?;
                to_value(people_data(db)?)
            }
            "host_plan" => to_value(plan_data(db)?),
            "host_generate_plan" => to_value(generate_host_plan(db)?),
            "host_repair_groups" => to_value(repair_host_groups(db)?),
            "host_start_event" => {
                let channel = self.settings.announcement_channel_id.trim().to_string();
                to_value(start_host_event_with_channel(
                    db,
                    if channel.is_empty() {
                        None
                    } else {
                        Some(channel.as_str())
                    },
                )?)
            }
            "host_lock_roster" => to_value(lock_host_roster(db)?),
            "host_open_round" => {
                let round = u32_arg(args, "round")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let now = now_secs();
                let outcome =
                    open_round(db, &event.id, round, now).map_err(|error| error.to_string())?;
                Ok(json!({ "round": round, "offers": outcome.offers }))
            }
            "host_reveal" => self.reveal(db),
            "host_reveal_package" => to_value(reveal_package_dto(db)?),
            "host_set_track_title" => {
                let track_code = string(args, "trackCode")?;
                let title = string(args, "title")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                set_track_title(db, &event.id, &track_code, &title, "host", now_secs())
                    .map_err(|error| error.to_string())?;
                to_value(reveal_package_dto(db)?)
            }
            "host_add_track_comment" => {
                let track_code = string(args, "trackCode")?;
                let participant_id = string(args, "participantId")?;
                let kind = string(args, "kind")?;
                let body = string(args, "body")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                add_track_comment(
                    db,
                    &event.id,
                    &track_code,
                    &participant_id,
                    &kind,
                    &body,
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                to_value(reveal_package_dto(db)?)
            }
            "host_redact_track_comment" => {
                let comment_id = string(args, "commentId")?;
                redact_track_comment(db, &comment_id, now_secs())
                    .map_err(|error| error.to_string())?;
                to_value(reveal_package_dto(db)?)
            }
            "host_decide_extension" => {
                let decision_id = string(args, "decisionId")?;
                let approve = bool_arg(args, "approve")?;
                let reason = string(args, "reason")?;
                let outcome =
                    swap_engine::decide_extension(db, &decision_id, approve, &reason, now_secs())
                        .map_err(|error| error.to_string())?;
                Ok(Value::String(
                    match outcome.status {
                        swap_engine::ExtensionStatus::Approved => "approved",
                        swap_engine::ExtensionStatus::Refused => "refused",
                        swap_engine::ExtensionStatus::PendingHost => "pending",
                    }
                    .to_string(),
                ))
            }
            "host_decide_deadline" => {
                let assignment_id = string(args, "assignmentId")?;
                let action = string(args, "action")?;
                let hours = i64_arg(args, "hours")?;
                let reason = string(args, "reason")?;
                swap_engine::decide_deadline(
                    db,
                    &assignment_id,
                    &action,
                    hours,
                    &reason,
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                Ok(Value::Null)
            }
            "host_plan_edit_candidates" => {
                let round = u32_arg(args, "round")?;
                let track_code = string(args, "trackCode")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let candidates = plan_edit_candidates(db, &event.id, round, &track_code)
                    .map_err(|error| friendly_plan_error(db, &event.id, error))?;
                to_value(
                    candidates
                        .into_iter()
                        .map(|candidate| PlanEditCandidateDto {
                            participant_id: candidate.participant_id,
                            participant_name: candidate.participant_name,
                            current_track: candidate.current_track,
                        })
                        .collect::<Vec<_>>(),
                )
            }
            "host_edit_plan_assignment" => {
                let round = u32_arg(args, "round")?;
                let track_code = string(args, "trackCode")?;
                let participant_id = string(args, "participantId")?;
                let expected_version = u32_arg(args, "expectedVersion")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                edit_plan_assignment(
                    db,
                    &event.id,
                    round,
                    &track_code,
                    &participant_id,
                    expected_version,
                    now_secs(),
                )
                .map_err(|error| friendly_plan_error(db, &event.id, error))?;
                to_value(plan_data(db)?)
            }
            "host_upload_pack" => {
                let path = string(args, "path")?;
                let store = swap_files::BlobStore::new(self.data_dir.join("files"));
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let source = Path::new(&path);
                let filename = source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| "the pack file has no name".to_string())?;
                let bytes = fs::read(source).map_err(|error| error.to_string())?;
                let outcome = upload_pack(db, &store, &event.id, filename, &bytes, now_secs())
                    .map_err(|error| error.to_string())?;
                Ok(json!({
                    "id": outcome.id,
                    "filename": filename,
                    "sha256": outcome.sha256,
                    "size": outcome.size,
                    "locked": false
                }))
            }
            "host_open_seed_round" => {
                let deadline_at = i64_arg(args, "deadlineAt")?;
                let channel_id = optional_string(args, "channelId");
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                open_seed_round(
                    db,
                    &event.id,
                    deadline_at,
                    channel_id.as_deref(),
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                to_value(summary(db, &event)?)
            }
            "host_extend_seed_deadline" => {
                let deadline_at = i64_arg(args, "deadlineAt")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                extend_seed_deadline(db, &event.id, deadline_at, now_secs())
                    .map_err(|error| error.to_string())?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                to_value(summary(db, &event)?)
            }
            "host_reopen_seed_round" => {
                let deadline_at = i64_arg(args, "deadlineAt")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                reopen_seed_round(db, &event.id, deadline_at, now_secs())
                    .map_err(|error| error.to_string())?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                to_value(summary(db, &event)?)
            }
            "host_pending_late_seeds" => {
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let seeds = pending_late_seeds(db, &event.id).map_err(|error| error.to_string())?;
                to_value(
                    seeds
                        .into_iter()
                        .map(|seed| LateSeedDto {
                            id: seed.id,
                            display_name: seed.display_name,
                            track_code: seed.track_code,
                            created_at: seed.created_at,
                        })
                        .collect::<Vec<_>>(),
                )
            }
            "host_decide_late_seed" => {
                let id = string(args, "id")?;
                let accept = bool_arg(args, "accept")?;
                let reason = optional_string(args, "reason");
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                if accept {
                    accept_late_seed(db, &event.id, &id, now_secs())
                        .map_err(|error| error.to_string())?;
                } else {
                    reject_late_seed(
                        db,
                        &event.id,
                        &id,
                        reason.as_deref().unwrap_or("not a fit for this event"),
                        now_secs(),
                    )
                    .map_err(|error| error.to_string())?;
                }
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                to_value(summary(db, &event)?)
            }
            "host_move_round_deadline" => {
                let round_no = u32_arg(args, "roundNo")?;
                let deadline_at = i64_arg(args, "deadlineAt")?;
                let reason = string(args, "reason")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                move_round_deadline(db, &event.id, round_no, deadline_at, &reason, now_secs())
                    .map_err(|error| error.to_string())?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                to_value(summary(db, &event)?)
            }
            "host_verify_commitment" => {
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let outcome =
                    verify_commitment(db, &event.id).map_err(|error| error.to_string())?;
                Ok(json!({
                    "recipe": outcome.recipe,
                    "commitment": outcome.commitment,
                    "verified": outcome.verified
                }))
            }
            "host_async_dispatch" => to_value(dispatch_host_async_wave(db)?),
            "host_start_async_event" => to_value(start_host_event_with_channel(db, None)?),
            "host_fill_in_candidates" => {
                let track_code = string(args, "trackCode")?;
                let round_no = u32_arg(args, "roundNo")?;
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let candidates = fill_in_candidates(db, &event.id, &track_code, round_no)
                    .map_err(|error| error.to_string())?;
                to_value(
                    candidates
                        .into_iter()
                        .map(FillInCandidateDto::from)
                        .collect::<Vec<_>>(),
                )
            }
            "host_assign_fill_in" => {
                let track_code = string(args, "trackCode")?;
                let round_no = u32_arg(args, "roundNo")?;
                let participant_id = string(args, "participantId")?;
                let due_at = args.get("dueAt").and_then(Value::as_i64);
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                assign_fill_in(
                    db,
                    &event.id,
                    &track_code,
                    round_no,
                    &participant_id,
                    due_at,
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                Ok(Value::Null)
            }
            "host_start_fill_in_list" => {
                let track_code = string(args, "trackCode")?;
                let round_no = u32_arg(args, "roundNo")?;
                let people: Vec<String> = args
                    .get("participantIds")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let outcome = swap_engine::start_fill_in_list(
                    db,
                    &event.id,
                    &track_code,
                    round_no,
                    &people,
                    now_secs(),
                )
                .map_err(|error| error.to_string())?;
                Ok(json!({ "offeredTo": outcome.offered_to, "status": outcome.status }))
            }
            "host_export_reveal_zip" => {
                let path = string(args, "path")?;
                let store = swap_files::BlobStore::new(self.data_dir.join("files"));
                let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
                let bytes = reveal_zip(db, &store, &event.id).map_err(|error| error.to_string())?;
                fs::write(&path, bytes).map_err(|error| error.to_string())?;
                Ok(Value::String(path))
            }
            "host_audit_log" => to_value(audit_log(db, &self.settings)?),
            other => Err(format!("the E2E bridge does not implement {other}")),
        }
    }

    fn reveal(&mut self, db: &Db) -> Result<Value, String> {
        let event = current_event(db)?.ok_or_else(|| "there is no event".to_string())?;
        let channel = self.settings.reveal_channel_id.trim();
        if channel.is_empty() {
            return Err("Map the reveal channel before you reveal the event.".to_string());
        }
        let now = now_secs();
        match reveal_with_channel(db, &event.id, Some(channel), now) {
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
                to_value(RevealDto {
                    published: true,
                    blocked: Vec::new(),
                    tracks: outcome.tracks.into_iter().map(reveal_track_dto).collect(),
                })
            }
            Err(EngineError::PendingAssignments { ids, .. }) => Ok(json!({
                "published": false,
                "blocked": ids.into_iter().map(|id| {
                    AssignmentRepo::get(db.conn(), &id)
                        .ok()
                        .flatten()
                        .map(|assignment| format!(
                            "Track {} · round {}",
                            assignment.track_code,
                            assignment.round + 1
                        ))
                        .unwrap_or(id)
                }).collect::<Vec<_>>(),
                "tracks": []
            })),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn to_value(value: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

/// Build the mission control DTO with the live gateway health.
pub fn mission_control_json(
    db: &Db,
    settings: &SettingsDto,
    health: Option<swap_discord::GatewayHealth>,
) -> Result<Value, String> {
    to_value(mission_control_with_health(db, settings, health)?)
}

fn string(value: &Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{name} is missing"))
}

fn optional_string(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).map(str::to_string)
}

fn bool_arg(value: &Value, name: &str) -> Result<bool, String> {
    value
        .get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{name} is missing"))
}

fn i64_arg(value: &Value, name: &str) -> Result<i64, String> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{name} is missing"))
}

fn u32_arg(value: &Value, name: &str) -> Result<u32, String> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .ok_or_else(|| format!("{name} is missing"))
}
