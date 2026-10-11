//! A6-C6 preregistered elastic 4..256 state-bit ANF capacity benchmark.
//!
//! Run: sml-elastic-capacity-bench [seeds:1..=5].
//!
//! Width families: 4,8,16,32,64,128,256 logical recurrent bits per
//! independent sequence; all are native dynamic-width LocalAnfNetwork gates.
//! Falsifiable tasks: direct output, stride-4/8/16/32/64/128/256 parity,
//! and an unrepresentable stride-257 negative control (max width=256).
//!
//! TWO panels: clean and independent Bernoulli(1/16) label noise.
//! FIVE independent seeds. Training/validation: 512 steps each, 64 lanes.
//! ALL 90 selections are FROZEN BEFORE any test stream is generated.
//! Held-out streams: 4096 steps and 16384 steps, separately seeded.
//!
//! The complete 16,384-candidate train sweep is shared between families.
//! Every family has one native ANF train+val oracle; on test, family scores
//! use the bit-exact circular scorer and the selected model is evaluated
//! with a streaming native LocalAnfNetwork runtime. Selected native loss is
//! verified identical to its circular scorer on each held-out stream.
//!
//! These rates are synthetic Boolean bitplane errors and bitlane throughput,
//! NOT next-token accuracy, tokenizer throughput, or SML-25B benchmarking.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_elastic_capacity::{
        fit_elastic_phase, native_errors, reference_errors, score_rule,
        ElasticPhaseReport, ElasticPhaseSpec, ELASTIC_REGISTERED_WIDTHS,
        ELASTIC_UNIQUE_CANDIDATES,
    },
    anf_local::LocalAnfNetwork,
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const RAW_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_STEPS: usize = 512;
const VALIDATION_STEPS: usize = 512;
const HORIZONS: [usize; 2] = [4096, 16384];
const FAMILY_COUNT: usize = ELASTIC_REGISTERED_WIDTHS.len();
const OUTPUTS: usize = FAMILY_COUNT + 2; // baseline + seven widths + chosen

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Direct,
    Stride4,
    Stride8,
    Stride16,
    Stride32,
    Stride64,
    Stride128,
    Stride256,
    Stride257Negative,
}
impl Task {
    const ALL: [Self; 9] = [
        Self::Direct, Self::Stride4, Self::Stride8, Self::Stride16,
        Self::Stride32, Self::Stride64, Self::Stride128,
        Self::Stride256, Self::Stride257Negative,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Stride4 => "stride4",
            Self::Stride8 => "stride8",
            Self::Stride16 => "stride16",
            Self::Stride32 => "stride32",
            Self::Stride64 => "stride64",
            Self::Stride128 => "stride128",
            Self::Stride256 => "stride256",
            Self::Stride257Negative => "stride257_unrepresentable",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Direct => 0, Self::Stride4 => 1, Self::Stride8 => 2,
            Self::Stride16 => 3, Self::Stride32 => 4, Self::Stride64 => 5,
            Self::Stride128 => 6, Self::Stride256 => 7,
            Self::Stride257Negative => 8,
        }
    }
    fn stride(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Stride4 => 4,
            Self::Stride8 => 8,
            Self::Stride16 => 16,
            Self::Stride32 => 32,
            Self::Stride64 => 64,
            Self::Stride128 => 128,
            Self::Stride256 => 256,
            Self::Stride257Negative => 257,
        }
    }
    fn expected_width(self) -> Option<usize> {
        (self.stride() <= 256).then_some(self.stride().max(4))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    LabelNoise,
}
impl Panel {
    const ALL: [Self; 2] = [Self::Clean, Self::LabelNoise];
    fn index(self) -> usize { match self { Self::Clean => 0, Self::LabelNoise => 1 } }
    fn name(self) -> &'static str {
        match self { Self::Clean => "clean", Self::LabelNoise => "noise_1_in_16" }
    }
}

fn spec() -> ElasticPhaseSpec {
    ElasticPhaseSpec {
        raw_candidates: vec![1, 2, 3, 4],
        baseline_input: 0,
        target_bit: 0,
    }
}
fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// All 64 lanes and all input channels are independent PRNG bit streams.
/// Labels are a known synthetic parity function, never shared with trainer.
fn dataset(seed: u64, steps: usize, task: Task, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * RAW_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let stride = task.stride();
    let mut phases = vec![0u64; stride.max(1)];
    let mut targets = vec![0u64; steps];
    for (t, label) in targets.iter_mut().enumerate() {
        let raw = inputs[t * RAW_BITS + 1];
        let residual = if stride == 0 {
            raw
        } else {
            phases[t % stride] ^= raw;
            phases[t % stride]
        };
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *label = inputs[t * RAW_BITS] ^ residual ^ noise;
    }
    LaneData {
        input_bits: RAW_BITS,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    }
}

