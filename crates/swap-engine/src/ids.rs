use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use swap_db::{PlanAssignmentRecord, PlanRecord};
use swap_files::sha256_hex;

/// Ambiguous characters are removed so a host can read a code aloud.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// An opaque, stable track code. The code does not contain the owner, the
/// submission order, or the original title.
pub fn track_code(seed: u64, salt: u32) -> String {
    let digest = Sha256::digest(format!("msw-track-v1:{seed}:{salt}").as_bytes());
    let mut code = String::with_capacity(6);
    code.push('S');
    for byte in digest.iter().take(5) {
        code.push(ALPHABET[(byte & 31) as usize] as char);
    }
    code
}

/// A stable assignment identifier. The Discord custom id uses it as a token.
pub fn assignment_id(event_id: &str, round: u32, track_code: &str) -> String {
    let hash = sha256_hex(format!("msw-assignment-v1:{event_id}:{round}:{track_code}").as_bytes());
    format!("a{}", &hash[..16])
}

/// Commitment over the schedule and the locked policy. The commitment never
/// contains a file hash.
pub fn plan_commitment(plan: &PlanRecord, assignments: &[PlanAssignmentRecord]) -> String {
    let mut canonical = String::new();
    let _ = writeln!(canonical, "msw-plan-v1");
    let _ = writeln!(canonical, "event={}", plan.event_id);
    let _ = writeln!(canonical, "seed={}", plan.seed);
    let _ = writeln!(canonical, "rounds={}", plan.rounds);
    let _ = writeln!(canonical, "settings={}", plan.settings_json);
    reduce_assignments(assignments, &mut canonical);
    sha256_hex(canonical.as_bytes())
}

fn reduce_assignments(assignments: &[PlanAssignmentRecord], canonical: &mut String) {
    let mut ordered: Vec<&PlanAssignmentRecord> = assignments.iter().collect();
    ordered.sort_by(|left, right| {
        left.round
            .cmp(&right.round)
            .then_with(|| left.track_code.cmp(&right.track_code))
            .then_with(|| left.participant_id.cmp(&right.participant_id))
    });
    for assignment in ordered {
        let _ = writeln!(
            canonical,
            "{}:{}:{}",
            assignment.round, assignment.track_code, assignment.participant_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> PlanRecord {
        PlanRecord {
            event_id: "evt-1".to_string(),
            version: 1,
            seed: 42,
            rounds: 1,
            settings_json: "{\"mode\":\"classic\"}".to_string(),
            commitment: String::new(),
            committed_at: 1_000,
            reason: None,
        }
    }

    fn assignment(round: u32, track: &str, participant: &str) -> PlanAssignmentRecord {
        PlanAssignmentRecord {
            event_id: "evt-1".to_string(),
            plan_version: 1,
            round,
            track_code: track.to_string(),
            participant_id: participant.to_string(),
        }
    }

    #[test]
    fn a_track_code_is_stable_and_readable() {
        let code = track_code(42, 0);
        assert_eq!(code, track_code(42, 0));
        assert_ne!(code, track_code(42, 1));
        assert_eq!(code.len(), 6);
        assert!(code.starts_with('S'));
        assert!(
            code.chars()
                .all(|character| character.is_ascii_alphanumeric())
        );
        assert!(!code.contains('0'));
        assert!(!code.contains('O'));
        assert!(!code.contains('I'));
    }

    #[test]
    fn the_assignment_id_is_stable_and_short() {
        let id = assignment_id("evt-1", 2, "S7F3A");
        assert_eq!(id, assignment_id("evt-1", 2, "S7F3A"));
        assert_ne!(id, assignment_id("evt-1", 3, "S7F3A"));
        assert_eq!(id.len(), 17);
        assert!(
            id.chars()
                .all(|character| character.is_ascii_alphanumeric())
        );
    }

    #[test]
    fn the_commitment_is_order_independent() {
        let first = plan_commitment(
            &plan(),
            &[
                assignment(0, "SAAAA", "alice"),
                assignment(0, "SBBBB", "bob"),
            ],
        );
        let second = plan_commitment(
            &plan(),
            &[
                assignment(0, "SBBBB", "bob"),
                assignment(0, "SAAAA", "alice"),
            ],
        );
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn the_commitment_changes_with_the_policy() {
        let assignments = [assignment(0, "SAAAA", "alice")];
        let first = plan_commitment(&plan(), &assignments);
        let mut changed = plan();
        changed.settings_json = "{\"mode\":\"classic\",\"rounds\":2}".to_string();
        assert_ne!(first, plan_commitment(&changed, &assignments));
    }
}
