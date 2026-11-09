//! DACR-1 bounded exact-key source index over CONTEXT-1c artifacts.
//!
//! A direct address is only an index hint. The complete literal key must match
//! and the authoritative CONTEXT-1c raw source must be verified before Hit.
//! This reference does not implement learned routing or attention.

use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};

use crate::{
    context_semantic_ingest::{parse_provenance_artifact, CONTEXT_PROVENANCE_HEADER_BYTES},
    raw_context::{load_raw_segment, RawContextStoreError, RawContextStoreLimits},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactIndexLimits {
    pub bucket_bits: u32,
    pub max_entries: usize,
    pub max_key_bytes: usize,
    pub max_bucket_entries: usize,
    pub max_provenance_bytes: u64,
    pub raw_limits: RawContextStoreLimits,
}

impl ExactIndexLimits {
    pub fn validate(self) -> Result<Self, ExactIndexError> {
        if !(1..=16).contains(&self.bucket_bits)
            || self.max_entries == 0
            || self.max_key_bytes == 0
            || self.max_bucket_entries == 0
            || self.max_bucket_entries > self.max_entries
            || !(u64::from(CONTEXT_PROVENANCE_HEADER_BYTES)..=64 * 1024 * 1024)
                .contains(&self.max_provenance_bytes)
            || self.raw_limits.validate().is_err()
        {
            return Err(ExactIndexError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactBinding {
    pub key: Vec<u8>,
    pub segment: ContextSegmentId,
    pub content_hash: ContextContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExactHit {
    pub segment: ContextSegmentId,
    pub content_hash: ContextContentHash,
    pub raw_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactLookup {
    Miss,
    Ambiguous,
    Hit(VerifiedExactHit),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExactLookupAccounting {
    pub address_buckets_probed: u64,
    pub full_keys_compared: u64,
    pub raw_reread_segments: u64,
    pub raw_reread_bytes: u64,
}

#[derive(Debug)]
pub enum ExactIndexError {
    InvalidLimits,
    InvalidKey,
    InvalidSegment,
    WrongStream,
    WrongEpoch,
    DuplicateBinding,
    EntryBudget,
    BucketBudget,
    MissingProvenance,
    ProvenanceLimit,
    SourceMismatch,
    KeyAbsentFromSource,
    Raw(RawContextStoreError),
    Io(io::Error),
}

impl fmt::Display for ExactIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ExactIndexError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Raw(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ExactIndexError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Stream- and epoch-bound reference. Construction is not constant time;
/// lookup visits only a bounded address bucket.
#[derive(Debug)]
pub struct ExactSourceIndex {
    directory: PathBuf,
    stream: ContextStreamId,
    epoch: u64,
    limits: ExactIndexLimits,
    buckets: BTreeMap<u16, Vec<ExactBinding>>,
    entries: usize,
}

impl ExactSourceIndex {
    pub fn new(
        directory: impl AsRef<Path>,
        stream: ContextStreamId,
        epoch: u64,
        limits: ExactIndexLimits,
    ) -> Result<Self, ExactIndexError> {
        if epoch == 0 {
            return Err(ExactIndexError::WrongEpoch);
        }
        Ok(Self {
            directory: directory.as_ref().to_path_buf(),
            stream,
            epoch,
            limits: limits.validate()?,
            buckets: BTreeMap::new(),
            entries: 0,
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries == 0
    }

    fn validate_key(&self, key: &[u8]) -> Result<(), ExactIndexError> {
        if key.is_empty() || key.len() > self.limits.max_key_bytes {
            return Err(ExactIndexError::InvalidKey);
        }
        Ok(())
    }

    fn address(&self, key: &[u8]) -> u16 {
        // Stable, deliberately NON-cryptographic FNV-1a bucket partition.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for &byte in key {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (hash as u16) & ((1_u32 << self.limits.bucket_bits) - 1) as u16
    }

    /// Reject without mutating the index if any admission gate fails.
    pub fn insert(&mut self, binding: ExactBinding) -> Result<(), ExactIndexError> {
        self.validate_key(&binding.key)?;
        if binding.segment.stream_id != self.stream {
            return Err(ExactIndexError::WrongStream);
        }
        if binding.segment.segment_id == 0 {
            return Err(ExactIndexError::InvalidSegment);
        }
        let address = self.address(&binding.key);
        let bucket = self.buckets.get(&address);
        if bucket.is_some_and(|items| items.iter().any(|old| old == &binding)) {
            return Err(ExactIndexError::DuplicateBinding);
        }
        if self.entries >= self.limits.max_entries {
            return Err(ExactIndexError::EntryBudget);
        }
        if bucket.is_some_and(|items| items.len() >= self.limits.max_bucket_entries) {
            return Err(ExactIndexError::BucketBudget);
        }
        self.buckets.entry(address).or_default().push(binding);
        self.entries += 1;
        Ok(())
    }

    pub fn lookup(
        &self,
        stream: ContextStreamId,
        epoch: u64,
        key: &[u8],
    ) -> Result<(ExactLookup, ExactLookupAccounting), ExactIndexError> {
        if stream != self.stream {
            return Err(ExactIndexError::WrongStream);
        }
        if epoch != self.epoch {
            return Err(ExactIndexError::WrongEpoch);
        }
        self.validate_key(key)?;
        let mut accounting = ExactLookupAccounting {
            address_buckets_probed: 1,
            ..ExactLookupAccounting::default()
        };
        let Some(bucket) = self.buckets.get(&self.address(key)) else {
            return Ok((ExactLookup::Miss, accounting));
        };
        let mut selected = None;
        for binding in bucket {
            accounting.full_keys_compared += 1;
            if binding.key == key {
                if selected.is_some() {
                    return Ok((ExactLookup::Ambiguous, accounting));
                }
                selected = Some(binding);
            }
        }
        let Some(binding) = selected else {
            return Ok((ExactLookup::Miss, accounting));
        };
        let hit = self.verify(binding)?;
        accounting.raw_reread_segments = 1;
        accounting.raw_reread_bytes = hit.raw_bytes.len() as u64;
        Ok((ExactLookup::Hit(hit), accounting))
    }

    fn basename(&self, segment: ContextSegmentId) -> String {
        format!(
            "stream-{:016x}-segment-{:016x}",
            self.stream.get(),
            segment.segment_id
        )
    }

    fn verify(&self, binding: &ExactBinding) -> Result<VerifiedExactHit, ExactIndexError> {
        let base = self.basename(binding.segment);
        let path = self.directory.join(format!("{base}.smlprov"));
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(ExactIndexError::MissingProvenance);
            }
            Err(e) => return Err(e.into()),
        };
        let limit = self.limits.max_provenance_bytes;
        if file.metadata()?.len() > limit {
            return Err(ExactIndexError::ProvenanceLimit);
        }
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(ExactIndexError::ProvenanceLimit);
        }
        let manifest =
            parse_provenance_artifact(&bytes).map_err(|_| ExactIndexError::MissingProvenance)?;
        if manifest.stream_id != self.stream
            || manifest.segment_id != binding.segment.segment_id
            || manifest.content_hash != binding.content_hash
            || manifest.semantic_record_ids.is_empty()
        {
            return Err(ExactIndexError::SourceMismatch);
        }
        let raw = load_raw_segment(
            self.directory.join(format!("{base}.smlctx")),
            self.limits.raw_limits,
        )
        .map_err(ExactIndexError::Raw)?;
        if raw.reference.identity != binding.segment
            || raw.reference.content_hash != binding.content_hash
            || raw.reference.semantic_record_ids() != manifest.semantic_record_ids.as_slice()
        {
            return Err(ExactIndexError::SourceMismatch);
        }
        if !raw
            .raw_bytes
            .windows(binding.key.len())
            .any(|window| window == binding.key)
        {
            return Err(ExactIndexError::KeyAbsentFromSource);
        }
        Ok(VerifiedExactHit {
            segment: binding.segment,
            content_hash: binding.content_hash,
            raw_bytes: raw.raw_bytes,
        })
    }
}

#[cfg(test)]
mod dacr_exact_tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use sml_core::context_ledger::ContextSegmentRef;
    use crate::{
        context_semantic_ingest::{build_provenance_artifact, ContextProvenanceManifest},
        raw_context::{build_raw_segment_artifact, sha256_raw_bytes},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct Fixture {
        directory: PathBuf,
        stream: ContextStreamId,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "sml-dacr-exact-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self { directory, stream: ContextStreamId::new(71).unwrap() }
        }

        fn add(&self, n: u64, raw: &[u8]) -> ExactBinding {
            let segment = ContextSegmentId::new(self.stream, n).unwrap();
            let content_hash = sha256_raw_bytes(raw).unwrap();
            let reference = ContextSegmentRef::new(
                segment, 0, raw.len() as u64, 0, raw.len() as u64, n - 1,
                content_hash, vec![100 + n],
            ).unwrap();
            let manifest = ContextProvenanceManifest {
                stream_id: self.stream,
                segment_id: n,
                content_hash,
                semantic_record_ids: vec![100 + n],
            };
            let base = format!("stream-{:016x}-segment-{n:016x}", self.stream.get());
            fs::write(
                self.directory.join(format!("{base}.smlctx")),
                build_raw_segment_artifact(&reference, raw).unwrap(),
            ).unwrap();
            fs::write(
                self.directory.join(format!("{base}.smlprov")),
                build_provenance_artifact(&manifest).unwrap(),
            ).unwrap();
            ExactBinding { key: Vec::new(), segment, content_hash }
        }

        fn index(&self, limits: ExactIndexLimits) -> ExactSourceIndex {
            ExactSourceIndex::new(&self.directory, self.stream, 1, limits).unwrap()
        }

        fn path(&self, n: u64, ext: &str) -> PathBuf {
            self.directory.join(format!(
                "stream-{:016x}-segment-{n:016x}.{ext}",
                self.stream.get()
            ))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn limits() -> ExactIndexLimits {
        ExactIndexLimits {
            bucket_bits: 2,
            max_entries: 8,
            max_key_bytes: 32,
            max_bucket_entries: 4,
            max_provenance_bytes: 4096,
            raw_limits: RawContextStoreLimits {
                max_file_bytes: 4096,
                max_raw_bytes: 1024,
                max_semantic_links: 4,
            },
        }
    }

    fn bound(mut b: ExactBinding, key: &[u8]) -> ExactBinding {
        b.key = key.to_vec();
        b
    }

    #[test]
    fn exact_source_hit_and_missing_query_are_accounted() {
        let fixture = Fixture::new();
        let mut index = fixture.index(limits());
        let key = b"KEY=abc";
        index.insert(bound(fixture.add(1, b"prefix KEY=abc suffix"), key)).unwrap();
        assert_eq!(index.len(), 1);
        let (result, accounting) = index.lookup(fixture.stream, 1, key).unwrap();
        let ExactLookup::Hit(hit) = result else { panic!("expected verified hit"); };
        assert_eq!(hit.raw_bytes, b"prefix KEY=abc suffix");
        assert_eq!(accounting.raw_reread_segments, 1);
        assert_eq!(accounting.raw_reread_bytes, 21);
        let (miss, counter) = index.lookup(fixture.stream, 1, b"KEY=abs").unwrap();
        assert_eq!(miss, ExactLookup::Miss);
        assert_eq!(counter.raw_reread_segments, 0);
        assert_eq!(counter.address_buckets_probed, 1);
    }

    #[test]
    fn hash_collision_is_not_a_source_match() {
        let fixture = Fixture::new();
        let mut index = fixture.index(limits());
        let keys: Vec<Vec<u8>> = (0..40).map(|n| format!("key-{n}").into_bytes()).collect();
        let (a, b) = keys.iter().enumerate()
            .flat_map(|(i, a)| keys.iter().skip(i + 1).map(move |b| (a, b)))
            .find(|(a, b)| index.address(a) == index.address(b))
            .unwrap();
        index.insert(bound(fixture.add(1, a), a)).unwrap();
        index.insert(bound(fixture.add(2, b), b)).unwrap();
        let (hit, accounting) = index.lookup(fixture.stream, 1, b).unwrap();
        let ExactLookup::Hit(hit) = hit else { panic!("second exact key not retrieved"); };
        assert_eq!(hit.segment.segment_id, 2);
        assert_eq!(accounting.full_keys_compared, 2);
    }

    #[test]
    fn duplicate_and_ambiguous_binding_fail_closed() {
        let fixture = Fixture::new();
        let first = bound(fixture.add(1, b"same KEY"), b"KEY");
        let second = bound(fixture.add(2, b"second KEY"), b"KEY");
        let mut index = fixture.index(limits());
        index.insert(first.clone()).unwrap();
        assert!(matches!(index.insert(first), Err(ExactIndexError::DuplicateBinding)));
        assert_eq!(index.len(), 1);
        index.insert(second).unwrap();
        let (result, accounting) = index.lookup(fixture.stream, 1, b"KEY").unwrap();
        assert_eq!(result, ExactLookup::Ambiguous);
        assert_eq!(accounting.raw_reread_segments, 0);
    }

    #[test]
    fn bucket_and_entry_budgets_are_atomic() {
        let fixture = Fixture::new();
        let keys: Vec<Vec<u8>> = (0..60).map(|n| format!("value-{n}").into_bytes()).collect();
        let a = &keys[0];
        let b = keys.iter().skip(1).find(|k| {
            let index = fixture.index(limits());
            index.address(k) == index.address(a)
        }).unwrap();
        let mut capped = limits();
        capped.max_bucket_entries = 1;
        let mut index = fixture.index(capped);
        index.insert(bound(fixture.add(1, a), a)).unwrap();
        assert!(matches!(index.insert(bound(fixture.add(2, b), b)), Err(ExactIndexError::BucketBudget)));
        assert_eq!(index.len(), 1);
        assert!(matches!(index.lookup(fixture.stream, 1, a).unwrap().0, ExactLookup::Hit(_)));

        let mut one_entry = limits();
        one_entry.max_entries = 1;
        one_entry.max_bucket_entries = 1;
        let mut index = fixture.index(one_entry);
        index.insert(bound(fixture.add(1, a), a)).unwrap();
        assert!(matches!(index.insert(bound(fixture.add(2, b), b)), Err(ExactIndexError::EntryBudget)));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn invalid_key_epoch_stream_and_limits_are_rejected() {
        let fixture = Fixture::new();
        let mut index = fixture.index(limits());
        let b = bound(fixture.add(1, b"KEY"), b"KEY");
        assert!(matches!(index.insert(bound(b.clone(), b"")), Err(ExactIndexError::InvalidKey)));
        assert!(matches!(index.insert(bound(b.clone(), &[b'X'; 33])), Err(ExactIndexError::InvalidKey)));
        let mut other = b.clone();
        other.segment = ContextSegmentId::new(ContextStreamId::new(99).unwrap(), 1).unwrap();
        assert!(matches!(index.insert(other), Err(ExactIndexError::WrongStream)));
        index.insert(b).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 2, b"KEY"), Err(ExactIndexError::WrongEpoch)));
        assert!(matches!(index.lookup(ContextStreamId::new(99).unwrap(), 1, b"KEY"), Err(ExactIndexError::WrongStream)));
        assert!(matches!(ExactSourceIndex::new(&fixture.directory, fixture.stream, 0, limits()), Err(ExactIndexError::WrongEpoch)));
        let mut zero = limits(); zero.max_key_bytes = 0;
        assert!(matches!(ExactSourceIndex::new(&fixture.directory, fixture.stream, 1, zero), Err(ExactIndexError::InvalidLimits)));
    }

    #[test]
    fn missing_corrupt_or_mismatched_provenance_is_rejected() {
        let fixture = Fixture::new();
        let b = bound(fixture.add(1, b"KEY"), b"KEY");
        let mut index = fixture.index(limits());
        index.insert(b).unwrap();
        let path = fixture.path(1, "smlprov");
        let saved = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"KEY"), Err(ExactIndexError::MissingProvenance)));
        fs::write(&path, b"bad").unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"KEY"), Err(ExactIndexError::MissingProvenance)));
        fs::write(path, saved).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"KEY"), Ok((ExactLookup::Hit(_), _))));
    }

    #[test]
    fn missing_or_tampered_raw_and_wrong_source_hash_fail() {
        let fixture = Fixture::new();
        let b = bound(fixture.add(1, b"KEY data"), b"KEY");
        let mut index = fixture.index(limits());
        index.insert(b.clone()).unwrap();
        let path = fixture.path(1, "smlctx");
        let saved = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"KEY"), Err(ExactIndexError::Raw(_))));
        let mut altered = saved.clone();
        *altered.last_mut().unwrap() ^= 1;
        fs::write(&path, altered).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"KEY"), Err(ExactIndexError::Raw(_))));
        fs::write(&path, saved).unwrap();
        let mut wrong_hash = fixture.index(limits());
        wrong_hash.insert(ExactBinding { content_hash: ContextContentHash::from_sha256([0; 32]), ..b }).unwrap();
        assert!(matches!(wrong_hash.lookup(fixture.stream, 1, b"KEY"), Err(ExactIndexError::SourceMismatch)));
    }

    #[test]
    fn key_not_present_in_verified_raw_source_is_rejected() {
        let fixture = Fixture::new();
        let binding = bound(fixture.add(1, b"verified but unrelated"), b"ABSENT");
        let mut index = fixture.index(limits());
        index.insert(binding).unwrap();
        assert!(matches!(index.lookup(fixture.stream, 1, b"ABSENT"), Err(ExactIndexError::KeyAbsentFromSource)));
    }

    #[test]
    fn repeat_queries_are_deterministic_and_do_not_mutate() {
        let fixture = Fixture::new();
        let mut index = fixture.index(limits());
        index.insert(bound(fixture.add(1, b"literal KEY"), b"KEY")).unwrap();
        let first = index.lookup(fixture.stream, 1, b"KEY").unwrap();
        let second = index.lookup(fixture.stream, 1, b"KEY").unwrap();
        assert_eq!(first, second);
        assert_eq!(index.len(), 1);
    }
}