struct Frozen {
    seed: usize,
    task: Task,
    panel: Panel,
    model: ElasticPhaseReport,
    training_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-elastic-capacity-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |s| s.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seeds must be between 1 and 5".into());
    }
    println!(
        "A6C6_CONFIG,seeds={seeds},tasks=direct;stride4;stride8;stride16;stride32;stride64;stride128;stride256;stride257_unrepresentable,panels=clean;noise_1_in_16,widths=4;8;16;32;64;128;256,train_steps={TRAIN_STEPS},val_steps={VALIDATION_STEPS},test_steps=4096;16384,lanes={LANES}"
    );
    println!(
        "A6C6_PROTOCOL,terminal_labels_only=true,identity_delay_scaffold_fixed=true,tap_raw_truth_train_learned=true,validation_min_width_selection=true,all_models_frozen_before_any_test_generation=true,full_256_bit_native_oracle=true"
    );
    println!(
        "A6C6_BUDGET,unique_training_hypotheses_per_case={ELASTIC_UNIQUE_CANDIDATES},feedback_taps=256,raw_choices=4,binary_truth_tables=16,expected_free_state_payload_256_bits_bytes=2048"
    );
    println!(
        "A6C6_MEM_HEADER,width,logical_state_words_64_lanes,logical_state_bytes_64_lanes"
    );
    for &width in &ELASTIC_REGISTERED_WIDTHS {
        println!("A6C6_MEM,{width},{width},{}", width * 8);
    }

    let mut frozen = Vec::with_capacity(Task::ALL.len() * Panel::ALL.len() * seeds);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..seeds {
                let train = dataset(
                    seed(index, task, panel, 0xa6c6_1001), TRAIN_STEPS, task, panel,
                );
                let val = dataset(
                    seed(index, task, panel, 0xa6c6_2002), VALIDATION_STEPS, task, panel,
                );
                let start = Instant::now();
                let model = fit_elastic_phase(
                    &train, &val, &spec(), &ELASTIC_REGISTERED_WIDTHS,
                )?;
                let training_seconds = start.elapsed().as_secs_f64();
                if model.unique_train_evaluations != ELASTIC_UNIQUE_CANDIDATES
                    || model.train_samples != (TRAIN_STEPS * LANES) as u64
                    || model.validation_samples != (VALIDATION_STEPS * LANES) as u64
                    || model.families.len() != FAMILY_COUNT
                {
                    return Err("A6-C6 preregistered train protocol violation".into());
                }
                let train_counts = model.families.iter()
                    .map(|w| w.train_errors.to_string()).collect::<Vec<_>>().join(";");
                let val_counts = model.families.iter()
                    .map(|w| w.validation_errors.to_string()).collect::<Vec<_>>().join(";");
                let selected_gate = model.selected.gates()[0].clone();
                println!(
                    "A6C6_FROZEN,task={},panel={},seed={},accepted={},selected_width={},selected_active_memory_bits={},expected_width={},width_match={},tap={},raw={},root_truth={},root_monomials={},train_errors={},val_errors={},baseline_train_errors={},baseline_val_errors={},train_errors_by_width={},val_errors_by_width={},evaluations={},search_ring_payload_bytes={},selected_gate_inputs={:?},selected_anf_coefficient_bits={},selected_bits_read_per_step={},fit_seconds={:.6}",
                    task.name(), panel.name(), index, model.accepted,
                    model.selected_width, model.selected_active_memory_bits,
                    task.expected_width().map_or("unsupported".to_string(), |v| v.to_string()),
                    task.expected_width() == Some(model.selected_width),
                    model.selected_rule.tap, model.selected_rule.raw,
                    model.selected_rule.truth, model.selected_rule.root_monomials(),
                    model.selected_train_errors, model.selected_validation_errors,
                    model.baseline_train_errors, model.baseline_validation_errors,
                    train_counts, val_counts, model.unique_train_evaluations,
                    model.search_ring_payload_bytes, selected_gate.inputs(),
                    model.selected.learned_bits(), model.selected.bits_read_per_token(),
                    training_seconds
                );
                frozen.push(Frozen {
                    seed: index,
                    task,
                    panel,
                    model,
                    training_seconds,
                });
            }
        }
    }

    // No test data is available to any fit/validation call above.
    println!("A6C6_ALL_FROZEN,true");
    println!(
        "A6C6_TEST_HEADER,task,panel,seed,steps,observations,baseline_errors,width4_errors,width8_errors,width16_errors,width32_errors,width64_errors,width128_errors,width256_errors,selected_native_errors,selected_width,selected_active_memory_bits,native_seconds,native_bitlane_throughput_per_sec"
    );

    let mut totals = [[[[0u64; OUTPUTS]; 2]; 2]; 9];
    let mut hits = [[0usize; 2]; 9];
    let mut accepts = [[0usize; 2]; 9];
    let mut durations = [[0f64; 2]; 9];
    let mut native_durations = [[[0f64; 2]; 2]; 9];

    for case in &frozen {
        let ti = case.task.index();
        let pi = case.panel.index();
        hits[ti][pi] += usize::from(
            case.task.expected_width() == Some(case.model.selected_width)
        );
        accepts[ti][pi] += usize::from(case.model.accepted);
        durations[ti][pi] += case.training_seconds;

        for (hi, steps) in HORIZONS.iter().copied().enumerate() {
            let test = dataset(
                seed(case.seed, case.task, case.panel, 0xa6c6_3003 + steps as u64),
                steps, case.task, case.panel,
            );

            // Families use bit-exact circular inference; the selected
            // *independent* native streaming scorer is always run as oracle.
            let mut ring = vec![0u64; ELASTIC_REGISTERED_WIDTHS[FAMILY_COUNT - 1]];
            let mut measured = [0u64; OUTPUTS];
            measured[0] = reference_errors(&test, &spec());
            for (j, family) in case.model.families.iter().enumerate() {
                measured[j + 1] = score_rule(family.rule, &test, &spec(), &mut ring)?;
            }
            let start = Instant::now();
            measured[OUTPUTS - 1] = native_errors(&case.model.selected, &test, &spec())?;
            let native_seconds = start.elapsed().as_secs_f64().max(f64::EPSILON);
            let expected_native = if case.model.accepted {
                score_rule(case.model.selected_rule, &test, &spec(), &mut ring)?
            } else {
                measured[0]
            };
            if expected_native != measured[OUTPUTS - 1] {
                return Err("A6-C6 native-long-horizon oracle disagreement".into());
            }
            for (total, value) in totals[ti][pi][hi].iter_mut().zip(measured) {
                *total += value;
            }
            native_durations[ti][pi][hi] += native_seconds;
            let throughput = (steps * LANES) as f64 / native_seconds;
            println!(
                "A6C6_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.2}",
                case.task.name(), case.panel.name(), case.seed, steps,
                steps * LANES,
                measured[0], measured[1], measured[2], measured[3],
                measured[4], measured[5], measured[6], measured[7],
                measured[8], case.model.selected_width,
                case.model.selected_active_memory_bits, native_seconds, throughput
            );
        }
    }

    for task in Task::ALL {
        for panel in Panel::ALL {
            let ti = task.index();
            let pi = panel.index();
            for (hi, steps) in HORIZONS.iter().copied().enumerate() {
                let sum = totals[ti][pi][hi];
                let total = (seeds * steps * LANES) as f64;
                let rate = |x: u64| x as f64 / total;
                let native_bps = total / native_durations[ti][pi][hi];
                println!(
                    "A6C6_SUMMARY,task={},panel={},steps={},seeds={},baseline_error_rate={:.8},width4_error_rate={:.8},width8_error_rate={:.8},width16_error_rate={:.8},width32_error_rate={:.8},width64_error_rate={:.8},width128_error_rate={:.8},width256_error_rate={:.8},adaptive_native_error_rate={:.8},selected_width_hits={},model_accepts={},mean_fit_seconds={:.6},native_aggregate_bitlane_per_second={:.2}",
                    task.name(), panel.name(), steps, seeds,
                    rate(sum[0]), rate(sum[1]), rate(sum[2]), rate(sum[3]),
                    rate(sum[4]), rate(sum[5]), rate(sum[6]), rate(sum[7]),
                    rate(sum[8]), hits[ti][pi], accepts[ti][pi],
                    durations[ti][pi] / seeds as f64, native_bps
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_stride_256_label_is_exact_per_phase() {
        let data = dataset(19, 600, Task::Stride256, Panel::Clean);
        let block = &data.blocks[0];
        let mut phases = [0u64; 256];
        for t in 0..block.steps {
            let x1 = block.inputs[t * RAW_BITS + 1];
            phases[t % 256] ^= x1;
            assert_eq!(
                block.targets[t] ^ block.inputs[t * RAW_BITS],
                phases[t % 256],
            );
        }
    }

    #[test]
    fn generated_noisy_panel_preserves_identical_raw_inputs() {
        let clean = dataset(55, 512, Task::Stride128, Panel::Clean);
        let noisy = dataset(55, 512, Task::Stride128, Panel::LabelNoise);
        assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
        let flips = clean.blocks[0].targets.iter()
            .zip(&noisy.blocks[0].targets)
            .map(|(a, b)| (a ^ b).count_ones() as u64)
            .sum::<u64>();
        assert!(flips > 1000 && flips < 3200);
    }

    #[test]
    fn all_registered_widths_and_negative_control_are_present() {
        assert_eq!(ELASTIC_REGISTERED_WIDTHS, [4, 8, 16, 32, 64, 128, 256]);
        assert_eq!(ELASTIC_UNIQUE_CANDIDATES, 16_384);
        assert_eq!(Task::ALL.len(), 9);
        assert_eq!(Task::Stride257Negative.expected_width(), None);
        assert_eq!(OUTPUTS, 9);
    }
}
