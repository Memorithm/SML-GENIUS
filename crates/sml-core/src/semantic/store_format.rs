//! SBG-0.13a versioned, checksummed, mmap-friendly semantic-store format.
//!
//! The parser borrows a byte slice and validates only header/index structure up
//! front. Record payload CRC32C and V/M/A/R reconstruction happen on demand, so
//! a later read-only mmap runtime can use the same format without rebuilding the
//! full semantic dictionary in RAM.

use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError, SemanticState};

pub const SEMANTIC_STORE_MAGIC: [u8; 8] = *b"SMLSBG13";
pub const SEMANTIC_STORE_VERSION: u16 = 1;
pub const SEMANTIC_STORE_PAGE_ALIGNMENT: u64 = 4_096;
pub const SEMANTIC_STORE_HEADER_BYTES: u16 = 64;
pub const SEMANTIC_STORE_INDEX_ENTRY_BYTES: u32 = 24;

const FIELD_COUNT: u16 = 4;
const WORD_BYTES: u16 = 8;
const INDEX_OFFSET: u64 = SEMANTIC_STORE_PAGE_ALIGNMENT;
const CRC32C_POLYNOMIAL: u32 = 0x82f63b78;

/// Parsed fixed header for one semantic-store image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticStoreHeader {
    pub version: u16,
    pub logical_bits: u32,
    pub record_count: u64,
    pub index_offset: u64,
    pub records_offset: u64,
    pub record_stride: u64,
    pub payload_bytes: u32,
}

/// One fixed-size on-disk index entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticStoreIndexEntry {
    pub semantic_id: u64,
    pub record_offset: u64,
    pub payload_crc32c: u32,
    pub payload_bytes: u32,
}

/// Borrowed zero-copy view over semantic-store metadata and record bytes.
///
/// Parsing does not decode all records. The get method verifies and reconstructs
/// only the requested V/M/A/R state.
#[derive(Debug, Clone, Copy)]
pub struct SemanticStoreView<'a, const BITS: usize> {
    bytes: &'a [u8],
    header: SemanticStoreHeader,
}

impl<'a, const BITS: usize> SemanticStoreView<'a, BITS> {
    /// Parses and validates header/index structure without touching every payload.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SemanticStoreError> {
        let header = parse_header::<BITS>(bytes)?;
        validate_index(bytes, header)?;
        Ok(Self { bytes, header })
    }

    #[must_use]
    pub const fn header(&self) -> SemanticStoreHeader {
        self.header
    }

    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.header.record_count
    }

    /// Reads one index entry without allocating.
    pub fn index_entry(
        &self,
        position: u64,
    ) -> Result<SemanticStoreIndexEntry, SemanticStoreError> {
        if position >= self.header.record_count {
            return Err(SemanticStoreError::IndexOutOfRange {
                position,
                record_count: self.header.record_count,
            });
        }
        read_index_entry(self.bytes, self.header, position)
    }

    /// Binary-searches the sorted mmap-friendly index and decodes one record.
    pub fn get(&self, semantic_id: u64) -> Result<Option<SemanticState<BITS>>, SemanticStoreError> {
        let mut low = 0_u64;
        let mut high = self.header.record_count;

        while low < high {
            let mid = low + (high - low) / 2;
            let entry = self.index_entry(mid)?;
            if entry.semantic_id < semantic_id {
                low = mid + 1;
            } else if entry.semantic_id > semantic_id {
                high = mid;
            } else {
                return decode_record::<BITS>(self.bytes, entry).map(Some);
            }
        }

        Ok(None)
    }
}

