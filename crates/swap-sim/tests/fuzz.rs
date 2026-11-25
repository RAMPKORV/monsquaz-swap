use swap_files::policy::detect_kind;
use swap_files::{FileKind, FilePolicy, scrub_metadata, wav_audio_data};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xBF58_476D_1CE4_E5B9);
        self.0
    }

    fn bytes(&mut self, length: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            bytes.extend_from_slice(&self.next().to_le_bytes());
        }
        bytes.truncate(length);
        bytes
    }
}

fn with_magic(rng: &mut Rng, extension: &str, length: usize) -> Vec<u8> {
    let mut bytes = rng.bytes(length.max(16));
    match extension {
        "it" | "mptm" => bytes[..4].copy_from_slice(b"IMPM"),
        "xm" => {
            bytes.resize(length.max(345), 0);
            bytes[..17].copy_from_slice(b"Extended Module: ");
        }
        "s3m" => {
            bytes.resize(length.max(256), 0);
            bytes[0x2C..0x30].copy_from_slice(b"SCRM");
        }
        "mod" => {
            bytes.resize(length.max(1084), 0);
            bytes[1080..1084].copy_from_slice(b"M.K.");
        }
        "mtm" => bytes[..3].copy_from_slice(b"MTM"),
        "669" => bytes[..2].copy_from_slice(b"if"),
        "zip" => bytes[..4].copy_from_slice(b"PK\x03\x04"),
        _ => {}
    }
    bytes
}

#[test]
fn random_containers_never_panic_the_policy_or_scrubber() {
    let policy = FilePolicy::default();
    let extensions = [
        "it", "mptm", "xm", "s3m", "mod", "mtm", "669", "wav", "flac", "mp3", "ogg", "zip", "exe",
    ];
    let mut rng = Rng(0xC0FF_EE00_1234_5678);
    for iteration in 0..2_000 {
        let extension = extensions[iteration % extensions.len()];
        let length = (rng.next() % 4_000) as usize;
        let bytes = with_magic(&mut rng, extension, length);
        let name = format!("fuzz.{extension}");
        if let Ok(validated) = policy.validate(&name, &bytes) {
            assert_eq!(validated.kind, detect_kind(&bytes, extension));
            let _ = scrub_metadata(&bytes, validated.kind);
        }
    }
}

#[test]
fn hostile_header_counts_are_bounded() {
    // IT: huge instrument/sample/pattern counts in a tiny file.
    let mut it = vec![0u8; 0xC1];
    it[..4].copy_from_slice(b"IMPM");
    it[0x20..0x22].copy_from_slice(&u16::MAX.to_le_bytes());
    it[0x22..0x24].copy_from_slice(&u16::MAX.to_le_bytes());
    it[0x24..0x26].copy_from_slice(&u16::MAX.to_le_bytes());
    it[0x26..0x28].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(scrub_metadata(&it, FileKind::It).is_err());

    // XM: huge instrument count with almost no body.
    let mut xm = vec![0u8; 345];
    xm[..17].copy_from_slice(b"Extended Module: ");
    xm[60..64].copy_from_slice(&276u32.to_le_bytes());
    xm[72..74].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(scrub_metadata(&xm, FileKind::Xm).is_err());

    // MTM: 255 samples in a 66-byte file.
    let mut mtm = vec![0u8; 0x42];
    mtm[..3].copy_from_slice(b"MTM");
    mtm[0x1E] = 255;
    assert!(scrub_metadata(&mtm, FileKind::Mtm).is_err());
}

#[test]
fn random_wav_chunks_never_panic() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF0);
    for _ in 0..2_000 {
        let length = (rng.next() % 2_048) as usize;
        let bytes = rng.bytes(length);
        let _ = wav_audio_data(&bytes);
    }
    let mut wav = b"RIFF\x24\x00\x00\x00WAVEdata\xff\xff\xff\x7f".to_vec();
    assert!(wav_audio_data(&wav).is_err());
    wav.extend_from_slice(&[0; 8]);
    assert!(wav_audio_data(&wav).is_err());
}
