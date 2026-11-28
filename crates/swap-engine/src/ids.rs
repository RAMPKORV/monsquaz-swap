use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use swap_db::{PlanAssignmentRecord, PlanRecord};
use swap_files::sha256_hex;

/// Ambiguous characters are removed so a host can read a code aloud.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// A cryptographically random 128-bit value, encoded as 32 lowercase hex
/// characters. The event secret and the commitment nonce use this.
pub fn random_hex_128() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        static FALLBACK: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        let counter = FALLBACK.fetch_add(1, Ordering::Relaxed);
        let digest = Sha256::digest(format!("msw-fallback-v1:{now}:{counter}").as_bytes());
        bytes.copy_from_slice(&digest[..16]);
    }
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

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

/// An opaque track code for a new-model event. The code derives from the
/// 128-bit event secret, not from the creation time or the submission order.
pub fn track_code_from_secret(secret_hex: &str, salt: u32) -> String {
    let digest = Sha256::digest(format!("msw-track-v2:{secret_hex}:{salt}").as_bytes());
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

/// A unique comment identifier. A process-local sequence keeps two equal
/// comments distinct. Comment creation is not retried after a commit.
pub fn comment_id(
    event_id: &str,
    track_code: &str,
    participant_id: &str,
    kind: &str,
    body: &str,
    now: i64,
) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let hash = sha256_hex(
        format!(
            "msw-comment-v1:{event_id}:{track_code}:{participant_id}:{kind}:{now}:{sequence}:{body}"
        )
        .as_bytes(),
    );
    format!("c{}", &hash[..16])
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

/// Inputs to commitment v2.
pub struct CommitmentInputs<'a> {
    pub event_id: &'a str,
    pub plan_version: u32,
    pub total_rounds: u32,
    pub exchange_rounds: u32,
    pub secret_hex: &'a str,
    pub nonce: &'a str,
    pub pack_id: Option<&'a str>,
    pub settings_json: &'a str,
    pub seed_owners: &'a [(String, String)],
    pub assignments: &'a [PlanAssignmentRecord],
}

/// Commitment v2 covers the seed round owner map, the exchange schedule, the
/// pack identity, the locked policy, and the plan version. It returns 32
/// lowercase hex characters. The event secret and the nonce are random
/// 128-bit values. The recipe is published at the reveal.
pub fn plan_commitment_v2(input: &CommitmentInputs<'_>) -> String {
    let mut canonical = String::new();
    let _ = writeln!(canonical, "msw-plan-v2");
    let _ = writeln!(canonical, "event={}", input.event_id);
    let _ = writeln!(canonical, "version={}", input.plan_version);
    let _ = writeln!(canonical, "model=1");
    let _ = writeln!(canonical, "total_rounds={}", input.total_rounds);
    let _ = writeln!(canonical, "exchange_rounds={}", input.exchange_rounds);
    let _ = writeln!(canonical, "secret={}", input.secret_hex);
    let _ = writeln!(canonical, "nonce={}", input.nonce);
    let _ = writeln!(canonical, "pack={}", input.pack_id.unwrap_or("none"));
    let _ = writeln!(
        canonical,
        "policy={}",
        sha256_hex(input.settings_json.as_bytes())
    );
    let mut owners: Vec<&(String, String)> = input.seed_owners.iter().collect();
    owners.sort_by(|left, right| left.0.cmp(&right.0));
    for (track_code, owner_id) in owners {
        let _ = writeln!(canonical, "R1:{track_code}:{owner_id}");
    }
    let mut ordered: Vec<&PlanAssignmentRecord> = input.assignments.iter().collect();
    ordered.sort_by(|left, right| {
        left.round
            .cmp(&right.round)
            .then_with(|| left.track_code.cmp(&right.track_code))
            .then_with(|| left.participant_id.cmp(&right.participant_id))
    });
    for assignment in ordered {
        let round_no = assignment.round + 2;
        let _ = writeln!(
            canonical,
            "R{round_no}:{}:{}",
            assignment.track_code, assignment.participant_id
        );
    }
    sha256_hex(canonical.as_bytes())[..32].to_string()
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

    // scenario: A commitment does not publish a file hash
    #[test]
    fn the_commitment_changes_with_the_policy() {
        let assignments = [assignment(0, "SAAAA", "alice")];
        let first = plan_commitment(&plan(), &assignments);
        let mut changed = plan();
        changed.settings_json = "{\"mode\":\"classic\",\"rounds\":2}".to_string();
        assert_ne!(first, plan_commitment(&changed, &assignments));
    }
}
