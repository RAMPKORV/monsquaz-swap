//! Fill-in flows for a stranded classic slot (D-040, MSW-083).
//!
//! `host_appoints` keeps the host in charge: a declined or expired fill-in
//! offer opens a host decision. `auto_chain` offers the slot to up to three
//! ranked eligible people, one at a time. `host_list` offers the slot down the
//! ordered list that the host sets. Every step re-checks eligibility (D-050).
//! A chain only offers a slot; it never opens or closes a round.

use swap_db::{
    AssignmentRecord, AssignmentRepo, AuditRepo, Connection, Db, EventRepo, FillInChainRecord,
    FillInChainRepo, HostDecisionRecord, HostDecisionRepo, ParticipantRepo,
};

use crate::delivery::enqueue_effect;
use crate::effects::Effect;
use crate::engine::{
    EngineError, begin, commit, load_event, parse_settings, quiet_hours, require_open_event,
};
use crate::fill_in::{
    FillInTier, Slot, ensure_stranded, is_fill_in_turn, offer_fill_in, open_round_deadline,
};
use crate::ids::random_hex_128;
use crate::settings::{EventSettings, FillInFlow, Mode};

/// The most offers an automatic chain makes before the host decides.
pub const AUTO_CHAIN_OFFERS: usize = 3;

const ACTIVE: &str = "active";
const FILL_IN_DECISION: &str = "fill_in";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillInListOutcome {
    /// The person who has the offer now, or `None` when the chain waits for
    /// quiet hours to end or ended without an eligible person.
    pub offered_to: Option<String>,
    /// `active` or `exhausted`.
    pub status: String,
    /// True when the exhausted list opened a host decision. A gap cell with
    /// no assignment row leaves the track unchanged and opens none.
    pub decision_opened: bool,
}

/// React to a classic slot that lost its holder: a pre-claim skip, a skip
/// after a claim, an offer that expired, or a turn that the host marked
/// missed. The caller runs this inside the transaction that strands the slot.
pub(crate) fn on_slot_stranded(
    conn: &Connection,
    stranded: &AssignmentRecord,
    now: i64,
) -> Result<(), EngineError> {
    let event_id = stranded.event_id.as_str();
    let Some(event) = EventRepo::get(conn, event_id)? else {
        return Ok(());
    };
    let settings = parse_settings(&event)?;
    if settings.mode != Mode::Classic
        || EventRepo::round_model(conn, event_id)? != 1
        || open_round_deadline(conn, event_id, stranded.round + 2).is_err()
    {
        return Ok(());
    }
    if let Some(mut chain) = FillInChainRepo::active_for_assignment(conn, &stranded.id)? {
        chain.current_assignment_id = None;
        advance(conn, &settings, &mut chain, now)?;
        return Ok(());
    }
    match settings.fill_in_flow {
        FillInFlow::AutoChain => {
            let mut chain = new_chain(stranded, "auto_chain", Vec::new(), now);
            chain.tried.push(stranded.participant_id.clone());
            advance(conn, &settings, &mut chain, now)?;
            Ok(())
        }
        FillInFlow::HostList => open_decision(conn, event_id, &stranded.id, now),
        FillInFlow::HostAppoints if is_fill_in_turn(event_id, stranded) => {
            open_decision(conn, event_id, &stranded.id, now)
        }
        FillInFlow::HostAppoints => Ok(()),
    }
}

/// A claim fills the chain that made the offer.
pub(crate) fn on_claimed(
    conn: &Connection,
    assignment: &AssignmentRecord,
    now: i64,
) -> Result<(), EngineError> {
    if let Some(mut chain) = FillInChainRepo::active_for_assignment(conn, &assignment.id)? {
        chain.status = "filled".to_string();
        chain.updated_at = now;
        FillInChainRepo::save(conn, &chain)?;
    }
    if is_fill_in_turn(&assignment.event_id, assignment) {
        close_decisions(
            conn,
            &assignment.event_id,
            Some(&assignment.track_code),
            "a fill-in claimed the track",
            now,
        )?;
    }
    Ok(())
}

