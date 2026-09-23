//! CONTEXT-1d crash-recoverable multi-artifact bundle journal.
//!
//! This Linux-oriented reference publishes an immutable journal before linking
//! final artifact names. Recovery validates length + SHA-256 for every existing
//! final artifact before mutating anything. A mismatched/foreign final file
//! blocks recovery and is never deleted.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::raw_context::{sha256_raw_bytes, RawContextStoreError};

pub const CONTEXT_BUNDLE_JOURNAL_MAGIC: [u8; 8] = *b"SMLTXN1D";
pub const CONTEXT_BUNDLE_JOURNAL_VERSION: u16 = 1;
pub const CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES: usize = 32;
const RECORD_FIXED_BYTES: usize = 2 + 8 + 32;
const CRC32C_POLYNOMIAL: u32 = 0x82f6_3b78;
static TXN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleJournalPolicy {
    pub max_artifacts: usize,
    pub max_artifact_bytes: u64,
    pub max_journal_bytes: u64,
}

impl Default for BundleJournalPolicy {
    fn default() -> Self {
        Self {
            max_artifacts: 8,
            max_artifact_bytes: 64 * 1024 * 1024,
            max_journal_bytes: 64 * 1024,
        }
    }
}

impl BundleJournalPolicy {
    pub fn validate(self) -> Result<Self, BundleJournalError> {
        if self.max_artifacts == 0 || self.max_artifact_bytes == 0 || self.max_journal_bytes == 0 {
            return Err(BundleJournalError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleArtifact {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

impl BundleArtifact {
    pub fn new(file_name: impl Into<String>, bytes: Vec<u8>) -> Result<Self, BundleJournalError> {
        let artifact = Self {
            file_name: file_name.into(),
            bytes,
        };
        validate_file_name(&artifact.file_name)?;
        Ok(artifact)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleJournalEntry {
    pub file_name: String,
    pub byte_len: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleJournal {
    pub transaction_id: u64,
    pub entries: Vec<BundleJournalEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleRecoveryAction {
    NoJournal,
    FinalizedCompleteBundle,
    RolledBackPartialBundle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournaledBundlePublisher {
    directory: PathBuf,
    policy: BundleJournalPolicy,
}

impl JournaledBundlePublisher {
    pub fn new(
        directory: impl AsRef<Path>,
        policy: BundleJournalPolicy,
    ) -> Result<Self, BundleJournalError> {
        let policy = policy.validate()?;
        let directory = directory.as_ref().to_path_buf();
        if !fs::metadata(&directory)?.is_dir() {
            return Err(BundleJournalError::OutputNotDirectory);
        }
        Ok(Self { directory, policy })
    }

    pub fn publish(
        &self,
        journal_file_name: &str,
        artifacts: Vec<BundleArtifact>,
    ) -> Result<BundleJournal, BundleJournalError> {
        validate_file_name(journal_file_name)?;
        if artifacts.is_empty() || artifacts.len() > self.policy.max_artifacts {
            return Err(BundleJournalError::ArtifactCountBudget);
        }
        let mut names = std::collections::BTreeSet::new();
        for artifact in &artifacts {
            validate_file_name(&artifact.file_name)?;
            if artifact.file_name == journal_file_name || !names.insert(artifact.file_name.as_str()) {
                return Err(BundleJournalError::DuplicateFileName);
            }
            let len = u64::try_from(artifact.bytes.len()).map_err(|_| BundleJournalError::Overflow)?;
            if len > self.policy.max_artifact_bytes {
                return Err(BundleJournalError::ArtifactByteBudget {
                    file_name: artifact.file_name.clone(),
                    actual: len,
                    maximum: self.policy.max_artifact_bytes,
                });
            }
            if self.directory.join(&artifact.file_name).exists() {
                return Err(BundleJournalError::TargetExists(artifact.file_name.clone()));
            }
        }
        let journal_path = self.directory.join(journal_file_name);
        if journal_path.exists() {
            return Err(BundleJournalError::JournalExists);
        }

        let transaction_id = next_transaction_id();
        let entries = artifacts
            .iter()
            .map(|artifact| {
                Ok(BundleJournalEntry {
                    file_name: artifact.file_name.clone(),
                    byte_len: u64::try_from(artifact.bytes.len())
                        .map_err(|_| BundleJournalError::Overflow)?,
                    sha256: sha256_raw_bytes(&artifact.bytes)?.into_bytes(),
                })
            })
            .collect::<Result<Vec<_>, BundleJournalError>>()?;
        let journal = BundleJournal {
            transaction_id,
            entries,
        };
        let journal_bytes = build_bundle_journal(&journal)?;
        let journal_len =
            u64::try_from(journal_bytes.len()).map_err(|_| BundleJournalError::Overflow)?;
        if journal_len > self.policy.max_journal_bytes {
            return Err(BundleJournalError::JournalByteBudget);
        }

        publish_immutable_file(&self.directory, journal_file_name, &journal_bytes, transaction_id, 0)?;
        sync_directory(&self.directory)?;

        for (index, artifact) in artifacts.iter().enumerate() {
            let ordinal = u64::try_from(index)
                .map_err(|_| BundleJournalError::Overflow)?
                .checked_add(1)
                .ok_or(BundleJournalError::Overflow)?;
            if let Err(error) = publish_immutable_file(
                &self.directory,
                &artifact.file_name,
                &artifact.bytes,
                transaction_id,
                ordinal,
            ) {
                // Leave journal + any linked finals in place for deterministic recovery.
                return Err(error);
            }
            sync_directory(&self.directory)?;
        }

        fs::remove_file(&journal_path)?;
        sync_directory(&self.directory)?;
        Ok(journal)
    }

    pub fn recover(
        &self,
        journal_file_name: &str,
    ) -> Result<BundleRecoveryAction, BundleJournalError> {
        validate_file_name(journal_file_name)?;
        let journal_path = self.directory.join(journal_file_name);
        if !journal_path.exists() {
            return Ok(BundleRecoveryAction::NoJournal);
        }
        let metadata = fs::metadata(&journal_path)?;
        if metadata.len() > self.policy.max_journal_bytes {
            return Err(BundleJournalError::JournalByteBudget);
        }
        let bytes = fs::read(&journal_path)?;
        let journal = parse_bundle_journal(&bytes)?;
        if journal.entries.is_empty() || journal.entries.len() > self.policy.max_artifacts {
            return Err(BundleJournalError::ArtifactCountBudget);
        }

        let mut present = Vec::with_capacity(journal.entries.len());
        for entry in &journal.entries {
            validate_file_name(&entry.file_name)?;
            if entry.byte_len > self.policy.max_artifact_bytes {
                return Err(BundleJournalError::ArtifactByteBudget {
                    file_name: entry.file_name.clone(),
                    actual: entry.byte_len,
                    maximum: self.policy.max_artifact_bytes,
                });
            }
            let path = self.directory.join(&entry.file_name);
            if !path.exists() {
                present.push(false);
                continue;
            }
            let metadata = fs::metadata(&path)?;
            if metadata.len() != entry.byte_len {
                return Err(BundleJournalError::ForeignOrModifiedArtifact(
                    entry.file_name.clone(),
                ));
            }
            let content = fs::read(&path)?;
            let digest = sha256_raw_bytes(&content)?.into_bytes();
            if digest != entry.sha256 {
                return Err(BundleJournalError::ForeignOrModifiedArtifact(
                    entry.file_name.clone(),
                ));
            }
            present.push(true);
        }

        if present.iter().all(|value| *value) {
            fs::remove_file(&journal_path)?;
            remove_transaction_temps(&self.directory, journal.transaction_id)?;
            sync_directory(&self.directory)?;
            return Ok(BundleRecoveryAction::FinalizedCompleteBundle);
        }

        for (entry, is_present) in journal.entries.iter().zip(&present) {
            if *is_present {
                fs::remove_file(self.directory.join(&entry.file_name))?;
            }
        }
        remove_transaction_temps(&self.directory, journal.transaction_id)?;
        fs::remove_file(&journal_path)?;
        sync_directory(&self.directory)?;
        Ok(BundleRecoveryAction::RolledBackPartialBundle)
    }
}

pub fn build_bundle_journal(journal: &BundleJournal) -> Result<Vec<u8>, BundleJournalError> {
    if journal.transaction_id == 0 || journal.entries.is_empty() {
        return Err(BundleJournalError::InvalidJournal);
    }
    let count = u16::try_from(journal.entries.len()).map_err(|_| BundleJournalError::Overflow)?;
    let mut payload_bytes = 0_usize;
    let mut names = std::collections::BTreeSet::new();
    for entry in &journal.entries {
        validate_file_name(&entry.file_name)?;
        if !names.insert(entry.file_name.as_str()) {
            return Err(BundleJournalError::DuplicateFileName);
        }
        let name_len = entry.file_name.len();
        u16::try_from(name_len).map_err(|_| BundleJournalError::FileNameTooLong)?;
        payload_bytes = payload_bytes
            .checked_add(RECORD_FIXED_BYTES)
            .and_then(|value| value.checked_add(name_len))
            .ok_or(BundleJournalError::Overflow)?;
    }
    let total = CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES
        .checked_add(payload_bytes)
        .ok_or(BundleJournalError::Overflow)?;
    let mut bytes = vec![0_u8; total];
    bytes[0..8].copy_from_slice(&CONTEXT_BUNDLE_JOURNAL_MAGIC);
    put_u16(&mut bytes, 8, CONTEXT_BUNDLE_JOURNAL_VERSION)?;
    put_u16(
        &mut bytes,
        10,
        u16::try_from(CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES)
            .map_err(|_| BundleJournalError::Overflow)?,
    )?;
    put_u16(&mut bytes, 12, count)?;
    put_u16(&mut bytes, 14, 0)?;
    put_u64(&mut bytes, 16, journal.transaction_id)?;
    put_u32(
        &mut bytes,
        24,
        u32::try_from(payload_bytes).map_err(|_| BundleJournalError::Overflow)?,
    )?;
    put_u32(&mut bytes, 28, 0)?;

    let mut cursor = CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES;
    for entry in &journal.entries {
        let name = entry.file_name.as_bytes();
        put_u16(
            &mut bytes,
            cursor,
            u16::try_from(name.len()).map_err(|_| BundleJournalError::FileNameTooLong)?,
        )?;
        cursor += 2;
        put_u64(&mut bytes, cursor, entry.byte_len)?;
        cursor += 8;
        put_bytes(&mut bytes, cursor, &entry.sha256)?;
        cursor += 32;
        put_bytes(&mut bytes, cursor, name)?;
        cursor = cursor.checked_add(name.len()).ok_or(BundleJournalError::Overflow)?;
    }

    let crc = crc32c(&bytes[CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES..]);
    put_u32(&mut bytes, 28, crc)?;
    Ok(bytes)
}

pub fn parse_bundle_journal(bytes: &[u8]) -> Result<BundleJournal, BundleJournalError> {
    if bytes.len() < CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES {
        return Err(BundleJournalError::TruncatedJournal);
    }
    if bytes[0..8] != CONTEXT_BUNDLE_JOURNAL_MAGIC {
        return Err(BundleJournalError::InvalidJournalMagic);
    }
    if get_u16(bytes, 8)? != CONTEXT_BUNDLE_JOURNAL_VERSION
        || usize::from(get_u16(bytes, 10)?) != CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES
    {
        return Err(BundleJournalError::UnsupportedJournalVersion);
    }
    if get_u16(bytes, 14)? != 0 {
        return Err(BundleJournalError::UnsupportedJournalFlags);
    }
    let count = usize::from(get_u16(bytes, 12)?);
    if count == 0 {
        return Err(BundleJournalError::InvalidJournal);
    }
    let transaction_id = get_u64(bytes, 16)?;
    if transaction_id == 0 {
        return Err(BundleJournalError::InvalidJournal);
    }
    let payload_bytes =
        usize::try_from(get_u32(bytes, 24)?).map_err(|_| BundleJournalError::Overflow)?;
    if CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES
        .checked_add(payload_bytes)
        .ok_or(BundleJournalError::Overflow)?
        != bytes.len()
    {
        return Err(BundleJournalError::JournalLayout);
    }
    if get_u32(bytes, 28)? != crc32c(&bytes[CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES..]) {
        return Err(BundleJournalError::JournalChecksum);
    }

    let mut cursor = CONTEXT_BUNDLE_JOURNAL_HEADER_BYTES;
    let mut entries = Vec::with_capacity(count);
    let mut names = std::collections::BTreeSet::new();
    for _ in 0..count {
        let name_len = usize::from(get_u16(bytes, cursor)?);
        cursor = cursor.checked_add(2).ok_or(BundleJournalError::Overflow)?;
        let byte_len = get_u64(bytes, cursor)?;
        cursor = cursor.checked_add(8).ok_or(BundleJournalError::Overflow)?;
        let sha256 = get_array::<32>(bytes, cursor)?;
        cursor = cursor.checked_add(32).ok_or(BundleJournalError::Overflow)?;
        let end = cursor
            .checked_add(name_len)
            .ok_or(BundleJournalError::Overflow)?;
        let name_bytes = bytes
            .get(cursor..end)
            .ok_or(BundleJournalError::TruncatedJournal)?;
        let file_name = std::str::from_utf8(name_bytes)
            .map_err(|_| BundleJournalError::InvalidFileName)?
            .to_string();
        validate_file_name(&file_name)?;
        if !names.insert(file_name.clone()) {
            return Err(BundleJournalError::DuplicateFileName);
        }
        entries.push(BundleJournalEntry {
            file_name,
            byte_len,
            sha256,
        });
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(BundleJournalError::JournalLayout);
    }
    Ok(BundleJournal {
        transaction_id,
        entries,
    })
}

fn publish_immutable_file(
    directory: &Path,
    file_name: &str,
    bytes: &[u8],
    transaction_id: u64,
    ordinal: u64,
) -> Result<(), BundleJournalError> {
    let final_path = directory.join(file_name);
    if final_path.exists() {
        return Err(BundleJournalError::TargetExists(file_name.to_string()));
    }
    let temp_name = temp_file_name(transaction_id, ordinal);
    let temp_path = directory.join(&temp_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match fs::hard_link(&temp_path, &final_path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temp_path);
            return Err(BundleJournalError::TargetExists(file_name.to_string()));
        }
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(BundleJournalError::Io(error.kind()));
        }
    }
    fs::remove_file(&temp_path)?;
    Ok(())
}

fn remove_transaction_temps(directory: &Path, transaction_id: u64) -> Result<(), BundleJournalError> {
    let prefix = format!(".smlctx-txn-{transaction_id:016x}-");
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn sync_directory(directory: &Path) -> Result<(), BundleJournalError> {
    let file = File::open(directory)?;
    file.sync_all()?;
    Ok(())
}

fn next_transaction_id() -> u64 {
    loop {
        let sequence = TXN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = (u64::from(std::process::id()) << 32) ^ sequence;
        if id != 0 {
            return id;
        }
    }
}

fn temp_file_name(transaction_id: u64, ordinal: u64) -> String {
    format!(".smlctx-txn-{transaction_id:016x}-{ordinal:04x}.tmp")
}

fn validate_file_name(value: &str) -> Result<(), BundleJournalError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(BundleJournalError::InvalidFileName);
    }
    Ok(())
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

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), BundleJournalError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), BundleJournalError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), BundleJournalError> {
    put_bytes(bytes, offset, &value.to_le_bytes())
}

fn put_bytes(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), BundleJournalError> {
    let end = offset.checked_add(value.len()).ok_or(BundleJournalError::Overflow)?;
    bytes
        .get_mut(offset..end)
        .ok_or(BundleJournalError::JournalLayout)?
        .copy_from_slice(value);
    Ok(())
}

fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, BundleJournalError> {
    Ok(u16::from_le_bytes(get_array::<2>(bytes, offset)?))
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, BundleJournalError> {
    Ok(u32::from_le_bytes(get_array::<4>(bytes, offset)?))
}

fn get_u64(bytes: &[u8], offset: usize) -> Result<u64, BundleJournalError> {
    Ok(u64::from_le_bytes(get_array::<8>(bytes, offset)?))
}

fn get_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], BundleJournalError> {
    let end = offset.checked_add(N).ok_or(BundleJournalError::Overflow)?;
    bytes
        .get(offset..end)
        .ok_or(BundleJournalError::TruncatedJournal)?
        .try_into()
        .map_err(|_| BundleJournalError::TruncatedJournal)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleJournalError {
    InvalidPolicy,
    OutputNotDirectory,
    ArtifactCountBudget,
    ArtifactByteBudget {
        file_name: String,
        actual: u64,
        maximum: u64,
    },
    JournalByteBudget,
    JournalExists,
    TargetExists(String),
    DuplicateFileName,
    InvalidFileName,
    FileNameTooLong,
    InvalidJournal,
    InvalidJournalMagic,
    UnsupportedJournalVersion,
    UnsupportedJournalFlags,
    TruncatedJournal,
    JournalLayout,
    JournalChecksum,
    ForeignOrModifiedArtifact(String),
    Overflow,
    Io(io::ErrorKind),
    Hash(RawContextStoreError),
}

impl From<io::Error> for BundleJournalError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<RawContextStoreError> for BundleJournalError {
    fn from(error: RawContextStoreError) -> Self {
        Self::Hash(error)
    }
}

impl fmt::Display for BundleJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for BundleJournalError {}
