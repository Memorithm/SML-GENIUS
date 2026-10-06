//! PB-6 matched-control Pascal task-utility experiment at K=2048.
//!
//! Frozen protocol: docs/SML-PASCAL-BOOLEAN-PB6-TASK-UTILITY-PREREGISTRATION.md.
//! Modes:
//! - deterministic: byte-stable exactness/work evidence.
//! - benchmark: non-deterministic physical timing on the pinned runner.

use std::{collections::BTreeSet, env, hint::black_box, time::Instant};

use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    pascal_gf2::pascal_gf2_in_place,
    ExecutionCounters,
};

const BITS: usize = 2048;
const WORDS: usize = BITS / 64;
const HALF_BITS: usize = BITS / 2;
const HALF_WORDS: usize = HALF_BITS / 64;
const VARS: usize = 11;
const CORPUS: usize = 64;
const FEISTEL_ROUNDS: usize = 4;
const Q_VALUES: [usize; 8] = [1, 16, 64, 128, 256, 512, 1024, 2048];
const BASE: u64 = 0x5042_3655_5449_4c31;
const P_ZETA_XOR_OPS: u64 = 11_264;
const P_ZETA_PACKED_UPDATES: u64 = 272;
const FULL_DIRECT_READS: u64 = 177_147;
const MEASURED_REPS: usize = 20;
const WARMUP_REPS: usize = 3;
const EXPECTED_RUNNER: &str = "tarek-scirust-arm64-01";

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

fn words_from_seed(word_count: usize, seed: u64) -> Vec<u64> {
    let mut stream = Stream::new(seed);
    (0..word_count).map(|_| stream.next()).collect()
}

fn get_bit(words: &[u64], index: usize) -> bool {
    (words[index / 64] & (1_u64 << (index % 64))) != 0
}

fn set_bit(words: &mut [u64], index: usize, value: bool) {
    let mask = 1_u64 << (index % 64);
    if value {
        words[index / 64] |= mask;
    } else {
        words[index / 64] &= !mask;
    }
}

fn xor_words(left: &[u64], right: &[u64]) -> Vec<u64> {
    assert_eq!(left.len(), right.len());
    left.iter().zip(right).map(|(a, b)| a ^ b).collect()
}

fn rotate_bits(input: &[u64], bits: usize, shift: usize) -> Vec<u64> {
    assert_eq!(input.len() * 64, bits);
    let shift = shift % bits;
    let mut output = vec![0_u64; input.len()];
    for index in 0..bits {
        if get_bit(input, index) {
            set_bit(&mut output, (index + shift) % bits, true);
        }
    }
    output
}

fn feistel_round(right: &[u64], round: usize) -> Vec<u64> {
    assert_eq!(right.len(), HALF_WORDS);
    let mut pascal = right.to_vec();
    pascal_gf2_in_place(&mut pascal).expect("PB6 half-width Pascal transform");
    let shift_a = (17 + 29 * round) % HALF_BITS;
    let shift_b = (43 + 53 * round) % HALF_BITS;
    let rotated_a = rotate_bits(&pascal, HALF_BITS, shift_a);
    let rotated_b = rotate_bits(&pascal, HALF_BITS, shift_b);
    for index in 0..HALF_WORDS {
        pascal[index] ^= rotated_a[index] & rotated_b[index];
    }
    pascal
}

fn feistel_forward(input: &[u64]) -> Vec<u64> {
    assert_eq!(input.len(), WORDS);
    let mut left = input[..HALF_WORDS].to_vec();
    let mut right = input[HALF_WORDS..].to_vec();

    for round in 0..FEISTEL_ROUNDS {
        let f = feistel_round(&right, round);
        let new_right = xor_words(&left, &f);
        left = right;
        right = new_right;
    }

    left.extend_from_slice(&right);
    left
}

fn feistel_inverse(input: &[u64]) -> Vec<u64> {
    assert_eq!(input.len(), WORDS);
    let mut left = input[..HALF_WORDS].to_vec();
    let mut right = input[HALF_WORDS..].to_vec();

    for round in (0..FEISTEL_ROUNDS).rev() {
        let old_right = left.clone();
        let f = feistel_round(&old_right, round);
        let old_left = xor_words(&right, &f);
        left = old_left;
        right = old_right;
    }

    left.extend_from_slice(&right);
    left
}

fn direct_submask_parity(words: &[u64], address: usize) -> bool {
    let mut parity = false;
    let mut k = address;
    loop {
        parity ^= get_bit(words, k);
        if k == 0 {
            break;
        }
        k = (k - 1) & address;
    }
    parity
}