/// The host appointed a person or ordered a list for the slot. Any older
/// chain for the slot stops, and the open fill-in decisions close.
pub(crate) fn host_took_over(
    conn: &Connection,
    event_id: &str,
    track_code: &str,
    round: u32,
    now: i64,
) -> Result<(), EngineError> {
    if let Some(mut chain) = FillInChainRepo::active_for_slot(conn, event_id, track_code, round)? {
        chain.status = "cancelled".to_string();
        chain.updated_at = now;
        FillInChainRepo::save(conn, &chain)?;
    }
    close_decisions(
        conn,
        event_id,
        Some(track_code),
        "the host chose a fill-in",
        now,
    )
}

/// The host opened the next round. A chain that waits for quiet hours stops,
/// and every open fill-in decision closes: the track passes unchanged.
pub(crate) fn on_round_open(
    conn: &Connection,
    event_id: &str,
    now: i64,
) -> Result<(), EngineError> {
    let chains = FillInChainRepo::list_active(conn, event_id)?;
    for mut chain in chains {
        chain.status = "cancelled".to_string();
        chain.updated_at = now;
        FillInChainRepo::save(conn, &chain)?;
        AuditRepo::insert(
            conn,
            event_id,
            now,
            "Host",
            "Fill-in chain stopped",
            Some(&chain.track_code),
            &serde_json::json!({ "round": chain.round + 2, "reason": "the next round opened" })
                .to_string(),
        )?;
    }
    close_decisions(conn, event_id, None, "the next round opened", now)
}

/// Retry every chain that waits without an offer. A chain waits when the next
/// person is inside their quiet hours. The deadline tick calls this.
pub(crate) fn resume_waiting(
    conn: &Connection,
    event_id: &str,
    now: i64,
) -> Result<(), EngineError> {
    let Some(event) = EventRepo::get(conn, event_id)? else {
        return Ok(());
    };
    let settings = parse_settings(&event)?;
    for mut chain in FillInChainRepo::list_active(conn, event_id)? {
        if chain.current_assignment_id.is_none() {
            advance(conn, &settings, &mut chain, now)?;
        }
    }
    Ok(())
}

/// The host orders a list of people for one stranded slot. The bot offers the
/// slot to the first eligible tier-1 person on the list, then to the next
/// person after a decline or an expiry. A tier-2 entry waits until no untried
/// tier-1 person is eligible.
pub fn start_fill_in_list(
    db: &Db,
    event_id: &str,
    track_code: &str,
    round_no: u32,
    people: &[String],
    now: i64,
) -> Result<FillInListOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "fill-in list")?;
    let settings = parse_settings(&event)?;
    if round_no < 2 {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    let mut unique = people.to_vec();
    unique.sort();
    unique.dedup();
    if people.is_empty() {
        return Err(EngineError::FillInListEmpty);
    }
    if unique.len() != people.len() {
        return Err(EngineError::FillInListDuplicate);
    }
    let transaction = begin(db)?;
    open_round_deadline(&transaction, event_id, round_no)?;
    if AssignmentRepo::list_active(&transaction, event_id)?
        .iter()
        .any(|assignment| assignment.track_code == track_code)
    {
        return Err(EngineError::FillInNotEligible(
            "the track has an active holder".to_string(),
        ));
    }
    let index = round_no - 2;
    ensure_stranded(&transaction, event_id, track_code, index)?;
    host_took_over(&transaction, event_id, track_code, index, now)?;
    let mut chain = FillInChainRecord {
        id: chain_id(event_id, track_code, index, now),
        event_id: event_id.to_string(),
        track_code: track_code.to_string(),
        round: index,
        flow: "host_list".to_string(),
        host_list: people.to_vec(),
        tried: Vec::new(),
        offers: 0,
        current_assignment_id: None,
        status: ACTIVE.to_string(),
        created_at: now,
        updated_at: now,
    };
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Fill-in list ordered",
        Some(track_code),
        &serde_json::json!({ "round": round_no, "people": people }).to_string(),
    )?;
    let decision_opened = advance(&transaction, &settings, &mut chain, now)?;
    let offered_to = match &chain.current_assignment_id {
        Some(id) => AssignmentRepo::get(&transaction, id)?.map(|offer| offer.participant_id),
        None => None,
    };
    commit(transaction)?;
    Ok(FillInListOutcome {
        offered_to,
        status: chain.status,
        decision_opened,
    })
}

