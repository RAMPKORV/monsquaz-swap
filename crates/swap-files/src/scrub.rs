//! Module-aware metadata scrub.
//!
//! The scrub removes identity-bearing text from the module container and
//! leaves the audio data untouched. It is deliberately conservative: a header
//! that does not match the expected layout is refused, not guessed.

use thiserror::Error;

use crate::policy::{FileKind, detect_kind};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScrubError {
    #[error("this file type has no module metadata scrub: {0:?}")]
    Unsupported(FileKind),
    #[error("the module header is truncated or malformed")]
    Malformed,
    #[error("a metadata range is outside the file")]
    OutOfBounds,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrubSummary {
    pub song_name: bool,
    pub tracker_name: bool,
    pub message_bytes: usize,
    pub instrument_names: u32,
    pub sample_names: u32,
    pub file_names: u32,
}

impl ScrubSummary {
    pub fn changed(&self) -> bool {
        self.song_name
            || self.tracker_name
            || self.message_bytes > 0
            || self.instrument_names > 0
            || self.sample_names > 0
            || self.file_names > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrubbed {
    pub bytes: Vec<u8>,
    pub summary: ScrubSummary,
}

pub fn scrub_metadata(bytes: &[u8], kind: FileKind) -> Result<Scrubbed, ScrubError> {
    let mut output = bytes.to_vec();
    let summary = match kind {
        FileKind::It | FileKind::Mptm => scrub_it(&mut output)?,
        FileKind::Xm => scrub_xm(&mut output)?,
        FileKind::S3m => scrub_s3m(&mut output)?,
        FileKind::Mtm => scrub_mtm(&mut output)?,
        FileKind::SixSixNine => scrub_669(&mut output)?,
        FileKind::Mod => scrub_mod(&mut output)?,
        other => return Err(ScrubError::Unsupported(other)),
    };
    Ok(Scrubbed {
        bytes: output,
        summary,
    })
}

fn scrub_it(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 0xC0 || &bytes[..4] != b"IMPM" {
        return Err(ScrubError::Malformed);
    }
    let ordnum = read_u16(bytes, 0x20)? as usize;
    let insnum = read_u16(bytes, 0x22)? as usize;
    let smpnum = read_u16(bytes, 0x24)? as usize;
    let patnum = read_u16(bytes, 0x26)? as usize;
    let song_name = zero(bytes, 4, 26)?;
    let mut summary = ScrubSummary {
        song_name,
        ..ScrubSummary::default()
    };

    let message_length = read_u16(bytes, 0x36)? as usize;
    let message_offset = read_u32(bytes, 0x38)? as usize;
    if message_length > 0 && message_offset > 0 && zero(bytes, message_offset, message_length)? {
        summary.message_bytes = message_length;
    }

    let mut cursor = 0xC0;
    cursor = checked_add(cursor, ordnum)?;
    let instrument_table = cursor;
    cursor = checked_add(cursor, checked_mul(insnum, 4)?)?;
    let sample_table = cursor;
    cursor = checked_add(cursor, checked_mul(smpnum, 4)?)?;
    let pattern_table = cursor;
    cursor = checked_add(cursor, checked_mul(patnum, 4)?)?;
    if cursor > bytes.len() {
        return Err(ScrubError::OutOfBounds);
    }

    for index in 0..insnum {
        let offset = read_u32(bytes, instrument_table + index * 4)? as usize;
        if offset == 0 {
            continue;
        }
        if offset + 0x3A > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if &bytes[offset..offset + 4] != b"IMPI" {
            return Err(ScrubError::Malformed);
        }
        if zero(bytes, offset + 4, 12)? {
            summary.file_names += 1;
        }
        if zero(bytes, offset + 0x20, 26)? {
            summary.instrument_names += 1;
        }
    }

    for index in 0..smpnum {
        let offset = read_u32(bytes, sample_table + index * 4)? as usize;
        if offset == 0 {
            continue;
        }
        if offset + 0x3A > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if &bytes[offset..offset + 4] != b"IMPS" {
            return Err(ScrubError::Malformed);
        }
        if zero(bytes, offset + 4, 12)? {
            summary.file_names += 1;
        }
        if zero(bytes, offset + 0x20, 26)? {
            summary.sample_names += 1;
        }
    }

    let _ = pattern_table;
    Ok(summary)
}

fn scrub_xm(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 80 || &bytes[..17] != b"Extended Module: " {
        return Err(ScrubError::Malformed);
    }
    let song_name = zero(bytes, 17, 20)?;
    let tracker_name = zero(bytes, 38, 20)?;
    let mut summary = ScrubSummary {
        song_name,
        tracker_name,
        ..ScrubSummary::default()
    };

    let header_size = read_u32(bytes, 60)? as usize;
    let patterns = read_u16(bytes, 70)? as usize;
    let instruments = read_u16(bytes, 72)? as usize;
    let data_start = checked_add(60, header_size)?;
    if data_start > bytes.len() {
        return Err(ScrubError::OutOfBounds);
    }

    let mut cursor = data_start;
    for _ in 0..patterns {
        let length = read_u32(bytes, cursor)? as usize;
        if length < 9 {
            return Err(ScrubError::Malformed);
        }
        let packed = read_u16(bytes, cursor + 7)? as usize;
        cursor = checked_add(cursor, checked_add(9, packed)?)?;
        if cursor > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
    }

    for _ in 0..instruments {
        if cursor + 34 > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        let size = read_u32(bytes, cursor)? as usize;
        if size < 29 {
            return Err(ScrubError::Malformed);
        }
        let end = checked_add(cursor, size)?;
        if end > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if zero(bytes, cursor + 4, 22)? {
            summary.instrument_names += 1;
        }
        let samples = read_u16(bytes, cursor + 27)? as usize;
        if samples > 0 {
            if size < 33 {
                return Err(ScrubError::Malformed);
            }
            let sample_header_size = read_u32(bytes, cursor + 29)? as usize;
            if sample_header_size < 40 {
                return Err(ScrubError::Malformed);
            }
            let sample_start = cursor + 33;
            for index in 0..samples {
                let header = checked_add(sample_start, checked_mul(index, sample_header_size)?)?;
                if header + 40 > end {
                    return Err(ScrubError::OutOfBounds);
                }
                if zero(bytes, header + 18, 22)? {
                    summary.sample_names += 1;
                }
            }
        }
        cursor = end;
    }

    Ok(summary)
}

fn scrub_s3m(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 0x60 || &bytes[0x2C..0x30] != b"SCRM" {
        return Err(ScrubError::Malformed);
    }
    let ordnum = read_u16(bytes, 0x20)? as usize;
    let insnum = read_u16(bytes, 0x22)? as usize;
    let patnum = read_u16(bytes, 0x24)? as usize;
    let song_name = zero(bytes, 0, 28)?;
    let mut summary = ScrubSummary {
        song_name,
        ..ScrubSummary::default()
    };

    let instrument_table = checked_add(0x60, ordnum)?;
    let pattern_table = checked_add(instrument_table, checked_mul(insnum, 2)?)?;
    let end = checked_add(pattern_table, checked_mul(patnum, 2)?)?;
    if end > bytes.len() {
        return Err(ScrubError::OutOfBounds);
    }

    for index in 0..insnum {
        let paragraph = read_u16(bytes, instrument_table + index * 2)? as usize;
        if paragraph == 0 {
            continue;
        }
        let offset = checked_mul(paragraph, 16)?;
        if offset + 0x4C > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if zero(bytes, offset + 1, 12)? {
            summary.file_names += 1;
        }
        if zero(bytes, offset + 0x30, 28)? {
            summary.sample_names += 1;
        }
    }

    Ok(summary)
}

fn scrub_mtm(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 0x42 || &bytes[..3] != b"MTM" {
        return Err(ScrubError::Malformed);
    }
    let samples = bytes[0x1E] as usize;
    let tracks = read_u16(bytes, 0x18)? as usize;
    let patterns = bytes[0x1A] as usize + 1;
    let comment_length = read_u16(bytes, 0x1C)? as usize;
    let song_name = zero(bytes, 4, 20)?;
    let mut summary = ScrubSummary {
        song_name,
        ..ScrubSummary::default()
    };

    let sample_start = 0x42;
    let order_start = checked_add(sample_start, checked_mul(samples, 37)?)?;
    let tracks_start = checked_add(order_start, 128)?;
    let sequence_start = checked_add(tracks_start, checked_mul(tracks, 192)?)?;
    let comment_start = checked_add(sequence_start, checked_mul(patterns, 64)?)?;
    if comment_start > bytes.len() {
        return Err(ScrubError::OutOfBounds);
    }
    for index in 0..samples {
        let offset = checked_add(sample_start, checked_mul(index, 37)?)?;
        if offset + 37 > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if zero(bytes, offset, 22)? {
            summary.sample_names += 1;
        }
    }
    if comment_length > 0 && zero(bytes, comment_start, comment_length)? {
        summary.message_bytes = comment_length;
    }
    Ok(summary)
}

fn scrub_669(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 0x1F1 || (&bytes[..2] != b"if" && &bytes[..2] != b"JN") {
        return Err(ScrubError::Malformed);
    }
    let samples = bytes[0x6E] as usize;
    let song_name = zero(bytes, 2, 108)?;
    let mut summary = ScrubSummary {
        message_bytes: if song_name { 108 } else { 0 },
        ..ScrubSummary::default()
    };
    let sample_start = 0x1F1;
    for index in 0..samples {
        let offset = checked_add(sample_start, checked_mul(index, 25)?)?;
        if offset + 25 > bytes.len() {
            return Err(ScrubError::OutOfBounds);
        }
        if zero(bytes, offset, 13)? {
            summary.sample_names += 1;
        }
    }
    Ok(summary)
}

fn scrub_mod(bytes: &mut [u8]) -> Result<ScrubSummary, ScrubError> {
    if bytes.len() < 1084 || detect_kind(bytes, "mod") != FileKind::Mod {
        return Err(ScrubError::Malformed);
    }
    let song_name = zero(bytes, 0, 20)?;
    let mut summary = ScrubSummary {
        song_name,
        ..ScrubSummary::default()
    };
    for index in 0..31 {
        if zero(bytes, 20 + index * 30, 22)? {
            summary.sample_names += 1;
        }
    }
    Ok(summary)
}

fn zero(bytes: &mut [u8], offset: usize, length: usize) -> Result<bool, ScrubError> {
    let end = offset.checked_add(length).ok_or(ScrubError::OutOfBounds)?;
    if end > bytes.len() {
        return Err(ScrubError::OutOfBounds);
    }
    let changed = bytes[offset..end].iter().any(|byte| *byte != 0);
    bytes[offset..end].fill(0);
    Ok(changed)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ScrubError> {
    let end = offset.checked_add(2).ok_or(ScrubError::OutOfBounds)?;
    let slice = bytes.get(offset..end).ok_or(ScrubError::OutOfBounds)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ScrubError> {
    let end = offset.checked_add(4).ok_or(ScrubError::OutOfBounds)?;
    let slice = bytes.get(offset..end).ok_or(ScrubError::OutOfBounds)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn checked_add(left: usize, right: usize) -> Result<usize, ScrubError> {
    left.checked_add(right).ok_or(ScrubError::OutOfBounds)
}

fn checked_mul(left: usize, right: usize) -> Result<usize, ScrubError> {
    left.checked_mul(right).ok_or(ScrubError::OutOfBounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mod_song_and_sample_names_are_zeroed() {
        let mut bytes = vec![0u8; 1084];
        bytes[..4].copy_from_slice(b"MOD!");
        bytes[20..26].copy_from_slice(b"kick!!");
        bytes[1080..1084].copy_from_slice(b"M.K.");
        let scrubbed = scrub_metadata(&bytes, FileKind::Mod).unwrap();
        assert_eq!(&scrubbed.bytes[..4], &[0, 0, 0, 0]);
        assert!(scrubbed.bytes[20..26].iter().all(|byte| *byte == 0));
        assert!(scrubbed.summary.song_name);
        assert_eq!(scrubbed.summary.sample_names, 1);
        assert!(scrubbed.summary.changed());
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        assert_eq!(
            scrub_metadata(b"hello", FileKind::Wav),
            Err(ScrubError::Unsupported(FileKind::Wav))
        );
    }

    #[test]
    fn a_truncated_mod_is_refused() {
        assert_eq!(
            scrub_metadata(b"M.K.", FileKind::Mod),
            Err(ScrubError::Malformed)
        );
    }

    #[test]
    fn an_it_header_with_no_text_is_unchanged() {
        let bytes = vec![0u8; 0xC0];
        let mut bytes = bytes;
        bytes[..4].copy_from_slice(b"IMPM");
        let scrubbed = scrub_metadata(&bytes, FileKind::It).unwrap();
        assert!(!scrubbed.summary.changed());
        assert_eq!(scrubbed.bytes, bytes);
    }

    #[test]
    fn it_instrument_and_sample_names_are_zeroed() {
        let mut bytes = vec![0u8; 0x200];
        bytes[..4].copy_from_slice(b"IMPM");
        bytes[0x20..0x22].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x22..0x24].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x24..0x26].copy_from_slice(&1u16.to_le_bytes());
        // One order, then the instrument and sample offset tables.
        let instrument = 0x100u32;
        let sample = 0x180u32;
        bytes[0xC1..0xC5].copy_from_slice(&instrument.to_le_bytes());
        bytes[0xC5..0xC9].copy_from_slice(&sample.to_le_bytes());
        bytes[0x100..0x104].copy_from_slice(b"IMPI");
        bytes[0x104..0x10D].copy_from_slice(b"secret.it");
        bytes[0x120..0x136].copy_from_slice(b"Secret Instrument Name");
        bytes[0x180..0x184].copy_from_slice(b"IMPS");
        bytes[0x184..0x18E].copy_from_slice(b"secret2.it");
        bytes[0x1A0..0x1B6].copy_from_slice(b"Secret Sample Name....");
        let scrubbed = scrub_metadata(&bytes, FileKind::It).unwrap();
        assert_eq!(scrubbed.summary.instrument_names, 1);
        assert_eq!(scrubbed.summary.sample_names, 1);
        assert_eq!(scrubbed.summary.file_names, 2);
        assert!(scrubbed.bytes[0x104..0x110].iter().all(|byte| *byte == 0));
        assert!(scrubbed.bytes[0x120..0x13A].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn mtm_song_sample_and_comment_are_zeroed() {
        let mut bytes = vec![0u8; 0x42 + 37 + 128 + 64 + 10];
        bytes[..3].copy_from_slice(b"MTM");
        bytes[4..18].copy_from_slice(b"Leaky MTM Song");
        bytes[0x1C..0x1E].copy_from_slice(&10u16.to_le_bytes());
        bytes[0x1E] = 1;
        bytes[0x42..0x57].copy_from_slice(b"Secret MTM Sample....");
        bytes[0x42 + 37 + 128 + 64..].copy_from_slice(b"secret com");
        let scrubbed = scrub_metadata(&bytes, FileKind::Mtm).unwrap();
        assert!(scrubbed.summary.song_name);
        assert_eq!(scrubbed.summary.sample_names, 1);
        assert_eq!(scrubbed.summary.message_bytes, 10);
        assert!(scrubbed.bytes[4..24].iter().all(|byte| *byte == 0));
        assert!(scrubbed.bytes[0x42..0x58].iter().all(|byte| *byte == 0));
        assert!(
            scrubbed.bytes[0x42 + 37 + 128 + 64..]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn a_669_message_and_sample_name_are_zeroed() {
        let mut bytes = vec![0u8; 0x1F1 + 25];
        bytes[..2].copy_from_slice(b"if");
        bytes[2..10].copy_from_slice(b"leak 669");
        bytes[0x6E] = 1;
        bytes[0x1F1..0x1FE].copy_from_slice(b"secret.raw!!!");
        let scrubbed = scrub_metadata(&bytes, FileKind::SixSixNine).unwrap();
        assert_eq!(scrubbed.summary.message_bytes, 108);
        assert_eq!(scrubbed.summary.sample_names, 1);
        assert!(scrubbed.bytes[2..110].iter().all(|byte| *byte == 0));
        assert!(scrubbed.bytes[0x1F1..0x1FE].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn s3m_instrument_names_are_zeroed() {
        let mut bytes = vec![0u8; 0x120];
        bytes[0x20..0x22].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x22..0x24].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x2C..0x30].copy_from_slice(b"SCRM");
        // One order, one instrument pointer at 0x61 -> paragraph 7.
        bytes[0x61..0x63].copy_from_slice(&7u16.to_le_bytes());
        bytes[0x70] = 1;
        bytes[0x71..0x7D].copy_from_slice(b"secret.s3m!!");
        bytes[0xA0..0xB9].copy_from_slice(b"Secret S3M Sample Name...");
        let scrubbed = scrub_metadata(&bytes, FileKind::S3m).unwrap();
        assert_eq!(scrubbed.summary.sample_names, 1);
        assert_eq!(scrubbed.summary.file_names, 1);
        assert!(scrubbed.bytes[0x71..0x7D].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn an_it_message_is_zeroed() {
        let mut bytes = vec![0u8; 0xD0];
        bytes[..4].copy_from_slice(b"IMPM");
        // One order, no instruments or samples, no patterns.
        bytes[0x20..0x22].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x36..0x38].copy_from_slice(&8u16.to_le_bytes());
        bytes[0x38..0x3C].copy_from_slice(&0xC8u32.to_le_bytes());
        bytes[0xC8..0xD0].copy_from_slice(b"secret!!");
        let scrubbed = scrub_metadata(&bytes, FileKind::It).unwrap();
        assert_eq!(scrubbed.summary.message_bytes, 8);
        assert!(scrubbed.bytes[0xC8..0xD0].iter().all(|byte| *byte == 0));
    }

    fn structured_seed(kind: FileKind, header: [u8; 16]) -> Vec<u8> {
        let mut bytes = match kind {
            FileKind::It | FileKind::Mptm => vec![0u8; 0xC0],
            FileKind::Xm => vec![0u8; 80],
            FileKind::S3m => vec![0u8; 0x60],
            FileKind::Mtm => vec![0u8; 0x42],
            FileKind::SixSixNine => vec![0u8; 0x1F1],
            FileKind::Mod => vec![0u8; 1084],
            _ => unreachable!("only module kinds are tested"),
        };
        match kind {
            FileKind::It | FileKind::Mptm => {
                bytes[..4].copy_from_slice(b"IMPM");
                bytes[0x20..0x24].copy_from_slice(&header[..4]);
                bytes[0x2E..0x30].copy_from_slice(&header[4..6]);
                bytes[0x36..0x3C].copy_from_slice(&header[6..12]);
            }
            FileKind::Xm => {
                bytes[..17].copy_from_slice(b"Extended Module: ");
                bytes[60..64].copy_from_slice(&header[..4]);
            }
            FileKind::S3m => {
                bytes[0x2C..0x30].copy_from_slice(b"SCRM");
                bytes[0x20..0x26].copy_from_slice(&header[..6]);
            }
            FileKind::Mtm => {
                bytes[..3].copy_from_slice(b"MTM");
                bytes[0x18..0x20].copy_from_slice(&header[..8]);
            }
            FileKind::SixSixNine => {
                bytes[..2].copy_from_slice(b"if");
                bytes[0x6E] = header[0];
            }
            FileKind::Mod => {
                bytes[1080..1084].copy_from_slice(b"M.K.");
                bytes[20..22].copy_from_slice(&header[..2]);
            }
            _ => unreachable!("only module kinds are tested"),
        }
        bytes
    }

    proptest::proptest! {
        #[test]
        fn scrubbing_arbitrary_bytes_never_panics(
            bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2048),
            kind_index in 0usize..7,
        ) {
            let kinds = [
                FileKind::It,
                FileKind::Mptm,
                FileKind::Xm,
                FileKind::S3m,
                FileKind::Mtm,
                FileKind::SixSixNine,
                FileKind::Mod,
            ];
            if let Ok(scrubbed) = scrub_metadata(&bytes, kinds[kind_index]) {
                proptest::prop_assert_eq!(scrubbed.bytes.len(), bytes.len());
            }
        }

        #[test]
        fn scrubbing_a_structured_header_never_panics(
            header in proptest::prelude::any::<[u8; 16]>(),
            tail in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2048),
            kind_index in 0usize..7,
        ) {
            let kinds = [
                FileKind::It,
                FileKind::Mptm,
                FileKind::Xm,
                FileKind::S3m,
                FileKind::Mtm,
                FileKind::SixSixNine,
                FileKind::Mod,
            ];
            let kind = kinds[kind_index];
            let mut bytes = structured_seed(kind, header);
            bytes.extend_from_slice(&tail);
            if let Ok(scrubbed) = scrub_metadata(&bytes, kind) {
                proptest::prop_assert_eq!(scrubbed.bytes.len(), bytes.len());
            }
        }
    }
}