/// Builds the deterministic reference image for a semantic dictionary.
///
/// Records are sorted by semantic ID in the image. Duplicate IDs fail closed.
/// The read path itself is slice-backed and does not require whole-store
/// reconstruction.
pub fn build_semantic_store<const BITS: usize>(
    records: &[(u64, SemanticState<BITS>)],
) -> Result<Vec<u8>, SemanticStoreError> {
    let record_count =
        u64::try_from(records.len()).map_err(|_| SemanticStoreError::LayoutOverflow)?;
    let layout = store_layout::<BITS>(record_count)?;
    let file_len =
        usize::try_from(layout.file_bytes).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
    let mut bytes = vec![0_u8; file_len];

    let mut ordered: Vec<(u64, &SemanticState<BITS>)> =
        records.iter().map(|(id, state)| (*id, state)).collect();
    ordered.sort_unstable_by_key(|(id, _)| *id);

    for pair in ordered.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(SemanticStoreError::DuplicateSemanticId(pair[0].0));
        }
    }

    write_header::<BITS>(&mut bytes, layout)?;

    for (position, (semantic_id, state)) in ordered.into_iter().enumerate() {
        let position =
            u64::try_from(position).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
        let record_offset = layout
            .header
            .records_offset
            .checked_add(
                position
                    .checked_mul(layout.header.record_stride)
                    .ok_or(SemanticStoreError::LayoutOverflow)?,
            )
            .ok_or(SemanticStoreError::LayoutOverflow)?;
        let payload = encode_state(state);
        if payload.len() != layout.header.payload_bytes as usize {
            return Err(SemanticStoreError::PayloadLength {
                expected: layout.header.payload_bytes,
                actual: u32::try_from(payload.len())
                    .map_err(|_| SemanticStoreError::LayoutOverflow)?,
            });
        }
        let start =
            usize::try_from(record_offset).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
        let end = start
            .checked_add(payload.len())
            .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
        bytes
            .get_mut(start..end)
            .ok_or(SemanticStoreError::TruncatedFile)?
            .copy_from_slice(&payload);

        let entry = SemanticStoreIndexEntry {
            semantic_id,
            record_offset,
            payload_crc32c: crc32c(&payload),
            payload_bytes: layout.header.payload_bytes,
        };
        write_index_entry(&mut bytes, layout.header, position, entry)?;
    }

    Ok(bytes)
}

#[derive(Debug, Clone, Copy)]
struct StoreLayout {
    header: SemanticStoreHeader,
    file_bytes: u64,
}

fn store_layout<const BITS: usize>(record_count: u64) -> Result<StoreLayout, SemanticStoreError> {
    if BITS == 0 {
        return Err(SemanticStoreError::ZeroWidth);
    }
    let logical_bits =
        u32::try_from(BITS).map_err(|_| SemanticStoreError::DimensionTooLarge(BITS))?;
    let words_per_field = BITS.div_ceil(64);
    let payload_usize = words_per_field
        .checked_mul(WORD_BYTES as usize)
        .and_then(|value| value.checked_mul(FIELD_COUNT as usize))
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let payload_bytes =
        u32::try_from(payload_usize).map_err(|_| SemanticStoreError::LayoutOverflow)?;

    let index_bytes = record_count
        .checked_mul(u64::from(SEMANTIC_STORE_INDEX_ENTRY_BYTES))
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let records_offset = align_up(
        INDEX_OFFSET
            .checked_add(index_bytes)
            .ok_or(SemanticStoreError::LayoutOverflow)?,
        SEMANTIC_STORE_PAGE_ALIGNMENT,
    )?;
    let record_stride = align_up(u64::from(payload_bytes), SEMANTIC_STORE_PAGE_ALIGNMENT)?;
    let file_bytes = records_offset
        .checked_add(
            record_count
                .checked_mul(record_stride)
                .ok_or(SemanticStoreError::LayoutOverflow)?,
        )
        .ok_or(SemanticStoreError::LayoutOverflow)?;

    Ok(StoreLayout {
        header: SemanticStoreHeader {
            version: SEMANTIC_STORE_VERSION,
            logical_bits,
            record_count,
            index_offset: INDEX_OFFSET,
            records_offset,
            record_stride,
            payload_bytes,
        },
        file_bytes,
    })
}

fn write_header<const BITS: usize>(
    bytes: &mut [u8],
    layout: StoreLayout,
) -> Result<(), SemanticStoreError> {
    let header = bytes
        .get_mut(..SEMANTIC_STORE_HEADER_BYTES as usize)
        .ok_or(SemanticStoreError::TruncatedFile)?;
    header.fill(0);
    header[0..8].copy_from_slice(&SEMANTIC_STORE_MAGIC);
    put_u16(header, 8, SEMANTIC_STORE_VERSION)?;
    put_u16(header, 10, SEMANTIC_STORE_HEADER_BYTES)?;
    put_u32(header, 12, layout.header.logical_bits)?;
    put_u16(header, 16, FIELD_COUNT)?;
    put_u16(header, 18, WORD_BYTES)?;
    put_u32(header, 20, SEMANTIC_STORE_INDEX_ENTRY_BYTES)?;
    put_u64(header, 24, layout.header.record_count)?;
    put_u64(header, 32, layout.header.index_offset)?;
    put_u64(header, 40, layout.header.records_offset)?;
    put_u64(header, 48, layout.header.record_stride)?;
    let checksum = crc32c(&header[..56]);
    put_u32(header, 56, checksum)?;
    put_u32(header, 60, 0)?;

    let parsed = parse_header::<BITS>(bytes)?;
    if parsed != layout.header {
        return Err(SemanticStoreError::HeaderRoundTrip);
    }
    Ok(())
}

