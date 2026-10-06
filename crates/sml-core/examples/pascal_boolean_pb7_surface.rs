//! PB-7 Pascal utility-surface generalization.
//!
//! Frozen protocol:
//! docs/SML-PASCAL-BOOLEAN-PB7-UTILITY-SURFACE-PREREGISTRATION.md.
//!
//! Modes:
//! - deterministic: byte-stable exactness and structural evidence.
//! - benchmark: physical timing on the pinned ARM64 runner.

use std::{collections::BTreeSet, env, hint::black_box, time::Instant};

use sml_core::pascal_gf2::pascal_gf2_in_place;

const WIDTHS: [usize; 7] = [64, 128, 256, 512, 1024, 2048, 4096];
const DETERMINISTIC_CORPUS: usize = 64;
const PHYSICAL_CORPUS: usize = 8;
const WARMUP_REPS: usize = 2;
const MEASURED_REPS: usize = 12;
const EXPECTED_RUNNER: &str = "tarek-scirust-arm64-01";
const BASE: u64 = 0x5042_3755_5449_4c31;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ScheduleKind {
    AffineUniform,
    LowPopcount,
    HighPopcount,
}

impl ScheduleKind {
    const ALL: [Self; 3] = [Self::AffineUniform, Self::LowPopcount, Self::HighPopcount];

