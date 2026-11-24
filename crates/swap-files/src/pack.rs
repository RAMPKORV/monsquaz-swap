use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackManifest {
    sample_hashes: BTreeSet<String>,
}

impl PackManifest {
    pub fn from_hashes<I, S>(hashes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            sample_hashes: hashes.into_iter().map(Into::into).collect(),
        }
    }

    pub fn contains(&self, sha256: &str) -> bool {
        self.sample_hashes.contains(sha256)
    }

    pub fn len(&self) -> usize {
        self.sample_hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sample_hashes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleInfo {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackReport {
    pub unknown_samples: Vec<SampleInfo>,
}

impl PackReport {
    pub fn is_clean(&self) -> bool {
        self.unknown_samples.is_empty()
    }

    pub fn needs_review(&self) -> bool {
        !self.is_clean()
    }
}

/// Compare sample hashes with the pack. This is a report, not a gate. The host
/// reviews unknown samples; a false positive must never reject a seed.
pub fn check_samples(pack: &PackManifest, samples: &[SampleInfo]) -> PackReport {
    PackReport {
        unknown_samples: samples
            .iter()
            .filter(|sample| !pack.contains(&sample.sha256))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, hash: &str) -> SampleInfo {
        SampleInfo {
            name: name.to_string(),
            sha256: hash.to_string(),
        }
    }

    #[test]
    fn a_known_sample_is_clean() {
        let pack = PackManifest::from_hashes(["aaa", "bbb"]);
        let report = check_samples(&pack, &[sample("kick", "aaa")]);
        assert!(report.is_clean());
        assert!(!report.needs_review());
    }

    #[test]
    fn an_unknown_sample_needs_review() {
        let pack = PackManifest::from_hashes(["aaa"]);
        let report = check_samples(&pack, &[sample("kick", "aaa"), sample("mystery", "ccc")]);
        assert!(report.needs_review());
        assert_eq!(report.unknown_samples.len(), 1);
        assert_eq!(report.unknown_samples[0].name, "mystery");
    }

    #[test]
    fn an_empty_report_is_clean() {
        let pack = PackManifest::from_hashes(["aaa"]);
        assert!(check_samples(&pack, &[]).is_clean());
    }
}
