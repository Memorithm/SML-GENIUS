//! Sequential Pascal-Boolean functional suite at K=2048.
//!
//! Frozen protocol: docs/SML-PASCAL-BOOLEAN-SUITE-PREREGISTRATION.md.
//! This is functional primitive evidence only; no model-quality promotion follows.

use std::collections::BTreeSet;

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
const PPM: u64 = 1_000_000;
const BASE: u64 = 0x5042_5355_4954_4531;

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

fn toggle_bit(words: &mut [u64], index: usize) {
    words[index / 64] ^= 1_u64 << (index % 64);
}

fn xor_distance(left: &[u64], right: &[u64]) -> u64 {
    assert_eq!(left.len(), right.len());
    left.iter()
        .zip(right)
        .map(|(a, b)| (a ^ b).count_ones() as u64)
        .sum()
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

fn distribution(mut values: Vec<u64>) -> (u64, u64, u64, u64, u64) {
    assert!(!values.is_empty());
    values.sort_unstable();
    let count = values.len();
    let median = if count % 2 == 0 {
        (values[count / 2 - 1] + values[count / 2]) / 2
    } else {
        values[count / 2]
    };
    let p95_rank = (95 * count).div_ceil(100);
    (
        values[0],
        median,
        values.iter().sum(),
        values[p95_rank - 1],
        values[count - 1],
    )
}

fn direct_submask_parity(words: &[u64], n: usize) -> bool {
    let mut parity = false;
    let mut k = n;
    loop {
        parity ^= get_bit(words, k);
        if k == 0 {
            break;
        }
        k = (k - 1) & n;
    }
    parity
}

fn pb1_zeta() -> bool {
    println!("PB1_BEGIN,name=P_ZETA,bits={BITS}");

    let original = words_from_seed(WORDS, BASE ^ 0x01);
    let mut transformed = original.clone();
    let stats = pascal_gf2_in_place(&mut transformed).expect("PB1 K=2048 transform");

    let oracle_failures = (0..BITS)
        .filter(|&n| get_bit(&transformed, n) != direct_submask_parity(&original, n))
        .count();

    let mut unique_sources = BTreeSet::new();
    let mut unique_outputs = BTreeSet::new();
    let mut inverse_failures = 0_u64;
    for sample in 0..CORPUS {
        let source = words_from_seed(WORDS, BASE ^ 0x1000 ^ sample as u64);
        unique_sources.insert(source.clone());
        let mut encoded = source.clone();
        pascal_gf2_in_place(&mut encoded).expect("PB1 corpus transform");
        unique_outputs.insert(encoded.clone());
        pascal_gf2_in_place(&mut encoded).expect("PB1 corpus inverse");
        inverse_failures += u64::from(encoded != source);
    }
    let source_collisions = CORPUS - unique_sources.len();
    let collisions = CORPUS - unique_outputs.len();

    let mut avalanche = Vec::with_capacity(BITS);
    for coordinate in 0..BITS {
        let mut basis = vec![0_u64; WORDS];
        set_bit(&mut basis, coordinate, true);
        pascal_gf2_in_place(&mut basis).expect("PB1 basis transform");
        avalanche.push(basis.iter().map(|word| word.count_ones() as u64).sum());
    }
    let (min, median, sum, p95, max) = distribution(avalanche);

    let direct_full_sweep_reads: u64 = (0..BITS).map(|n| 1_u64 << n.count_ones()).sum();
    let mut uniform_break_even_queries = 0_usize;
    for queries in 1..=BITS {
        let zeta_scaled = (stats.logical_xor_ops + queries as u64) * BITS as u64;
        let direct_scaled = queries as u64 * direct_full_sweep_reads;
        if zeta_scaled < direct_scaled {
            uniform_break_even_queries = queries;
            break;
        }
    }

    println!(
        "PB1_EXACTNESS,oracle_failures={},inverse_failures={},corpus={},source_collisions={},output_collisions={}",
        oracle_failures, inverse_failures, CORPUS, source_collisions, collisions
    );
    println!(
        "PB1_COST,stages={},logical_xor_ops={},packed_word_updates={},state_bytes={},direct_full_sweep_reads={},zeta_full_sweep_reads={},uniform_break_even_queries={}",
        stats.stages,
        stats.logical_xor_ops,
        stats.packed_word_updates,
        stats.state_bytes,
        direct_full_sweep_reads,
        BITS,
        uniform_break_even_queries
    );
    println!(
        "PB1_AVALANCHE,observations={},min={},median={},sum={},p95={},max={}",
        BITS, min, median, sum, p95, max
    );

    let valid =
        oracle_failures == 0 && inverse_failures == 0 && source_collisions == 0 && collisions == 0;
    println!(
        "PB1_HYPOTHESIS,EXACT_SUBSET_PARITY,{}",
        if valid { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB1_DECISION,{}",
        if valid { "RETAIN_FUNCTION" } else { "INVALID" }
    );
    valid
}

fn subset_indices(address: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut k = address;
    loop {
        result.push(k);
        if k == 0 {
            break;
        }
        k = (k - 1) & address;
    }
    result
}

fn pb2_router() -> bool {
    println!("PB2_BEGIN,name=P_ROUTER,bits={BITS}");

    let mut membership_failures = 0_u64;
    let mut cardinality_failures = 0_u64;
    let mut duplicate_failures = 0_u64;

    for address in 0..BITS {
        let selected = subset_indices(address);
        let expected_count = 1_usize << address.count_ones();
        cardinality_failures += u64::from(selected.len() != expected_count);

        let mut membership = vec![false; BITS];
        let mut duplicate = false;
        for index in selected {
            if membership[index] {
                duplicate = true;
            }
            membership[index] = true;
        }
        duplicate_failures += u64::from(duplicate);

        for (index, actual) in membership.iter().copied().enumerate() {
            let expected = (index & !address) == 0;
            membership_failures += u64::from(actual != expected);
        }
    }

    for popcount in [3_u32, 5, 7, 9] {
        let addresses = (0..BITS)
            .filter(|address| address.count_ones() == popcount)
            .collect::<Vec<_>>();
        let active = 1_u64 << popcount;
        let active_ppm = active * PPM / BITS as u64;
        let mut overlap_sum_ppm = 0_u64;
        let mut pairs = 0_u64;
        for i in 0..addresses.len() {
            for j in (i + 1)..addresses.len() {
                let intersection = 1_u64 << (addresses[i] & addresses[j]).count_ones();
                let union = 2 * active - intersection;
                overlap_sum_ppm += intersection * PPM / union;
                pairs += 1;
            }
        }
        let mean_overlap_ppm = if pairs == 0 {
            0
        } else {
            overlap_sum_ppm / pairs
        };
        println!(
            "PB2_BAND,popcount={},addresses={},active_routes={},active_ppm={},mean_pair_jaccard_ppm={},pairs={}",
            popcount,
            addresses.len(),
            active,
            active_ppm,
            mean_overlap_ppm,
            pairs
        );
    }

    let explicit_membership_bits = (BITS as u64) * (BITS as u64);
    let single_target_direct_route_bits = (BITS as u64) * VARS as u64;
    println!(
        "PB2_STORAGE,implicit_learned_bits=0,explicit_membership_bits={},single_target_direct_route_bits={},single_target_structurally_equivalent=0",
        explicit_membership_bits, single_target_direct_route_bits
    );
    println!(
        "PB2_EXACTNESS,membership_failures={},cardinality_failures={},duplicate_failures={}",
        membership_failures, cardinality_failures, duplicate_failures
    );

    let valid = membership_failures == 0 && cardinality_failures == 0 && duplicate_failures == 0;
    println!(
        "PB2_HYPOTHESIS,EXACT_SUBMASK_ROUTING,{}",
        if valid { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB2_DECISION,{}",
        if valid { "RETAIN_FUNCTION" } else { "INVALID" }
    );
    valid
}

struct AnfFixture {
    name: &'static str,
    truth: Vec<u64>,
    expected_coefficients: Option<Vec<usize>>,
}

fn truth_table<F>(predicate: F) -> Vec<u64>
where
    F: Fn(usize) -> bool,
{
    let mut words = vec![0_u64; WORDS];
    for assignment in 0..BITS {
        if predicate(assignment) {
            set_bit(&mut words, assignment, true);
        }
    }
    words
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
        monomials.push(AnfMonomial::new(variables).expect("canonical ANF monomial"));
    }
    monomials.sort();
    AnfPolynomial::new(VARS, constant, monomials).expect("canonical ANF polynomial")
}

fn pb3_anf() -> bool {
    println!("PB3_BEGIN,name=P_ANF,variables={VARS},truth_rows={BITS}");

    let mut fixtures = vec![
        AnfFixture {
            name: "CONSTANT_1",
            truth: truth_table(|_| true),
            expected_coefficients: Some(vec![0]),
        },
        AnfFixture {
            name: "SINGLE_X0",
            truth: truth_table(|assignment| assignment & 1 != 0),
            expected_coefficients: Some(vec![1]),
        },
        AnfFixture {
            name: "XOR_X0_X3_X7",
            truth: truth_table(|assignment| {
                ((assignment & 1 != 0) as u8
                    ^ (assignment & (1 << 3) != 0) as u8
                    ^ (assignment & (1 << 7) != 0) as u8)
                    != 0
            }),
            expected_coefficients: Some(vec![1, 1 << 3, 1 << 7]),
        },
        AnfFixture {
            name: "AND_X1_X4",
            truth: truth_table(|assignment| {
                assignment & (1 << 1) != 0 && assignment & (1 << 4) != 0
            }),
            expected_coefficients: Some(vec![(1 << 1) | (1 << 4)]),
        },
        AnfFixture {
            name: "MAJORITY_X0_X1_X2",
            truth: truth_table(|assignment| (assignment & 0b111).count_ones() >= 2),
            expected_coefficients: Some(vec![0b011, 0b101, 0b110]),
        },
    ];

    for random_id in 0..4_u64 {
        fixtures.push(AnfFixture {
            name: match random_id {
                0 => "RANDOM_0",
                1 => "RANDOM_1",
                2 => "RANDOM_2",
                _ => "RANDOM_3",
            },
            truth: words_from_seed(WORDS, BASE ^ 0x3000 ^ random_id),
            expected_coefficients: None,
        });
    }

    let mut reconstruction_failures = 0_u64;
    let mut expected_coefficient_failures = 0_u64;
    let mut direct_eval_failures = 0_u64;

    for fixture in &fixtures {
        let mut coefficients = fixture.truth.clone();
        pascal_gf2_in_place(&mut coefficients).expect("PB3 truth-to-ANF");

        let actual_indices = set_indices(&coefficients);
        if let Some(expected) = &fixture.expected_coefficients {
            expected_coefficient_failures += u64::from(&actual_indices != expected);
        }

        let mut reconstructed = coefficients.clone();
        pascal_gf2_in_place(&mut reconstructed).expect("PB3 ANF-to-truth");
        let reconstruction_error = xor_distance(&fixture.truth, &reconstructed);
        reconstruction_failures += u64::from(reconstruction_error != 0);

        let mut histogram = [0_u64; VARS + 1];
        for &mask in &actual_indices {
            histogram[mask.count_ones() as usize] += 1;
        }
        let histogram_text = histogram
            .iter()
            .enumerate()
            .map(|(degree, count)| format!("{degree}:{count}"))
            .collect::<Vec<_>>()
            .join("|");

        let polynomial = polynomial_from_coefficients(&coefficients);
        let mut counters = ExecutionCounters::default();
        let mut eval_errors = 0_u64;
        for assignment in 0..BITS {
            let inputs = (0..VARS)
                .map(|variable| assignment & (1 << variable) != 0)
                .collect::<Vec<_>>();
            let observed = polynomial
                .evaluate(&inputs, &mut counters)
                .expect("PB3 direct ANF evaluation");
            eval_errors += u64::from(observed != get_bit(&fixture.truth, assignment));
        }
        direct_eval_failures += eval_errors;

        println!(
            "PB3_FIXTURE,name={},coefficients={},reconstruction_error_bits={},direct_eval_errors={},anf_monomials={},boolean_ops={},gf2_ops={},degree_histogram={}",
            fixture.name,
            actual_indices.len(),
            reconstruction_error,
            eval_errors,
            counters.anf_monomials,
            counters.boolean_ops,
            counters.gf2_ops,
            histogram_text
        );

        for degree in [2_usize, 3, 4] {
            let mut truncated = coefficients.clone();
            let mut retained = 0_u64;
            for mask in 0..BITS {
                if !get_bit(&coefficients, mask) {
                    continue;
                }
                if mask.count_ones() as usize <= degree {
                    retained += 1;
                } else {
                    set_bit(&mut truncated, mask, false);
                }
            }
            let mut approximate_truth = truncated;
            pascal_gf2_in_place(&mut approximate_truth).expect("PB3 truncated inverse");
            let error_bits = xor_distance(&fixture.truth, &approximate_truth);
            println!(
                "PB3_TRUNCATION,name={},degree_le={},retained_coefficients={},error_bits={},error_ppm={}",
                fixture.name,
                degree,
                retained,
                error_bits,
                error_bits * PPM / BITS as u64
            );
        }
    }

    println!(
        "PB3_EXACTNESS,reconstruction_failures={},expected_coefficient_failures={},direct_eval_failures={}",
        reconstruction_failures, expected_coefficient_failures, direct_eval_failures
    );
    let valid = reconstruction_failures == 0
        && expected_coefficient_failures == 0
        && direct_eval_failures == 0;
    println!(
        "PB3_HYPOTHESIS,EXACT_ANF_TRANSFORM,{}",
        if valid { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB3_DECISION,{}",
        if valid { "RETAIN_FUNCTION" } else { "INVALID" }
    );
    valid
}

fn feistel_round(right: &[u64], round: usize) -> Vec<u64> {
    assert_eq!(right.len(), HALF_WORDS);
    let mut pascal = right.to_vec();
    pascal_gf2_in_place(&mut pascal).expect("PB4 half-width Pascal transform");
    let shift_a = (17 + 29 * round) % HALF_BITS;
    let shift_b = (43 + 53 * round) % HALF_BITS;
    let rotated_a = rotate_bits(&pascal, HALF_BITS, shift_a);
    let rotated_b = rotate_bits(&pascal, HALF_BITS, shift_b);
    for index in 0..HALF_WORDS {
        pascal[index] ^= rotated_a[index] & rotated_b[index];
    }
    pascal
}

fn feistel_forward(input: &[u64], rounds: usize) -> Vec<u64> {
    assert_eq!(input.len(), WORDS);
    let mut left = input[..HALF_WORDS].to_vec();
    let mut right = input[HALF_WORDS..].to_vec();

    for round in 0..rounds {
        let f = feistel_round(&right, round);
        let new_right = xor_words(&left, &f);
        left = right;
        right = new_right;
    }

    left.extend_from_slice(&right);
    left
}

fn feistel_inverse(input: &[u64], rounds: usize) -> Vec<u64> {
    assert_eq!(input.len(), WORDS);
    let mut left = input[..HALF_WORDS].to_vec();
    let mut right = input[HALF_WORDS..].to_vec();

    for round in (0..rounds).rev() {
        let old_right = left.clone();
        let f = feistel_round(&old_right, round);
        let old_left = xor_words(&right, &f);
        left = old_left;
        right = old_right;
    }

    left.extend_from_slice(&right);
    left
}

fn identity(input: &[u64]) -> Vec<u64> {
    input.to_vec()
}

fn permute(input: &[u64]) -> Vec<u64> {
    rotate_bits(input, BITS, 683)
}

fn xor_permute(input: &[u64]) -> Vec<u64> {
    let role = words_from_seed(WORDS, BASE ^ 0x4444);
    let mixed = xor_words(input, &role);
    rotate_bits(&mixed, BITS, 683)
}

fn linear_pascal(input: &[u64]) -> Vec<u64> {
    let mut output = input.to_vec();
    pascal_gf2_in_place(&mut output).expect("PB4 linear Pascal control");
    output
}

fn avalanche_report<F>(name: &str, base: &[u64], transform: F)
where
    F: Fn(&[u64]) -> Vec<u64>,
{
    let baseline = transform(base);
    let mut distances = Vec::with_capacity(BITS);
    for coordinate in 0..BITS {
        let mut perturbed = base.to_vec();
        toggle_bit(&mut perturbed, coordinate);
        let output = transform(&perturbed);
        distances.push(xor_distance(&baseline, &output));
    }
    let (min, median, sum, p95, max) = distribution(distances);
    println!(
        "PB4_AVALANCHE,arm={},observations={},min={},median={},sum={},p95={},max={}",
        name, BITS, min, median, sum, p95, max
    );
}

fn nonlinearity_witness(rounds: usize) -> Option<(usize, usize)> {
    for a in 0..8 {
        for b in (a + 1)..8 {
            let mut left = vec![0_u64; WORDS];
            let mut right = vec![0_u64; WORDS];
            set_bit(&mut left, a, true);
            set_bit(&mut right, b, true);
            let both = xor_words(&left, &right);
            let fa = feistel_forward(&left, rounds);
            let fb = feistel_forward(&right, rounds);
            let fab = feistel_forward(&both, rounds);
            if fab != xor_words(&fa, &fb) {
                return Some((a, b));
            }
        }
    }
    None
}

fn pb4_feistel() -> bool {
    println!("PB4_BEGIN,name=P_FEISTEL,bits={BITS},half_bits={HALF_BITS}");

    let base = words_from_seed(WORDS, BASE ^ 0x4000);
    avalanche_report("IDENTITY", &base, identity);
    avalanche_report("PERMUTE", &base, permute);
    avalanche_report("XOR_PERMUTE", &base, xor_permute);
    avalanche_report("PASCAL_LINEAR", &base, linear_pascal);

    println!(
        "PB4_CONTROL_COST,arm=IDENTITY,logical_xor_ops=0,logical_and_ops=0,packed_word_updates=0,coordinate_maps=0"
    );
    println!(
        "PB4_CONTROL_COST,arm=PERMUTE,logical_xor_ops=0,logical_and_ops=0,packed_word_updates=0,coordinate_maps={BITS}"
    );
    println!(
        "PB4_CONTROL_COST,arm=XOR_PERMUTE,logical_xor_ops={BITS},logical_and_ops=0,packed_word_updates={WORDS},coordinate_maps={BITS}"
    );
    let mut pascal_cost_words = vec![0_u64; WORDS];
    let pascal_cost = pascal_gf2_in_place(&mut pascal_cost_words).expect("PB4 Pascal control cost");
    println!(
        "PB4_CONTROL_COST,arm=PASCAL_LINEAR,logical_xor_ops={},logical_and_ops=0,packed_word_updates={},coordinate_maps=0",
        pascal_cost.logical_xor_ops, pascal_cost.packed_word_updates
    );

    let mut all_valid = true;
    for rounds in [2_usize, 4, 6] {
        let mut inverse_failures = 0_u64;
        let mut determinism_failures = 0_u64;
        let mut unique = BTreeSet::new();

        for sample in 0..CORPUS {
            let source = words_from_seed(WORDS, BASE ^ 0x5000 ^ sample as u64);
            let encoded = feistel_forward(&source, rounds);
            let replay = feistel_forward(&source, rounds);
            determinism_failures += u64::from(encoded != replay);
            unique.insert(encoded.clone());
            let decoded = feistel_inverse(&encoded, rounds);
            inverse_failures += u64::from(decoded != source);
        }

        let collisions = CORPUS - unique.len();
        let witness = nonlinearity_witness(rounds);
        let nonlinear = witness.is_some();
        if let Some((a, b)) = witness {
            println!(
                "PB4_NONLINEARITY,rounds={},witness_a={},witness_b={},holds=1",
                rounds, a, b
            );
        } else {
            println!(
                "PB4_NONLINEARITY,rounds={},witness_a=NONE,witness_b=NONE,holds=0",
                rounds
            );
        }

        avalanche_report(&format!("P_FEISTEL_R{rounds}"), &base, |input| {
            feistel_forward(input, rounds)
        });

        let per_round_xor = 7_168_u64;
        let per_round_and = 1_024_u64;
        let per_round_word_updates = 160_u64;
        let per_round_coordinate_maps = 2_048_u64;
        println!(
            "PB4_COST,rounds={},logical_xor_ops={},logical_and_ops={},packed_word_updates={},coordinate_maps={}",
            rounds,
            per_round_xor * rounds as u64,
            per_round_and * rounds as u64,
            per_round_word_updates * rounds as u64,
            per_round_coordinate_maps * rounds as u64
        );
        println!(
            "PB4_EXACTNESS,rounds={},corpus={},inverse_failures={},determinism_failures={},collisions={}",
            rounds, CORPUS, inverse_failures, determinism_failures, collisions
        );

        let valid =
            inverse_failures == 0 && determinism_failures == 0 && collisions == 0 && nonlinear;
        all_valid &= valid;
        println!(
            "PB4_GATE,rounds={},{}",
            rounds,
            if valid { "HOLDS" } else { "INVALID" }
        );
    }

    println!(
        "PB4_HYPOTHESIS,REVERSIBLE_NONLINEAR_PASCAL_COMPOSITION,{}",
        if all_valid { "HOLDS" } else { "INVALID" }
    );
    println!(
        "PB4_DECISION,{}",
        if all_valid {
            "RETAIN_FUNCTION"
        } else {
            "INVALID"
        }
    );
    all_valid
}

fn main() {
    println!("PASCAL_BOOLEAN_SUITE,protocol=1,bits={BITS},execution=SEQUENTIAL");

    let pb1_valid = pb1_zeta();
    if !pb1_valid {
        println!("PASCAL_BOOLEAN_SUITE_STOP,after=PB1,reason=SHARED_PREMISE_INVALID");
        println!("PROMOTION,BLOCKED,pascal_boolean_functional_evidence_only");
        return;
    }

    let pb2_valid = pb2_router();
    let pb3_valid = pb3_anf();
    let pb4_valid = pb4_feistel();

    let all_valid = pb1_valid && pb2_valid && pb3_valid && pb4_valid;
    println!(
        "PB5_GATE,{}",
        if all_valid {
            "READY_FOR_COMPOSED_PREREGISTRATION"
        } else {
            "BLOCKED_INVALID_EVIDENCE"
        }
    );
    println!(
        "PASCAL_BOOLEAN_SUITE_COMPLETE,PB1={},PB2={},PB3={},PB4={}",
        if pb1_valid { "VALID" } else { "INVALID" },
        if pb2_valid { "VALID" } else { "INVALID" },
        if pb3_valid { "VALID" } else { "INVALID" },
        if pb4_valid { "VALID" } else { "INVALID" }
    );
    println!("PROMOTION,BLOCKED,pascal_boolean_functional_evidence_only");
}
