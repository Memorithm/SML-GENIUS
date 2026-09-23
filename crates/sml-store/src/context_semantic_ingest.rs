//! CONTEXT-1c streaming semantic-index and provenance emission.
//!
//! Raw bytes, SBG semantic records and provenance remain three distinct
//! immutable artifacts. They are prepared from the same segment before any
//! final pathname is published. Process-level failures roll back links created
//! during the current bundle; crash-durable multi-file transactions are not
//! claimed by this reference slice.

use std::{
    collections::BTreeSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::{
        ContextContentHash, ContextLedgerError, ContextSegmentId, ContextSegmentRef,
        ContextStreamId,
    },
    semantic::{build_semantic_store, SemanticState, SemanticStoreError},
};

use crate::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    raw_context::{build_raw_segment_artifact, sha256_raw_bytes, RawContextStoreError},
};

pub const CONTEXT_PROVENANCE_MAGIC: [u8; 8] = *b"SMLPRV1C";
pub const CONTEXT_PROVENANCE_VERSION: u16 = 1;
pub const CONTEXT_PROVENANCE_HEADER_BYTES: u16 = 84;

const PROVENANCE_COUNT_OFFSET: usize = 12;
const PROVENANCE_STREAM_OFFSET: usize = 16;
const PROVENANCE_SEGMENT_OFFSET: usize = 24;
const PROVENANCE_HASH_OFFSET: usize = 32;
const PROVENANCE_PAYLOAD_BYTES_OFFSET: usize = 64;
const PROVENANCE_PAYLOAD_CRC_OFFSET: usize = 72;
const PROVENANCE_HEADER_CRC_OFFSET: usize = 76;
const PROVENANCE_RESERVED_OFFSET: usize = 80;
const CRC32C_POLYNOMIAL: u32 = 0x82f6_3b78;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticIngestPolicy {
    pub max_records_per_segment: usize,
    pub max_semantic_store_bytes: u64,
    pub max_provenance_bytes: u64,
}

