//! CONTEXT-1a immutable raw context segment artifact.
//!
//! Raw source bytes and semantic indexes stay separate. This store computes and
//! verifies SHA-256 over the exact raw bytes, while CRC32C is used only for
//! accidental metadata corruption detection. Neither checksum is an
//! authorization mechanism.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::context_ledger::{
    ContextContentHash, ContextLedgerError, ContextSegmentId, ContextSegmentRef, ContextStreamId,
};

pub const RAW_CONTEXT_MAGIC: [u8; 8] = *b"SMLCTX1A";
pub const RAW_CONTEXT_VERSION: u16 = 1;
pub const RAW_CONTEXT_HEADER_BYTES: u16 = 144;

const FLAG_OFFSET: usize = 12;
const STREAM_ID_OFFSET: usize = 16;
const SEGMENT_ID_OFFSET: usize = 24;
const BYTE_START_OFFSET: usize = 32;
const BYTE_LEN_OFFSET: usize = 40;
const TOKEN_START_OFFSET: usize = 48;
const TOKEN_LEN_OFFSET: usize = 56;
const ORDER_INDEX_OFFSET: usize = 64;
const SEMANTIC_COUNT_OFFSET: usize = 72;
const SEMANTIC_CRC_OFFSET: usize = 76;
const SEMANTIC_OFFSET_OFFSET: usize = 80;
const RAW_OFFSET_OFFSET: usize = 88;
const FILE_BYTES_OFFSET: usize = 96;
const CONTENT_HASH_OFFSET: usize = 104;
const HEADER_CRC_OFFSET: usize = 136;
const RESERVED_OFFSET: usize = 140;
const CRC32C_POLYNOMIAL: u32 = 0x82f6_3b78;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawContextStoreLimits {
    pub max_file_bytes: u64,
    pub max_raw_bytes: u64,
    pub max_semantic_links: u32,
}

