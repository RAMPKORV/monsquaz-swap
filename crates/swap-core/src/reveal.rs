//! Reveal-only ordering helpers.
//!
//! The album order is a keyed permutation of the track codes. The key is the
//! event seed. Submission time and plan order never affect the result.

use sha2::{Digest, Sha256};

/// Return the track codes in reveal order.
///
/// The order is `sort(HMAC-SHA256(seed, track_code), track_code)`. It is
/// deterministic for one event and does not depend on the input order.
pub fn album_order(track_codes: &[String], seed: u64) -> Vec<String> {
    let mut keyed = track_codes
        .iter()
        .map(|code| {
            (
                hmac_sha256(&seed.to_be_bytes(), code.as_bytes()),
                code.clone(),
            )
        })
        .collect::<Vec<_>>();
    keyed.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    keyed.into_iter().map(|(_, code)| code).collect()
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0x36_u8; BLOCK];
    let mut outer = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner[index] ^= normalized[index];
        outer[index] ^= normalized[index];
    }
    let mut inner_hash = Sha256::new();
    inner_hash.update(inner);
    inner_hash.update(message);
    let inner_digest = inner_hash.finalize();
    let mut outer_hash = Sha256::new();
    outer_hash.update(outer);
    outer_hash.update(inner_digest);
    outer_hash.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // scenario: The album order is secret-shuffled
    #[test]
    fn hmac_matches_rfc_4231_case_two() {
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    // scenario: The reveal order does not follow submission time
    #[test]
    fn the_order_is_stable_and_uses_every_track_once() {
        let codes = ["S1", "S2", "S3", "S4", "S5"]
            .iter()
            .map(|code| (*code).to_string())
            .collect::<Vec<_>>();
        let first = album_order(&codes, 42);
        assert_eq!(first, album_order(&codes, 42));
        let mut reversed = codes.clone();
        reversed.reverse();
        assert_eq!(first, album_order(&reversed, 42));
        let mut sorted = first.clone();
        sorted.sort();
        assert_eq!(sorted, codes);
    }

    // scenario: The reveal order does not follow submission time
    #[test]
    fn another_seed_changes_the_order() {
        let codes = ["S1", "S2", "S3", "S4"]
            .iter()
            .map(|code| (*code).to_string())
            .collect::<Vec<_>>();
        assert_ne!(album_order(&codes, 1), album_order(&codes, 2));
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
