//! PB-9 large-width Pascal scaling experiment.
//!
//! Frozen protocol:
//! docs/SML-PASCAL-BOOLEAN-PB9-LARGE-WIDTH-SCALING-PREREGISTRATION.md
//!
//! Modes:
//! - deterministic: mandatory exactness/accounting through K=1,048,576.
//! - benchmark: mandatory physical panels plus resource-bounded extensions.

use std::{
    collections::BTreeSet,
    env, fs,
    hint::black_box,
    time::Instant,
};

use sml_core::pascal_gf2::{pascal_gf2_in_place, PascalGf2Stats};

const MANDATORY_WIDTHS: [usize; 8] = [
    8_192,
    16_384,
    32_768,
    65_536,
    131_072,
    262_144,
    524_288,
    1_048_576,
];

const EXTENSION_WIDTHS: [usize; 8] = [
    2_097_152,
    4_194_304,
    8_388_608,
    16_777_216,
    33_554_432,
    67_108_864,
    134_217_728,
    268_435_456,
];

const QUERY_WIDTHS: [usize; 3] = [8_192, 65_536, 1_048_576];
const QUERY_Q: [usize; 3] = [128, 512, 2_048];

const DETERMINISTIC_SOURCES: usize = 8;
const EXTENSION_SOURCES: usize = 2;
const QUERY_SOURCES: usize = 4;
const SAMPLED_SOURCES: usize = 4;
const SAMPLE_ADDRESSES: usize = 64;
const MAX_SAMPLE_POPCOUNT: u32 = 12;

const MANDATORY_WARMUPS: usize = 2;
const MANDATORY_REPS: usize = 12;
const EXTENSION_WARMUPS: usize = 1;
const EXTENSION_REPS: usize = 7;
const DEEP_REPS: usize = 5;
const QUERY_WARMUPS: usize = 2;
const QUERY_REPS: usize = 12;

const SEMANTIC_BUFFER_CAP: usize = 256 * 1024 * 1024;
const PROBE_STOP_NS: u128 = 2_000_000_000;
const PREVIOUS_MATERIALIZE_STOP_NS: u128 = 1_000_000_000;

const EXPECTED_RUNNER: &str = "tarek-scirust-arm64-01";
const BASE: u64 = 0x5042_394c_4152_4745;

#[derive(Debug, Clone, Copy)]
struct Stream(u64);

impl Stream {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }
}

fn words_for(bits: usize) -> usize {
    bits / 64
}

fn state_bytes(bits: usize) -> usize {
    bits / 8
}

fn expected_logical_xor(bits: usize) -> u64 {
    (bits as u64 / 2) * u64::from(bits.ilog2())
}

fn expected_packed_updates(bits: usize) -> u64 {
    let words = words_for(bits) as u64;
    6 * words + u64::from(bits.ilog2() - 6) * (words / 2)
}

fn full_direct_reads(bits: usize) -> u128 {
    3_u128.pow(bits.ilog2())
}

fn get_bit(words: &[u64], index: usize) -> bool {
    words[index / 64] & (1_u64 << (index % 64)) != 0
}

fn set_bit(words: &mut [u64], index: usize) {
    words[index / 64] |= 1_u64 << (index % 64);
}

