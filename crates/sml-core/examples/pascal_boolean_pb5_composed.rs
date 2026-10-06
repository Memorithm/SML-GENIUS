//! PB-5 composed Pascal-Boolean functional pipeline at K=2048.
//!
//! Frozen protocol: docs/SML-PASCAL-BOOLEAN-PB5-PREREGISTRATION.md.
//! This is functional compatibility evidence only.

use std::collections::BTreeSet;

use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    pascal_gf2::{pascal_gf2_in_place, PascalGf2Stats},
    ExecutionCounters,
};

const BITS: usize = 2048;
const WORDS: usize = BITS / 64;
const HALF_BITS: usize = BITS / 2;
const HALF_WORDS: usize = HALF_BITS / 64;
const VARS: usize = 11;
const CORPUS: usize = 64;
const FEISTEL_ROUNDS: usize = 4;
const EXPECTED_COMPARISONS: u64 = (CORPUS * BITS) as u64;
const BASE: u64 = 0x5042_3543_4f4d_5031;

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
    pascal_gf2_in_place(&mut pascal).expect("PB5 half-width Pascal transform");
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
        monomials.push(AnfMonomial::new(variables).expect("PB5 canonical monomial"));
    }
    monomials.sort();
    AnfPolynomial::new(VARS, constant, monomials).expect("PB5 canonical polynomial")
}

fn coefficient_difference(actual: &[usize], expected: &[usize]) -> usize {
    let actual = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    actual.symmetric_difference(&expected).count()
}

