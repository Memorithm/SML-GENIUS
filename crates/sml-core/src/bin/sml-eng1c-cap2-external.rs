//! ENG-1c/CAP2a: exact full-TRAIN suffix cardinality by bounded external sort.
//!
//! Does NOT fit a Witten-Bell model, score any VAL/TEST examples, or change
//! the declared capacity of ElasticNGramTrie. Only exact chronological suffix
//! bytes and next-byte identities are recorded; fixed-size runs are sorted
//! and merged without ever materializing the full index in memory.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_trie::{MAX_TRIE_EVENTS, MAX_TRIE_FOLLOWER_PAIRS, MAX_TRIE_NODES},
};

const RECORD_BYTES: usize = 34;
const MAX_DEPTH: usize = 32;
const SORT_RECORDS: usize = 262_144;
const MAX_RUNS: usize = 96;
const READER_BUFFER: usize = 32_768;
const MAX_TEMP_BYTES: u64 = 1_073_741_824;
const SCAN_EVENT_CEILING: usize = 650_000;
const ADDRESS_SPACE_CAP_KIB: usize = 262_144;
const DEPTH_LADDER: [usize; 8] = [0, 2, 4, 7, 12, 16, 24, 32];

type Record = [u8; RECORD_BYTES];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Limits {
    records_per_run: usize,
    max_runs: usize,
    scratch_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            records_per_run: SORT_RECORDS,
            max_runs: MAX_RUNS,
            scratch_bytes: MAX_TEMP_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Disk,
    RunCount,
}