fn source_words(bits: usize, sample: usize) -> Vec<u64> {
    let seed = BASE
        ^ (bits as u64).rotate_left(17)
        ^ (sample as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut stream = Stream::new(seed);
    (0..words_for(bits)).map(|_| stream.next()).collect()
}

fn sources(bits: usize, count: usize) -> Vec<Vec<u64>> {
    (0..count).map(|sample| source_words(bits, sample)).collect()
}

fn word_checksum(words: &[u64]) -> u64 {
    words.iter().enumerate().fold(
        0xcbf2_9ce4_8422_2325_u64,
        |checksum, (index, &word)| {
            checksum
                .rotate_left(9)
                .wrapping_mul(0x1000_0000_01b3)
                ^ word
                ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        },
    )
}

fn checksum_push(
    checksum: u64,
    value: bool,
    sample: usize,
    address: usize,
    position: usize,
) -> u64 {
    let token = ((sample as u64) << 48)
        ^ ((address as u64) << 16)
        ^ position as u64
        ^ if value {
            0xd6e8_feb8_6659_fd93
        } else {
            0xa5a3_564e_27f1_886b
        };
    checksum.rotate_left(11).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ token
}

fn direct_submask_parity(words: &[u64], address: usize) -> bool {
    let mut parity = false;
    let mut submask = address;
    loop {
        parity ^= get_bit(words, submask);
        if submask == 0 {
            break;
        }
        submask = (submask - 1) & address;
    }
    parity
}

fn sampled_addresses(bits: usize) -> Vec<usize> {
    let mut addresses = Vec::with_capacity(SAMPLE_ADDRESSES);
    let mut seen = BTreeSet::new();
    for index in 0..bits {
        let address = (405 * index + 17) % bits;
        if address.count_ones() <= MAX_SAMPLE_POPCOUNT && seen.insert(address) {
            addresses.push(address);
            if addresses.len() == SAMPLE_ADDRESSES {
                break;
            }
        }
    }
    assert_eq!(addresses.len(), SAMPLE_ADDRESSES);
    addresses
}

fn logical_fast_zeta(words: &[u64], bits: usize) -> Vec<u8> {
    let mut output = (0..bits)
        .map(|index| u8::from(get_bit(words, index)))
        .collect::<Vec<_>>();
    let mut stride = 1_usize;
    while stride < bits {
        for mask in 0..bits {
            if mask & stride != 0 {
                output[mask] ^= output[mask ^ stride];
            }
        }
        stride <<= 1;
    }
    output
}

fn stats_match(bits: usize, stats: PascalGf2Stats) -> bool {
    stats.bits == bits
        && stats.words == words_for(bits)
        && stats.stages == bits.ilog2()
        && stats.logical_xor_ops == expected_logical_xor(bits)
        && stats.packed_word_updates == expected_packed_updates(bits)
        && stats.state_bytes == state_bytes(bits)
        && stats.scratch_words == 0
}

fn analytic_fixture_mismatches(bits: usize) -> (u64, PascalGf2Stats) {
    let words = words_for(bits);
    let mut mismatches = 0_u64;

    let mut zero = vec![0_u64; words];
    let stats = pascal_gf2_in_place(&mut zero).expect("PB9 zero fixture");
    mismatches += zero.iter().filter(|&&word| word != 0).count() as u64;

    let mut ones = vec![u64::MAX; words];
    pascal_gf2_in_place(&mut ones).expect("PB9 all-one fixture");
    for (index, &word) in ones.iter().enumerate() {
        let expected = if index == 0 { 1 } else { 0 };
        mismatches += u64::from(word != expected);
    }

    let mut basis_zero = vec![0_u64; words];
    set_bit(&mut basis_zero, 0);
    pascal_gf2_in_place(&mut basis_zero).expect("PB9 basis-zero fixture");
    mismatches += basis_zero
        .iter()
        .filter(|&&word| word != u64::MAX)
        .count() as u64;

    let mut basis_half = vec![0_u64; words];
    set_bit(&mut basis_half, bits / 2);
    pascal_gf2_in_place(&mut basis_half).expect("PB9 basis-half fixture");
    for (index, &word) in basis_half.iter().enumerate() {
        let expected = if index < words / 2 { 0 } else { u64::MAX };
        mismatches += u64::from(word != expected);
    }

    let mut basis_last = vec![0_u64; words];
    set_bit(&mut basis_last, bits - 1);
    pascal_gf2_in_place(&mut basis_last).expect("PB9 basis-last fixture");
    for (index, &word) in basis_last.iter().enumerate() {
        let expected = if index + 1 == words {
            1_u64 << 63
        } else {
            0
        };
        mismatches += u64::from(word != expected);
    }

    (mismatches, stats)
}

#[derive(Debug, Clone)]
struct WidthEvidence {
    bits: usize,
    source_collisions: usize,
    analytic_mismatches: u64,
    inverse_mismatches: u64,
    sampled_direct_mismatches: u64,
    last_address_mismatch: u64,
    logical_reference_mismatches: u64,
    source_checksum: u64,
    transformed_checksum: u64,
    stats: PascalGf2Stats,
    sample_addresses: Vec<usize>,
}

#[derive(Debug)]
struct DeterministicEvidence {
    widths: Vec<WidthEvidence>,
    l1: bool,
    l2: bool,
}

fn mandatory_width_evidence(bits: usize) -> WidthEvidence {
    let dense_sources = sources(bits, DETERMINISTIC_SOURCES);
    let unique = dense_sources
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .len();
    let source_collisions = DETERMINISTIC_SOURCES - unique;

    let (analytic_mismatches, stats) = analytic_fixture_mismatches(bits);
    let mut inverse_mismatches = 0_u64;
    let mut source_checksum = 0_u64;
    let mut transformed_checksum = 0_u64;

    for source in &dense_sources {
        source_checksum ^= word_checksum(source);
        let mut transformed = source.clone();
        pascal_gf2_in_place(&mut transformed).expect("PB9 mandatory transform");
        transformed_checksum ^= word_checksum(&transformed);
        pascal_gf2_in_place(&mut transformed).expect("PB9 mandatory inverse");
        inverse_mismatches += u64::from(transformed != *source);
    }

    let addresses = sampled_addresses(bits);
    let mut sampled_direct_mismatches = 0_u64;
    for source in dense_sources.iter().take(SAMPLED_SOURCES) {
        let mut transformed = source.clone();
        pascal_gf2_in_place(&mut transformed).expect("PB9 sampled transform");
        for &address in &addresses {
            sampled_direct_mismatches += u64::from(
                get_bit(&transformed, address) != direct_submask_parity(source, address),
            );
        }
    }

    let designated = &dense_sources[0];
    let mut transformed = designated.clone();
    pascal_gf2_in_place(&mut transformed).expect("PB9 last-address transform");
    let last_address_mismatch = u64::from(
        get_bit(&transformed, bits - 1) != direct_submask_parity(designated, bits - 1),
    );

    let logical = logical_fast_zeta(designated, bits);
    let logical_reference_mismatches = (0..bits)
        .map(|index| u64::from(get_bit(&transformed, index) != (logical[index] != 0)))
        .sum();

    WidthEvidence {
        bits,
        source_collisions,
        analytic_mismatches,
        inverse_mismatches,
        sampled_direct_mismatches,
        last_address_mismatch,
        logical_reference_mismatches,
        source_checksum,
        transformed_checksum,
        stats,
        sample_addresses: addresses,
    }
}

fn run_deterministic(emit: bool) -> DeterministicEvidence {
    let widths = MANDATORY_WIDTHS
        .into_iter()
        .map(mandatory_width_evidence)
        .collect::<Vec<_>>();

    let l1 = widths.iter().all(|evidence| {
        evidence.source_collisions == 0
            && evidence.analytic_mismatches == 0
            && evidence.inverse_mismatches == 0
            && evidence.sampled_direct_mismatches == 0
            && evidence.last_address_mismatch == 0
            && evidence.logical_reference_mismatches == 0
    });
    let l2 = widths
        .iter()
        .all(|evidence| stats_match(evidence.bits, evidence.stats));

    if emit {
        println!(
            "PB9_DETERMINISTIC_BEGIN,protocol=1,widths=8192|16384|32768|65536|131072|262144|524288|1048576,dense_sources={DETERMINISTIC_SOURCES},sampled_sources={SAMPLED_SOURCES},sample_addresses={SAMPLE_ADDRESSES}"
        );
        for evidence in &widths {
            let addresses = evidence
                .sample_addresses
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join("|");
            println!(
                "PB9_WIDTH,bits={},log2={},source_collisions={},analytic_mismatches={},inverse_mismatches={},sampled_direct_mismatches={},last_address_mismatch={},logical_reference_mismatches={},stages={},logical_xor_ops={},packed_word_updates={},state_bytes={},scratch_words={},full_direct_reads={},source_checksum={:016x},transformed_checksum={:016x}",
                evidence.bits,
                evidence.bits.ilog2(),
                evidence.source_collisions,
                evidence.analytic_mismatches,
                evidence.inverse_mismatches,
                evidence.sampled_direct_mismatches,
                evidence.last_address_mismatch,
                evidence.logical_reference_mismatches,
                evidence.stats.stages,
                evidence.stats.logical_xor_ops,
                evidence.stats.packed_word_updates,
                evidence.stats.state_bytes,
                evidence.stats.scratch_words,
                full_direct_reads(evidence.bits),
                evidence.source_checksum,
                evidence.transformed_checksum
            );
            println!(
                "PB9_SAMPLE_ADDRESSES,bits={},addresses={addresses}",
                evidence.bits
            );
        }
        println!(
            "PB9_GATE,L1_MANDATORY_EXACTNESS,{}",
            if l1 { "HOLDS" } else { "INVALID" }
        );
        println!(
            "PB9_GATE,L2_ACCOUNTING_EXACTNESS,{}",
            if l2 { "HOLDS" } else { "INVALID" }
        );
        println!("PB9_DETERMINISTIC_DECISION,PENDING_PHYSICAL");
        println!("PROMOTION,BLOCKED,pb9_requires_physical_scaling");
    }

    DeterministicEvidence { widths, l1, l2 }
}

fn percentile(values: &[u128], numerator: usize, denominator: usize) -> u128 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (numerator * sorted.len()).div_ceil(denominator);
    sorted[rank.saturating_sub(1)]
}

fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    if count % 2 == 0 {
        (sorted[count / 2 - 1] + sorted[count / 2]) / 2
    } else {
        sorted[count / 2]
    }
}

fn format_times(values: &[u128]) -> String {
    values
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join("|")
}

fn rss_value_kib(name: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let rest = line.strip_prefix(name)?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

#[derive(Debug, Clone)]
struct TransformBench {
    bits: usize,
    kernel_times: Vec<u128>,
    materialize_times: Vec<u128>,
    kernel_median: u128,
    materialize_median: u128,
    kernel_p95: u128,
    materialize_p95: u128,
    checksum: u64,
    stable: bool,
    rss_before_kib: Option<u64>,
    rss_after_kib: Option<u64>,
    hwm_after_kib: Option<u64>,
}

fn bench_transform(bits: usize, warmups: usize, reps: usize) -> TransformBench {
    let source = source_words(bits, 97);
    let mut kernel_buffer = vec![0_u64; words_for(bits)];
    let mut materialize_buffer = vec![0_u64; words_for(bits)];

    for _ in 0..warmups {
        kernel_buffer.copy_from_slice(&source);
        pascal_gf2_in_place(&mut kernel_buffer).expect("PB9 kernel warmup");
        materialize_buffer.copy_from_slice(&source);
        pascal_gf2_in_place(&mut materialize_buffer).expect("PB9 materialize warmup");
        black_box(word_checksum(&kernel_buffer));
        black_box(word_checksum(&materialize_buffer));
    }

    let rss_before_kib = rss_value_kib("VmRSS:");
    let mut kernel_times = Vec::with_capacity(reps);
    let mut materialize_times = Vec::with_capacity(reps);
    let mut checksums = Vec::with_capacity(reps * 2);

    for repetition in 0..reps {
        if repetition % 2 == 0 {
            kernel_buffer.copy_from_slice(&source);
            let start = Instant::now();
            pascal_gf2_in_place(&mut kernel_buffer).expect("PB9 kernel benchmark");
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            kernel_times.push(elapsed);
            checksums.push(word_checksum(&kernel_buffer));

            let start = Instant::now();
            materialize_buffer.copy_from_slice(&source);
            pascal_gf2_in_place(&mut materialize_buffer).expect("PB9 materialize benchmark");
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            materialize_times.push(elapsed);
            checksums.push(word_checksum(&materialize_buffer));
        } else {
            let start = Instant::now();
            materialize_buffer.copy_from_slice(&source);
            pascal_gf2_in_place(&mut materialize_buffer).expect("PB9 materialize benchmark");
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            materialize_times.push(elapsed);
            checksums.push(word_checksum(&materialize_buffer));

            kernel_buffer.copy_from_slice(&source);
            let start = Instant::now();
            pascal_gf2_in_place(&mut kernel_buffer).expect("PB9 kernel benchmark");
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            kernel_times.push(elapsed);
            checksums.push(word_checksum(&kernel_buffer));
        }
    }

    let checksum = checksums.first().copied().unwrap_or_default();
    let stable = checksums.iter().all(|&value| value == checksum);
    let rss_after_kib = rss_value_kib("VmRSS:");
    let hwm_after_kib = rss_value_kib("VmHWM:");

    TransformBench {
        bits,
        kernel_median: median(&kernel_times),
        materialize_median: median(&materialize_times),
        kernel_p95: percentile(&kernel_times, 95, 100),
        materialize_p95: percentile(&materialize_times, 95, 100),
        kernel_times,
        materialize_times,
        checksum,
        stable,
        rss_before_kib,
        rss_after_kib,
        hwm_after_kib,
    }
}

fn emit_transform_bench(panel: &str, bench: &TransformBench) {
    let bits_per_second =
        (bench.bits as u128 * 1_000_000_000_u128) / bench.kernel_median.max(1);
    let xor_per_second =
        (expected_logical_xor(bench.bits) as u128 * 1_000_000_000_u128)
            / bench.kernel_median.max(1);
    let packed_per_second =
        (expected_packed_updates(bench.bits) as u128 * 1_000_000_000_u128)
            / bench.kernel_median.max(1);

    println!(
        "PB9_BENCH_TRANSFORM,panel={},bits={},kernel_times_ns={},kernel_median_ns={},kernel_p95_ns={},materialize_times_ns={},materialize_median_ns={},materialize_p95_ns={},bits_per_second={},logical_xor_per_second={},packed_updates_per_second={},state_bytes={},checksum={:016x},stable={},rss_before_kib={},rss_after_kib={},hwm_after_kib={}",
        panel,
        bench.bits,
        format_times(&bench.kernel_times),
        bench.kernel_median,
        bench.kernel_p95,
        format_times(&bench.materialize_times),
        bench.materialize_median,
        bench.materialize_p95,
        bits_per_second,
        xor_per_second,
        packed_per_second,
        state_bytes(bench.bits),
        bench.checksum,
        u8::from(bench.stable),
        bench.rss_before_kib.map_or_else(|| "NA".to_string(), |value| value.to_string()),
        bench.rss_after_kib.map_or_else(|| "NA".to_string(), |value| value.to_string()),
        bench.hwm_after_kib.map_or_else(|| "NA".to_string(), |value| value.to_string()),
    );
}

fn extension_correctness(bits: usize) -> (u64, u64, bool, u64) {
    let (analytic_mismatches, stats) = analytic_fixture_mismatches(bits);
    let dense_sources = sources(bits, EXTENSION_SOURCES);
    let mut inverse_mismatches = 0_u64;
    let mut transformed_checksum = 0_u64;

    for source in &dense_sources {
        let mut transformed = source.clone();
        pascal_gf2_in_place(&mut transformed).expect("PB9 extension transform");
        transformed_checksum ^= word_checksum(&transformed);
        pascal_gf2_in_place(&mut transformed).expect("PB9 extension inverse");
        inverse_mismatches += u64::from(transformed != *source);
    }

    (
        analytic_mismatches,
        inverse_mismatches,
        stats_match(bits, stats),
        transformed_checksum,
    )
}

fn query_schedule(bits: usize) -> Vec<usize> {
    (0..bits)
        .map(|index| (405 * index + 17) % bits)
        .collect()
}

fn direct_reads_for_prefix(addresses: &[usize], q: usize) -> u64 {
    addresses
        .iter()
        .take(q)
        .map(|address| 1_u64 << address.count_ones())
        .sum()
}

fn query_pascal(
    sources: &[Vec<u64>],
    addresses: &[usize],
    q: usize,
    workspace: &mut [u64],
) -> u64 {
    let mut checksum = 0_u64;
    for (sample, source) in sources.iter().enumerate() {
        workspace.copy_from_slice(source);
        pascal_gf2_in_place(workspace).expect("PB9 query Pascal");
        for (position, &address) in addresses.iter().take(q).enumerate() {
            checksum = checksum_push(
                checksum,
                get_bit(workspace, address),
                sample,
                address,
                position,
            );
        }
    }
    checksum
}

fn query_direct(sources: &[Vec<u64>], addresses: &[usize], q: usize) -> u64 {
    let mut checksum = 0_u64;
    for (sample, source) in sources.iter().enumerate() {
        for (position, &address) in addresses.iter().take(q).enumerate() {
            checksum = checksum_push(
                checksum,
                direct_submask_parity(source, address),
                sample,
                address,
                position,
            );
        }
    }
    checksum
}

fn query_copy(
    sources: &[Vec<u64>],
    addresses: &[usize],
    q: usize,
    workspace: &mut [u64],
) -> u64 {
    let mut checksum = 0_u64;
    for (sample, source) in sources.iter().enumerate() {
        workspace.copy_from_slice(source);
        for (position, &address) in addresses.iter().take(q).enumerate() {
            checksum = checksum_push(
                checksum,
                get_bit(workspace, address),
                sample,
                address,
                position,
            );
        }
    }
    checksum
}

#[derive(Debug, Clone)]
struct QueryBench {
    bits: usize,
    q: usize,
    a_times: Vec<u128>,
    b_times: Vec<u128>,
    c_times: Vec<u128>,
    a_median: u128,
    b_median: u128,
    c_median: u128,
    a_p95: u128,
    b_p95: u128,
    c_p95: u128,
    checksum: u64,
    checksums_equal: bool,
    paired_wins: usize,
    direct_reads_per_state: u64,
}

fn bench_query(bits: usize, q: usize) -> QueryBench {
    let query_sources = sources(bits, QUERY_SOURCES);
    let addresses = query_schedule(bits);
    let mut workspace = vec![0_u64; words_for(bits)];

    for warmup in 0..QUERY_WARMUPS {
        if warmup % 2 == 0 {
            black_box(query_pascal(
                &query_sources,
                &addresses,
                q,
                &mut workspace,
            ));
            black_box(query_direct(&query_sources, &addresses, q));
        } else {
            black_box(query_direct(&query_sources, &addresses, q));
            black_box(query_pascal(
                &query_sources,
                &addresses,
                q,
                &mut workspace,
            ));
        }
        black_box(query_copy(
            &query_sources,
            &addresses,
            q,
            &mut workspace,
        ));
    }

    let mut a_times = Vec::with_capacity(QUERY_REPS);
    let mut b_times = Vec::with_capacity(QUERY_REPS);
    let mut c_times = Vec::with_capacity(QUERY_REPS);
    let mut a_checksums = Vec::with_capacity(QUERY_REPS);
    let mut b_checksums = Vec::with_capacity(QUERY_REPS);

    for repetition in 0..QUERY_REPS {
        if repetition % 2 == 0 {
            let start = Instant::now();
            let checksum = query_pascal(&query_sources, &addresses, q, &mut workspace);
            a_times.push(start.elapsed().as_nanos());
            a_checksums.push(checksum);

            let start = Instant::now();
            let checksum = query_direct(&query_sources, &addresses, q);
            b_times.push(start.elapsed().as_nanos());
            b_checksums.push(checksum);
        } else {
            let start = Instant::now();
            let checksum = query_direct(&query_sources, &addresses, q);
            b_times.push(start.elapsed().as_nanos());
            b_checksums.push(checksum);

            let start = Instant::now();
            let checksum = query_pascal(&query_sources, &addresses, q, &mut workspace);
            a_times.push(start.elapsed().as_nanos());
            a_checksums.push(checksum);
        }

        let start = Instant::now();
        black_box(query_copy(
            &query_sources,
            &addresses,
            q,
            &mut workspace,
        ));
        c_times.push(start.elapsed().as_nanos());
    }

    let checksum = a_checksums.first().copied().unwrap_or_default();
    let checksums_equal = a_checksums.iter().all(|&value| value == checksum)
        && b_checksums.iter().all(|&value| value == checksum);
    let paired_wins = (0..QUERY_REPS)
        .filter(|&index| a_times[index] < b_times[index])
        .count();

    QueryBench {
        bits,
        q,
        a_median: median(&a_times),
        b_median: median(&b_times),
        c_median: median(&c_times),
        a_p95: percentile(&a_times, 95, 100),
        b_p95: percentile(&b_times, 95, 100),
        c_p95: percentile(&c_times, 95, 100),
        a_times,
        b_times,
        c_times,
        checksum,
        checksums_equal,
        paired_wins,
        direct_reads_per_state: direct_reads_for_prefix(&addresses, q),
    }
}

fn emit_query_bench(bench: &QueryBench) {
    println!(
        "PB9_QUERY,bits={},q={},a_times_ns={},a_median_ns={},a_p95_ns={},b_times_ns={},b_median_ns={},b_p95_ns={},c_times_ns={},c_median_ns={},c_p95_ns={},checksum={:016x},checksums_equal={},paired_wins_a_of_12={},direct_reads_per_state={}",
        bench.bits,
        bench.q,
        format_times(&bench.a_times),
        bench.a_median,
        bench.a_p95,
        format_times(&bench.b_times),
        bench.b_median,
        bench.b_p95,
        format_times(&bench.c_times),
        bench.c_median,
        bench.c_p95,
        bench.checksum,
        u8::from(bench.checksums_equal),
        bench.paired_wins,
        bench.direct_reads_per_state,
    );
}

fn benchmark() {
    let deterministic = run_deterministic(false);
    let runner = env::var("RUNNER_NAME").unwrap_or_else(|_| "UNKNOWN".to_string());
    let arch = env::consts::ARCH;

    println!(
        "PB9_BENCH_BEGIN,protocol=1,runner={},arch={},mandatory_widths=8,extension_widths=8,semantic_buffer_cap_bytes={SEMANTIC_BUFFER_CAP}",
        runner, arch
    );

    let mut mandatory_benches = Vec::new();
    for bits in MANDATORY_WIDTHS {
        let bench = bench_transform(bits, MANDATORY_WARMUPS, MANDATORY_REPS);
        emit_transform_bench("MANDATORY", &bench);
        mandatory_benches.push(bench);
    }

    let mut query_benches = Vec::new();
    for bits in QUERY_WIDTHS {
        for q in QUERY_Q {
            let bench = bench_query(bits, q);
            emit_query_bench(&bench);
            query_benches.push(bench);
        }
    }

    let mut extension_valid = true;
    let mut previous_materialize_median = mandatory_benches
        .last()
        .map_or(0, |bench| bench.materialize_median);
    let mut furthest_width = *MANDATORY_WIDTHS.last().expect("mandatory widths");
    let mut stop_reason: Option<(&'static str, usize)> = None;

    for bits in EXTENSION_WIDTHS {
        let required_buffers = state_bytes(bits).saturating_mul(3);
        if required_buffers > SEMANTIC_BUFFER_CAP {
            stop_reason = Some(("SEMANTIC_BUFFER_CAP", bits));
            break;
        }
        if previous_materialize_median > PREVIOUS_MATERIALIZE_STOP_NS {
            stop_reason = Some(("PREVIOUS_MATERIALIZE_LATENCY", bits));
            break;
        }

        let mut probe = source_words(bits, 211);
        let probe_start = Instant::now();
        let probe_result = pascal_gf2_in_place(&mut probe);
        let probe_ns = probe_start.elapsed().as_nanos();
        if probe_result.is_err() {
            stop_reason = Some(("PROBE_TRANSFORM_ERROR", bits));
            break;
        }
        if probe_ns > PROBE_STOP_NS {
            stop_reason = Some(("PROBE_LATENCY", bits));
            break;
        }

        let (analytic, inverse, accounting, checksum) = extension_correctness(bits);
        println!(
            "PB9_EXTENSION_CORRECTNESS,bits={},analytic_mismatches={},inverse_mismatches={},accounting_match={},probe_ns={},state_bytes={},semantic_buffers_bytes={},checksum={:016x}",
            bits,
            analytic,
            inverse,
            u8::from(accounting),
            probe_ns,
            state_bytes(bits),
            required_buffers,
            checksum
        );
        if analytic != 0 || inverse != 0 || !accounting {
            extension_valid = false;
            println!("PB9_EXTENSION_STOP,bits={bits},reason=CORRECTNESS_OR_ACCOUNTING");
            break;
        }

        let reps = if bits <= 67_108_864 {
            EXTENSION_REPS
        } else {
            DEEP_REPS
        };
        let bench = bench_transform(bits, EXTENSION_WARMUPS, reps);
        emit_transform_bench(
            if bits <= 67_108_864 {
                "EXTENDED"
            } else {
                "DEEP"
            },
            &bench,
        );
        if !bench.stable || bench.kernel_times.len() != reps || bench.materialize_times.len() != reps
        {
            extension_valid = false;
            println!("PB9_EXTENSION_STOP,bits={bits},reason=BENCHMARK_SANITY");
            break;
        }
        previous_materialize_median = bench.materialize_median;
        furthest_width = bits;
    }

    if let Some((reason, bits)) = stop_reason {
        println!("PB9_EXTENSION_STOP,bits={bits},reason={reason}");
    } else if extension_valid && furthest_width == *EXTENSION_WIDTHS.last().expect("extensions") {
        println!("PB9_EXTENSION_COMPLETE,furthest_bits={furthest_width}");
    }

    let runner_valid = runner == EXPECTED_RUNNER && arch == "aarch64";
    let l3 = runner_valid
        && mandatory_benches.len() == MANDATORY_WIDTHS.len()
        && mandatory_benches.iter().all(|bench| {
            bench.kernel_times.len() == MANDATORY_REPS
                && bench.materialize_times.len() == MANDATORY_REPS
                && bench.kernel_times.iter().all(|&value| value > 0)
                && bench.materialize_times.iter().all(|&value| value > 0)
                && bench.stable
                && bench.kernel_median > 0
                && bench.materialize_median > 0
        });

    let l4 = deterministic
        .widths
        .iter()
        .any(|evidence| evidence.bits == 1_048_576)
        && mandatory_benches
            .iter()
            .any(|bench| bench.bits == 1_048_576 && bench.stable);

    let one_million = query_benches
        .iter()
        .find(|bench| bench.bits == 1_048_576 && bench.q == 2_048);
    let l5 = one_million.is_some_and(|bench| {
        bench.checksums_equal
            && 10_u128 * bench.a_median <= 9_u128 * bench.b_median
            && bench.paired_wins >= 9
    });
    let query_sanity = query_benches.len() == QUERY_WIDTHS.len() * QUERY_Q.len()
        && query_benches.iter().all(|bench| {
            bench.checksums_equal
                && bench.a_times.len() == QUERY_REPS
                && bench.b_times.len() == QUERY_REPS
                && bench.c_times.len() == QUERY_REPS
                && bench.a_times.iter().all(|&value| value > 0)
                && bench.b_times.iter().all(|&value| value > 0)
                && bench.c_times.iter().all(|&value| value > 0)
        });
    let l6 = extension_valid;

    println!(
        "PB9_GATE,L1_MANDATORY_EXACTNESS,{}",
        if deterministic.l1 { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB9_GATE,L2_ACCOUNTING_EXACTNESS,{}",
        if deterministic.l2 { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB9_GATE,L3_MANDATORY_PHYSICAL_SANITY,{}",
        if l3 && query_sanity { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB9_GATE,L4_ONE_MILLION_EXECUTION,{}",
        if l4 { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB9_GATE,L5_ONE_MILLION_TASK_UTILITY,{}",
        if l5 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB9_GATE,L6_EXTENSION_INTEGRITY,{}",
        if l6 { "HOLDS" } else { "INVALID" }
    );
    println!("PB9_FURTHEST_EXECUTED,bits={furthest_width}");

    let mandatory_valid =
        deterministic.l1 && deterministic.l2 && l3 && query_sanity && l4 && l6;
    let decision = if !mandatory_valid {
        "INVALID"
    } else if l5 {
        "RETAIN_LARGE_WIDTH_SCALING_AND_1M_UTILITY"
    } else {
        "RETAIN_LARGE_WIDTH_SCALING_ONLY"
    };
    println!("PB9_DECISION,{decision}");
    println!("PROMOTION,BLOCKED,pb9_large_width_scaling_only");
}

fn main() {
    let mode = env::args()
        .nth(1)
        .unwrap_or_else(|| "deterministic".to_string());

    match mode.as_str() {
        "deterministic" => {
            run_deterministic(true);
        }
        "benchmark" => benchmark(),
        other => panic!("unknown PB9 mode: {other}"),
    }
}
