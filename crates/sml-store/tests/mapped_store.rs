#![cfg(target_os = "linux")]

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::semantic::{build_semantic_store, SemanticState, SemanticStoreError};
use sml_store::{MappedSemanticStore, MappedStoreError, MappedStoreLimits};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct Artifact {
    dir: PathBuf,
    path: PathBuf,
}

impl Artifact {
    fn new(bytes: &[u8]) -> Self {
        let dir = loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("sml-store-{}-{id}", std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => break dir,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("temporary directory: {error}"),
            }
        };
        let path = dir.join("semantic.bin");
        fs::write(&path, bytes).unwrap();
        Self { dir, path }
    }
}

impl Drop for Artifact {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn state<const BITS: usize>(bit: usize) -> SemanticState<BITS> {
    let mut state = SemanticState::zeroed().unwrap();
    state.value.set(bit % BITS, true).unwrap();
    state.meaning_mask.set((bit + 1) % BITS, true).unwrap();
    state.ambiguity.set((bit + 2) % BITS, true).unwrap();
    state.routing.set((bit + 3) % BITS, true).unwrap();
    state
}

fn limits() -> MappedStoreLimits {
    MappedStoreLimits {
        max_file_bytes: 1_048_576,
        max_records: 100,
        max_decoded_record_bytes: 8_192,
    }
}

#[test]
fn linux_mapping_decodes_only_requested_records() {
    let bytes = build_semantic_store(&[(42, state::<257>(7)), (9, state::<257>(2))]).unwrap();
    let artifact = Artifact::new(&bytes);
    let mut store = MappedSemanticStore::<257>::open(&artifact.path, limits()).unwrap();
    assert_eq!(store.stats().mapped_file_bytes, bytes.len() as u64);
    assert_eq!(store.stats().validated_index_entries, 2);
    assert_eq!(store.stats().decoded_records, 0);
    assert_eq!(store.stats().decoded_payload_bytes, 0);
    assert!(fs::read_to_string("/proc/self/maps")
        .unwrap()
        .contains(artifact.path.to_str().unwrap()));
    assert_eq!(store.get(42).unwrap(), Some(state::<257>(7)));
    assert_eq!(store.get(111).unwrap(), None);
    assert_eq!(store.stats().decoded_records, 1);
    assert_eq!(store.stats().decoded_payload_bytes, 160);
    assert_eq!(store.stats().missing_lookups, 1);
    println!(
        "mapped_bytes={} records={} decoded_records={} decoded_payload_bytes={}",
        store.stats().mapped_file_bytes,
        store.header().record_count,
        store.stats().decoded_records,
        store.stats().decoded_payload_bytes
    );
}

#[test]
fn reference_width_survives_close_and_reopen() {
    let expected = state::<16_384>(16_383);
    let artifact = Artifact::new(&build_semantic_store(&[(1, expected.clone())]).unwrap());
    for _ in 0..2 {
        let mut store = MappedSemanticStore::<16_384>::open(&artifact.path, limits()).unwrap();
        assert_eq!(store.header().payload_bytes, 8_192);
        let owned = store.get(1).unwrap().unwrap();
        drop(store);
        assert_eq!(owned, expected);
    }
}

#[test]
fn corrupt_payload_is_detected_lazily_without_decoding_other_records() {
    let mut bytes = build_semantic_store(&[(1, state::<65>(1)), (2, state::<65>(2))]).unwrap();
    let offset = sml_core::semantic::SemanticStoreView::<65>::parse(&bytes)
        .unwrap()
        .index_entry(1)
        .unwrap()
        .record_offset as usize;
    bytes[offset] ^= 1; // Corrupt before mapping; never mutate a live mapped inode.
    let artifact = Artifact::new(&bytes);
    let mut store = MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap();
    assert_eq!(store.get(1).unwrap(), Some(state::<65>(1)));
    assert!(matches!(
        store.get(2),
        Err(MappedStoreError::Format(
            SemanticStoreError::RecordChecksumMismatch { semantic_id: 2, .. }
        ))
    ));
    assert_eq!(store.stats().decoded_records, 1);
    assert_eq!(store.stats().failed_lookups, 1);
}

#[test]
fn corrupt_header_and_wrong_width_are_rejected() {
    let mut bytes = build_semantic_store(&[(1, state::<65>(1))]).unwrap();
    let artifact = Artifact::new(&bytes);
    assert!(matches!(
        MappedSemanticStore::<66>::open(&artifact.path, limits()),
        Err(MappedStoreError::Format(
            SemanticStoreError::DimensionMismatch { .. }
        ))
    ));
    bytes[56] ^= 1;
    fs::write(&artifact.path, &bytes).unwrap();
    assert!(matches!(
        MappedSemanticStore::<65>::open(&artifact.path, limits()),
        Err(MappedStoreError::Format(
            SemanticStoreError::HeaderChecksumMismatch { .. }
        ))
    ));
}

#[test]
fn empty_file_and_truncated_image_are_rejected_but_empty_store_is_valid() {
    let empty = Artifact::new(&[]);
    assert!(matches!(
        MappedSemanticStore::<65>::open(&empty.path, limits()),
        Err(MappedStoreError::Format(SemanticStoreError::TruncatedFile))
    ));
    let mut bytes = build_semantic_store(&[(1, state::<65>(1))]).unwrap();
    bytes.pop();
    let truncated = Artifact::new(&bytes);
    assert!(matches!(
        MappedSemanticStore::<65>::open(&truncated.path, limits()),
        Err(MappedStoreError::Format(
            SemanticStoreError::FileLength { .. }
        ))
    ));
    let valid_empty = Artifact::new(&build_semantic_store::<65>(&[]).unwrap());
    let mut store = MappedSemanticStore::<65>::open(&valid_empty.path, limits()).unwrap();
    assert_eq!(store.header().record_count, 0);
    assert_eq!(store.get(1).unwrap(), None);
}

#[test]
fn admission_limits_are_independent() {
    let bytes = build_semantic_store(&[(1, state::<257>(1))]).unwrap();
    let artifact = Artifact::new(&bytes);
    assert!(matches!(
        MappedSemanticStore::<257>::open(
            &artifact.path,
            MappedStoreLimits {
                max_file_bytes: bytes.len() as u64 - 1,
                ..limits()
            }
        ),
        Err(MappedStoreError::FileLimit { .. })
    ));
    assert!(matches!(
        MappedSemanticStore::<257>::open(
            &artifact.path,
            MappedStoreLimits {
                max_records: 0,
                ..limits()
            }
        ),
        Err(MappedStoreError::RecordLimit { .. })
    ));
    assert!(matches!(
        MappedSemanticStore::<257>::open(
            &artifact.path,
            MappedStoreLimits {
                max_decoded_record_bytes: 159,
                ..limits()
            }
        ),
        Err(MappedStoreError::PayloadLimit { .. })
    ));
}

#[test]
fn directories_are_not_mapped() {
    let artifact = Artifact::new(&[]);
    assert!(matches!(
        MappedSemanticStore::<65>::open(&artifact.dir, limits()),
        Err(MappedStoreError::NotRegularFile)
    ));
}

#[test]
fn shared_locks_live_until_the_last_reader_is_dropped() {
    let artifact = Artifact::new(&build_semantic_store(&[(1, state::<65>(1))]).unwrap());
    let first = MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap();
    let second = MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap();
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&artifact.path)
        .unwrap();
    assert!(matches!(writer.try_lock(), Err(TryLockError::WouldBlock)));
    drop(first);
    assert!(matches!(writer.try_lock(), Err(TryLockError::WouldBlock)));
    drop(second);
    writer.try_lock().unwrap();
    writer.unlock().unwrap();
}

