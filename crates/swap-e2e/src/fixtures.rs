//! Deterministic audio fixtures for end-to-end tests.
//!
//! The fixtures are generated, not committed. Each fixture is a real file that
//! openmpt123 can render, so the A/B check runs the same code path as a
//! participant upload.

use std::io::{Cursor, Write};

/// The ProTracker period table for one octave, starting at C-3.
pub const MOD_PERIODS: [u16; 12] = [428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226];

/// Build a valid four-channel ProTracker module with one looping sample.
///
/// `title` fills the song title. `note` selects a semitone from 0 to 11.
/// `octave` shifts the note. The module has one pattern and one sample.
pub fn mod_track(title: &str, note: u8, octave: i8) -> Vec<u8> {
    let sample = sine_sample(128);
    let mut bytes = Vec::with_capacity(2236);
    push_padded(&mut bytes, title.as_bytes(), 20);
    push_sample_header(&mut bytes, "seed", sample.len());
    for _ in 1..31 {
        push_sample_header(&mut bytes, "", 0);
    }
    bytes.push(1); // one order
    bytes.push(0); // restart position
    let mut orders = [0u8; 128];
    orders[0] = 0;
    bytes.extend_from_slice(&orders);
    bytes.extend_from_slice(b"M.K.");

    let mut pattern = vec![0u8; 64 * 4 * 4];
    let period = shifted_period(note, octave);
    pattern[0] = (period >> 8) as u8 & 0x0f;
    pattern[1] = period as u8;
    pattern[2] = 0x10; // sample 1, effect 0
    bytes.extend_from_slice(&pattern);
    bytes.extend_from_slice(&sample);
    bytes
}

/// Build a ZIP pack with the given file names and bytes.
pub fn pack_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

/// Build a valid 16-bit PCM mono WAV file.
pub fn wav_tone(seconds: u32, hertz: u32) -> Vec<u8> {
    let sample_rate = 44_100u32;
    let frames = sample_rate.saturating_mul(seconds);
    let data_len = frames.saturating_mul(2);
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        let phase = (frame as f64 * hertz as f64 / sample_rate as f64) * std::f64::consts::TAU;
        let value = (phase.sin() * 12_000.0) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Bytes with a `.mod` name that the file policy must refuse.
pub fn invalid_module() -> Vec<u8> {
    b"this is not a tracker module".to_vec()
}

fn push_padded(bytes: &mut Vec<u8>, value: &[u8], width: usize) {
    let take = value.len().min(width);
    bytes.extend_from_slice(&value[..take]);
    bytes.resize(bytes.len() + (width - take), 0);
}

fn push_sample_header(bytes: &mut Vec<u8>, name: &str, sample_len: usize) {
    push_padded(bytes, name.as_bytes(), 22);
    let words = (sample_len / 2) as u16;
    bytes.extend_from_slice(&words.to_be_bytes());
    bytes.push(0); // finetune
    bytes.push(64); // volume
    bytes.extend_from_slice(&0u16.to_be_bytes()); // loop start
    bytes.extend_from_slice(&words.to_be_bytes()); // loop length
}

fn shifted_period(note: u8, octave: i8) -> u16 {
    let base = MOD_PERIODS[usize::from(note.min(11))];
    match octave {
        octave if octave < 0 => (base >> u32::from(octave.unsigned_abs())).max(1),
        octave if octave > 0 => base.saturating_mul(1 << u32::from(octave as u8)),
        _ => base,
    }
}

fn sine_sample(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            let phase = index as f64 / length as f64 * std::f64::consts::TAU;
            ((phase.sin() * 96.0) as i8) as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use swap_files::policy::detect_kind;
    use swap_files::{FilePolicy, scrub_metadata};

    use super::*;

    #[test]
    fn the_policy_accepts_a_generated_module() {
        let bytes = mod_track("Night drive", 0, 0);
        let policy = FilePolicy::default();
        let validated = policy.validate("seed.mod", &bytes).unwrap();
        assert_eq!(validated.kind, swap_files::FileKind::Mod);
        assert!(validated.size > 2000);
        assert_eq!(detect_kind(&bytes, "mod"), swap_files::FileKind::Mod);
    }

    #[test]
    fn the_scrub_removes_the_title_and_keeps_the_module_valid() {
        let bytes = mod_track("A secret title", 4, 1);
        let scrubbed = scrub_metadata(&bytes, swap_files::FileKind::Mod).unwrap();
        assert!(scrubbed.summary.song_name);
        assert_eq!(&scrubbed.bytes[..20], &[0u8; 20]);
        assert_eq!(
            detect_kind(&scrubbed.bytes, "mod"),
            swap_files::FileKind::Mod
        );
    }

    #[test]
    fn the_policy_refuses_text_with_a_module_name() {
        let policy = FilePolicy::default();
        assert!(policy.validate("fake.mod", &invalid_module()).is_err());
    }

    #[test]
    fn the_pack_is_a_valid_zip() {
        let first = mod_track("First", 0, 0);
        let second = mod_track("Second", 7, 0);
        let pack = pack_zip(&[("01-first.mod", &first), ("02-second.mod", &second)]);
        let policy = FilePolicy::default();
        let validated = policy.validate("pack.zip", &pack).unwrap();
        assert_eq!(validated.kind, swap_files::FileKind::Zip);
    }

    #[test]
    fn the_wav_has_a_riff_header() {
        let bytes = wav_tone(1, 440);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 44_100 * 2);
    }

    #[test]
    fn notes_and_octaves_change_the_period() {
        assert_eq!(shifted_period(0, 0), 428);
        assert_eq!(shifted_period(0, 1), 856);
        assert_eq!(shifted_period(0, -1), 214);
        assert_eq!(shifted_period(11, 0), 226);
    }

    #[test]
    fn openmpt_renders_the_same_audio_after_the_scrub() {
        let renderer = swap_files::OpenMptRenderer::discover().expect("openmpt123 is required");
        let bytes = mod_track("A secret title", 2, 0);
        let prepared =
            swap_files::prepare_module(&renderer, &bytes, swap_files::FileKind::Mod, "mod", 2)
                .unwrap();
        assert!(prepared.audio_matches);
        assert!(prepared.summary.changed());
        assert_eq!(&prepared.bytes[..20], &[0u8; 20]);
    }
}