fn parse_header<const BITS: usize>(
    bytes: &[u8],
) -> Result<SemanticStoreHeader, SemanticStoreError> {
    let header = bytes
        .get(..SEMANTIC_STORE_HEADER_BYTES as usize)
        .ok_or(SemanticStoreError::TruncatedFile)?;
    if header[0..8] != SEMANTIC_STORE_MAGIC {
        return Err(SemanticStoreError::InvalidMagic);
    }

    let version = get_u16(header, 8)?;
    if version != SEMANTIC_STORE_VERSION {
        return Err(SemanticStoreError::UnsupportedVersion(version));
    }
    let header_bytes = get_u16(header, 10)?;
    if header_bytes != SEMANTIC_STORE_HEADER_BYTES {
        return Err(SemanticStoreError::InvalidHeaderBytes(header_bytes));
    }

    let logical_bits = get_u32(header, 12)?;
    if logical_bits as usize != BITS {
        return Err(SemanticStoreError::DimensionMismatch {
            expected: BITS,
            actual: logical_bits,
        });
    }
    if get_u16(header, 16)? != FIELD_COUNT {
        return Err(SemanticStoreError::InvalidFieldCount);
    }
    if get_u16(header, 18)? != WORD_BYTES {
        return Err(SemanticStoreError::InvalidWordBytes);
    }
    if get_u32(header, 20)? != SEMANTIC_STORE_INDEX_ENTRY_BYTES {
        return Err(SemanticStoreError::InvalidIndexEntryBytes);
    }
    let expected_crc = get_u32(header, 56)?;
    let actual_crc = crc32c(&header[..56]);
    if expected_crc != actual_crc {
        return Err(SemanticStoreError::HeaderChecksumMismatch {
            expected: expected_crc,
            actual: actual_crc,
        });
    }
    if get_u32(header, 60)? != 0 {
        return Err(SemanticStoreError::UnsupportedFlags);
    }

    let record_count = get_u64(header, 24)?;
    let index_offset = get_u64(header, 32)?;
    let records_offset = get_u64(header, 40)?;
    let record_stride = get_u64(header, 48)?;
    if index_offset != INDEX_OFFSET
        || index_offset % SEMANTIC_STORE_PAGE_ALIGNMENT != 0
        || records_offset % SEMANTIC_STORE_PAGE_ALIGNMENT != 0
        || record_stride % SEMANTIC_STORE_PAGE_ALIGNMENT != 0
    {
        return Err(SemanticStoreError::InvalidAlignment);
    }

    let words_per_field = BITS.div_ceil(64);
    let payload_bytes = words_per_field
        .checked_mul(WORD_BYTES as usize)
        .and_then(|value| value.checked_mul(FIELD_COUNT as usize))
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(SemanticStoreError::LayoutOverflow)?;

    Ok(SemanticStoreHeader {
        version,
        logical_bits,
        record_count,
        index_offset,
        records_offset,
        record_stride,
        payload_bytes,
    })
}