fn direct_gate(address: usize) -> bool {
    let bit = |index: usize| address & (1_usize << index) != 0;
    bit(0) ^ (bit(1) & bit(3)) ^ (bit(2) & bit(5) & bit(8)) ^ (bit(7) & bit(9))
}

fn gate_truth_table() -> Vec<u64> {
    let mut truth = vec![0_u64; WORDS];
    for address in 0..BITS {
        if direct_gate(address) {
            set_bit(&mut truth, address, true);
        }
    }
    truth
}

fn set_indices(words: &[u64]) -> Vec<usize> {
    (0..words.len() * 64)
        .filter(|&index| get_bit(words, index))
        .collect()
}

fn polynomial_from_coefficients(coefficients: &[u64]) -> AnfPolynomial {
    let constant = get_bit(coefficients, 0);
    let mut monomials = Vec::new();
    for mask in 1..BITS {
        if !get_bit(coefficients, mask) {
            continue;
        }
        let variables = (0..VARS)
            .filter(|variable| mask & (1_usize << variable) != 0)
            .map(|variable| variable as u16)
            .collect::<Vec<_>>();
        monomials.push(AnfMonomial::new(variables).expect("PB6 canonical monomial"));
    }
    monomials.sort();
    AnfPolynomial::new(VARS, constant, monomials).expect("PB6 canonical polynomial")
}

#[derive(Debug)]
struct GateEvidence {
    values: Vec<bool>,
    coefficient_mismatches: usize,
    evaluation_mismatches: u64,
}

fn build_gate_evidence() -> GateEvidence {
    let expected = [
        1_usize,
        (1_usize << 1) | (1_usize << 3),
        (1_usize << 2) | (1_usize << 5) | (1_usize << 8),
        (1_usize << 7) | (1_usize << 9),
    ];

    let mut coefficients = gate_truth_table();
    pascal_gf2_in_place(&mut coefficients).expect("PB6 truth-table to ANF");
    let actual = set_indices(&coefficients);

    let actual_set = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    let coefficient_mismatches = actual_set.symmetric_difference(&expected_set).count();

    let polynomial = polynomial_from_coefficients(&coefficients);
    let mut counters = ExecutionCounters::default();
    let mut values = vec![false; BITS];
    let mut evaluation_mismatches = 0_u64;

    for (address, value) in values.iter_mut().enumerate() {
        let inputs = (0..VARS)
            .map(|variable| address & (1_usize << variable) != 0)
            .collect::<Vec<_>>();
        *value = polynomial
            .evaluate(&inputs, &mut counters)
            .expect("PB6 ANF gate evaluation");
        evaluation_mismatches += u64::from(*value != direct_gate(address));
    }

    GateEvidence {
        values,
        coefficient_mismatches,
        evaluation_mismatches,
    }
}

fn address_schedule() -> Vec<usize> {
    (0..BITS).map(|index| (405 * index + 17) % BITS).collect()
}

fn deterministic_sources() -> Vec<Vec<u64>> {
    (0..CORPUS)
        .map(|sample| words_from_seed(WORDS, BASE ^ sample as u64))
        .collect()
}

fn checksum_push(checksum: u64, value: bool, sample: usize, position: usize) -> u64 {
    let token = ((sample as u64) << 32)
        ^ position as u64
        ^ if value {
            0xd6e8_feb8_6659_fd93
        } else {
            0xa5a3_564e_27f1_886b
        };
    checksum.rotate_left(11).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ token
}

fn direct_reads_for_prefix(schedule: &[usize], q: usize) -> u64 {
    schedule
        .iter()
        .take(q)
        .map(|address| 1_u64 << address.count_ones())
        .sum()
}

#[derive(Debug, Clone, Copy, Default)]
struct PrefixEvidence {
    answer_count: u64,
    a_mismatches: u64,
    b_mismatches: u64,
    c_mismatches: u64,
    a_checksum: u64,
    b_checksum: u64,
    c_checksum: u64,
    oracle_checksum: u64,
}

#[derive(Debug)]
struct DeterministicEvidence {
    source_collisions: usize,
    inverse_failures: u64,
    gate_coefficient_mismatches: usize,
    gate_evaluation_mismatches: u64,
    prefixes: [PrefixEvidence; Q_VALUES.len()],
    u1: bool,
    u2: bool,
    u3: bool,
    u4: bool,
}

