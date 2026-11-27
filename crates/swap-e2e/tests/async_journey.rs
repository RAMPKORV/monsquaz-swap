//! The full async journey: intake, two waves, expiry, pause, resume, and reveal.

use std::time::Duration;

use swap_core::{AssignmentStatus, EventState, Readiness};
use swap_e2e::{BASE_TIME, HeadlessApp, mod_track, pack_zip};

fn timeout() -> Duration {
    Duration::from_secs(20)
}

fn hour() -> i64 {
    60 * 60
}

fn day() -> i64 {
    24 * hour()
}

fn next_midnight() -> i64 {
    (BASE_TIME / day() + 1) * day()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_full_async_journey_reaches_a_reveal() {
    let app = HeadlessApp::start().await.expect("the app must start");
    let event = app
        .create_event_with("Async all the way", app.async_setting())
        .expect("the async event must be created");
    let pack = pack_zip(&[
        ("01-pack.mod", &mod_track("Pack one", 0, 0)),
        ("02-pack.mod", &mod_track("Pack two", 4, 0)),
    ]);
    app.upload_pack(&event, "pack.zip", &pack)
        .expect("the pack must upload");
    app.open_seed_round(&event, BASE_TIME + 2 * day())
        .expect("Round 1 must open");

    let mira = app.sim.participant("mira").unwrap();
    let jonas = app.sim.participant("jonas").unwrap();
    let selma = app.sim.participant("selma").unwrap();
    let theo = app.sim.participant("theo").unwrap();
    let people = [mira, jonas, selma, theo];

    for (index, user) in people.iter().enumerate() {
        app.join(user).expect("the join must send");
        app.send_file(
            user,
            &format!("seed-{index}.mod"),
            &mod_track(&format!("Async seed {index}"), index as u8, 0),
            "my seed",
        )
        .expect("the seed must send");
    }
    for user in people {
        app.wait_for_dm(user, "Monsquaz Swap pack", timeout())
            .await
            .expect("the pack DM must arrive");
    }
    let lock = app.lock_roster(&event).expect("the roster must lock");
    assert_eq!(lock.active, 4);
    assert_eq!(lock.fill_ins, 0);
    app.start_async_event(&event)
        .expect("the async event must start");
    assert_eq!(
        app.with_db(|db| {
            swap_db::EventRepo::get(db.conn(), &event)
                .unwrap()
                .unwrap()
                .state
        }),
        EventState::Active
    );

    // Wave one of the first day.
    let first_wave = next_midnight() + 30 * 60;
    app.advance(first_wave - app.now());
    let offers = app
        .dispatch_async_release(&event)
        .expect("the release must dispatch");
    assert_eq!(offers, 4);
    for user in people {
        app.wait_for_dm(user, "is available", timeout())
            .await
            .expect("the offer DM must arrive");
    }

    // One claim and submit, the others expire.
    app.claim(mira).expect("the claim must send");
    app.wait_for_assignment(&event, mira, AssignmentStatus::Claimed, timeout())
        .await
        .expect("the claim must land");
    app.send_file(
        mira,
        "mira-edit.mod",
        &mod_track("Mira edit", 5, 0),
        "my edit",
    )
    .expect("the edit must send");
    app.wait_for_assignment(&event, mira, AssignmentStatus::Submitted, timeout())
        .await
        .expect("the edit must land");

    app.advance(day());
    for user in [jonas, selma, theo] {
        app.wait_for_dm(user, "Your offer expired", timeout())
            .await
            .expect("the expiry DM must arrive");
    }
    let coverage = app.async_coverage(&event).expect("coverage must build");
    assert_eq!(coverage.participants, 4);
    assert_eq!(coverage.tracks, 4);
    assert!(coverage.claimed_handoffs >= 1);
    assert!(coverage.gaps.iter().any(|gap| gap.reason == "expired"));

    // The next day: the cooldown is over, so the queue offers again.
    app.advance(day());
    let second_wave = app
        .dispatch_async_release(&event)
        .expect("the second day must dispatch");
    assert_eq!(second_wave, 4);
    for user in people {
        app.wait_for_dm(user, "is available", timeout())
            .await
            .expect("the second offer DM must arrive");
    }
    app.claim(mira).expect("the second claim must send");
    app.wait_for_assignment(&event, mira, AssignmentStatus::Claimed, timeout())
        .await
        .expect("the second claim must land");
    app.send_file(
        mira,
        "mira-second.mod",
        &mod_track("Mira second", 8, 0),
        "my second edit",
    )
    .expect("the second edit must send");
    app.wait_for_assignment(&event, mira, AssignmentStatus::Submitted, timeout())
        .await
        .expect("the second edit must land");

    // A second expiry pauses the participant.
    app.advance(day());
    for user in [jonas, selma, theo] {
        app.wait_for_dm(user, "A second offer expired", timeout())
            .await
            .expect("the pause DM must arrive");
    }
    let readiness = app.with_db(|db| {
        swap_db::ParticipantRepo::get(db.conn(), &event, jonas)
            .unwrap()
            .unwrap()
            .readiness
    });
    assert_eq!(readiness, Readiness::Paused);

    // The participant resumes and the queue offers again.
    app.sim
        .command(jonas, "resume", Default::default())
        .expect("the resume must send");
    app.sim
        .command(mira, "pause", Default::default())
        .expect("the pause must send");
    app.wait_until("the resumed participant", timeout(), || {
        app.with_db(|db| {
            swap_db::ParticipantRepo::get(db.conn(), &event, jonas)
                .unwrap()
                .unwrap()
                .readiness
                == Readiness::Ready
        })
    })
    .await
    .expect("the participant must be ready");
    app.wait_until("the paused participant", timeout(), || {
        app.with_db(|db| {
            swap_db::ParticipantRepo::get(db.conn(), &event, mira)
                .unwrap()
                .unwrap()
                .readiness
                == Readiness::Paused
        })
    })
    .await
    .expect("the participant must be paused");

    app.advance(day());
    let resumed_wave = app
        .dispatch_async_wave(&event)
        .expect("the queue must offer again");
    assert!(resumed_wave >= 1);
    app.wait_for_dm(jonas, "is available", timeout())
        .await
        .expect("the resumed offer must arrive");
    app.claim(jonas).expect("the resumed claim must send");
    app.wait_for_assignment(&event, jonas, AssignmentStatus::Claimed, timeout())
        .await
        .expect("the resumed claim must land");
    app.send_file(
        jonas,
        "jonas-again.mod",
        &mod_track("Jonas again", 7, 1),
        "my second edit",
    )
    .expect("the second edit must send");
    app.wait_for_assignment(&event, jonas, AssignmentStatus::Submitted, timeout())
        .await
        .expect("the second edit must land");

    let revealed = app.reveal(&event).expect("the host must reveal");
    assert_eq!(revealed.tracks.len(), 4);
    assert_eq!(
        app.with_db(|db| {
            swap_db::EventRepo::get(db.conn(), &event)
                .unwrap()
                .unwrap()
                .state
        }),
        EventState::Reveal
    );
    let zip = app.reveal_zip(&event).expect("the ZIP must build");
    assert!(zip.starts_with(b"PK"));

    app.shutdown().await;
}