fn validate_index(bytes: &[u8], header: SemanticStoreHeader) -> Result<(), SemanticStoreError> {
    let index_bytes = header
        .record_count
        .checked_mul(u64::from(SEMANTIC_STORE_INDEX_ENTRY_BYTES))
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let index_end = header
        .index_offset
        .checked_add(index_bytes)
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    if index_end > header.records_offset {
        return Err(SemanticStoreError::IndexOverlapsRecords);
    }

    let file_len =
        u64::try_from(bytes.len()).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
    let expected_file_len = header
        .records_offset
        .checked_add(
            header
                .record_count
                .checked_mul(header.record_stride)
                .ok_or(SemanticStoreError::LayoutOverflow)?,
        )
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    if file_len != expected_file_len {
        return Err(SemanticStoreError::FileLength {
            expected: expected_file_len,
            actual: file_len,
        });
    }

    let mut previous_id = None;
    for position in 0..header.record_count {
        let entry = read_index_entry(bytes, header, position)?;
        if entry.payload_bytes != header.payload_bytes {
            return Err(SemanticStoreError::PayloadLength {
                expected: header.payload_bytes,
                actual: entry.payload_bytes,
            });
        }
        if entry.record_offset % SEMANTIC_STORE_PAGE_ALIGNMENT != 0 {
            return Err(SemanticStoreError::InvalidAlignment);
        }
        let expected_offset = header
            .records_offset
            .checked_add(
                position
                    .checked_mul(header.record_stride)
                    .ok_or(SemanticStoreError::LayoutOverflow)?,
            )
            .ok_or(SemanticStoreError::LayoutOverflow)?;
        if entry.record_offset != expected_offset {
            return Err(SemanticStoreError::InvalidRecordOffset {
                expected: expected_offset,
                actual: entry.record_offset,
            });
        }
        if previous_id.is_some_and(|id| id >= entry.semantic_id) {
            return Err(SemanticStoreError::UnsortedIndex);
        }
        previous_id = Some(entry.semantic_id);
    }

    Ok(())
}

fn write_index_entry(
    bytes: &mut [u8],
    header: SemanticStoreHeader,
    position: u64,
    entry: SemanticStoreIndexEntry,
) -> Result<(), SemanticStoreError> {
    let offset = header
        .index_offset
        .checked_add(
            position
                .checked_mul(u64::from(SEMANTIC_STORE_INDEX_ENTRY_BYTES))
                .ok_or(SemanticStoreError::LayoutOverflow)?,
        )
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let start = usize::try_from(offset).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
    let end = start
        .checked_add(SEMANTIC_STORE_INDEX_ENTRY_BYTES as usize)
        .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
    let target = bytes
        .get_mut(start..end)
        .ok_or(SemanticStoreError::TruncatedFile)?;
    put_u64(target, 0, entry.semantic_id)?;
    put_u64(target, 8, entry.record_offset)?;
    put_u32(target, 16, entry.payload_crc32c)?;
    put_u32(target, 20, entry.payload_bytes)?;
    Ok(())
}

fn read_index_entry(
    bytes: &[u8],
    header: SemanticStoreHeader,
    position: u64,
) -> Result<SemanticStoreIndexEntry, SemanticStoreError> {
    let offset = header
        .index_offset
        .checked_add(
            position
                .checked_mul(u64::from(SEMANTIC_STORE_INDEX_ENTRY_BYTES))
                .ok_or(SemanticStoreError::LayoutOverflow)?,
        )
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let start = usize::try_from(offset).map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
    let end = start
        .checked_add(SEMANTIC_STORE_INDEX_ENTRY_BYTES as usize)
        .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
    let source = bytes
        .get(start..end)
        .ok_or(SemanticStoreError::TruncatedFile)?;

    Ok(SemanticStoreIndexEntry {
        semantic_id: get_u64(source, 0)?,
        record_offset: get_u64(source, 8)?,
        payload_crc32c: get_u32(source, 16)?,
        payload_bytes: get_u32(source, 20)?,
    })
}