#[test]
fn exclusive_writer_blocks_mapping() {
    let artifact = Artifact::new(&build_semantic_store(&[(1, state::<65>(1))]).unwrap());
    let writer = File::open(&artifact.path).unwrap();
    writer.try_lock().unwrap();
    match MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap_err() {
        MappedStoreError::Io(error) => assert_eq!(error.kind(), io::ErrorKind::WouldBlock),
        error => panic!("unexpected mapping error: {error}"),
    }
    writer.unlock().unwrap();
}

#[test]
fn atomic_replacement_keeps_old_mapping_and_opens_new_generation() {
    let old = state::<65>(1);
    let new = state::<65>(40);
    let artifact = Artifact::new(&build_semantic_store(&[(1, old.clone())]).unwrap());
    let mut old_reader = MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap();
    let replacement = artifact.dir.join("next.bin");
    fs::write(
        &replacement,
        build_semantic_store(&[(1, new.clone())]).unwrap(),
    )
    .unwrap();
    fs::rename(replacement, &artifact.path).unwrap();
    let mut new_reader = MappedSemanticStore::<65>::open(&artifact.path, limits()).unwrap();
    assert_eq!(old_reader.get(1).unwrap(), Some(old));
    assert_eq!(new_reader.get(1).unwrap(), Some(new));
}

fn rewrite_header_crc(bytes: &mut [u8]) {
    let mut crc = u32::MAX;
    for byte in &bytes[..56] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f6_3b78 & (crc & 1).wrapping_neg());
        }
    }
    bytes[56..60].copy_from_slice(&(!crc).to_le_bytes());
}

#[test]
fn zero_stride_is_rejected_even_with_a_valid_header_checksum() {
    let mut bytes = build_semantic_store(&[(1, state::<65>(1))]).unwrap();
    bytes[48..56].copy_from_slice(&0_u64.to_le_bytes());
    bytes.truncate(8_192);
    rewrite_header_crc(&mut bytes);
    let artifact = Artifact::new(&bytes);
    assert!(matches!(
        MappedSemanticStore::<65>::open(&artifact.path, limits()),
        Err(MappedStoreError::NonCanonicalLayout)
    ));
}

#[test]
fn undersized_record_stride_is_rejected_before_payload_decoding() {
    let mut bytes = build_semantic_store(&[(1, state::<16_384>(1))]).unwrap();
    bytes[48..56].copy_from_slice(&4_096_u64.to_le_bytes());
    bytes.truncate(12_288);
    rewrite_header_crc(&mut bytes);
    let artifact = Artifact::new(&bytes);
    assert!(matches!(
        MappedSemanticStore::<16_384>::open(&artifact.path, limits()),
        Err(MappedStoreError::NonCanonicalLayout)
    ));
}
