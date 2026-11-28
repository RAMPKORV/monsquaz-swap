use std::process::Command;

use swap_files::{
    FileKind, OpenMptRenderer, compare_audio, prepare_module, render_preview, scrub_metadata,
    wav_audio_data,
};

fn mod_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Leaky MOD Title");
    bytes.resize(20, 0);
    for index in 0..31 {
        let name = format!("Sample {index} secret");
        bytes.extend_from_slice(name.as_bytes());
        bytes.resize(20 + (index + 1) * 30, 0);
    }
    bytes.push(1);
    bytes.push(0);
    bytes.resize(20 + 31 * 30 + 2 + 128, 0);
    bytes[20 + 31 * 30 + 2] = 0;
    bytes.extend_from_slice(b"M.K.");
    bytes.resize(1084 + 1024, 0);
    bytes
}

fn xm_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Extended Module: ");
    bytes.extend_from_slice(b"Leaky XM Title");
    bytes.resize(37, 0);
    bytes.push(0x1A);
    bytes.extend_from_slice(b"SecretTracker");
    bytes.resize(58, 0);
    bytes.extend_from_slice(&0x0104u16.to_le_bytes());
    bytes.extend_from_slice(&276u32.to_le_bytes());
    for value in [1u16, 0, 1, 1, 1, 0, 6, 125] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(336, 0);
    bytes.extend_from_slice(&9u32.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&64u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());

    bytes.extend_from_slice(&73u32.to_le_bytes());
    bytes.extend_from_slice(b"Instrument Secret");
    bytes.resize(345 + 4 + 22, 0);
    bytes.push(0);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&[64, 0, 0, 128, 0, 0]);
    bytes.extend_from_slice(b"Sample Secret");
    bytes.resize(345 + 73, 0);
    bytes
}

fn it_fixture() -> Vec<u8> {
    let mut bytes = vec![0u8; 0xC0];
    bytes[..4].copy_from_slice(b"IMPM");
    bytes[4..17].copy_from_slice(b"Leaky IT Song");
    bytes[0x20..0x22].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x24..0x26].copy_from_slice(&0u16.to_le_bytes());
    bytes[0x26..0x28].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x30] = 128;
    bytes[0x31] = 48;
    bytes[0x32] = 6;
    bytes[0x33] = 125;
    for index in 0..32 {
        bytes[0x40 + index * 2] = 32;
    }
    bytes[0x80..0xC0].fill(64);
    bytes.push(0);
    let pattern_offset = bytes.len() as u32 + 4;
    bytes.extend_from_slice(&pattern_offset.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 64]);
    bytes
}

fn s3m_fixture() -> Vec<u8> {
    let mut bytes = vec![0u8; 96];
    bytes[..14].copy_from_slice(b"Leaky S3M Song");
    bytes[0x1C] = 0x1A;
    bytes[0x1D] = 0x10;
    bytes[0x20..0x22].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x22..0x24].copy_from_slice(&0u16.to_le_bytes());
    bytes[0x24..0x26].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x28..0x2A].copy_from_slice(&0x1320u16.to_le_bytes());
    bytes[0x2A..0x2C].copy_from_slice(&2u16.to_le_bytes());
    bytes[0x2C..0x30].copy_from_slice(b"SCRM");
    bytes[0x30] = 64;
    bytes[0x31] = 6;
    bytes[0x32] = 125;
    bytes.push(0);
    bytes.extend_from_slice(&(112u16 / 16).to_le_bytes());
    bytes.resize(112, 0);
    bytes.extend_from_slice(&66u16.to_le_bytes());
    bytes.extend_from_slice(&[0x20u8; 64]);
    bytes
}

fn mtm_fixture() -> Vec<u8> {
    let mut bytes = vec![0u8; 0x42 + 37 + 128 + 64 + 10];
    bytes[..3].copy_from_slice(b"MTM");
    bytes[3] = 0x10;
    bytes[4..18].copy_from_slice(b"Leaky MTM Song");
    bytes[0x1C..0x1E].copy_from_slice(&10u16.to_le_bytes());
    bytes[0x1E] = 1;
    bytes[0x42..0x57].copy_from_slice(b"Secret MTM Sample....");
    bytes[0x42 + 37 + 128 + 64..].copy_from_slice(b"secret com");
    bytes
}