fn encode_state<const BITS: usize>(state: &SemanticState<BITS>) -> Vec<u8> {
    let fields = [
        &state.value,
        &state.meaning_mask,
        &state.ambiguity,
        &state.routing,
    ];
    let mut bytes =
        Vec::with_capacity(BITS.div_ceil(64) * WORD_BYTES as usize * FIELD_COUNT as usize);
    for field in fields {
        for word in field.words() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes
}

fn decode_record<const BITS: usize>(
    bytes: &[u8],
    entry: SemanticStoreIndexEntry,
) -> Result<SemanticState<BITS>, SemanticStoreError> {
    let start = usize::try_from(entry.record_offset)
        .map_err(|_| SemanticStoreError::AddressSpaceOverflow)?;
    let payload_len = entry.payload_bytes as usize;
    let end = start
        .checked_add(payload_len)
        .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
    let payload = bytes
        .get(start..end)
        .ok_or(SemanticStoreError::TruncatedFile)?;
    let actual_crc = crc32c(payload);
    if actual_crc != entry.payload_crc32c {
        return Err(SemanticStoreError::RecordChecksumMismatch {
            semantic_id: entry.semantic_id,
            expected: entry.payload_crc32c,
            actual: actual_crc,
        });
    }

    let words_per_field = BITS.div_ceil(64);
    let field_bytes = words_per_field * WORD_BYTES as usize;
    if payload.len() != field_bytes * FIELD_COUNT as usize {
        return Err(SemanticStoreError::PayloadLength {
            expected: u32::try_from(field_bytes * FIELD_COUNT as usize)
                .map_err(|_| SemanticStoreError::LayoutOverflow)?,
            actual: u32::try_from(payload.len()).map_err(|_| SemanticStoreError::LayoutOverflow)?,
        });
    }

    let mut fields = Vec::with_capacity(FIELD_COUNT as usize);
    for field_index in 0..FIELD_COUNT as usize {
        let field_start = field_index * field_bytes;
        let field_end = field_start + field_bytes;
        let source = &payload[field_start..field_end];
        let mut words = Vec::with_capacity(words_per_field);
        for chunk in source.chunks_exact(WORD_BYTES as usize) {
            let raw: [u8; 8] = chunk
                .try_into()
                .map_err(|_| SemanticStoreError::TruncatedFile)?;
            words.push(u64::from_le_bytes(raw));
        }
        fields.push(SemanticBitField::<BITS>::from_words(words)?);
    }

    let mut fields = fields.into_iter();
    Ok(SemanticState {
        value: fields.next().ok_or(SemanticStoreError::TruncatedFile)?,
        meaning_mask: fields.next().ok_or(SemanticStoreError::TruncatedFile)?,
        ambiguity: fields.next().ok_or(SemanticStoreError::TruncatedFile)?,
        routing: fields.next().ok_or(SemanticStoreError::TruncatedFile)?,
    })
}

fn align_up(value: u64, alignment: u64) -> Result<u64, SemanticStoreError> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(SemanticStoreError::LayoutOverflow)
    }
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

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), SemanticStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), SemanticStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), SemanticStoreError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_bytes(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), SemanticStoreError> {
    let end = offset
        .checked_add(value.len())
        .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
    bytes
        .get_mut(offset..end)
        .ok_or(SemanticStoreError::TruncatedFile)?
        .copy_from_slice(value);
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, SemanticStoreError> {
    Ok(u16::from_le_bytes(get_array::<2>(bytes, offset)?))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, SemanticStoreError> {
    Ok(u32::from_le_bytes(get_array::<4>(bytes, offset)?))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, SemanticStoreError> {
    Ok(u64::from_le_bytes(get_array::<8>(bytes, offset)?))
}

fn get_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], SemanticStoreError> {
    let end = offset
        .checked_add(N)
        .ok_or(SemanticStoreError::AddressSpaceOverflow)?;
    bytes
        .get(offset..end)
        .ok_or(SemanticStoreError::TruncatedFile)?
        .try_into()
        .map_err(|_| SemanticStoreError::TruncatedFile)
}

/// Structural or corruption error in the SBG semantic-store format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticStoreError {
    ZeroWidth,
    DimensionTooLarge(usize),
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidHeaderBytes(u16),
    DimensionMismatch {
        expected: usize,
        actual: u32,
    },
    InvalidFieldCount,
    InvalidWordBytes,
    InvalidIndexEntryBytes,
    UnsupportedFlags,
    HeaderChecksumMismatch {
        expected: u32,
        actual: u32,
    },
    HeaderRoundTrip,
    DuplicateSemanticId(u64),
    UnsortedIndex,
    IndexOutOfRange {
        position: u64,
        record_count: u64,
    },
    IndexOverlapsRecords,
    InvalidAlignment,
    InvalidRecordOffset {
        expected: u64,
        actual: u64,
    },
    PayloadLength {
        expected: u32,
        actual: u32,
    },
    RecordChecksumMismatch {
        semantic_id: u64,
        expected: u32,
        actual: u32,
    },
    FileLength {
        expected: u64,
        actual: u64,
    },
    LayoutOverflow,
    AddressSpaceOverflow,
    TruncatedFile,
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticStoreError {}

impl From<SemanticBitFieldError> for SemanticStoreError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::crc32c;

    #[test]
    fn crc32c_matches_castagnoli_check_vector() {
        assert_eq!(crc32c(b"123456789"), 0xe306_9283);
    }
}
