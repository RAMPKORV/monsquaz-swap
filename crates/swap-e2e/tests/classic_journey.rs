//! The full classic journey: host actions, participant DMs, and reveal.

use std::time::Duration;

use swap_core::{AssignmentStatus, EventState, Membership};
use swap_e2e::{BASE_TIME, HeadlessApp, mod_track, pack_zip};

fn timeout() -> Duration {
    Duration::from_secs(20)
}

fn day() -> i64 {
    24 * 60 * 60
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seed_round_collects_seeds_and_sends_the_pack() {
    let app = HeadlessApp::start().await.expect("the app must start");
    let event = app
        .create_event("Classic smoke")
        .expect("the event must be created");
    let pack = pack_zip(&[("01-pack.mod", &mod_track("Pack one", 0, 0))]);
    app.upload_pack(&event, "pack.zip", &pack)
        .expect("the pack must upload");
    app.open_seed_round(&event, BASE_TIME + 7 * day())
        .expect("Round 1 must open");

    let mira = app.sim.participant("mira").unwrap();
    app.join(mira).expect("the join must send");
    app.send_file(mira, "mira.mod", &mod_track("Mira seed", 0, 0), "my seed")
        .expect("the seed must send");

    app.wait_for_participant(&event, mira, timeout())
        .await
        .expect("the participant must exist");
    app.wait_for_dm(mira, "Monsquaz Swap pack", timeout())
        .await
        .expect("the pack DM must arrive");

    let membership = app.with_db(|db| {
        swap_db::ParticipantRepo::get(db.conn(), &event, mira)
            .unwrap()
            .unwrap()
            .membership
    });
    assert_eq!(membership, Membership::Seeded);
    let tracks = app.with_db(|db| swap_db::TrackRepo::list_for_event(db.conn(), &event).unwrap());
    assert_eq!(tracks.len(), 1);

    app.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_full_classic_journey_reaches_a_reveal() {
    let app = HeadlessApp::start().await.expect("the app must start");
    let event = app
        .create_event("The long way around")
        .expect("the event must be created");
    let pack = pack_zip(&[
        ("01-pack.mod", &mod_track("Pack one", 0, 0)),
        ("02-pack.mod", &mod_track("Pack two", 4, 0)),
    ]);
    app.upload_pack(&event, "pack.zip", &pack)
        .expect("the pack must upload");
    app.open_seed_round(&event, BASE_TIME + 7 * day())
        .expect("Round 1 must open");

    let mira = app.sim.participant("mira").unwrap();
    let jonas = app.sim.participant("jonas").unwrap();
    let selma = app.sim.participant("selma").unwrap();
    let theo = app.sim.participant("theo").unwrap();
    let ines = app.sim.participant("ines").unwrap();
    let ada = app.sim.participant("ada").unwrap();
    let rune = app.sim.participant("rune").unwrap();
    let seeded = [mira, jonas, selma, theo, ines];

    for (index, user) in seeded.iter().enumerate() {
        app.join(user).expect("the join must send");
        app.send_file(
            user,
            &format!("seed-{index}.mod"),
            &mod_track(&format!("Seed {index}"), index as u8, 0),
            "my seed",
        )
        .expect("the seed must send");
    }
    app.join(rune).expect("the fill-in join must send");
    for user in seeded {
        app.wait_for_dm(user, "Monsquaz Swap pack", timeout())
            .await
            .expect("the pack DM must arrive");
    }
    app.wait_for_participant(&event, rune, timeout())
        .await
        .expect("the fill-in signup must exist");

    // A second seed replaces the first before the deadline.
    let first_seed = app.with_db(|db| {
        let track = swap_db::TrackRepo::list_for_event(db.conn(), &event)
            .unwrap()
            .into_iter()
            .find(|track| track.owner_id == mira)
            .expect("mira must own a track");
        swap_db::FileRepo::list_for_track(db.conn(), &event, &track.code)
            .unwrap()
            .first()
            .map(|file| file.sha256.clone())
            .expect("the seed must have a file")
    });
    let replacement = mod_track("Mira replacement", 7, 0);
    app.send_file(mira, "mira-2.mod", &replacement, "better seed")
        .expect("the replacement must send");
    app.wait_for_dm(mira, "Seed replaced", timeout())
        .await
        .expect("the replacement receipt must arrive");
    let replaced_seed = app.with_db(|db| {
        let track = swap_db::TrackRepo::list_for_event(db.conn(), &event)
            .unwrap()
            .into_iter()
            .find(|track| track.owner_id == mira)
            .unwrap();
        swap_db::FileRepo::list_for_track(db.conn(), &event, &track.code).unwrap()[0]
            .sha256
            .clone()
    });
    assert_ne!(first_seed, replaced_seed);

    // A late seed waits for the host.
    app.advance(8 * day());
    app.join(ada).expect("the late join must send");
    app.send_file(ada, "ada.mod", &mod_track("Late seed", 2, 0), "late seed")
        .expect("the late seed must send");
    app.wait_until("the late seed hold", timeout(), || {
        app.with_db(|db| {
            swap_engine::pending_late_seeds(db, &event)
                .map(|seeds| seeds.len() == 1)
                .unwrap_or(false)
        })
    })
    .await
    .expect("the late seed must wait for the host");
    let late_id = app.with_db(|db| {
        swap_engine::pending_late_seeds(db, &event).unwrap()[0]
            .id
            .clone()
    });
    app.with_db(|db| {
        swap_engine::accept_late_seed(db, &event, &late_id, app.now())
            .expect("the host must accept")
    });

    let lock = app.lock_roster(&event).expect("the roster must lock");
    assert_eq!(lock.active, 6);
    assert_eq!(lock.fill_ins, 1);

    let schedule = app
        .generate_schedule(&event)
        .expect("the plan must generate");
    assert_eq!(schedule.rounds, 3);
    assert!(
        app.verify_commitment(&event)
            .expect("the commitment must verify")
    );

    // Edit one assignment, then start on the new plan version.
    let edit = app.with_db(|db| {
        for assignment in &schedule.assignments {
            let candidates = swap_engine::plan_edit_candidates(
                db,
                &event,
                assignment.round,
                &assignment.track_code,
            )
            .unwrap_or_default();
            if let Some(candidate) = candidates.first() {
                return Some((
                    assignment.round,
                    assignment.track_code.clone(),
                    candidate.participant_id.clone(),
                ));
            }
        }
        None
    });
    let (edit_round, edit_track, edit_user) = edit.expect("a valid plan edit must exist");
    app.with_db(|db| {
        swap_engine::edit_plan_assignment(
            db,
            &event,
            edit_round,
            &edit_track,
            &edit_user,
            schedule.version,
            app.now(),
        )
        .expect("the host must edit the plan")
    });
    assert!(
        app.verify_commitment(&event)
            .expect("the edited commitment must verify")
    );
    app.start_event(&event).expect("the event must start");
    assert_eq!(
        app.with_db(|db| {
            swap_db::EventRepo::get(db.conn(), &event)
                .unwrap()
                .unwrap()
                .state
        }),
        EventState::Active
    );

    // The first exchange: five submit, one skips, one fill-in picks up.
    app.open_round(&event, 0)
        .expect("the first exchange must open");
    let mut skipped_track = None;
    for (index, user) in [mira, jonas, selma, theo, ines, ada].iter().enumerate() {
        app.claim(user).expect("the claim must send");
        let track = app
            .wait_for_assignment(&event, user, AssignmentStatus::Claimed, timeout())
            .await
            .expect("the claim must land");
        if index == 3 {
            app.skip(user).expect("the skip must send");
            app.wait_for_assignment(&event, user, AssignmentStatus::SkippedAfterClaim, timeout())
                .await
                .expect("the skip must land");
            skipped_track = Some(track);
        } else {
            let edited = mod_track(&format!("Edit {index}"), index as u8 + 2, 1);
            app.send_file(user, &format!("edit-{index}.mod"), &edited, "my edit")
                .expect("the edit must send");
            app.wait_for_assignment(&event, user, AssignmentStatus::Submitted, timeout())
                .await
                .expect("the edit must land");
        }
    }

    let skipped_track = skipped_track.expect("one track must be skipped");
    app.assign_fill_in(&event, &skipped_track, 2, rune)
        .expect("the host must assign the fill-in");
    app.claim(rune).expect("the fill-in must claim");
    app.wait_for_assignment(&event, rune, AssignmentStatus::Claimed, timeout())
        .await
        .expect("the fill-in must claim");
    app.send_file(
        rune,
        "rune-edit.mod",
        &mod_track("Rune edit", 4, 1),
        "the rescue",
    )
    .expect("the fill-in must submit");
    app.wait_for_assignment(&event, rune, AssignmentStatus::Submitted, timeout())
        .await
        .expect("the fill-in must submit");

    // Move the open deadline. The planned rounds and the reveal shift with it.
    let reveal_before = app.with_db(|db| {
        swap_db::EventRepo::reveal_at(db.conn(), &event)
            .unwrap()
            .unwrap()
    });
    app.move_round_deadline(&event, 2, app.now() + 5 * day(), "A short extension")
        .expect("the host must move the deadline");
    let reveal_after = app.with_db(|db| {
        swap_db::EventRepo::reveal_at(db.conn(), &event)
            .unwrap()
            .unwrap()
    });
    assert!(reveal_after > reveal_before);

    // Three more exchange rounds. Everyone submits.
    for round in 1..3 {
        app.open_round(&event, round).expect("the round must open");
        let participants = [mira, jonas, selma, theo, ines, ada];
        for user in participants {
            app.claim(user).expect("the claim must send");
            app.wait_for_assignment(&event, user, AssignmentStatus::Claimed, timeout())
                .await
                .expect("the claim must land");
            app.send_file(
                user,
                &format!("round-{round}-{user}.mod"),
                &mod_track("Another pass", 1, 1),
                "my edit",
            )
            .expect("the edit must send");
            app.wait_for_assignment(&event, user, AssignmentStatus::Submitted, timeout())
                .await
                .expect("the edit must land");
        }
    }

    let revealed = app.reveal(&event).expect("the host must reveal");
    assert_eq!(revealed.tracks.len(), 6);
    assert_eq!(
        app.with_db(|db| {
            swap_db::EventRepo::get(db.conn(), &event)
                .unwrap()
                .unwrap()
                .state
        }),
        EventState::Reveal
    );
    assert!(!revealed.effects.is_empty());

    let zip = app.reveal_zip(&event).expect("the ZIP must build");
    assert!(zip.starts_with(b"PK"));

    app.shutdown().await;
}