fn chain_id(event_id: &str, track_code: &str, round: u32, now: i64) -> String {
    // A random suffix keeps two chains for the same slot in the same second
    // apart. The active-slot index still allows only one active chain.
    format!(
        "chain:{event_id}:{round}:{track_code}:{now}:{}",
        &random_hex_128()[..8]
    )
}

fn new_chain(
    stranded: &AssignmentRecord,
    flow: &str,
    host_list: Vec<String>,
    now: i64,
) -> FillInChainRecord {
    FillInChainRecord {
        id: chain_id(
            &stranded.event_id,
            &stranded.track_code,
            stranded.round,
            now,
        ),
        event_id: stranded.event_id.clone(),
        track_code: stranded.track_code.clone(),
        round: stranded.round,
        flow: flow.to_string(),
        host_list,
        tried: Vec::new(),
        offers: 0,
        current_assignment_id: None,
        status: ACTIVE.to_string(),
        created_at: now,
        updated_at: now,
    }
}

/// Offer the slot to the next eligible person, wait for quiet hours, or end
/// the chain with a host decision. The chain row is saved in every case.
fn advance(
    conn: &Connection,
    settings: &EventSettings,
    chain: &mut FillInChainRecord,
    now: i64,
) -> Result<bool, EngineError> {
    chain.updated_at = now;
    if open_round_deadline(conn, &chain.event_id, chain.round + 2).is_err() {
        chain.status = "cancelled".to_string();
        FillInChainRepo::save(conn, chain)?;
        return Ok(false);
    }
    let limit = if chain.flow == "host_list" {
        chain.host_list.len()
    } else {
        AUTO_CHAIN_OFFERS
    };
    let next = if (chain.offers as usize) < limit {
        next_person(conn, settings, chain, now)?
    } else {
        None
    };
    let Some(user_id) = next else {
        return exhaust(conn, chain, now);
    };
    let participant = ParticipantRepo::get(conn, &chain.event_id, &user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.clone()))?;
    let resting = participant.timezone.parse().is_ok_and(|timezone| {
        quiet_hours(
            now,
            timezone,
            settings.quiet_hours_start,
            settings.quiet_hours_end,
        )
    });
    if resting {
        chain.current_assignment_id = None;
        FillInChainRepo::save(conn, chain)?;
        return Ok(false);
    }
    let due_at = now + settings.fill_in_window_secs;
    let claim_deadline_at = now + settings.fill_in_claim_window_secs;
    let offer_id = offer_fill_in(
        conn,
        &chain.event_id,
        &chain.track_code,
        chain.round,
        &user_id,
        due_at,
        claim_deadline_at,
        "worker",
        now,
    )?;
    chain.tried.push(user_id);
    chain.offers += 1;
    chain.current_assignment_id = Some(offer_id);
    FillInChainRepo::save(conn, chain)?;
    Ok(false)
}