fn run_deterministic(emit: bool) -> DeterministicEvidence {
    let gate = build_gate_evidence();
    let schedule = address_schedule();
    let sources = deterministic_sources();
    let source_collisions = sources
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .len()
        .abs_diff(CORPUS);

    let mut prefixes = [PrefixEvidence::default(); Q_VALUES.len()];
    let mut inverse_failures = 0_u64;

    for (sample, source) in sources.iter().enumerate() {
        let encoded = feistel_forward(source);
        let decoded = feistel_inverse(&encoded);
        inverse_failures += u64::from(decoded != *source);

        let mut zeta = decoded.clone();
        let stats = pascal_gf2_in_place(&mut zeta).expect("PB6 K=2048 P-ZETA");
        assert_eq!(stats.logical_xor_ops, P_ZETA_XOR_OPS);
        assert_eq!(stats.packed_word_updates, P_ZETA_PACKED_UPDATES);
        assert_eq!(stats.state_bytes, 256);
        assert_eq!(stats.scratch_words, 0);

        let mut generic = vec![false; BITS];
        for (address, value) in generic.iter_mut().enumerate() {
            *value = direct_submask_parity(&decoded, address);
        }

        let mut answers_a = Vec::with_capacity(BITS);
        let mut answers_b = Vec::with_capacity(BITS);
        let mut answers_c = Vec::with_capacity(BITS);
        let mut oracle = Vec::with_capacity(BITS);

        for &address in &schedule {
            let gate_value = gate.values[address];
            answers_a.push(get_bit(&zeta, address) & gate_value);
            answers_b.push(direct_submask_parity(&decoded, address) & gate_value);
            answers_c.push(generic[address] & gate_value);
            oracle.push(direct_submask_parity(source, address) & direct_gate(address));
        }

        for (q_index, &q) in Q_VALUES.iter().enumerate() {
            let evidence = &mut prefixes[q_index];
            for position in 0..q {
                let expected = oracle[position];
                let a = answers_a[position];
                let b = answers_b[position];
                let c = answers_c[position];

                evidence.answer_count += 1;
                evidence.a_mismatches += u64::from(a != expected);
                evidence.b_mismatches += u64::from(b != expected);
                evidence.c_mismatches += u64::from(c != expected);
                evidence.a_checksum = checksum_push(evidence.a_checksum, a, sample, position);
                evidence.b_checksum = checksum_push(evidence.b_checksum, b, sample, position);
                evidence.c_checksum = checksum_push(evidence.c_checksum, c, sample, position);
                evidence.oracle_checksum =
                    checksum_push(evidence.oracle_checksum, expected, sample, position);
            }
        }
    }

    let expected_source_count = CORPUS;
    let u1 = source_collisions == 0
        && inverse_failures == 0
        && gate.coefficient_mismatches == 0
        && gate.evaluation_mismatches == 0
        && prefixes.iter().all(|evidence| {
            evidence.a_mismatches == 0
                && evidence.b_mismatches == 0
                && evidence.c_mismatches == 0
                && evidence.a_checksum == evidence.oracle_checksum
                && evidence.b_checksum == evidence.oracle_checksum
                && evidence.c_checksum == evidence.oracle_checksum
        })
        && sources.len() == expected_source_count;

    let u2 = Q_VALUES
        .iter()
        .copied()
        .filter(|q| *q >= 256)
        .all(|q| P_ZETA_XOR_OPS + (q as u64) < direct_reads_for_prefix(&schedule, q));

    let u3 = FULL_DIRECT_READS >= 15 * P_ZETA_XOR_OPS;
    let u4 = 256_usize == BITS / 8;

    if emit {
        println!(
            "PB6_DETERMINISTIC_BEGIN,protocol=1,bits={BITS},states={CORPUS},q_values=1|16|64|128|256|512|1024|2048"
        );
        println!(
            "PB6_COMMON,source_collisions={},inverse_failures={},gate_coefficient_mismatches={},gate_evaluation_mismatches={}",
            source_collisions,
            inverse_failures,
            gate.coefficient_mismatches,
            gate.evaluation_mismatches
        );

        for (q_index, &q) in Q_VALUES.iter().enumerate() {
            let evidence = prefixes[q_index];
            let direct_reads = direct_reads_for_prefix(&schedule, q);
            let direct_xors = direct_reads - q as u64;
            let p_zeta_work = P_ZETA_XOR_OPS + q as u64;
            let generic_query_reads = q as u64;
            println!(
                "PB6_Q,q={},answers={},a_mismatches={},b_mismatches={},c_mismatches={},a_checksum={:016x},b_checksum={:016x},c_checksum={:016x},oracle_checksum={:016x},direct_source_reads_per_state={},direct_xor_reductions_per_state={},p_zeta_xor_per_state={},p_zeta_query_reads_per_state={},p_zeta_conservative_work={},generic_full_source_reads_per_state={},generic_query_reads_per_state={}",
                q,
                evidence.answer_count,
                evidence.a_mismatches,
                evidence.b_mismatches,
                evidence.c_mismatches,
                evidence.a_checksum,
                evidence.b_checksum,
                evidence.c_checksum,
                evidence.oracle_checksum,
                direct_reads,
                direct_xors,
                P_ZETA_XOR_OPS,
                q,
                p_zeta_work,
                FULL_DIRECT_READS,
                generic_query_reads
            );
        }

        println!(
            "PB6_STORAGE,p_zeta_answer_bytes=256,generic_answer_bytes=256,implicit_route_learned_bits=0"
        );
        println!(
            "PB6_GATE,U1_EXACT_TASK_EQUIVALENCE,{}",
            if u1 { "HOLDS" } else { "INVALID" }
        );
        println!(
            "PB6_GATE,U2_STRUCTURAL_UTILITY,{}",
            if u2 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB6_GATE,U3_FAST_PRECOMPUTE,{}",
            if u3 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB6_GATE,U4_STORAGE_PARITY,{}",
            if u4 { "HOLDS" } else { "INVALID" }
        );
        println!("PB6_DETERMINISTIC_DECISION,PENDING_PHYSICAL_U5");
        println!("PROMOTION,BLOCKED,pb6_requires_physical_gate");
    }

    DeterministicEvidence {
        source_collisions,
        inverse_failures,
        gate_coefficient_mismatches: gate.coefficient_mismatches,
        gate_evaluation_mismatches: gate.evaluation_mismatches,
        prefixes,
        u1,
        u2,
        u3,
        u4,
    }
}