impl Stop {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disk => "DISK_QUOTA_STOP",
            Self::RunCount => "RUN_QUOTA_STOP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanError {
    InvalidCorpus,
    InvalidDepth,
    InvalidLimits,
    TrainEventsOverScanCeiling,
    ArithmeticOverflow,
    InvalidRun,
    IncorrectRecordCount,
    IncorrectChecksum,
    UnsortedInput,
    Io,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for ScanError {}

impl From<io::Error> for ScanError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MultisetChecksum {
    sum: u64,
    reverse_sum: u64,
}

impl MultisetChecksum {
    fn insert(&mut self, key: &Record) {
        let h = record_hash(key);
        self.sum = self.sum.wrapping_add(h);
        let h2 = record_hash_reverse(key);
        self.reverse_sum = self.reverse_sum.wrapping_add(h2);
    }
}

fn record_hash(key: &Record) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for &byte in key {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

fn record_hash_reverse(key: &Record) -> u64 {
    let mut value = 0x9e37_79b9_7f4a_7c15u64;
    for &byte in key.iter().rev() {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

fn ordered_step(mut digest: u64, record: &Record) -> u64 {
    for &byte in record {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
    }
    digest
}

const ORDERED_HASH_INITIAL: u64 = 0xcbf2_9ce4_8422_2325;

fn record(history: &[u8], next: u8, depth: usize) -> Record {
    let mut result = [0u8; RECORD_BYTES];
    result[0] = depth as u8; // depth <=32 checked by caller
    if depth != 0 {
        result[1..depth + 1].copy_from_slice(&history[history.len() - depth..]);
    }
    result[RECORD_BYTES - 1] = next;
    result
}

#[derive(Debug)]
struct Scratch {
    directory: PathBuf,
}

impl Scratch {
    fn create(directory: &Path) -> Result<Self, ScanError> {
        fs::create_dir(directory)?;
        Ok(Self {
            directory: directory.to_path_buf(),
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Only a newly created, private directory is ever deleted.
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[derive(Debug)]
struct RunManifest {
    path: PathBuf,
    count: usize,
    expected_bytes: u64,
    ordered_checksum: u64,
}

fn write_run(
    records: &mut Vec<Record>,
    scratch: &Scratch,
    runs: &mut Vec<RunManifest>,
    peak_scratch: &mut u64,
    limits: Limits,
) -> Result<Option<Stop>, ScanError> {
    if records.is_empty() {
        return Ok(None);
    }
    if runs.len() >= limits.max_runs {
        return Ok(Some(Stop::RunCount));
    }
    let bytes = records
        .len()
        .checked_mul(RECORD_BYTES)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(ScanError::ArithmeticOverflow)?;
    let new_total = peak_scratch
        .checked_add(bytes)
        .ok_or(ScanError::ArithmeticOverflow)?;
    if new_total > limits.scratch_bytes {
        return Ok(Some(Stop::Disk));
    }

    records.sort_unstable();
    let path = scratch.directory.join(format!("run-{:04}.bin", runs.len()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let mut writer = BufWriter::with_capacity(65_536, file);
    let mut digest = ORDERED_HASH_INITIAL;
    for key in records.iter() {
        writer.write_all(key)?;
        digest = ordered_step(digest, key);
    }
    writer.flush()?;
    drop(writer);
    if fs::metadata(&path)?.len() != bytes {
        return Err(ScanError::InvalidRun);
    }
    runs.push(RunManifest {
        path,
        count: records.len(),
        expected_bytes: bytes,
        ordered_checksum: digest,
    });
    *peak_scratch = new_total;
    records.clear();
    Ok(None)
}

struct RunReader {
    reader: BufReader<File>,
    left: usize,
    expected_digest: u64,
    seen_digest: u64,
    previous: Option<Record>,
}

impl RunReader {
    fn open(run: &RunManifest) -> Result<Self, ScanError> {
        if fs::metadata(&run.path)?.len() != run.expected_bytes {
            return Err(ScanError::InvalidRun);
        }
        Ok(Self {
            reader: BufReader::with_capacity(READER_BUFFER, File::open(&run.path)?),
            left: run.count,
            expected_digest: run.ordered_checksum,
            seen_digest: ORDERED_HASH_INITIAL,
            previous: None,
        })
    }

    fn next(&mut self) -> Result<Option<Record>, ScanError> {
        if self.left == 0 {
            if self.seen_digest != self.expected_digest {
                return Err(ScanError::IncorrectChecksum);
            }
            return Ok(None);
        }
        let mut key = [0u8; RECORD_BYTES];
        self.reader.read_exact(&mut key)?;
        if let Some(last) = self.previous {
            if last > key {
                return Err(ScanError::UnsortedInput);
            }
        }
        self.previous = Some(key);
        self.seen_digest = ordered_step(self.seen_digest, &key);
        self.left -= 1;
        Ok(Some(key))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Counts {
    contexts: usize,
    follower_pairs: usize,
    rows: usize,
    checksums: MultisetChecksum,
}

fn merge_count(runs: &[RunManifest], expected: MultisetChecksum) -> Result<Counts, ScanError> {
    let mut readers: Vec<RunReader> = runs.iter().map(RunReader::open).collect::<Result<_, _>>()?;
    let mut pending: BinaryHeap<Reverse<(Record, usize)>> = BinaryHeap::new();
    for (i, reader) in readers.iter_mut().enumerate() {
        if let Some(rec) = reader.next()? {
            pending.push(Reverse((rec, i)));
        }
    }

    let mut last: Option<Record> = None;
    let mut count = Counts {
        contexts: 0,
        follower_pairs: 0,
        rows: 0,
        checksums: MultisetChecksum::default(),
    };
    while let Some(Reverse((key, reader_id))) = pending.pop() {
        if let Some(previous) = last {
            if previous > key {
                return Err(ScanError::UnsortedInput);
            }
            if previous[..RECORD_BYTES - 1] != key[..RECORD_BYTES - 1] {
                count.contexts = count
                    .contexts
                    .checked_add(1)
                    .ok_or(ScanError::ArithmeticOverflow)?;
            }
            if previous != key {
                count.follower_pairs = count
                    .follower_pairs
                    .checked_add(1)
                    .ok_or(ScanError::ArithmeticOverflow)?;
            }
        } else {
            count.contexts = 1;
            count.follower_pairs = 1;
        }
        last = Some(key);
        count.rows = count
            .rows
            .checked_add(1)
            .ok_or(ScanError::ArithmeticOverflow)?;
        count.checksums.insert(&key);
        if let Some(next) = readers[reader_id].next()? {
            pending.push(Reverse((next, reader_id)));
        }
    }
    let expected_rows = runs
        .iter()
        .try_fold(0usize, |a, r| a.checked_add(r.count))
        .ok_or(ScanError::ArithmeticOverflow)?;
    if count.rows != expected_rows {
        return Err(ScanError::IncorrectRecordCount);
    }
    if count.checksums != expected {
        return Err(ScanError::IncorrectChecksum);
    }
    Ok(count)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Completed {
    train_events: usize,
    rows: usize,
    contexts: usize,
    pairs: usize,
    scratch_peak_bytes: u64,
    runs: usize,
    checksum: MultisetChecksum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanResult {
    Full(Completed),
    QuotaStop {
        kind: Stop,
        completed_events: usize,
        generated_rows: usize,
        stored_run_bytes: u64,
    },
}

fn expected_records(events_per_lane: usize, depth: usize) -> Result<usize, ScanError> {
    let per_lane = (1..=events_per_lane)
        .try_fold(0usize, |sum, t| sum.checked_add(1 + t.min(depth)))
        .ok_or(ScanError::ArithmeticOverflow)?;
    per_lane
        .checked_mul(LANES)
        .ok_or(ScanError::ArithmeticOverflow)
}

fn scan(
    train: &[u8],
    depth: usize,
    scratch_path: &Path,
    limits: Limits,
) -> Result<ScanResult, ScanError> {
    if depth > MAX_DEPTH || !DEPTH_LADDER.contains(&depth) {
        return Err(ScanError::InvalidDepth);
    }
    if limits.records_per_run == 0
        || limits.records_per_run > SORT_RECORDS
        || limits.max_runs == 0
        || limits.max_runs > MAX_RUNS
        || limits.scratch_bytes == 0
        || limits.scratch_bytes > MAX_TEMP_BYTES
    {
        return Err(ScanError::InvalidLimits);
    }
    let layout = ChunkLayout::new(train.len()).map_err(|_| ScanError::InvalidCorpus)?;
    let train_events = LANES
        .checked_mul(layout.chunk - 1)
        .ok_or(ScanError::ArithmeticOverflow)?;
    if train_events > SCAN_EVENT_CEILING {
        return Err(ScanError::TrainEventsOverScanCeiling);
    }

    let scratch = Scratch::create(scratch_path)?;
    let mut buf = Vec::<Record>::with_capacity(limits.records_per_run);
    let mut runs = Vec::<RunManifest>::new();
    let mut scratch_bytes = 0u64;
    let mut generated_rows = 0usize;
    let mut completed_events = 0usize;
    let mut checksums = MultisetChecksum::default();

    for lane in 0..LANES {
        let chunk = &train[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            let history = &chunk[..t];
            for d in 0..=depth.min(t) {
                let key = record(history, chunk[t], d);
                checksums.insert(&key);
                buf.push(key);
                generated_rows += 1;
                if buf.len() >= limits.records_per_run {
                    if let Some(kind) =
                        write_run(&mut buf, &scratch, &mut runs, &mut scratch_bytes, limits)?
                    {
                        return Ok(ScanResult::QuotaStop {
                            kind,
                            completed_events,
                            generated_rows,
                            stored_run_bytes: scratch_bytes,
                        });
                    }
                }
            }
            completed_events += 1;
        }
    }
    if let Some(kind) = write_run(&mut buf, &scratch, &mut runs, &mut scratch_bytes, limits)? {
        return Ok(ScanResult::QuotaStop {
            kind,
            completed_events,
            generated_rows,
            stored_run_bytes: scratch_bytes,
        });
    }

    if generated_rows != expected_records(layout.chunk - 1, depth)?
        || completed_events != train_events
    {
        return Err(ScanError::IncorrectRecordCount);
    }
    let counts = merge_count(&runs, checksums)?;
    if counts.rows != generated_rows {
        return Err(ScanError::IncorrectRecordCount);
    }
    Ok(ScanResult::Full(Completed {
        train_events,
        rows: generated_rows,
        contexts: counts.contexts,
        pairs: counts.follower_pairs,
        scratch_peak_bytes: scratch_bytes,
        runs: runs.len(),
        checksum: counts.checksums,
    }))
}

#[derive(Clone, Copy)]
struct Corpus {
    name: &'static str,
    train_bytes: usize,
    events: usize,
}

fn frozen(language: &str) -> Option<Corpus> {
    match language {
        "FR" => Some(Corpus {
            name: "FR",
            train_bytes: 598_284,
            events: 598_208,
        }),
        "EN" => Some(Corpus {
            name: "EN",
            train_bytes: 590_360,
            events: 590_272,
        }),
        _ => None,
    }
}

fn vmhwm_kib() -> Result<u64, Box<dyn Error>> {
    let proc = fs::read_to_string("/proc/self/status")?;
    let line = proc
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .ok_or("Linux VmHWM missing")?;
    let mut tokens = line.split_whitespace();
    if tokens.next() != Some("VmHWM:") {
        return Err("Malformed VmHWM".into());
    }
    let kib = tokens.next().ok_or("VmHWM empty")?.parse::<u64>()?;
    if kib == 0 || tokens.next() != Some("kB") || tokens.next().is_some() {
        return Err("VmHWM invalid".into());
    }
    Ok(kib)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "usage: sml-eng1c-cap2-external FR|EN depth <TRAIN-file> <NEW-private-workdir>".into(),
        );
    }
    let corpus = frozen(&args[1]).ok_or("unknown frozen corpus")?;
    let depth = args[2].parse::<usize>()?;
    if !DEPTH_LADDER.contains(&depth) {
        return Err("depth outside preregistered CAP2a ladder".into());
    }
    let train = fs::read(&args[3])?;
    if train.len() != corpus.train_bytes {
        return Err("frozen TRAIN length mismatch".into());
    }
    let report = scan(&train, depth, Path::new(&args[4]), Limits::default())?;
    let memory = vmhwm_kib()?;

    match report {
        ScanResult::Full(measured) => {
            if measured.train_events != corpus.events {
                return Err("frozen TRAIN event denominator mismatch".into());
            }
            let node_gate = if measured.contexts <= MAX_TRIE_NODES {
                "PASS"
            } else {
                "STOP"
            };
            let pair_gate = if measured.pairs <= MAX_TRIE_FOLLOWER_PAIRS {
                "PASS"
            } else {
                "STOP"
            };
            let event_gate = if measured.train_events <= MAX_TRIE_EVENTS {
                "PASS"
            } else {
                "STOP"
            };
            println!(
                "ENG1C_CAP2,language={},depth={},status=FULL_SCAN_COMPLETE,train_bytes={},train_events={},records={},contexts={},follower_pairs={},runs={},scratch_peak_bytes={},checksum_sum={:016x},checksum_reverse={:016x},vmhwm_kib={},virtual_space_limit_kib={},model_event_gate={},model_node_gate={},model_pair_gate={},integrity=PASS",
                corpus.name, depth, train.len(), measured.train_events,
                measured.rows, measured.contexts, measured.pairs, measured.runs,
                measured.scratch_peak_bytes, measured.checksum.sum,
                measured.checksum.reverse_sum, memory, ADDRESS_SPACE_CAP_KIB,
                event_gate, node_gate, pair_gate
            );
        }
        ScanResult::QuotaStop {
            kind,
            completed_events,
            generated_rows,
            stored_run_bytes,
        } => {
            println!(
                "ENG1C_CAP2,language={},depth={},status={},train_bytes={},train_events={},completed_events={},generated_rows={},contexts=UNKNOWN,follower_pairs=UNKNOWN,scratch_peak_bytes={},vmhwm_kib={},virtual_space_limit_kib={},model_event_gate=STOP,model_node_gate=UNKNOWN,model_pair_gate=UNKNOWN,integrity=PARTIAL_STOP",
                corpus.name, depth, kind.as_str(), train.len(), corpus.events,
                completed_events, generated_rows, stored_run_bytes, memory,
                ADDRESS_SPACE_CAP_KIB
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sml_core::elastic_ngram_trie::{ElasticNGramTrie, TrieLimits};

    fn fixture() -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11) % 23) as u8)
            })
            .collect()
    }

    fn work(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sml-cap2a-{name}-{}", std::process::id()))
    }

    fn run_fixture(data: &[u8], depth: usize, suffix: &str, limits: Limits) -> ScanResult {
        scan(data, depth, &work(suffix), limits).unwrap()
    }

    #[test]
    fn exact_full_suffix_counts_equal_independent_ENG1b_at_all_depths() {
        let data = fixture();
        for depth in DEPTH_LADDER {
            let r = run_fixture(
                &data,
                depth,
                &format!("oracle-{depth}"),
                Limits {
                    records_per_run: 1000,
                    ..Limits::default()
                },
            );
            let ScanResult::Full(result) = r else {
                panic!("unexpected quota STOP for oracle");
            };
            let fitted = ElasticNGramTrie::train(
                &data,
                TrieLimits {
                    max_depth: depth,
                    ..TrieLimits::default()
                },
            )
            .unwrap();
            assert_eq!(result.train_events, LANES * 39);
            assert_eq!(result.contexts, fitted.accounting().nodes);
            assert_eq!(result.pairs, fitted.accounting().follower_pairs);
            assert_eq!(result.rows, expected_records(39, depth).unwrap());
        }
    }

    #[test]
    fn exact_padded_keys_cannot_collapse_twelve_byte_older_byte_difference() {
        let a = [0u8; 12];
        let mut b = a;
        b[0] = 1;
        assert_eq!(record(&a, 200, 7)[..33], record(&b, 200, 7)[..33]);
        assert_ne!(record(&a, 200, 12)[..33], record(&b, 200, 12)[..33]);
        assert_ne!(record(&a, 200, 0)[..33], record(&b, 200, 12)[..33]);
        assert_ne!(record(&a, 200, 12), record(&a, 201, 12));
    }

    #[test]
    fn two_plus_run_sort_merge_matches_full_buffer_mode() {
        let data = fixture();
        let a = run_fixture(
            &data,
            12,
            "one-buffer",
            Limits {
                records_per_run: SORT_RECORDS,
                ..Limits::default()
            },
        );
        let b = run_fixture(
            &data,
            12,
            "many-buffers",
            Limits {
                records_per_run: 311,
                ..Limits::default()
            },
        );
        let (ScanResult::Full(a), ScanResult::Full(b)) = (a, b) else {
            panic!("unexpected quota stop");
        };
        assert_eq!(
            (a.contexts, a.pairs, a.rows, a.checksum),
            (b.contexts, b.pairs, b.rows, b.checksum)
        );
        assert!(b.runs > 1);
        assert_eq!(a.runs, 1);
    }

    #[test]
    fn strict_file_budget_and_run_limit_return_partial_scientific_stop() {
        let data = fixture();
        let disk = run_fixture(
            &data,
            12,
            "disk-stop",
            Limits {
                records_per_run: 10,
                scratch_bytes: 1,
                ..Limits::default()
            },
        );
        assert!(matches!(
            disk,
            ScanResult::QuotaStop {
                kind: Stop::Disk,
                ..
            }
        ));
        let runs = run_fixture(
            &data,
            12,
            "run-stop",
            Limits {
                records_per_run: 10,
                max_runs: 1,
                ..Limits::default()
            },
        );
        assert!(matches!(
            runs,
            ScanResult::QuotaStop {
                kind: Stop::RunCount,
                ..
            }
        ));
    }

    #[test]
    fn corrupted_sorted_run_fails_validation_and_does_not_count_contexts() {
        let data = fixture();
        let directory = work("corrupt-run");
        let scratch = Scratch::create(&directory).unwrap();
        let mut runs = Vec::new();
        let mut chunk = vec![record(&data[..2], 33, 1), record(&data[..2], 55, 0)];
        let mut peak = 0u64;
        let mut checksums = MultisetChecksum::default();
        for item in &chunk {
            checksums.insert(item);
        }
        write_run(
            &mut chunk,
            &scratch,
            &mut runs,
            &mut peak,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(merge_count(&runs, checksums).unwrap().rows, 2);
        let mut bytes = fs::read(&runs[0].path).unwrap();
        bytes[3] ^= 0x80;
        fs::write(&runs[0].path, bytes).unwrap();
        assert!(matches!(
            merge_count(&runs, checksums),
            Err(ScanError::IncorrectChecksum) | Err(ScanError::UnsortedInput)
        ));
    }

    #[test]
    fn invalid_source_depth_limits_and_directory_reuse_fail_closed() {
        let data = fixture();
        assert_eq!(
            scan(&data, 3, &work("bad-depth"), Limits::default()),
            Err(ScanError::InvalidDepth)
        );
        assert_eq!(
            scan(&[0u8; LANES], 0, &work("bad-corpus"), Limits::default()),
            Err(ScanError::InvalidCorpus)
        );
        assert_eq!(
            scan(
                &vec![0u8; LANES * 10500],
                0,
                &work("huge"),
                Limits::default()
            ),
            Err(ScanError::TrainEventsOverScanCeiling)
        );
        assert_eq!(
            scan(
                &data,
                4,
                &work("bad-limits"),
                Limits {
                    records_per_run: 0,
                    ..Limits::default()
                }
            ),
            Err(ScanError::InvalidLimits)
        );
        let directory = work("exists");
        fs::create_dir(&directory).unwrap();
        assert_eq!(
            scan(&data, 0, &directory, Limits::default()),
            Err(ScanError::Io)
        );
        fs::remove_dir(&directory).unwrap();
    }
}
