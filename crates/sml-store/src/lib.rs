#![forbid(unsafe_code)]

pub mod raw_context;

//! Read-only, file-backed SBG storage for immutable Linux artifacts.
//!
//! The mapping and its cooperative shared lock stay alive together. No heap
//! fallback is accepted. Only requested records are decoded; opening validates
//! the index once. Mapped virtual bytes are NOT resident RAM measurements.
//!
//! # Artifact contract
//!
//! Never modify or truncate a mapped inode, including through another handle.
//! Publish a replacement file using atomic rename instead. Advisory locks do
//! not protect against non-cooperative writers; such writes can cause SIGBUS.
//! The unsafe OS boundary lives in mmap-guard/memmap2, not in sml-core. This
//! dependency boundary is not a proof of safety against arbitrary file mutation.

use std::{fmt, fs::File, io, path::Path};

use mmap_guard::FileData;
use sml_core::semantic::{
    store_format::SemanticStoreDescriptor, SemanticState, SemanticStoreError, SemanticStoreHeader,
    SEMANTIC_STORE_HEADER_BYTES, SEMANTIC_STORE_INDEX_ENTRY_BYTES, SEMANTIC_STORE_PAGE_ALIGNMENT,
};

/// Explicit admission limits; virtual mapping and decoded payload are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedStoreLimits {
    pub max_file_bytes: u64,
    pub max_records: u64,
    pub max_decoded_record_bytes: u64,
}

/// Cumulative logical accounting, not allocator/RSS or physical disk-I/O counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MappedStoreStats {
    pub mapped_file_bytes: u64,
    pub validated_index_entries: u64,
    pub decoded_records: u64,
    pub decoded_payload_bytes: u64,
    pub missing_lookups: u64,
    pub failed_lookups: u64,
}

/// Owns the mapping, its lock and the descriptor validated for these same bytes.
#[derive(Debug)]
pub struct MappedSemanticStore<const BITS: usize> {
    data: FileData,
    descriptor: SemanticStoreDescriptor<BITS>,
    stats: MappedStoreStats,
}

impl<const BITS: usize> MappedSemanticStore<BITS> {
    /// Opens an immutable regular file. The initial backend is Linux-only.
    ///
    /// Uses /proc/self/fd to map the already-open file rather than reopen a
    /// replaceable pathname. Fails closed if procfs or the mapping is unavailable.
    pub fn open(
        path: impl AsRef<Path>,
        limits: MappedStoreLimits,
    ) -> Result<Self, MappedStoreError> {
        Self::from_file(File::open(path)?, limits)
    }

    fn from_file(file: File, limits: MappedStoreLimits) -> Result<Self, MappedStoreError> {
        if limits.max_file_bytes < u64::from(SEMANTIC_STORE_HEADER_BYTES)
            || limits.max_decoded_record_bytes == 0
        {
            return Err(MappedStoreError::InvalidLimits);
        }
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(MappedStoreError::NotRegularFile);
        }
        validate_file_size(metadata.len(), limits.max_file_bytes)?;
        let data = map_open_file(&file)?;
        if !matches!(&data, FileData::Mapped(..)) {
            return Err(MappedStoreError::MappingRequired);
        }
        let file_bytes = u64::try_from(data.len())
            .map_err(|_| MappedStoreError::Format(SemanticStoreError::AddressSpaceOverflow))?;
        validate_file_size(file_bytes, limits.max_file_bytes)?;
        if file.metadata()?.len() != file_bytes {
            return Err(MappedStoreError::FileLengthChanged);
        }