fn openmpt_available() -> bool {
    Command::new("openmpt123")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn probe(bytes: &[u8], extension: &str) -> bool {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("probe.{extension}"));
    std::fs::write(&path, bytes).unwrap();
    Command::new("openmpt123")
        .arg("--probe")
        .arg(&path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn the_mod_scrub_removes_every_name() {
    let original = mod_fixture();
    let scrubbed = scrub_metadata(&original, FileKind::Mod).unwrap();
    assert!(scrubbed.summary.song_name);
    assert_eq!(scrubbed.summary.sample_names, 31);
    assert!(scrubbed.bytes[..20].iter().all(|byte| *byte == 0));
    for index in 0..31 {
        let start = 20 + index * 30;
        assert!(
            scrubbed.bytes[start..start + 22]
                .iter()
                .all(|byte| *byte == 0)
        );
    }
    assert_eq!(&scrubbed.bytes[1080..1084], b"M.K.");
}

#[test]
fn the_xm_scrub_removes_song_tracker_instrument_and_sample_names() {
    let original = xm_fixture();
    let scrubbed = scrub_metadata(&original, FileKind::Xm).unwrap();
    assert!(scrubbed.summary.song_name);
    assert!(scrubbed.summary.tracker_name);
    assert_eq!(scrubbed.summary.instrument_names, 1);
    assert_eq!(scrubbed.summary.sample_names, 1);
    assert!(scrubbed.bytes[17..37].iter().all(|byte| *byte == 0));
    assert!(scrubbed.bytes[38..58].iter().all(|byte| *byte == 0));
    assert!(
        scrubbed.bytes[345 + 4..345 + 26]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert!(
        scrubbed.bytes[345 + 33 + 18..345 + 33 + 40]
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn the_it_and_s3m_song_names_are_removed() {
    let it = scrub_metadata(&it_fixture(), FileKind::It).unwrap();
    assert!(it.summary.song_name);
    assert!(it.bytes[4..30].iter().all(|byte| *byte == 0));

    let s3m = scrub_metadata(&s3m_fixture(), FileKind::S3m).unwrap();
    assert!(s3m.summary.song_name);
    assert!(s3m.bytes[..28].iter().all(|byte| *byte == 0));

    let mptm = scrub_metadata(&it_fixture(), FileKind::Mptm).unwrap();
    assert!(mptm.summary.song_name);
    assert!(mptm.bytes[4..30].iter().all(|byte| *byte == 0));
}

#[test]
fn the_fixtures_are_valid_modules_before_and_after_the_scrub() {
    if !openmpt_available() {
        return;
    }
    for (bytes, kind, extension) in [
        (mod_fixture(), FileKind::Mod, "mod"),
        (xm_fixture(), FileKind::Xm, "xm"),
        (it_fixture(), FileKind::It, "it"),
        (s3m_fixture(), FileKind::S3m, "s3m"),
        (mtm_fixture(), FileKind::Mtm, "mtm"),
    ] {
        assert!(probe(&bytes, extension), "fixture must load before scrub");
        let scrubbed = scrub_metadata(&bytes, kind).unwrap();
        assert!(
            probe(&scrubbed.bytes, extension),
            "scrubbed fixture must load"
        );
    }
}

// scenario: Metadata is scrubbed and checked
#[test]
fn a_real_render_accepts_the_scrub_and_changes_only_metadata() {
    if !openmpt_available() {
        return;
    }
    let renderer = OpenMptRenderer::default();
    let original = mod_fixture();
    let prepared = prepare_module(&renderer, &original, FileKind::Mod, "mod", 2).unwrap();
    assert!(prepared.audio_matches);
    assert!(prepared.summary.changed());
    assert_ne!(prepared.sha256, prepared.original_sha256);

    let ab = compare_audio(&renderer, &original, &prepared.bytes, "mod", 2).unwrap();
    assert!(ab.audio_matches);
    assert_eq!(ab.original_audio_sha256, ab.scrubbed_audio_sha256);

    let preview = render_preview(&renderer, &prepared.bytes, "mod", 2).unwrap();
    assert!(!wav_audio_data(&preview).unwrap().is_empty());
}

#[test]
fn the_mtm_render_also_matches_after_the_scrub() {
    if !openmpt_available() {
        return;
    }
    let renderer = OpenMptRenderer::default();
    let original = mtm_fixture();
    let scrubbed = scrub_metadata(&original, FileKind::Mtm).unwrap();
    let ab = compare_audio(&renderer, &original, &scrubbed.bytes, "mtm", 2).unwrap();
    assert!(ab.audio_matches);
}

#[test]
fn the_xm_render_also_matches_after_the_scrub() {
    if !openmpt_available() {
        return;
    }
    let renderer = OpenMptRenderer::default();
    let original = xm_fixture();
    let scrubbed = scrub_metadata(&original, FileKind::Xm).unwrap();
    let ab = compare_audio(&renderer, &original, &scrubbed.bytes, "xm", 2).unwrap();
    assert!(ab.audio_matches);
}