    const fn name(self) -> &'static str {
        match self {
            Self::AffineUniform => "AFFINE_UNIFORM",
            Self::LowPopcount => "LOW_POPCOUNT",
            Self::HighPopcount => "HIGH_POPCOUNT",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Arm {
    PackedPascal,
    Direct,
    GenericFull,
}

impl Arm {
    const fn name(self) -> &'static str {
        match self {
            Self::PackedPascal => "PACKED_PASCAL_ZETA",
            Self::Direct => "DIRECT_ON_DEMAND",
            Self::GenericFull => "GENERIC_FULL_PRECOMPUTE",
        }
    }
}

fn get_bit(words: &[u64], index: usize) -> bool {
    words[index / 64] & (1_u64 << (index % 64)) != 0
}

fn words_from_seed(word_count: usize, seed: u64) -> Vec<u64> {
    let mut stream = Stream::new(seed);
    (0..word_count).map(|_| stream.next()).collect()
}

fn sources(bits: usize, count: usize) -> Vec<Vec<u64>> {
    let words = bits / 64;
    (0..count)
        .map(|sample| words_from_seed(words, BASE ^ ((bits as u64) << 32) ^ (sample as u64)))
        .collect()
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

fn direct_truth_vector(words: &[u64], bits: usize) -> Vec<bool> {
    (0..bits)
        .map(|address| direct_submask_parity(words, address))
        .collect()
}

fn logical_fast_zeta(words: &[u64], bits: usize) -> Vec<bool> {
    let mut output = (0..bits)
        .map(|index| get_bit(words, index))
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

fn schedule(bits: usize, kind: ScheduleKind) -> Vec<usize> {
    match kind {
        ScheduleKind::AffineUniform => (0..bits).map(|index| (405 * index + 17) % bits).collect(),
        ScheduleKind::LowPopcount => {
            let mut addresses = (0..bits).collect::<Vec<_>>();
            addresses.sort_by_key(|&address| (address.count_ones(), address));
            addresses
        }
        ScheduleKind::HighPopcount => {
            let mut addresses = (0..bits).collect::<Vec<_>>();
            addresses.sort_by(|left, right| {
                right
                    .count_ones()
                    .cmp(&left.count_ones())
                    .then_with(|| left.cmp(right))
            });
            addresses
        }
    }
}

fn q_values(bits: usize) -> Vec<usize> {
    let mut values = vec![1, (bits / 64).max(1), bits / 16, bits / 4, bits / 2, bits];
    values.sort_unstable();
    values.dedup();
    values
}

fn zeta_xor_ops(bits: usize) -> u64 {
    (bits as u64 / 2) * u64::from(bits.ilog2())
}

fn full_direct_reads(bits: usize) -> u64 {
    3_u64.pow(bits.ilog2())
}

fn direct_reads(addresses: &[usize], q: usize) -> u64 {
    addresses
        .iter()
        .take(q)
        .map(|address| 1_u64 << address.count_ones())
        .sum()
}

fn checksum_push(
    checksum: u64,
    value: bool,
    sample: usize,
    address: usize,
    position: usize,
) -> u64 {
    let token = ((sample as u64) << 48)
        ^ ((address as u64) << 20)
        ^ position as u64
        ^ if value {
            0xd6e8_feb8_6659_fd93
        } else {
            0xa5a3_564e_27f1_886b
        };
    checksum.rotate_left(11).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ token
}

#[derive(Debug, Clone)]
struct CellEvidence {
    bits: usize,
    schedule: ScheduleKind,
    q: usize,
    answer_count: u64,
    a_mismatches: u64,
    b_mismatches: u64,
    c_mismatches: u64,
    a_checksum: u64,
    b_checksum: u64,
    c_checksum: u64,
    oracle_checksum: u64,
    direct_reads: u64,
    direct_xor_reductions: u64,
    zeta_xor_ops: u64,
    full_direct_reads: u64,
}

#[derive(Debug, Clone)]
struct WidthEvidence {
    bits: usize,
    source_collisions: usize,
    packed_logical_mismatches: u64,
    packed_oracle_mismatches: u64,
}

#[derive(Debug)]
struct DeterministicEvidence {
    widths: Vec<WidthEvidence>,
    cells: Vec<CellEvidence>,
    g1: bool,
    g2: bool,
    g3: bool,
    g4: bool,
}

fn earliest_crossover(bits: usize, kind: ScheduleKind) -> Option<usize> {
    let addresses = schedule(bits, kind);
    q_values(bits)
        .into_iter()
        .find(|&q| zeta_xor_ops(bits) + (q as u64) < direct_reads(&addresses, q))
}

fn run_deterministic(emit: bool) -> DeterministicEvidence {
    let mut width_evidence = Vec::new();
    let mut cells = Vec::new();

    for bits in WIDTHS {
        let width_sources = sources(bits, DETERMINISTIC_CORPUS);
        let unique_sources = width_sources.iter().cloned().collect::<BTreeSet<_>>().len();
        let source_collisions = DETERMINISTIC_CORPUS - unique_sources;
        let mut packed_logical_mismatches = 0_u64;
        let mut packed_oracle_mismatches = 0_u64;

        let schedules = ScheduleKind::ALL
            .into_iter()
            .map(|kind| (kind, schedule(bits, kind)))
            .collect::<Vec<_>>();
        let qs = q_values(bits);

        let mut per_cell = schedules
            .iter()
            .flat_map(|(kind, _)| {
                qs.iter().map(move |&q| CellEvidence {
                    bits,
                    schedule: *kind,
                    q,
                    answer_count: 0,
                    a_mismatches: 0,
                    b_mismatches: 0,
                    c_mismatches: 0,
                    a_checksum: 0,
                    b_checksum: 0,
                    c_checksum: 0,
                    oracle_checksum: 0,
                    direct_reads: 0,
                    direct_xor_reductions: 0,
                    zeta_xor_ops: zeta_xor_ops(bits),
                    full_direct_reads: full_direct_reads(bits),
                })
            })
            .collect::<Vec<_>>();

        for (sample, source) in width_sources.iter().enumerate() {
            let oracle = direct_truth_vector(source, bits);
            let logical = logical_fast_zeta(source, bits);
            let mut packed = source.clone();
            let stats = pascal_gf2_in_place(&mut packed).expect("PB7 packed Pascal transform");
            assert_eq!(stats.bits, bits);
            assert_eq!(stats.logical_xor_ops, zeta_xor_ops(bits));
            assert_eq!(stats.state_bytes, bits / 8);
            assert_eq!(stats.scratch_words, 0);

            for address in 0..bits {
                let packed_value = get_bit(&packed, address);
                packed_logical_mismatches += u64::from(packed_value != logical[address]);
                packed_oracle_mismatches += u64::from(packed_value != oracle[address]);
            }

            let mut cell_index = 0_usize;
            for (_, addresses) in &schedules {
                for &q in &qs {
                    let cell = &mut per_cell[cell_index];
                    for (position, &address) in addresses.iter().take(q).enumerate() {
                        let expected = oracle[address];
                        let a = get_bit(&packed, address);
                        let b = oracle[address];
                        let c = oracle[address];

                        cell.answer_count += 1;
                        cell.a_mismatches += u64::from(a != expected);
                        cell.b_mismatches += u64::from(b != expected);
                        cell.c_mismatches += u64::from(c != expected);
                        cell.a_checksum =
                            checksum_push(cell.a_checksum, a, sample, address, position);
                        cell.b_checksum =
                            checksum_push(cell.b_checksum, b, sample, address, position);
                        cell.c_checksum =
                            checksum_push(cell.c_checksum, c, sample, address, position);
                        cell.oracle_checksum = checksum_push(
                            cell.oracle_checksum,
                            expected,
                            sample,
                            address,
                            position,
                        );
                    }
                    cell_index += 1;
                }
            }
        }

        for cell in &mut per_cell {
            let addresses = schedule(bits, cell.schedule);
            cell.direct_reads = direct_reads(&addresses, cell.q);
            cell.direct_xor_reductions = cell.direct_reads - cell.q as u64;
        }

        width_evidence.push(WidthEvidence {
            bits,
            source_collisions,
            packed_logical_mismatches,
            packed_oracle_mismatches,
        });
        cells.extend(per_cell);
    }

    let g1 = width_evidence.iter().all(|width| {
        width.source_collisions == 0
            && width.packed_logical_mismatches == 0
            && width.packed_oracle_mismatches == 0
    }) && cells.iter().all(|cell| {
        cell.a_mismatches == 0
            && cell.b_mismatches == 0
            && cell.c_mismatches == 0
            && cell.a_checksum == cell.oracle_checksum
            && cell.b_checksum == cell.oracle_checksum
            && cell.c_checksum == cell.oracle_checksum
            && cell.answer_count == (DETERMINISTIC_CORPUS * cell.q) as u64
    });

    let g2 = WIDTHS.into_iter().all(|bits| bits / 8 == bits.div_ceil(8));

    let full_domain_advantage = WIDTHS
        .into_iter()
        .all(|bits| zeta_xor_ops(bits) + (bits as u64) < full_direct_reads(bits) + bits as u64);
    let ratios_monotonic = WIDTHS.windows(2).all(|pair| {
        let left = pair[0];
        let right = pair[1];
        full_direct_reads(right) * zeta_xor_ops(left)
            > full_direct_reads(left) * zeta_xor_ops(right)
    });
    let g3 = full_domain_advantage && ratios_monotonic;

    let g4 = WIDTHS.into_iter().all(|bits| {
        let low = schedule(bits, ScheduleKind::LowPopcount);
        let affine = schedule(bits, ScheduleKind::AffineUniform);
        let high = schedule(bits, ScheduleKind::HighPopcount);
        q_values(bits).into_iter().all(|q| {
            let low_reads = direct_reads(&low, q);
            let affine_reads = direct_reads(&affine, q);
            let high_reads = direct_reads(&high, q);
            low_reads <= affine_reads && affine_reads <= high_reads
        })
    });

    if emit {
        println!(
            "PB7_DETERMINISTIC_BEGIN,protocol=1,widths=64|128|256|512|1024|2048|4096,states_per_width={DETERMINISTIC_CORPUS},schedules=AFFINE_UNIFORM|LOW_POPCOUNT|HIGH_POPCOUNT"
        );
        for width in &width_evidence {
            println!(
                "PB7_WIDTH,bits={},source_collisions={},packed_logical_mismatches={},packed_oracle_mismatches={},zeta_xor_ops={},full_direct_reads={},answer_bytes={}",
                width.bits,
                width.source_collisions,
                width.packed_logical_mismatches,
                width.packed_oracle_mismatches,
                zeta_xor_ops(width.bits),
                full_direct_reads(width.bits),
                width.bits / 8
            );
            for kind in ScheduleKind::ALL {
                let crossover = earliest_crossover(width.bits, kind)
                    .map_or_else(|| "NONE".to_string(), |q| q.to_string());
                println!(
                    "PB7_CROSSOVER,bits={},schedule={},earliest_frozen_q={}",
                    width.bits,
                    kind.name(),
                    crossover
                );
            }
        }

        for cell in &cells {
            println!(
                "PB7_CELL,bits={},schedule={},q={},answers={},a_mismatches={},b_mismatches={},c_mismatches={},a_checksum={:016x},b_checksum={:016x},c_checksum={:016x},oracle_checksum={:016x},direct_reads_per_state={},direct_xor_reductions_per_state={},zeta_xor_per_state={},zeta_query_reads_per_state={},full_direct_reads_per_state={},answer_bytes={}",
                cell.bits,
                cell.schedule.name(),
                cell.q,
                cell.answer_count,
                cell.a_mismatches,
                cell.b_mismatches,
                cell.c_mismatches,
                cell.a_checksum,
                cell.b_checksum,
                cell.c_checksum,
                cell.oracle_checksum,
                cell.direct_reads,
                cell.direct_xor_reductions,
                cell.zeta_xor_ops,
                cell.q,
                cell.full_direct_reads,
                cell.bits / 8
            );
        }

        println!(
            "PB7_GATE,G1_EXACTNESS,{}",
            if g1 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB7_GATE,G2_STORAGE_PARITY,{}",
            if g2 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB7_GATE,G3_FULL_DOMAIN_STRUCTURAL_ADVANTAGE,{}",
            if g3 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB7_GATE,G4_ADDRESS_POPCOUNT_BOUNDS,{}",
            if g4 { "HOLDS" } else { "REFUTED" }
        );
        println!("PB7_DETERMINISTIC_DECISION,PENDING_PHYSICAL_G5_G6");
        println!("PROMOTION,BLOCKED,pb7_requires_physical_generalization");
    }

    DeterministicEvidence {
        widths: width_evidence,
        cells,
        g1,
        g2,
        g3,
        g4,
    }
}

fn run_benchmark_arm(
    arm: Arm,
    bits: usize,
    width_sources: &[Vec<u64>],
    addresses: &[usize],
    q: usize,
) -> u64 {
    let mut checksum = 0_u64;

    for (sample, source) in width_sources.iter().enumerate() {
        let source = black_box(source);
        match arm {
            Arm::PackedPascal => {
                let mut transformed = source.clone();
                pascal_gf2_in_place(&mut transformed).expect("PB7 benchmark Pascal");
                for (position, &address) in addresses.iter().take(q).enumerate() {
                    checksum = checksum_push(
                        checksum,
                        get_bit(&transformed, address),
                        sample,
                        address,
                        position,
                    );
                }
            }
            Arm::Direct => {
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
            Arm::GenericFull => {
                let generic = direct_truth_vector(source, bits);
                for (position, &address) in addresses.iter().take(q).enumerate() {
                    checksum = checksum_push(checksum, generic[address], sample, address, position);
                }
            }
        }
    }

    checksum
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

#[derive(Debug, Clone)]
struct BenchResult {
    bits: usize,
    schedule: ScheduleKind,
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
    a_checksum: u64,
    b_checksum: u64,
    c_checksum: u64,
    checksums_equal: bool,
    paired_wins: usize,
    ten_percent_gate: bool,
}

fn benchmark_cell(bits: usize, kind: ScheduleKind, q: usize) -> BenchResult {
    let width_sources = sources(bits, PHYSICAL_CORPUS);
    let addresses = schedule(bits, kind);

    for warmup in 0..WARMUP_REPS {
        let order = match warmup % 3 {
            0 => [Arm::PackedPascal, Arm::Direct, Arm::GenericFull],
            1 => [Arm::Direct, Arm::GenericFull, Arm::PackedPascal],
            _ => [Arm::GenericFull, Arm::PackedPascal, Arm::Direct],
        };
        for arm in order {
            black_box(run_benchmark_arm(arm, bits, &width_sources, &addresses, q));
        }
    }

    let mut a_times = Vec::with_capacity(MEASURED_REPS);
    let mut b_times = Vec::with_capacity(MEASURED_REPS);
    let mut c_times = Vec::with_capacity(MEASURED_REPS);
    let mut a_checksums = Vec::with_capacity(MEASURED_REPS);
    let mut b_checksums = Vec::with_capacity(MEASURED_REPS);
    let mut c_checksums = Vec::with_capacity(MEASURED_REPS);

    for repetition in 0..MEASURED_REPS {
        let order = match repetition % 3 {
            0 => [Arm::PackedPascal, Arm::Direct, Arm::GenericFull],
            1 => [Arm::Direct, Arm::GenericFull, Arm::PackedPascal],
            _ => [Arm::GenericFull, Arm::PackedPascal, Arm::Direct],
        };

        for arm in order {
            let start = Instant::now();
            let checksum = run_benchmark_arm(arm, bits, &width_sources, &addresses, q);
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            black_box(checksum);

            match arm {
                Arm::PackedPascal => {
                    a_times.push(elapsed);
                    a_checksums.push(checksum);
                }
                Arm::Direct => {
                    b_times.push(elapsed);
                    b_checksums.push(checksum);
                }
                Arm::GenericFull => {
                    c_times.push(elapsed);
                    c_checksums.push(checksum);
                }
            }
        }
    }

    let a_median = median(&a_times);
    let b_median = median(&b_times);
    let c_median = median(&c_times);
    let a_checksum = a_checksums.first().copied().unwrap_or_default();
    let b_checksum = b_checksums.first().copied().unwrap_or_default();
    let c_checksum = c_checksums.first().copied().unwrap_or_default();
    let stable_a = a_checksums.iter().all(|&value| value == a_checksum);
    let stable_b = b_checksums
        .first()
        .is_some_and(|&first| b_checksums.iter().all(|&value| value == first));
    let stable_c = c_checksums
        .first()
        .is_some_and(|&first| c_checksums.iter().all(|&value| value == first));
    let checksums_equal =
        stable_a && stable_b && stable_c && b_checksum == a_checksum && c_checksum == a_checksum;

    let paired_wins = (0..MEASURED_REPS)
        .filter(|&index| a_times[index] < b_times[index].min(c_times[index]))
        .count();
    let faster_control = b_median.min(c_median);
    let ten_percent_gate = 10_u128 * a_median <= 9_u128 * faster_control;

    BenchResult {
        bits,
        schedule: kind,
        q,
        a_p95: percentile(&a_times, 95, 100),
        b_p95: percentile(&b_times, 95, 100),
        c_p95: percentile(&c_times, 95, 100),
        a_times,
        b_times,
        c_times,
        a_median,
        b_median,
        c_median,
        a_checksum,
        b_checksum,
        c_checksum,
        checksums_equal,
        paired_wins,
        ten_percent_gate,
    }
}

fn physical_cells() -> Vec<(usize, ScheduleKind, usize)> {
    let mut cells = Vec::new();

    for bits in WIDTHS {
        cells.push((bits, ScheduleKind::AffineUniform, bits));
    }

    for kind in ScheduleKind::ALL {
        for q in [32_usize, 128, 512, 1024, 2048] {
            let cell = (2048, kind, q);
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
    }

    cells
}

fn benchmark() {
    let deterministic = run_deterministic(false);
    let runner = env::var("RUNNER_NAME").unwrap_or_else(|_| "UNKNOWN".to_string());
    let arch = env::consts::ARCH;
    let cells = physical_cells();

    println!(
        "PB7_BENCH_BEGIN,protocol=1,runner={},arch={},physical_states_per_cell={PHYSICAL_CORPUS},warmups={WARMUP_REPS},measured_reps={MEASURED_REPS},cells={}",
        runner,
        arch,
        cells.len()
    );

    let mut results = Vec::with_capacity(cells.len());
    for (bits, kind, q) in cells {
        let result = benchmark_cell(bits, kind, q);
        let states_per_second = |elapsed_ns: u128| -> u128 {
            (PHYSICAL_CORPUS as u128 * 1_000_000_000_u128) / elapsed_ns.max(1)
        };

        println!(
            "PB7_BENCH_ARM,bits={},schedule={},q={},arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
            bits,
            kind.name(),
            q,
            Arm::PackedPascal.name(),
            format_times(&result.a_times),
            result.a_median,
            result.a_p95,
            states_per_second(result.a_median),
            result.a_checksum
        );
        println!(
            "PB7_BENCH_ARM,bits={},schedule={},q={},arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
            bits,
            kind.name(),
            q,
            Arm::Direct.name(),
            format_times(&result.b_times),
            result.b_median,
            result.b_p95,
            states_per_second(result.b_median),
            result.b_checksum
        );
        println!(
            "PB7_BENCH_ARM,bits={},schedule={},q={},arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
            bits,
            kind.name(),
            q,
            Arm::GenericFull.name(),
            format_times(&result.c_times),
            result.c_median,
            result.c_p95,
            states_per_second(result.c_median),
            result.c_checksum
        );
        println!(
            "PB7_BENCH_COMPARE,bits={},schedule={},q={},faster_control_median_ns={},paired_wins_a_of_12={},ten_percent_gate={},checksums_equal={}",
            bits,
            kind.name(),
            q,
            result.b_median.min(result.c_median),
            result.paired_wins,
            u8::from(result.ten_percent_gate),
            u8::from(result.checksums_equal)
        );
        results.push(result);
    }

    let g5 = results
        .iter()
        .find(|result| {
            result.bits == 2048
                && result.schedule == ScheduleKind::AffineUniform
                && result.q == 2048
        })
        .is_some_and(|result| {
            result.checksums_equal && result.ten_percent_gate && result.paired_wins >= 9
        });

    let g6 = [512_usize, 1024, 2048, 4096].into_iter().all(|bits| {
        results
            .iter()
            .find(|result| {
                result.bits == bits
                    && result.schedule == ScheduleKind::AffineUniform
                    && result.q == bits
            })
            .is_some_and(|result| result.checksums_equal && result.ten_percent_gate)
    });

    let repetitions_valid = results.iter().all(|result| {
        result.a_times.len() == MEASURED_REPS
            && result.b_times.len() == MEASURED_REPS
            && result.c_times.len() == MEASURED_REPS
            && result.a_times.iter().all(|&value| value > 0)
            && result.b_times.iter().all(|&value| value > 0)
            && result.c_times.iter().all(|&value| value > 0)
    });
    let runner_valid = runner == EXPECTED_RUNNER && arch == "aarch64";
    let deterministic_valid =
        deterministic.g1 && deterministic.g2 && deterministic.g3 && deterministic.g4;
    let cells_valid = results.len() == physical_cells().len()
        && results
            .iter()
            .all(|result| result.checksums_equal && result.q > 0 && result.q <= result.bits);
    let g7 = repetitions_valid
        && runner_valid
        && deterministic_valid
        && cells_valid
        && deterministic.widths.len() == WIDTHS.len()
        && !deterministic.cells.is_empty();

    println!(
        "PB7_GATE,G5_PB6_SIGNAL_REPLICATION,{}",
        if g5 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB7_GATE,G6_WIDTH_GENERALIZATION,{}",
        if g6 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB7_GATE,G7_BENCHMARK_SANITY,{}",
        if g7 { "HOLDS" } else { "INVALID" }
    );

    let decision = if !g7 {
        "INVALID"
    } else if deterministic_valid && g5 && g6 {
        "RETAIN_GENERALIZED_UTILITY_SURFACE"
    } else {
        "RETAIN_MIXED_UTILITY_SURFACE"
    };
    println!("PB7_DECISION,{decision}");
    println!("PROMOTION,BLOCKED,pb7_utility_surface_only");
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
        other => panic!("unknown PB7 mode: {other}"),
    }
}