        // Bound index validation before its O(record_count) scan. No payloads
        // are decoded here. The descriptor subsequently checks the header CRC.
        let raw_count: [u8; 8] = data[24..32]
            .try_into()
            .map_err(|_| MappedStoreError::Format(SemanticStoreError::TruncatedFile))?;
        let count = u64::from_le_bytes(raw_count);
        if count > limits.max_records {
            return Err(MappedStoreError::RecordLimit {
                actual: count,
                maximum: limits.max_records,
            });
        }
        let descriptor = SemanticStoreDescriptor::<BITS>::validate(&data)?;
        let header = descriptor.header();
        validate_canonical_layout::<BITS>(header)?;
        if u64::from(header.payload_bytes) > limits.max_decoded_record_bytes {
            return Err(MappedStoreError::PayloadLimit {
                actual: u64::from(header.payload_bytes),
                maximum: limits.max_decoded_record_bytes,
            });
        }
        Ok(Self {
            data,
            descriptor,
            stats: MappedStoreStats {
                mapped_file_bytes: file_bytes,
                validated_index_entries: header.record_count,
                ..MappedStoreStats::default()
            },
        })
    }

    #[must_use]
    pub fn header(&self) -> SemanticStoreHeader {
        self.descriptor.header()
    }

    #[must_use]
    pub const fn stats(&self) -> MappedStoreStats {
        self.stats
    }

    /// Binary-searches the resident/mapped index and verifies one requested record.
    ///
    /// Returned states are owned and may outlive this mapping. Caller-retained
    /// states are outside this reader's budget. No decoded-state cache is kept.
    pub fn get(
        &mut self,
        semantic_id: u64,
    ) -> Result<Option<SemanticState<BITS>>, MappedStoreError> {
        match self.descriptor.get(&self.data, semantic_id) {
            Ok(Some(state)) => {
                self.stats.decoded_records = self.stats.decoded_records.saturating_add(1);
                self.stats.decoded_payload_bytes = self
                    .stats
                    .decoded_payload_bytes
                    .saturating_add(u64::from(self.descriptor.header().payload_bytes));
                Ok(Some(state))
            }
            Ok(None) => {
                self.stats.missing_lookups = self.stats.missing_lookups.saturating_add(1);
                Ok(None)
            }
            Err(error) => {
                self.stats.failed_lookups = self.stats.failed_lookups.saturating_add(1);
                Err(error.into())
            }
        }
    }
}

fn validate_file_size(actual: u64, maximum: u64) -> Result<(), MappedStoreError> {
    if actual < u64::from(SEMANTIC_STORE_HEADER_BYTES) {
        return Err(SemanticStoreError::TruncatedFile.into());
    }
    if actual > maximum || actual > isize::MAX as u64 {
        return Err(MappedStoreError::FileLimit { actual, maximum });
    }
    Ok(())
}

fn validate_canonical_layout<const BITS: usize>(
    header: SemanticStoreHeader,
) -> Result<(), MappedStoreError> {
    if BITS == 0 {
        return Err(SemanticStoreError::ZeroWidth.into());
    }
    let alignment = SEMANTIC_STORE_PAGE_ALIGNMENT;
    let index_end = header
        .record_count
        .checked_mul(u64::from(SEMANTIC_STORE_INDEX_ENTRY_BYTES))
        .and_then(|bytes| bytes.checked_add(alignment))
        .ok_or(SemanticStoreError::LayoutOverflow)?;
    let records_offset = align_up(index_end, alignment)?;
    let stride = align_up(u64::from(header.payload_bytes), alignment)?;
    if header.index_offset != alignment
        || header.records_offset != records_offset
        || header.record_stride != stride
        || stride == 0
    {
        return Err(MappedStoreError::NonCanonicalLayout);
    }
    Ok(())
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

#[cfg(target_os = "linux")]
fn map_open_file(file: &File) -> Result<FileData, MappedStoreError> {
    use std::os::fd::AsRawFd;
    // file stays live until mmap-guard owns a distinct read-only handle and lock.
    Ok(mmap_guard::map_file(format!(
        "/proc/self/fd/{}",
        file.as_raw_fd()
    ))?)
}

#[cfg(not(target_os = "linux"))]
fn map_open_file(_file: &File) -> Result<FileData, MappedStoreError> {
    Err(MappedStoreError::UnsupportedPlatform)
}

#[derive(Debug)]
pub enum MappedStoreError {
    Io(io::Error),
    Format(SemanticStoreError),
    InvalidLimits,
    NotRegularFile,
    MappingRequired,
    UnsupportedPlatform,
    NonCanonicalLayout,
    FileLengthChanged,
    FileLimit { actual: u64, maximum: u64 },
    RecordLimit { actual: u64, maximum: u64 },
    PayloadLimit { actual: u64, maximum: u64 },
}

impl fmt::Display for MappedStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MappedStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for MappedStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SemanticStoreError> for MappedStoreError {
    fn from(error: SemanticStoreError) -> Self {
        Self::Format(error)
    }
}
