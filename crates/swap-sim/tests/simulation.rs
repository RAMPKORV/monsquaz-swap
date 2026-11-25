use swap_discord::{DiscordError, FakeDiscord};
use swap_engine::dispatch;
use swap_sim::{Profile, SimulationConfig, SimulationReport, run};

fn assert_clean(report: &SimulationReport) {
    assert!(
        report.all_passed(),
        "failed checks: {:#?}",
        report
            .checks
            .iter()
            .filter(|check| !check.passed)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_mixed_classic_event_passes_and_reveals() {
    let report = run(&SimulationConfig::new(8)).unwrap();
    assert_clean(&report);
    assert_eq!(report.rounds, 7);
    assert_eq!(report.assignments, 56);
    assert_eq!(report.revealed_tracks, 8);
    assert!(report.seen_pairs > 0);
    assert!(report.effects.offers == 56);
    assert!(report.effects.files > 0);
}

#[test]
fn expired_only_offers_create_no_seen_pairs() {
    let mut config = SimulationConfig::new(3);
    config.profiles = vec![Profile::Expires];
    let report = run(&config).unwrap();
    assert_clean(&report);
    assert_eq!(report.seen_pairs, 0);
    assert_eq!(report.file_versions, 3);
    assert_eq!(report.effects.files, 0);
    assert_eq!(report.revealed_tracks, 3);
}

#[test]
fn a_full_reliable_event_keeps_every_version() {
    let mut config = SimulationConfig::new(8);
    config.profiles = vec![Profile::Reliable];
    let report = run(&config).unwrap();
    assert_clean(&report);
    assert_eq!(report.seen_pairs, 56);
    assert_eq!(report.file_versions, 64);
    assert_eq!(report.effects.files, 56);
}

#[test]
fn a_dense_event_stays_within_the_classic_cap() {
    let mut config = SimulationConfig::new(24);
    config.rounds = Some(8);
    let report = run(&config).unwrap();
    assert_clean(&report);
    assert_eq!(report.rounds, 8);
    assert_eq!(report.assignments, 192);
    assert_eq!(report.tracks, 24);
}

#[test]
fn seeds_do_not_leak_names_or_versions() {
    let report = run(&SimulationConfig::new(6)).unwrap();
    assert_clean(&report);
    let check = report
        .checks
        .iter()
        .find(|check| check.name == "anonymity.opaque_outbound")
        .unwrap();
    assert!(check.passed);
}

#[test]
fn invalid_configs_are_refused() {
    let mut one = SimulationConfig::new(1);
    assert!(run(&one).is_err());
    one.participants = 4;
    one.rounds = Some(0);
    assert!(run(&one).is_err());
}

#[test]
fn several_deterministic_seeds_stay_valid() {
    for participants in 2..=10 {
        for seed in [1u64, 7, 42, 9_999] {
            let mut config = SimulationConfig::new(participants);
            config.seed = seed;
            let report = run(&config).unwrap();
            assert_clean(&report);
            assert_eq!(report.tracks, participants);
        }
    }
}

#[tokio::test]
async fn every_effect_reaches_the_fake_transport() {
    let report = run(&SimulationConfig::new(5)).unwrap();
    let fake = FakeDiscord::new();
    let results = dispatch(&fake, &report.effect_log).await;
    assert_eq!(results.len(), report.effects.total());
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(fake.sent_files().len(), report.effects.files);
}

#[tokio::test]
async fn a_transport_failure_is_surfaced_and_does_not_stop_the_batch() {
    let report = run(&SimulationConfig::new(5)).unwrap();
    let fake = FakeDiscord::new();
    fake.fail_next_dm(DiscordError::RateLimited);
    let results = dispatch(&fake, &report.effect_log).await;
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(DiscordError::RateLimited)))
    );
    assert!(results.len() > 1);
    assert_eq!(fake.sent_files().len(), report.effects.files);
}
