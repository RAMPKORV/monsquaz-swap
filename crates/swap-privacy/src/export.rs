use std::io::{Cursor, Write};

use serde::Serialize;
use swap_core::AssignmentStatus;
use swap_db::{AssignmentRepo, Db, FileRepo, ParticipantRepo, SeenRepo, TrackRepo};
use swap_files::{BlobStore, sha256_hex};

use crate::PrivacyError;

#[derive(Debug, Clone)]
pub struct ParticipantExport {
    pub zip: Vec<u8>,
    pub manifest: ExportManifest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    pub event_id: String,
    pub generated_at: i64,
    pub participant: ParticipantInfo,
    pub seeded_tracks: Vec<String>,
    pub assignments: Vec<ExportAssignment>,
    pub files: Vec<ExportFileInfo>,
    pub seen_pairs: Vec<ExportSeenPair>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantInfo {
    pub user_id: String,
    pub display_name: String,
    pub role: String,
    pub membership: String,
    pub readiness: String,
    pub timezone: String,
    pub max_handoffs: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAssignment {
    pub track_code: String,
    pub round: u32,
    pub status: String,
    pub offered_at: i64,
    pub claimed_at: Option<i64>,
    pub due_at: Option<i64>,
    pub submitted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileInfo {
    pub track_code: String,
    pub version: u32,
    pub extension: String,
    pub size: u64,
    pub sha256: String,
    pub delivery_sha256: String,
    pub path_in_zip: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSeenPair {
    pub track_code: String,
    pub reason: String,
    pub first_seen_at: i64,
}

pub fn export_participant(
    db: &Db,
    store: &BlobStore,
    event_id: &str,
    user_id: &str,
    now: i64,
) -> Result<ParticipantExport, PrivacyError> {
    let participant = ParticipantRepo::get(db.conn(), event_id, user_id)?
        .ok_or_else(|| PrivacyError::ParticipantMissing(user_id.to_string()))?;

    let seeded_tracks: Vec<String> = TrackRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|track| track.owner_id == user_id)
        .map(|track| track.code)
        .collect();

    let assignments: Vec<ExportAssignment> = AssignmentRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|assignment| assignment.participant_id == user_id)
        .map(|assignment| ExportAssignment {
            track_code: assignment.track_code,
            round: assignment.round,
            status: status_name(assignment.status).to_string(),
            offered_at: assignment.offered_at,
            claimed_at: assignment.claimed_at,
            due_at: assignment.due_at,
            submitted_at: assignment.submitted_at,
        })
        .collect();

    let seen_pairs: Vec<ExportSeenPair> =
        SeenRepo::list_for_participant(db.conn(), event_id, user_id)?
            .into_iter()
            .map(|pair| ExportSeenPair {
                track_code: pair.track_code,
                reason: pair.reason,
                first_seen_at: pair.first_seen_at,
            })
            .collect();

    let mut files = Vec::new();
    for file in FileRepo::list_for_holder(db.conn(), event_id, user_id)? {
        let original_sha = file
            .original_sha256
            .clone()
            .unwrap_or_else(|| file.sha256.clone());
        let bytes = store.read(&original_sha)?;
        if sha256_hex(&bytes) != original_sha {
            return Err(PrivacyError::HashMismatch(file.id));
        }
        let path_in_zip = format!(
            "files/{}.v{}.{}",
            file.track_code, file.version, file.extension
        );
        files.push((
            ExportFileInfo {
                track_code: file.track_code,
                version: file.version,
                extension: file.extension,
                size: bytes.len() as u64,
                sha256: original_sha,
                delivery_sha256: file.sha256,
                path_in_zip: path_in_zip.clone(),
            },
            bytes,
        ));
    }

    let manifest = ExportManifest {
        event_id: event_id.to_string(),
        generated_at: now,
        participant: ParticipantInfo {
            user_id: participant.user_id,
            display_name: participant.display_name,
            role: format!("{:?}", participant.role),
            membership: format!("{:?}", participant.membership),
            readiness: format!("{:?}", participant.readiness),
            timezone: participant.timezone,
            max_handoffs: participant.max_handoffs,
        },
        seeded_tracks,
        assignments,
        files: files.iter().map(|(info, _)| info.clone()).collect(),
        seen_pairs,
    };

    let json = serde_json::to_vec_pretty(&manifest)?;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("participant.json", options)?;
    writer.write_all(&json)?;
    writer.start_file("README.txt", options)?;
    writer.write_all(
        b"This export contains your Monsquaz Swap records and the files you submitted.\n\
          It does not contain another participant's private data.\n",
    )?;
    for (info, bytes) in &files {
        writer.start_file(&info.path_in_zip, options)?;
        writer.write_all(bytes)?;
    }
    let zip = writer.finish()?.into_inner();
    Ok(ParticipantExport { zip, manifest })
}

fn status_name(status: AssignmentStatus) -> &'static str {
    match status {
        AssignmentStatus::Offered => "offered",
        AssignmentStatus::Claimed => "claimed",
        AssignmentStatus::Submitted => "submitted",
        AssignmentStatus::SkippedBeforeClaim => "skipped_before_claim",
        AssignmentStatus::SkippedAfterClaim => "skipped_after_claim",
        AssignmentStatus::Expired => "expired",
        AssignmentStatus::Overdue => "overdue",
        AssignmentStatus::Revoked => "revoked",
        AssignmentStatus::Superseded => "superseded",
    }
}