fn main() {
    println!(
        "PB5_BEGIN,protocol=1,bits={BITS},states={CORPUS},addresses={BITS},comparisons={EXPECTED_COMPARISONS},feistel_rounds={FEISTEL_ROUNDS}"
    );

    let expected_coefficients = vec![
        1_usize,
        (1_usize << 1) | (1_usize << 3),
        (1_usize << 2) | (1_usize << 5) | (1_usize << 8),
        (1_usize << 7) | (1_usize << 9),
    ];

    let truth = gate_truth_table();
    let mut coefficients = truth.clone();
    let coefficient_stats =
        pascal_gf2_in_place(&mut coefficients).expect("PB5 truth-table to ANF transform");
    let actual_coefficients = set_indices(&coefficients);
    let coefficient_mismatches =
        coefficient_difference(&actual_coefficients, &expected_coefficients);

    let polynomial = polynomial_from_coefficients(&coefficients);
    let mut anf_counters = ExecutionCounters::default();
    let mut anf_gate_values = vec![false; BITS];
    let mut gate_mismatches = 0_u64;
    let mut gate_true = 0_u64;
    for (address, gate_value) in anf_gate_values.iter_mut().enumerate() {
        let inputs = (0..VARS)
            .map(|variable| address & (1_usize << variable) != 0)
            .collect::<Vec<_>>();
        let observed = polynomial
            .evaluate(&inputs, &mut anf_counters)
            .expect("PB5 ANF gate evaluation");
        *gate_value = observed;
        gate_true += u64::from(observed);
        gate_mismatches += u64::from(observed != direct_gate(address));
    }

    println!(
        "PB5_ANF,expected_coefficients=1|10|292|640,actual_coefficients={},coefficient_mismatches={},gate_mismatches={},gate_true={},anf_monomials={},boolean_ops={},gf2_ops={}",
        actual_coefficients
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("|"),
        coefficient_mismatches,
        gate_mismatches,
        gate_true,
        anf_counters.anf_monomials,
        anf_counters.boolean_ops,
        anf_counters.gf2_ops
    );

    let mut unique_sources = BTreeSet::new();
    let mut unique_encoded = BTreeSet::new();
    let mut inverse_failures = 0_u64;
    let mut feistel_determinism_failures = 0_u64;
    let mut parity_mismatches = 0_u64;
    let mut final_mismatches = 0_u64;
    let mut comparisons = 0_u64;
    let mut zeta_stats: Option<PascalGf2Stats> = None;
    let mut zeta_stats_mismatches = 0_u64;

    for sample in 0..CORPUS {
        let source = words_from_seed(WORDS, BASE ^ sample as u64);
        unique_sources.insert(source.clone());

        let encoded = feistel_forward(&source);
        let replay = feistel_forward(&source);
        feistel_determinism_failures += u64::from(encoded != replay);
        unique_encoded.insert(encoded.clone());

        let decoded = feistel_inverse(&encoded);
        inverse_failures += u64::from(decoded != source);

        let mut zeta = decoded.clone();
        let stats = pascal_gf2_in_place(&mut zeta).expect("PB5 K=2048 P-ZETA");
        if let Some(reference) = zeta_stats {
            zeta_stats_mismatches += u64::from(stats != reference);
        } else {
            zeta_stats = Some(stats);
        }

        for (address, &gate_value) in anf_gate_values.iter().enumerate() {
            let native_parity = get_bit(&zeta, address);
            let routed_parity = direct_submask_parity(&decoded, address);
            parity_mismatches += u64::from(native_parity != routed_parity);

            let native_answer = native_parity & gate_value;
            let direct_answer = direct_submask_parity(&source, address) & direct_gate(address);
            final_mismatches += u64::from(native_answer != direct_answer);
            comparisons += 1;
        }
    }

    let source_collisions = CORPUS - unique_sources.len();
    let encoded_collisions = CORPUS - unique_encoded.len();

    println!(
        "PB5_C1,source_collisions={},encoded_collisions={},inverse_failures={},feistel_determinism_failures={}",
        source_collisions,
        encoded_collisions,
        inverse_failures,
        feistel_determinism_failures
    );
    println!(
        "PB5_C2,parity_mismatches={},comparisons={}",
        parity_mismatches, comparisons
    );
    println!(
        "PB5_C3,coefficient_mismatches={},gate_mismatches={},addresses={BITS}",
        coefficient_mismatches, gate_mismatches
    );
    println!(
        "PB5_C4,final_mismatches={},comparisons={},expected_comparisons={EXPECTED_COMPARISONS}",
        final_mismatches, comparisons
    );

    let stats = zeta_stats.expect("PB5 at least one source state");
    let mut half_cost_words = vec![0_u64; HALF_WORDS];
    let half_stats =
        pascal_gf2_in_place(&mut half_cost_words).expect("PB5 half-width cost accounting");

    let feistel_round_xor_ops = half_stats.logical_xor_ops + 2 * HALF_BITS as u64;
    let feistel_round_and_ops = HALF_BITS as u64;
    let feistel_round_packed_updates = half_stats.packed_word_updates + 2 * HALF_WORDS as u64;
    let feistel_round_coordinate_maps = 2 * HALF_BITS as u64;

    let feistel_passes_per_state = 2 * FEISTEL_ROUNDS as u64;
    let feistel_xor_per_state = feistel_round_xor_ops * feistel_passes_per_state;
    let feistel_and_per_state = feistel_round_and_ops * feistel_passes_per_state;
    let feistel_packed_per_state = feistel_round_packed_updates * feistel_passes_per_state;
    let feistel_maps_per_state = feistel_round_coordinate_maps * feistel_passes_per_state;

    let direct_subset_reads_per_state: u64 =
        (0..BITS).map(|address| 1_u64 << address.count_ones()).sum();
    let direct_subset_reads_total = direct_subset_reads_per_state * CORPUS as u64;
    let native_query_reads_total = EXPECTED_COMPARISONS;
    let final_and_ops = EXPECTED_COMPARISONS;

    println!(
        "PB5_ACCOUNTING,zeta_xor_per_state={},zeta_packed_updates_per_state={},zeta_state_bytes={},zeta_stats_mismatches={},feistel_xor_per_state={},feistel_and_per_state={},feistel_packed_updates_per_state={},feistel_coordinate_maps_per_state={},direct_subset_reads_per_state={},direct_subset_reads_total={},native_query_reads_total={},final_and_ops={}",
        stats.logical_xor_ops,
        stats.packed_word_updates,
        stats.state_bytes,
        zeta_stats_mismatches,
        feistel_xor_per_state,
        feistel_and_per_state,
        feistel_packed_per_state,
        feistel_maps_per_state,
        direct_subset_reads_per_state,
        direct_subset_reads_total,
        native_query_reads_total,
        final_and_ops
    );
    println!(
        "PB5_REPRESENTATION,implicit_route_learned_bits=0,explicit_membership_bits={},gate_truth_table_bits={BITS},gate_active_anf_coefficients={}",
        BITS as u64 * BITS as u64,
        actual_coefficients.len()
    );
    println!(
        "PB5_ANF_TRANSFORM_COST,logical_xor_ops={},packed_word_updates={},state_bytes={}",
        coefficient_stats.logical_xor_ops,
        coefficient_stats.packed_word_updates,
        coefficient_stats.state_bytes
    );

    let c1 = source_collisions == 0
        && encoded_collisions == 0
        && inverse_failures == 0
        && feistel_determinism_failures == 0;
    let c2 = parity_mismatches == 0 && comparisons == EXPECTED_COMPARISONS;
    let c3 = coefficient_mismatches == 0 && gate_mismatches == 0;
    let c4 = final_mismatches == 0 && comparisons == EXPECTED_COMPARISONS;
    let c6 = stats.bits == BITS
        && stats.words == WORDS
        && stats.stages == VARS as u32
        && stats.logical_xor_ops == 11_264
        && stats.packed_word_updates == 272
        && stats.state_bytes == 256
        && stats.scratch_words == 0
        && zeta_stats_mismatches == 0
        && half_stats.bits == HALF_BITS
        && half_stats.stages == 10;

    println!(
        "PB5_GATE,C1_REVERSIBLE_STORAGE,{}",
        if c1 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB5_GATE,C2_ROUTER_ZETA,{}",
        if c2 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB5_GATE,C3_ANF_GATE,{}",
        if c3 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB5_GATE,C4_END_TO_END,{}",
        if c4 { "HOLDS" } else { "REFUTED" }
    );
    println!("PB5_GATE,C5_DETERMINISTIC_REPLAY,CI_ENFORCED");
    println!(
        "PB5_GATE,C6_ACCOUNTING,{}",
        if c6 { "HOLDS" } else { "INVALID" }
    );

    let retained = c1 && c2 && c3 && c4 && c6;
    println!(
        "PB5_DECISION,{}",
        if retained {
            "RETAIN_COMPOSED_FUNCTION"
        } else {
            "CLOSE_COMPOSED_FUNCTION"
        }
    );
    println!("PROMOTION,BLOCKED,pb5_functional_composition_only");
}