impl RawContextStoreLimits {
    pub fn validate(self) -> Result<Self, RawContextStoreError> {
        if self.max_file_bytes < u64::from(RAW_CONTEXT_HEADER_BYTES) || self.max_raw_bytes == 0 {
            return Err(RawContextStoreError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawContextSegment {
    pub reference: ContextSegmentRef,
    pub raw_bytes: Vec<u8>,
}

pub fn sha256_raw_bytes(bytes: &[u8]) -> Result<ContextContentHash, RawContextStoreError> {
    Ok(ContextContentHash::from_sha256(sha256(bytes)?))
}

pub fn build_raw_segment_artifact(
    reference: &ContextSegmentRef,
    raw_bytes: &[u8],
) -> Result<Vec<u8>, RawContextStoreError> {
    let raw_len = u64::try_from(raw_bytes.len()).map_err(|_| RawContextStoreError::Overflow)?;
    if raw_len != reference.byte_len {
        return Err(RawContextStoreError::RawLengthMismatch {
            expected: reference.byte_len,
            actual: raw_len,
        });
    }
    let actual_hash = sha256_raw_bytes(raw_bytes)?;
    if actual_hash != reference.content_hash {
        return Err(RawContextStoreError::ContentHashMismatch {
            expected: reference.content_hash,
            actual: actual_hash,
        });
    }

    let semantic_count = u32::try_from(reference.semantic_record_ids().len())
        .map_err(|_| RawContextStoreError::Overflow)?;
    let semantic_bytes = usize::try_from(semantic_count)
        .map_err(|_| RawContextStoreError::Overflow)?
        .checked_mul(core::mem::size_of::<u64>())
        .ok_or(RawContextStoreError::Overflow)?;
    let raw_offset = usize::from(RAW_CONTEXT_HEADER_BYTES)
        .checked_add(semantic_bytes)
        .ok_or(RawContextStoreError::Overflow)?;
    let file_len = raw_offset
        .checked_add(raw_bytes.len())
        .ok_or(RawContextStoreError::Overflow)?;
    let raw_offset_u64 = u64::try_from(raw_offset).map_err(|_| RawContextStoreError::Overflow)?;
    let file_len_u64 = u64::try_from(file_len).map_err(|_| RawContextStoreError::Overflow)?;

    let mut artifact = vec![0_u8; file_len];
    artifact[0..8].copy_from_slice(&RAW_CONTEXT_MAGIC);
    put_u16(&mut artifact, 8, RAW_CONTEXT_VERSION)?;
    put_u16(&mut artifact, 10, RAW_CONTEXT_HEADER_BYTES)?;
    put_u32(&mut artifact, FLAG_OFFSET, 0)?;
    put_u64(
        &mut artifact,
        STREAM_ID_OFFSET,
        reference.identity.stream_id.get(),
    )?;
    put_u64(
        &mut artifact,
        SEGMENT_ID_OFFSET,
        reference.identity.segment_id,
    )?;
    put_u64(&mut artifact, BYTE_START_OFFSET, reference.byte_start)?;
    put_u64(&mut artifact, BYTE_LEN_OFFSET, reference.byte_len)?;
    put_u64(&mut artifact, TOKEN_START_OFFSET, reference.token_start)?;
    put_u64(&mut artifact, TOKEN_LEN_OFFSET, reference.token_len)?;
    put_u64(&mut artifact, ORDER_INDEX_OFFSET, reference.order_index)?;
    put_u32(&mut artifact, SEMANTIC_COUNT_OFFSET, semantic_count)?;
    put_u64(
        &mut artifact,
        SEMANTIC_OFFSET_OFFSET,
        u64::from(RAW_CONTEXT_HEADER_BYTES),
    )?;
    put_u64(&mut artifact, RAW_OFFSET_OFFSET, raw_offset_u64)?;
    put_u64(&mut artifact, FILE_BYTES_OFFSET, file_len_u64)?;
    put_bytes(
        &mut artifact,
        CONTENT_HASH_OFFSET,
        reference.content_hash.bytes(),
    )?;

    let mut cursor = usize::from(RAW_CONTEXT_HEADER_BYTES);
    for &semantic_id in reference.semantic_record_ids() {
        put_u64(&mut artifact, cursor, semantic_id)?;
        cursor = cursor
            .checked_add(core::mem::size_of::<u64>())
            .ok_or(RawContextStoreError::Overflow)?;
    }
    let semantic_crc = crc32c(&artifact[usize::from(RAW_CONTEXT_HEADER_BYTES)..raw_offset]);
    put_u32(&mut artifact, SEMANTIC_CRC_OFFSET, semantic_crc)?;
    let header_crc = crc32c(&artifact[..HEADER_CRC_OFFSET]);
    put_u32(&mut artifact, HEADER_CRC_OFFSET, header_crc)?;
    put_u32(&mut artifact, RESERVED_OFFSET, 0)?;
    artifact[raw_offset..].copy_from_slice(raw_bytes);
    Ok(artifact)
}

pub fn parse_raw_segment_artifact(
    bytes: &[u8],
    limits: RawContextStoreLimits,
) -> Result<RawContextSegment, RawContextStoreError> {
    let limits = limits.validate()?;
    if bytes.len() < usize::from(RAW_CONTEXT_HEADER_BYTES) {
        return Err(RawContextStoreError::TruncatedArtifact);
    }
    if bytes[0..8] != RAW_CONTEXT_MAGIC {
        return Err(RawContextStoreError::InvalidMagic);
    }
    let version = get_u16(bytes, 8)?;
    if version != RAW_CONTEXT_VERSION {
        return Err(RawContextStoreError::UnsupportedVersion(version));
    }
    let header_bytes = get_u16(bytes, 10)?;
    if header_bytes != RAW_CONTEXT_HEADER_BYTES {
        return Err(RawContextStoreError::InvalidHeaderBytes(header_bytes));
    }
    if get_u32(bytes, FLAG_OFFSET)? != 0 || get_u32(bytes, RESERVED_OFFSET)? != 0 {
        return Err(RawContextStoreError::UnsupportedFlags);
    }

    let expected_header_crc = get_u32(bytes, HEADER_CRC_OFFSET)?;
    let actual_header_crc = crc32c(&bytes[..HEADER_CRC_OFFSET]);
    if expected_header_crc != actual_header_crc {
        return Err(RawContextStoreError::HeaderChecksumMismatch {
            expected: expected_header_crc,
            actual: actual_header_crc,
        });
    }

    let file_bytes = get_u64(bytes, FILE_BYTES_OFFSET)?;
    let actual_file_bytes =
        u64::try_from(bytes.len()).map_err(|_| RawContextStoreError::Overflow)?;
    if file_bytes != actual_file_bytes {
        return Err(RawContextStoreError::FileLengthMismatch {
            expected: file_bytes,
            actual: actual_file_bytes,
        });
    }
    if file_bytes > limits.max_file_bytes {
        return Err(RawContextStoreError::FileLimit {
            actual: file_bytes,
            maximum: limits.max_file_bytes,
        });
    }

    let raw_len = get_u64(bytes, BYTE_LEN_OFFSET)?;
    if raw_len == 0 || raw_len > limits.max_raw_bytes {
        return Err(RawContextStoreError::RawLimit {
            actual: raw_len,
            maximum: limits.max_raw_bytes,
        });
    }
    let semantic_count = get_u32(bytes, SEMANTIC_COUNT_OFFSET)?;
    if semantic_count > limits.max_semantic_links {
        return Err(RawContextStoreError::SemanticLinkLimit {
            actual: semantic_count,
            maximum: limits.max_semantic_links,
        });
    }
    let semantic_offset = get_u64(bytes, SEMANTIC_OFFSET_OFFSET)?;
    if semantic_offset != u64::from(RAW_CONTEXT_HEADER_BYTES) {
        return Err(RawContextStoreError::NonCanonicalLayout);
    }
    let semantic_bytes = u64::from(semantic_count)
        .checked_mul(core::mem::size_of::<u64>() as u64)
        .ok_or(RawContextStoreError::Overflow)?;
    let expected_raw_offset = semantic_offset
        .checked_add(semantic_bytes)
        .ok_or(RawContextStoreError::Overflow)?;
    let raw_offset = get_u64(bytes, RAW_OFFSET_OFFSET)?;
    if raw_offset != expected_raw_offset {
        return Err(RawContextStoreError::NonCanonicalLayout);
    }
    let expected_file_bytes = raw_offset
        .checked_add(raw_len)
        .ok_or(RawContextStoreError::Overflow)?;
    if file_bytes != expected_file_bytes {
        return Err(RawContextStoreError::FileLengthMismatch {
            expected: expected_file_bytes,
            actual: file_bytes,
        });
    }

    let semantic_start =
        usize::try_from(semantic_offset).map_err(|_| RawContextStoreError::Overflow)?;
    let raw_start = usize::try_from(raw_offset).map_err(|_| RawContextStoreError::Overflow)?;
    let semantic_slice = bytes
        .get(semantic_start..raw_start)
        .ok_or(RawContextStoreError::TruncatedArtifact)?;
    let expected_semantic_crc = get_u32(bytes, SEMANTIC_CRC_OFFSET)?;
    let actual_semantic_crc = crc32c(semantic_slice);
    if expected_semantic_crc != actual_semantic_crc {
        return Err(RawContextStoreError::SemanticChecksumMismatch {
            expected: expected_semantic_crc,
            actual: actual_semantic_crc,
        });
    }

    let mut semantic_record_ids = Vec::with_capacity(semantic_count as usize);
    let mut previous = None;
    for chunk in semantic_slice.chunks_exact(8) {
        let semantic_id = u64::from_le_bytes(
            chunk
                .try_into()
                .map_err(|_| RawContextStoreError::TruncatedArtifact)?,
        );
        if previous.is_some_and(|value| value >= semantic_id) {
            return Err(RawContextStoreError::NonCanonicalSemanticIds);
        }
        previous = Some(semantic_id);
        semantic_record_ids.push(semantic_id);
    }

    let raw = bytes
        .get(raw_start..)
        .ok_or(RawContextStoreError::TruncatedArtifact)?;
    let expected_hash_bytes: [u8; 32] = bytes[CONTENT_HASH_OFFSET..CONTENT_HASH_OFFSET + 32]
        .try_into()
        .map_err(|_| RawContextStoreError::TruncatedArtifact)?;
    let expected_hash = ContextContentHash::from_sha256(expected_hash_bytes);
    let actual_hash = sha256_raw_bytes(raw)?;
    if actual_hash != expected_hash {
        return Err(RawContextStoreError::ContentHashMismatch {
            expected: expected_hash,
            actual: actual_hash,
        });
    }

    let stream = ContextStreamId::new(get_u64(bytes, STREAM_ID_OFFSET)?)?;
    let identity = ContextSegmentId::new(stream, get_u64(bytes, SEGMENT_ID_OFFSET)?)?;
    let reference = ContextSegmentRef::new(
        identity,
        get_u64(bytes, BYTE_START_OFFSET)?,
        raw_len,
        get_u64(bytes, TOKEN_START_OFFSET)?,
        get_u64(bytes, TOKEN_LEN_OFFSET)?,
        get_u64(bytes, ORDER_INDEX_OFFSET)?,
        expected_hash,
        semantic_record_ids,
    )?;
    Ok(RawContextSegment {
        reference,
        raw_bytes: raw.to_vec(),
    })
}

pub fn publish_raw_segment(
    path: impl AsRef<Path>,
    reference: &ContextSegmentRef,
    raw_bytes: &[u8],
) -> Result<(), RawContextStoreError> {
    let path = path.as_ref();
    if path.exists() {
        return Err(RawContextStoreError::TargetExists(path.to_path_buf()));
    }
    let parent = path.parent().ok_or(RawContextStoreError::MissingParent)?;
    let artifact = build_raw_segment_artifact(reference, raw_bytes)?;
    let (temp_path, mut file) = create_temp_file(parent)?;

    let publish_result = (|| -> Result<(), RawContextStoreError> {
        file.write_all(&artifact)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&temp_path, path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(RawContextStoreError::TargetExists(path.to_path_buf()));
            }
            Err(error) => return Err(error.into()),
        }
        fs::remove_file(&temp_path)?;
        Ok(())
    })();

    if publish_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    publish_result
}

pub fn load_raw_segment(
    path: impl AsRef<Path>,
    limits: RawContextStoreLimits,
) -> Result<RawContextSegment, RawContextStoreError> {
    let limits = limits.validate()?;
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(RawContextStoreError::NotRegularFile);
    }
    if metadata.len() > limits.max_file_bytes {
        return Err(RawContextStoreError::FileLimit {
            actual: metadata.len(),
            maximum: limits.max_file_bytes,
        });
    }
    let read_limit = limits.max_file_bytes.saturating_add(1);
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    let actual = u64::try_from(bytes.len()).map_err(|_| RawContextStoreError::Overflow)?;
    if actual > limits.max_file_bytes {
        return Err(RawContextStoreError::FileLimit {
            actual,
            maximum: limits.max_file_bytes,
        });
    }
    parse_raw_segment_artifact(&bytes, limits)
}

fn create_temp_file(parent: &Path) -> Result<(PathBuf, File), RawContextStoreError> {
    for _ in 0..32 {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(".sml-context-{}.{}.tmp", std::process::id(), id);
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(RawContextStoreError::TempNameExhausted)
}

fn sha256(bytes: &[u8]) -> Result<[u8; 32], RawContextStoreError> {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let len_u64 = u64::try_from(bytes.len()).map_err(|_| RawContextStoreError::Overflow)?;
    let bit_len = len_u64
        .checked_mul(8)
        .ok_or(RawContextStoreError::Overflow)?;
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut chunks = bytes.chunks_exact(64);
    for block in &mut chunks {
        sha256_compress(&mut state, block, &K);
    }

    let remainder = chunks.remainder();
    let mut tail = [0_u8; 128];
    tail[..remainder.len()].copy_from_slice(remainder);
    tail[remainder.len()] = 0x80;
    let padded_len = if remainder.len() < 56 { 64 } else { 128 };
    tail[padded_len - 8..padded_len].copy_from_slice(&bit_len.to_be_bytes());
    for block in tail[..padded_len].chunks_exact(64) {
        sha256_compress(&mut state, block, &K);
    }

    let mut output = [0_u8; 32];
    for (index, word) in state.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    Ok(output)
}

fn sha256_compress(state: &mut [u32; 8], block: &[u8], k: &[u32; 64]) {
    let mut schedule = [0_u32; 64];
    for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
        schedule[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for index in 0..64 {
        let big_s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(big_s1)
            .wrapping_add(choose)
            .wrapping_add(k[index])
            .wrapping_add(schedule[index]);
        let big_s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = big_s0.wrapping_add(majority);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
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

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), RawContextStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), RawContextStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), RawContextStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_bytes(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), RawContextStoreError> {
    let end = offset
        .checked_add(value.len())
        .ok_or(RawContextStoreError::Overflow)?;
    bytes
        .get_mut(offset..end)
        .ok_or(RawContextStoreError::TruncatedArtifact)?
        .copy_from_slice(value);
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, RawContextStoreError> {
    Ok(u16::from_le_bytes(get_array::<2>(bytes, offset)?))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, RawContextStoreError> {
    Ok(u32::from_le_bytes(get_array::<4>(bytes, offset)?))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, RawContextStoreError> {
    Ok(u64::from_le_bytes(get_array::<8>(bytes, offset)?))
}

fn get_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], RawContextStoreError> {
    let end = offset
        .checked_add(N)
        .ok_or(RawContextStoreError::Overflow)?;
    bytes
        .get(offset..end)
        .ok_or(RawContextStoreError::TruncatedArtifact)?
        .try_into()
        .map_err(|_| RawContextStoreError::TruncatedArtifact)
}

#[derive(Debug)]
#[non_exhaustive]
pub enum RawContextStoreError {
    Io(io::Error),
    Ledger(ContextLedgerError),
    InvalidLimits,
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidHeaderBytes(u16),
    UnsupportedFlags,
    TruncatedArtifact,
    Overflow,
    RawLengthMismatch {
        expected: u64,
        actual: u64,
    },
    ContentHashMismatch {
        expected: ContextContentHash,
        actual: ContextContentHash,
    },
    HeaderChecksumMismatch {
        expected: u32,
        actual: u32,
    },
    SemanticChecksumMismatch {
        expected: u32,
        actual: u32,
    },
    NonCanonicalSemanticIds,
    NonCanonicalLayout,
    FileLengthMismatch {
        expected: u64,
        actual: u64,
    },
    FileLimit {
        actual: u64,
        maximum: u64,
    },
    RawLimit {
        actual: u64,
        maximum: u64,
    },
    SemanticLinkLimit {
        actual: u32,
        maximum: u32,
    },
    NotRegularFile,
    MissingParent,
    TargetExists(PathBuf),
    TempNameExhausted,
}

impl fmt::Display for RawContextStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RawContextStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Ledger(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for RawContextStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ContextLedgerError> for RawContextStoreError {
    fn from(error: ContextLedgerError) -> Self {
        Self::Ledger(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn hex(bytes: &[u8]) -> String {
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use core::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }

    fn fixture(raw: &[u8]) -> ContextSegmentRef {
        let stream = ContextStreamId::new(7).unwrap();
        ContextSegmentRef::new(
            ContextSegmentId::new(stream, 11).unwrap(),
            0,
            raw.len() as u64,
            0,
            3,
            0,
            sha256_raw_bytes(raw).unwrap(),
            vec![41, 42],
        )
        .unwrap()
    }

    fn limits() -> RawContextStoreLimits {
        RawContextStoreLimits {
            max_file_bytes: 1 << 20,
            max_raw_bytes: 1 << 19,
            max_semantic_links: 64,
        }
    }

    fn test_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sml-context-raw-test-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            hex(sha256_raw_bytes(b"").unwrap().bytes()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(sha256_raw_bytes(b"abc").unwrap().bytes()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn artifact_round_trip_preserves_reference_and_raw_bytes() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let artifact = build_raw_segment_artifact(&reference, raw).unwrap();
        let parsed = parse_raw_segment_artifact(&artifact, limits()).unwrap();
        assert_eq!(parsed.reference, reference);
        assert_eq!(parsed.raw_bytes, raw);
    }

    #[test]
    fn raw_tamper_is_detected_by_sha256() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let mut artifact = build_raw_segment_artifact(&reference, raw).unwrap();
        let last = artifact.len() - 1;
        artifact[last] ^= 0x01;
        assert!(matches!(
            parse_raw_segment_artifact(&artifact, limits()),
            Err(RawContextStoreError::ContentHashMismatch { .. })
        ));
    }

    #[test]
    fn semantic_link_tamper_is_detected_by_crc32c() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let mut artifact = build_raw_segment_artifact(&reference, raw).unwrap();
        artifact[usize::from(RAW_CONTEXT_HEADER_BYTES)] ^= 0x01;
        assert!(matches!(
            parse_raw_segment_artifact(&artifact, limits()),
            Err(RawContextStoreError::SemanticChecksumMismatch { .. })
        ));
    }

    #[test]
    fn header_tamper_is_detected_before_payload_use() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let mut artifact = build_raw_segment_artifact(&reference, raw).unwrap();
        artifact[BYTE_START_OFFSET] ^= 0x01;
        assert!(matches!(
            parse_raw_segment_artifact(&artifact, limits()),
            Err(RawContextStoreError::HeaderChecksumMismatch { .. })
        ));
    }

    #[test]
    fn limits_are_independent_and_fail_closed() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let artifact = build_raw_segment_artifact(&reference, raw).unwrap();

        let mut raw_limit = limits();
        raw_limit.max_raw_bytes = 4;
        assert!(matches!(
            parse_raw_segment_artifact(&artifact, raw_limit),
            Err(RawContextStoreError::RawLimit { .. })
        ));

        let mut link_limit = limits();
        link_limit.max_semantic_links = 1;
        assert!(matches!(
            parse_raw_segment_artifact(&artifact, link_limit),
            Err(RawContextStoreError::SemanticLinkLimit { .. })
        ));
    }

    #[test]
    fn publish_is_immutable_and_load_reverifies_content() {
        let raw = b"alpha beta gamma";
        let reference = fixture(raw);
        let dir = test_dir();
        let path = dir.join("segment.smlraw");

        publish_raw_segment(&path, &reference, raw).unwrap();
        let loaded = load_raw_segment(&path, limits()).unwrap();
        assert_eq!(loaded.reference, reference);
        assert_eq!(loaded.raw_bytes, raw);

        assert!(matches!(
            publish_raw_segment(&path, &reference, raw),
            Err(RawContextStoreError::TargetExists(_))
        ));

        fs::remove_file(&path).unwrap();
        fs::remove_dir(&dir).unwrap();
    }
}