impl SemanticIngestPolicy {
    pub fn validate(self) -> Result<Self, ContextSemanticIngestError> {
        if self.max_records_per_segment == 0
            || self.max_semantic_store_bytes == 0
            || self.max_provenance_bytes < u64::from(CONTEXT_PROVENANCE_HEADER_BYTES)
        {
            return Err(ContextSemanticIngestError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentEmissionContext {
    pub stream_id: ContextStreamId,
    pub segment_id: u64,
    pub byte_start: u64,
    pub token_start: u64,
    pub token_len: u64,
    pub order_index: u64,
}

pub trait SemanticSegmentEmitter<const BITS: usize> {
    fn emit_segment(
        &mut self,
        raw_segment: &[u8],
        context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<BITS>)>, ContextSemanticIngestError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProvenanceManifest {
    pub stream_id: ContextStreamId,
    pub segment_id: u64,
    pub content_hash: ContextContentHash,
    pub semantic_record_ids: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SemanticStreamingStats {
    pub segments_published: u64,
    pub raw_bytes_published: u64,
    pub tokenizer_tokens_published: u64,
    pub semantic_records_published: u64,
    pub semantic_store_bytes_published: u64,
    pub provenance_bytes_published: u64,
    pub peak_raw_segment_bytes: u64,
    pub peak_semantic_store_bytes: u64,
    pub peak_provenance_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStreamingOutcome {
    pub stream_id: ContextStreamId,
    pub stats: SemanticStreamingStats,
    pub last_segment: Option<ContextSegmentRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStreamingContextIngestor<const BITS: usize> {
    stream_id: ContextStreamId,
    output_directory: PathBuf,
    raw_policy: StreamingIngestPolicy,
    semantic_policy: SemanticIngestPolicy,
    _bits: PhantomData<[u8; BITS]>,
}

impl<const BITS: usize> SemanticStreamingContextIngestor<BITS> {
    pub fn new(
        stream_id: ContextStreamId,
        output_directory: impl AsRef<Path>,
        raw_policy: StreamingIngestPolicy,
        semantic_policy: SemanticIngestPolicy,
    ) -> Result<Self, ContextSemanticIngestError> {
        let raw_policy = raw_policy
            .validate()
            .map_err(ContextSemanticIngestError::Ingest)?;
        let semantic_policy = semantic_policy.validate()?;
        let output_directory = output_directory.as_ref().to_path_buf();
        if !fs::metadata(&output_directory)?.is_dir() {
            return Err(ContextSemanticIngestError::OutputNotDirectory);
        }
        Ok(Self {
            stream_id,
            output_directory,
            raw_policy,
            semantic_policy,
            _bits: PhantomData,
        })
    }

    pub fn ingest_reader<R, T, E>(
        &self,
        mut reader: R,
        token_counter: &mut T,
        emitter: &mut E,
    ) -> Result<SemanticStreamingOutcome, ContextSemanticIngestError>
    where
        R: Read,
        T: SegmentTokenCounter,
        E: SemanticSegmentEmitter<BITS>,
    {
        let mut buffer = vec![0_u8; self.raw_policy.max_segment_bytes];
        let mut filled = 0_usize;
        let mut byte_start = 0_u64;
        let mut token_start = 0_u64;
        let mut order_index = 0_u64;
        let mut next_segment_id = 1_u64;
        let mut last_segment = None;
        let mut stats = SemanticStreamingStats::default();

        loop {
            let read = reader.read(&mut buffer[filled..])?;
            if read == 0 {
                if filled != 0 {
                    self.publish_buffer(
                        &buffer[..filled],
                        token_counter,
                        emitter,
                        &mut byte_start,
                        &mut token_start,
                        &mut order_index,
                        &mut next_segment_id,
                        &mut stats,
                        &mut last_segment,
                    )?;
                }
                break;
            }
            filled = filled
                .checked_add(read)
                .ok_or(ContextSemanticIngestError::Overflow)?;
            if filled == buffer.len() {
                self.publish_buffer(
                    &buffer,
                    token_counter,
                    emitter,
                    &mut byte_start,
                    &mut token_start,
                    &mut order_index,
                    &mut next_segment_id,
                    &mut stats,
                    &mut last_segment,
                )?;
                filled = 0;
            }
        }

        Ok(SemanticStreamingOutcome {
            stream_id: self.stream_id,
            stats,
            last_segment,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_buffer<T, E>(
        &self,
        raw: &[u8],
        token_counter: &mut T,
        emitter: &mut E,
        byte_start: &mut u64,
        token_start: &mut u64,
        order_index: &mut u64,
        next_segment_id: &mut u64,
        stats: &mut SemanticStreamingStats,
        last_segment: &mut Option<ContextSegmentRef>,
    ) -> Result<(), ContextSemanticIngestError>
    where
        T: SegmentTokenCounter,
        E: SemanticSegmentEmitter<BITS>,
    {
        if stats.segments_published >= self.raw_policy.max_segments {
            return Err(ContextSemanticIngestError::SegmentBudget);
        }
        let raw_len = u64::try_from(raw.len()).map_err(|_| ContextSemanticIngestError::Overflow)?;
        let total_raw = stats
            .raw_bytes_published
            .checked_add(raw_len)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        if total_raw > self.raw_policy.max_total_raw_bytes {
            return Err(ContextSemanticIngestError::RawByteBudget {
                actual: total_raw,
                maximum: self.raw_policy.max_total_raw_bytes,
            });
        }

        let token_len = token_counter
            .count_segment_tokens(raw)
            .map_err(ContextSemanticIngestError::Ingest)?;
        let segment_id = *next_segment_id;
        let context = SegmentEmissionContext {
            stream_id: self.stream_id,
            segment_id,
            byte_start: *byte_start,
            token_start: *token_start,
            token_len,
            order_index: *order_index,
        };
        let mut records = emitter.emit_segment(raw, context)?;
        if records.is_empty() {
            return Err(ContextSemanticIngestError::EmptySemanticEmission);
        }
        if records.len() > self.semantic_policy.max_records_per_segment {
            return Err(ContextSemanticIngestError::SemanticRecordBudget {
                actual: records.len(),
                maximum: self.semantic_policy.max_records_per_segment,
            });
        }
        records.sort_unstable_by_key(|(id, _)| *id);
        for pair in records.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(ContextSemanticIngestError::DuplicateSemanticId(pair[0].0));
            }
        }

        let semantic_ids = records.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let content_hash = sha256_raw_bytes(raw)?;
        let identity = ContextSegmentId::new(self.stream_id, segment_id)?;
        let reference = ContextSegmentRef::new(
            identity,
            *byte_start,
            raw_len,
            *token_start,
            token_len,
            *order_index,
            content_hash,
            semantic_ids,
        )?;

        validate_contiguity(last_segment.as_ref(), &reference)?;

        let raw_artifact = build_raw_segment_artifact(&reference, raw)?;
        let semantic_artifact = build_semantic_store(&records)?;
        let semantic_bytes = u64::try_from(semantic_artifact.len())
            .map_err(|_| ContextSemanticIngestError::Overflow)?;
        if semantic_bytes > self.semantic_policy.max_semantic_store_bytes {
            return Err(ContextSemanticIngestError::SemanticStoreBudget {
                actual: semantic_bytes,
                maximum: self.semantic_policy.max_semantic_store_bytes,
            });
        }

        let provenance = ContextProvenanceManifest {
            stream_id: self.stream_id,
            segment_id,
            content_hash,
            semantic_record_ids: reference.semantic_record_ids().to_vec(),
        };
        let provenance_artifact = build_provenance_artifact(&provenance)?;
        let provenance_bytes = u64::try_from(provenance_artifact.len())
            .map_err(|_| ContextSemanticIngestError::Overflow)?;
        if provenance_bytes > self.semantic_policy.max_provenance_bytes {
            return Err(ContextSemanticIngestError::ProvenanceBudget {
                actual: provenance_bytes,
                maximum: self.semantic_policy.max_provenance_bytes,
            });
        }

        self.publish_bundle(
            segment_id,
            &raw_artifact,
            &semantic_artifact,
            &provenance_artifact,
        )?;

        stats.segments_published = stats.segments_published.saturating_add(1);
        stats.raw_bytes_published = total_raw;
        stats.tokenizer_tokens_published = stats
            .tokenizer_tokens_published
            .checked_add(token_len)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        stats.semantic_records_published = stats
            .semantic_records_published
            .checked_add(
                u64::try_from(records.len()).map_err(|_| ContextSemanticIngestError::Overflow)?,
            )
            .ok_or(ContextSemanticIngestError::Overflow)?;
        stats.semantic_store_bytes_published = stats
            .semantic_store_bytes_published
            .checked_add(semantic_bytes)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        stats.provenance_bytes_published = stats
            .provenance_bytes_published
            .checked_add(provenance_bytes)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        stats.peak_raw_segment_bytes = stats.peak_raw_segment_bytes.max(raw_len);
        stats.peak_semantic_store_bytes = stats.peak_semantic_store_bytes.max(semantic_bytes);
        stats.peak_provenance_bytes = stats.peak_provenance_bytes.max(provenance_bytes);

        *byte_start = reference.byte_end()?;
        *token_start = reference.token_end()?;
        *order_index = order_index
            .checked_add(1)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        *next_segment_id = next_segment_id
            .checked_add(1)
            .ok_or(ContextSemanticIngestError::Overflow)?;
        *last_segment = Some(reference);
        Ok(())
    }

    fn publish_bundle(
        &self,
        segment_id: u64,
        raw: &[u8],
        semantic: &[u8],
        provenance: &[u8],
    ) -> Result<(), ContextSemanticIngestError> {
        let paths = [
            self.raw_path(segment_id),
            self.semantic_path(segment_id),
            self.provenance_path(segment_id),
        ];
        if paths.iter().any(|path| path.exists()) {
            return Err(ContextSemanticIngestError::TargetExists);
        }

        let payloads = [raw, semantic, provenance];
        let mut published = Vec::new();
        for (path, payload) in paths.iter().zip(payloads) {
            match publish_immutable_bytes(path, payload) {
                Ok(()) => published.push(path.clone()),
                Err(error) => {
                    for created in published {
                        let _ = fs::remove_file(created);
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn raw_path(&self, segment_id: u64) -> PathBuf {
        self.output_directory
            .join(base_name(self.stream_id, segment_id) + ".smlctx")
    }

    #[must_use]
    pub fn semantic_path(&self, segment_id: u64) -> PathBuf {
        self.output_directory
            .join(base_name(self.stream_id, segment_id) + ".smlsbg")
    }

    #[must_use]
    pub fn provenance_path(&self, segment_id: u64) -> PathBuf {
        self.output_directory
            .join(base_name(self.stream_id, segment_id) + ".smlprov")
    }
}

fn base_name(stream_id: ContextStreamId, segment_id: u64) -> String {
    format!("stream-{:016x}-segment-{segment_id:016x}", stream_id.get())
}

fn validate_contiguity(
    previous: Option<&ContextSegmentRef>,
    current: &ContextSegmentRef,
) -> Result<(), ContextSemanticIngestError> {
    match previous {
        None => {
            if current.byte_start != 0 || current.token_start != 0 || current.order_index != 0 {
                return Err(ContextSemanticIngestError::NonContiguous);
            }
        }
        Some(previous) => {
            if current.byte_start != previous.byte_end()?
                || current.token_start != previous.token_end()?
                || current.order_index
                    != previous
                        .order_index
                        .checked_add(1)
                        .ok_or(ContextSemanticIngestError::Overflow)?
                || current.identity.segment_id <= previous.identity.segment_id
            {
                return Err(ContextSemanticIngestError::NonContiguous);
            }
        }
    }
    Ok(())
}

pub fn build_provenance_artifact(
    manifest: &ContextProvenanceManifest,
) -> Result<Vec<u8>, ContextSemanticIngestError> {
    if manifest.semantic_record_ids.is_empty() {
        return Err(ContextSemanticIngestError::EmptySemanticEmission);
    }
    let mut ids = manifest.semantic_record_ids.clone();
    ids.sort_unstable();
    if ids != manifest.semantic_record_ids {
        return Err(ContextSemanticIngestError::NonCanonicalSemanticIds);
    }
    for pair in ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(ContextSemanticIngestError::DuplicateSemanticId(pair[0]));
        }
    }

    let count = u32::try_from(ids.len()).map_err(|_| ContextSemanticIngestError::Overflow)?;
    let payload_bytes = u64::from(count)
        .checked_mul(8)
        .ok_or(ContextSemanticIngestError::Overflow)?;
    let total = u64::from(CONTEXT_PROVENANCE_HEADER_BYTES)
        .checked_add(payload_bytes)
        .ok_or(ContextSemanticIngestError::Overflow)?;
    let total = usize::try_from(total).map_err(|_| ContextSemanticIngestError::Overflow)?;
    let mut bytes = vec![0_u8; total];

    bytes[0..8].copy_from_slice(&CONTEXT_PROVENANCE_MAGIC);
    put_u16(&mut bytes, 8, CONTEXT_PROVENANCE_VERSION)?;
    put_u16(&mut bytes, 10, CONTEXT_PROVENANCE_HEADER_BYTES)?;
    put_u32(&mut bytes, PROVENANCE_COUNT_OFFSET, count)?;
    put_u64(
        &mut bytes,
        PROVENANCE_STREAM_OFFSET,
        manifest.stream_id.get(),
    )?;
    put_u64(&mut bytes, PROVENANCE_SEGMENT_OFFSET, manifest.segment_id)?;
    put_bytes(
        &mut bytes,
        PROVENANCE_HASH_OFFSET,
        manifest.content_hash.bytes(),
    )?;
    put_u64(&mut bytes, PROVENANCE_PAYLOAD_BYTES_OFFSET, payload_bytes)?;

    let payload_start = usize::from(CONTEXT_PROVENANCE_HEADER_BYTES);
    let mut cursor = payload_start;
    for id in ids {
        put_u64(&mut bytes, cursor, id)?;
        cursor = cursor
            .checked_add(8)
            .ok_or(ContextSemanticIngestError::Overflow)?;
    }
    let payload_crc = crc32c(&bytes[payload_start..]);
    put_u32(&mut bytes, PROVENANCE_PAYLOAD_CRC_OFFSET, payload_crc)?;
    put_u32(&mut bytes, PROVENANCE_RESERVED_OFFSET, 0)?;

    let mut header_for_crc = bytes[..usize::from(CONTEXT_PROVENANCE_HEADER_BYTES)].to_vec();
    header_for_crc[PROVENANCE_HEADER_CRC_OFFSET..PROVENANCE_HEADER_CRC_OFFSET + 4].fill(0);
    let header_crc = crc32c(&header_for_crc);
    put_u32(&mut bytes, PROVENANCE_HEADER_CRC_OFFSET, header_crc)?;
    Ok(bytes)
}

pub fn parse_provenance_artifact(
    bytes: &[u8],
) -> Result<ContextProvenanceManifest, ContextSemanticIngestError> {
    let header_bytes = usize::from(CONTEXT_PROVENANCE_HEADER_BYTES);
    if bytes.len() < header_bytes {
        return Err(ContextSemanticIngestError::TruncatedProvenance);
    }
    if bytes[0..8] != CONTEXT_PROVENANCE_MAGIC {
        return Err(ContextSemanticIngestError::InvalidProvenanceMagic);
    }
    if get_u16(bytes, 8)? != CONTEXT_PROVENANCE_VERSION
        || get_u16(bytes, 10)? != CONTEXT_PROVENANCE_HEADER_BYTES
    {
        return Err(ContextSemanticIngestError::UnsupportedProvenanceVersion);
    }
    if get_u32(bytes, PROVENANCE_RESERVED_OFFSET)? != 0 {
        return Err(ContextSemanticIngestError::UnsupportedProvenanceFlags);
    }

    let mut header_for_crc = bytes[..header_bytes].to_vec();
    let expected_header_crc = get_u32(bytes, PROVENANCE_HEADER_CRC_OFFSET)?;
    header_for_crc[PROVENANCE_HEADER_CRC_OFFSET..PROVENANCE_HEADER_CRC_OFFSET + 4].fill(0);
    let actual_header_crc = crc32c(&header_for_crc);
    if expected_header_crc != actual_header_crc {
        return Err(ContextSemanticIngestError::ProvenanceHeaderChecksum);
    }

    let count = get_u32(bytes, PROVENANCE_COUNT_OFFSET)?;
    if count == 0 {
        return Err(ContextSemanticIngestError::EmptySemanticEmission);
    }
    let payload_bytes = get_u64(bytes, PROVENANCE_PAYLOAD_BYTES_OFFSET)?;
    if payload_bytes != u64::from(count) * 8 {
        return Err(ContextSemanticIngestError::ProvenanceLayout);
    }
    let expected_len = u64::from(CONTEXT_PROVENANCE_HEADER_BYTES)
        .checked_add(payload_bytes)
        .ok_or(ContextSemanticIngestError::Overflow)?;
    if u64::try_from(bytes.len()).map_err(|_| ContextSemanticIngestError::Overflow)? != expected_len
    {
        return Err(ContextSemanticIngestError::ProvenanceLayout);
    }
    let payload = &bytes[header_bytes..];
    if get_u32(bytes, PROVENANCE_PAYLOAD_CRC_OFFSET)? != crc32c(payload) {
        return Err(ContextSemanticIngestError::ProvenancePayloadChecksum);
    }

    let mut ids = Vec::with_capacity(count as usize);
    for chunk in payload.chunks_exact(8) {
        ids.push(u64::from_le_bytes(
            chunk
                .try_into()
                .map_err(|_| ContextSemanticIngestError::ProvenanceLayout)?,
        ));
    }
    for pair in ids.windows(2) {
        if pair[0] >= pair[1] {
            return Err(ContextSemanticIngestError::NonCanonicalSemanticIds);
        }
    }

    let hash: [u8; 32] = bytes[PROVENANCE_HASH_OFFSET..PROVENANCE_HASH_OFFSET + 32]
        .try_into()
        .map_err(|_| ContextSemanticIngestError::TruncatedProvenance)?;
    let stream_id = ContextStreamId::new(get_u64(bytes, PROVENANCE_STREAM_OFFSET)?)?;
    let segment_id = get_u64(bytes, PROVENANCE_SEGMENT_OFFSET)?;
    let _identity = ContextSegmentId::new(stream_id, segment_id)?;
    Ok(ContextProvenanceManifest {
        stream_id,
        segment_id,
        content_hash: ContextContentHash::from_sha256(hash),
        semantic_record_ids: ids,
    })
}

fn publish_immutable_bytes(
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), ContextSemanticIngestError> {
    if final_path.exists() {
        return Err(ContextSemanticIngestError::TargetExists);
    }
    let parent = final_path
        .parent()
        .ok_or(ContextSemanticIngestError::MissingParent)?;
    let (temp_path, mut file) = create_temp_file(parent)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&temp_path, final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(ContextSemanticIngestError::TargetExists);
            }
            Err(error) => return Err(ContextSemanticIngestError::Io(error.kind())),
        }
        fs::remove_file(&temp_path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_temp_file(parent: &Path) -> Result<(PathBuf, File), ContextSemanticIngestError> {
    for _ in 0..32 {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".sml-context-semantic-{}.{}.tmp",
            std::process::id(),
            id
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ContextSemanticIngestError::Io(error.kind())),
        }
    }
    Err(ContextSemanticIngestError::TempNameExhausted)
}

fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (CRC32C_POLYNOMIAL & mask);
        }
    }
    !crc
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), ContextSemanticIngestError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), ContextSemanticIngestError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), ContextSemanticIngestError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_bytes(
    bytes: &mut [u8],
    offset: usize,
    value: &[u8],
) -> Result<(), ContextSemanticIngestError> {
    let end = offset
        .checked_add(value.len())
        .ok_or(ContextSemanticIngestError::Overflow)?;
    bytes
        .get_mut(offset..end)
        .ok_or(ContextSemanticIngestError::ProvenanceLayout)?
        .copy_from_slice(value);
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, ContextSemanticIngestError> {
    Ok(u16::from_le_bytes(get_array::<2>(bytes, offset)?))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, ContextSemanticIngestError> {
    Ok(u32::from_le_bytes(get_array::<4>(bytes, offset)?))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, ContextSemanticIngestError> {
    Ok(u64::from_le_bytes(get_array::<8>(bytes, offset)?))
}

fn get_array<const N: usize>(
    bytes: &[u8],
    offset: usize,
) -> Result<[u8; N], ContextSemanticIngestError> {
    let end = offset
        .checked_add(N)
        .ok_or(ContextSemanticIngestError::Overflow)?;
    bytes
        .get(offset..end)
        .ok_or(ContextSemanticIngestError::TruncatedProvenance)?
        .try_into()
        .map_err(|_| ContextSemanticIngestError::TruncatedProvenance)
}

#[derive(Debug)]
pub enum ContextSemanticIngestError {
    InvalidPolicy,
    OutputNotDirectory,
    SegmentBudget,
    RawByteBudget { actual: u64, maximum: u64 },
    EmptySemanticEmission,
    SemanticRecordBudget { actual: usize, maximum: usize },
    DuplicateSemanticId(u64),
    SemanticStoreBudget { actual: u64, maximum: u64 },
    ProvenanceBudget { actual: u64, maximum: u64 },
    TargetExists,
    MissingParent,
    TempNameExhausted,
    NonContiguous,
    InvalidProvenanceMagic,
    UnsupportedProvenanceVersion,
    UnsupportedProvenanceFlags,
    TruncatedProvenance,
    ProvenanceHeaderChecksum,
    ProvenancePayloadChecksum,
    ProvenanceLayout,
    NonCanonicalSemanticIds,
    Overflow,
    Io(io::ErrorKind),
    Ingest(ContextIngestError),
    Ledger(ContextLedgerError),
    Store(RawContextStoreError),
    SemanticStore(SemanticStoreError),
}

impl From<io::Error> for ContextSemanticIngestError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<ContextLedgerError> for ContextSemanticIngestError {
    fn from(error: ContextLedgerError) -> Self {
        Self::Ledger(error)
    }
}

impl From<RawContextStoreError> for ContextSemanticIngestError {
    fn from(error: RawContextStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<SemanticStoreError> for ContextSemanticIngestError {
    fn from(error: SemanticStoreError) -> Self {
        Self::SemanticStore(error)
    }
}

impl fmt::Display for ContextSemanticIngestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContextSemanticIngestError {}