/// The next person for the chain, from a fresh eligibility check. The automatic
/// chain walks every eligible tier in rank order, so a tier-2 person gets the
/// next offer after the tier-1 people decline (D-064). A host list keeps the
/// host order inside each tier; it does not override the tier-1-first rule. A
/// host list entry who is not eligible now is passed over and never offered.
fn next_person(
    conn: &Connection,
    settings: &EventSettings,
    chain: &mut FillInChainRecord,
    now: i64,
) -> Result<Option<String>, EngineError> {
    let slot = Slot::load(
        conn,
        &chain.event_id,
        &chain.track_code,
        chain.round,
        settings,
        now,
    )?;
    let ranked = slot.ranked(conn)?;
    if chain.flow != "host_list" {
        return Ok(ranked
            .into_iter()
            .map(|candidate| candidate.participant_id)
            .find(|user_id| !chain.tried.contains(user_id)));
    }
    let tier_one_untried = ranked.iter().any(|candidate| {
        candidate.tier == FillInTier::Volunteer && !chain.tried.contains(&candidate.participant_id)
    });
    for user_id in chain.host_list.clone() {
        if chain.tried.contains(&user_id) {
            continue;
        }
        let Some(candidate) = ranked
            .iter()
            .find(|candidate| candidate.participant_id == user_id)
        else {
            chain.tried.push(user_id);
            continue;
        };
        if candidate.tier == FillInTier::ExtraTurn && tier_one_untried {
            // The host order applies inside a tier. A tier-2 entry waits
            // until no untried tier-1 person is eligible (D-050).
            continue;
        }
        return Ok(Some(user_id));
    }
    Ok(None)
}

fn exhaust(
    conn: &Connection,
    chain: &mut FillInChainRecord,
    now: i64,
) -> Result<bool, EngineError> {
    chain.status = "exhausted".to_string();
    chain.current_assignment_id = None;
    FillInChainRepo::save(conn, chain)?;
    let last = AssignmentRepo::list_for_event(conn, &chain.event_id)?
        .into_iter()
        .filter(|assignment| {
            assignment.track_code == chain.track_code && assignment.round == chain.round
        })
        .max_by_key(|assignment| (assignment.offered_at, assignment.created_at));
    let Some(last) = last else {
        return Ok(false);
    };
    open_decision(conn, &chain.event_id, &last.id, now)?;
    Ok(true)
}

/// Open one host decision for a slot that still needs a fill-in.
fn open_decision(
    conn: &Connection,
    event_id: &str,
    assignment_id: &str,
    now: i64,
) -> Result<(), EngineError> {
    if HostDecisionRepo::open_for_assignment(conn, assignment_id, FILL_IN_DECISION)?.is_some() {
        return Ok(());
    }
    HostDecisionRepo::insert(
        conn,
        &HostDecisionRecord {
            id: format!("{assignment_id}:{FILL_IN_DECISION}:{now}"),
            event_id: event_id.to_string(),
            assignment_id: Some(assignment_id.to_string()),
            kind: FILL_IN_DECISION.to_string(),
            status: "open".to_string(),
            requested_hours: None,
            created_at: now,
            decided_at: None,
            reason: None,
        },
    )?;
    enqueue_effect(
        conn,
        event_id,
        &Effect::HostAlert {
            key: format!("{assignment_id}:{FILL_IN_DECISION}"),
            content: "A stranded track needs a fill-in decision.".to_string(),
        },
        now,
    )?;
    Ok(())
}

fn close_decisions(
    conn: &Connection,
    event_id: &str,
    track_code: Option<&str>,
    reason: &str,
    now: i64,
) -> Result<(), EngineError> {
    for decision in HostDecisionRepo::list_open(conn, event_id)? {
        if decision.kind != FILL_IN_DECISION {
            continue;
        }
        let matches = match (track_code, &decision.assignment_id) {
            (None, _) => true,
            (Some(track), Some(id)) => AssignmentRepo::get(conn, id)?
                .is_some_and(|assignment| assignment.track_code == track),
            (Some(_), None) => false,
        };
        if matches {
            HostDecisionRepo::close(conn, &decision.id, "addressed", now, reason)?;
        }
    }
    Ok(())
}
