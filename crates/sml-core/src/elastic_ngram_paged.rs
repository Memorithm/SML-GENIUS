//! ENG-1c/CAP2b/1: exact out-of-core Witten-Bell next-byte model.
//!
//! Bounded external sort builds immutable counted (depth, exact suffix,
//! next-byte) records. Read-only sparse page lookup supports all 256
//! probabilities at explicit 0..32 byte context depth. No trained model
//! quality or adaptive depth authority is inferred from successful fitting.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    anf_char::{ChunkLayout, LANES},
    char_baselines::ByteScore,
    elastic_ngram_paged_sha::{hex, parse_hex, sha256, Sha256},
};

const SOURCE_RECORD_BYTES: usize = 34;
const COUNT_RECORD_BYTES: usize = 38;
const INDEX_ENTRY_BYTES: usize = 42;
const PAGE_ROWS: usize = 256;
const READER_BUFFER_BYTES: usize = 32_768;
const SORT_WRITE_BUFFER: usize = 65_536;
const RUN_HASH_INITIAL: u64 = 0xcbf2_9ce4_8422_2325;

pub const MAX_PAGED_DEPTH: usize = 32;
pub const MAX_PAGED_TRAIN_EVENTS: usize = 650_000;
pub const MAX_PAGED_CONTEXTS: u64 = 16_000_000;
pub const MAX_PAGED_FOLLOWER_PAIRS: u64 = 16_000_000;
pub const MAX_PAGED_SORT_RECORDS: usize = 262_144;
pub const MAX_PAGED_RUNS: usize = 96;
pub const MAX_PAGED_INDEX_ENTRIES: u64 = 100_000;
pub const MAX_PAGED_SCRATCH_BYTES: u64 = 1_073_741_824;
pub const MAX_PAGED_DATA_BYTES: u64 = 805_306_368;
pub const MAX_PAGED_TOTAL_BYTES: u64 = 2_147_483_648;

type Record = [u8; SOURCE_RECORD_BYTES];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagedLimits {
    pub rows_per_run: usize,
    pub max_runs: usize,
    pub max_scratch_bytes: u64,
    pub max_data_bytes: u64,
    pub max_total_bytes: u64,
    pub max_index_entries: u64,
    pub max_contexts: u64,
    pub max_follower_pairs: u64,
}

impl Default for PagedLimits {
    fn default() -> Self {
        Self {
            rows_per_run: MAX_PAGED_SORT_RECORDS,
            max_runs: MAX_PAGED_RUNS,
            max_scratch_bytes: MAX_PAGED_SCRATCH_BYTES,
            max_data_bytes: MAX_PAGED_DATA_BYTES,
            max_total_bytes: MAX_PAGED_TOTAL_BYTES,
            max_index_entries: MAX_PAGED_INDEX_ENTRIES,
            max_contexts: MAX_PAGED_CONTEXTS,
            max_follower_pairs: MAX_PAGED_FOLLOWER_PAIRS,
        }
    }
}