#[derive(Debug, Clone, Copy)]
enum Arm {
    PZeta,
    Direct,
    Generic,
}

impl Arm {
    const fn name(self) -> &'static str {
        match self {
            Self::PZeta => "P_ZETA_PRECOMPUTE",
            Self::Direct => "DIRECT_ON_DEMAND",
            Self::Generic => "GENERIC_FULL_PRECOMPUTE",
        }
    }
}

fn run_benchmark_arm(
    arm: Arm,
    sources: &[Vec<u64>],
    schedule: &[usize],
    gate_values: &[bool],
) -> u64 {
    let mut checksum = 0_u64;

    for (sample, source) in sources.iter().enumerate() {
        let source = black_box(source);
        let encoded = feistel_forward(source);
        let decoded = feistel_inverse(&encoded);

        match arm {
            Arm::PZeta => {
                let mut zeta = decoded;
                pascal_gf2_in_place(&mut zeta).expect("PB6 benchmark P-ZETA");
                for (position, &address) in schedule.iter().enumerate() {
                    let answer = get_bit(&zeta, address) & gate_values[address];
                    checksum = checksum_push(checksum, answer, sample, position);
                }
            }
            Arm::Direct => {
                for (position, &address) in schedule.iter().enumerate() {
                    let answer = direct_submask_parity(&decoded, address) & gate_values[address];
                    checksum = checksum_push(checksum, answer, sample, position);
                }
            }
            Arm::Generic => {
                let mut generic = vec![false; BITS];
                for (address, value) in generic.iter_mut().enumerate() {
                    *value = direct_submask_parity(&decoded, address);
                }
                for (position, &address) in schedule.iter().enumerate() {
                    let answer = generic[address] & gate_values[address];
                    checksum = checksum_push(checksum, answer, sample, position);
                }
            }
        }
    }

    black_box(checksum)
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

fn benchmark() {
    let deterministic = run_deterministic(false);
    let gate = build_gate_evidence();
    let schedule = address_schedule();
    let sources = deterministic_sources();

    let runner = env::var("RUNNER_NAME").unwrap_or_else(|_| "UNKNOWN".to_string());
    let arch = env::consts::ARCH;

    println!(
        "PB6_BENCH_BEGIN,protocol=1,runner={},arch={},states={CORPUS},q=2048,warmups={WARMUP_REPS},measured_reps={MEASURED_REPS}",
        runner, arch
    );

    for warmup in 0..WARMUP_REPS {
        let order = match warmup % 3 {
            0 => [Arm::PZeta, Arm::Direct, Arm::Generic],
            1 => [Arm::Direct, Arm::Generic, Arm::PZeta],
            _ => [Arm::Generic, Arm::PZeta, Arm::Direct],
        };
        for arm in order {
            let checksum = run_benchmark_arm(arm, &sources, &schedule, &gate.values);
            black_box(checksum);
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
            0 => [Arm::PZeta, Arm::Direct, Arm::Generic],
            1 => [Arm::Direct, Arm::Generic, Arm::PZeta],
            _ => [Arm::Generic, Arm::PZeta, Arm::Direct],
        };

        for arm in order {
            let start = Instant::now();
            let checksum = run_benchmark_arm(arm, &sources, &schedule, &gate.values);
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            black_box(checksum);

            match arm {
                Arm::PZeta => {
                    a_times.push(elapsed);
                    a_checksums.push(checksum);
                }
                Arm::Direct => {
                    b_times.push(elapsed);
                    b_checksums.push(checksum);
                }
                Arm::Generic => {
                    c_times.push(elapsed);
                    c_checksums.push(checksum);
                }
            }
        }
    }

    let a_median = median(&a_times);
    let b_median = median(&b_times);
    let c_median = median(&c_times);
    let a_p95 = percentile(&a_times, 95, 100);
    let b_p95 = percentile(&b_times, 95, 100);
    let c_p95 = percentile(&c_times, 95, 100);

    let checksum_a = a_checksums.first().copied().unwrap_or_default();
    let checksum_b = b_checksums.first().copied().unwrap_or_default();
    let checksum_c = c_checksums.first().copied().unwrap_or_default();

    let stable_checksums = a_checksums.iter().all(|&value| value == checksum_a)
        && b_checksums.iter().all(|&value| value == checksum_b)
        && c_checksums.iter().all(|&value| value == checksum_c);

    let checksums_equal = stable_checksums && checksum_a == checksum_b && checksum_a == checksum_c;

    let paired_wins = (0..MEASURED_REPS)
        .filter(|&index| a_times[index] < b_times[index].min(c_times[index]))
        .count();

    let faster_control_median = b_median.min(c_median);
    let ten_percent_gate = 10_u128 * a_median <= 9_u128 * faster_control_median;
    let paired_gate = paired_wins >= 16;
    let u5 = checksums_equal && ten_percent_gate && paired_gate;

    let repetitions_valid = a_times.len() == MEASURED_REPS
        && b_times.len() == MEASURED_REPS
        && c_times.len() == MEASURED_REPS;
    let runner_valid = runner == EXPECTED_RUNNER && arch == "aarch64";
    let deterministic_valid =
        deterministic.u1 && deterministic.u2 && deterministic.u3 && deterministic.u4;
    let u6 = repetitions_valid
        && runner_valid
        && checksums_equal
        && deterministic_valid
        && deterministic.source_collisions == 0
        && deterministic.inverse_failures == 0
        && deterministic.gate_coefficient_mismatches == 0
        && deterministic.gate_evaluation_mismatches == 0
        && deterministic
            .prefixes
            .last()
            .is_some_and(|evidence| evidence.answer_count == (CORPUS * BITS) as u64);

    let states_per_second =
        |elapsed_ns: u128| -> u128 { (CORPUS as u128 * 1_000_000_000_u128) / elapsed_ns.max(1) };

    println!(
        "PB6_BENCH_ARM,arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
        Arm::PZeta.name(),
        format_times(&a_times),
        a_median,
        a_p95,
        states_per_second(a_median),
        checksum_a
    );
    println!(
        "PB6_BENCH_ARM,arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
        Arm::Direct.name(),
        format_times(&b_times),
        b_median,
        b_p95,
        states_per_second(b_median),
        checksum_b
    );
    println!(
        "PB6_BENCH_ARM,arm={},times_ns={},median_ns={},p95_ns={},states_per_second={},checksum={:016x}",
        Arm::Generic.name(),
        format_times(&c_times),
        c_median,
        c_p95,
        states_per_second(c_median),
        checksum_c
    );
    println!(
        "PB6_BENCH_COMPARE,faster_control_median_ns={},paired_wins_a_of_20={},ten_percent_gate={},paired_gate={}",
        faster_control_median,
        paired_wins,
        u8::from(ten_percent_gate),
        u8::from(paired_gate)
    );
    println!(
        "PB6_GATE,U5_MEASURED_UTILITY,{}",
        if u5 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB6_GATE,U6_BENCHMARK_SANITY,{}",
        if u6 { "HOLDS" } else { "INVALID" }
    );

    let decision = if !u6 {
        "INVALID"
    } else if deterministic_valid && u5 {
        "RETAIN_TASK_UTILITY"
    } else {
        "CLOSE_TASK_UTILITY"
    };
    println!("PB6_DECISION,{decision}");
    println!("PROMOTION,BLOCKED,pb6_subset_task_utility_only");
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
        other => panic!("unknown PB6 mode: {other}"),
    }
}
