//! DKV-2a-N3A current-stack replay: ambiguity produced by derive_ambiguity.
//!
//! Protocol: docs/SML-DKV-2A-N3A-HYPOTHESIS-AMBIGUITY-REPLAY.md.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::{derive_ambiguity, AmbiguityPolicy, SemanticBitField, SemanticState},
    PageId,
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const WIDTHS: [usize; 2] = [256, 1024];
const LOAD: usize = 16;
const CONFIGS: [(usize, usize); 3] = [(3, 1), (5, 1), (5, 2)];
const NOISE_BP: [u64; 7] = [100, 200, 500, 1000, 2000, 3000, 4000];
const CLEAR_ABOVE: u32 = 128;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const ARM_NAMES: [&str; 2] = ["Bipolar", "ActiveOnly"];

#[derive(Clone, Copy)]
struct Stream(u64);

impl Stream {
    fn new(base: u64) -> Self {
        let mut stream = Self(base | 1);
        for _ in 0..8 {
            stream.next();
        }
        stream
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        ((self.next() >> 11) % bound as u64) as usize
    }
}

fn tagged(seed: u64, a: u64, b: u64) -> Stream {
    Stream::new(
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ 0x1B1B_1B1B_1B1B_1B1B
            ^ (a + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (b + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    )
}

fn decision() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(5)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn field_from(code: u64) -> SemanticBitField<BITS> {
    let mut field = SemanticBitField::<BITS>::zeroed().expect("field");
    for bit in 0..BITS {
        field.set(bit, (code >> bit) & 1 == 1).expect("bit");
    }
    field
}

fn hypothesis_state(stream: &mut Stream, h: usize, m: usize, noise_bp: u64) -> (State, u64) {
    let truth = stream.next();
    let hypotheses: Vec<SemanticBitField<BITS>> = (0..h)
        .map(|_| {
            let mut code = truth;
            for bit in 0..BITS {
                if (stream.next() >> 20) % 10_000 < noise_bp {
                    code ^= 1_u64 << bit;
                }
            }
            field_from(code)
        })
        .collect();
    let refs: Vec<&SemanticBitField<BITS>> = hypotheses.iter().collect();
    let ambiguity = derive_ambiguity(
        &refs,
        AmbiguityPolicy {
            min_minority_support: m,
        },
    )
    .expect("ambiguity");

    let mut state = State::zeroed().expect("state");
    for bit in 0..BITS {
        let votes = hypotheses
            .iter()
            .filter(|field| field.get(bit).expect("bit"))
            .count();
        state.value.set(bit, votes * 2 > h).expect("value");
    }
    state.ambiguity = ambiguity;
    state.meaning_mask = field_from(stream.next());
    state.routing = field_from(stream.next());
    let weight = state.ambiguity.count_ones();
    (state, weight)
}

fn target_q12(code: u64) -> Vec<i16> {
    (0..BITS)
        .map(|bit| {
            if (code >> bit) & 1 == 1 {
                DELTA_KV_ONE
            } else {
                -DELTA_KV_ONE
            }
        })
        .collect()
}

fn strict_recall(values: &[i32], code: u64) -> bool {
    values.iter().enumerate().all(|(bit, value)| {
        value.unsigned_abs() > CLEAR_ABOVE && (*value > 0) == ((code >> bit) & 1 == 1)
    })
}

fn cosine(a: &[i16], b: &[i16]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let na: f64 = a.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    dot / (na * nb)
}

fn mode(arm: usize) -> DeltaKvAmbiguityProjection {
    match arm {
        0 => DeltaKvAmbiguityProjection::Bipolar,
        1 => DeltaKvAmbiguityProjection::ActiveOnly,
        _ => unreachable!("two arms"),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Cell {
    facts: u64,
    strict: u64,
    weight_sum: u64,
    dense: u64,
    cosine_sum: f64,
    cosine_pairs: u64,
}

type Cells = BTreeMap<(usize, usize, usize, usize), Cell>;

fn run_width<const K: usize>(cells: &mut Cells) {
    for (config_index, &(h, m)) in CONFIGS.iter().enumerate() {
        for (noise_index, &noise_bp) in NOISE_BP.iter().enumerate() {
            for seed in 0..SEEDS {
                let projector_seed = seed * 1000 + 17;
                let mut stream = tagged(
                    seed,
                    100 * config_index as u64 + noise_index as u64,
                    K as u64,
                );
                let mut facts: Vec<(State, u64, u64)> = (0..LOAD)
                    .map(|_| {
                        let (state, weight) = hypothesis_state(&mut stream, h, m, noise_bp);
                        (state, stream.next(), weight)
                    })
                    .collect();
                for index in (1..facts.len()).rev() {
                    let swap_with = stream.below(index + 1);
                    facts.swap(index, swap_with);
                }

                for arm in 0..2 {
                    let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
                        DeltaKvSemanticProjectionPolicy::default(),
                        projector_seed,
                        mode(arm),
                    )
                    .expect("projector");
                    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
                    let mut counters = DeltaKvCounters::default();

                    let mut keys = Vec::with_capacity(facts.len());
                    for (state, code, _) in &facts {
                        let evidence = projector
                            .project(state, &decision(), None)
                            .expect("project");
                        memory
                            .update(
                                &evidence.key_q12,
                                &target_q12(*code),
                                FULL_RATE,
                                &mut counters,
                            )
                            .expect("update");
                        keys.push(evidence.key_q12);
                    }

                    let cell = cells
                        .entry((K, arm, config_index, noise_index))
                        .or_default();
                    for ((_, code, weight), stored_key) in facts.iter().zip(&keys) {
                        let values = memory.read(stored_key, &mut counters).expect("read");
                        cell.facts += 1;
                        cell.strict += u64::from(strict_recall(&values, *code));
                        cell.weight_sum = cell.weight_sum.saturating_add(*weight);
                        cell.dense += u64::from(*weight > 16);
                    }
                    for pair in keys.windows(2) {
                        cell.cosine_sum += cosine(&pair[0], &pair[1]);
                        cell.cosine_pairs += 1;
                    }
                }
            }
        }
    }
}

fn run_grid() -> Cells {
    let mut cells = Cells::new();
    run_width::<256>(&mut cells);
    run_width::<1024>(&mut cells);
    cells
}

fn strict_ppm(cells: &Cells, k: usize, arm: usize, config: usize, noise: usize) -> u64 {
    let cell = cells[&(k, arm, config, noise)];
    cell.strict.saturating_mul(1_000_000) / cell.facts
}

fn mean_weight(cells: &Cells, k: usize, config: usize, noise: usize) -> f64 {
    let cell = cells[&(k, 0, config, noise)];
    cell.weight_sum as f64 / cell.facts as f64
}

fn report() -> String {
    let cells = run_grid();
    let mut out = String::new();

    for (config_index, &(h, m)) in CONFIGS.iter().enumerate() {
        for &k in &WIDTHS {
            for (noise_index, &noise_bp) in NOISE_BP.iter().enumerate() {
                for (arm, arm_name) in ARM_NAMES.iter().enumerate() {
                    let cell = cells[&(k, arm, config_index, noise_index)];
                    writeln!(
                        out,
                        "N3A,K={k},H={h},m={m},p_bp={noise_bp},arm={arm_name},facts={},mean_a={:.6},dense_ppm={},strict={},strict_ppm={},mean_cosine={:.6}",
                        cell.facts,
                        cell.weight_sum as f64 / cell.facts as f64,
                        cell.dense.saturating_mul(1_000_000) / cell.facts,
                        cell.strict,
                        cell.strict.saturating_mul(1_000_000) / cell.facts,
                        cell.cosine_sum / cell.cosine_pairs as f64,
                    )
                    .unwrap();
                }
            }
        }

        let mut max_noise = 0_u64;
        for (noise_index, &noise_bp) in NOISE_BP.iter().enumerate() {
            if mean_weight(&cells, 1024, config_index, noise_index) <= 16.0 {
                max_noise = noise_bp;
            }
        }
        writeln!(
            out,
            "N3A_BOUNDARY,H={h},m={m},max_p_bp_mean_a_le_16={max_noise}"
        )
        .unwrap();
    }

    let h1 = (0..CONFIGS.len()).all(|config| {
        (1..NOISE_BP.len()).all(|noise| {
            mean_weight(&cells, 1024, config, noise) >= mean_weight(&cells, 1024, config, noise - 1)
        })
    });

    let mut valid_cells = 0_u64;
    let mut h2 = true;
    let mut dense_cells = 0_u64;
    let mut h3 = true;
    for &k in &WIDTHS {
        for config in 0..CONFIGS.len() {
            for noise in 0..NOISE_BP.len() {
                let mean = mean_weight(&cells, k, config, noise);
                if mean <= 16.0 {
                    valid_cells += 1;
                    h2 &= strict_ppm(&cells, k, 1, config, noise).saturating_add(10_000)
                        >= strict_ppm(&cells, k, 0, config, noise);
                }
                if mean >= 40.0 {
                    dense_cells += 1;
                    h3 &= strict_ppm(&cells, k, 0, config, noise)
                        > strict_ppm(&cells, k, 1, config, noise).saturating_add(20_000);
                }
            }
        }
    }

    let h4 = (0..CONFIGS.len()).all(|config| {
        mean_weight(&cells, 1024, config, 0) <= 4.0
            && strict_ppm(&cells, 1024, 1, config, 0) >= 900_000
            && strict_ppm(&cells, 1024, 0, config, 0) <= 600_000
    });

    let h5 = (0..CONFIGS.len()).all(|config| {
        (0..NOISE_BP.len()).all(|noise| {
            mean_weight(&cells, 1024, config, noise) > 16.0
                || strict_ppm(&cells, 1024, 1, config, noise) >= 900_000
        })
    });

    for (name, holds) in [
        ("H1_MONOTONE", h1),
        ("H2_VALID_RANGE", valid_cells > 0 && h2),
        ("H3_DENSE_RANGE", dense_cells > 0 && h3),
        ("H4_LOW_NOISE", h4),
        ("H5_NO_CLIFF", h5),
    ] {
        writeln!(
            out,
            "N3A_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }
    writeln!(
        out,
        "N3A_COUNTS,valid_cells={valid_cells},dense_cells={dense_cells}"
    )
    .unwrap();
    writeln!(out, "PROMOTION,BLOCKED,n3a_noise_model_only").unwrap();
    out
}

fn main() {
    print!("{}", report());
}