impl PagedLimits {
    fn valid(self) -> bool {
        self.rows_per_run > 0
            && self.rows_per_run <= MAX_PAGED_SORT_RECORDS
            && self.max_runs > 0
            && self.max_runs <= MAX_PAGED_RUNS
            && self.max_scratch_bytes > 0
            && self.max_scratch_bytes <= MAX_PAGED_SCRATCH_BYTES
            && self.max_data_bytes > 0
            && self.max_data_bytes <= MAX_PAGED_DATA_BYTES
            && self.max_total_bytes > 0
            && self.max_total_bytes <= MAX_PAGED_TOTAL_BYTES
            && self.max_index_entries > 0
            && self.max_index_entries <= MAX_PAGED_INDEX_ENTRIES
            && self.max_contexts > 0
            && self.max_contexts <= MAX_PAGED_CONTEXTS
            && self.max_follower_pairs > 0
            && self.max_follower_pairs <= MAX_PAGED_FOLLOWER_PAIRS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagedQuota {
    SortDisk,
    SortRunCount,
    CountedData,
    SparseIndex,
    TotalDisk,
    Contexts,
    FollowerPairs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagedError {
    InvalidCorpus,
    InvalidDepth,
    InvalidLimits,
    TooManyTrainEvents,
    ArithmeticOverflow,
    SourceMismatch,
    DestinationExists,
    CorruptRun,
    CorruptSortedOrder,
    CorruptRecordCount,
    CorruptChecksum,
    CorruptManifest,
    CorruptSparseIndex,
    InvalidProbability,
    Io,
    Quota(PagedQuota),
}

impl fmt::Display for PagedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PagedError {}

impl From<std::io::Error> for PagedError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedMetadata {
    pub max_depth: usize,
    pub train_bytes: u64,
    pub train_events: u64,
    pub source_records: u64,
    pub contexts: u64,
    pub follower_pairs: u64,
    pub count_store_bytes: u64,
    pub sparse_index_bytes: u64,
    pub sparse_entries: u64,
    pub scratch_peak_bytes: u64,
    pub sorted_runs: usize,
    pub source_sha256: [u8; 32],
    pub count_store_sha256: [u8; 32],
    pub sparse_index_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagedBuild {
    Published(PagedMetadata),
    CapacityStop(PagedQuota),
}

pub fn training_sha256(bytes: &[u8]) -> [u8; 32] {
    sha256(bytes)
}

fn source_record(history: &[u8], next: u8, depth: usize) -> Record {
    let mut key = [0u8; SOURCE_RECORD_BYTES];
    key[0] = depth as u8;
    if depth > 0 {
        key[1..depth + 1].copy_from_slice(&history[history.len() - depth..]);
    }
    key[SOURCE_RECORD_BYTES - 1] = next;
    key
}

fn run_step(mut h: u64, key: &Record) -> u64 {
    for &byte in key {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn reverse_hash(key: &Record) -> u64 {
    let mut h = 0x9e37_79b9_7f4a_7c15u64;
    for &byte in key.iter().rev() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InputDigest {
    sum: u64,
    reverse: u64,
}

impl InputDigest {
    fn add(&mut self, key: &Record) {
        self.sum = self.sum.wrapping_add(run_step(RUN_HASH_INITIAL, key));
        self.reverse = self.reverse.wrapping_add(reverse_hash(key));
    }
}

struct PrivateDir {
    path: PathBuf,
    should_remove: bool,
}

impl PrivateDir {
    fn create(path: &Path) -> Result<Self, PagedError> {
        if path.exists() {
            return Err(PagedError::DestinationExists);
        }
        fs::create_dir(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            should_remove: true,
        })
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        if self.should_remove {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct PrivateLock {
    path: PathBuf,
}

impl PrivateLock {
    fn create(path: &Path) -> Result<Self, PagedError> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| PagedError::DestinationExists)?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PrivateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
struct SortRun {
    path: PathBuf,
    rows: usize,
    bytes: u64,
    order_hash: u64,
}

fn push_run(
    batch: &mut Vec<Record>,
    dir: &Path,
    runs: &mut Vec<SortRun>,
    scratch_bytes: &mut u64,
    limits: PagedLimits,
) -> Result<(), PagedError> {
    if batch.is_empty() {
        return Ok(());
    }
    if runs.len() >= limits.max_runs {
        return Err(PagedError::Quota(PagedQuota::SortRunCount));
    }
    let bytes = (batch.len() as u64)
        .checked_mul(SOURCE_RECORD_BYTES as u64)
        .ok_or(PagedError::ArithmeticOverflow)?;
    let after = scratch_bytes
        .checked_add(bytes)
        .ok_or(PagedError::ArithmeticOverflow)?;
    if after > limits.max_scratch_bytes {
        return Err(PagedError::Quota(PagedQuota::SortDisk));
    }
    batch.sort_unstable();
    let path = dir.join(format!("sort-{:04}.bin", runs.len()));
    let f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let mut writer = BufWriter::with_capacity(SORT_WRITE_BUFFER, f);
    let mut h = RUN_HASH_INITIAL;
    for key in batch.iter() {
        writer.write_all(key)?;
        h = run_step(h, key);
    }
    writer.flush()?;
    drop(writer);
    if fs::metadata(&path)?.len() != bytes {
        return Err(PagedError::CorruptRun);
    }
    runs.push(SortRun {
        path,
        rows: batch.len(),
        bytes,
        order_hash: h,
    });
    *scratch_bytes = after;
    batch.clear();
    Ok(())
}

struct SortReader {
    file: BufReader<File>,
    remaining: usize,
    digest: u64,
    expected_digest: u64,
    previous: Option<Record>,
}

impl SortReader {
    fn open(run: &SortRun) -> Result<Self, PagedError> {
        if fs::metadata(&run.path)?.len() != run.bytes {
            return Err(PagedError::CorruptRun);
        }
        Ok(Self {
            file: BufReader::with_capacity(READER_BUFFER_BYTES, File::open(&run.path)?),
            remaining: run.rows,
            digest: RUN_HASH_INITIAL,
            expected_digest: run.order_hash,
            previous: None,
        })
    }

    fn next(&mut self) -> Result<Option<Record>, PagedError> {
        if self.remaining == 0 {
            if self.digest != self.expected_digest {
                return Err(PagedError::CorruptChecksum);
            }
            return Ok(None);
        }
        let mut record = [0u8; SOURCE_RECORD_BYTES];
        self.file.read_exact(&mut record)?;
        if let Some(prev) = self.previous {
            if prev > record {
                return Err(PagedError::CorruptSortedOrder);
            }
        }
        self.previous = Some(record);
        self.digest = run_step(self.digest, &record);
        self.remaining -= 1;
        Ok(Some(record))
    }
}

struct StoreCounters {
    contexts: u64,
    pairs: u64,
    observations: u64,
    index_entries: u64,
    last_context: Option<[u8; 33]>,
}

impl StoreCounters {
    fn new() -> Self {
        Self {
            contexts: 0,
            pairs: 0,
            observations: 0,
            index_entries: 0,
            last_context: None,
        }
    }

    fn push_count(
        &mut self,
        key: &Record,
        count: u32,
        data: &mut BufWriter<File>,
        sparse: &mut BufWriter<File>,
        scratch: u64,
        limits: PagedLimits,
    ) -> Result<(), PagedError> {
        if count == 0 {
            return Err(PagedError::CorruptRecordCount);
        }
        let next_pairs = self
            .pairs
            .checked_add(1)
            .ok_or(PagedError::ArithmeticOverflow)?;
        if next_pairs > limits.max_follower_pairs {
            return Err(PagedError::Quota(PagedQuota::FollowerPairs));
        }
        let next_data_bytes = next_pairs
            .checked_mul(COUNT_RECORD_BYTES as u64)
            .ok_or(PagedError::ArithmeticOverflow)?;
        if next_data_bytes > limits.max_data_bytes {
            return Err(PagedError::Quota(PagedQuota::CountedData));
        }
        let mut next_index = self.index_entries;
        if self.pairs % PAGE_ROWS as u64 == 0 {
            next_index = next_index
                .checked_add(1)
                .ok_or(PagedError::ArithmeticOverflow)?;
            if next_index > limits.max_index_entries {
                return Err(PagedError::Quota(PagedQuota::SparseIndex));
            }
        }
        let next_index_bytes = next_index
            .checked_mul(INDEX_ENTRY_BYTES as u64)
            .ok_or(PagedError::ArithmeticOverflow)?;
        if scratch
            .checked_add(next_data_bytes)
            .and_then(|bytes| bytes.checked_add(next_index_bytes))
            .ok_or(PagedError::ArithmeticOverflow)?
            > limits.max_total_bytes
        {
            return Err(PagedError::Quota(PagedQuota::TotalDisk));
        }

        let context: [u8; 33] = key[..33].try_into().map_err(|_| PagedError::CorruptRun)?;
        if self.last_context != Some(context) {
            self.contexts = self
                .contexts
                .checked_add(1)
                .ok_or(PagedError::ArithmeticOverflow)?;
            if self.contexts > limits.max_contexts {
                return Err(PagedError::Quota(PagedQuota::Contexts));
            }
            self.last_context = Some(context);
        }
        if self.pairs % PAGE_ROWS as u64 == 0 {
            sparse.write_all(key)?;
            sparse.write_all(&self.pairs.to_le_bytes())?;
            self.index_entries = next_index;
        }
        data.write_all(key)?;
        data.write_all(&count.to_le_bytes())?;
        self.pairs = next_pairs;
        self.observations = self
            .observations
            .checked_add(u64::from(count))
            .ok_or(PagedError::ArithmeticOverflow)?;
        Ok(())
    }
}

fn merge_counts(
    runs: &[SortRun],
    expected_source_rows: u64,
    incoming: InputDigest,
    stage: &Path,
    scratch: u64,
    limits: PagedLimits,
) -> Result<StoreCounters, PagedError> {
    let mut readers: Vec<SortReader> = runs
        .iter()
        .map(SortReader::open)
        .collect::<Result<_, _>>()?;
    let mut heap: BinaryHeap<Reverse<(Record, usize)>> = BinaryHeap::new();
    for (i, reader) in readers.iter_mut().enumerate() {
        if let Some(key) = reader.next()? {
            heap.push(Reverse((key, i)));
        }
    }
    let data_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("counts.bin"))?;
    let index_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("sparse.bin"))?;
    let mut data = BufWriter::with_capacity(SORT_WRITE_BUFFER, data_file);
    let mut index = BufWriter::with_capacity(SORT_WRITE_BUFFER, index_file);
    let mut counters = StoreCounters::new();
    let mut previous: Option<Record> = None;
    let mut same_count = 0u32;
    let mut observed_rows = 0u64;
    let mut outgoing = InputDigest::default();

    while let Some(Reverse((key, source_id))) = heap.pop() {
        if let Some(prev) = previous {
            if prev > key {
                return Err(PagedError::CorruptSortedOrder);
            }
            if prev == key {
                same_count = same_count
                    .checked_add(1)
                    .ok_or(PagedError::ArithmeticOverflow)?;
            } else {
                counters.push_count(&prev, same_count, &mut data, &mut index, scratch, limits)?;
                previous = Some(key);
                same_count = 1;
            }
        } else {
            previous = Some(key);
            same_count = 1;
        }
        outgoing.add(&key);
        observed_rows = observed_rows
            .checked_add(1)
            .ok_or(PagedError::ArithmeticOverflow)?;
        if let Some(next) = readers[source_id].next()? {
            heap.push(Reverse((next, source_id)));
        }
    }
    if let Some(last) = previous {
        counters.push_count(&last, same_count, &mut data, &mut index, scratch, limits)?;
    }
    let run_rows = runs
        .iter()
        .try_fold(0u64, |total, run| total.checked_add(run.rows as u64))
        .ok_or(PagedError::ArithmeticOverflow)?;
    if observed_rows != expected_source_rows
        || observed_rows != run_rows
        || counters.observations != observed_rows
    {
        return Err(PagedError::CorruptRecordCount);
    }
    if incoming != outgoing {
        return Err(PagedError::CorruptChecksum);
    }
    data.flush()?;
    index.flush()?;
    data.get_ref().sync_all()?;
    index.get_ref().sync_all()?;
    drop(data);
    drop(index);
    let data_len = fs::metadata(stage.join("counts.bin"))?.len();
    let index_len = fs::metadata(stage.join("sparse.bin"))?.len();
    if data_len != counters.pairs * COUNT_RECORD_BYTES as u64
        || index_len != counters.index_entries * INDEX_ENTRY_BYTES as u64
    {
        return Err(PagedError::CorruptRecordCount);
    }
    Ok(counters)
}

fn sha256_file(path: &Path) -> Result<[u8; 32], PagedError> {
    let mut reader = BufReader::with_capacity(65_536, File::open(path)?);
    let mut sha = Sha256::default();
    let mut buffer = [0u8; 65_536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        sha.update(&buffer[..n]);
    }
    Ok(sha.finish())
}

fn expected_source_rows(events_per_lane: usize, depth: usize) -> Result<u64, PagedError> {
    let one_lane = (1..=events_per_lane)
        .try_fold(0u64, |sum, t| sum.checked_add(1 + t.min(depth) as u64))
        .ok_or(PagedError::ArithmeticOverflow)?;
    one_lane
        .checked_mul(LANES as u64)
        .ok_or(PagedError::ArithmeticOverflow)
}

fn append_manifest(dir: &Path, meta: &PagedMetadata) -> Result<(), PagedError> {
    let manifest = format!(
        "schema=ENG2B001\nmax_depth={}\ntrain_bytes={}\ntrain_events={}\nsource_records={}\ncontexts={}\nfollower_pairs={}\ncount_store_bytes={}\nsparse_index_bytes={}\nsparse_entries={}\nscratch_peak_bytes={}\nsorted_runs={}\nsource_sha256={}\ncounts_sha256={}\nindex_sha256={}\n",
        meta.max_depth,
        meta.train_bytes,
        meta.train_events,
        meta.source_records,
        meta.contexts,
        meta.follower_pairs,
        meta.count_store_bytes,
        meta.sparse_index_bytes,
        meta.sparse_entries,
        meta.scratch_peak_bytes,
        meta.sorted_runs,
        hex(&meta.source_sha256),
        hex(&meta.count_store_sha256),
        hex(&meta.sparse_index_sha256),
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join("manifest.txt"))?;
    file.write_all(manifest.as_bytes())?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn parse_manifest(path: &Path) -> Result<PagedMetadata, PagedError> {
    let manifest = fs::read_to_string(path.join("manifest.txt"))?;
    if manifest.len() > 2048 {
        return Err(PagedError::CorruptManifest);
    }
    let mut lines = manifest.lines();
    let mut next = |field: &str| -> Result<String, PagedError> {
        let line = lines.next().ok_or(PagedError::CorruptManifest)?;
        let (key, value) = line.split_once('=').ok_or(PagedError::CorruptManifest)?;
        if key != field || value.is_empty() {
            return Err(PagedError::CorruptManifest);
        }
        Ok(value.to_owned())
    };
    if next("schema")? != "ENG2B001" {
        return Err(PagedError::CorruptManifest);
    }
    let number = |text: String| text.parse::<u64>().map_err(|_| PagedError::CorruptManifest);
    let max_depth =
        usize::try_from(number(next("max_depth")?)?).map_err(|_| PagedError::CorruptManifest)?;
    let train_bytes = number(next("train_bytes")?)?;
    let train_events = number(next("train_events")?)?;
    let source_records = number(next("source_records")?)?;
    let contexts = number(next("contexts")?)?;
    let follower_pairs = number(next("follower_pairs")?)?;
    let count_store_bytes = number(next("count_store_bytes")?)?;
    let sparse_index_bytes = number(next("sparse_index_bytes")?)?;
    let sparse_entries = number(next("sparse_entries")?)?;
    let scratch_peak_bytes = number(next("scratch_peak_bytes")?)?;
    let sorted_runs =
        usize::try_from(number(next("sorted_runs")?)?).map_err(|_| PagedError::CorruptManifest)?;
    let source_sha256 = parse_hex(&next("source_sha256")?).ok_or(PagedError::CorruptManifest)?;
    let count_store_sha256 =
        parse_hex(&next("counts_sha256")?).ok_or(PagedError::CorruptManifest)?;
    let sparse_index_sha256 =
        parse_hex(&next("index_sha256")?).ok_or(PagedError::CorruptManifest)?;
    drop(next);
    if lines.next().is_some()
        || max_depth > MAX_PAGED_DEPTH
        || train_events == 0
        || train_events > MAX_PAGED_TRAIN_EVENTS as u64
        || train_bytes == 0
        || follower_pairs == 0
        || contexts == 0
        || follower_pairs > MAX_PAGED_FOLLOWER_PAIRS
        || contexts > MAX_PAGED_CONTEXTS
        || count_store_bytes != follower_pairs * COUNT_RECORD_BYTES as u64
        || sparse_index_bytes != sparse_entries * INDEX_ENTRY_BYTES as u64
        || sparse_entries != follower_pairs.div_ceil(PAGE_ROWS as u64)
        || sparse_entries > MAX_PAGED_INDEX_ENTRIES
        || sorted_runs == 0
        || sorted_runs > MAX_PAGED_RUNS
        || scratch_peak_bytes > MAX_PAGED_SCRATCH_BYTES
        || count_store_bytes > MAX_PAGED_DATA_BYTES
        || scratch_peak_bytes
            .checked_add(count_store_bytes)
            .and_then(|value| value.checked_add(sparse_index_bytes))
            .ok_or(PagedError::CorruptManifest)?
            > MAX_PAGED_TOTAL_BYTES
    {
        return Err(PagedError::CorruptManifest);
    }
    Ok(PagedMetadata {
        max_depth,
        train_bytes,
        train_events,
        source_records,
        contexts,
        follower_pairs,
        count_store_bytes,
        sparse_index_bytes,
        sparse_entries,
        scratch_peak_bytes,
        sorted_runs,
        source_sha256,
        count_store_sha256,
        sparse_index_sha256,
    })
}

fn build_inner(
    train: &[u8],
    depth: usize,
    output_dir: &Path,
    scratch_dir: &Path,
    limits: PagedLimits,
    fail_before_publish: bool,
) -> Result<PagedMetadata, PagedError> {
    if depth > MAX_PAGED_DEPTH {
        return Err(PagedError::InvalidDepth);
    }
    if !limits.valid() || output_dir == scratch_dir {
        return Err(PagedError::InvalidLimits);
    }
    let layout = ChunkLayout::new(train.len()).map_err(|_| PagedError::InvalidCorpus)?;
    let events = LANES
        .checked_mul(layout.chunk - 1)
        .ok_or(PagedError::ArithmeticOverflow)?;
    if events > MAX_PAGED_TRAIN_EVENTS {
        return Err(PagedError::TooManyTrainEvents);
    }
    if output_dir.exists() {
        return Err(PagedError::DestinationExists);
    }

    let lock_path = output_dir.with_extension("eng2b-lock");
    let _lock = PrivateLock::create(&lock_path)?;
    let stage_path = output_dir.with_extension("eng2b-staging");
    let mut stage = PrivateDir::create(&stage_path)?;
    let scratch = PrivateDir::create(scratch_dir)?;

    let mut batch = Vec::<Record>::with_capacity(limits.rows_per_run);
    let mut runs = Vec::<SortRun>::new();
    let mut scratch_bytes = 0u64;
    let mut generated = 0u64;
    let mut input_checksum = InputDigest::default();

    for lane in 0..LANES {
        let chunk = &train[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            for d in 0..=t.min(depth) {
                let key = source_record(&chunk[..t], chunk[t], d);
                input_checksum.add(&key);
                batch.push(key);
                generated = generated
                    .checked_add(1)
                    .ok_or(PagedError::ArithmeticOverflow)?;
                if batch.len() == limits.rows_per_run {
                    push_run(
                        &mut batch,
                        &scratch.path,
                        &mut runs,
                        &mut scratch_bytes,
                        limits,
                    )?;
                }
            }
        }
    }
    push_run(
        &mut batch,
        &scratch.path,
        &mut runs,
        &mut scratch_bytes,
        limits,
    )?;
    if generated != expected_source_rows(layout.chunk - 1, depth)? {
        return Err(PagedError::CorruptRecordCount);
    }
    let counters = merge_counts(
        &runs,
        generated,
        input_checksum,
        &stage.path,
        scratch_bytes,
        limits,
    )?;

    let data_bytes = counters
        .pairs
        .checked_mul(COUNT_RECORD_BYTES as u64)
        .ok_or(PagedError::ArithmeticOverflow)?;
    let index_bytes = counters
        .index_entries
        .checked_mul(INDEX_ENTRY_BYTES as u64)
        .ok_or(PagedError::ArithmeticOverflow)?;
    let meta = PagedMetadata {
        max_depth: depth,
        train_bytes: train.len() as u64,
        train_events: events as u64,
        source_records: generated,
        contexts: counters.contexts,
        follower_pairs: counters.pairs,
        count_store_bytes: data_bytes,
        sparse_index_bytes: index_bytes,
        sparse_entries: counters.index_entries,
        scratch_peak_bytes: scratch_bytes,
        sorted_runs: runs.len(),
        source_sha256: training_sha256(train),
        count_store_sha256: sha256_file(&stage.path.join("counts.bin"))?,
        sparse_index_sha256: sha256_file(&stage.path.join("sparse.bin"))?,
    };
    append_manifest(&stage.path, &meta)?;
    File::open(&stage.path)?.sync_all()?;
    let opened = PagedNGram::open(&stage.path, meta.source_sha256)?;
    if opened.metadata() != &meta || fail_before_publish {
        return Err(PagedError::CorruptManifest);
    }
    drop(opened);
    if output_dir.exists() {
        return Err(PagedError::DestinationExists);
    }
    fs::rename(&stage.path, output_dir)?;
    stage.should_remove = false;
    if let Some(parent) = output_dir.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(meta)
}

pub fn build_paged(
    train: &[u8],
    depth: usize,
    output_dir: &Path,
    scratch_dir: &Path,
    limits: PagedLimits,
) -> Result<PagedBuild, PagedError> {
    match build_inner(train, depth, output_dir, scratch_dir, limits, false) {
        Ok(meta) => Ok(PagedBuild::Published(meta)),
        Err(PagedError::Quota(quota)) => Ok(PagedBuild::CapacityStop(quota)),
        Err(error) => Err(error),
    }
}

#[derive(Clone)]
struct IndexRow {
    key: Record,
    offset: u64,
}

pub struct PagedNGram {
    metadata: PagedMetadata,
    data: File,
    index: Vec<IndexRow>,
}

impl PagedNGram {
    pub fn open(path: &Path, expected_source_sha256: [u8; 32]) -> Result<Self, PagedError> {
        let meta = parse_manifest(path)?;
        if meta.source_sha256 != expected_source_sha256 {
            return Err(PagedError::SourceMismatch);
        }
        let data_path = path.join("counts.bin");
        let index_path = path.join("sparse.bin");
        if fs::metadata(&data_path)?.len() != meta.count_store_bytes
            || fs::metadata(&index_path)?.len() != meta.sparse_index_bytes
        {
            return Err(PagedError::CorruptManifest);
        }
        if sha256_file(&data_path)? != meta.count_store_sha256
            || sha256_file(&index_path)? != meta.sparse_index_sha256
        {
            return Err(PagedError::CorruptChecksum);
        }
        let mut reader = BufReader::new(File::open(&index_path)?);
        let mut index = Vec::with_capacity(meta.sparse_entries as usize);
        for i in 0..meta.sparse_entries {
            let mut key = [0u8; SOURCE_RECORD_BYTES];
            let mut index_bytes = [0u8; 8];
            reader.read_exact(&mut key)?;
            reader.read_exact(&mut index_bytes)?;
            let offset = u64::from_le_bytes(index_bytes);
            if offset != i * PAGE_ROWS as u64
                || index.last().is_some_and(|last: &IndexRow| last.key >= key)
            {
                return Err(PagedError::CorruptSparseIndex);
            }
            index.push(IndexRow { key, offset });
        }
        let mut data = File::open(&data_path)?;
        // Independently cross-check that every persisted sparse row really
        // identifies the first counted record in its own fixed-size page.
        for entry in &index {
            data.seek(SeekFrom::Start(entry.offset * COUNT_RECORD_BYTES as u64))?;
            let mut record = [0u8; COUNT_RECORD_BYTES];
            data.read_exact(&mut record)?;
            if record[..SOURCE_RECORD_BYTES] != entry.key {
                return Err(PagedError::CorruptSparseIndex);
            }
        }
        Ok(Self {
            metadata: meta,
            data,
            index,
        })
    }

    #[must_use]
    pub fn metadata(&self) -> &PagedMetadata {
        &self.metadata
    }

    fn context_histogram(
        &mut self,
        history: &[u8],
        depth: usize,
    ) -> Result<Option<(u64, Vec<(u8, u32)>)>, PagedError> {
        let mut query = [0u8; SOURCE_RECORD_BYTES];
        query[0] = depth as u8;
        if depth > 0 {
            query[1..depth + 1].copy_from_slice(&history[history.len() - depth..]);
        }
        let index_pos = self.index.partition_point(|entry| entry.key <= query);
        let first_record = if index_pos == 0 {
            0
        } else {
            self.index[index_pos - 1].offset
        };
        self.data
            .seek(SeekFrom::Start(first_record * COUNT_RECORD_BYTES as u64))?;
        let mut row = first_record;
        let mut followers = Vec::new();
        let mut total = 0u64;
        while row < self.metadata.follower_pairs {
            let mut counted = [0u8; COUNT_RECORD_BYTES];
            self.data.read_exact(&mut counted)?;
            let prefix = &counted[..SOURCE_RECORD_BYTES - 1];
            let target = &query[..SOURCE_RECORD_BYTES - 1];
            if prefix < target {
                row += 1;
                continue;
            }
            if prefix > target {
                break;
            }
            let count = u32::from_le_bytes(
                counted[SOURCE_RECORD_BYTES..]
                    .try_into()
                    .map_err(|_| PagedError::CorruptManifest)?,
            );
            if count == 0
                || followers
                    .last()
                    .is_some_and(|(last, _): &(u8, u32)| *last >= counted[SOURCE_RECORD_BYTES - 1])
            {
                return Err(PagedError::CorruptRecordCount);
            }
            total = total
                .checked_add(u64::from(count))
                .ok_or(PagedError::ArithmeticOverflow)?;
            followers.push((counted[SOURCE_RECORD_BYTES - 1], count));
            row += 1;
        }
        Ok(if followers.is_empty() {
            None
        } else {
            Some((total, followers))
        })
    }

    pub fn distribution(&mut self, history: &[u8], depth: usize) -> Result<[f64; 256], PagedError> {
        if depth > self.metadata.max_depth {
            return Err(PagedError::InvalidDepth);
        }
        let mut probabilities = [1.0 / 256.0; 256];
        for d in 0..=depth.min(history.len()) {
            if let Some((count, followers)) = self.context_histogram(history, d)? {
                let n = count as f64;
                let distinct = followers.len() as f64;
                let den = n + distinct;
                let scale = distinct / den;
                for p in &mut probabilities {
                    *p *= scale;
                }
                for (label, frequency) in followers {
                    probabilities[usize::from(label)] += f64::from(frequency) / den;
                }
            }
        }
        if probabilities.iter().any(|p| !p.is_finite() || *p <= 0.0)
            || (probabilities.iter().sum::<f64>() - 1.0).abs() > 1e-10
        {
            return Err(PagedError::InvalidProbability);
        }
        Ok(probabilities)
    }

    pub fn score(&mut self, bytes: &[u8], depth: usize) -> Result<ByteScore, PagedError> {
        if depth > self.metadata.max_depth {
            return Err(PagedError::InvalidDepth);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| PagedError::InvalidCorpus)?;
        let (mut total_bits, mut correct, mut tokens) = (0.0f64, 0u64, 0u64);
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let probs = self.distribution(&chunk[..t], depth)?;
                let label = usize::from(chunk[t]);
                total_bits -= probs[label].log2();
                let best = (0..256).fold(0, |prior, candidate| {
                    if probs[candidate] > probs[prior] {
                        candidate
                    } else {
                        prior
                    }
                });
                correct += u64::from(best == label);
                tokens += 1;
            }
        }
        Ok(ByteScore {
            tokens,
            bits_per_byte: total_bits / tokens as f64,
            accuracy: correct as f64 / tokens as f64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
        elastic_ngram_trie::{ElasticNGramTrie, TrieLimits},
    };

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sml-eng2b-paged-{}-{name}", std::process::id()));
            fs::create_dir(&path).expect("test directory must be unique");
            Self(path)
        }
        fn output(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn corpus(seed: usize) -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11 + seed) % 23) as u8)
            })
            .collect()
    }

    #[test]
    fn cap2b_exact_256_way_oracles_and_scores_across_depths_and_reopens() {
        let train = corpus(0);
        let probe = corpus(13);
        let root = TestRoot::new("oracles");
        for depth in [0, 4, 7, 12, 32] {
            let target = root.output(&format!("model-{depth}"));
            let scratch = root.output(&format!("scratch-{depth}"));
            let built =
                build_paged(&train, depth, &target, &scratch, PagedLimits::default()).unwrap();
            let PagedBuild::Published(meta) = built else {
                panic!("unexpected capacity STOP");
            };
            assert_eq!(meta.train_events, LANES as u64 * 39);
            assert!(!scratch.exists());
            let old = ElasticNGramTrie::train(
                &train,
                TrieLimits {
                    max_depth: depth,
                    ..TrieLimits::default()
                },
            )
            .unwrap();
            let full = ElasticNGramLong::train(
                &train,
                LongNGramLimits {
                    max_depth: depth,
                    ..LongNGramLimits::default()
                },
            )
            .unwrap();
            assert_eq!(meta.contexts, old.accounting().nodes as u64);
            assert_eq!(meta.follower_pairs, old.accounting().follower_pairs as u64);
            let mut disk = PagedNGram::open(&target, training_sha256(&train)).unwrap();
            let mut cold = PagedNGram::open(&target, training_sha256(&train)).unwrap();
            for lane in 0..LANES {
                let chunk = &probe[lane * 40..(lane + 1) * 40];
                for t in 1..40 {
                    let actual = disk.distribution(&chunk[..t], depth).unwrap();
                    let reference = old.distribution(&chunk[..t], depth).unwrap();
                    let plain = full.distribution(&chunk[..t], depth).unwrap();
                    let reopen = cold.distribution(&chunk[..t], depth).unwrap();
                    for y in 0..256 {
                        assert_eq!(actual[y].to_bits(), reference[y].to_bits());
                        assert_eq!(actual[y].to_bits(), plain[y].to_bits());
                        assert_eq!(actual[y].to_bits(), reopen[y].to_bits());
                    }
                }
            }
            let a = disk.score(&probe, depth).unwrap();
            let b = old.score(&probe, depth).unwrap();
            assert_eq!(a.tokens, b.tokens);
            assert_eq!(a.bits_per_byte.to_bits(), b.bits_per_byte.to_bits());
            assert_eq!(a.accuracy.to_bits(), b.accuracy.to_bits());
        }
    }

    #[test]
    fn cap2b_twelve_byte_teachers_and_lossless_zero_padding() {
        let train = (0..LANES)
            .flat_map(|lane| {
                let tag = (lane % 2) as u8;
                std::iter::once(tag)
                    .chain(std::iter::repeat_n(0, 11))
                    .chain(std::iter::once(200 + tag))
                    .chain(std::iter::repeat_n(77, 7))
            })
            .collect::<Vec<_>>();
        let root = TestRoot::new("twelve");
        let target = root.output("published");
        assert!(matches!(
            build_paged(
                &train,
                12,
                &target,
                &root.output("scratch"),
                PagedLimits::default()
            ),
            Ok(PagedBuild::Published(_))
        ));
        let mut disk = PagedNGram::open(&target, training_sha256(&train)).unwrap();
        let a = std::iter::once(0u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let b = std::iter::once(1u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let x = disk.distribution(&a, 7).unwrap();
        let y = disk.distribution(&b, 7).unwrap();
        for i in 0..256 {
            assert_eq!(x[i].to_bits(), y[i].to_bits());
        }
        assert!(disk.distribution(&a, 12).unwrap()[200] > 0.9);
        assert!(disk.distribution(&b, 12).unwrap()[201] > 0.9);
    }

    #[test]
    fn cap2b_many_sorted_runs_match_single_run_and_enforce_all_limits() {
        let train = corpus(0);
        let root = TestRoot::new("runlimits");
        let a = root.output("one");
        let b = root.output("many");
        let complete = build_paged(
            &train,
            12,
            &a,
            &root.output("scratch-one"),
            PagedLimits::default(),
        )
        .unwrap();
        let small_runs = build_paged(
            &train,
            12,
            &b,
            &root.output("scratch-many"),
            PagedLimits {
                rows_per_run: 1024,
                ..PagedLimits::default()
            },
        )
        .unwrap();
        let (PagedBuild::Published(x), PagedBuild::Published(y)) = (complete, small_runs) else {
            panic!("unexpected quota STOP");
        };
        assert_eq!(
            (x.contexts, x.follower_pairs, x.count_store_sha256),
            (y.contexts, y.follower_pairs, y.count_store_sha256)
        );
        assert_eq!(x.source_sha256, y.source_sha256);
        assert!(y.sorted_runs > 1);

        let quota = build_paged(
            &train,
            12,
            &root.output("quota"),
            &root.output("scratch-quota"),
            PagedLimits {
                max_data_bytes: 38,
                ..PagedLimits::default()
            },
        )
        .unwrap();
        assert_eq!(quota, PagedBuild::CapacityStop(PagedQuota::CountedData));
        assert!(!root.output("quota").exists());
        assert!(!root.output("scratch-quota").exists());

        assert_eq!(
            build_paged(
                &train,
                33,
                &root.output("bad-depth"),
                &root.output("unused"),
                PagedLimits::default()
            ),
            Err(PagedError::InvalidDepth)
        );
        assert_eq!(
            build_paged(
                &[0u8; LANES],
                4,
                &root.output("bad-corpus"),
                &root.output("unused"),
                PagedLimits::default()
            ),
            Err(PagedError::InvalidCorpus)
        );
        assert_eq!(
            build_paged(
                &train,
                4,
                &root.output("invalid-cap"),
                &root.output("unused"),
                PagedLimits {
                    rows_per_run: 0,
                    ..PagedLimits::default()
                }
            ),
            Err(PagedError::InvalidLimits)
        );
    }

    #[test]
    fn cap2b_corruption_source_id_and_atomic_publish_fail_closed() {
        let train = corpus(0);
        let root = TestRoot::new("corruption");
        let target = root.output("published");
        let scratch = root.output("scratch");
        let result = build_paged(&train, 12, &target, &scratch, PagedLimits::default()).unwrap();
        assert!(matches!(result, PagedBuild::Published(_)));
        let digest = training_sha256(&train);
        let mut fake = digest;
        fake[0] ^= 1;
        assert!(matches!(
            PagedNGram::open(&target, fake),
            Err(PagedError::SourceMismatch)
        ));
        assert!(matches!(
            build_paged(
                &train,
                12,
                &target,
                &root.output("other-scratch"),
                PagedLimits::default()
            ),
            Err(PagedError::DestinationExists)
        ));
        assert!(PagedNGram::open(&target, digest).is_ok());

        let not_published = root.output("failed-target");
        assert_eq!(
            build_inner(
                &train,
                12,
                &not_published,
                &root.output("failed-scratch"),
                PagedLimits::default(),
                true
            ),
            Err(PagedError::CorruptManifest)
        );
        assert!(!not_published.exists());
        assert!(!not_published.with_extension("eng2b-staging").exists());
        assert!(!not_published.with_extension("eng2b-lock").exists());

        let mut count_bytes = fs::read(target.join("counts.bin")).unwrap();
        count_bytes[35] ^= 0x40;
        fs::write(target.join("counts.bin"), count_bytes).unwrap();
        assert!(matches!(
            PagedNGram::open(&target, digest),
            Err(PagedError::CorruptChecksum)
        ));
    }

    #[test]
    fn cap2b_corrupted_sparse_index_and_manifest_are_never_opened() {
        let train = corpus(0);
        let root = TestRoot::new("index-corrupt");
        let target = root.output("published");
        assert!(matches!(
            build_paged(
                &train,
                12,
                &target,
                &root.output("scratch"),
                PagedLimits::default()
            ),
            Ok(PagedBuild::Published(_))
        ));
        let hash = training_sha256(&train);
        let mut idx = fs::read(target.join("sparse.bin")).unwrap();
        idx[0] ^= 1;
        fs::write(target.join("sparse.bin"), idx).unwrap();
        assert!(matches!(
            PagedNGram::open(&target, hash),
            Err(PagedError::CorruptChecksum)
        ));

        let text = fs::read_to_string(target.join("manifest.txt")).unwrap();
        fs::write(
            target.join("manifest.txt"),
            text.replace("schema=ENG2B001", "schema=BAD"),
        )
        .unwrap();
        assert!(matches!(
            PagedNGram::open(&target, hash),
            Err(PagedError::CorruptManifest)
        ));
    }
}
