//! Classic group shape, lookup, cadence, and soft-bound warnings.
//!
//! These helpers are pure. The engine and the host use the same rules so the
//! planner preview and the generated plan agree.

use serde::{Deserialize, Serialize};

pub const MIN_GROUP_SIZE: u32 = 5;
pub const MAX_GROUP_SIZE: u32 = 12;
pub const MAX_GROUP_COUNT: u32 = 4;
pub const MIN_PHASES: u32 = 3;
pub const MAX_PHASES: u32 = 5;

/// A suggested group count and the balanced size for that count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupShape {
    pub count: u32,
    pub size: u32,
}

/// Return the lookup suggestion for a roster size.
///
/// The ranges follow `docs/EVENT_SETTINGS.md`:
/// 1 group up to 9 people, 2 for 10–13, 3 for 14–18 and 23–32, and 4 for
/// 19–22 and 33–40. Larger rosters use 4 groups.
pub fn suggest_groups(participants: usize) -> GroupShape {
    let count = match participants {
        0..=9 => 1,
        10..=13 => 2,
        14..=18 => 3,
        19..=22 => 4,
        23..=32 => 3,
        33..=40 => 4,
        _ => 4,
    };
    let size = participants.div_ceil(count);
    GroupShape {
        count: count as u32,
        size: size as u32,
    }
}

/// Split a roster into group sizes.
///
/// A positive `size` is a maximum group size. This can produce unequal groups,
/// for example 12 and 5. A zero `size` asks for balanced groups.
pub fn group_sizes(participants: usize, count: u32, size: u32) -> Vec<u32> {
    if participants == 0 {
        return Vec::new();
    }
    if size > 0 {
        let mut remaining = participants;
        let mut sizes = Vec::new();
        while remaining > 0 && sizes.len() < MAX_GROUP_COUNT as usize {
            let next = remaining.min(size as usize);
            sizes.push(next as u32);
            remaining -= next;
        }
        if remaining > 0 {
            // The configured shape cannot hold the roster. Put the overflow in
            // the last group so no participant disappears. The warning path
            // reports the large group.
            if let Some(last) = sizes.last_mut() {
                *last += remaining as u32;
            }
        }
        return sizes;
    }
    let count = count.clamp(1, MAX_GROUP_COUNT) as usize;
    let balanced = participants.div_ceil(count);
    let mut sizes = Vec::new();
    let mut remaining = participants;
    while remaining > 0 {
        let next = remaining.min(balanced);
        sizes.push(next as u32);
        remaining -= next;
    }
    sizes
}

/// Return the default K-round cadence for a classic event.
///
/// K is about T/4. The result keeps the phase count between 3 and 5 when the
/// event is long enough. The result is at least 1.
pub fn default_cadence(rounds: u32) -> u32 {
    if rounds <= 1 {
        return 1;
    }
    let mut cadence = (rounds + 2) / 4;
    cadence = cadence.max(1).min(rounds);
    if rounds.div_ceil(cadence) > MAX_PHASES {
        cadence = rounds.div_ceil(MAX_PHASES).max(1);
    }
    if rounds.div_ceil(cadence) < MIN_PHASES {
        cadence = 1;
    }
    cadence
}

/// Return the number of phases for a cadence.
pub fn phase_count(rounds: u32, cadence: u32) -> u32 {
    if rounds == 0 || cadence == 0 {
        return 0;
    }
    rounds.div_ceil(cadence)
}

/// A soft-bound or cadence warning. These never block a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupWarning {
    Small { size: u32 },
    Large { size: u32 },
    Unequal { smallest: u32, largest: u32 },
    Cadence { phases: u32 },
    Exhausted { size: u32, rounds: u32 },
}

/// Return the soft warnings for a group shape.
///
/// `cadence` is the raw event setting. The exhausted warning only applies when
/// the host selected that policy.
pub fn group_warnings(sizes: &[u32], rounds: u32, cadence: &str) -> Vec<GroupWarning> {
    let mut warnings = Vec::new();
    if sizes.is_empty() {
        return warnings;
    }
    let smallest = *sizes.iter().min().unwrap_or(&0);
    let largest = *sizes.iter().max().unwrap_or(&0);
    if smallest < MIN_GROUP_SIZE {
        warnings.push(GroupWarning::Small { size: smallest });
    }
    if largest > MAX_GROUP_SIZE {
        warnings.push(GroupWarning::Large { size: largest });
    }
    if largest.saturating_sub(smallest) > 1 {
        warnings.push(GroupWarning::Unequal { smallest, largest });
    }
    let cadence_rounds = default_cadence(rounds);
    let phases = phase_count(rounds, cadence_rounds);
    if rounds > 0 && !(MIN_PHASES..=MAX_PHASES).contains(&phases) {
        warnings.push(GroupWarning::Cadence { phases });
    }
    if cadence.eq_ignore_ascii_case("exhausted") && largest > rounds.saturating_add(1) {
        warnings.push(GroupWarning::Exhausted {
            size: largest,
            rounds,
        });
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_matches_the_setting_table() {
        assert_eq!(suggest_groups(9), GroupShape { count: 1, size: 9 });
        assert_eq!(suggest_groups(10), GroupShape { count: 2, size: 5 });
        assert_eq!(suggest_groups(18), GroupShape { count: 3, size: 6 });
        assert_eq!(suggest_groups(20), GroupShape { count: 4, size: 5 });
        assert_eq!(suggest_groups(23), GroupShape { count: 3, size: 8 });
        assert_eq!(suggest_groups(40), GroupShape { count: 4, size: 10 });
        assert_eq!(suggest_groups(41), GroupShape { count: 4, size: 11 });
    }

    #[test]
    fn manual_sizes_can_be_unequal() {
        assert_eq!(group_sizes(17, 2, 12), vec![12, 5]);
        assert_eq!(group_sizes(17, 2, 0), vec![9, 8]);
        assert_eq!(group_sizes(13, 4, 0), vec![4, 4, 4, 1]);
    }

    #[test]
    fn the_default_cadence_targets_three_to_five_phases() {
        assert_eq!(default_cadence(8), 2);
        assert_eq!(phase_count(8, 2), 4);
        assert_eq!(default_cadence(4), 1);
        assert_eq!(phase_count(4, 1), 4);
        assert_eq!(default_cadence(20), 5);
        assert_eq!(phase_count(20, 5), 4);
        assert_eq!(default_cadence(3), 1);
        assert_eq!(phase_count(3, 1), 3);
    }

    #[test]
    fn soft_warnings_do_not_block() {
        let warnings = group_warnings(&[3, 3], 8, "K rounds");
        assert!(warnings.contains(&GroupWarning::Small { size: 3 }));
        assert!(!warnings.contains(&GroupWarning::Cadence { phases: 4 }));
        let warnings = group_warnings(&[12, 5], 8, "K rounds");
        assert!(warnings.contains(&GroupWarning::Unequal {
            smallest: 5,
            largest: 12
        }));
        assert!(!warnings.contains(&GroupWarning::Large { size: 12 }));
    }

    #[test]
    fn the_exhausted_policy_warns_only_when_it_cannot_fire() {
        assert!(
            group_warnings(&[10], 8, "Exhausted").contains(&GroupWarning::Exhausted {
                size: 10,
                rounds: 8
            })
        );
        assert!(
            !group_warnings(&[8], 8, "Exhausted")
                .contains(&GroupWarning::Exhausted { size: 8, rounds: 8 })
        );
    }
}
