//! MSW-081: the database refuses scheduling invariant violations by itself,
//! so an engine bug cannot store them. Each case writes raw SQL.

use proptest::prelude::*;
use swap_db::Db;

const PEOPLE: [&str; 3] = ["p0", "p1", "p2"];
const STATUSES: [&str; 9] = [
    "offered",
    "claimed",
    "submitted",
    "skipped_before_claim",
    "skipped_after_claim",
    "expired",
    "overdue",
    "revoked",
    "superseded",
];

#[derive(Debug, Clone)]
enum Op {
    Insert {
        track: usize,
        person: usize,
        status: usize,
    },
    SetStatus {
        row: usize,
        status: usize,
    },
    Move {
        row: usize,
        person: usize,
    },
    Seen {
        track: usize,
        person: usize,
        assignment: usize,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..3usize, 0..3usize, 0..9usize).prop_map(|(track, person, status)| Op::Insert {
            track,
            person,
            status
        }),
        (0..20usize, 0..9usize).prop_map(|(row, status)| Op::SetStatus { row, status }),
        (0..20usize, 0..3usize).prop_map(|(row, person)| Op::Move { row, person }),
        (0..3usize, 0..3usize, 0..4usize).prop_map(|(track, person, assignment)| Op::Seen {
            track,
            person,
            assignment
        }),
    ]
}

fn setup() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO events (id, name, state, seed, settings_json, created_at, updated_at)
             VALUES ('e', 'E', 'active', 1, '{}', 1, 1);",
        )
        .unwrap();
    for (index, person) in PEOPLE.iter().enumerate() {
        db.conn()
            .execute(
                "INSERT INTO participants (event_id, user_id, display_name, role, membership,
                    readiness, timezone, created_at, updated_at)
                 VALUES ('e', ?1, ?1, 'participant', 'active', 'ready', 'UTC', 1, 1)",
                [person],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO tracks (event_id, code, owner_id, state, created_at)
                 VALUES ('e', ?1, ?2, 'pool', 1)",
                [format!("T{index}"), person.to_string()],
            )
            .unwrap();
    }
    db
}

fn count(db: &Db, sql: &str) -> i64 {
    db.conn().query_row(sql, [], |row| row.get(0)).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn raw_writes_never_store_an_invariant_violation(ops in prop::collection::vec(op(), 1..40)) {
        let db = setup();
        let mut next = 0usize;
        for op in ops {
            // Every write can fail. The property is about what the database keeps.
            let _ = match op {
                Op::Insert { track, person, status } => {
                    next += 1;
                    db.conn().execute(
                        "INSERT INTO assignments (id, event_id, track_code, participant_id, round,
                            status, offered_at, created_at, updated_at)
                         VALUES (?1, 'e', ?2, ?3, 0, ?4, 1, 1, 1)",
                        [format!("a{next}"), format!("T{track}"), PEOPLE[person].to_string(),
                         STATUSES[status].to_string()],
                    )
                }
                Op::SetStatus { row, status } => db.conn().execute(
                    "UPDATE assignments SET status = ?1 WHERE id = ?2",
                    [STATUSES[status].to_string(), format!("a{row}")],
                ),
                Op::Move { row, person } => db.conn().execute(
                    "UPDATE assignments SET participant_id = ?1 WHERE id = ?2",
                    [PEOPLE[person].to_string(), format!("a{row}")],
                ),
                Op::Seen { track, person, assignment } => db.conn().execute(
                    "INSERT INTO seen_pairs (event_id, track_code, participant_id, reason,
                        first_seen_at, assignment_id)
                     VALUES ('e', ?1, ?2, 'claim', 1, ?3)",
                    [format!("T{track}"), PEOPLE[person].to_string(), format!("a{assignment}")],
                ),
            };
            prop_assert_eq!(count(&db,
                "SELECT COUNT(*) FROM assignments a JOIN tracks t
                   ON t.event_id = a.event_id AND t.code = a.track_code
                 WHERE t.owner_id = a.participant_id"), 0);
            prop_assert_eq!(count(&db,
                "SELECT COUNT(*) FROM (SELECT track_code FROM assignments
                   WHERE status IN ('offered', 'claimed', 'overdue')
                   GROUP BY event_id, track_code HAVING COUNT(*) > 1)"), 0);
            prop_assert_eq!(count(&db,
                "SELECT COUNT(*) FROM (SELECT participant_id FROM assignments
                   WHERE status IN ('offered', 'claimed', 'overdue')
                   GROUP BY event_id, participant_id HAVING COUNT(*) > 1)"), 0);
            prop_assert_eq!(count(&db,
                "SELECT COUNT(*) FROM (SELECT 1 FROM seen_pairs
                   GROUP BY event_id, track_code, participant_id HAVING COUNT(*) > 1)"), 0);
            // A historical pre-claim skip or expiry may sit beside a seen pair
            // for the same person and track. Only a new active or claimable
            // assignment is a violation.
            prop_assert_eq!(count(&db,
                "SELECT COUNT(*) FROM assignments a
                   JOIN seen_pairs s ON s.event_id = a.event_id
                     AND s.track_code = a.track_code
                     AND s.participant_id = a.participant_id
                  WHERE a.status IN ('offered', 'claimed', 'overdue')
                    AND (s.assignment_id IS NULL OR s.assignment_id <> a.id)"), 0);
        }
    }
}

#[test]
fn the_database_refuses_a_seen_pair_but_allows_the_same_assignment_retry() {
    let db = setup();
    db.conn()
        .execute_batch(
            "INSERT INTO assignments (id, event_id, track_code, participant_id, round,
                status, offered_at, created_at, updated_at)
             VALUES ('a1', 'e', 'T0', 'p1', 0, 'claimed', 1, 1, 1);
             INSERT INTO seen_pairs (event_id, track_code, participant_id, reason,
                first_seen_at, assignment_id)
             VALUES ('e', 'T0', 'p1', 'claim', 1, 'a1');",
        )
        .unwrap();
    let refused = db.conn().execute(
        "INSERT INTO assignments (id, event_id, track_code, participant_id, round,
            status, offered_at, created_at, updated_at)
         VALUES ('a2', 'e', 'T0', 'p1', 1, 'offered', 2, 2, 2)",
        [],
    );
    assert!(
        refused.is_err(),
        "a new assignment for a seen pair is refused"
    );
    let moved = db.conn().execute(
        "UPDATE assignments SET participant_id = 'p1', track_code = 'T0'
         WHERE id = 'a1'",
        [],
    );
    assert!(moved.is_ok(), "the same assignment can keep its pair");
    db.conn()
        .execute("DELETE FROM assignments WHERE id = 'a1'", [])
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO assignments (id, event_id, track_code, participant_id, round,
                status, offered_at, created_at, updated_at)
             VALUES ('a1', 'e', 'T0', 'p1', 0, 'claimed', 1, 1, 1)",
            [],
        )
        .expect("a retry of the same assignment id must survive the guard");
}
